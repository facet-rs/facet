//! Coroutine-based deserialization infrastructure.
//!
//! This module provides a way to write deserializers in a natural imperative style
//! while keeping the parser-specific code separate. The inner deserialization logic
//! runs in a coroutine and yields when it needs parser operations. The wrapper
//! handles the parser operations and resumes the coroutine with the results.
//!
//! This dramatically reduces monomorphization: the inner logic is compiled once,
//! not once per parser type.

extern crate alloc;

use alloc::borrow::Cow;
use alloc::format;

use corosensei::stack::DefaultStack;
use corosensei::{CoroutineResult, ScopedCoroutine, Yielder};
use facet_reflect::{Partial, Span};

use crate::{
    DeserializeError, FormatDeserializer, FormatParser, InnerDeserializeError, ParseEvent,
};

/// Request from the inner deserialization logic to the wrapper.
pub(crate) enum DeserializeRequest<'input, const BORROW: bool> {
    /// Need to call `expect_event(expected)` and get the result.
    ExpectEvent { expected: &'static str },

    /// Need to call `expect_peek(expected)` and get the result.
    ExpectPeek { expected: &'static str },

    /// Need to call `parser.skip_value()`.
    SkipValue,

    /// Need to call `deserialize_into(wip)` recursively.
    DeserializeInto { wip: Partial<'input, BORROW> },

    /// Need to call `set_string_value(wip, s)`.
    SetStringValue {
        wip: Partial<'input, BORROW>,
        s: Cow<'input, str>,
    },

    /// Get the current span for error reporting.
    GetSpan,
}

/// Response from the wrapper to the inner deserialization logic.
pub(crate) enum DeserializeResponse<'input, const BORROW: bool> {
    /// Result of `expect_event` or `expect_peek`.
    Event(ParseEvent<'input>),

    /// Result of `skip_value` (success).
    Skipped,

    /// Result of `deserialize_into` or `set_string_value` (success).
    Wip(Partial<'input, BORROW>),

    /// Current span value.
    Span(Option<Span>),

    /// An error occurred.
    Error(InnerDeserializeError),
}

impl<'input, const BORROW: bool> DeserializeResponse<'input, BORROW> {
    /// Unwrap as an event, or return an error.
    pub fn into_event(self) -> Result<ParseEvent<'input>, InnerDeserializeError> {
        match self {
            DeserializeResponse::Event(e) => Ok(e),
            DeserializeResponse::Error(e) => Err(e),
            other => Err(InnerDeserializeError::Unsupported(format!(
                "expected Event response, got {:?}",
                core::mem::discriminant(&other)
            ))),
        }
    }

    /// Unwrap as a wip, or return an error.
    pub fn into_wip(self) -> Result<Partial<'input, BORROW>, InnerDeserializeError> {
        match self {
            DeserializeResponse::Wip(wip) => Ok(wip),
            DeserializeResponse::Error(e) => Err(e),
            other => Err(InnerDeserializeError::Unsupported(format!(
                "expected Wip response, got {:?}",
                core::mem::discriminant(&other)
            ))),
        }
    }

    /// Unwrap as skipped confirmation, or return an error.
    pub fn into_skipped(self) -> Result<(), InnerDeserializeError> {
        match self {
            DeserializeResponse::Skipped => Ok(()),
            DeserializeResponse::Error(e) => Err(e),
            other => Err(InnerDeserializeError::Unsupported(format!(
                "expected Skipped response, got {:?}",
                core::mem::discriminant(&other)
            ))),
        }
    }

    /// Unwrap as span, or return an error.
    pub fn into_span(self) -> Result<Option<Span>, InnerDeserializeError> {
        match self {
            DeserializeResponse::Span(s) => Ok(s),
            DeserializeResponse::Error(e) => Err(e),
            other => Err(InnerDeserializeError::Unsupported(format!(
                "expected Span response, got {:?}",
                core::mem::discriminant(&other)
            ))),
        }
    }
}

/// Type alias for the yielder used in deserialization coroutines.
pub(crate) type DeserializeYielder<'input, const BORROW: bool> =
    Yielder<DeserializeResponse<'input, BORROW>, DeserializeRequest<'input, BORROW>>;

/// Helper to request an event from the parser.
pub(crate) fn request_event<'input, const BORROW: bool>(
    yielder: &DeserializeYielder<'input, BORROW>,
    expected: &'static str,
) -> Result<ParseEvent<'input>, InnerDeserializeError> {
    yielder
        .suspend(DeserializeRequest::ExpectEvent { expected })
        .into_event()
}

/// Helper to peek at the next event.
pub(crate) fn request_peek<'input, const BORROW: bool>(
    yielder: &DeserializeYielder<'input, BORROW>,
    expected: &'static str,
) -> Result<ParseEvent<'input>, InnerDeserializeError> {
    yielder
        .suspend(DeserializeRequest::ExpectPeek { expected })
        .into_event()
}

/// Helper to skip a value.
pub(crate) fn request_skip<'input, const BORROW: bool>(
    yielder: &DeserializeYielder<'input, BORROW>,
) -> Result<(), InnerDeserializeError> {
    yielder
        .suspend(DeserializeRequest::SkipValue)
        .into_skipped()
}

/// Helper to recursively deserialize into a partial.
pub(crate) fn request_deserialize_into<'input, const BORROW: bool>(
    yielder: &DeserializeYielder<'input, BORROW>,
    wip: Partial<'input, BORROW>,
) -> Result<Partial<'input, BORROW>, InnerDeserializeError> {
    yielder
        .suspend(DeserializeRequest::DeserializeInto { wip })
        .into_wip()
}

/// Helper to set a string value.
pub(crate) fn request_set_string<'input, const BORROW: bool>(
    yielder: &DeserializeYielder<'input, BORROW>,
    wip: Partial<'input, BORROW>,
    s: Cow<'input, str>,
) -> Result<Partial<'input, BORROW>, InnerDeserializeError> {
    yielder
        .suspend(DeserializeRequest::SetStringValue { wip, s })
        .into_wip()
}

/// Helper to get the current span for error reporting.
pub(crate) fn request_span<'input, const BORROW: bool>(
    yielder: &DeserializeYielder<'input, BORROW>,
) -> Option<Span> {
    yielder
        .suspend(DeserializeRequest::GetSpan)
        .into_span()
        .unwrap_or(None)
}

/// Run a coroutine-based deserializer with the given inner function.
///
/// This is the generic wrapper that handles parser operations. The inner function
/// is non-generic over the parser type, reducing monomorphization.
pub(crate) fn run_deserialize_coro<'input, const BORROW: bool, P, F, R>(
    deser: &mut FormatDeserializer<'input, BORROW, P>,
    inner_fn: F,
) -> Result<R, DeserializeError<P::Error>>
where
    P: FormatParser<'input>,
    F: FnOnce(&DeserializeYielder<'input, BORROW>) -> Result<R, InnerDeserializeError>,
{
    // Create the coroutine with its own stack
    let stack = DefaultStack::new(64 * 1024).expect("failed to allocate coroutine stack");

    let coro: ScopedCoroutine<
        DeserializeResponse<'input, BORROW>,
        DeserializeRequest<'input, BORROW>,
        Result<R, InnerDeserializeError>,
        DefaultStack,
    > = ScopedCoroutine::with_stack(stack, move |yielder, _initial| inner_fn(yielder));

    coro.scope(|mut coro_ref| {
        // First resume with a dummy response to start the coroutine
        let mut result = coro_ref.as_mut().resume(DeserializeResponse::Skipped);

        loop {
            match result {
                CoroutineResult::Yield(request) => {
                    let response = match request {
                        DeserializeRequest::ExpectEvent { expected } => {
                            match deser.expect_event(expected) {
                                Ok(event) => DeserializeResponse::Event(event),
                                Err(e) => DeserializeResponse::Error(e.into_inner()),
                            }
                        }
                        DeserializeRequest::ExpectPeek { expected } => {
                            match deser.expect_peek(expected) {
                                Ok(event) => DeserializeResponse::Event(event),
                                Err(e) => DeserializeResponse::Error(e.into_inner()),
                            }
                        }
                        DeserializeRequest::SkipValue => match deser.parser.skip_value() {
                            Ok(()) => DeserializeResponse::Skipped,
                            Err(e) => DeserializeResponse::Error(InnerDeserializeError::Parser(
                                format!("{e:?}"),
                            )),
                        },
                        DeserializeRequest::DeserializeInto { wip } => {
                            match deser.deserialize_into(wip) {
                                Ok(wip) => DeserializeResponse::Wip(wip),
                                Err(e) => DeserializeResponse::Error(e.into_inner()),
                            }
                        }
                        DeserializeRequest::SetStringValue { wip, s } => {
                            match deser.set_string_value(wip, s) {
                                Ok(wip) => DeserializeResponse::Wip(wip),
                                Err(e) => DeserializeResponse::Error(e.into_inner()),
                            }
                        }
                        DeserializeRequest::GetSpan => DeserializeResponse::Span(deser.last_span),
                    };
                    result = coro_ref.as_mut().resume(response);
                }
                CoroutineResult::Return(inner_result) => {
                    return inner_result.map_err(|e| e.into_deserialize_error());
                }
            }
        }
    })
}
