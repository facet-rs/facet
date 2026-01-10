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
//! TOML, etc. - because they all produce the same event stream.
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

/// Check if JIT debug output is enabled (cached).
/// Set FACET_JIT_DEBUG=1 to enable verbose JIT tracing.
pub(crate) fn jit_debug_enabled() -> bool {
    use std::sync::OnceLock;
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("FACET_JIT_DEBUG").is_ok())
}

/// Debug print macro for JIT - opt-in via FACET_JIT_DEBUG=1 environment variable.
/// This covers all JIT debugging: tier selection, compilation diagnostics, and runtime tracing.
macro_rules! jit_debug {
    ($($arg:tt)*) => {
        if $crate::jit::jit_debug_enabled() {
            eprintln!("[JIT] {}", format!($($arg)*));
        }
    }
}

pub(crate) use jit_debug;

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

// =============================================================================
// Tier-2 Incompatibility Reasons
// =============================================================================

/// Reason why a type is not compatible with Tier-2 JIT compilation.
///
/// This provides detailed, actionable information about why compilation failed,
/// including the specific type, field, or constraint that caused the issue.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum Tier2Incompatibility {
    /// Platform not supported (requires 64-bit for ABI)
    Not64BitPlatform,

    /// Shape type not recognized (not List, Map, Struct, or Enum)
    UnrecognizedShapeType {
        /// The type that was not recognized.
        type_name: &'static str,
    },

    /// Struct uses tuple or unit kind with map-based format (like JSON)
    TupleStructWithMapFormat {
        /// The tuple struct type.
        type_name: &'static str,
    },

    /// Field has a custom default (not supported in Tier-2)
    FieldHasCustomDefault {
        /// The containing struct type.
        type_name: &'static str,
        /// The field with the custom default.
        field_name: &'static str,
    },

    /// Field type not supported
    UnsupportedFieldType {
        /// The containing struct type.
        type_name: &'static str,
        /// The field with the unsupported type.
        field_name: &'static str,
        /// Description of the unsupported type.
        field_type: &'static str,
    },

    /// Flattened field type not supported (must be struct, enum, or HashMap<String, V>)
    UnsupportedFlattenType {
        /// The containing struct type.
        type_name: &'static str,
        /// The flattened field.
        field_name: &'static str,
    },

    /// Flattened map has non-String key
    FlattenedMapNonStringKey {
        /// The containing struct type.
        type_name: &'static str,
        /// The flattened map field.
        field_name: &'static str,
    },

    /// Enum representation not supported
    UnsupportedEnumRepr {
        /// The enum type.
        type_name: &'static str,
        /// The unsupported representation.
        repr: &'static str,
    },

    /// Enum variant has no discriminant
    EnumVariantNoDiscriminant {
        /// The enum type.
        type_name: &'static str,
        /// The variant without a discriminant.
        variant_name: &'static str,
    },

    /// Enum variant field type not supported
    UnsupportedEnumVariantField {
        /// The enum type.
        type_name: &'static str,
        /// The variant containing the unsupported field.
        variant_name: &'static str,
        /// The unsupported field.
        field_name: &'static str,
    },

    /// Flattened enum variant is unit (has no payload)
    FlattenedEnumUnitVariant {
        /// The enum type.
        type_name: &'static str,
        /// The unit variant.
        variant_name: &'static str,
    },

    /// Enum only supported with positional format (like postcard)
    EnumRequiresPositionalFormat {
        /// The enum type.
        type_name: &'static str,
    },

    /// Result<T, E> has unsupported Ok or Err type
    UnsupportedResultType {
        /// The Result type.
        type_name: &'static str,
        /// Which part is unsupported: "Ok" or "Err".
        which: &'static str,
    },

    /// Map key must be String
    MapNonStringKey {
        /// The map type.
        type_name: &'static str,
    },

    /// Budget exceeded (too many fields or nesting too deep)
    BudgetExceeded {
        /// The type that exceeded the budget.
        type_name: &'static str,
        /// Why the budget was exceeded.
        reason: &'static str,
    },

    /// JIT builder failed to initialize
    JitBuilderFailed {
        /// The error message from Cranelift.
        error: String,
    },

    /// Compilation of specific deserializer failed
    CompilationFailed {
        /// The type being compiled.
        type_name: &'static str,
        /// The compilation stage that failed.
        stage: &'static str,
    },

    /// Finalization of JIT module failed
    FinalizationFailed {
        /// The type being finalized.
        type_name: &'static str,
        /// The error message.
        error: String,
    },
}

impl std::fmt::Display for Tier2Incompatibility {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Not64BitPlatform => {
                write!(
                    f,
                    "Tier-2 JIT requires 64-bit platform (for ABI bit-packing)"
                )
            }
            Self::UnrecognizedShapeType { type_name } => {
                write!(
                    f,
                    "type `{}` is not a supported shape (must be struct, enum, Vec, or HashMap)",
                    type_name
                )
            }
            Self::TupleStructWithMapFormat { type_name } => {
                write!(
                    f,
                    "type `{}` is a tuple/unit struct, which requires positional format (e.g., postcard), not map-based format (e.g., JSON)",
                    type_name
                )
            }
            Self::FieldHasCustomDefault {
                type_name,
                field_name,
            } => {
                write!(
                    f,
                    "field `{}::{}` has a custom default, which is not supported in Tier-2 JIT (use Option<T> instead)",
                    type_name, field_name
                )
            }
            Self::UnsupportedFieldType {
                type_name,
                field_name,
                field_type,
            } => {
                write!(
                    f,
                    "field `{}::{}` has unsupported type `{}` (supported: scalars, String, Option<T>, Vec<T>, HashMap<String, V>, nested structs)",
                    type_name, field_name, field_type
                )
            }
            Self::UnsupportedFlattenType {
                type_name,
                field_name,
            } => {
                write!(
                    f,
                    "flattened field `{}::{}` must be a struct, enum, or HashMap<String, V>",
                    type_name, field_name
                )
            }
            Self::FlattenedMapNonStringKey {
                type_name,
                field_name,
            } => {
                write!(
                    f,
                    "flattened map `{}::{}` must have String keys",
                    type_name, field_name
                )
            }
            Self::UnsupportedEnumRepr { type_name, repr } => {
                write!(
                    f,
                    "enum `{}` has unsupported repr `{}` (use #[repr(C)] or explicit integer repr like #[repr(u8)])",
                    type_name, repr
                )
            }
            Self::EnumVariantNoDiscriminant {
                type_name,
                variant_name,
            } => {
                write!(
                    f,
                    "enum `{}` variant `{}` has no discriminant value (required for JIT)",
                    type_name, variant_name
                )
            }
            Self::UnsupportedEnumVariantField {
                type_name,
                variant_name,
                field_name,
            } => {
                write!(
                    f,
                    "enum `{}::{}` field `{}` has unsupported type (supported: scalars, String, structs)",
                    type_name, variant_name, field_name
                )
            }
            Self::FlattenedEnumUnitVariant {
                type_name,
                variant_name,
            } => {
                write!(
                    f,
                    "flattened enum `{}` variant `{}` is a unit variant, which is not supported (flattened variants must have payload)",
                    type_name, variant_name
                )
            }
            Self::EnumRequiresPositionalFormat { type_name } => {
                write!(
                    f,
                    "enum `{}` requires positional format (e.g., postcard); map-based formats (e.g., JSON) only support enums as struct fields",
                    type_name
                )
            }
            Self::UnsupportedResultType { type_name, which } => {
                write!(
                    f,
                    "Result type `{}` has unsupported {} type",
                    type_name, which
                )
            }
            Self::MapNonStringKey { type_name } => {
                write!(
                    f,
                    "map `{}` must have String keys (other key types not supported)",
                    type_name
                )
            }
            Self::BudgetExceeded { type_name, reason } => {
                write!(
                    f,
                    "type `{}` exceeds Tier-2 budget: {} (configure via FACET_TIER2_MAX_FIELDS or FACET_TIER2_MAX_NESTING)",
                    type_name, reason
                )
            }
            Self::JitBuilderFailed { error } => {
                write!(f, "JIT builder initialization failed: {}", error)
            }
            Self::CompilationFailed { type_name, stage } => {
                write!(
                    f,
                    "compilation failed for `{}` at stage: {}",
                    type_name, stage
                )
            }
            Self::FinalizationFailed { type_name, error } => {
                write!(f, "JIT finalization failed for `{}`: {}", type_name, error)
            }
        }
    }
}

impl std::error::Error for Tier2Incompatibility {}

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
// Re-export version that returns the reason on failure (for crates without fallback)
pub use cache::get_format_deserializer_with_reason;

// Re-export FormatJitParser from crate root for convenience
pub use crate::FormatJitParser;

// Re-export utility functions for format crates
pub use format::{c_call_conv, make_c_sig};

// Re-export Cranelift types for format crates implementing JitFormat
pub use cranelift::codegen::ir::BlockArg;
pub use cranelift::codegen::ir::{ExtFuncData, ExternalName, SigRef, Type, UserExternalName};
pub use cranelift::codegen::isa::CallConv;
pub use cranelift::prelude::{
    AbiParam, Block, FunctionBuilder, InstBuilder, IntCC, MemFlags, Signature, StackSlotData,
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
            jit_debug!(
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
            jit_debug!(
                "✗ Tier-2 RUNTIME UNSUPPORTED for {}",
                std::any::type_name::<T>()
            );
            None
        }
        Err(e) => {
            // Runtime error (parse error, not unsupported)
            TIER2_RUNTIME_ERROR.fetch_add(1, Ordering::Relaxed);
            jit_debug!("✗ Tier-2 RUNTIME ERROR for {}", std::any::type_name::<T>());
            Some(Err(e))
        }
    }
}

/// Error type for Tier-2 format JIT deserialization without fallback.
#[derive(Debug)]
pub enum Tier2DeserializeError<E> {
    /// Parser has buffered state (no JIT position available)
    ParserHasBufferedState,
    /// Type is not Tier-2 compatible (with detailed reason)
    Incompatible(Tier2Incompatibility),
    /// Runtime deserialization error (parse error)
    Deserialize(DeserializeError<E>),
}

impl<E: std::fmt::Display> std::fmt::Display for Tier2DeserializeError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ParserHasBufferedState => {
                write!(f, "Tier-2 JIT unavailable: parser has buffered state")
            }
            Self::Incompatible(reason) => {
                write!(f, "Tier-2 JIT unavailable: {}", reason)
            }
            Self::Deserialize(e) => write!(f, "{}", e),
        }
    }
}

impl<E: std::fmt::Debug + std::fmt::Display> std::error::Error for Tier2DeserializeError<E> {}

/// Deserialize using Tier-2 format JIT, returning the reason on failure.
///
/// This is like [`try_deserialize_format`] but returns `Result` instead of `Option`,
/// providing detailed information about why Tier-2 is not available.
///
/// Use this for format crates that have NO fallback (like facet-msgpack).
///
/// Returns:
/// - `Ok(value)` on successful deserialization
/// - `Err(ParserHasBufferedState)` if the parser can't provide raw input
/// - `Err(Incompatible(reason))` if the type is not Tier-2 compatible
/// - `Err(Deserialize(e))` if parsing failed
pub fn try_deserialize_format_with_reason<'de, T, P>(
    parser: &mut P,
) -> Result<T, Tier2DeserializeError<P::Error>>
where
    T: Facet<'de>,
    P: FormatJitParser<'de>,
{
    // Check if parser position is available (no buffered state)
    if parser.jit_pos().is_none() {
        return Err(Tier2DeserializeError::ParserHasBufferedState);
    }

    // Get or compile the Tier-2 deserializer with reason on failure
    let key = (T::SHAPE.id, ConstTypeId::of::<P>());
    let compiled = cache::get_or_compile_format_with_reason::<T, P>(key)
        .map_err(Tier2DeserializeError::Incompatible)?;

    // Execute the compiled deserializer
    compiled
        .deserialize(parser)
        .map_err(Tier2DeserializeError::Deserialize)
}

/// Check if a type can use Tier-2 format JIT.
///
/// Returns `true` for types that can be deserialized via format-specific
/// byte parsing (currently `Vec<scalar>` types).
///
/// Note: This uses a conservative default (Map encoding). For format-specific
/// checks, use [`is_format_jit_compatible_for`] instead.
pub fn is_format_jit_compatible<'a, T: Facet<'a>>() -> bool {
    format_compiler::ensure_format_jit_compatible(T::SHAPE, std::any::type_name::<T>()).is_ok()
}

/// Ensure a type can use Tier-2 format JIT, returning the reason if not.
///
/// This is like [`is_format_jit_compatible`] but returns detailed information
/// about why the type is not compatible.
pub fn ensure_format_jit_compatible<'a, T: Facet<'a>>() -> Result<(), Tier2Incompatibility> {
    format_compiler::ensure_format_jit_compatible(T::SHAPE, std::any::type_name::<T>())
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
    format_compiler::ensure_format_jit_compatible_with_encoding(
        T::SHAPE,
        F::STRUCT_ENCODING,
        std::any::type_name::<T>(),
    )
    .is_ok()
}

/// Ensure a type can use Tier-2 format JIT for a specific format, returning the reason if not.
///
/// This is like [`is_format_jit_compatible_for`] but returns detailed information
/// about why the type is not compatible.
pub fn ensure_format_jit_compatible_for<'a, T: Facet<'a>, F: JitFormat>()
-> Result<(), Tier2Incompatibility> {
    format_compiler::ensure_format_jit_compatible_with_encoding(
        T::SHAPE,
        F::STRUCT_ENCODING,
        std::any::type_name::<T>(),
    )
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
    jit_debug!("Attempting Tier-2 for {}", std::any::type_name::<T>());

    if let Some(result) = try_deserialize_format::<T, P>(parser) {
        TIER2_SUCCESSES.fetch_add(1, Ordering::Relaxed);
        jit_debug!("✓ Tier-2 USED for {}", std::any::type_name::<T>());
        return Some(result);
    }

    // Fall back to Tier-1
    jit_debug!(
        "Tier-2 unavailable, falling back to Tier-1 for {}",
        std::any::type_name::<T>()
    );
    let result = try_deserialize::<T, P>(parser);
    if result.is_some() {
        TIER1_USES.fetch_add(1, Ordering::Relaxed);
        jit_debug!("✓ Tier-1 USED for {}", std::any::type_name::<T>());
    } else {
        jit_debug!(
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
