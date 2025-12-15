//! JIT-compiled deserialization for facet-format.
//!
//! This module provides Cranelift-based JIT compilation for deserializers,
//! enabling fast deserialization that bypasses the reflection machinery.
//!
//! The key insight is that `FormatParser` produces a stream of `ParseEvent`s,
//! and we can JIT-compile the code that consumes these events and writes
//! directly to struct memory at known offsets.
//!
//! This works with **any** format that implements `FormatParser` - JSON, YAML,
//! KDL, TOML, etc. - because they all produce the same event stream.

mod cache;
mod compiler;
mod helpers;

use facet_core::{ConstTypeId, Facet};

use crate::{DeserializeError, FormatDeserializer, FormatParser};

pub use compiler::CompiledDeserializer;

/// Try to deserialize using JIT-compiled code.
///
/// Returns `Some(result)` if JIT compilation succeeded and deserialization was attempted.
/// Returns `None` if the type is not JIT-compatible (has flatten, untagged enums, etc.),
/// in which case the caller should fall back to reflection-based deserialization.
pub fn try_deserialize<'de, T, P>(parser: &mut P) -> Option<Result<T, DeserializeError<P::Error>>>
where
    T: Facet<'de>,
    P: FormatParser<'de>,
{
    // Check if this type is JIT-compatible
    if !compiler::is_jit_compatible(T::SHAPE) {
        return None;
    }

    // Get or compile the deserializer
    // Use ConstTypeId for both T and P to erase lifetimes
    let key = (T::SHAPE.id, ConstTypeId::of::<P>());
    let compiled = cache::get_or_compile::<T, P>(key)?;

    // Execute the compiled deserializer
    Some(compiled.deserialize(parser))
}

/// Check if a type can be JIT-compiled.
///
/// Returns `true` for simple structs without flatten or untagged enums.
pub fn is_jit_compatible<'a, T: Facet<'a>>() -> bool {
    compiler::is_jit_compatible(T::SHAPE)
}

/// Deserialize with automatic fallback to reflection-based deserialization.
///
/// Tries JIT-compiled deserialization first. If the type is not JIT-compatible,
/// falls back to the standard `FormatDeserializer`.
///
/// This is the recommended entry point for production use.
pub fn deserialize_with_fallback<'de, T, P>(mut parser: P) -> Result<T, DeserializeError<P::Error>>
where
    T: Facet<'de>,
    P: FormatParser<'de>,
{
    // Try JIT first
    if let Some(result) = try_deserialize::<T, P>(&mut parser) {
        return result;
    }

    // Fall back to reflection-based deserialization
    FormatDeserializer::new(parser).deserialize()
}
