#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::std_instead_of_alloc)]
#![deny(unsafe_code)]
#![doc = include_str!("../README.md")]

extern crate alloc;

use alloc::string::ToString;
use alloc::{vec, vec::Vec};
use core::fmt::Debug;

mod debug;
mod error;
use alloc::borrow::Cow;
pub use debug::InputDebug;

pub use error::*;

mod span;
use facet_core::{
    Characteristic, Def, Facet, FieldFlags, PointerType, ScalarAffinity, SequenceType, StructKind,
    Type, UserType,
};
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
    /// Boolean scalar.
    Bool(bool),
    /// Null scalar (e.g. for formats supporting explicit null).
    Null,
}

#[derive(PartialEq, Debug, Clone)]
/// Expected next input token or structure during deserialization.
pub enum Expectation {
    /// Accept a value.
    Value,
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
    Scalar(Scalar<'input>),
    /// Starting a list/array.
    ListStarted,
    /// Ending a list/array.
    ListEnded,
    /// Starting an object/map.
    ObjectStarted,
    /// Ending an object/map.
    ObjectEnded,
}

impl<'input> From<Scalar<'input>> for Outcome<'input> {
    fn from(scalar: Scalar<'input>) -> Self {
        Outcome::Scalar(scalar)
    }
}

use core::fmt;

/// Display implementation for `Outcome`, focusing on user-friendly descriptions.
impl fmt::Display for Outcome<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Outcome::Scalar(scalar) => write!(f, "scalar {}", scalar),
            Outcome::ListStarted => write!(f, "list start"),
            Outcome::ListEnded => write!(f, "list end"),
            Outcome::ObjectStarted => write!(f, "object start"),
            Outcome::ObjectEnded => write!(f, "object end"),
        }
    }
}

/// Display implementation for `Scalar`, for use in displaying `Outcome`.
impl fmt::Display for Scalar<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Scalar::String(s) => write!(f, "string \"{}\"", s),
            Scalar::U64(val) => write!(f, "u64 {}", val),
            Scalar::I64(val) => write!(f, "i64 {}", val),
            Scalar::F64(val) => write!(f, "f64 {}", val),
            Scalar::Bool(val) => write!(f, "bool {}", val),
            Scalar::Null => write!(f, "null"),
        }
    }
}

impl Outcome<'_> {
    fn into_owned(self) -> Outcome<'static> {
        match self {
            Outcome::Scalar(scalar) => {
                let owned_scalar = match scalar {
                    Scalar::String(cow) => Scalar::String(Cow::Owned(cow.into_owned())),
                    Scalar::U64(val) => Scalar::U64(val),
                    Scalar::I64(val) => Scalar::I64(val),
                    Scalar::F64(val) => Scalar::F64(val),
                    Scalar::Bool(val) => Scalar::Bool(val),
                    Scalar::Null => Scalar::Null,
                };
                Outcome::Scalar(owned_scalar)
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
pub struct NextData<'input, 'facet, 'shape, C = Cooked, I = [u8]>
where
    'input: 'facet,
    I: ?Sized + 'input,
{
    /// The offset we're supposed to start parsing from
    start: usize,

    /// Controls the parsing flow and stack state.
    runner: StackRunner<'input, C, I>,

    /// Holds the intermediate representation of the value being built.
    pub wip: Wip<'facet, 'shape>,
}

impl<'input, 'facet, 'shape, C, I> NextData<'input, 'facet, 'shape, C, I>
where
    'input: 'facet,
    I: ?Sized + 'input,
{
    /// Returns the input (from the start! not from the current position)
    pub fn input(&self) -> &'input I {
        self.runner.input
    }

    /// Returns the parsing start offset.
    pub fn start(&self) -> usize {
        self.start
    }
}

/// The result of advancing the parser: updated state and parse outcome or error.
pub type NextResult<'input, 'facet, 'shape, T, E, C, I = [u8]> =
    (NextData<'input, 'facet, 'shape, C, I>, Result<T, E>);

/// Trait defining a deserialization format.
/// Provides the next parsing step based on current state and expected input.
pub trait Format {
    /// The kind of input this format consumes, parameterized by input lifetime.
    ///
    /// * `JsonFmt` => `Input<'input> = [u8]`
    /// * `CliFmt`  => `Input<'input> = [&'input str]`
    type Input<'input>: ?Sized;

    /// The type of span used by this format (Raw or Cooked)
    type SpanType: Debug + 'static;

    /// The lowercase source ID of the format, used for error reporting.
    fn source(&self) -> &'static str;

    /// Advance the parser with current state and expectation, producing the next outcome or error.
    #[allow(clippy::type_complexity)]
    fn next<'input, 'facet, 'shape>(
        &mut self,
        nd: NextData<'input, 'facet, 'shape, Self::SpanType, Self::Input<'input>>,
        expectation: Expectation,
    ) -> NextResult<
        'input,
        'facet,
        'shape,
        Spanned<Outcome<'input>, Self::SpanType>,
        Spanned<DeserErrorKind<'shape>, Self::SpanType>,
        Self::SpanType,
        Self::Input<'input>,
    >
    where
        'shape: 'input;

    /// Skip the next value; used to ignore an input.
    #[allow(clippy::type_complexity)]
    fn skip<'input, 'facet, 'shape>(
        &mut self,
        nd: NextData<'input, 'facet, 'shape, Self::SpanType, Self::Input<'input>>,
    ) -> NextResult<
        'input,
        'facet,
        'shape,
        Span<Self::SpanType>,
        Spanned<DeserErrorKind<'shape>, Self::SpanType>,
        Self::SpanType,
        Self::Input<'input>,
    >
    where
        'shape: 'input;
}

/// Trait handling conversion regardless of `Format::SpanType` to `Span<Cooked>`
pub trait ToCooked<'input, F: Format> {
    /// Convert a span to a Cooked span (with byte index over the input, not format-specific index)
    fn to_cooked(self, format: &F, input: &'input F::Input<'input>) -> Span<Cooked>;
}

impl<'input, F: Format> ToCooked<'input, F> for Span<Cooked> {
    #[inline]
    fn to_cooked(self, _format: &F, _input: &'input F::Input<'input>) -> Span<Cooked> {
        self
    }
}

impl<'input, F: Format<SpanType = Raw, Input<'input> = [&'input str]>> ToCooked<'input, F>
    for Span<Raw>
{
    #[inline]
    fn to_cooked(self, _format: &F, input: &'input [&'input str]) -> Span<Cooked> {
        // Calculate start position by summing lengths of preceding args plus spaces
        let mut start = 0;
        for arg in input.iter().take(self.start) {
            start += arg.len() + 1; // +1 for space between args
        }

        // Length is the length of the current arg
        let len = input[self.start].len();

        Span::<Cooked>::new(start, len)
    }
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
    /// Ending value within a list
    ListVal,
    /// Ending a `Some()` in an option
    Some,
}

mod deser_impl {
    use super::*;

    /// Deserialize a value of type `T` from raw input bytes using format `F`.
    ///
    /// This function sets up the initial working state and drives the deserialization process,
    /// ensuring that the resulting value is fully materialized and valid.
    pub fn deserialize<'input, 'facet, 'shape, T, F>(
        input: &'input F::Input<'input>,
        format: &mut F,
    ) -> Result<T, DeserError<'input, 'shape, Cooked>>
    where
        T: Facet<'facet>,
        F: Format + 'shape,
        F::Input<'input>: InputDebug,
        F::SpanType: core::fmt::Debug,
        Span<F::SpanType>: ToCooked<'input, F>,
        'input: 'facet,
        'shape: 'input,
    {
        // Run the entire deserialization process and capture any errors
        let result: Result<T, DeserError<'input, 'shape, Cooked>> = {
            let source = format.source();

            // Step 1: Allocate shape
            let wip = match Wip::alloc_shape(T::SHAPE) {
                Ok(wip) => wip,
                Err(e) => {
                    let default_span = Span::<F::SpanType>::default();
                    // let cooked_span = cook_span_dispatch!(format, default_span, input);
                    let cooked_span = default_span.to_cooked(format, input);
                    return Err(DeserError::new_reflect(e, input, cooked_span, source));
                }
            };

            // Step 2: Run deserialize_wip
            let heap_value = match deserialize_wip(wip, input, format) {
                Ok(val) => val,
                Err(e) => {
                    let cooked_span = e.span.to_cooked(format, input);

                    // Create a completely new error variable with the Cooked type
                    let cooked_error = DeserError {
                        input: e.input,
                        span: cooked_span,
                        kind: e.kind,
                        source_id: e.source_id,
                    };

                    return Err(cooked_error);
                }
            };

            // Step 3: Materialize
            match heap_value.materialize() {
                Ok(val) => Ok(val),
                Err(e) => {
                    let default_span = Span::<F::SpanType>::default();
                    let cooked_span = default_span.to_cooked(format, input);
                    return Err(DeserError::new_reflect(e, input, cooked_span, source));
                }
            }
        };

        // Apply span conversion for errors from materialization
        match result {
            Ok(value) => Ok(value),
            Err(mut error) => {
                let new_span = error.span.to_cooked(format, input);

                if new_span != error.span {
                    error = DeserError {
                        input: error.input,
                        span: new_span,
                        kind: error.kind,
                        source_id: error.source_id,
                    };
                }

                Err(error)
            }
        }
    }
}

/// Deserialize a value of type `T` from raw input bytes using format `F`.
///
/// This function sets up the initial working state and drives the deserialization process,
/// ensuring that the resulting value is fully materialized and valid.
pub fn deserialize<'input, 'facet, 'shape, T, F>(
    input: &'input F::Input<'input>,
    format: F,
) -> Result<T, DeserError<'input, 'shape, Cooked>>
where
    T: Facet<'facet>,
    F: Format + 'shape,
    F::Input<'input>: InputDebug,
    F::SpanType: core::fmt::Debug,
    Span<F::SpanType>: ToCooked<'input, F>,
    'input: 'facet,
    'shape: 'input,
{
    let mut format_copy = format;
    deser_impl::deserialize(input, &mut format_copy)
}

/// Deserializes a working-in-progress value into a fully materialized heap value.
/// This function drives the parsing loop until the entire input is consumed and the value is complete.
pub fn deserialize_wip<'input, 'facet, 'shape, F>(
    mut wip: Wip<'facet, 'shape>,
    input: &'input F::Input<'input>,
    format: &mut F,
) -> Result<HeapValue<'facet, 'shape>, DeserError<'input, 'shape, Cooked>>
where
    F: Format + 'shape,
    F::Input<'input>: InputDebug,
    Span<F::SpanType>: ToCooked<'input, F>,
    'input: 'facet,
    'shape: 'input,
{
    // This struct is just a bundle of the state that we need to pass around all the time.
    let mut runner = StackRunner {
        original_input: input,
        input,
        stack: vec![
            Instruction::Pop(PopReason::TopLevel),
            Instruction::Value(ValueReason::TopLevel),
        ],
        last_span: Span::new(0, 0),
        format_source: format.source(),
    };

    macro_rules! next {
        ($runner:ident, $wip:ident, $expectation:expr, $method:ident) => {{
            let nd = NextData {
                start: $runner.last_span.end(), // or supply the appropriate start value if available
                runner: $runner,
                wip: $wip,
            };
            let (nd, res) = format.next(nd, $expectation);
            $runner = nd.runner;
            $wip = nd.wip;
            let outcome = res.map_err(|span_kind| {
                $runner.last_span = span_kind.span;
                let error = $runner.err(span_kind.node);
                // Convert the error's span to Cooked
                DeserError {
                    input: error.input,
                    span: error.span.to_cooked(format, input),
                    kind: error.kind,
                    source_id: error.source_id,
                }
            })?;
            $runner.last_span = outcome.span;
            $wip = $runner.$method($wip, outcome).map_err(|error| {
                DeserError {
                    input:  error.input,
                    span:   error.span.to_cooked(format, input),
                    kind:   error.kind,
                    source_id: error.source_id,
                }
            })?;
        }};
    }

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
                wip = runner.pop(wip, reason).map_err(|error| {
                    // Convert the error's span to Cooked
                    DeserError {
                        input: error.input,
                        span: error.span.to_cooked(format, input),
                        kind: error.kind,
                        source_id: error.source_id,
                    }
                })?;

                if reason == PopReason::TopLevel {
                    return wip.build().map_err(|e| {
                        let reflect_error = runner.reflect_err(e);
                        // Convert the reflection error's span to Cooked
                        DeserError {
                            input: reflect_error.input,
                            span: reflect_error.span.to_cooked(format, input),
                            kind: reflect_error.kind,
                            source_id: reflect_error.source_id,
                        }
                    });
                } else {
                    wip = wip.pop().map_err(|e| {
                        let reflect_error = runner.reflect_err(e);
                        // Convert the reflection error's span to Cooked
                        DeserError {
                            input: reflect_error.input,
                            span: reflect_error.span.to_cooked(format, input),
                            kind: reflect_error.kind,
                            source_id: reflect_error.source_id,
                        }
                    })?;
                }
            }
            Instruction::Value(_why) => {
                let expectation = match _why {
                    ValueReason::TopLevel => Expectation::Value,
                    ValueReason::ObjectVal => Expectation::ObjectVal,
                };
                next!(runner, wip, expectation, value);
            }
            Instruction::ObjectKeyOrObjectClose => {
                next!(
                    runner,
                    wip,
                    Expectation::ObjectKeyOrObjectClose,
                    object_key_or_object_close
                );
            }
            Instruction::ListItemOrListClose => {
                next!(
                    runner,
                    wip,
                    Expectation::ListItemOrListClose,
                    list_item_or_list_close
                );
            }
            Instruction::SkipValue => {
                // Call F::skip to skip over the next value in the input
                let nd = NextData {
                    start: runner.last_span.end(),
                    runner,
                    wip,
                };
                let (nd, res) = format.skip(nd);
                runner = nd.runner;
                wip = nd.wip;
                // Only propagate error, don't modify wip, since skip just advances input
                let span = res.map_err(|span_kind| {
                    runner.last_span = span_kind.span;
                    let error = runner.err(span_kind.node);
                    // Convert the error's span to Cooked
                    DeserError {
                        input: error.input,
                        span: error.span.to_cooked(format, input),
                        kind: error.kind,
                        source_id: error.source_id,
                    }
                })?;
                // do the actual skip
                runner.last_span = span;
            }
        }
    }
}

#[doc(hidden)]
/// Maintains the parsing state and context necessary to drive deserialization.
///
/// This struct tracks what the parser expects next, manages input position,
/// and remembers the span of the last processed token to provide accurate error reporting.
pub struct StackRunner<'input, C = Cooked, I: ?Sized + 'input = [u8]> {
    /// A version of the input that doesn't advance as we parse.
    pub original_input: &'input I,

    /// The raw input data being deserialized.
    pub input: &'input I,

    /// Stack of parsing instructions guiding the control flow.
    pub stack: Vec<Instruction>,

    /// Span of the last processed token, for accurate error reporting.
    pub last_span: Span<C>,

    /// Format source identifier for error reporting
    pub format_source: &'static str,
}

impl<'input, 'shape, C, I: ?Sized + 'input> StackRunner<'input, C, I>
where
    I: InputDebug,
{
    /// Convenience function to create a DeserError using the original input and last_span.
    fn err(&self, kind: DeserErrorKind<'shape>) -> DeserError<'input, 'shape, C> {
        DeserError::new(
            kind,
            self.original_input,
            self.last_span,
            self.format_source,
        )
    }

    /// Convenience function to create a DeserError from a ReflectError,
    /// using the original input and last_span for context.
    fn reflect_err(&self, err: ReflectError<'shape>) -> DeserError<'input, 'shape, C> {
        DeserError::new_reflect(err, self.original_input, self.last_span, self.format_source)
    }

    pub fn pop<'facet>(
        &mut self,
        mut wip: Wip<'facet, 'shape>,
        reason: PopReason,
    ) -> Result<Wip<'facet, 'shape>, DeserError<'input, 'shape, C>> {
        trace!("Popping because {:?}", reason.yellow());

        let container_shape = wip.shape();
        match container_shape.ty {
            Type::User(UserType::Struct(sd)) => {
                let mut has_unset = false;

                trace!("Let's check all fields are initialized");
                for (index, field) in sd.fields.iter().enumerate() {
                    let is_set = wip.is_field_set(index).map_err(|err| {
                        trace!("Error checking field set status: {:?}", err);
                        self.reflect_err(err)
                    })?;
                    if !is_set {
                        if field.flags.contains(FieldFlags::DEFAULT) {
                            wip = wip.field(index).map_err(|e| self.reflect_err(e))?;
                            if let Some(default_in_place_fn) = field.vtable.default_fn {
                                wip = wip
                                    .put_from_fn(default_in_place_fn)
                                    .map_err(|e| self.reflect_err(e))?;
                                trace!(
                                    "Field #{} {} @ {} was set to default value (via custom fn)",
                                    index.yellow(),
                                    field.name.green(),
                                    field.offset.blue(),
                                );
                            } else {
                                if !field.shape().is(Characteristic::Default) {
                                    return Err(self.reflect_err(
                                        ReflectError::DefaultAttrButNoDefaultImpl {
                                            shape: field.shape(),
                                        },
                                    ));
                                }
                                wip = wip.put_default().map_err(|e| self.reflect_err(e))?;
                                trace!(
                                    "Field #{} {} @ {} was set to default value (via default impl)",
                                    index.yellow(),
                                    field.name.green(),
                                    field.offset.blue(),
                                );
                            }
                            wip = wip.pop().map_err(|e| self.reflect_err(e))?;
                        } else {
                            trace!(
                                "Field #{} {} @ {} is not initialized",
                                index.yellow(),
                                field.name.green(),
                                field.offset.blue(),
                            );
                            has_unset = true;
                        }
                    }
                }

                if has_unset && container_shape.has_default_attr() {
                    // let's allocate and build a default value
                    let default_val = Wip::alloc_shape(container_shape)
                        .map_err(|e| self.reflect_err(e))?
                        .put_default()
                        .map_err(|e| self.reflect_err(e))?
                        .build()
                        .map_err(|e| self.reflect_err(e))?;
                    let peek = default_val.peek().into_struct().unwrap();

                    for (index, field) in sd.fields.iter().enumerate() {
                        let is_set = wip.is_field_set(index).map_err(|err| {
                            trace!("Error checking field set status: {:?}", err);
                            self.reflect_err(err)
                        })?;
                        if !is_set {
                            let address_of_field_from_default = peek.field(index).unwrap().data();
                            wip = wip.field(index).map_err(|e| self.reflect_err(e))?;
                            wip = wip
                                .put_shape(address_of_field_from_default, field.shape())
                                .map_err(|e| self.reflect_err(e))?;
                            wip = wip.pop().map_err(|e| self.reflect_err(e))?;
                        }
                    }
                }
            }
            Type::User(UserType::Enum(ed)) => {
                trace!("Checking if enum is initialized correctly");

                // Check if a variant has been selected
                if let Some(variant) = wip.selected_variant() {
                    trace!("Variant {} is selected", variant.name.blue());

                    // Check if all fields in the variant are initialized
                    if !variant.data.fields.is_empty() {
                        let mut has_unset = false;

                        for (index, field) in variant.data.fields.iter().enumerate() {
                            let is_set = wip.is_field_set(index).map_err(|err| {
                                trace!("Error checking field set status: {:?}", err);
                                self.reflect_err(err)
                            })?;

                            if !is_set {
                                if field.flags.contains(FieldFlags::DEFAULT) {
                                    wip = wip.field(index).map_err(|e| self.reflect_err(e))?;
                                    if let Some(default_in_place_fn) = field.vtable.default_fn {
                                        wip = wip
                                            .put_from_fn(default_in_place_fn)
                                            .map_err(|e| self.reflect_err(e))?;
                                        trace!(
                                            "Field #{} @ {} in variant {} was set to default value (via custom fn)",
                                            index.yellow(),
                                            field.offset.blue(),
                                            variant.name
                                        );
                                    } else {
                                        if !field.shape().is(Characteristic::Default) {
                                            return Err(self.reflect_err(
                                                ReflectError::DefaultAttrButNoDefaultImpl {
                                                    shape: field.shape(),
                                                },
                                            ));
                                        }
                                        wip = wip.put_default().map_err(|e| self.reflect_err(e))?;
                                        trace!(
                                            "Field #{} @ {} in variant {} was set to default value (via default impl)",
                                            index.yellow(),
                                            field.offset.blue(),
                                            variant.name
                                        );
                                    }
                                    wip = wip.pop().map_err(|e| self.reflect_err(e))?;
                                } else {
                                    trace!(
                                        "Field #{} @ {} in variant {} is not initialized",
                                        index.yellow(),
                                        field.offset.blue(),
                                        variant.name
                                    );
                                    has_unset = true;
                                }
                            }
                        }

                        if has_unset && container_shape.has_default_attr() {
                            trace!("Enum has DEFAULT attr but variant has uninitialized fields");
                            // Handle similar to struct, allocate and build default value for variant
                            let default_val = Wip::alloc_shape(container_shape)
                                .map_err(|e| self.reflect_err(e))?
                                .put_default()
                                .map_err(|e| self.reflect_err(e))?
                                .build()
                                .map_err(|e| self.reflect_err(e))?;

                            let peek = default_val.peek();
                            let peek_enum = peek.into_enum().map_err(|e| self.reflect_err(e))?;
                            let default_variant = peek_enum
                                .active_variant()
                                .map_err(|e| self.err(DeserErrorKind::VariantError(e)))?;

                            if default_variant == &variant {
                                // It's the same variant, fill in the missing fields
                                for (index, field) in variant.data.fields.iter().enumerate() {
                                    let is_set = wip.is_field_set(index).map_err(|err| {
                                        trace!("Error checking field set status: {:?}", err);
                                        self.reflect_err(err)
                                    })?;
                                    if !is_set {
                                        if let Ok(Some(def_field)) = peek_enum.field(index) {
                                            wip = wip
                                                .field(index)
                                                .map_err(|e| self.reflect_err(e))?;
                                            wip = wip
                                                .put_shape(def_field.data(), field.shape())
                                                .map_err(|e| self.reflect_err(e))?;
                                            wip = wip.pop().map_err(|e| self.reflect_err(e))?;
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else if container_shape.has_default_attr() {
                    // No variant selected, but enum has default attribute - set to default
                    trace!("No variant selected but enum has DEFAULT attr; setting to default");
                    let default_val = Wip::alloc_shape(container_shape)
                        .map_err(|e| self.reflect_err(e))?
                        .put_default()
                        .map_err(|e| self.reflect_err(e))?
                        .build()
                        .map_err(|e| self.reflect_err(e))?;

                    let peek = default_val.peek();
                    let peek_enum = peek.into_enum().map_err(|e| self.reflect_err(e))?;
                    let default_variant_idx = peek_enum
                        .variant_index()
                        .map_err(|e| self.err(DeserErrorKind::VariantError(e)))?;

                    // Select the default variant
                    wip = wip
                        .variant(default_variant_idx)
                        .map_err(|e| self.reflect_err(e))?;

                    // Copy all fields from default value
                    let variant = &ed.variants[default_variant_idx];
                    for (index, field) in variant.data.fields.iter().enumerate() {
                        if let Ok(Some(def_field)) = peek_enum.field(index) {
                            wip = wip.field(index).map_err(|e| self.reflect_err(e))?;
                            wip = wip
                                .put_shape(def_field.data(), field.shape())
                                .map_err(|e| self.reflect_err(e))?;
                            wip = wip.pop().map_err(|e| self.reflect_err(e))?;
                        }
                    }
                }
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

    /// Internal common handler for GotScalar outcome, to deduplicate code.
    fn handle_scalar<'facet>(
        &self,
        wip: Wip<'facet, 'shape>,
        scalar: Scalar<'input>,
    ) -> Result<Wip<'facet, 'shape>, DeserError<'input, 'shape, C>>
    where
        'input: 'facet, // 'input outlives 'facet
    {
        match scalar {
            Scalar::String(cow) => {
                match wip.innermost_shape().ty {
                    Type::User(UserType::Enum(_)) => {
                        if wip.selected_variant().is_some() {
                            // If we already have a variant selected, just put the string
                            wip.put(cow.to_string()).map_err(|e| self.reflect_err(e))
                        } else {
                            // Try to select the variant
                            match wip.find_variant(&cow) {
                                Some((variant_index, _)) => {
                                    wip.variant(variant_index).map_err(|e| self.reflect_err(e))
                                }
                                None => Err(self.err(DeserErrorKind::NoSuchVariant {
                                    name: cow.to_string(),
                                    enum_shape: wip.innermost_shape(),
                                })),
                            }
                        }
                    }
                    Type::Pointer(PointerType::Reference(_))
                        if wip.innermost_shape().is_type::<&str>() =>
                    {
                        // This is for handling the &str type
                        // The Cow may be Borrowed (we may have an owned string but need a &str)
                        match cow {
                            Cow::Borrowed(s) => wip.put(s).map_err(|e| self.reflect_err(e)),
                            Cow::Owned(s) => wip.put(s).map_err(|e| self.reflect_err(e)),
                        }
                    }
                    _ => wip.put(cow.to_string()).map_err(|e| self.reflect_err(e)),
                }
            }
            Scalar::U64(value) => wip.put(value).map_err(|e| self.reflect_err(e)),
            Scalar::I64(value) => wip.put(value).map_err(|e| self.reflect_err(e)),
            Scalar::F64(value) => wip.put(value).map_err(|e| self.reflect_err(e)),
            Scalar::Bool(value) => wip.put(value).map_err(|e| self.reflect_err(e)),
            Scalar::Null => wip.put_default().map_err(|e| self.reflect_err(e)),
        }
    }

    /// Handle value parsing
    fn value<'facet>(
        &mut self,
        mut wip: Wip<'facet, 'shape>,
        outcome: Spanned<Outcome<'input>, C>,
    ) -> Result<Wip<'facet, 'shape>, DeserError<'input, 'shape, C>>
    where
        'input: 'facet, // 'input must outlive 'facet
    {
        trace!(
            "Handling value at {} (innermost {})",
            wip.shape().blue(),
            wip.innermost_shape().yellow()
        );

        match outcome.node {
            Outcome::Scalar(Scalar::Null) => {
                return wip.put_default().map_err(|e| self.reflect_err(e));
            }
            _ => {
                if matches!(wip.shape().def, Def::Option(_)) {
                    // TODO: Update option handling
                    trace!("Starting Some(_) option for {}", wip.shape().blue());
                    wip = wip.push_some().map_err(|e| self.reflect_err(e))?;
                    self.stack.push(Instruction::Pop(PopReason::Some));
                }
            }
        }

        match outcome.node {
            Outcome::Scalar(s) => {
                wip = self.handle_scalar(wip, s)?;
            }
            Outcome::ListStarted => {
                let shape = wip.innermost_shape();
                match shape.def {
                    Def::Array(_) => {
                        trace!("Array starting for array ({})!", shape.blue());
                        // We'll initialize the array elements one by one through the pushback workflow
                        // Don't call put_default, as arrays need different initialization
                    }
                    Def::Slice(_) => {
                        trace!("Array starting for slice ({})!", shape.blue());
                    }
                    Def::List(_) => {
                        trace!("Array starting for list ({})!", shape.blue());
                        wip = wip.put_default().map_err(|e| self.reflect_err(e))?;
                    }
                    Def::Scalar(sd) => {
                        if matches!(sd.affinity, ScalarAffinity::Empty(_)) {
                            trace!("Empty tuple/scalar, nice");
                            wip = wip.put_default().map_err(|e| self.reflect_err(e))?;
                        } else {
                            return Err(self.err(DeserErrorKind::UnsupportedType {
                                got: shape,
                                wanted: "array, list, tuple, or slice",
                            }));
                        }
                    }
                    _ => {
                        // For non-collection types, check the Type enum
                        if let Type::User(user_ty) = shape.ty {
                            match user_ty {
                                UserType::Enum(_) => {
                                    trace!("Array starting for enum ({})!", shape.blue());
                                }
                                UserType::Struct(_) => {
                                    trace!("Array starting for tuple struct ({})!", shape.blue());
                                    wip = wip.put_default().map_err(|e| self.reflect_err(e))?;
                                }
                                _ => {
                                    return Err(self.err(DeserErrorKind::UnsupportedType {
                                        got: shape,
                                        wanted: "array, list, tuple, or slice",
                                    }));
                                }
                            }
                        } else if let Type::Sequence(SequenceType::Tuple(tuple_type)) = shape.ty {
                            trace!(
                                "Array starting for tuple ({}) with {} fields!",
                                shape.blue(),
                                tuple_type.fields.len()
                            );
                            // Initialize the tuple with default values
                            wip = wip.put_default().map_err(|e| self.reflect_err(e))?;
                            // No special handling needed here - the tuple is already set up correctly
                            // and will receive array elements via pushback
                        } else {
                            return Err(self.err(DeserErrorKind::UnsupportedType {
                                got: shape,
                                wanted: "array, list, tuple, or slice",
                            }));
                        }
                    }
                }
                trace!("Beginning pushback");
                self.stack.push(Instruction::ListItemOrListClose);
                wip = wip.begin_pushback().map_err(|e| self.reflect_err(e))?;
            }
            Outcome::ListEnded => {
                trace!("List closing");
                wip = wip.pop().map_err(|e| self.reflect_err(e))?;
            }
            Outcome::ObjectStarted => {
                let shape = wip.innermost_shape();
                match shape.def {
                    Def::Map(_md) => {
                        trace!("Object starting for map value ({})!", shape.blue());
                        wip = wip.put_default().map_err(|e| self.reflect_err(e))?;
                    }
                    _ => {
                        // For non-collection types, check the Type enum
                        if let Type::User(user_ty) = shape.ty {
                            match user_ty {
                                UserType::Enum(_) => {
                                    trace!("Object starting for enum value ({})!", shape.blue());
                                    // nothing to do here
                                }
                                UserType::Struct(_) => {
                                    trace!("Object starting for struct value ({})!", shape.blue());
                                    // nothing to do here
                                }
                                _ => {
                                    return Err(self.err(DeserErrorKind::UnsupportedType {
                                        got: shape,
                                        wanted: "map, enum, or struct",
                                    }));
                                }
                            }
                        } else if let Type::Sequence(SequenceType::Tuple(tuple_type)) = shape.ty {
                            // This could be a tuple that was serialized as an object
                            // Despite this being unusual, we'll handle it here for robustness
                            trace!(
                                "Object starting for tuple ({}) with {} fields - unusual but handling",
                                shape.blue(),
                                tuple_type.fields.len()
                            );
                            // Initialize the tuple with default values
                            wip = wip.put_default().map_err(|e| self.reflect_err(e))?;
                        } else {
                            return Err(self.err(DeserErrorKind::UnsupportedType {
                                got: shape,
                                wanted: "map, enum, struct, or tuple",
                            }));
                        }
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
        mut wip: Wip<'facet, 'shape>,
        outcome: Spanned<Outcome<'input>, C>,
    ) -> Result<Wip<'facet, 'shape>, DeserError<'input, 'shape, C>>
    where
        'input: 'facet,
    {
        match outcome.node {
            Outcome::Scalar(Scalar::String(key)) => {
                trace!("Parsed object key: {}", key.cyan());

                let mut ignore = false;
                let mut needs_pop = true;
                let mut handled_by_flatten = false;

                let shape = wip.innermost_shape();
                match shape.ty {
                    Type::User(UserType::Struct(sd)) => {
                        // First try to find a direct field match
                        if let Some(index) = wip.field_index(&key) {
                            trace!("It's a struct field");
                            wip = wip.field(index).map_err(|e| self.reflect_err(e))?;
                        } else {
                            // Check for flattened fields
                            let mut found_in_flatten = false;
                            for (index, field) in sd.fields.iter().enumerate() {
                                if field.flags.contains(FieldFlags::FLATTEN) {
                                    trace!("Found flattened field #{}", index);
                                    // Enter the flattened field
                                    wip = wip.field(index).map_err(|e| self.reflect_err(e))?;

                                    // Check if this flattened field has the requested key
                                    if let Some(subfield_index) = wip.field_index(&key) {
                                        trace!("Found key {} in flattened field", key);
                                        wip = wip
                                            .field(subfield_index)
                                            .map_err(|e| self.reflect_err(e))?;
                                        found_in_flatten = true;
                                        handled_by_flatten = true;
                                        break;
                                    } else if let Some((_variant_index, _variant)) =
                                        wip.find_variant(&key)
                                    {
                                        trace!("Found key {} in flattened field", key);
                                        wip = wip
                                            .variant_named(&key)
                                            .map_err(|e| self.reflect_err(e))?;
                                        found_in_flatten = true;
                                        break;
                                    } else {
                                        // Key not in this flattened field, go back up
                                        wip = wip.pop().map_err(|e| self.reflect_err(e))?;
                                    }
                                }
                            }

                            if !found_in_flatten {
                                if wip.shape().has_deny_unknown_fields_attr() {
                                    trace!(
                                        "It's not a struct field AND we're denying unknown fields"
                                    );
                                    return Err(self.err(DeserErrorKind::UnknownField {
                                        field_name: key.to_string(),
                                        shape: wip.shape(),
                                    }));
                                } else {
                                    trace!(
                                        "It's not a struct field and we're ignoring unknown fields"
                                    );
                                    ignore = true;
                                }
                            }
                        }
                    }
                    Type::User(UserType::Enum(_ed)) => match wip.find_variant(&key) {
                        Some((index, variant)) => {
                            trace!(
                                "Selecting variant {}::{}",
                                wip.shape().blue(),
                                variant.name.yellow(),
                            );
                            wip = wip.variant(index).map_err(|e| self.reflect_err(e))?;

                            // Let's see what's in the variant — if it's tuple-like with only one field, we want to push field 0
                            if matches!(variant.data.kind, StructKind::Tuple)
                                && variant.data.fields.len() == 1
                            {
                                trace!(
                                    "Tuple variant {}::{} encountered, pushing field 0",
                                    wip.shape().blue(),
                                    variant.name.yellow()
                                );
                                wip = wip.field(0).map_err(|e| self.reflect_err(e))?;
                                self.stack.push(Instruction::Pop(PopReason::ObjectVal));
                            }

                            needs_pop = false;
                        }
                        None => {
                            if let Some(_variant_index) = wip.selected_variant() {
                                trace!(
                                    "Already have a variant selected, treating {} as struct field of {}::{}",
                                    key,
                                    wip.shape().blue(),
                                    wip.selected_variant().unwrap().name.yellow(),
                                );
                                // Try to find the field index of the key within the selected variant
                                if let Some(index) = wip.field_index(&key) {
                                    trace!("Found field {} in selected variant", key.blue());
                                    wip = wip.field(index).map_err(|e| self.reflect_err(e))?;
                                } else if wip.shape().has_deny_unknown_fields_attr() {
                                    trace!("Unknown field in variant and denying unknown fields");
                                    return Err(self.err(DeserErrorKind::UnknownField {
                                        field_name: key.to_string(),
                                        shape: wip.shape(),
                                    }));
                                } else {
                                    trace!(
                                        "Ignoring unknown field '{}' in variant '{}::{}'",
                                        key,
                                        wip.shape(),
                                        wip.selected_variant().unwrap().name
                                    );
                                    ignore = true;
                                }
                            } else {
                                return Err(self.err(DeserErrorKind::NoSuchVariant {
                                    name: key.to_string(),
                                    enum_shape: wip.shape(),
                                }));
                            }
                        }
                    },
                    _ => {
                        // Check if it's a map
                        if let Def::Map(_) = shape.def {
                            wip = wip.push_map_key().map_err(|e| self.reflect_err(e))?;
                            wip = wip.put(key.to_string()).map_err(|e| self.reflect_err(e))?;
                            wip = wip.push_map_value().map_err(|e| self.reflect_err(e))?;
                        } else {
                            return Err(self.err(DeserErrorKind::Unimplemented(
                                "object key for non-struct/map",
                            )));
                        }
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
            _ => Err(self.err(DeserErrorKind::UnexpectedOutcome {
                got: outcome.node.into_owned(),
                wanted: "scalar or object close",
            })),
        }
    }

    fn list_item_or_list_close<'facet>(
        &mut self,
        mut wip: Wip<'facet, 'shape>,
        outcome: Spanned<Outcome<'input>, C>,
    ) -> Result<Wip<'facet, 'shape>, DeserError<'input, 'shape, C>>
    where
        'input: 'facet,
    {
        match outcome.node {
            Outcome::ListEnded => {
                trace!("List close");
                Ok(wip)
            }
            _ => {
                self.stack.push(Instruction::ListItemOrListClose);
                self.stack.push(Instruction::Pop(PopReason::ListVal));

                trace!(
                    "Expecting list item, doing a little push before doing value with outcome {}",
                    outcome.magenta()
                );
                trace!("Before push, wip.shape is {}", wip.shape().blue());

                // Special handling for tuples - we need to identify if we're in a tuple context
                let is_tuple = matches!(
                    wip.innermost_shape().ty,
                    Type::Sequence(SequenceType::Tuple(_))
                );

                if is_tuple {
                    trace!("Handling list item for a tuple type");
                    // For tuples, we need to use field-based access by index
                    wip = wip.push().map_err(|e| self.reflect_err(e))?;
                } else {
                    // Standard list/array handling
                    wip = wip.push().map_err(|e| self.reflect_err(e))?;
                }

                trace!(" After push, wip.shape is {}", wip.shape().cyan());
                wip = self.value(wip, outcome)?;
                Ok(wip)
            }
        }
    }
}
