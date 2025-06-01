#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::std_instead_of_alloc)]
#![forbid(unsafe_code)]
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
    Characteristic, Def, Facet, FieldFlags, PointerType, ScalarAffinity, StructKind, Type, UserType,
};
use owo_colors::OwoColorize;
pub use span::*;

use facet_reflect::{HeapValue, Partial, ReflectError};
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
    /// Resegmenting input into subspans.
    Resegmented(Vec<Subspan>),
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
            Outcome::Resegmented(_) => write!(f, "resegment"),
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
            Outcome::Resegmented(subspans) => {
                let owned_subspans = subspans
                    .into_iter()
                    .map(|s| Subspan {
                        offset: s.offset,
                        len: s.len,
                        meta: s.meta,
                    })
                    .collect();
                Outcome::Resegmented(owned_subspans)
            }
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
    pub wip: Partial<'facet, 'shape>,
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

    /// Access the substack
    pub fn substack(&self) -> &Substack<C> {
        &self.runner.substack
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
    type SpanType: Debug + SubstackBehavior + 'static;

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
        if self.start >= input.len() {
            // start points past the end of the args;
            // use byte offset = total length of whole input minus 1, len = 1
            let mut total_len = 0;
            for (i, arg) in input.iter().enumerate() {
                total_len += arg.len();
                if i < input.len() - 1 {
                    total_len += 1; // space after each arg except last
                }
            }
            return Span::<Cooked>::new(total_len.saturating_sub(1), 1);
        }

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
    /// Triggers clearing a substack.
    SubstackClose,
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
    /// Ending a smart pointer (ie. wrapping a `T` back into a `Box<T>`, or `Arc<T>` etc.)
    SmartPointer,
    /// Ending a wrapper value such as a newtype
    Wrapper,
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
            let wip = match Partial::alloc_shape(T::SHAPE) {
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
    mut wip: Partial<'facet, 'shape>,
    input: &'input F::Input<'input>,
    format: &mut F,
) -> Result<HeapValue<'facet, 'shape>, DeserError<'input, 'shape, Cooked>>
where
    F: Format + 'shape,
    F::SpanType: SubstackBehavior,
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
        substack: Substack::new(),
        last_span: Span::new(0, 0),
        format_source: format.source(),
        array_indices: Vec::new(),
        enum_tuple_field_count: None,
        enum_tuple_current_field: None,
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
            if F::SpanType::USES_SUBSTACK {
                if !$runner.substack.get().is_empty() {
                    trace!("Substack: {}", "carried".cyan());
                } else {
                    trace!("Substack: {}", "-".red());
                }
            }
            $runner.last_span = outcome.span;
            if F::SpanType::USES_SUBSTACK {
                if let Outcome::Resegmented(subspans) = &outcome.node {
                    $runner.substack = subspans.clone().into();
                }
            }
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
        // Note: frames_count() is no longer available in the new Partial API
        // This was used for debugging/assertions only

        let insn = match runner.stack.pop() {
            Some(insn) => insn,
            None => unreachable!("Instruction stack is empty"),
        };

        trace!("Instruction {:?}", insn.bright_red());

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
                    // Exit all nested frames (e.g., from flattened fields) before building
                    while wip.frame_count() > 1 {
                        wip.end().map_err(|e| {
                            let reflect_error = runner.reflect_err(e);
                            DeserError {
                                input: reflect_error.input,
                                span: reflect_error.span.to_cooked(format, input),
                                kind: reflect_error.kind,
                                source_id: reflect_error.source_id,
                            }
                        })?;
                    }

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
                    wip.end().map_err(|e| {
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
            Instruction::SubstackClose => {
                runner.substack.clear();
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

/// Helper function to check if an f64 has no fractional part
/// This is needed for no-std compatibility where f64::fract() is not available
#[inline]
fn has_no_fractional_part(value: f64) -> bool {
    value == (value as i64) as f64
}

/// Trait for numeric type conversions
trait NumericConvert: Sized {
    const TYPE_NAME: &'static str;

    fn to_i8(self) -> Option<i8>;
    fn to_i16(self) -> Option<i16>;
    fn to_i32(self) -> Option<i32>;
    fn to_i64(self) -> Option<i64>;
    fn to_i128(self) -> Option<i128>;
    fn to_isize(self) -> Option<isize>;

    fn to_u8(self) -> Option<u8>;
    fn to_u16(self) -> Option<u16>;
    fn to_u32(self) -> Option<u32>;
    fn to_u64(self) -> Option<u64>;
    fn to_u128(self) -> Option<u128>;
    fn to_usize(self) -> Option<usize>;

    fn to_f32(self) -> Option<f32>;
    fn to_f64(self) -> Option<f64>;
}

impl NumericConvert for u64 {
    const TYPE_NAME: &'static str = "u64";

    fn to_i8(self) -> Option<i8> {
        self.try_into().ok()
    }
    fn to_i16(self) -> Option<i16> {
        self.try_into().ok()
    }
    fn to_i32(self) -> Option<i32> {
        self.try_into().ok()
    }
    fn to_i64(self) -> Option<i64> {
        self.try_into().ok()
    }
    fn to_i128(self) -> Option<i128> {
        Some(self as i128)
    }
    fn to_isize(self) -> Option<isize> {
        self.try_into().ok()
    }

    fn to_u8(self) -> Option<u8> {
        self.try_into().ok()
    }
    fn to_u16(self) -> Option<u16> {
        self.try_into().ok()
    }
    fn to_u32(self) -> Option<u32> {
        self.try_into().ok()
    }
    fn to_u64(self) -> Option<u64> {
        Some(self)
    }
    fn to_u128(self) -> Option<u128> {
        Some(self as u128)
    }
    fn to_usize(self) -> Option<usize> {
        self.try_into().ok()
    }

    fn to_f32(self) -> Option<f32> {
        Some(self as f32)
    }
    fn to_f64(self) -> Option<f64> {
        Some(self as f64)
    }
}

impl NumericConvert for i64 {
    const TYPE_NAME: &'static str = "i64";

    fn to_i8(self) -> Option<i8> {
        self.try_into().ok()
    }
    fn to_i16(self) -> Option<i16> {
        self.try_into().ok()
    }
    fn to_i32(self) -> Option<i32> {
        self.try_into().ok()
    }
    fn to_i64(self) -> Option<i64> {
        Some(self)
    }
    fn to_i128(self) -> Option<i128> {
        Some(self as i128)
    }
    fn to_isize(self) -> Option<isize> {
        self.try_into().ok()
    }

    fn to_u8(self) -> Option<u8> {
        self.try_into().ok()
    }
    fn to_u16(self) -> Option<u16> {
        self.try_into().ok()
    }
    fn to_u32(self) -> Option<u32> {
        self.try_into().ok()
    }
    fn to_u64(self) -> Option<u64> {
        self.try_into().ok()
    }
    fn to_u128(self) -> Option<u128> {
        self.try_into().ok()
    }
    fn to_usize(self) -> Option<usize> {
        self.try_into().ok()
    }

    fn to_f32(self) -> Option<f32> {
        Some(self as f32)
    }
    fn to_f64(self) -> Option<f64> {
        Some(self as f64)
    }
}

impl NumericConvert for f64 {
    const TYPE_NAME: &'static str = "f64";

    fn to_i8(self) -> Option<i8> {
        if has_no_fractional_part(self) && self >= i8::MIN as f64 && self <= i8::MAX as f64 {
            Some(self as i8)
        } else {
            None
        }
    }
    fn to_i16(self) -> Option<i16> {
        if has_no_fractional_part(self) && self >= i16::MIN as f64 && self <= i16::MAX as f64 {
            Some(self as i16)
        } else {
            None
        }
    }
    fn to_i32(self) -> Option<i32> {
        if has_no_fractional_part(self) && self >= i32::MIN as f64 && self <= i32::MAX as f64 {
            Some(self as i32)
        } else {
            None
        }
    }
    fn to_i64(self) -> Option<i64> {
        if has_no_fractional_part(self) && self >= i64::MIN as f64 && self <= i64::MAX as f64 {
            Some(self as i64)
        } else {
            None
        }
    }
    fn to_i128(self) -> Option<i128> {
        if has_no_fractional_part(self) && self >= i128::MIN as f64 && self <= i128::MAX as f64 {
            Some(self as i128)
        } else {
            None
        }
    }
    fn to_isize(self) -> Option<isize> {
        if has_no_fractional_part(self) && self >= isize::MIN as f64 && self <= isize::MAX as f64 {
            Some(self as isize)
        } else {
            None
        }
    }

    fn to_u8(self) -> Option<u8> {
        if has_no_fractional_part(self) && self >= 0.0 && self <= u8::MAX as f64 {
            Some(self as u8)
        } else {
            None
        }
    }
    fn to_u16(self) -> Option<u16> {
        if has_no_fractional_part(self) && self >= 0.0 && self <= u16::MAX as f64 {
            Some(self as u16)
        } else {
            None
        }
    }
    fn to_u32(self) -> Option<u32> {
        if has_no_fractional_part(self) && self >= 0.0 && self <= u32::MAX as f64 {
            Some(self as u32)
        } else {
            None
        }
    }
    fn to_u64(self) -> Option<u64> {
        if has_no_fractional_part(self) && self >= 0.0 && self <= u64::MAX as f64 {
            Some(self as u64)
        } else {
            None
        }
    }
    fn to_u128(self) -> Option<u128> {
        if has_no_fractional_part(self) && self >= 0.0 && self <= u128::MAX as f64 {
            Some(self as u128)
        } else {
            None
        }
    }
    fn to_usize(self) -> Option<usize> {
        if has_no_fractional_part(self) && self >= 0.0 && self <= usize::MAX as f64 {
            Some(self as usize)
        } else {
            None
        }
    }

    fn to_f32(self) -> Option<f32> {
        Some(self as f32)
    }
    fn to_f64(self) -> Option<f64> {
        Some(self)
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

    /// Subspan storage, if the format uses them.
    pub substack: Substack<C>,

    /// Span of the last processed token, for accurate error reporting.
    pub last_span: Span<C>,

    /// Format source identifier for error reporting
    pub format_source: &'static str,

    /// Array index tracking - maps depth to current index for arrays
    pub array_indices: Vec<usize>,

    /// Tuple variant field tracking - number of fields in current enum tuple variant
    pub enum_tuple_field_count: Option<usize>,

    /// Tuple variant field tracking - current field index being processed
    pub enum_tuple_current_field: Option<usize>,
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
        mut wip: Partial<'facet, 'shape>,
        reason: PopReason,
    ) -> Result<Partial<'facet, 'shape>, DeserError<'input, 'shape, C>> {
        trace!(
            "--- STACK has {:?} {}",
            self.stack.green(),
            "(POP)".bright_yellow()
        );
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
                            wip.begin_nth_field(index)
                                .map_err(|e| self.reflect_err(e))?;

                            // Check for field-level default function first, then type-level default
                            if let Some(field_default_fn) = field.vtable.default_fn {
                                wip.set_field_default(field_default_fn)
                                    .map_err(|e| self.reflect_err(e))?;
                                trace!(
                                    "Field #{} {} @ {} was set to default value (via field default function)",
                                    index.yellow(),
                                    field.name.green(),
                                    field.offset.blue(),
                                );
                            } else if field.shape().is(Characteristic::Default) {
                                wip.set_default().map_err(|e| self.reflect_err(e))?;
                                trace!(
                                    "Field #{} {} @ {} was set to default value (via type default impl)",
                                    index.yellow(),
                                    field.name.green(),
                                    field.offset.blue(),
                                );
                            } else {
                                return Err(self.reflect_err(
                                    ReflectError::DefaultAttrButNoDefaultImpl {
                                        shape: field.shape(),
                                    },
                                ));
                            }
                            wip.end().map_err(|e| self.reflect_err(e))?;
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

                if has_unset {
                    if container_shape.has_default_attr() {
                        // let's allocate and build a default value
                        let default_val = Partial::alloc_shape(container_shape)
                            .map_err(|e| self.reflect_err(e))?
                            .set_default()
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
                                trace!(
                                    "Field #{} {} @ {} is being set to default value (from default instance)",
                                    index.yellow(),
                                    field.name.green(),
                                    field.offset.blue(),
                                );
                                wip.begin_nth_field(index)
                                    .map_err(|e| self.reflect_err(e))?;
                                // Get the field as a Peek from the default value
                                let def_field = peek.field(index).unwrap();
                                wip.set_from_peek(&def_field)
                                    .map_err(|e| self.reflect_err(e))?;
                                wip.end().map_err(|e| self.reflect_err(e))?;
                            }
                        }
                    } else {
                        // Find the first uninitialized field to report in the error
                        for (index, field) in sd.fields.iter().enumerate() {
                            let is_set = wip.is_field_set(index).map_err(|err| {
                                trace!("Error checking field set status: {:?}", err);
                                self.reflect_err(err)
                            })?;
                            if !is_set {
                                return Err(self.reflect_err(ReflectError::UninitializedField {
                                    shape: container_shape,
                                    field_name: field.name,
                                }));
                            }
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
                                    wip.begin_nth_field(index)
                                        .map_err(|e| self.reflect_err(e))?;

                                    // Check for field-level default function first, then type-level default
                                    if let Some(field_default_fn) = field.vtable.default_fn {
                                        wip.set_field_default(field_default_fn)
                                            .map_err(|e| self.reflect_err(e))?;
                                        trace!(
                                            "Field #{} @ {} in variant {} was set to default value (via field default function)",
                                            index.yellow(),
                                            field.offset.blue(),
                                            variant.name
                                        );
                                    } else if field.shape().is(Characteristic::Default) {
                                        wip.set_default().map_err(|e| self.reflect_err(e))?;
                                        trace!(
                                            "Field #{} @ {} in variant {} was set to default value (via type default impl)",
                                            index.yellow(),
                                            field.offset.blue(),
                                            variant.name
                                        );
                                    } else {
                                        return Err(self.reflect_err(
                                            ReflectError::DefaultAttrButNoDefaultImpl {
                                                shape: field.shape(),
                                            },
                                        ));
                                    }
                                    wip.end().map_err(|e| self.reflect_err(e))?;
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

                        if has_unset {
                            if container_shape.has_default_attr() {
                                trace!(
                                    "Enum has DEFAULT attr but variant has uninitialized fields"
                                );
                                // Handle similar to struct, allocate and build default value for variant
                                let default_val = Partial::alloc_shape(container_shape)
                                    .map_err(|e| self.reflect_err(e))?
                                    .set_default()
                                    .map_err(|e| self.reflect_err(e))?
                                    .build()
                                    .map_err(|e| self.reflect_err(e))?;

                                let peek = default_val.peek();
                                let peek_enum =
                                    peek.into_enum().map_err(|e| self.reflect_err(e))?;
                                let default_variant = peek_enum
                                    .active_variant()
                                    .map_err(|e| self.err(DeserErrorKind::VariantError(e)))?;

                                if default_variant == &variant {
                                    // It's the same variant, fill in the missing fields
                                    for (index, _field) in variant.data.fields.iter().enumerate() {
                                        let is_set = wip.is_field_set(index).map_err(|err| {
                                            trace!("Error checking field set status: {:?}", err);
                                            self.reflect_err(err)
                                        })?;
                                        if !is_set {
                                            if let Ok(Some(def_field)) = peek_enum.field(index) {
                                                wip.begin_nth_field(index)
                                                    .map_err(|e| self.reflect_err(e))?;
                                                wip.set_from_peek(&def_field)
                                                    .map_err(|e| self.reflect_err(e))?;
                                                wip.end().map_err(|e| self.reflect_err(e))?;
                                            }
                                        }
                                    }
                                }
                            } else {
                                // Find the first uninitialized field to report in the error
                                for (index, field) in variant.data.fields.iter().enumerate() {
                                    let is_set = wip.is_field_set(index).map_err(|err| {
                                        trace!("Error checking field set status: {:?}", err);
                                        self.reflect_err(err)
                                    })?;
                                    if !is_set {
                                        return Err(self.reflect_err(
                                            ReflectError::UninitializedEnumField {
                                                shape: container_shape,
                                                variant_name: variant.name,
                                                field_name: field.name,
                                            },
                                        ));
                                    }
                                }
                            }
                        }
                    }
                } else if container_shape.has_default_attr() {
                    // No variant selected, but enum has default attribute - set to default
                    trace!("No variant selected but enum has DEFAULT attr; setting to default");
                    let default_val = Partial::alloc_shape(container_shape)
                        .map_err(|e| self.reflect_err(e))?
                        .set_default()
                        .map_err(|e| self.reflect_err(e))?
                        .build()
                        .map_err(|e| self.reflect_err(e))?;

                    let peek = default_val.peek();
                    let peek_enum = peek.into_enum().map_err(|e| self.reflect_err(e))?;
                    let default_variant_idx = peek_enum
                        .variant_index()
                        .map_err(|e| self.err(DeserErrorKind::VariantError(e)))?;

                    // Select the default variant
                    wip.select_nth_variant(default_variant_idx)
                        .map_err(|e| self.reflect_err(e))?;

                    // Copy all fields from default value
                    let variant = &ed.variants[default_variant_idx];
                    for (index, _field) in variant.data.fields.iter().enumerate() {
                        if let Ok(Some(def_field)) = peek_enum.field(index) {
                            wip.begin_nth_field(index)
                                .map_err(|e| self.reflect_err(e))?;
                            wip.set_from_peek(&def_field)
                                .map_err(|e| self.reflect_err(e))?;
                            wip.end().map_err(|e| self.reflect_err(e))?;
                        }
                    }
                }
            }
            _ => {
                trace!(
                    "Thing being popped is not a container I guess (it's a {}, innermost is {})",
                    wip.shape(),
                    wip.innermost_shape()
                );
            }
        }
        Ok(wip)
    }

    /// Internal common handler for GotScalar outcome, to deduplicate code.
    /// Helper to set numeric values with type conversion
    fn set_numeric_value<'facet, N>(
        &self,
        wip: &mut Partial<'facet, 'shape>,
        value: N,
    ) -> Result<(), DeserError<'input, 'shape, C>>
    where
        'input: 'facet,
        N: NumericConvert,
    {
        let shape = wip.innermost_shape();

        // Check if this is a numeric scalar
        if let Def::Scalar(sd) = shape.def {
            if let ScalarAffinity::Number(num_affinity) = sd.affinity {
                use facet_core::{IntegerSize, NumberBits, Signedness};

                // Helper closure to convert and set numeric value
                macro_rules! convert_and_set {
                    ($converter:expr, $target_type:expr) => {{
                        let converted = $converter(value).ok_or_else(|| {
                            self.err(DeserErrorKind::NumericConversion {
                                from: N::TYPE_NAME,
                                to: $target_type,
                            })
                        })?;
                        wip.set(converted).map_err(|e| self.reflect_err(e))?;
                    }};
                }

                // Check if it's integer or float based on the bits type
                match num_affinity.bits {
                    NumberBits::Integer { size, sign } => {
                        // Integer type - check signed/unsigned and size
                        match (size, sign) {
                            (IntegerSize::Fixed(bits), Signedness::Signed) => match bits {
                                8 => convert_and_set!(N::to_i8, "i8"),
                                16 => convert_and_set!(N::to_i16, "i16"),
                                32 => convert_and_set!(N::to_i32, "i32"),
                                64 => convert_and_set!(N::to_i64, "i64"),
                                128 => convert_and_set!(N::to_i128, "i128"),
                                _ => {
                                    return Err(self.err(DeserErrorKind::NumericConversion {
                                        from: N::TYPE_NAME,
                                        to: "unknown fixed-size signed integer",
                                    }));
                                }
                            },
                            (IntegerSize::Fixed(bits), Signedness::Unsigned) => match bits {
                                8 => convert_and_set!(N::to_u8, "u8"),
                                16 => convert_and_set!(N::to_u16, "u16"),
                                32 => convert_and_set!(N::to_u32, "u32"),
                                64 => convert_and_set!(N::to_u64, "u64"),
                                128 => convert_and_set!(N::to_u128, "u128"),
                                _ => {
                                    return Err(self.err(DeserErrorKind::NumericConversion {
                                        from: N::TYPE_NAME,
                                        to: "unknown fixed-size unsigned integer",
                                    }));
                                }
                            },
                            (IntegerSize::PointerSized, Signedness::Signed) => {
                                convert_and_set!(N::to_isize, "isize")
                            }
                            (IntegerSize::PointerSized, Signedness::Unsigned) => {
                                convert_and_set!(N::to_usize, "usize")
                            }
                        }
                    }
                    NumberBits::Float {
                        sign_bits,
                        exponent_bits,
                        mantissa_bits,
                        ..
                    } => {
                        // Floating point - calculate total bits
                        let total_bits = sign_bits + exponent_bits + mantissa_bits;
                        match total_bits {
                            32 => convert_and_set!(N::to_f32, "f32"),
                            64 => convert_and_set!(N::to_f64, "f64"),
                            _ => {
                                // Unknown float size
                                return Err(self.err(DeserErrorKind::NumericConversion {
                                    from: N::TYPE_NAME,
                                    to: "unknown float size",
                                }));
                            }
                        }
                    }
                    _ => {
                        // Fixed-point, Decimal, or other numeric types not supported
                        return Err(self.err(DeserErrorKind::NumericConversion {
                            from: N::TYPE_NAME,
                            to: "fixed-point or decimal",
                        }));
                    }
                }
            } else {
                // Not a number affinity - cannot convert
                return Err(self.err(DeserErrorKind::UnsupportedType {
                    got: shape,
                    wanted: "numeric type",
                }));
            }
        } else {
            // Not a scalar def - cannot convert
            return Err(self.err(DeserErrorKind::UnsupportedType {
                got: shape,
                wanted: "scalar type",
            }));
        }

        Ok(())
    }

    fn handle_scalar<'facet>(
        &self,
        wip: &mut Partial<'facet, 'shape>,
        scalar: Scalar<'input>,
    ) -> Result<(), DeserError<'input, 'shape, C>>
    where
        'input: 'facet, // 'input outlives 'facet
    {
        match scalar {
            Scalar::String(cow) => {
                match wip.innermost_shape().ty {
                    Type::User(UserType::Enum(_)) => {
                        if wip.selected_variant().is_some() {
                            // If we already have a variant selected, just put the string
                            wip.set(cow).map_err(|e| self.reflect_err(e))?;
                        } else {
                            // Try to select the variant
                            match wip.find_variant(&cow) {
                                Some((variant_index, _)) => {
                                    wip.select_nth_variant(variant_index)
                                        .map_err(|e| self.reflect_err(e))?;
                                }
                                None => {
                                    return Err(self.err(DeserErrorKind::NoSuchVariant {
                                        name: cow.to_string(),
                                        enum_shape: wip.innermost_shape(),
                                    }));
                                }
                            }
                        }
                    }
                    Type::Pointer(PointerType::Reference(_))
                        if wip.innermost_shape().is_type::<&str>() =>
                    {
                        // This is for handling the &str type
                        // The Cow may be Borrowed (we may have an owned string but need a &str)
                        match cow {
                            Cow::Borrowed(s) => wip.set(s).map_err(|e| self.reflect_err(e))?,
                            Cow::Owned(s) => wip.set(s).map_err(|e| self.reflect_err(e))?,
                        }; // Add semicolon to ignore the return value
                    }
                    _ => {
                        // Check if this is a scalar type that can be parsed from a string
                        let shape = wip.innermost_shape();
                        if let Def::Scalar(scalar_def) = shape.def {
                            // Check if this is a type that expects to be parsed from string
                            // (like IpAddr, UUID, Path, etc.)
                            if !matches!(scalar_def.affinity, facet_core::ScalarAffinity::String(_))
                            {
                                // Try parse_from_str for non-string scalar types
                                match wip.parse_from_str(cow.as_ref()) {
                                    Ok(_) => {
                                        // Successfully parsed
                                    }
                                    Err(parse_err) => {
                                        // Parsing failed - check if it's because parse isn't supported
                                        // or if parsing actually failed
                                        match parse_err {
                                            ReflectError::OperationFailed {
                                                shape: _,
                                                operation,
                                            } if operation.contains("does not support parsing") => {
                                                // Type doesn't have a parse function, try direct conversion
                                                wip.set(cow.to_string())
                                                    .map_err(|e| self.reflect_err(e))?;
                                            }
                                            _ => {
                                                // Actual parsing failure
                                                return Err(self.err(DeserErrorKind::ReflectError(
                                                    ReflectError::OperationFailed {
                                                        shape,
                                                        operation: "Failed to parse string value",
                                                    }
                                                )));
                                            }
                                        }
                                    }
                                }
                            } else {
                                // It's a string type, set directly
                                wip.set(cow.to_string()).map_err(|e| self.reflect_err(e))?;
                            }
                        } else {
                            // Not a scalar, just set as String
                            wip.set(cow.to_string()).map_err(|e| self.reflect_err(e))?;
                        }
                    }
                }
            }
            Scalar::U64(value) => {
                self.set_numeric_value(wip, value)?;
            }
            Scalar::I64(value) => {
                self.set_numeric_value(wip, value)?;
            }
            Scalar::F64(value) => {
                self.set_numeric_value(wip, value)?;
            }
            Scalar::Bool(value) => {
                wip.set(value).map_err(|e| self.reflect_err(e))?;
            }
            Scalar::Null => {
                wip.set_default().map_err(|e| self.reflect_err(e))?;
            }
        }
        Ok(())
    }

    /// Handle value parsing
    fn value<'facet>(
        &mut self,
        mut wip: Partial<'facet, 'shape>,
        outcome: Spanned<Outcome<'input>, C>,
    ) -> Result<Partial<'facet, 'shape>, DeserError<'input, 'shape, C>>
    where
        'input: 'facet, // 'input must outlive 'facet
    {
        trace!(
            "--- STACK has {:?} {}",
            self.stack.green(),
            "(VALUE)".bright_yellow()
        );

        let original_shape = wip.shape();
        trace!("Handling value of type {}", original_shape.blue());

        // Handle null values
        if matches!(outcome.node, Outcome::Scalar(Scalar::Null)) {
            wip.set_default().map_err(|e| self.reflect_err(e))?;
            return Ok(wip);
        }

        // Resolve the innermost value to deserialize
        loop {
            if matches!(wip.shape().def, Def::Option(_)) {
                trace!("  Starting Some(_) option for {}", wip.shape().blue());
                wip.push_some().map_err(|e| self.reflect_err(e))?;
                self.stack.push(Instruction::Pop(PopReason::Some));
            } else if let Def::SmartPointer(inner) = wip.shape().def {
                if let Some(pointee) = inner.pointee() {
                    trace!(
                        "  Starting smart pointer for {} (pointee is {})",
                        wip.shape().blue(),
                        pointee.yellow(),
                    );
                } else {
                    trace!(
                        "  Starting smart pointer for {} (no pointee)",
                        wip.shape().blue()
                    );
                }
                wip.push_pointee().map_err(|e| self.reflect_err(e))?;
                self.stack.push(Instruction::Pop(PopReason::SmartPointer));
            } else if let Some(inner_fn) = wip.shape().inner {
                let inner = inner_fn();
                trace!(
                    "  Starting wrapped value for {} (inner is {})",
                    wip.shape().blue(),
                    inner.yellow()
                );
                wip.push_inner().map_err(|e| self.reflect_err(e))?;
                self.stack.push(Instruction::Pop(PopReason::Wrapper));
            } else {
                break;
            }
        }

        if wip.shape() != original_shape {
            trace!(
                "Handling shape {} as innermost {}",
                original_shape.blue(),
                wip.shape().yellow()
            );
        }

        match outcome.node {
            Outcome::Scalar(s) => {
                trace!("Parsed scalar value: {}", s.cyan());
                self.handle_scalar(&mut wip, s)?;
            }
            Outcome::ListStarted => {
                let shape = wip.innermost_shape();

                // First check if this is a tuple struct (including empty tuples)
                if let Type::User(UserType::Struct(st)) = shape.ty {
                    if st.kind == StructKind::Tuple {
                        trace!(
                            "Array starting for tuple struct ({}) with {} fields!",
                            shape.blue(),
                            st.fields.len()
                        );

                        // Non-empty tuples need to process list events
                        trace!("Beginning pushback");
                        self.stack.push(Instruction::ListItemOrListClose);
                        return Ok(wip);
                    }
                }

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
                        wip.set_default().map_err(|e| self.reflect_err(e))?;
                    }
                    _ => {
                        // For non-collection types, check the Type enum
                        if let Type::User(user_ty) = shape.ty {
                            match user_ty {
                                UserType::Enum(_) => {
                                    trace!("Array starting for enum ({})!", shape.blue());
                                    // Check if we have a tuple variant selected
                                    if let Some(variant) = wip.selected_variant() {
                                        use facet_core::StructKind;
                                        if variant.data.kind == StructKind::Tuple {
                                            // For tuple variants, we'll handle array elements as tuple fields
                                            // Initialize tuple field tracking
                                            self.enum_tuple_field_count =
                                                Some(variant.data.fields.len());
                                            self.enum_tuple_current_field = Some(0);
                                        } else {
                                            return Err(self.err(DeserErrorKind::UnsupportedType {
                                                got: shape,
                                                wanted: "tuple variant for array deserialization",
                                            }));
                                        }
                                    } else {
                                        return Err(self.err(DeserErrorKind::UnsupportedType {
                                            got: shape,
                                            wanted: "enum with variant selected",
                                        }));
                                    }
                                }
                                UserType::Struct(_) => {
                                    // Regular struct shouldn't be parsed from array
                                    // (Tuples are already handled above)
                                    return Err(self.err(DeserErrorKind::UnsupportedType {
                                        got: shape,
                                        wanted: "array, list, tuple, or slice",
                                    }));
                                }
                                _ => {
                                    return Err(self.err(DeserErrorKind::UnsupportedType {
                                        got: shape,
                                        wanted: "array, list, tuple, or slice",
                                    }));
                                }
                            }
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

                // Only call begin_list() for actual lists, not arrays
                match shape.def {
                    Def::List(_) => {
                        wip.begin_list().map_err(|e| self.reflect_err(e))?;
                    }
                    Def::Array(_) => {
                        // Arrays don't need begin_list()
                        // Initialize index tracking for this array
                        self.array_indices.push(0);
                    }
                    Def::Slice(_) => {
                        // Slices don't need begin_list()
                        // They will be populated element by element
                    }
                    _ => {
                        // For other types like tuples, no special initialization needed
                    }
                }
            }
            Outcome::ListEnded => {
                trace!("List closing");
                // Clean up array index tracking if this was an array
                let shape = wip.shape();
                if matches!(shape.def, Def::Array(_)) {
                    self.array_indices.pop();
                }
                wip.end().map_err(|e| self.reflect_err(e))?;
            }
            Outcome::ObjectStarted => {
                let shape = wip.shape();
                match shape.def {
                    Def::Map(_md) => {
                        trace!("Object starting for map value ({})!", shape.blue());
                        wip.begin_map().map_err(|e| self.reflect_err(e))?;
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
                        } else if let Type::User(UserType::Struct(struct_type)) = shape.ty {
                            if struct_type.kind == StructKind::Tuple {
                                // This could be a tuple that was serialized as an object
                                // Despite this being unusual, we'll handle it here for robustness
                                trace!(
                                    "Object starting for tuple ({}) with {} fields - unusual but handling",
                                    shape.blue(),
                                    struct_type.fields.len()
                                );
                                // Tuples are treated as structs
                            }
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
            Outcome::Resegmented(subspans) => {
                trace!("Resegmented with {} subspans (value)", subspans.len());
                // Push an instruction to process the current argument again
                // (but this time it will use the subspan from the substack)
                // self.stack.push(Instruction::ObjectKeyOrObjectClose);
                // 1) Go back to expecting another value
                // self.stack.push(Instruction::Pop(PopReason::ObjectVal));
                // self.stack.push(Instruction::Value(ValueReason::ObjectVal));
            }
            Outcome::ObjectEnded => todo!(),
        }
        Ok(wip)
    }

    fn object_key_or_object_close<'facet>(
        &mut self,
        mut wip: Partial<'facet, 'shape>,
        outcome: Spanned<Outcome<'input>, C>,
    ) -> Result<Partial<'facet, 'shape>, DeserError<'input, 'shape, C>>
    where
        'input: 'facet,
    {
        trace!(
            "STACK: {:?} {}",
            self.stack.green(),
            "(OK/OC)".bright_yellow()
        );
        trace!("SUBSTACK: {:?}", self.substack.get().bright_green());
        match outcome.node {
            Outcome::Scalar(Scalar::String(key)) => {
                trace!("Parsed object key: {}", key.cyan());

                let mut ignore = false;
                let mut needs_pop = true;
                let mut handled_by_flatten = false;
                let has_substack = !self.substack.get().is_empty();

                let shape = wip.innermost_shape();
                match shape.ty {
                    Type::User(UserType::Struct(sd)) => {
                        // First try to find a direct field match
                        if let Some(index) = wip.field_index(&key) {
                            trace!("It's a struct field");
                            wip.begin_nth_field(index)
                                .map_err(|e| self.reflect_err(e))?;
                        } else {
                            trace!(
                                "Did not find direct field match in innermost shape {}",
                                shape.blue()
                            );

                            // Check for flattened fields
                            let mut found_in_flatten = false;
                            for (index, field) in sd.fields.iter().enumerate() {
                                if field.flags.contains(FieldFlags::FLATTEN) {
                                    trace!("Found flattened field #{}", index);
                                    // Enter the flattened field
                                    wip.begin_nth_field(index)
                                        .map_err(|e| self.reflect_err(e))?;

                                    // Check if this flattened field has the requested key
                                    if let Some(subfield_index) = wip.field_index(&key) {
                                        trace!("Found key {} in flattened field", key);
                                        wip.begin_nth_field(subfield_index)
                                            .map_err(|e| self.reflect_err(e))?;
                                        found_in_flatten = true;
                                        handled_by_flatten = true;
                                        break;
                                    } else if let Some((_variant_index, _variant)) =
                                        wip.find_variant(&key)
                                    {
                                        trace!("Found key {} in flattened field", key);
                                        wip.select_variant_named(&key)
                                            .map_err(|e| self.reflect_err(e))?;
                                        found_in_flatten = true;
                                        break;
                                    } else {
                                        // Key not in this flattened field, go back up
                                        wip.end().map_err(|e| self.reflect_err(e))?;
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
                            wip.select_nth_variant(index)
                                .map_err(|e| self.reflect_err(e))?;

                            // Let's see what's in the variant — if it's tuple-like with only one field, we want to push field 0
                            if matches!(variant.data.kind, StructKind::Tuple)
                                && variant.data.fields.len() == 1
                            {
                                trace!(
                                    "Tuple variant {}::{} encountered, pushing field 0",
                                    wip.shape().blue(),
                                    variant.name.yellow()
                                );
                                wip.begin_nth_field(0).map_err(|e| self.reflect_err(e))?;
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
                                    wip.begin_nth_field(index)
                                        .map_err(|e| self.reflect_err(e))?;
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
                            wip.push_map_key().map_err(|e| self.reflect_err(e))?;
                            wip.set(key.to_string()).map_err(|e| self.reflect_err(e))?;
                            wip.end().map_err(|e| self.reflect_err(e))?; // Complete the key frame
                            wip.push_map_value().map_err(|e| self.reflect_err(e))?;
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
                        if has_substack {
                            trace!("Pushing SubstackClose insn to stack");
                            self.stack.push(Instruction::SubstackClose);
                        }
                    } else if handled_by_flatten {
                        // For flattened fields, we only need one pop for the field itself.
                        // The flattened struct should remain active until the outer object is finished.
                        trace!("Pushing Pop insn to stack (ObjectVal) for flattened field");
                        self.stack.push(Instruction::Pop(PopReason::ObjectVal));
                        if has_substack {
                            trace!("Pushing SubstackClose insn to stack");
                            self.stack.push(Instruction::SubstackClose);
                        }
                    }
                    self.stack.push(Instruction::Value(ValueReason::ObjectVal));
                }
                Ok(wip)
            }
            Outcome::ObjectEnded => {
                trace!("Object closing");
                Ok(wip)
            }
            Outcome::Resegmented(subspans) => {
                trace!(
                    "Resegmented into {} subspans ({:?}) - obj. key/close",
                    subspans.len(),
                    subspans
                );
                // stay in the same state: parse another 'object key'
                self.stack.push(Instruction::ObjectKeyOrObjectClose);
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
        mut wip: Partial<'facet, 'shape>,
        outcome: Spanned<Outcome<'input>, C>,
    ) -> Result<Partial<'facet, 'shape>, DeserError<'input, 'shape, C>>
    where
        'input: 'facet,
    {
        trace!(
            "--- STACK has {:?} {}",
            self.stack.green(),
            "(LI/LC)".bright_yellow()
        );
        match outcome.node {
            Outcome::ListEnded => {
                trace!("List close");
                // Clean up array index tracking if this was an array
                let shape = wip.shape();
                if matches!(shape.def, Def::Array(_)) {
                    self.array_indices.pop();
                }

                // Clean up enum tuple variant tracking if this was an enum tuple
                if let Type::User(UserType::Enum(_)) = shape.ty {
                    if self.enum_tuple_field_count.is_some() {
                        trace!("Enum tuple variant list ended");
                        self.enum_tuple_field_count = None;
                        self.enum_tuple_current_field = None;
                    }
                }

                // Special case: if we're at an empty tuple, we've successfully parsed it
                if let Type::User(UserType::Struct(st)) = shape.ty {
                    if st.kind == StructKind::Tuple && st.fields.is_empty() {
                        trace!("Empty tuple parsed from []");
                        // The empty tuple is complete - no fields to initialize
                    }
                }

                // Don't end the list here - let the Pop instruction handle it
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

                // Different handling for arrays vs lists
                let shape = wip.shape();
                match shape.def {
                    Def::Array(ad) => {
                        // Arrays use the last index in our tracking vector
                        if let Some(current_index) = self.array_indices.last().copied() {
                            // Check bounds
                            if current_index >= ad.n {
                                return Err(self.err(DeserErrorKind::ArrayOverflow {
                                    shape,
                                    max_len: ad.n,
                                }));
                            }

                            // Set this array element
                            wip.begin_nth_element(current_index)
                                .map_err(|e| self.reflect_err(e))?;

                            // Increment the index for next time
                            if let Some(last) = self.array_indices.last_mut() {
                                *last += 1;
                            }
                        } else {
                            // This shouldn't happen if we properly initialize in ListStarted
                            return Err(self.err(DeserErrorKind::Unimplemented(
                                "Array index tracking not initialized",
                            )));
                        }
                    }
                    Def::List(_) => {
                        wip.begin_list_item().map_err(|e| self.reflect_err(e))?;
                    }
                    _ => {
                        // Check if this is an enum tuple variant
                        if let Type::User(UserType::Enum(_)) = shape.ty {
                            if let (Some(field_count), Some(current_field)) =
                                (self.enum_tuple_field_count, self.enum_tuple_current_field)
                            {
                                if current_field >= field_count {
                                    // Too many elements for this tuple variant
                                    return Err(self.err(DeserErrorKind::ArrayOverflow {
                                        shape,
                                        max_len: field_count,
                                    }));
                                }

                                // Process this tuple field
                                wip.begin_nth_enum_field(current_field)
                                    .map_err(|e| self.reflect_err(e))?;

                                // Advance to next field
                                self.enum_tuple_current_field = Some(current_field + 1);
                            } else {
                                return Err(self.err(DeserErrorKind::UnsupportedType {
                                    got: shape,
                                    wanted: "enum with tuple variant selected",
                                }));
                            }
                        }
                        // Check if this is a tuple
                        else if let Type::User(UserType::Struct(struct_type)) = shape.ty {
                            if struct_type.kind == StructKind::Tuple {
                                // Tuples use field indexing
                                // Find the next uninitialized field
                                let mut field_index = None;
                                for i in 0..struct_type.fields.len() {
                                    if !wip.is_field_set(i).map_err(|e| self.reflect_err(e))? {
                                        field_index = Some(i);
                                        break;
                                    }
                                }

                                if let Some(idx) = field_index {
                                    wip.begin_nth_field(idx).map_err(|e| self.reflect_err(e))?;
                                } else {
                                    // All fields are set, this is too many elements
                                    return Err(self.err(DeserErrorKind::ArrayOverflow {
                                        shape,
                                        max_len: struct_type.fields.len(),
                                    }));
                                }
                            } else {
                                // Not a tuple struct
                                return Err(self.err(DeserErrorKind::UnsupportedType {
                                    got: shape,
                                    wanted: "array, list, or tuple",
                                }));
                            }
                        } else {
                            // Not a struct type at all
                            return Err(self.err(DeserErrorKind::UnsupportedType {
                                got: shape,
                                wanted: "array, list, or tuple",
                            }));
                        }
                    }
                }

                trace!(" After push, wip.shape is {}", wip.shape().cyan());

                // Special handling: if we're now at an empty tuple and we see a list start,
                // we can handle the flexible coercion from []
                if matches!(outcome.node, Outcome::ListStarted) {
                    if let Type::User(UserType::Struct(st)) = wip.shape().ty {
                        if st.kind == StructKind::Tuple && st.fields.is_empty() {
                            trace!(
                                "Empty tuple field with list start - initializing empty tuple and expecting immediate close"
                            );
                            // Initialize the empty tuple with default value since it has no fields to fill
                            wip.set_default().map_err(|e| self.reflect_err(e))?;
                            // Continue processing - we still need to handle the list close
                        }
                    }
                }

                wip = self.value(wip, outcome)?;
                Ok(wip)
            }
        }
    }
}
