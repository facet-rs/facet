//! Tier-2 Format JIT Compiler
//!
//! This module compiles deserializers that parse bytes directly using format-specific
//! IR generation, bypassing the event abstraction for maximum performance.
//!
//! ## ABI Contract
//!
//! ### Compiled Function Signature
//!
//! All Tier-2 compiled functions share this signature:
//! ```ignore
//! unsafe extern "C" fn(
//!     input_ptr: *const u8,  // Pointer to input byte slice
//!     len: usize,            // Length of input slice
//!     pos: usize,            // Starting cursor position
//!     out: *mut u8,          // Pointer to output value (uninitialized)
//!     scratch: *mut JitScratch, // Error/state scratch buffer
//! ) -> isize
//! ```
//!
//! ### Return Value
//!
//! - `>= 0`: Success - returns new cursor position after parsing
//! - `< 0`: Failure - error code; details written to `scratch`
//!
//! ### Error Handling
//!
//! On failure (return < 0), the scratch buffer contains:
//! - `error_code` field: Format-specific error code or `T2_ERR_UNSUPPORTED` (-1)
//! - `error_pos` field: Cursor position where error occurred
//! - `output_initialized` field: false (output is NOT valid on error)
//!
//! The compiled function MUST NOT partially initialize the output on error.
//!
//! ### Output Initialization
//!
//! The `out` parameter points to `MaybeUninit<T>`. The compiled function MUST:
//! - Fully initialize `out` before returning success (>= 0)
//! - NOT touch `out` or leave it partially initialized on error (< 0)
//!
//! The caller will use `output_initialized` to determine if `out` is valid.

use std::collections::HashMap;
use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::sync::Arc;

use cranelift::codegen::ir::FuncRef;
use cranelift::prelude::*;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::FuncId;

use facet_core::{Def, Facet, Shape, Type, UserType};

use super::Tier2Incompatibility;
use super::format::{JitFormat, JitScratch, StructEncoding, make_c_sig};
use super::helpers;
use super::jit_debug;
use crate::DeserializeError;
use crate::jit::FormatJitParser;

mod support;
pub use support::*;

mod map_format_deserializer;
use map_format_deserializer::*;

mod struct_format_deserializer;
use struct_format_deserializer::*;

mod struct_positional_deserializer;
use struct_positional_deserializer::*;

mod enum_positional_deserializer;
use enum_positional_deserializer::*;

mod list_format_deserializer;
use list_format_deserializer::*;

fn tier2_call_sig(module: &mut JITModule, pointer_type: cranelift::prelude::Type) -> Signature {
    let mut s = make_c_sig(module);
    s.params.push(AbiParam::new(pointer_type)); // input_ptr
    s.params.push(AbiParam::new(pointer_type)); // len
    s.params.push(AbiParam::new(pointer_type)); // pos
    s.params.push(AbiParam::new(pointer_type)); // out
    s.params.push(AbiParam::new(pointer_type)); // scratch
    s.returns.push(AbiParam::new(pointer_type)); // isize
    s
}

fn func_addr_value(
    builder: &mut FunctionBuilder,
    pointer_type: cranelift::prelude::Type,
    func_ref: FuncRef,
) -> Value {
    builder.ins().func_addr(pointer_type, func_ref)
}

/// Memoization table for compiled deserializers.
/// Maps shape pointer to compiled FuncId to avoid duplicate declarations.
type ShapeMemo = HashMap<*const Shape, FuncId>;

/// Budget limits for Tier-2 compilation to prevent pathological compile times.
/// Uses shape-based heuristics since IR inspection before finalization is difficult.
struct BudgetLimits {
    max_fields: usize,
    max_nesting_depth: usize,
}

impl BudgetLimits {
    fn from_env() -> Self {
        let max_fields = std::env::var("FACET_TIER2_MAX_FIELDS")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(100); // Conservative: 100 fields max

        let max_nesting_depth = std::env::var("FACET_TIER2_MAX_NESTING")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(10); // Conservative: 10 levels of nesting max

        Self {
            max_fields,
            max_nesting_depth,
        }
    }

    /// Check if a shape is within budget (shape-based heuristic).
    /// Returns `Ok(())` if within budget, or `Err` with reason if over budget.
    fn check_shape(
        &self,
        shape: &'static Shape,
        type_name: &'static str,
    ) -> Result<(), Tier2Incompatibility> {
        self.check_shape_recursive(shape, 0, type_name)
    }

    fn check_shape_recursive(
        &self,
        shape: &'static Shape,
        depth: usize,
        type_name: &'static str,
    ) -> Result<(), Tier2Incompatibility> {
        // Check nesting depth
        if depth > self.max_nesting_depth {
            jit_debug!(
                "[Tier-2 JIT] Budget exceeded: nesting depth {} > {} max",
                depth,
                self.max_nesting_depth
            );
            return Err(Tier2Incompatibility::BudgetExceeded {
                type_name,
                reason: "nesting depth exceeded",
            });
        }

        match &shape.def {
            Def::Option(opt) => self.check_shape_recursive(opt.t, depth, type_name),
            Def::List(list) => self.check_shape_recursive(list.t, depth + 1, type_name),
            _ => {
                // Check struct field count
                if let Type::User(UserType::Struct(struct_def)) = &shape.ty {
                    if struct_def.fields.len() > self.max_fields {
                        jit_debug!(
                            "[Tier-2 JIT] Budget exceeded: {} fields > {} max",
                            struct_def.fields.len(),
                            self.max_fields
                        );
                        return Err(Tier2Incompatibility::BudgetExceeded {
                            type_name,
                            reason: "too many fields",
                        });
                    }

                    // Check nested fields recursively
                    for field in struct_def.fields {
                        self.check_shape_recursive(field.shape(), depth + 1, type_name)?;
                    }
                }
                Ok(())
            }
        }
    }
}

// =============================================================================
// Tier-2 Error Codes
// =============================================================================

/// Format emitter returned unsupported (-1 from NoFormatJit)
pub const T2_ERR_UNSUPPORTED: i32 = -1;

// =============================================================================
// Cached Format Module
// =============================================================================

/// Owns a JITModule and its compiled function pointer.
/// This is stored in the cache and shared via Arc.
pub struct CachedFormatModule {
    /// The JIT module that owns the compiled code memory
    #[allow(dead_code)]
    module: JITModule,
    /// Pointer to the compiled function
    fn_ptr: *const u8,
}

impl CachedFormatModule {
    /// Create a new cached module.
    pub const fn new(module: JITModule, fn_ptr: *const u8) -> Self {
        Self { module, fn_ptr }
    }

    /// Get the function pointer.
    pub const fn fn_ptr(&self) -> *const u8 {
        self.fn_ptr
    }
}

// Safety: The compiled code is thread-safe (no mutable static state)
unsafe impl Send for CachedFormatModule {}
unsafe impl Sync for CachedFormatModule {}

// =============================================================================
// Compiled Format Deserializer
// =============================================================================

/// A Tier-2 compiled deserializer for a specific type and parser.
///
/// Unlike Tier-1 which uses vtable calls, Tier-2 parses bytes directly
/// via format-specific IR. Holds a reference to the cached module.
pub struct CompiledFormatDeserializer<T, P> {
    /// Direct function pointer (avoids Arc deref on every call)
    fn_ptr: *const u8,
    /// Shared reference to the cached module (keeps code memory alive)
    _cached: Arc<CachedFormatModule>,
    /// Phantom data for type safety
    _phantom: PhantomData<fn(&mut P) -> T>,
}

// Safety: The compiled code is thread-safe (no mutable static state)
unsafe impl<T, P> Send for CompiledFormatDeserializer<T, P> {}
unsafe impl<T, P> Sync for CompiledFormatDeserializer<T, P> {}

impl<T, P> CompiledFormatDeserializer<T, P> {
    /// Create from a cached module.
    pub fn from_cached(cached: Arc<CachedFormatModule>) -> Self {
        // Cache the fn_ptr directly to avoid Arc deref on every call
        let fn_ptr = cached.fn_ptr();
        Self {
            fn_ptr,
            _cached: cached,
            _phantom: PhantomData,
        }
    }

    /// Get the raw function pointer.
    #[inline(always)]
    pub const fn fn_ptr(&self) -> *const u8 {
        self.fn_ptr
    }
}

impl<'de, T: Facet<'de>, P: FormatJitParser<'de>> CompiledFormatDeserializer<T, P> {
    /// Execute the compiled deserializer.
    ///
    /// Returns the deserialized value and updates the parser's cursor position.
    pub fn deserialize(&self, parser: &mut P) -> Result<T, DeserializeError> {
        // Get input slice and position from parser
        let input = parser.jit_input();
        let Some(pos) = parser.jit_pos() else {
            return Err(DeserializeError::unsupported(
                "Tier-2 JIT: parser has buffered state",
            ));
        };

        jit_debug!("[Tier-2] Executing: input_len={}, pos={}", input.len(), pos);

        // Create output storage
        let mut output: MaybeUninit<T> = MaybeUninit::uninit();

        // Create scratch space for error reporting
        let mut scratch = JitScratch::default();

        // Call the compiled function
        // Signature: fn(input_ptr, len, pos, out, scratch) -> isize
        type CompiledFn =
            unsafe extern "C" fn(*const u8, usize, usize, *mut u8, *mut JitScratch) -> isize;
        let fn_ptr = self.fn_ptr();
        let func: CompiledFn = unsafe { std::mem::transmute(fn_ptr) };

        jit_debug!("[Tier-2] Calling JIT function at {:p}", fn_ptr);
        let result = unsafe {
            func(
                input.as_ptr(),
                input.len(),
                pos,
                output.as_mut_ptr() as *mut u8,
                &mut scratch,
            )
        };
        jit_debug!("[Tier-2] JIT function returned: result={}", result);

        if result >= 0 {
            // Success: update parser position and return value
            let new_pos = result as usize;
            parser.jit_set_pos(new_pos);
            jit_debug!("[Tier-2] Success! new_pos={}", new_pos);
            Ok(unsafe { output.assume_init() })
        } else {
            // Error: check if it's "unsupported" (allows fallback) or a real parse error
            jit_debug!(
                "[Tier-2] Error: code={}, pos={}, output_initialized={}",
                scratch.error_code,
                scratch.error_pos,
                scratch.output_initialized
            );

            // If output was initialized (e.g., Vec was created), we must drop it to avoid leaks
            // SAFETY: Only List/Map deserializers should set output_initialized=1.
            // Struct deserializers must NOT set this flag because nested calls may fail,
            // leaving the struct partially initialized (UB to drop).
            if scratch.output_initialized != 0 {
                // Only drop for List/Map shapes (never structs)
                match T::SHAPE.def {
                    Def::List(_) | Def::Map(_) => {
                        // SAFETY: List/Map deserializers set output_initialized=1 after
                        // calling init, so output contains a valid value that needs dropping.
                        unsafe { output.assume_init_drop() };
                    }
                    _ => {
                        // Struct shapes should never set output_initialized=1
                        // If they do, it's a bug - but we can't safely drop
                        jit_debug!(
                            "[Tier-2] WARNING: Struct deserializer incorrectly set output_initialized=1"
                        );
                    }
                }
            }

            // T2_ERR_UNSUPPORTED means the format doesn't implement this operation
            // Return Unsupported so try_deserialize_format can convert to None and fallback
            if scratch.error_code == T2_ERR_UNSUPPORTED {
                return Err(DeserializeError::unsupported(
                    "Tier-2 format operation not implemented",
                ));
            }

            Err(parser.jit_error(input, scratch.error_pos, scratch.error_code))
        }
    }
}

// =============================================================================
// Tier-2 Compiler
// =============================================================================

/// Try to compile a Tier-2 format deserializer module.
///
/// Returns `Ok((JITModule, fn_ptr))` on success, or `Err(Tier2Incompatibility)` with
/// details about why the type is not Tier-2 compatible.
///
/// The JITModule must be kept alive for the function pointer to remain valid.
pub fn try_compile_format_module<'de, T, P>() -> Result<(JITModule, *const u8), Tier2Incompatibility>
where
    T: Facet<'de>,
    P: FormatJitParser<'de>,
{
    let type_name = std::any::type_name::<T>();
    let shape = T::SHAPE;

    // Use the encoding specified by the format
    let encoding = P::FormatJit::STRUCT_ENCODING;
    ensure_format_jit_compatible_with_encoding(shape, encoding, type_name)?;

    // Build the JIT module
    let builder = match JITBuilder::new(cranelift_module::default_libcall_names()) {
        Ok(b) => b,
        Err(e) => {
            jit_debug!("[Tier-2 JIT] JITBuilder::new failed: {:?}", e);
            return Err(Tier2Incompatibility::JitBuilderFailed {
                error: format!("{:?}", e),
            });
        }
    };

    let mut builder = builder;

    // Check budget limits before compilation to avoid expensive work on pathological shapes
    let budget = BudgetLimits::from_env();
    budget.check_shape(shape, type_name)?;

    // Register shared helpers
    register_helpers(&mut builder);

    // Register format-specific helpers
    P::FormatJit::register_helpers(&mut builder);

    let mut module = JITModule::new(builder);

    // Create memo table for shape compilation
    let mut memo = ShapeMemo::new();

    // Compile based on shape
    let func_id = if let Def::List(_) = &shape.def {
        match compile_list_format_deserializer::<P::FormatJit>(&mut module, shape, &mut memo) {
            Some(id) => id,
            None => {
                jit_debug!("[Tier-2 JIT] compile_list_format_deserializer returned None");
                return Err(Tier2Incompatibility::CompilationFailed {
                    type_name,
                    stage: "list deserializer",
                });
            }
        }
    } else if let Def::Map(_) = &shape.def {
        match compile_map_format_deserializer::<P::FormatJit>(&mut module, shape, &mut memo) {
            Some(id) => id,
            None => {
                jit_debug!("[Tier-2 JIT] compile_map_format_deserializer returned None");
                return Err(Tier2Incompatibility::CompilationFailed {
                    type_name,
                    stage: "map deserializer",
                });
            }
        }
    } else if let Type::User(UserType::Struct(_)) = &shape.ty {
        // Dispatch to map-based or positional struct compiler based on format encoding
        let func_id = match <P::FormatJit as JitFormat>::STRUCT_ENCODING {
            StructEncoding::Map => {
                compile_struct_format_deserializer::<P::FormatJit>(&mut module, shape, &mut memo)
            }
            StructEncoding::Positional => compile_struct_positional_deserializer::<P::FormatJit>(
                &mut module,
                shape,
                &mut memo,
            ),
        };
        match func_id {
            Some(id) => id,
            None => {
                jit_debug!("[Tier-2 JIT] compile_struct_format_deserializer returned None");
                return Err(Tier2Incompatibility::CompilationFailed {
                    type_name,
                    stage: "struct deserializer",
                });
            }
        }
    } else if let Type::User(UserType::Enum(_)) = &shape.ty {
        // Enum types - use dedicated enum deserializer for positional formats
        // For positional formats like postcard, enums are their own top-level type
        match compile_enum_positional_deserializer::<P::FormatJit>(&mut module, shape, &mut memo) {
            Some(id) => id,
            None => {
                jit_debug!("[Tier-2 JIT] compile_enum_positional_deserializer returned None");
                return Err(Tier2Incompatibility::CompilationFailed {
                    type_name,
                    stage: "enum deserializer",
                });
            }
        }
    } else {
        jit_debug!("[Tier-2 JIT] Unsupported shape type");
        return Err(Tier2Incompatibility::UnrecognizedShapeType { type_name });
    };

    // Finalize and get the function pointer
    if let Err(e) = module.finalize_definitions() {
        jit_debug!("[Tier-2 JIT] finalize_definitions failed: {:?}", e);
        return Err(Tier2Incompatibility::FinalizationFailed {
            type_name,
            error: format!("{:?}", e),
        });
    }
    let fn_ptr = module.get_finalized_function(func_id);

    Ok((module, fn_ptr))
}

/// Register shared helper functions with the JIT module.
///
/// These are format-agnostic helpers (Vec operations, etc.).
/// Format-specific helpers are registered by `JitFormat::register_helpers`.
fn register_helpers(builder: &mut JITBuilder) {
    // Vec helpers (reuse from Tier-1)
    builder.symbol(
        "jit_vec_init_with_capacity",
        helpers::jit_vec_init_with_capacity as *const u8,
    );
    builder.symbol("jit_vec_push_bool", helpers::jit_vec_push_bool as *const u8);
    builder.symbol("jit_vec_push_u8", helpers::jit_vec_push_u8 as *const u8);
    builder.symbol("jit_vec_push_i64", helpers::jit_vec_push_i64 as *const u8);
    builder.symbol("jit_vec_push_u64", helpers::jit_vec_push_u64 as *const u8);
    builder.symbol("jit_vec_push_f64", helpers::jit_vec_push_f64 as *const u8);
    builder.symbol(
        "jit_vec_push_string",
        helpers::jit_vec_push_string as *const u8,
    );
    builder.symbol("jit_vec_set_len", helpers::jit_vec_set_len as *const u8);
    builder.symbol(
        "jit_vec_as_mut_ptr_typed",
        helpers::jit_vec_as_mut_ptr_typed as *const u8,
    );

    builder.symbol(
        "jit_map_init_with_capacity",
        helpers::jit_map_init_with_capacity as *const u8,
    );

    // Tier-2 specific helpers
    builder.symbol(
        "jit_drop_owned_string",
        helpers::jit_drop_owned_string as *const u8,
    );
    builder.symbol(
        "jit_option_init_none",
        helpers::jit_option_init_none as *const u8,
    );
    builder.symbol(
        "jit_option_init_some_from_value",
        helpers::jit_option_init_some_from_value as *const u8,
    );
    builder.symbol(
        "jit_result_init_ok_from_value",
        helpers::jit_result_init_ok_from_value as *const u8,
    );
    builder.symbol(
        "jit_result_init_err_from_value",
        helpers::jit_result_init_err_from_value as *const u8,
    );
    builder.symbol("jit_drop_in_place", helpers::jit_drop_in_place as *const u8);
    builder.symbol("jit_write_string", helpers::jit_write_string as *const u8);
    builder.symbol("jit_memcpy", helpers::jit_memcpy as *const u8);
    builder.symbol(
        "jit_write_error_string",
        helpers::jit_write_error_string as *const u8,
    );
}

/// Element type for Tier-2 list codegen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FormatListElementKind {
    Bool,
    U8, // Raw byte (not varint in postcard)
    I64,
    U64,
    F64,
    String,
    Struct(&'static Shape),
    List(&'static Shape),
    Map(&'static Shape),
}

impl FormatListElementKind {
    fn from_shape(shape: &'static Shape) -> Option<Self> {
        use facet_core::ScalarType;

        // Check for nested containers first (List/Map)
        if let Def::List(_) = &shape.def {
            return Some(Self::List(shape));
        }
        if let Def::Map(_) = &shape.def {
            return Some(Self::Map(shape));
        }

        // Check for String (not a scalar type)
        if shape.is_type::<String>() {
            return Some(Self::String);
        }

        // Check for struct types
        if matches!(shape.ty, Type::User(UserType::Struct(_))) {
            return Some(Self::Struct(shape));
        }

        // Then check scalar types
        let scalar_type = shape.scalar_type()?;
        match scalar_type {
            ScalarType::Bool => Some(Self::Bool),
            ScalarType::U8 => Some(Self::U8), // U8 is special (raw byte in binary formats)
            ScalarType::I8 | ScalarType::I16 | ScalarType::I32 | ScalarType::I64 => Some(Self::I64),
            ScalarType::U16 | ScalarType::U32 | ScalarType::U64 => Some(Self::U64),
            ScalarType::F32 | ScalarType::F64 => Some(Self::F64),
            ScalarType::String => Some(Self::String),
            _ => None,
        }
    }
}

/// Field codegen information for struct compilation.
#[derive(Debug)]
struct FieldCodegenInfo {
    /// Serialized name to match in the input
    name: &'static str,
    /// Byte offset within the struct
    offset: usize,
    /// Field shape for recursive compilation
    shape: &'static Shape,
    /// Is this field `Option<T>`?
    is_option: bool,
    /// If not Option and no default, this is required - track with this bit index
    required_bit_index: Option<u8>,
}

/// Metadata for a flattened enum variant.
struct FlattenedVariantInfo {
    /// Variant name (e.g., "Password") - this becomes a dispatch key
    variant_name: &'static str,
    /// Byte offset of the enum field within the parent struct
    enum_field_offset: usize,
    /// Variant discriminant value (for #[repr(C)] enums)
    discriminant: usize,
    /// Payload struct shape (for recursive deserialization)
    payload_shape: &'static Shape,
    /// Byte offset of the payload within the enum (accounts for discriminant size/alignment)
    payload_offset_in_enum: usize,
    /// Bit index for tracking whether this enum has been set (shared by all variants of same enum)
    enum_seen_bit_index: u8,
}

/// Metadata for a flattened map field (for capturing unknown keys).
struct FlattenedMapInfo {
    /// Byte offset of the HashMap field within the parent struct
    map_field_offset: usize,
    /// Value type shape (for HashMap<String, V>)
    value_shape: &'static Shape,
    /// Value element kind (validated to be Tier-2 compatible)
    value_kind: FormatListElementKind,
}

/// Dispatch target for struct key matching.
enum DispatchTarget {
    /// Normal struct field (index into field_infos)
    Field(usize),
    /// Flattened enum variant (index into flatten_variants)
    FlattenEnumVariant(usize),
}

/// Key dispatch strategy for field name matching.
#[derive(Debug)]
enum KeyDispatchStrategy {
    /// Inline key matching - matches `"key":` directly from input
    /// Most efficient for small structs with short keys (≤5 chars)
    Inline,
    /// Linear scan for small structs (< 10 fields) with longer keys
    Linear,
    /// Prefix-based switch for larger structs
    PrefixSwitch { prefix_len: usize },
}

/// Compute a prefix value from a field name for dispatch switching.
/// Returns (prefix_u64, actual_len_used) where actual_len_used ≤ 8.
fn compute_field_prefix(name: &str, prefix_len: usize) -> (u64, usize) {
    let bytes = name.as_bytes();
    let actual_len = bytes.len().min(prefix_len);
    let mut prefix: u64 = 0;

    for (i, &byte) in bytes.iter().take(actual_len).enumerate() {
        prefix |= (byte as u64) << (i * 8);
    }

    (prefix, actual_len)
}

/// Key-colon pattern for inline matching.
/// For short keys (≤5 chars), only `pattern1` is used.
/// For longer keys (6-13 chars), both patterns are used.
#[derive(Clone, Copy, Debug)]
struct KeyColonPattern {
    /// First u64 pattern (always used)
    pattern1: u64,
    /// Length of first pattern in bytes (1-8)
    pattern1_len: usize,
    /// Second u64 pattern (only for keys > 5 chars)
    pattern2: u64,
    /// Length of second pattern in bytes (0-8)
    pattern2_len: usize,
    /// Total pattern length (pattern1_len + pattern2_len)
    total_len: usize,
}

/// Compute key-colon pattern for inline key matching.
/// Supports keys up to 13 chars (pattern up to 16 bytes, using two u64 loads).
/// For keys ≤5 chars, only pattern1 is needed.
/// For keys 6-13 chars, both pattern1 and pattern2 are needed.
/// For keys >13 chars, returns None.
fn compute_key_colon_pattern_extended(name: &str) -> Option<KeyColonPattern> {
    let bytes = name.as_bytes();
    let total_len = bytes.len() + 3; // " + key + " + :

    if total_len > 16 {
        return None; // Pattern won't fit in 16 bytes (two u64s)
    }

    // Build the full pattern as bytes
    let mut full_pattern = [0u8; 16];
    full_pattern[0] = b'"';
    full_pattern[1..=bytes.len()].copy_from_slice(bytes);
    full_pattern[bytes.len() + 1] = b'"';
    full_pattern[bytes.len() + 2] = b':';

    // Convert first 8 bytes to u64 (little-endian)
    let pattern1_len = total_len.min(8);
    let pattern1 = u64::from_le_bytes([
        full_pattern[0],
        full_pattern[1],
        full_pattern[2],
        full_pattern[3],
        full_pattern[4],
        full_pattern[5],
        full_pattern[6],
        full_pattern[7],
    ]);

    // Convert next 8 bytes to u64 (little-endian) if needed
    let pattern2_len = total_len.saturating_sub(8);
    let pattern2 = if pattern2_len > 0 {
        u64::from_le_bytes([
            full_pattern[8],
            full_pattern[9],
            full_pattern[10],
            full_pattern[11],
            full_pattern[12],
            full_pattern[13],
            full_pattern[14],
            full_pattern[15],
        ])
    } else {
        0
    };

    Some(KeyColonPattern {
        pattern1,
        pattern1_len,
        pattern2,
        pattern2_len,
        total_len,
    })
}

/// Field info for positional struct deserialization.
struct PositionalFieldInfo {
    name: &'static str,
    offset: usize,
    #[allow(dead_code)]
    shape: &'static Shape,
    kind: PositionalFieldKind,
}

/// Field type classification for positional struct deserialization.
#[derive(Clone, Debug)]
enum PositionalFieldKind {
    Bool,
    U8,
    I8,
    I64(facet_core::ScalarType),
    U64(facet_core::ScalarType),
    F32,
    F64,
    String,
    Option(&'static facet_core::OptionDef),
    Result(&'static facet_core::ResultDef),
    Struct(&'static Shape),
    List(&'static Shape),
    Map(&'static Shape),
    Enum(&'static Shape),
}

/// Classify a field shape for positional deserialization.
fn classify_positional_field(shape: &'static Shape) -> Option<PositionalFieldKind> {
    use facet_core::ScalarType;

    // Check for Option first
    if let Def::Option(opt_def) = &shape.def {
        return Some(PositionalFieldKind::Option(opt_def));
    }

    // Check for Result
    if let Def::Result(result_def) = &shape.def {
        return Some(PositionalFieldKind::Result(result_def));
    }

    // Check for List
    if let Def::List(_) = &shape.def {
        return Some(PositionalFieldKind::List(shape));
    }

    // Check for Map
    if let Def::Map(_) = &shape.def {
        return Some(PositionalFieldKind::Map(shape));
    }

    // Check for Enum
    if matches!(shape.ty, Type::User(UserType::Enum(_))) {
        return Some(PositionalFieldKind::Enum(shape));
    }

    // Check for Struct
    if matches!(shape.ty, Type::User(UserType::Struct(_))) {
        return Some(PositionalFieldKind::Struct(shape));
    }

    // Check for String
    if shape.is_type::<String>() {
        return Some(PositionalFieldKind::String);
    }

    // Check scalar types
    let scalar_type = shape.scalar_type()?;
    match scalar_type {
        ScalarType::Bool => Some(PositionalFieldKind::Bool),
        ScalarType::U8 => Some(PositionalFieldKind::U8),
        ScalarType::I8 => Some(PositionalFieldKind::I8),
        ScalarType::I16 | ScalarType::I32 | ScalarType::I64 => {
            Some(PositionalFieldKind::I64(scalar_type))
        }
        ScalarType::U16 | ScalarType::U32 | ScalarType::U64 => {
            Some(PositionalFieldKind::U64(scalar_type))
        }
        ScalarType::F32 => Some(PositionalFieldKind::F32),
        ScalarType::F64 => Some(PositionalFieldKind::F64),
        _ => None,
    }
}
