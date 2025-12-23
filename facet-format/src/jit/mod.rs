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
//!
//! ## Tier-2 Contract (Format JIT)
//!
//! ### Supported Shapes
//!
//! Tier-2 currently supports a carefully-chosen subset of shapes for maximum performance:
//!
//! - **Scalar types**: `bool`, `u8-u64`, `i8-i64`, `f32`, `f64`, `String`
//! - **`Option<T>`**: Where `T` is any supported type (scalar, Vec, nested struct, enum, map)
//! - **`Vec<T>`**: Where `T` is any supported type
//!   - Includes bulk-copy optimization for `Vec<u8>`
//! - **HashMap<String, V>**: Where `V` is any supported type
//!   - Only String keys are supported (not arbitrary key types)
//! - **Enums**: Standalone enums (newtype variants with struct payloads)
//!   - Each variant must have exactly one unnamed field containing a struct
//!   - Discriminant is written, payload is deserialized recursively
//! - **Structs**: Named-field structs containing supported types
//!   - Recursive nesting allowed (within budget limits)
//!   - No custom defaults (Option pre-init is fine)
//!   - **Flatten support**:
//!     - `#[facet(flatten)]` on struct fields: Inner fields merged into parent dispatch table
//!     - `#[facet(flatten)]` on enum fields: Variant names become dispatch keys
//!     - `#[facet(flatten)]` on HashMap<String, V> fields: Captures unknown keys (serde-style "extra fields")
//!     - Multiple flattened structs/enums allowed, but only ONE flattened map per struct
//!
//! **Not yet supported**: Tuple structs, unit structs, enums with unit/tuple variants, maps with non-String keys.
//!
//! ### Execution Outcomes
//!
//! Tier-2 compiled functions return `isize` with three possible outcomes:
//!
//! 1. **Success** (`>= 0`):
//!    - Return value is the new cursor position
//!    - Output is fully initialized and valid
//!    - Parser cursor advanced via `jit_set_pos()`
//!
//! 2. **Unsupported** (returns `-1`, code `T2_ERR_UNSUPPORTED`):
//!    - Shape or input not compatible with Tier-2 at runtime
//!    - Parser cursor **unchanged** (no side effects)
//!    - Output **not initialized** (fallback required)
//!    - Caller must fall back to Tier-1 or reflection
//!
//! 3. **Parse Error** (returns `-2` or format-specific negative code):
//!    - Invalid input encountered
//!    - `scratch.error_code` and `scratch.error_pos` contain error details
//!    - Caller maps to `DeserializeError` via `jit_error()`
//!    - Output **not valid** (error state)
//!
//! ### Ownership & Drop Semantics
//!
//! Tier-2 manages heap allocations for `String` and `Vec<T>`:
//!
//! - **Allocation points**: String unescaping, Vec growth
//! - **Transfer on success**: Ownership moved to output; caller responsible for drop
//! - **Cleanup on error**: Tier-2 drops any partially-constructed values before returning error
//! - **Helper functions**: `jit_drop_owned_string` centralizes drop logic
//! - **Unknown field skip**: Temporary allocations during skip are dropped correctly
//!
//! ### Caching Behavior
//!
//! Tier-2 uses a **two-level cache** with **positive and negative caching**:
//!
//! 1. **Thread-local cache** (TLS):
//!    - Single-entry cache for hot loops (O(1) key comparison)
//!    - Caches both compiled modules (Hit) and known failures (Miss)
//!
//! 2. **Global cache**:
//!    - Bounded HashMap with FIFO eviction (default: 1024 entries)
//!    - Keyed by `(TypeId<T>, TypeId<P>)`
//!    - Caches both successes and failures (negative cache)
//!    - Eviction is safe: `Arc<CachedFormatModule>` keeps modules alive
//!
//! **Negative caching**: Compilation failures (unsupported shapes, budget exceeded) are
//! cached to avoid repeated expensive compilation attempts. Second attempt for same
//! `(T,P)` returns `None` immediately from cache (no recompilation).
//!
//! **Configuration**:
//! - `FACET_TIER2_CACHE_MAX_ENTRIES`: Maximum cache size (default: 1024)
//!
//! ### Compilation Budgets
//!
//! To prevent pathological shapes from causing long compile times or code bloat:
//!
//! - **Field count limit**: Maximum fields per struct (default: 100)
//! - **Nesting depth limit**: Maximum recursion depth (default: 10)
//! - Budget checks happen **before** IR generation (early rejection)
//! - Budget failures are **negative cached** (no retry)
//!
//! **Configuration**:
//! - `FACET_TIER2_MAX_FIELDS`: Max fields per struct (default: 100)
//! - `FACET_TIER2_MAX_NESTING`: Max nesting depth (default: 10)
//!
//! ### Debugging & Observability
//!
//! **Environment variables**:
//! - `FACET_JIT_TRACE=1`: Enable tier selection trace messages
//! - `FACET_TIER2_CACHE_MAX_ENTRIES`: Cache capacity (default: 1024)
//! - `FACET_TIER2_MAX_FIELDS`: Budget: max fields (default: 100)
//! - `FACET_TIER2_MAX_NESTING`: Budget: max nesting (default: 10)
//!
//! **Statistics**:
//! - [`get_tier_stats()`]: Get counters without reset
//! - [`get_and_reset_tier_stats()`]: Get counters and reset
//! - [`print_tier_stats()`]: Print summary to stderr
//! - [`cache::get_cache_stats()`]: Get cache hit/miss/eviction counters
//!
//! **Counters**:
//! - `TIER2_ATTEMPTS`: How many times Tier-2 was attempted
//! - `TIER2_SUCCESSES`: How many times Tier-2 succeeded
//! - `TIER2_COMPILE_UNSUPPORTED`: Compilation refused (shape/budget)
//! - `TIER2_RUNTIME_UNSUPPORTED`: Runtime unsupported (fallback)
//! - `TIER2_RUNTIME_ERROR`: Parse errors in Tier-2
//! - `TIER1_USES`: Fallbacks to Tier-1
//! - `CACHE_HIT`: Cache hits (successful compilations)
//! - `CACHE_MISS_NEGATIVE`: Negative cache hits (known failures)
//! - `CACHE_MISS_COMPILE`: Cache misses (new compilations)
//! - `CACHE_EVICTIONS`: Number of cache evictions

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

/// Tier-2 compilation diagnostics - always enabled, even in release builds.
/// Use FACET_TIER2_DIAG=1 environment variable to enable.
macro_rules! jit_diag {
    ($($arg:tt)*) => {
        if std::env::var("FACET_TIER2_DIAG").is_ok() {
            eprintln!("[TIER2_DIAG] {}", format!($($arg)*));
        }
    }
}

pub(crate) use jit_debug;
#[allow(unused_imports)] // Used in diagnostic mode via FACET_TIER2_DIAG
pub(crate) use jit_diag;
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

pub mod cache; // Public for testing (provides cache stats, clear functions)
mod compiler;
#[cfg(all(debug_assertions, unix))]
pub mod crash_handler;
mod format;
mod format_compiler;
pub mod helpers;

use facet_core::{ConstTypeId, Facet};

use crate::{DeserializeError, FormatDeserializer, FormatParser};

pub use compiler::CompiledDeserializer;
pub use format::{JitCursor, JitFormat, JitScratch, JitStringValue, NoFormatJit, StructEncoding};
pub use format_compiler::CompiledFormatDeserializer;

// Re-export handle getter for performance-critical code
pub use cache::get_format_deserializer;

// Re-export FormatJitParser from crate root for convenience
pub use crate::FormatJitParser;

// Re-export utility functions for format crates
pub use format::{c_call_conv, make_c_sig};

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
    parser.jit_pos()?;

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
/// byte parsing (currently `Vec<scalar>` types).
///
/// Note: This uses a conservative default (Map encoding). For format-specific
/// checks, use [`is_format_jit_compatible_for`] instead.
pub fn is_format_jit_compatible<'a, T: Facet<'a>>() -> bool {
    format_compiler::is_format_jit_compatible(T::SHAPE)
}

/// Check if a type can use Tier-2 format JIT for a specific format.
///
/// This is the format-aware version that knows about each format's struct encoding.
/// For example, JSON (map-based) doesn't support tuple structs, while postcard
/// (positional) does.
///
/// # Type Parameters
/// * `T` - The type to check for compatibility
/// * `F` - The format implementation (e.g., `JsonJitFormat`, `PostcardJitFormat`)
///
/// # Examples
/// ```ignore
/// use facet_format::jit::{is_format_jit_compatible_for, JsonJitFormat};
/// use facet::Facet;
///
/// #[derive(Facet)]
/// struct TupleStruct(i64, String);
///
/// // Tuple structs are NOT supported for JSON (map-based)
/// assert!(!is_format_jit_compatible_for::<TupleStruct, JsonJitFormat>());
/// ```
pub fn is_format_jit_compatible_for<'a, T: Facet<'a>, F: JitFormat>() -> bool {
    format_compiler::is_format_jit_compatible_with_encoding(T::SHAPE, F::STRUCT_ENCODING)
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
