#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::std_instead_of_alloc)]
#![deny(unsafe_code)]
#![doc = include_str!("../README.md")]
//
// FIXME: don't! they were just distracting when running tests.
#![expect(warnings)]

mod error;
use std::borrow::Cow;

pub use error::*;

mod span;
use facet_core::{Characteristic, Def, Facet, FieldFlags};
use owo_colors::OwoColorize;
pub use span::*;

use facet_reflect::{HeapValue, ReflectError, Wip};
use log::{debug, trace};

extern crate alloc;

#[derive(PartialEq, Debug, Clone)]
enum Scalar<'input> {
    String(Cow<'input, str>),
    U64(u64),
    I64(i64),
    F64(f64),
}

#[derive(PartialEq, Debug, Clone)]
enum Expectation {
    Any,
    ObjectKeyOrObjectClose,
    ObjectValue,
}

#[derive(PartialEq, Debug, Clone)]
enum Outcome<'input> {
    GotScalar(Scalar<'input>),
    ListStarted,
    ListEnded,
    ObjectStarted,
    ObjectEnded,
}

impl<'input> Outcome<'input> {
    fn to_owned(self) -> Outcome<'static> {
        match self {
            Outcome::GotScalar(scalar) => {
                let owned_scalar = match scalar {
                    Scalar::String(cow) => Scalar::String(Cow::Owned(cow.into_owned())),
                    Scalar::U64(val) => Scalar::U64(val),
                    Scalar::I64(val) => Scalar::I64(val),
                    Scalar::F64(val) => Scalar::F64(val),
                };
                Outcome::GotScalar(owned_scalar)
            }
            Outcome::ListStarted => Outcome::ListStarted,
            Outcome::ListEnded => Outcome::ListEnded,
            Outcome::ObjectStarted => Outcome::ObjectStarted,
            Outcome::ObjectEnded => Outcome::ObjectEnded,
        }
    }
}

pub struct NextData<'input: 'facet, 'facet> {
    runner: StackRunner<'input>,
    wip: Wip<'facet>,
}

pub type NextResult<'input, 'facet, T, E> = (NextData<'input, 'facet>, Result<T, E>);

/// A format, like JSON, msgpack, etc.
trait Format {
    fn next<'input, 'facet>(
        nd: NextData<'input, 'facet>,
        expection: Expectation,
    ) -> NextResult<'input, 'facet, Outcome<'input>, DeserError<'input>>;
}

/// The JSON format
pub struct Json;

impl Format for Json {
    fn next<'input, 'facet>(
        mut nd: NextData<'input, 'facet>,
        mut expectation: Expectation,
    ) -> NextResult<'input, 'facet, Outcome<'input>, DeserError<'input>> {
        loop {
            // Skip whitespace
            let mut n = 0;
            while let Some(&ch) = nd.runner.input.get(n) {
                if ch == b' ' {
                    n += 1;
                } else {
                    break;
                }
            }

            // Update input to skip whitespace
            nd.runner.input = &nd.runner.input[n..];

            // Check if we've reached the end after skipping whitespace
            if nd.runner.input.is_empty() {
                let err = DeserError::new(
                    DeserErrorKind::UnexpectedEof("unexpected end of input after whitespace"),
                    nd.runner.input,
                    nd.runner.last_span,
                );
                return (nd, Err(err));
            }

            // Update 'next' with the new first character
            let next = nd.runner.input[0];
            let mut n = 0;
            let res = match next {
                b'0'..=b'9' => {
                    debug!("Found number");
                    while let Some(next) = nd.runner.input.get(n) {
                        if *next >= b'0' && *next <= b'9' {
                            n += 1;
                        } else {
                            break;
                        }
                    }
                    let num_slice = &nd.runner.input[0..n];
                    let num_str = std::str::from_utf8(num_slice).unwrap();
                    let number = num_str.parse::<u64>().unwrap();
                    nd.runner.input = &nd.runner.input[n..];
                    Ok(Outcome::GotScalar(Scalar::U64(number)))
                }
                b'"' => {
                    trace!("Found string");
                    n += 1; // Skip the opening quote
                    let start = n;

                    // Parse until closing quote
                    let mut escaped = false;
                    while let Some(&next) = nd.runner.input.get(n) {
                        if escaped {
                            escaped = false;
                        } else if next == b'\\' {
                            escaped = true;
                        } else if next == b'"' {
                            break;
                        }
                        n += 1;
                    }

                    // Skip the closing quote if found
                    if nd.runner.input.get(n) == Some(&b'"') {
                        n += 1;
                    }

                    let string_slice = &nd.runner.input[start..n - 1];
                    let string_content = std::str::from_utf8(string_slice).unwrap();
                    trace!("String content: {:?}", string_content);

                    nd.runner.input = &nd.runner.input[n..];
                    Ok(Outcome::GotScalar(Scalar::String(Cow::Borrowed(
                        string_content,
                    ))))
                }
                b'{' => {
                    nd.runner.input = &nd.runner.input[1..];
                    Ok(Outcome::ObjectStarted)
                }
                b':' => {
                    if expectation == Expectation::ObjectValue {
                        // makes sense, let's skip it and try again
                        nd.runner.input = &nd.runner.input[1..];
                        expectation = Expectation::Any;

                        continue;
                    } else {
                        trace!("Did not expect ObjectValue, expected {:?}", expectation);

                        Err(DeserError {
                            input: nd.runner.input.into(),
                            span: nd.runner.last_span,
                            kind: DeserErrorKind::Unimplemented("unexpected colon"),
                        })
                    }
                }
                b',' => {
                    if expectation == Expectation::ObjectKeyOrObjectClose {
                        // Let's skip the comma and try again
                        nd.runner.input = &nd.runner.input[1..];
                        continue;
                    } else {
                        trace!("Did not expect comma, expected {:?}", expectation);
                        Err(DeserError {
                            input: nd.runner.input.into(),
                            span: nd.runner.last_span,
                            kind: DeserErrorKind::Unimplemented("unexpected comma"),
                        })
                    }
                }
                b'}' => {
                    if expectation == Expectation::ObjectKeyOrObjectClose {
                        nd.runner.input = &nd.runner.input[1..];
                        Ok(Outcome::ObjectEnded)
                    } else {
                        Err(DeserError {
                            input: nd.runner.input.into(),
                            span: nd.runner.last_span,
                            kind: DeserErrorKind::Unimplemented("unexpected closing brace"),
                        })
                    }
                }
                c => Err(DeserError {
                    input: nd.runner.input.into(),
                    span: nd.runner.last_span,
                    kind: DeserErrorKind::Unimplemented("unexpected character"),
                }),
            };
            return (nd, res);
        }
    }
}

/// Represents the next expected token or structure while parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Instruction {
    Value(ValueReason),
    SkipValue,
    Pop(PopReason),
    ObjectKeyOrObjectClose,
    ArrayItemOrArrayClose,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ValueReason {
    TopLevel,
    ObjectVal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PopReason {
    TopLevel,
    ObjectVal,
    ArrayItem,
    Some,
}

pub fn deserialize<'input, 'facet, T: Facet<'facet>, F: Format>(
    input: &'input [u8],
) -> Result<T, DeserError<'input>>
where
    T: Facet<'facet>,
    'input: 'facet,
{
    let wip = Wip::alloc_shape(T::SHAPE).map_err(|e| DeserError {
        input: input.into(),
        span: Span { start: 0, len: 0 },
        kind: DeserErrorKind::ReflectError(e),
    })?;
    Ok(deserialize_wip::<F>(wip, input)?
        .materialize()
        .map_err(|e| DeserError::new_reflect(e, input, Span { start: 0, len: 0 }))?)
}

pub fn deserialize_wip<'input: 'facet, 'facet, F>(
    mut wip: Wip<'facet>,
    input: &'input [u8],
) -> Result<HeapValue<'facet>, DeserError<'input>>
where
    F: Format,
    'input: 'facet,
{
    // This struct is just a bundle of the state that we need to pass around all the time.
    let mut runner = StackRunner {
        stack: vec![
            Instruction::Pop(PopReason::TopLevel),
            Instruction::Value(ValueReason::TopLevel),
        ],
        input,
        last_span: Span::new(0, 0),
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
                        .map_err(|e| DeserError::new_reflect(e, input, runner.last_span));
                } else {
                    wip = wip
                        .pop()
                        .map_err(|e| DeserError::new_reflect(e, input, runner.last_span))?;
                }
            }
            Instruction::Value(_why) => {
                let nd = NextData { runner, wip };
                let expectation = match _why {
                    ValueReason::TopLevel => Expectation::Any,
                    ValueReason::ObjectVal => Expectation::ObjectValue,
                };
                let (nd, res) = F::next(nd, expectation);
                runner = nd.runner;
                wip = nd.wip;
                let outcome = res?;
                trace!("Got outcome {:?}", outcome.blue());
                wip = runner.value(wip, outcome)?
            }
            Instruction::ObjectKeyOrObjectClose => {
                let nd = NextData { runner, wip };
                let (nd, res) = F::next(nd, Expectation::ObjectKeyOrObjectClose);
                runner = nd.runner;
                wip = nd.wip;
                let outcome = res?;
                trace!("Got outcome {:?}", outcome.blue());
                wip = runner.object_key_or_object_close(wip, outcome)?;
            }
            _ => {
                todo!("Support instruction {:?}", insn)
            }
        }
    }
}

/// It runs along the stack!
struct StackRunner<'input> {
    /// Look! A stack!
    stack: Vec<Instruction>,
    input: &'input [u8],
    last_span: Span,
}

impl<'input> StackRunner<'input> {
    fn pop<'facet>(
        &mut self,
        mut wip: Wip<'facet>,
        reason: PopReason,
    ) -> Result<Wip<'facet>, DeserError<'input>> {
        trace!("Popping because {:?}", reason.yellow());

        let container_shape = wip.shape();
        match container_shape.def {
            Def::Struct(sd) => {
                let mut has_unset = false;

                trace!("Let's check all fields are initialized");
                for (index, field) in sd.fields.iter().enumerate() {
                    let is_set = wip.is_field_set(index).map_err(|err| {
                        trace!("Error checking field set status: {:?}", err);
                        DeserError::new_reflect(err, self.input, self.last_span)
                    })?;
                    if !is_set {
                        if field.flags.contains(FieldFlags::DEFAULT) {
                            wip = wip.field(index).map_err(|e| {
                                DeserError::new_reflect(e, self.input, self.last_span)
                            })?;
                            if let Some(default_in_place_fn) = field.vtable.default_fn {
                                wip = wip.put_from_fn(default_in_place_fn).map_err(|e| {
                                    DeserError::new_reflect(e, self.input, self.last_span)
                                })?;
                                trace!(
                                    "Field #{} {:?} was set to default value (via custom fn)",
                                    index.yellow(),
                                    field.blue()
                                );
                            } else {
                                if !field.shape().is(Characteristic::Default) {
                                    return Err(DeserError::new_reflect(
                                        ReflectError::DefaultAttrButNoDefaultImpl {
                                            shape: field.shape(),
                                        },
                                        self.input,
                                        self.last_span,
                                    ));
                                }
                                wip = wip.put_default().map_err(|e| {
                                    DeserError::new_reflect(e, self.input, self.last_span)
                                })?;
                                trace!(
                                    "Field #{} {:?} was set to default value (via default impl)",
                                    index.yellow(),
                                    field.blue()
                                );
                            }
                            wip = wip.pop().map_err(|e| {
                                DeserError::new_reflect(e, self.input, self.last_span)
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
                        .map_err(|e| DeserError::new_reflect(e, self.input, self.last_span))?
                        .put_default()
                        .map_err(|e| DeserError::new_reflect(e, self.input, self.last_span))?
                        .build()
                        .map_err(|e| DeserError::new_reflect(e, self.input, self.last_span))?;
                    let peek = default_val.peek().into_struct().unwrap();

                    for (index, field) in sd.fields.iter().enumerate() {
                        let is_set = wip.is_field_set(index).map_err(|err| {
                            trace!("Error checking field set status: {:?}", err);
                            DeserError::new_reflect(err, self.input, self.last_span)
                        })?;
                        if !is_set {
                            let address_of_field_from_default = peek.field(index).unwrap().data();
                            wip = wip.field(index).map_err(|e| {
                                DeserError::new_reflect(e, self.input, self.last_span)
                            })?;
                            wip = wip
                                .put_shape(address_of_field_from_default, field.shape())
                                .map_err(|e| {
                                    DeserError::new_reflect(e, self.input, self.last_span)
                                })?;
                            wip = wip.pop().map_err(|e| {
                                DeserError::new_reflect(e, self.input, self.last_span)
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
                trace!(
                    "Thing being popped is not a container I guess (it's a {})",
                    wip.shape()
                );
            }
        }
        Ok(wip)
    }

    /// Handle value parsing
    fn value<'facet>(
        &mut self,
        mut wip: Wip<'facet>,
        outcome: Outcome<'input>,
    ) -> Result<Wip<'facet>, DeserError<'input>> {
        match outcome {
            Outcome::GotScalar(s) => match s {
                Scalar::String(cow) => {
                    wip = wip
                        .put(cow.to_string())
                        .map_err(|e| DeserError::new_reflect(e, self.input, self.last_span))?;
                }
                Scalar::U64(value) => {
                    wip = wip
                        .put(value)
                        .map_err(|e| DeserError::new_reflect(e, self.input, self.last_span))?;
                }
                Scalar::I64(value) => {
                    wip = wip
                        .put(value)
                        .map_err(|e| DeserError::new_reflect(e, self.input, self.last_span))?;
                }
                Scalar::F64(value) => {
                    wip = wip
                        .put(value)
                        .map_err(|e| DeserError::new_reflect(e, self.input, self.last_span))?;
                }
            },
            Outcome::ListStarted => todo!(),
            Outcome::ListEnded => todo!(),
            Outcome::ObjectStarted => {
                match wip.innermost_shape().def {
                    Def::Map(_md) => {
                        trace!("Object starting for map value ({})!", wip.shape().blue());
                        wip = wip
                            .put_default()
                            .map_err(|e| DeserError::new_reflect(e, self.input, self.last_span))?;
                    }
                    Def::Enum(_ed) => {
                        trace!("Object starting for enum value ({})!", wip.shape().blue());
                        // nothing to do here
                    }
                    Def::Struct(_) => {
                        trace!("Object starting for struct value ({})!", wip.shape().blue());
                        // nothing to do here
                    }
                    _ => {
                        return Err(DeserError {
                            input: self.input.into(),
                            span: self.last_span,
                            kind: DeserErrorKind::UnsupportedType {
                                got: wip.innermost_shape(),
                                wanted: "map, enum, or struct",
                            },
                        });
                    }
                }

                self.stack.push(Instruction::ObjectKeyOrObjectClose);
            }
            Outcome::ObjectEnded => todo!(),
        }
        Ok(wip)
    }

    fn object_key_or_object_close<'facet>(
        &mut self,
        mut wip: Wip<'facet>,
        outcome: Outcome<'input>,
    ) -> Result<Wip<'facet>, DeserError<'input>>
    where
        'input: 'facet,
    {
        match outcome {
            Outcome::GotScalar(Scalar::String(key)) => {
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
                                DeserError::new_reflect(e, self.input, self.last_span)
                            })?;
                        } else {
                            // Check for flattened fields
                            let mut found_in_flatten = false;
                            for (index, field) in sd.fields.iter().enumerate() {
                                if field.flags.contains(FieldFlags::FLATTEN) {
                                    trace!("Found flattened field #{}", index);
                                    // Enter the flattened field
                                    wip = wip.field(index).map_err(|e| {
                                        DeserError::new_reflect(e, self.input, self.last_span)
                                    })?;

                                    // Check if this flattened field has the requested key
                                    if let Some(subfield_index) = wip.field_index(&key) {
                                        trace!("Found key {} in flattened field", key);
                                        wip = wip.field(subfield_index).map_err(|e| {
                                            DeserError::new_reflect(e, self.input, self.last_span)
                                        })?;
                                        found_in_flatten = true;
                                        handled_by_flatten = true;
                                        break;
                                    } else if let Some((_variant_index, _variant)) =
                                        wip.find_variant(&key)
                                    {
                                        trace!("Found key {} in flattened field", key);
                                        wip = wip.variant_named(&key).map_err(|e| {
                                            DeserError::new_reflect(e, self.input, self.last_span)
                                        })?;
                                        found_in_flatten = true;
                                        break;
                                    } else {
                                        // Key not in this flattened field, go back up
                                        wip = wip.pop().map_err(|e| {
                                            DeserError::new_reflect(e, self.input, self.last_span)
                                        })?;
                                    }
                                }
                            }

                            if !found_in_flatten {
                                if wip.shape().has_deny_unknown_fields_attr() {
                                    trace!(
                                        "It's not a struct field AND we're denying unknown fields"
                                    );
                                    return Err(DeserError::new(
                                        DeserErrorKind::UnknownField {
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
                                DeserError::new_reflect(e, self.input, self.last_span)
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
                                        DeserError::new_reflect(e, self.input, self.last_span)
                                    })?;
                                } else if wip.shape().has_deny_unknown_fields_attr() {
                                    trace!("Unknown field in variant and denying unknown fields");
                                    return Err(DeserError::new(
                                        DeserErrorKind::UnknownField {
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
                                return Err(DeserError::new(
                                    DeserErrorKind::NoSuchVariant {
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
                            .map_err(|e| DeserError::new_reflect(e, self.input, self.last_span))?;
                        wip = wip
                            .put(key.to_string())
                            .map_err(|e| DeserError::new_reflect(e, self.input, self.last_span))?;
                        wip = wip
                            .push_map_value()
                            .map_err(|e| DeserError::new_reflect(e, self.input, self.last_span))?;
                    }
                    _ => {
                        return Err(DeserError::new(
                            DeserErrorKind::Unimplemented("object key for non-struct/map"),
                            self.input,
                            self.last_span,
                        ));
                    }
                }

                self.stack.push(Instruction::ObjectKeyOrObjectClose);
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
                    self.stack.push(Instruction::Value(ValueReason::ObjectVal));
                }
                Ok(wip)
            }
            Outcome::ObjectEnded => {
                trace!("Object closing");
                Ok(wip)
            }
            _ => Err(DeserError::new(
                DeserErrorKind::UnexpectedOutcome(outcome.to_owned()),
                self.input,
                self.last_span,
            )),
        }
    }
}
