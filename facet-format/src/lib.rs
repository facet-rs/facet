#![cfg_attr(not(feature = "jit"), deny(unsafe_code))]
#![deny(missing_docs, rustdoc::broken_intra_doc_links)]
#![allow(unused_macros)]

//! Prototype types for the format deserializer.

/// Trace-level logging macro that forwards to `tracing::trace!` when the `tracing` feature is enabled.
///
/// When the `tracing` feature is disabled, this expands to nothing.
#[cfg(feature = "tracing")]
macro_rules! trace {
    ($($arg:tt)*) => {
        ::tracing::trace!($($arg)*)
    };
}

/// Trace-level logging macro (no-op when `tracing` feature is disabled).
#[cfg(not(feature = "tracing"))]
macro_rules! trace {
    ($($arg:tt)*) => {};
}

/// Debug-level logging macro that forwards to `tracing::debug!` when the `tracing` feature is enabled.
///
/// When the `tracing` feature is disabled, this expands to nothing.
#[cfg(feature = "tracing")]
#[allow(unused_macros)]
macro_rules! debug {
    ($($arg:tt)*) => {
        ::tracing::debug!($($arg)*)
    };
}

/// Debug-level logging macro (no-op when `tracing` feature is disabled).
#[cfg(not(feature = "tracing"))]
#[allow(unused_macros)]
macro_rules! debug {
    ($($arg:tt)*) => {};
}

#[allow(unused_imports)]
pub(crate) use debug;
#[allow(unused_imports)]
pub(crate) use trace;

mod deserializer;
mod event;
mod evidence;
mod parser;
mod serializer;
mod solver;
mod visitor;

#[cfg(feature = "jit")]
pub mod jit;

pub use deserializer::{
    DeserializeError, DeserializeErrorKind, FormatDeserializer, ParseError, SpanGuard,
};
pub use event::{
    ContainerKind, FieldKey, FieldLocationHint, ParseEvent, ParseEventKind, ScalarValue, ValueMeta,
    ValueMetaBuilder, ValueTypeHint,
};
pub use evidence::FieldEvidence;
#[cfg(feature = "jit")]
pub use parser::FormatJitParser;
pub use parser::{EnumVariantHint, FormatParser, SavePoint, ScalarTypeHint};
pub use serializer::{
    DynamicValueEncoding, DynamicValueTag, EnumVariantEncoding, FieldOrdering, FormatSerializer,
    MapEncoding, SerializeError, StructFieldMode, serialize_root, serialize_value_with_shape,
};
pub use solver::{SolveOutcome, SolveVariantError, solve_variant};
pub use visitor::{FieldMatch, StructFieldTracker};
