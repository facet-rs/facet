#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::std_instead_of_alloc)]
#![deny(unsafe_code)]
#![doc = include_str!("../README.md")]

extern crate alloc;

mod error;
use alloc::borrow::Cow;

pub use error::*;

mod span;
use facet_core::{Characteristic, Def, Facet, FieldFlags};
use owo_colors::OwoColorize;
pub use span::*;

use facet_reflect::{HeapValue, ReflectError, Wip};
use log::trace;

#[derive(PartialEq, Debug, Clone)]
/// A scalar value used during deserialization.
/// `u64` and `i64` are separated because `i64` doesn't fit in `u64`,
/// but having `u64` is a fast path for 64-bit architectures — no need to
/// go through `u128` / `i128` for everything
pub enum Scalar<'input> {
    /// Owned or borrowed string data.
    String(Cow<'input, str>),
    /// Unsigned 64-bit integer scalar.
    U64(u64),
    /// Signed 64-bit integer scalar.
    I64(i64),
    /// 64-bit floating-point scalar.
    F64(f64),
}

#[derive(PartialEq, Debug, Clone)]
/// Expected next input token or structure during deserialization.
pub enum Expectation {
    /// Accept any token.
    Any,
    /// Expect an object key or the end of an object.
    ObjectKeyOrObjectClose,
    /// Expect a value inside an object.
    ObjectVal,
    /// Expect a list item or the end of a list.
    ListItemOrListClose,
}

#[derive(PartialEq, Debug, Clone)]
/// Outcome of parsing the next input element.
pub enum Outcome<'input> {
    /// Parsed a scalar value.
    GotScalar(Scalar<'input>),
    /// Starting a list/array.
    ListStarted,
    /// Ending a list/array.
    ListEnded,
    /// Starting an object/map.
    ObjectStarted,
    /// Ending an object/map.
    ObjectEnded,
}

impl Outcome<'_> {
    fn into_owned(self) -> Outcome<'static> {
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

/// Carries the current parsing state and the in-progress value during deserialization.
/// This bundles the mutable context that must be threaded through parsing steps.
pub struct NextData<'input: 'facet, 'facet> {
    /// Controls the parsing flow and stack state.
    pub runner: StackRunner<'input>,
    /// Holds the intermediate representation of the value being built.
    pub wip: Wip<'facet>,
}

/// The result of advancing the parser: updated state and parse outcome or error.
pub type NextResult<'input, 'facet, T, E> = (NextData<'input, 'facet>, Result<T, E>);

/// Trait defining a deserialization format.
/// Provides the next parsing step based on current state and expected input.
pub trait Format {
    /// Advance the parser with current state and expectation, producing the next outcome or error.
    fn next<'input, 'facet>(
        nd: NextData<'input, 'facet>,
        expectation: Expectation,
    ) -> NextResult<'input, 'facet, Outcome<'input>, DeserError<'input>>;
}

/// Instructions guiding the parsing flow, indicating the next expected action or token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Instruction {
    /// Expect a value, specifying the context or reason.
    Value(ValueReason),
    /// Skip the next value; used to ignore an input.
    SkipValue,
    /// Indicate completion of a structure or value; triggers popping from stack.
    Pop(PopReason),
    /// Expect an object key or the end of an object.
    ObjectKeyOrObjectClose,
    /// Expect a list item or the end of a list.
    ListItemOrListClose,
}

/// Reasons for expecting a value, reflecting the current parse context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueReason {
    /// Parsing at the root level.
    TopLevel,
    /// Parsing a value inside an object.
    ObjectVal,
}

/// Reasons for popping a state from the stack, indicating why a scope is ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopReason {
    /// Ending the top-level parsing scope.
    TopLevel,
    /// Ending a value within an object.
    ObjectVal,
}

/// Deserialize a value of type `T` from raw input bytes using format `F`.
///
/// This function sets up the initial working state and drives the deserialization process,
/// ensuring that the resulting value is fully materialized and valid.
pub fn deserialize<'input, 'facet, T, F>(input: &'input [u8]) -> Result<T, DeserError<'input>>
where
    T: Facet<'facet>,
    F: Format,
    'input: 'facet,
{
    let wip = Wip::alloc_shape(T::SHAPE).map_err(|e| DeserError {
        input: input.into(),
        span: Span { start: 0, len: 0 },
        kind: DeserErrorKind::ReflectError(e),
    })?;
    deserialize_wip::<F>(wip, input)?
        .materialize()
        .map_err(|e| DeserError::new_reflect(e, input, Span { start: 0, len: 0 }))
}

/// Deserializes a working-in-progress value into a fully materialized heap value.
/// This function drives the parsing loop until the entire input is consumed and the value is complete.
pub fn deserialize_wip<'input, 'facet, F>(
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
                    ValueReason::ObjectVal => Expectation::ObjectVal,
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
            Instruction::ListItemOrListClose => {
                let nd = NextData { runner, wip };
                let (nd, res) = F::next(nd, Expectation::ListItemOrListClose);
                runner = nd.runner;
                wip = nd.wip;
                let outcome = res?;
                trace!("Got outcome {:?}", outcome.blue());
                wip = runner.list_item_or_list_close(wip, outcome)?;
            }
            _ => {
                todo!("Support instruction {:?}", insn)
            }
        }
    }
}

#[doc(hidden)]
/// Maintains the parsing state and context necessary to drive deserialization.
///
/// This struct tracks what the parser expects next, manages input position,
/// and remembers the span of the last processed token to provide accurate error reporting.
pub struct StackRunner<'input> {
    /// Stack of parsing instructions guiding the control flow.
    pub stack: Vec<Instruction>,
    /// The raw input data being deserialized.
    pub input: &'input [u8],
    /// Span of the last token or value processed.
    pub last_span: Span,
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
            Outcome::ListStarted => {
                match wip.innermost_shape().def {
                    Def::Array(_) => {
                        trace!("Array starting for array ({})!", wip.shape().blue());
                    }
                    Def::Slice(_) => {
                        trace!("Array starting for slice ({})!", wip.shape().blue());
                    }
                    Def::List(_) => {
                        trace!("Array starting for list ({})!", wip.shape().blue());
                        wip = wip
                            .put_default()
                            .map_err(|e| DeserError::new_reflect(e, self.input, self.last_span))?;
                    }
                    Def::Enum(_) => {
                        trace!("Array starting for enum ({})!", wip.shape().blue());
                    }
                    Def::Struct(_) => {
                        trace!("Array starting for tuple ({})!", wip.shape().blue());
                        wip = wip
                            .put_default()
                            .map_err(|e| DeserError::new_reflect(e, self.input, self.last_span))?;
                    }
                    _ => {
                        return Err(DeserError::new(
                            DeserErrorKind::UnsupportedType {
                                got: wip.innermost_shape(),
                                wanted: "array, list, tuple, or slice",
                            },
                            self.input,
                            self.last_span,
                        ));
                    }
                }

                trace!("Beginning pushback");
                self.stack.push(Instruction::ListItemOrListClose);
                wip = wip
                    .begin_pushback()
                    .map_err(|e| DeserError::new_reflect(e, self.input, self.last_span))?;
            }
            Outcome::ListEnded => {
                trace!("List closing");
                wip = wip
                    .pop()
                    .map_err(|e| DeserError::new_reflect(e, self.input, self.last_span))?;
            }
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
                DeserErrorKind::UnexpectedOutcome(outcome.into_owned()),
                self.input,
                self.last_span,
            )),
        }
    }

    fn list_item_or_list_close<'facet>(
        &mut self,
        mut wip: Wip<'facet>,
        outcome: Outcome<'input>,
    ) -> Result<Wip<'facet>, DeserError<'input>>
    where
        'input: 'facet,
    {
        match outcome {
            Outcome::ListEnded => {
                trace!("List close");
                Ok(wip)
            }
            Outcome::GotScalar(scalar) => {
                trace!("In list_item_or_item_list_close, got ");
                wip = wip
                    .push()
                    .map_err(|e| DeserError::new_reflect(e, self.input, self.last_span))?;
                match scalar {
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
                }
                wip = wip
                    .pop()
                    .map_err(|e| DeserError::new_reflect(e, self.input, self.last_span))?;

                self.stack.push(Instruction::ListItemOrListClose);

                Ok(wip)
            }
            _ => Err(DeserError::new(
                DeserErrorKind::UnexpectedOutcome(outcome.into_owned()),
                self.input,
                self.last_span,
            )),
        }
    }
}
