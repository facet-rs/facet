use alloc::string::{String, ToString};
use alloc::vec;
use facet_core::{Characteristic, Def, Facet, ScalarAffinity, StructKind};
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
    let wip = Wip::alloc::<T>().map_err(|e| {
        JsonError::new(
            JsonErrorKind::ReflectError(e),
            json,
            Span::new(0, json.len()),
            "$".to_string(),
        )
    })?;
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
    let mut stack = vec![Instruction::Pop(PopReason::TopLevel), Instruction::Value];
    let mut tokenizer = Tokenizer::new(input);
    let mut last_span = Span::new(0, 0);
    let mut unread_token: Option<Spanned<Token>> = None;

    macro_rules! bail {
        ($kind:expr) => {
            return Err(JsonError::new($kind, input, last_span, wip.path()))
        };
    }

    macro_rules! read_token {
        () => {
            if let Some(token) = unread_token.take() {
                last_span = token.span;
                token
            } else {
                match tokenizer.next_token() {
                    Ok(token) => {
                        last_span = token.span;
                        token
                    }
                    Err(e) => {
                        last_span = e.span;
                        bail!(JsonErrorKind::SyntaxError(e.kind));
                    }
                }
            }
        };
    }

    macro_rules! put_back_token {
        ($token:expr) => {
            assert!(
                unread_token.is_none(),
                "Cannot put back more than one token at a time"
            );
            unread_token = Some($token);
        };
    }

    macro_rules! reflect {
        ($($tt:tt)*) => {
            let path = wip.path();
            wip = match wip.$($tt)* {
                Ok(wip) => wip,
                Err(e) => {
                    return Err(JsonError::new(
                        JsonErrorKind::ReflectError(e),
                        input,
                        last_span,
                        path,
                    ));
                }
            }
        };
    }

    loop {
        let frame_count = wip.frames_count();
        let insn = match stack.pop() {
            Some(insn) => insn,
            None => unreachable!("Instruction stack is empty"),
        };
        trace!("[{frame_count}] Instruction {:?}", insn.yellow());

        match insn {
            Instruction::Pop(reason) => {
                trace!("Popping because {:?}", reason.yellow());

                let container_shape = wip.shape();
                match container_shape.def {
                    Def::Struct(sd) => {
                        let mut has_unset = false;

                        trace!("Let's check all fields are initialized");
                        for (index, field) in sd.fields.iter().enumerate() {
                            let is_set = wip.is_field_set(index).map_err(|err| {
                                trace!("Error checking field set status: {:?}", err);
                                JsonError::new(
                                    JsonErrorKind::ReflectError(err),
                                    input,
                                    last_span,
                                    wip.path(),
                                )
                            })?;
                            if !is_set {
                                if let Some(default_in_place_fn) = field.maybe_default_fn() {
                                    reflect!(field(index));
                                    if let Some(default_in_place_fn) = default_in_place_fn {
                                        reflect!(put_from_fn(default_in_place_fn));
                                        trace!(
                                            "Field #{} {:?} was set to default value (via custom fn)",
                                            index.yellow(),
                                            field.blue()
                                        );
                                    } else {
                                        if !field.shape().is(Characteristic::Default) {
                                            return Err(JsonError::new(
                                                JsonErrorKind::ReflectError(
                                                    ReflectError::DefaultAttrButNoDefaultImpl {
                                                        shape: field.shape(),
                                                    },
                                                ),
                                                input,
                                                last_span,
                                                wip.path(),
                                            ));
                                        }
                                        reflect!(put_default());
                                        trace!(
                                            "Field #{} {:?} was set to default value (via default impl)",
                                            index.yellow(),
                                            field.blue()
                                        );
                                    }
                                    reflect!(pop());
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
                            let default_val = Wip::<'facet>::alloc_shape(container_shape)
                                .map_err(|e| {
                                    JsonError::new(
                                        JsonErrorKind::ReflectError(e),
                                        input,
                                        last_span,
                                        wip.path(),
                                    )
                                })?
                                .put_default()
                                .map_err(|e| {
                                    JsonError::new(
                                        JsonErrorKind::ReflectError(e),
                                        input,
                                        last_span,
                                        wip.path(),
                                    )
                                })?
                                .build()
                                .map_err(|e| {
                                    JsonError::new(
                                        JsonErrorKind::ReflectError(e),
                                        input,
                                        last_span,
                                        wip.path(),
                                    )
                                })?;
                            let peek = default_val.peek().into_struct().unwrap();

                            for (index, field) in sd.fields.iter().enumerate() {
                                let is_set = wip.is_field_set(index).map_err(|err| {
                                    trace!("Error checking field set status: {:?}", err);
                                    JsonError::new(
                                        JsonErrorKind::ReflectError(err),
                                        input,
                                        last_span,
                                        wip.path(),
                                    )
                                })?;
                                if !is_set {
                                    let address_of_field_from_default =
                                        peek.field(index).unwrap().data();
                                    reflect!(field(index));
                                    reflect!(put_shape(
                                        address_of_field_from_default,
                                        field.shape()
                                    ));
                                    reflect!(pop());
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

                if reason == PopReason::TopLevel {
                    let path = wip.path();
                    return Ok(match wip.build() {
                        Ok(hv) => hv,
                        Err(e) => {
                            return Err(JsonError::new(
                                JsonErrorKind::ReflectError(e),
                                input,
                                last_span,
                                path,
                            ));
                        }
                    });
                } else {
                    reflect!(pop());
                }
            }
            Instruction::SkipValue => {
                let token = read_token!();
                match token.node {
                    Token::LBrace | Token::LBracket => {
                        // Skip a compound value by tracking nesting depth
                        let mut depth = 1;
                        while depth > 0 {
                            let token = read_token!();
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
                    }
                    Token::String(_)
                    | Token::F64(_)
                    | Token::I64(_)
                    | Token::U64(_)
                    | Token::True
                    | Token::False
                    | Token::Null => {
                        // Primitive value; nothing more to skip
                    }
                    other => {
                        // Unexpected token when skipping a value
                        bail!(JsonErrorKind::UnexpectedToken {
                            got: other,
                            wanted: "value"
                        });
                    }
                }
            }
            Instruction::Value => {
                let token = read_token!();
                match token.node {
                    Token::Null => {
                        reflect!(put_default());
                    }
                    _ => {
                        if matches!(wip.shape().def, Def::Option(_)) {
                            trace!("Starting Some(_) option for {}", wip.shape().blue());
                            reflect!(push_some());
                            stack.push(Instruction::Pop(PopReason::Some))
                        }

                        match token.node {
                            Token::Null => unreachable!(),
                            Token::LBrace => {
                                match wip.innermost_shape().def {
                                    Def::Map(_md) => {
                                        trace!(
                                            "Object starting for map value ({})!",
                                            wip.shape().blue()
                                        );
                                        reflect!(put_default());
                                    }
                                    Def::Enum(_ed) => {
                                        trace!(
                                            "Object starting for enum value ({})!",
                                            wip.shape().blue()
                                        );
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
                                        bail!(JsonErrorKind::UnsupportedType {
                                            got: wip.innermost_shape(),
                                            wanted: "map, enum, or struct"
                                        });
                                    }
                                }

                                stack.push(Instruction::ObjectKeyOrObjectClose)
                            }
                            Token::LBracket => {
                                match wip.innermost_shape().def {
                                    Def::Array(_) => {
                                        trace!(
                                            "Array starting for array ({})!",
                                            wip.shape().blue()
                                        );
                                    }
                                    Def::Slice(_) => {
                                        trace!(
                                            "Array starting for slice ({})!",
                                            wip.shape().blue()
                                        );
                                    }
                                    Def::List(_) => {
                                        trace!("Array starting for list ({})!", wip.shape().blue());
                                        reflect!(put_default());
                                    }
                                    Def::Enum(_) => {
                                        trace!("Array starting for enum ({})!", wip.shape().blue());
                                    }
                                    Def::Struct(s) => {
                                        if s.kind == StructKind::Tuple {
                                            trace!(
                                                "Array starting for tuple ({})!",
                                                wip.shape().blue()
                                            );
                                            // Special handling for unit type ()
                                            if s.fields.is_empty() {
                                                // Check if the array is empty by peeking at the next token
                                                let next_token = read_token!();
                                                if next_token.node == Token::RBracket {
                                                    // Empty array means unit type () - we're good
                                                    reflect!(put_default());
                                                } else {
                                                    // Non-empty array is not valid for unit type
                                                    bail!(JsonErrorKind::UnsupportedType {
                                                        got: wip.innermost_shape(),
                                                        wanted: "empty array",
                                                    });
                                                }
                                            } else {
                                                reflect!(put_default());
                                            }
                                        } else {
                                            bail!(JsonErrorKind::UnsupportedType {
                                                got: wip.shape(),
                                                wanted: "array, list, tuple, or slice"
                                            });
                                        }
                                    }
                                    Def::Scalar(s)
                                        if matches!(s.affinity, ScalarAffinity::Empty(_)) =>
                                    {
                                        trace!(
                                            "Array starting for tuple ({})!",
                                            wip.shape().blue()
                                        );
                                        // reflect!(put_default());
                                        // Check if the array is empty by peeking at the next token
                                        let next_token = read_token!();
                                        if next_token.node == Token::RBracket {
                                            // Empty array means unit type () - we're good
                                        } else {
                                            // Non-empty array is not valid for unit type
                                            bail!(JsonErrorKind::UnsupportedType {
                                                got: wip.innermost_shape(),
                                                wanted: "empty array",
                                            });
                                        }
                                    }
                                    _ => {
                                        bail!(JsonErrorKind::UnsupportedType {
                                            got: wip.innermost_shape(),
                                            wanted: "array, list, tuple, or slice"
                                        });
                                    }
                                }

                                trace!("Beginning pushback");
                                reflect!(begin_pushback());
                                stack.push(Instruction::ArrayItemOrArrayClose)
                            }
                            Token::RBrace | Token::RBracket | Token::Colon | Token::Comma => {
                                bail!(JsonErrorKind::UnexpectedToken {
                                    got: token.node,
                                    wanted: "value"
                                });
                            }
                            Token::String(s) => match wip.innermost_shape().def {
                                Def::Scalar(_sd) => {
                                    reflect!(put::<String>(s));
                                }
                                Def::Enum(_ed) => {
                                    if wip.selected_variant().is_some() {
                                        trace!("Have variant selected arleady, just putting");

                                        // just put, then — if it's a tuple field it'll work
                                        reflect!(put::<String>(s));
                                    } else {
                                        match wip.find_variant(&s) {
                                            Some((variant_index, _)) => {
                                                reflect!(variant(variant_index));
                                            }
                                            None => {
                                                bail!(JsonErrorKind::NoSuchVariant {
                                                    name: s.to_string(),
                                                    enum_shape: wip.shape()
                                                });
                                            }
                                        }
                                    }
                                }
                                _ => bail!(JsonErrorKind::UnsupportedType {
                                    got: wip.innermost_shape(),
                                    wanted: "enum or string"
                                }),
                            },
                            Token::F64(n) => {
                                if wip.innermost_shape() == <f32 as Facet>::SHAPE {
                                    reflect!(put(n as f32));
                                } else {
                                    reflect!(put(n));
                                }
                            }
                            Token::U64(n) => {
                                reflect!(put(n));
                            }
                            Token::I64(n) => {
                                reflect!(put(n));
                            }
                            Token::True => {
                                reflect!(put::<bool>(true));
                            }
                            Token::False => {
                                reflect!(put::<bool>(false));
                            }
                            Token::EOF => {
                                bail!(JsonErrorKind::UnexpectedEof("in value"));
                            }
                        }
                    }
                }
            }
            Instruction::ObjectKeyOrObjectClose => {
                let token = read_token!();
                match token.node {
                    Token::String(key) => {
                        trace!("Parsed object key: {}", key);

                        let mut ignore = false;
                        let mut needs_pop = true;

                        match wip.shape().def {
                            Def::Struct(_) => match wip.field_index(&key) {
                                Some(index) => {
                                    trace!("It's a struct field");
                                    reflect!(field(index));
                                }
                                None => {
                                    if wip.shape().has_deny_unknown_fields_attr() {
                                        trace!(
                                            "It's not a struct field AND we're denying unknown fields"
                                        );
                                        // well, it all depends.
                                        bail!(JsonErrorKind::UnknownField {
                                            field_name: key.to_string(),
                                            shape: wip.shape(),
                                        })
                                    } else {
                                        trace!(
                                            "It's not a struct field and we're ignoring unknown fields"
                                        );
                                        ignore = true;
                                    }
                                }
                            },
                            Def::Enum(_sd) => match wip.find_variant(&key) {
                                Some((index, variant)) => {
                                    trace!("Variant {} selected", variant.name.blue());
                                    reflect!(variant(index));
                                    needs_pop = false;
                                }
                                None => {
                                    if let Some(_variant_index) = wip.selected_variant() {
                                        trace!(
                                            "Already have a variant selected, treating key as struct field of variant"
                                        );
                                        // Try to find the field index of the key within the selected variant
                                        if let Some(index) = wip.field_index(&key) {
                                            trace!(
                                                "Found field {} in selected variant",
                                                key.blue()
                                            );
                                            reflect!(field(index));
                                        } else if wip.shape().has_deny_unknown_fields_attr() {
                                            trace!(
                                                "Unknown field in variant and denying unknown fields"
                                            );
                                            bail!(JsonErrorKind::UnknownField {
                                                field_name: key.to_string(),
                                                shape: wip.shape(),
                                            });
                                        } else {
                                            trace!("Ignoring unknown field in variant");
                                            // Mark to ignore this field below
                                            // (We do not set ignore here since it's not in struct outer branch)
                                            // Instead we handle ignoring in the calling code as needed
                                        }
                                    } else {
                                        bail!(JsonErrorKind::NoSuchVariant {
                                            name: key.to_string(),
                                            enum_shape: wip.shape()
                                        });
                                    }
                                }
                            },
                            Def::Map(_) => {
                                reflect!(push_map_key());
                                reflect!(put(key));
                                reflect!(push_map_value());
                            }
                            _ => {
                                bail!(JsonErrorKind::Unimplemented(
                                    "object key for non-struct/map"
                                ));
                            }
                        }

                        let colon = read_token!();
                        if colon.node != Token::Colon {
                            bail!(JsonErrorKind::UnexpectedToken {
                                got: colon.node,
                                wanted: "colon"
                            });
                        }
                        stack.push(Instruction::CommaThenObjectKeyOrObjectClose);
                        if ignore {
                            stack.push(Instruction::SkipValue);
                        } else {
                            if needs_pop {
                                trace!("Pushing Pop insn to stack (ObjectVal)");
                                stack.push(Instruction::Pop(PopReason::ObjectVal));
                            }
                            stack.push(Instruction::Value);
                        }
                    }
                    Token::RBrace => {
                        trace!("Object closing");
                    }
                    _ => {
                        bail!(JsonErrorKind::UnexpectedToken {
                            got: token.node,
                            wanted: "object key or closing brace"
                        });
                    }
                }
            }
            Instruction::CommaThenObjectKeyOrObjectClose => {
                let token = read_token!();
                match token.node {
                    Token::Comma => {
                        trace!("Object comma");
                        stack.push(Instruction::ObjectKeyOrObjectClose);
                    }
                    Token::RBrace => {
                        trace!("Object close");
                    }
                    _ => {
                        bail!(JsonErrorKind::UnexpectedToken {
                            got: token.node,
                            wanted: "comma"
                        });
                    }
                }
            }
            Instruction::ArrayItemOrArrayClose => {
                let token = read_token!();
                match token.node {
                    Token::RBracket => {
                        trace!("Array close");
                    }
                    _ => {
                        trace!("Array item");
                        put_back_token!(token);
                        reflect!(begin_pushback());
                        reflect!(push());

                        stack.push(Instruction::CommaThenArrayItemOrArrayClose);
                        trace!("Pushing Pop insn to stack (arrayitem)");
                        stack.push(Instruction::Pop(PopReason::ArrayItem));
                        stack.push(Instruction::Value);
                    }
                }
            }
            Instruction::CommaThenArrayItemOrArrayClose => {
                let token = read_token!();
                match token.node {
                    Token::RBracket => {
                        trace!("Array close");
                    }
                    Token::Comma => {
                        trace!("Array comma");
                        reflect!(push());
                        stack.push(Instruction::CommaThenArrayItemOrArrayClose);
                        trace!("Pushing Pop insn to stack (arrayitem)");
                        stack.push(Instruction::Pop(PopReason::ArrayItem));
                        stack.push(Instruction::Value);
                    }
                    _ => {
                        bail!(JsonErrorKind::UnexpectedToken {
                            got: token.node,
                            wanted: "comma or closing bracket"
                        });
                    }
                }
            }
        }
    }
}
