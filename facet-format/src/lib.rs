#![cfg_attr(not(feature = "jit"), deny(unsafe_code))]
#![deny(missing_docs, rustdoc::broken_intra_doc_links)]

//! Prototype types for the format codex deserializer.

mod deserializer;
mod event;
mod evidence;
mod parser;
mod serializer;
mod solver;
mod visitor;

#[cfg(feature = "jit")]
pub mod jit;

pub use deserializer::{DeserializeError, FormatDeserializer};
pub use event::{
    ContainerKind, FieldKey, FieldLocationHint, ParseEvent, ScalarValue, ValueTypeHint,
};
pub use evidence::FieldEvidence;
#[cfg(feature = "jit")]
pub use parser::FormatJitParser;
pub use parser::{FormatParser, ProbeStream};
pub use serializer::{FieldOrdering, FormatSerializer, SerializeError, serialize_root};
pub use solver::{SolveOutcome, SolveVariantError, solve_variant};
pub use visitor::{FieldMatch, StructFieldTracker};
