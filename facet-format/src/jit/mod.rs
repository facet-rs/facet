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

/// Tier selection trace - always enabled, even in release builds.
/// Use FACET_JIT_TRACE=1 environment variable to enable.
macro_rules! jit_tier_trace {
    ($($arg:tt)*) => {
        if std::env::var("FACET_JIT_TRACE").is_ok() {
            eprintln!("[TIER] {}", format!($($arg)*));
        }
    }
}

pub(crate) use jit_debug;
pub(crate) use jit_tier_trace;

// Tier usage counters - always enabled
use std::sync::atomic::{AtomicU64, Ordering};

static TIER2_ATTEMPTS: AtomicU64 = AtomicU64::new(0);
static TIER2_SUCCESSES: AtomicU64 = AtomicU64::new(0);
static TIER2_COMPILE_UNSUPPORTED: AtomicU64 = AtomicU64::new(0);
static TIER2_RUNTIME_UNSUPPORTED: AtomicU64 = AtomicU64::new(0);
static TIER2_RUNTIME_ERROR: AtomicU64 = AtomicU64::new(0);
static TIER1_USES: AtomicU64 = AtomicU64::new(0);

/// Get tier usage statistics without resetting counters.
/// Returns (tier2_attempts, tier2_successes, tier2_compile_unsupported, tier2_runtime_unsupported, tier2_runtime_error, tier1_uses).
pub fn get_tier_stats() -> (u64, u64, u64, u64, u64, u64) {
    (
        TIER2_ATTEMPTS.load(Ordering::Relaxed),
        TIER2_SUCCESSES.load(Ordering::Relaxed),
        TIER2_COMPILE_UNSUPPORTED.load(Ordering::Relaxed),
        TIER2_RUNTIME_UNSUPPORTED.load(Ordering::Relaxed),
        TIER2_RUNTIME_ERROR.load(Ordering::Relaxed),
        TIER1_USES.load(Ordering::Relaxed),
    )
}

/// Get tier usage statistics and reset counters.
/// Returns (tier2_attempts, tier2_successes, tier2_compile_unsupported, tier2_runtime_unsupported, tier2_runtime_error, tier1_uses).
pub fn get_and_reset_tier_stats() -> (u64, u64, u64, u64, u64, u64) {
    (
        TIER2_ATTEMPTS.swap(0, Ordering::Relaxed),
        TIER2_SUCCESSES.swap(0, Ordering::Relaxed),
        TIER2_COMPILE_UNSUPPORTED.swap(0, Ordering::Relaxed),
        TIER2_RUNTIME_UNSUPPORTED.swap(0, Ordering::Relaxed),
        TIER2_RUNTIME_ERROR.swap(0, Ordering::Relaxed),
        TIER1_USES.swap(0, Ordering::Relaxed),
    )
}

/// Reset tier statistics counters.
pub fn reset_tier_stats() {
    TIER2_ATTEMPTS.store(0, Ordering::Relaxed);
    TIER2_SUCCESSES.store(0, Ordering::Relaxed);
    TIER2_COMPILE_UNSUPPORTED.store(0, Ordering::Relaxed);
    TIER2_RUNTIME_UNSUPPORTED.store(0, Ordering::Relaxed);
    TIER2_RUNTIME_ERROR.store(0, Ordering::Relaxed);
    TIER1_USES.store(0, Ordering::Relaxed);
}

/// Print tier usage statistics to stderr.
pub fn print_tier_stats() {
    let (t2_attempts, t2_successes, t2_compile_unsup, t2_runtime_unsup, t2_runtime_err, t1_uses) =
        get_and_reset_tier_stats();
    if t2_attempts > 0 || t1_uses > 0 {
        eprintln!("━━━ JIT Tier Usage ━━━");
        eprintln!("  Tier-2 attempts:   {}", t2_attempts);
        eprintln!(
            "  Tier-2 successes:  {} ({:.1}%)",
            t2_successes,
            if t2_attempts > 0 {
                (t2_successes as f64 / t2_attempts as f64) * 100.0
            } else {
                0.0
            }
        );
        if t2_compile_unsup > 0 {
            eprintln!("  Tier-2 compile unsupported: {}", t2_compile_unsup);
        }
        if t2_runtime_unsup > 0 {
            eprintln!("  Tier-2 runtime unsupported: {}", t2_runtime_unsup);
        }
        if t2_runtime_err > 0 {
            eprintln!("  Tier-2 runtime errors: {}", t2_runtime_err);
        }
        eprintln!("  Tier-1 fallbacks:  {}", t1_uses);
        eprintln!("━━━━━━━━━━━━━━━━━━━━━");
    }
}

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

// Re-export handle getter for performance-critical code
pub use cache::get_format_deserializer;

// Re-export FormatJitParser from crate root for convenience
pub use crate::FormatJitParser;

// Re-export Cranelift types for format crates implementing JitFormat
pub use cranelift::codegen::ir::BlockArg;
pub use cranelift::codegen::ir::{ExtFuncData, ExternalName, UserExternalName};
pub use cranelift::codegen::isa::CallConv;
pub use cranelift::prelude::{
    AbiParam, FunctionBuilder, InstBuilder, IntCC, MemFlags, Signature, StackSlotData,
    StackSlotKind, Value, Variable, types,
};
pub use cranelift_jit::{JITBuilder, JITModule};
pub use cranelift_module::{Linkage, Module};

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

    // Get or compile the Tier-2 deserializer
    // (compatibility check happens inside on cache miss only)
    let key = (T::SHAPE.id, ConstTypeId::of::<P>());
    let compiled = match cache::get_or_compile_format::<T, P>(key) {
        Some(c) => c,
        None => {
            // Compile-time unsupported (type not compatible or compilation failed)
            TIER2_COMPILE_UNSUPPORTED.fetch_add(1, Ordering::Relaxed);
            jit_tier_trace!(
                "✗ Tier-2 COMPILE UNSUPPORTED for {}",
                std::any::type_name::<T>()
            );
            return None;
        }
    };

    // Execute the compiled deserializer
    // Convert Unsupported errors to None (allows fallback to Tier-1)
    match compiled.deserialize(parser) {
        Ok(value) => Some(Ok(value)),
        Err(DeserializeError::Unsupported(_)) => {
            // Runtime unsupported (JIT returned T2_ERR_UNSUPPORTED)
            TIER2_RUNTIME_UNSUPPORTED.fetch_add(1, Ordering::Relaxed);
            jit_tier_trace!(
                "✗ Tier-2 RUNTIME UNSUPPORTED for {}",
                std::any::type_name::<T>()
            );
            None
        }
        Err(e) => {
            // Runtime error (parse error, not unsupported)
            TIER2_RUNTIME_ERROR.fetch_add(1, Ordering::Relaxed);
            jit_tier_trace!("✗ Tier-2 RUNTIME ERROR for {}", std::any::type_name::<T>());
            Some(Err(e))
        }
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
    TIER2_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
    jit_tier_trace!("Attempting Tier-2 for {}", std::any::type_name::<T>());

    if let Some(result) = try_deserialize_format::<T, P>(parser) {
        TIER2_SUCCESSES.fetch_add(1, Ordering::Relaxed);
        jit_tier_trace!("✓ Tier-2 USED for {}", std::any::type_name::<T>());
        return Some(result);
    }

    // Fall back to Tier-1
    jit_tier_trace!(
        "Tier-2 unavailable, falling back to Tier-1 for {}",
        std::any::type_name::<T>()
    );
    let result = try_deserialize::<T, P>(parser);
    if result.is_some() {
        TIER1_USES.fetch_add(1, Ordering::Relaxed);
        jit_tier_trace!("✓ Tier-1 USED for {}", std::any::type_name::<T>());
    } else {
        jit_tier_trace!(
            "✗ NO JIT (both tiers unavailable) for {}",
            std::any::type_name::<T>()
        );
    }
    result
}

/// Deserialize with format JIT and automatic fallback.
///
/// Tries Tier-2 format JIT first, then Tier-1 shape JIT, then reflection.
/// This is the recommended entry point for production use with parsers
/// that implement [`FormatJitParser`].
///
/// Note: This function tracks tier usage statistics if used during benchmarks.
pub fn deserialize_with_format_jit_fallback<'de, T, P>(
    mut parser: P,
) -> Result<T, DeserializeError<P::Error>>
where
    T: Facet<'de>,
    P: FormatJitParser<'de>,
{
    // Use the tier-tracking version to ensure stats are collected
    if let Some(result) = try_deserialize_with_format_jit::<T, P>(&mut parser) {
        return result;
    }

    // Fall back to reflection-based deserialization
    FormatDeserializer::new(parser).deserialize()
}
