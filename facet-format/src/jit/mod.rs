//! JIT-compiled deserialization for facet-format.
//!
//! This module provides Cranelift-based JIT compilation for deserializers,
//! enabling fast deserialization that bypasses the reflection machinery.
//!
//! ## Two-Tier JIT Architecture
//!
//! ### Tier 1: Shape JIT (existing)
//! The key insight is that `FormatParser` produces a stream of `ParseEvent`s,
//! and we can JIT-compile the code that consumes these events and writes
//! directly to struct memory at known offsets.
//!
//! This works with **any** format that implements `FormatParser` - JSON, YAML,
//! KDL, TOML, etc. - because they all produce the same event stream.
//!
//! ### Tier 2: Format JIT (new)
//! For the "full slice available upfront" case, format crates can provide
//! a [`JitFormat`] implementation that emits Cranelift IR to parse bytes
//! directly, bypassing the event abstraction for maximum performance.
//!
//! ## Entry Points
//!
//! - [`try_deserialize`]: Tier-1 shape JIT (works with any `FormatParser`)
//! - [`try_deserialize_format`]: Tier-2 format JIT (requires `FormatJitParser`)
//! - [`try_deserialize_with_format_jit`]: Try Tier-2 first, then Tier-1
//! - [`deserialize_with_fallback`]: Try JIT, then reflection

/// Debug print macro for JIT - only active in debug builds.
#[cfg(debug_assertions)]
macro_rules! jit_debug {
    ($($arg:tt)*) => { eprintln!($($arg)*) }
}

#[cfg(not(debug_assertions))]
macro_rules! jit_debug {
    ($($arg:tt)*) => {};
}

pub(crate) use jit_debug;

mod cache;
mod compiler;
mod format;
mod format_compiler;
pub mod helpers;

use facet_core::{ConstTypeId, Facet};

use crate::{DeserializeError, FormatDeserializer, FormatParser};

pub use compiler::CompiledDeserializer;
pub use format::{JitCursor, JitFormat, JitScratch, JitStringValue, NoFormatJit};
pub use format_compiler::CompiledFormatDeserializer;

// Re-export FormatJitParser from crate root for convenience
pub use crate::FormatJitParser;

// Re-export Cranelift types for format crates implementing JitFormat
pub use cranelift::codegen::ir::BlockArg;
pub use cranelift::prelude::{
    AbiParam, FunctionBuilder, InstBuilder, IntCC, MemFlags, Signature, StackSlotData,
    StackSlotKind, Value, Variable, types,
};
pub use cranelift_jit::JITBuilder;

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

// =============================================================================
// Tier-2 Format JIT Entry Points
// =============================================================================

/// Try to deserialize using Tier-2 format JIT.
///
/// This is the Tier-2 entry point that requires the parser to implement
/// [`FormatJitParser`]. It generates code that parses bytes directly using
/// format-specific IR, bypassing the event abstraction.
///
/// Returns `Some(result)` if:
/// - The type is Tier-2 compatible
/// - The parser provides a complete input slice (`jit_input`)
/// - The parser has no buffered state (`jit_pos` returns Some)
///
/// Returns `None` if Tier-2 cannot be used, in which case the caller should
/// try [`try_deserialize`] (Tier-1) or fall back to reflection.
///
/// Note: `Err(Unsupported(...))` from the compiled deserializer is converted
/// to `None` to allow fallback. Only actual parse errors are returned as `Some(Err(...))`.
pub fn try_deserialize_format<'de, T, P>(
    parser: &mut P,
) -> Option<Result<T, DeserializeError<P::Error>>>
where
    T: Facet<'de>,
    P: FormatJitParser<'de>,
{
    // Check if parser position is available (no buffered state)
    if parser.jit_pos().is_none() {
        return None;
    }

    // Check if this type is Tier-2 compatible
    if !format_compiler::is_format_jit_compatible(T::SHAPE) {
        return None;
    }

    // Get or compile the Tier-2 deserializer
    let key = (T::SHAPE.id, ConstTypeId::of::<P>());
    let compiled = cache::get_or_compile_format::<T, P>(key)?;

    // Execute the compiled deserializer
    // Convert Unsupported errors to None (allows fallback to Tier-1)
    match compiled.deserialize(parser) {
        Ok(value) => Some(Ok(value)),
        Err(DeserializeError::Unsupported(_)) => None,
        Err(e) => Some(Err(e)),
    }
}

/// Check if a type can use Tier-2 format JIT.
///
/// Returns `true` for types that can be deserialized via format-specific
/// byte parsing (currently Vec<scalar> types).
pub fn is_format_jit_compatible<'a, T: Facet<'a>>() -> bool {
    format_compiler::is_format_jit_compatible(T::SHAPE)
}

/// Try Tier-2 format JIT first, then fall back to Tier-1 shape JIT.
///
/// This is the recommended entry point for parsers that implement
/// [`FormatJitParser`]. It attempts the fastest path first.
///
/// Returns `Some(result)` if either JIT tier succeeded.
/// Returns `None` if neither JIT tier applies (caller should use reflection).
pub fn try_deserialize_with_format_jit<'de, T, P>(
    parser: &mut P,
) -> Option<Result<T, DeserializeError<P::Error>>>
where
    T: Facet<'de>,
    P: FormatJitParser<'de>,
{
    // Try Tier-2 first
    if let Some(result) = try_deserialize_format::<T, P>(parser) {
        return Some(result);
    }

    // Fall back to Tier-1
    try_deserialize::<T, P>(parser)
}

/// Deserialize with format JIT and automatic fallback.
///
/// Tries Tier-2 format JIT first, then Tier-1 shape JIT, then reflection.
/// This is the recommended entry point for production use with parsers
/// that implement [`FormatJitParser`].
pub fn deserialize_with_format_jit_fallback<'de, T, P>(
    mut parser: P,
) -> Result<T, DeserializeError<P::Error>>
where
    T: Facet<'de>,
    P: FormatJitParser<'de>,
{
    // Try Tier-2 first
    if let Some(result) = try_deserialize_format::<T, P>(&mut parser) {
        return result;
    }

    // Try Tier-1
    if let Some(result) = try_deserialize::<T, P>(&mut parser) {
        return result;
    }

    // Fall back to reflection-based deserialization
    FormatDeserializer::new(parser).deserialize()
}
