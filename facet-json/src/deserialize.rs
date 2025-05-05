use alloc::string::{String, ToString};
use alloc::{vec, vec::Vec};
use facet_core::{Characteristic, Def, Facet, FieldFlags, ScalarAffinity, StructKind};
use facet_reflect::{HeapValue, ReflectError, Wip};
use log::trace;
use owo_colors::OwoColorize;

mod tokenizer;
pub use tokenizer::*;

mod error;
pub use error::*;

/// Deserializes a JSON string into a value of type `T` that implements `Facet`.
///
/// This function takes a JSON string representation and converts it into a Rust
/// value of the specified type `T`. The type must implement the `Facet` trait
/// to provide the necessary type information for deserialization.
pub fn from_str<'input, 'facet, T>(json: &'input str) -> Result<T, JsonError<'input>>
where
    T: Facet<'facet>,
    'input: 'facet,
{
    from_slice(json.as_bytes())
}

/// Deserialize JSON from a slice
///
/// # Arguments
///
/// * `json` - A slice of bytes representing the JSON input.
///
/// # Returns
///
/// A result containing the deserialized value of type `T` or a `JsonParseErrorWithContext`.
pub fn from_slice<'input, 'facet, T>(json: &'input [u8]) -> Result<T, JsonError<'input>>
where
    T: Facet<'facet>,
    'input: 'facet,
{
    let wip =
        Wip::alloc::<T>().map_err(|e| JsonError::new_reflect(e, json, Span::new(0, json.len())))?;
    let heap_value = from_slice_wip(wip, json)?;
    Ok(heap_value.materialize::<T>().unwrap())
}

/// Represents the next expected token or structure while parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Instruction {
    Value,
    SkipValue,
    Pop(PopReason),
    ObjectKeyOrObjectClose,
    CommaThenObjectKeyOrObjectClose,
    ArrayItemOrArrayClose,
    CommaThenArrayItemOrArrayClose,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PopReason {
    TopLevel,
    ObjectVal,
    ArrayItem,
    Some,
}

/// Deserialize a JSON string into a Wip object.
///
/// # Arguments
///
/// * `wip` - A mutable Wip object to deserialize into.
/// * `input` - A byte slice representing the JSON input.
///
/// # Returns
///
/// A result containing the updated `Wip` or a `JsonParseErrorWithContext`.
pub fn from_slice_wip<'input: 'facet, 'facet>(
    mut wip: Wip<'facet>,
    input: &'input [u8],
) -> Result<HeapValue<'facet>, JsonError<'input>> {
    // This struct is just a bundle of the state that we need to pass around all the time.
    let mut runner = StackRunner {
        stack: vec![Instruction::Pop(PopReason::TopLevel), Instruction::Value],
        tokenizer: Tokenizer::new(input),
        last_span: Span::new(0, 0),
        unread_token: None,
        input,
    };

    loop {
        let frame_count = wip.frames_count();
        debug_assert!(
            frame_count
                >= runner
                    .stack
                    .iter()
                    .filter(|f| matches!(f, Instruction::Pop(_)))
                    .count()
        );

        let insn = match runner.stack.pop() {
            Some(insn) => insn,
            None => unreachable!("Instruction stack is empty"),
        };

        trace!("[{frame_count}] Instruction {:?}", insn.yellow());

        match insn {
            Instruction::Pop(reason) => {
                wip = runner.pop(wip, reason)?;

                if reason == PopReason::TopLevel {
                    return wip
                        .build()
                        .map_err(|e| JsonError::new_reflect(e, input, runner.last_span));
                } else {
                    wip = wip
                        .pop()
                        .map_err(|e| JsonError::new_reflect(e, input, runner.last_span))?;
                }
            }
            Instruction::SkipValue => runner.skip_value(&wip)?,
            Instruction::Value => wip = runner.value(wip)?,
            Instruction::ObjectKeyOrObjectClose => wip = runner.object_key_or_object_close(wip)?,
            Instruction::CommaThenObjectKeyOrObjectClose => {
                runner.comma_then_object_key_or_object_close(&wip)?
            }
            Instruction::ArrayItemOrArrayClose => wip = runner.array_item_or_array_close(wip)?,
            Instruction::CommaThenArrayItemOrArrayClose => {
                wip = runner.comma_then_array_item_or_array_close(wip)?
            }
        }
    }
}

/// It runs along the stack!
struct StackRunner<'a> {
    /// Look! A stack!
    stack: Vec<Instruction>,
    tokenizer: Tokenizer<'a>,
    last_span: Span,
    unread_token: Option<Spanned<Token>>,
    input: &'a [u8],
}

impl<'a> StackRunner<'a> {
    fn pop<'f>(&mut self, mut wip: Wip<'f>, reason: PopReason) -> Result<Wip<'f>, JsonError<'a>> {
        trace!("Popping because {:?}", reason.yellow());

        let container_shape = wip.shape();
        match container_shape.def {
            Def::Struct(sd) => {
                let mut has_unset = false;

                trace!("Let's check all fields are initialized");
                for (index, field) in sd.fields.iter().enumerate() {
                    let is_set = wip.is_field_set(index).map_err(|err| {
                        trace!("Error checking field set status: {:?}", err);
                        JsonError::new_reflect(err, self.input, self.last_span)
                    })?;
                    if !is_set {
                        if field.flags.contains(FieldFlags::DEFAULT) {
                            wip = wip.field(index).map_err(|e| {
                                JsonError::new_reflect(e, self.input, self.last_span)
                            })?;
                            if let Some(default_in_place_fn) = field.vtable.default_fn {
                                wip = wip.put_from_fn(default_in_place_fn).map_err(|e| {
                                    JsonError::new_reflect(e, self.input, self.last_span)
                                })?;
                                trace!(
                                    "Field #{} {:?} was set to default value (via custom fn)",
                                    index.yellow(),
                                    field.blue()
                                );
                            } else {
                                if !field.shape().is(Characteristic::Default) {
                                    return Err(JsonError::new_reflect(
                                        ReflectError::DefaultAttrButNoDefaultImpl {
                                            shape: field.shape(),
                                        },
                                        self.input,
                                        self.last_span,
                                    ));
                                }
                                wip = wip.put_default().map_err(|e| {
                                    JsonError::new_reflect(e, self.input, self.last_span)
                                })?;
                                trace!(
                                    "Field #{} {:?} was set to default value (via default impl)",
                                    index.yellow(),
                                    field.blue()
                                );
                            }
                            wip = wip.pop().map_err(|e| {
                                JsonError::new_reflect(e, self.input, self.last_span)
                            })?;
                        } else {
                            trace!(
                                "Field #{} {:?} is not initialized",
                                index.yellow(),
                                field.blue()
                            );
                            has_unset = true;
                        }
                    }
                }

                if has_unset && container_shape.has_default_attr() {
                    // let's allocate and build a default value
                    let default_val = Wip::alloc_shape(container_shape)
                        .map_err(|e| JsonError::new_reflect(e, self.input, self.last_span))?
                        .put_default()
                        .map_err(|e| JsonError::new_reflect(e, self.input, self.last_span))?
                        .build()
                        .map_err(|e| JsonError::new_reflect(e, self.input, self.last_span))?;
                    let peek = default_val.peek().into_struct().unwrap();

                    for (index, field) in sd.fields.iter().enumerate() {
                        let is_set = wip.is_field_set(index).map_err(|err| {
                            trace!("Error checking field set status: {:?}", err);
                            JsonError::new_reflect(err, self.input, self.last_span)
                        })?;
                        if !is_set {
                            let address_of_field_from_default = peek.field(index).unwrap().data();
                            wip = wip.field(index).map_err(|e| {
                                JsonError::new_reflect(e, self.input, self.last_span)
                            })?;
                            wip = wip
                                .put_shape(address_of_field_from_default, field.shape())
                                .map_err(|e| {
                                    JsonError::new_reflect(e, self.input, self.last_span)
                                })?;
                            wip = wip.pop().map_err(|e| {
                                JsonError::new_reflect(e, self.input, self.last_span)
                            })?;
                        }
                    }
                }
            }
            Def::Enum(_) => {
                trace!(
                    "TODO: make sure enums are initialized (support container-level and field-level default, etc.)"
                );
            }
            _ => {
                trace!("Thing being popped is not a container I guess");
            }
        }
        Ok(wip)
    }

    fn skip_value(&mut self, wip: &Wip<'_>) -> Result<(), JsonError<'a>> {
        let token = self.read_token(wip)?;
        match token.node {
            Token::LBrace | Token::LBracket => {
                // Skip a compound value by tracking nesting depth
                let mut depth = 1;
                while depth > 0 {
                    let token = self.read_token(wip)?;
                    match token.node {
                        Token::LBrace | Token::LBracket => {
                            depth += 1;
                        }
                        Token::RBrace | Token::RBracket => {
                            depth -= 1;
                        }
                        _ => {
                            // primitives, commas, colons, strings, numbers, etc.
                        }
                    }
                }
                Ok(())
            }
            Token::String(_)
            | Token::F64(_)
            | Token::I64(_)
            | Token::U64(_)
            | Token::True
            | Token::False
            | Token::Null => {
                // Primitive value; nothing more to skip
                Ok(())
            }
            other => {
                // Unexpected token when skipping a value
                Err(JsonError::new(
                    JsonErrorKind::UnexpectedToken {
                        got: other,
                        wanted: "value",
                    },
                    self.input,
                    self.last_span,
                ))
            }
        }
    }

    fn value<'facet>(&mut self, mut wip: Wip<'facet>) -> Result<Wip<'facet>, JsonError<'a>> {
        let token = self.read_token(&wip)?;
        match token.node {
            Token::Null => wip
                .put_default()
                .map_err(|e| JsonError::new_reflect(e, self.input, self.last_span)),
            _ => {
                if matches!(wip.shape().def, Def::Option(_)) {
                    trace!("Starting Some(_) option for {}", wip.shape().blue());
                    wip = wip
                        .push_some()
                        .map_err(|e| JsonError::new_reflect(e, self.input, self.last_span))?;
                    self.stack.push(Instruction::Pop(PopReason::Some))
                }

                match token.node {
                    Token::Null => unreachable!(),
                    Token::LBrace => {
                        match wip.innermost_shape().def {
                            Def::Map(_md) => {
                                trace!("Object starting for map value ({})!", wip.shape().blue());
                                wip = wip.put_default().map_err(|e| {
                                    JsonError::new_reflect(e, self.input, self.last_span)
                                })?;
                            }
                            Def::Enum(_ed) => {
                                trace!("Object starting for enum value ({})!", wip.shape().blue());
                                // nothing to do here
                            }
                            Def::Struct(_) => {
                                trace!(
                                    "Object starting for struct value ({})!",
                                    wip.shape().blue()
                                );
                                // nothing to do here
                            }
                            _ => {
                                return Err(JsonError::new(
                                    JsonErrorKind::UnsupportedType {
                                        got: wip.innermost_shape(),
                                        wanted: "map, enum, or struct",
                                    },
                                    self.input,
                                    self.last_span,
                                ));
                            }
                        }

                        self.stack.push(Instruction::ObjectKeyOrObjectClose);
                        Ok(wip)
                    }
                    Token::LBracket => {
                        match wip.innermost_shape().def {
                            Def::Array(_) => {
                                trace!("Array starting for array ({})!", wip.shape().blue());
                            }
                            Def::Slice(_) => {
                                trace!("Array starting for slice ({})!", wip.shape().blue());
                            }
                            Def::List(_) => {
                                trace!("Array starting for list ({})!", wip.shape().blue());
                                wip = wip.put_default().map_err(|e| {
                                    JsonError::new_reflect(e, self.input, self.last_span)
                                })?;
                            }
                            Def::Enum(_) => {
                                trace!("Array starting for enum ({})!", wip.shape().blue());
                            }
                            Def::Struct(s) => {
                                if s.kind == StructKind::Tuple {
                                    trace!("Array starting for tuple ({})!", wip.shape().blue());
                                    wip = wip.put_default().map_err(|e| {
                                        JsonError::new_reflect(e, self.input, self.last_span)
                                    })?;
                                } else {
                                    return Err(JsonError::new(
                                        JsonErrorKind::UnsupportedType {
                                            got: wip.shape(),
                                            wanted: "array, list, tuple, or slice",
                                        },
                                        self.input,
                                        self.last_span,
                                    ));
                                }
                            }
                            Def::Scalar(s) if matches!(s.affinity, ScalarAffinity::Empty(_)) => {
                                trace!("Array starting for unit type ({})!", wip.shape().blue());

                                // Check if the array is empty by peeking at the next token
                                let next_token = self.read_token(&wip)?;
                                if next_token.node == Token::RBracket {
                                    // Empty array means unit type () - we're good
                                    wip = wip.put_default().map_err(|e| {
                                        JsonError::new_reflect(e, self.input, self.last_span)
                                    })?;
                                    return Ok(wip); // Return immediately - no need to push anything
                                } else {
                                    // Non-empty array is not valid for unit type
                                    return Err(JsonError::new(
                                        JsonErrorKind::UnsupportedType {
                                            got: wip.innermost_shape(),
                                            wanted: "empty array",
                                        },
                                        self.input,
                                        self.last_span,
                                    ));
                                }
                            }
                            _ => {
                                return Err(JsonError::new(
                                    JsonErrorKind::UnsupportedType {
                                        got: wip.innermost_shape(),
                                        wanted: "array, list, tuple, or slice",
                                    },
                                    self.input,
                                    self.last_span,
                                ));
                            }
                        }

                        trace!("Beginning pushback");
                        self.stack.push(Instruction::ArrayItemOrArrayClose);
                        wip.begin_pushback()
                            .map_err(|e| JsonError::new_reflect(e, self.input, self.last_span))
                    }
                    Token::RBrace | Token::RBracket | Token::Colon | Token::Comma => {
                        Err(JsonError::new(
                            JsonErrorKind::UnexpectedToken {
                                got: token.node,
                                wanted: "value",
                            },
                            self.input,
                            self.last_span,
                        ))
                    }
                    Token::String(s) => match wip.innermost_shape().def {
                        Def::Scalar(_sd) => wip
                            .put::<String>(s)
                            .map_err(|e| JsonError::new_reflect(e, self.input, self.last_span)),
                        Def::Enum(_ed) => {
                            if wip.selected_variant().is_some() {
                                trace!("Have variant selected already, just putting");

                                // just put, then — if it's a tuple field it'll work
                                wip.put::<String>(s).map_err(|e| {
                                    JsonError::new_reflect(e, self.input, self.last_span)
                                })
                            } else {
                                match wip.find_variant(&s) {
                                    Some((variant_index, _)) => {
                                        wip.variant(variant_index).map_err(|e| {
                                            JsonError::new_reflect(e, self.input, self.last_span)
                                        })
                                    }
                                    None => Err(JsonError::new(
                                        JsonErrorKind::NoSuchVariant {
                                            name: s.to_string(),
                                            enum_shape: wip.shape(),
                                        },
                                        self.input,
                                        self.last_span,
                                    )),
                                }
                            }
                        }
                        _ => Err(JsonError::new(
                            JsonErrorKind::UnsupportedType {
                                got: wip.innermost_shape(),
                                wanted: "enum or string",
                            },
                            self.input,
                            self.last_span,
                        )),
                    },
                    Token::F64(n) => {
                        if wip.innermost_shape() == <f32 as Facet>::SHAPE {
                            wip.put(n as f32)
                                .map_err(|e| JsonError::new_reflect(e, self.input, self.last_span))
                        } else {
                            wip.put(n)
                                .map_err(|e| JsonError::new_reflect(e, self.input, self.last_span))
                        }
                    }
                    Token::U64(n) => wip
                        .put(n)
                        .map_err(|e| JsonError::new_reflect(e, self.input, self.last_span)),
                    Token::I64(n) => wip
                        .put(n)
                        .map_err(|e| JsonError::new_reflect(e, self.input, self.last_span)),
                    Token::True => wip
                        .put::<bool>(true)
                        .map_err(|e| JsonError::new_reflect(e, self.input, self.last_span)),
                    Token::False => wip
                        .put::<bool>(false)
                        .map_err(|e| JsonError::new_reflect(e, self.input, self.last_span)),
                    Token::EOF => Err(JsonError::new(
                        JsonErrorKind::UnexpectedEof("in value"),
                        self.input,
                        self.last_span,
                    )),
                }
            }
        }
    }

    fn object_key_or_object_close<'f>(
        &mut self,
        mut wip: Wip<'f>,
    ) -> Result<Wip<'f>, JsonError<'a>> {
        let token = self.read_token(&wip)?;
        match token.node {
            Token::String(key) => {
                trace!("Parsed object key: {}", key);

                let mut ignore = false;
                let mut needs_pop = true;
                let mut handled_by_flatten = false;

                match wip.shape().def {
                    Def::Struct(sd) => {
                        // First try to find a direct field match
                        if let Some(index) = wip.field_index(&key) {
                            trace!("It's a struct field");
                            wip = wip.field(index).map_err(|e| {
                                JsonError::new_reflect(e, self.input, self.last_span)
                            })?;
                        } else {
                            // Check for flattened fields
                            let mut found_in_flatten = false;
                            for (index, field) in sd.fields.iter().enumerate() {
                                if field.flags.contains(FieldFlags::FLATTEN) {
                                    trace!("Found flattened field #{}", index);
                                    // Enter the flattened field
                                    wip = wip.field(index).map_err(|e| {
                                        JsonError::new_reflect(e, self.input, self.last_span)
                                    })?;

                                    // Check if this flattened field has the requested key
                                    if let Some(subfield_index) = wip.field_index(&key) {
                                        trace!("Found key {} in flattened field", key);
                                        wip = wip.field(subfield_index).map_err(|e| {
                                            JsonError::new_reflect(e, self.input, self.last_span)
                                        })?;
                                        found_in_flatten = true;
                                        handled_by_flatten = true;
                                        break;
                                    } else if let Some((_variant_index, _variant)) =
                                        wip.find_variant(&key)
                                    {
                                        trace!("Found key {} in flattened field", key);
                                        wip = wip.variant_named(&key).map_err(|e| {
                                            JsonError::new_reflect(e, self.input, self.last_span)
                                        })?;
                                        found_in_flatten = true;
                                        break;
                                    } else {
                                        // Key not in this flattened field, go back up
                                        wip = wip.pop().map_err(|e| {
                                            JsonError::new_reflect(e, self.input, self.last_span)
                                        })?;
                                    }
                                }
                            }

                            if !found_in_flatten {
                                if wip.shape().has_deny_unknown_fields_attr() {
                                    trace!(
                                        "It's not a struct field AND we're denying unknown fields"
                                    );
                                    return Err(JsonError::new(
                                        JsonErrorKind::UnknownField {
                                            field_name: key.to_string(),
                                            shape: wip.shape(),
                                        },
                                        self.input,
                                        self.last_span,
                                    ));
                                } else {
                                    trace!(
                                        "It's not a struct field and we're ignoring unknown fields"
                                    );
                                    ignore = true;
                                }
                            }
                        }
                    }
                    Def::Enum(_ed) => match wip.find_variant(&key) {
                        Some((index, variant)) => {
                            trace!("Variant {} selected", variant.name.blue());
                            wip = wip.variant(index).map_err(|e| {
                                JsonError::new_reflect(e, self.input, self.last_span)
                            })?;
                            needs_pop = false;
                        }
                        None => {
                            if let Some(_variant_index) = wip.selected_variant() {
                                trace!(
                                    "Already have a variant selected, treating key as struct field of variant"
                                );
                                // Try to find the field index of the key within the selected variant
                                if let Some(index) = wip.field_index(&key) {
                                    trace!("Found field {} in selected variant", key.blue());
                                    wip = wip.field(index).map_err(|e| {
                                        JsonError::new_reflect(e, self.input, self.last_span)
                                    })?;
                                } else if wip.shape().has_deny_unknown_fields_attr() {
                                    trace!("Unknown field in variant and denying unknown fields");
                                    return Err(JsonError::new(
                                        JsonErrorKind::UnknownField {
                                            field_name: key.to_string(),
                                            shape: wip.shape(),
                                        },
                                        self.input,
                                        self.last_span,
                                    ));
                                } else {
                                    trace!("Ignoring unknown field in variant");
                                    ignore = true;
                                }
                            } else {
                                return Err(JsonError::new(
                                    JsonErrorKind::NoSuchVariant {
                                        name: key.to_string(),
                                        enum_shape: wip.shape(),
                                    },
                                    self.input,
                                    self.last_span,
                                ));
                            }
                        }
                    },
                    Def::Map(_) => {
                        wip = wip
                            .push_map_key()
                            .map_err(|e| JsonError::new_reflect(e, self.input, self.last_span))?;
                        wip = wip
                            .put(key)
                            .map_err(|e| JsonError::new_reflect(e, self.input, self.last_span))?;
                        wip = wip
                            .push_map_value()
                            .map_err(|e| JsonError::new_reflect(e, self.input, self.last_span))?;
                    }
                    _ => {
                        return Err(JsonError::new(
                            JsonErrorKind::Unimplemented("object key for non-struct/map"),
                            self.input,
                            self.last_span,
                        ));
                    }
                }

                let colon = self.read_token(&wip)?;
                if colon.node != Token::Colon {
                    return Err(JsonError::new(
                        JsonErrorKind::UnexpectedToken {
                            got: colon.node,
                            wanted: "colon",
                        },
                        self.input,
                        self.last_span,
                    ));
                }
                self.stack
                    .push(Instruction::CommaThenObjectKeyOrObjectClose);
                if ignore {
                    self.stack.push(Instruction::SkipValue);
                } else {
                    if needs_pop && !handled_by_flatten {
                        trace!("Pushing Pop insn to stack (ObjectVal)");
                        self.stack.push(Instruction::Pop(PopReason::ObjectVal));
                    } else if handled_by_flatten {
                        // We need two pops for flattened fields - one for the field itself,
                        // one for the containing struct
                        trace!("Pushing Pop insn to stack (ObjectVal) for flattened field");
                        self.stack.push(Instruction::Pop(PopReason::ObjectVal));
                        self.stack.push(Instruction::Pop(PopReason::ObjectVal));
                    }
                    self.stack.push(Instruction::Value);
                }
                Ok(wip)
            }
            Token::RBrace => {
                trace!("Object closing");
                Ok(wip)
            }
            _ => Err(JsonError::new(
                JsonErrorKind::UnexpectedToken {
                    got: token.node,
                    wanted: "object key or closing brace",
                },
                self.input,
                self.last_span,
            )),
        }
    }

    fn comma_then_object_key_or_object_close(
        &mut self,
        wip: &Wip<'_>,
    ) -> Result<(), JsonError<'a>> {
        let token = self.read_token(wip)?;
        match token.node {
            Token::Comma => {
                trace!("Object comma");
                self.stack.push(Instruction::ObjectKeyOrObjectClose);
                Ok(())
            }
            Token::RBrace => {
                trace!("Object close");
                Ok(())
            }
            _ => Err(JsonError::new(
                JsonErrorKind::UnexpectedToken {
                    got: token.node,
                    wanted: "comma",
                },
                self.input,
                self.last_span,
            )),
        }
    }

    fn array_item_or_array_close<'facet>(
        &mut self,
        mut wip: Wip<'facet>,
    ) -> Result<Wip<'facet>, JsonError<'a>> {
        let token = self.read_token(&wip)?;
        match token.node {
            Token::RBracket => {
                trace!("Array close");
                Ok(wip)
            }
            _ => {
                trace!("Array item");
                assert!(
                    self.unread_token.is_none(),
                    "Cannot put back more than one token at a time"
                );
                self.unread_token = Some(token);
                wip = wip
                    .begin_pushback()
                    .map_err(|e| JsonError::new_reflect(e, self.input, self.last_span))?;
                wip = wip
                    .push()
                    .map_err(|e| JsonError::new_reflect(e, self.input, self.last_span))?;

                self.stack.push(Instruction::CommaThenArrayItemOrArrayClose);
                trace!("Pushing Pop insn to stack (arrayitem)");
                self.stack.push(Instruction::Pop(PopReason::ArrayItem));
                self.stack.push(Instruction::Value);
                Ok(wip)
            }
        }
    }

    fn comma_then_array_item_or_array_close<'facet>(
        &mut self,
        mut wip: Wip<'facet>,
    ) -> Result<Wip<'facet>, JsonError<'a>> {
        let token = self.read_token(&wip)?;
        match token.node {
            Token::RBracket => {
                trace!("Array close");
                Ok(wip)
            }
            Token::Comma => {
                trace!("Array comma");
                wip = wip
                    .push()
                    .map_err(|e| JsonError::new_reflect(e, self.input, self.last_span))?;
                self.stack.push(Instruction::CommaThenArrayItemOrArrayClose);
                trace!("Pushing Pop insn to stack (arrayitem)");
                self.stack.push(Instruction::Pop(PopReason::ArrayItem));
                self.stack.push(Instruction::Value);
                Ok(wip)
            }
            _ => Err(JsonError::new(
                JsonErrorKind::UnexpectedToken {
                    got: token.node,
                    wanted: "comma or closing bracket",
                },
                self.input,
                self.last_span,
            )),
        }
    }

    fn read_token(&mut self, _wip: &Wip<'_>) -> Result<Spanned<Token>, JsonError<'a>> {
        if let Some(token) = self.unread_token.take() {
            self.last_span = token.span;
            Ok(token)
        } else {
            match self.tokenizer.next_token() {
                Ok(token) => {
                    self.last_span = token.span;
                    Ok(token)
                }
                Err(e) => {
                    self.last_span = e.span;
                    Err(JsonError::new_syntax(e.kind, self.input, self.last_span))
                }
            }
        }
    }
}
