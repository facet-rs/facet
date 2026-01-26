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

use alloc::format;

use corosensei::stack::DefaultStack;
use corosensei::{CoroutineResult, ScopedCoroutine, Yielder};
use facet_reflect::{Partial, Span};

use alloc::borrow::Cow;
use alloc::vec::Vec;

use crate::{
    DeserializeError, FieldEvidence, FormatDeserializer, FormatParser, InnerDeserializeError,
    ParseEvent,
};

/// Stack size for coroutines: 2MB to handle deep nesting and coverage instrumentation.
const STACK_SIZE: usize = 2 * 1024 * 1024;

/// Request from the inner deserialization logic to the wrapper.
pub(crate) enum DeserializeRequest<'input, const BORROW: bool> {
    /// Need to call `expect_event(expected)` and get the result.
    ExpectEvent { expected: &'static str },

    /// Need to call `expect_peek(expected)` and get the result.
    ExpectPeek { expected: &'static str },

    /// Need to call `parser.peek_event()` and get the raw Option result.
    PeekEventRaw,

    /// Need to call `parser.skip_value()`.
    SkipValue,

    /// Need to call `deserialize_into(wip)` recursively.
    DeserializeInto { wip: Partial<'input, BORROW> },

    /// Get the current span for error reporting.
    GetSpan,

    /// Need to call `parser.begin_probe()` and collect evidence.
    CollectEvidence,

    /// Need to call `set_string_value(wip, value)`.
    SetStringValue {
        wip: Partial<'input, BORROW>,
        value: Cow<'input, str>,
    },

    /// Need to call `deserialize_variant_struct_fields(wip)`.
    DeserializeVariantStructFields { wip: Partial<'input, BORROW> },

    /// Need to call `deserialize_enum_variant_content(wip)`.
    #[allow(dead_code)] // Infrastructure for future extraction
    DeserializeEnumVariantContent { wip: Partial<'input, BORROW> },

    /// Need to call `deserialize_other_variant_with_captured_tag(wip, captured_tag)`.
    #[allow(dead_code)] // Infrastructure for future extraction
    DeserializeOtherVariantWithCapturedTag {
        wip: Partial<'input, BORROW>,
        captured_tag: Option<&'input str>,
    },

    /// Need to call `deserialize_value_recursive(wip, hint_shape)`.
    DeserializeValueRecursive {
        wip: Partial<'input, BORROW>,
        hint_shape: &'static facet_core::Shape,
    },

    /// Need to call `solve_variant(shape, &mut parser)` for untagged enum resolution.
    SolveVariant { shape: &'static facet_core::Shape },

    /// Need to call `parser.hint_enum(&variants)` for non-self-describing parsers.
    HintEnum {
        variants: Vec<crate::EnumVariantHint>,
    },

    /// Need to call `deserialize_tuple_dynamic(wip, fields)`.
    DeserializeTupleDynamic {
        wip: Partial<'input, BORROW>,
        fields: &'static [facet_core::Field],
    },

    /// Need to call `deserialize_struct_dynamic(wip, fields)`.
    DeserializeStructDynamic {
        wip: Partial<'input, BORROW>,
        fields: &'static [facet_core::Field],
    },

    /// Need to call `deserialize_enum_as_struct(wip, enum_def)`.
    DeserializeEnumAsStruct {
        wip: Partial<'input, BORROW>,
        enum_def: &'static facet_core::EnumType,
    },
}

/// Response from the wrapper to the inner deserialization logic.
pub(crate) enum DeserializeResponse<'input, const BORROW: bool> {
    /// Result of `expect_event` or `expect_peek`.
    Event(ParseEvent<'input>),

    /// Result of `peek_event_raw` (may be None at EOF).
    MaybeEvent(Option<ParseEvent<'input>>),

    /// Result of `skip_value` (success).
    Skipped,

    /// Result of `deserialize_into` or `set_string_value` (success).
    Wip(Partial<'input, BORROW>),

    /// Current span value.
    Span(Option<Span>),

    /// Result of `collect_evidence`.
    Evidence(Vec<FieldEvidence<'input>>),

    /// Result of `solve_variant` - the resolved variant name, or None if no match.
    SolveVariantResult(Option<&'static str>),

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

    /// Unwrap as an optional event (for raw peek), or return an error.
    pub fn into_maybe_event(self) -> Result<Option<ParseEvent<'input>>, InnerDeserializeError> {
        match self {
            DeserializeResponse::MaybeEvent(e) => Ok(e),
            DeserializeResponse::Error(e) => Err(e),
            other => Err(InnerDeserializeError::Unsupported(format!(
                "expected MaybeEvent response, got {:?}",
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

    /// Unwrap as evidence, or return an error.
    pub fn into_evidence(self) -> Result<Vec<FieldEvidence<'input>>, InnerDeserializeError> {
        match self {
            DeserializeResponse::Evidence(ev) => Ok(ev),
            DeserializeResponse::Error(e) => Err(e),
            other => Err(InnerDeserializeError::Unsupported(format!(
                "expected Evidence response, got {:?}",
                core::mem::discriminant(&other)
            ))),
        }
    }

    /// Unwrap as solve_variant result, or return an error.
    pub fn into_solve_variant_result(self) -> Result<Option<&'static str>, InnerDeserializeError> {
        match self {
            DeserializeResponse::SolveVariantResult(result) => Ok(result),
            DeserializeResponse::Error(e) => Err(e),
            other => Err(InnerDeserializeError::Unsupported(format!(
                "expected SolveVariantResult response, got {:?}",
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

/// Helper to peek at the next event, returning None at EOF.
pub(crate) fn request_peek_raw<'input, const BORROW: bool>(
    yielder: &DeserializeYielder<'input, BORROW>,
) -> Result<Option<ParseEvent<'input>>, InnerDeserializeError> {
    yielder
        .suspend(DeserializeRequest::PeekEventRaw)
        .into_maybe_event()
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

/// Helper to get the current span for error reporting.
pub(crate) fn request_span<'input, const BORROW: bool>(
    yielder: &DeserializeYielder<'input, BORROW>,
) -> Option<Span> {
    yielder
        .suspend(DeserializeRequest::GetSpan)
        .into_span()
        .unwrap_or(None)
}

/// Helper to collect evidence via probe.
pub(crate) fn request_collect_evidence<'input, const BORROW: bool>(
    yielder: &DeserializeYielder<'input, BORROW>,
) -> Result<Vec<FieldEvidence<'input>>, InnerDeserializeError> {
    yielder
        .suspend(DeserializeRequest::CollectEvidence)
        .into_evidence()
}

/// Helper to set a string value.
pub(crate) fn request_set_string_value<'input, const BORROW: bool>(
    yielder: &DeserializeYielder<'input, BORROW>,
    wip: Partial<'input, BORROW>,
    value: Cow<'input, str>,
) -> Result<Partial<'input, BORROW>, InnerDeserializeError> {
    yielder
        .suspend(DeserializeRequest::SetStringValue { wip, value })
        .into_wip()
}

/// Helper to deserialize variant struct fields.
pub(crate) fn request_deserialize_variant_struct_fields<'input, const BORROW: bool>(
    yielder: &DeserializeYielder<'input, BORROW>,
    wip: Partial<'input, BORROW>,
) -> Result<Partial<'input, BORROW>, InnerDeserializeError> {
    yielder
        .suspend(DeserializeRequest::DeserializeVariantStructFields { wip })
        .into_wip()
}

/// Helper to deserialize enum variant content.
#[allow(dead_code)] // Infrastructure for future extraction
pub(crate) fn request_deserialize_enum_variant_content<'input, const BORROW: bool>(
    yielder: &DeserializeYielder<'input, BORROW>,
    wip: Partial<'input, BORROW>,
) -> Result<Partial<'input, BORROW>, InnerDeserializeError> {
    yielder
        .suspend(DeserializeRequest::DeserializeEnumVariantContent { wip })
        .into_wip()
}

/// Helper to deserialize other variant with captured tag.
#[allow(dead_code)] // Infrastructure for future extraction
pub(crate) fn request_deserialize_other_variant_with_captured_tag<'input, const BORROW: bool>(
    yielder: &DeserializeYielder<'input, BORROW>,
    wip: Partial<'input, BORROW>,
    captured_tag: Option<&'input str>,
) -> Result<Partial<'input, BORROW>, InnerDeserializeError> {
    yielder
        .suspend(DeserializeRequest::DeserializeOtherVariantWithCapturedTag { wip, captured_tag })
        .into_wip()
}

/// Helper to deserialize a value recursively with a shape hint.
pub(crate) fn request_deserialize_value_recursive<'input, const BORROW: bool>(
    yielder: &DeserializeYielder<'input, BORROW>,
    wip: Partial<'input, BORROW>,
    hint_shape: &'static facet_core::Shape,
) -> Result<Partial<'input, BORROW>, InnerDeserializeError> {
    yielder
        .suspend(DeserializeRequest::DeserializeValueRecursive { wip, hint_shape })
        .into_wip()
}

/// Helper to solve which variant matches for untagged enums.
pub(crate) fn request_solve_variant<'input, const BORROW: bool>(
    yielder: &DeserializeYielder<'input, BORROW>,
    shape: &'static facet_core::Shape,
) -> Result<Option<&'static str>, InnerDeserializeError> {
    yielder
        .suspend(DeserializeRequest::SolveVariant { shape })
        .into_solve_variant_result()
}

/// Helper to hint the parser about enum variants.
pub(crate) fn request_hint_enum<'input, const BORROW: bool>(
    yielder: &DeserializeYielder<'input, BORROW>,
    variants: Vec<crate::EnumVariantHint>,
) -> Result<(), InnerDeserializeError> {
    yielder
        .suspend(DeserializeRequest::HintEnum { variants })
        .into_skipped()
}

/// Helper to deserialize a tuple with dynamic fields.
pub(crate) fn request_deserialize_tuple_dynamic<'input, const BORROW: bool>(
    yielder: &DeserializeYielder<'input, BORROW>,
    wip: Partial<'input, BORROW>,
    fields: &'static [facet_core::Field],
) -> Result<Partial<'input, BORROW>, InnerDeserializeError> {
    yielder
        .suspend(DeserializeRequest::DeserializeTupleDynamic { wip, fields })
        .into_wip()
}

/// Helper to deserialize a struct with dynamic fields.
pub(crate) fn request_deserialize_struct_dynamic<'input, const BORROW: bool>(
    yielder: &DeserializeYielder<'input, BORROW>,
    wip: Partial<'input, BORROW>,
    fields: &'static [facet_core::Field],
) -> Result<Partial<'input, BORROW>, InnerDeserializeError> {
    yielder
        .suspend(DeserializeRequest::DeserializeStructDynamic { wip, fields })
        .into_wip()
}

/// Helper to deserialize an enum as a struct (for non-self-describing formats).
pub(crate) fn request_deserialize_enum_as_struct<'input, const BORROW: bool>(
    yielder: &DeserializeYielder<'input, BORROW>,
    wip: Partial<'input, BORROW>,
    enum_def: &'static facet_core::EnumType,
) -> Result<Partial<'input, BORROW>, InnerDeserializeError> {
    yielder
        .suspend(DeserializeRequest::DeserializeEnumAsStruct { wip, enum_def })
        .into_wip()
}

/// Run a coroutine-based deserializer with the given inner function.
///
/// This is the generic wrapper that handles parser operations. The inner function
/// is non-generic over the parser type, reducing monomorphization.
#[inline(never)]
pub(crate) fn run_deserialize_coro<'input, const BORROW: bool, P, F, R>(
    deser: &mut FormatDeserializer<'input, BORROW, P>,
    inner_fn: F,
) -> Result<R, DeserializeError<P::Error>>
where
    P: FormatParser<'input>,
    F: FnOnce(&DeserializeYielder<'input, BORROW>) -> Result<R, InnerDeserializeError>,
{
    // Create the coroutine with its own stack
    let stack = DefaultStack::new(STACK_SIZE).expect("failed to allocate coroutine stack");

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
                        DeserializeRequest::PeekEventRaw => match deser.parser.peek_event() {
                            Ok(maybe_event) => DeserializeResponse::MaybeEvent(maybe_event),
                            Err(e) => DeserializeResponse::Error(InnerDeserializeError::Parser(
                                format!("{e:?}"),
                            )),
                        },
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
                        DeserializeRequest::GetSpan => DeserializeResponse::Span(deser.last_span),
                        DeserializeRequest::CollectEvidence => match deser.parser.begin_probe() {
                            Ok(probe) => {
                                match FormatDeserializer::<'input, BORROW, P>::collect_evidence(
                                    probe,
                                ) {
                                    Ok(ev) => DeserializeResponse::Evidence(ev),
                                    Err(e) => DeserializeResponse::Error(
                                        InnerDeserializeError::Parser(format!("{e:?}")),
                                    ),
                                }
                            }
                            Err(e) => DeserializeResponse::Error(InnerDeserializeError::Parser(
                                format!("{e:?}"),
                            )),
                        },
                        DeserializeRequest::SetStringValue { wip, value } => {
                            match deser.set_string_value(wip, value) {
                                Ok(wip) => DeserializeResponse::Wip(wip),
                                Err(e) => DeserializeResponse::Error(e.into_inner()),
                            }
                        }
                        DeserializeRequest::DeserializeVariantStructFields { wip } => {
                            match deser.deserialize_variant_struct_fields(wip) {
                                Ok(wip) => DeserializeResponse::Wip(wip),
                                Err(e) => DeserializeResponse::Error(e.into_inner()),
                            }
                        }
                        DeserializeRequest::DeserializeEnumVariantContent { wip } => {
                            match deser.deserialize_enum_variant_content(wip) {
                                Ok(wip) => DeserializeResponse::Wip(wip),
                                Err(e) => DeserializeResponse::Error(e.into_inner()),
                            }
                        }
                        DeserializeRequest::DeserializeOtherVariantWithCapturedTag {
                            wip,
                            captured_tag,
                        } => match deser
                            .deserialize_other_variant_with_captured_tag(wip, captured_tag)
                        {
                            Ok(wip) => DeserializeResponse::Wip(wip),
                            Err(e) => DeserializeResponse::Error(e.into_inner()),
                        },
                        DeserializeRequest::DeserializeValueRecursive { wip, hint_shape } => {
                            match deser.deserialize_value_recursive(wip, hint_shape) {
                                Ok(wip) => DeserializeResponse::Wip(wip),
                                Err(e) => DeserializeResponse::Error(e.into_inner()),
                            }
                        }
                        DeserializeRequest::SolveVariant { shape } => {
                            match crate::solve_variant(shape, &mut deser.parser) {
                                Ok(Some(outcome)) => {
                                    // Extract the variant name from the resolution
                                    let variant_name = outcome
                                        .resolution()
                                        .variant_selections()
                                        .first()
                                        .map(|vs| vs.variant_name);
                                    DeserializeResponse::SolveVariantResult(variant_name)
                                }
                                Ok(None) => DeserializeResponse::SolveVariantResult(None),
                                Err(e) => {
                                    DeserializeResponse::Error(InnerDeserializeError::Unsupported(
                                        format!("solve_variant failed: {e:?}"),
                                    ))
                                }
                            }
                        }
                        DeserializeRequest::HintEnum { variants } => {
                            deser.parser.hint_enum(&variants);
                            DeserializeResponse::Skipped
                        }
                        DeserializeRequest::DeserializeTupleDynamic { wip, fields } => {
                            match deser.deserialize_tuple_dynamic(wip, fields) {
                                Ok(wip) => DeserializeResponse::Wip(wip),
                                Err(e) => DeserializeResponse::Error(e.into_inner()),
                            }
                        }
                        DeserializeRequest::DeserializeStructDynamic { wip, fields } => {
                            match deser.deserialize_struct_dynamic(wip, fields) {
                                Ok(wip) => DeserializeResponse::Wip(wip),
                                Err(e) => DeserializeResponse::Error(e.into_inner()),
                            }
                        }
                        DeserializeRequest::DeserializeEnumAsStruct { wip, enum_def } => {
                            match deser.deserialize_enum_as_struct(wip, enum_def) {
                                Ok(wip) => DeserializeResponse::Wip(wip),
                                Err(e) => DeserializeResponse::Error(e.into_inner()),
                            }
                        }
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
