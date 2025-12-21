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

use cranelift::prelude::*;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, Linkage, Module};

use facet_core::{Def, Facet, Shape, StructType, Type, UserType};

use super::format::{
    JIT_SCRATCH_ERROR_CODE_OFFSET, JIT_SCRATCH_ERROR_POS_OFFSET,
    JIT_SCRATCH_OUTPUT_INITIALIZED_OFFSET, JitCursor, JitFormat, JitScratch,
};
use super::helpers;
use super::jit_debug;
use crate::DeserializeError;
use crate::jit::FormatJitParser;

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
    /// Returns true if within budget, false if over budget.
    fn check_shape(&self, shape: &'static Shape) -> bool {
        self.check_shape_recursive(shape, 0)
    }

    fn check_shape_recursive(&self, shape: &'static Shape, depth: usize) -> bool {
        // Check nesting depth
        if depth > self.max_nesting_depth {
            #[cfg(debug_assertions)]
            jit_debug!(
                "[Tier-2 JIT] Budget exceeded: nesting depth {} > {} max",
                depth,
                self.max_nesting_depth
            );
            return false;
        }

        match &shape.def {
            Def::Option(opt) => self.check_shape_recursive(opt.t, depth),
            Def::List(list) => self.check_shape_recursive(list.t, depth + 1),
            _ => {
                // Check struct field count
                if let Type::User(UserType::Struct(struct_def)) = &shape.ty {
                    if struct_def.fields.len() > self.max_fields {
                        #[cfg(debug_assertions)]
                        jit_debug!(
                            "[Tier-2 JIT] Budget exceeded: {} fields > {} max",
                            struct_def.fields.len(),
                            self.max_fields
                        );
                        return false;
                    }

                    // Check nested fields recursively
                    for field in struct_def.fields {
                        if !self.check_shape_recursive(field.shape(), depth + 1) {
                            return false;
                        }
                    }
                }
                true
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
    pub fn new(module: JITModule, fn_ptr: *const u8) -> Self {
        Self { module, fn_ptr }
    }

    /// Get the function pointer.
    pub fn fn_ptr(&self) -> *const u8 {
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
    pub fn fn_ptr(&self) -> *const u8 {
        self.fn_ptr
    }
}

impl<'de, T: Facet<'de>, P: FormatJitParser<'de>> CompiledFormatDeserializer<T, P> {
    /// Execute the compiled deserializer.
    ///
    /// Returns the deserialized value and updates the parser's cursor position.
    pub fn deserialize(&self, parser: &mut P) -> Result<T, DeserializeError<P::Error>> {
        // Get input slice and position from parser
        let input = parser.jit_input();
        let Some(pos) = parser.jit_pos() else {
            return Err(DeserializeError::Unsupported(
                "Tier-2 JIT: parser has buffered state".into(),
            ));
        };

        jit_debug!("[Tier-2] Executing: input_len={}, pos={}", input.len(), pos);

        // Create output storage
        let mut output: MaybeUninit<T> = MaybeUninit::uninit();

        // Create scratch space for error reporting
        let mut scratch = JitScratch {
            error_code: 0,
            error_pos: 0,
            output_initialized: 0,
        };

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
                return Err(DeserializeError::Unsupported(
                    "Tier-2 format operation not implemented".into(),
                ));
            }

            let err = parser.jit_error(input, scratch.error_pos, scratch.error_code);
            Err(DeserializeError::Parser(err))
        }
    }
}

// =============================================================================
// Tier-2 Compatibility Check
// =============================================================================

/// Check if a shape is compatible with Tier-2 format JIT.
///
/// For MVP, supports:
/// - `Vec<T>` where T is bool
///
/// Note: Tier-2 is only available on 64-bit platforms due to ABI constraints
/// (bit-packing in return values assumes 64-bit pointers).
pub fn is_format_jit_compatible(shape: &'static Shape) -> bool {
    // Tier-2 requires 64-bit for ABI (bit-63 packing in return values)
    #[cfg(not(target_pointer_width = "64"))]
    {
        return false;
    }

    #[cfg(target_pointer_width = "64")]
    {
        use facet_core::ScalarType;

        // Check for Vec<T> types
        if let Def::List(list_def) = &shape.def {
            return is_format_jit_element_supported(list_def.t);
        }

        // Check for HashMap<String, V> types
        if let Def::Map(map_def) = &shape.def {
            // Key must be String
            if map_def.k.scalar_type() != Some(ScalarType::String) {
                return false;
            }
            // Value must be a supported element type
            return is_format_jit_element_supported(map_def.v);
        }

        // Check for simple struct types
        if let Type::User(UserType::Struct(struct_def)) = &shape.ty {
            let supported = is_format_jit_struct_supported(struct_def);
            if !supported {
                jit_diag!("Struct incompatible (see earlier field diagnostics)");
            }
            return supported;
        }

        jit_diag!("Shape type not recognized as compatible");
        false
    }
}

/// Check if a struct type is supported for Tier-2 (simple struct subset).
///
/// Simple struct subset:
/// - Named fields only (StructKind::Struct)
/// - Flatten supported for: structs, enums, and `HashMap<String, V>`
/// - ≤64 fields (for bitset tracking)
/// - Fields can be: scalars, `Option<T>`, `Vec<T>`, `HashMap<String, V>`, or nested simple structs
/// - No custom defaults (only Option pre-initialization)
fn is_format_jit_struct_supported(struct_def: &StructType) -> bool {
    use facet_core::StructKind;

    // Only named structs (not tuples or unit)
    if !matches!(struct_def.kind, StructKind::Struct) {
        return false;
    }

    // Note: We don't check total field count here because:
    // 1. Flattened structs expand to more fields, so raw count is misleading
    // 2. Only *required* fields need tracking bits, Option fields are free
    // 3. The accurate check happens in compile_struct_format_deserializer
    //    which counts actual tracking bits (required fields + enum seen bits)

    // Check all fields are compatible
    for field in struct_def.fields {
        // Flatten is supported for enums, structs, and HashMap<String, V>
        if field.is_flattened() {
            let field_shape = field.shape();

            // Handle flattened HashMap<String, V>
            if let Def::Map(map_def) = &field_shape.def {
                // Validate key is String
                if map_def.k.scalar_type() != Some(facet_core::ScalarType::String) {
                    jit_diag!(
                        "Field '{}' is flattened map but key type is not String",
                        field.name
                    );
                    return false;
                }
                // Validate value type is supported (same check as map values)
                if !is_format_jit_element_supported(map_def.v) {
                    jit_diag!(
                        "Field '{}' is flattened map but value type not supported",
                        field.name
                    );
                    return false;
                }
                // Flattened map is OK - skip normal field type check and continue to next field
                continue;
            }

            // Handle flattened enum or struct
            match &field_shape.ty {
                facet_core::Type::User(facet_core::UserType::Enum(enum_type)) => {
                    // Check if it's a supported enum
                    if !is_format_jit_enum_supported(enum_type) {
                        jit_diag!("Field '{}' is flattened enum but not supported", field.name);
                        return false;
                    }
                    // Flattened enum is OK - skip normal field type check and continue to next field
                    continue;
                }
                facet_core::Type::User(facet_core::UserType::Struct(inner_struct)) => {
                    // Recursively check if the inner struct is supported
                    if !is_format_jit_struct_supported(inner_struct) {
                        jit_diag!(
                            "Field '{}' is flattened struct but inner struct not supported",
                            field.name
                        );
                        return false;
                    }
                    // Flattened struct is OK - skip normal field type check and continue to next field
                    continue;
                }
                _ => {
                    jit_diag!(
                        "Field '{}' is flattened but type is not enum, struct, or HashMap (not supported)",
                        field.name
                    );
                    return false;
                }
            }
        }

        // No custom defaults in simple subset (Option pre-init is OK)
        if field.has_default() {
            jit_diag!("Field '{}' has custom default (not supported)", field.name);
            return false;
        }

        // Field type must be supported (for normal, non-flattened fields)
        if !is_format_jit_field_type_supported(field.shape()) {
            jit_diag!(
                "Field '{}' has unsupported type: {:?}",
                field.name,
                field.shape().def
            );
            return false;
        }
    }

    true
}

/// Check if an enum is supported for Tier-2 JIT compilation (MVP).
///
/// MVP requirements:
/// - #[repr(C)] only
/// - All variants must be tuple variants with exactly one field
/// - Payload structs must be JIT-compatible
fn is_format_jit_enum_supported(enum_type: &facet_core::EnumType) -> bool {
    use facet_core::{BaseRepr, EnumRepr, StructKind};

    // Must be #[repr(C)]
    if enum_type.repr.base != BaseRepr::C {
        jit_diag!("Enum must be #[repr(C)]");
        return false;
    }

    // Verify discriminant representation is known
    // We support any explicit integer representation for the discriminant
    // The field offset will account for the discriminant size/alignment automatically
    match enum_type.enum_repr {
        EnumRepr::U8
        | EnumRepr::U16
        | EnumRepr::U32
        | EnumRepr::U64
        | EnumRepr::USize
        | EnumRepr::I8
        | EnumRepr::I16
        | EnumRepr::I32
        | EnumRepr::I64
        | EnumRepr::ISize => {
            // All explicit discriminant sizes are supported
            // The payload offset from variant.data.fields[0].offset already accounts for size/alignment
        }
        EnumRepr::RustNPO => {
            jit_diag!("Enum with niche/NPO optimization (Option-like) not supported");
            return false;
        }
    }

    // Check all variants are single-field tuple variants
    // Also verify all variants have consistent payload offset
    let mut expected_payload_offset: Option<usize> = None;

    for variant in enum_type.variants {
        // Must be tuple variant (not struct or unit)
        if !matches!(variant.data.kind, StructKind::TupleStruct) {
            jit_diag!(
                "Enum variant '{}' must be tuple variant (got {:?})",
                variant.name,
                variant.data.kind
            );
            return false;
        }

        // Must have exactly one field
        if variant.data.fields.len() != 1 {
            jit_diag!(
                "Enum variant '{}' must have exactly one field, has {}",
                variant.name,
                variant.data.fields.len()
            );
            return false;
        }

        // Payload must be a supported struct
        let payload_shape = variant.data.fields[0].shape();
        if let facet_core::Type::User(facet_core::UserType::Struct(struct_def)) = &payload_shape.ty
        {
            if !is_format_jit_struct_supported(struct_def) {
                jit_diag!(
                    "Enum variant '{}' payload struct not supported",
                    variant.name
                );
                return false;
            }
        } else {
            jit_diag!(
                "Enum variant '{}' payload must be a struct, got {:?}",
                variant.name,
                payload_shape.ty
            );
            return false;
        }

        // Verify payload offset is consistent across all variants
        // For #[repr(C)], all variants should have the same offset for the payload field
        let payload_offset = variant.data.fields[0].offset;
        match expected_payload_offset {
            None => {
                expected_payload_offset = Some(payload_offset);
                jit_diag!(
                    "Enum variant '{}' payload offset: {} bytes",
                    variant.name,
                    payload_offset
                );
            }
            Some(expected) => {
                if payload_offset != expected {
                    jit_diag!(
                        "Enum variant '{}' has inconsistent payload offset: expected {}, got {}",
                        variant.name,
                        expected,
                        payload_offset
                    );
                    return false;
                }
            }
        }
    }

    true
}

/// Check if a field type is supported for Tier-2.
///
/// Supported types:
/// - Scalars (bool, integers, floats, String)
/// - `Option<T>` where T is supported
/// - `Vec<T>` where T is a supported element type (scalars, structs, nested Vec/Map)
/// - HashMap<String, V> where V is a supported element type
/// - Nested simple structs (recursive)
fn is_format_jit_field_type_supported(shape: &'static Shape) -> bool {
    use facet_core::ScalarType;

    // Check for Option<T>
    if let Def::Option(opt_def) = &shape.def {
        return is_format_jit_field_type_supported(opt_def.t);
    }

    // Check for Vec<T>
    if let Def::List(list_def) = &shape.def {
        return is_format_jit_element_supported(list_def.t);
    }

    // Check for HashMap<String, V>
    if let Def::Map(map_def) = &shape.def {
        // Key must be String
        if map_def.k.scalar_type() != Some(ScalarType::String) {
            return false;
        }
        // Value must be a supported element type
        return is_format_jit_element_supported(map_def.v);
    }

    // Check for scalars
    if let Some(scalar_type) = shape.scalar_type() {
        return matches!(
            scalar_type,
            ScalarType::Bool
                | ScalarType::I8
                | ScalarType::I16
                | ScalarType::I32
                | ScalarType::I64
                | ScalarType::U8
                | ScalarType::U16
                | ScalarType::U32
                | ScalarType::U64
                | ScalarType::F32
                | ScalarType::F64
                | ScalarType::String
        );
    }

    // Check for nested simple structs
    if let Type::User(UserType::Struct(struct_def)) = &shape.ty {
        return is_format_jit_struct_supported(struct_def);
    }

    // Check for enums (non-flattened)
    if let Type::User(UserType::Enum(enum_def)) = &shape.ty {
        return is_format_jit_enum_supported(enum_def);
    }

    false
}

/// Check if a Vec element type is supported for Tier-2.
fn is_format_jit_element_supported(elem_shape: &'static Shape) -> bool {
    use facet_core::ScalarType;

    if let Some(scalar_type) = elem_shape.scalar_type() {
        // All scalar types (including String) are supported with Tier-2 JIT.
        return matches!(
            scalar_type,
            ScalarType::Bool
                | ScalarType::I8
                | ScalarType::I16
                | ScalarType::I32
                | ScalarType::I64
                | ScalarType::U8
                | ScalarType::U16
                | ScalarType::U32
                | ScalarType::U64
                | ScalarType::F32
                | ScalarType::F64
                | ScalarType::String
        );
    }

    // Support nested Vec<Vec<T>> by recursively checking the inner element type
    if let Def::List(list_def) = &elem_shape.def {
        return is_format_jit_element_supported(list_def.t);
    }

    // Support nested HashMap<String, V> as Vec element
    if let Def::Map(map_def) = &elem_shape.def {
        // Key must be String
        if map_def.k.scalar_type() != Some(ScalarType::String) {
            return false;
        }
        // Value must be a supported element type (recursive check)
        return is_format_jit_element_supported(map_def.v);
    }

    // Support struct elements (Vec<struct>) - but only if the struct itself is Tier-2 compatible
    if let Type::User(UserType::Struct(struct_def)) = &elem_shape.ty {
        return is_format_jit_struct_supported(struct_def);
    }

    false
}

// =============================================================================
// Tier-2 Compiler
// =============================================================================

/// Try to compile a Tier-2 format deserializer module.
///
/// Returns `(JITModule, fn_ptr)` on success, `None` if the type is not Tier-2 compatible.
/// The JITModule must be kept alive for the function pointer to remain valid.
pub fn try_compile_format_module<'de, T, P>() -> Option<(JITModule, *const u8)>
where
    T: Facet<'de>,
    P: FormatJitParser<'de>,
{
    jit_diag!(
        "try_compile_format_module for {}",
        std::any::type_name::<T>()
    );
    let shape = T::SHAPE;

    if !is_format_jit_compatible(shape) {
        #[cfg(debug_assertions)]
        jit_debug!("[Tier-2 JIT] Shape not compatible");
        jit_diag!("Shape not compatible: {}", std::any::type_name::<T>());
        return None;
    }

    // Build the JIT module
    let builder = match JITBuilder::new(cranelift_module::default_libcall_names()) {
        Ok(b) => b,
        Err(e) => {
            jit_debug!("[Tier-2 JIT] JITBuilder::new failed: {:?}", e);
            jit_diag!("JITBuilder::new failed: {:?}", e);
            return None;
        }
    };

    let mut builder = builder;

    // Check budget limits before compilation to avoid expensive work on pathological shapes
    let budget = BudgetLimits::from_env();
    if !budget.check_shape(shape) {
        #[cfg(debug_assertions)]
        jit_debug!("[Tier-2 JIT] Shape exceeds budget, refusing compilation");
        jit_diag!(
            "Shape exceeds budget limits: {}",
            std::any::type_name::<T>()
        );
        return None;
    }

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
                #[cfg(debug_assertions)]
                jit_debug!("[Tier-2 JIT] compile_list_format_deserializer returned None");
                jit_diag!(
                    "compile_list_format_deserializer failed for {}",
                    std::any::type_name::<T>()
                );
                return None;
            }
        }
    } else if let Def::Map(_) = &shape.def {
        match compile_map_format_deserializer::<P::FormatJit>(&mut module, shape, &mut memo) {
            Some(id) => id,
            None => {
                #[cfg(debug_assertions)]
                jit_debug!("[Tier-2 JIT] compile_map_format_deserializer returned None");
                jit_diag!(
                    "compile_map_format_deserializer failed for {}",
                    std::any::type_name::<T>()
                );
                return None;
            }
        }
    } else if let Type::User(UserType::Struct(_)) = &shape.ty {
        match compile_struct_format_deserializer::<P::FormatJit>(&mut module, shape, &mut memo) {
            Some(id) => id,
            None => {
                #[cfg(debug_assertions)]
                jit_debug!("[Tier-2 JIT] compile_struct_format_deserializer returned None");
                jit_diag!(
                    "compile_struct_format_deserializer failed for {}",
                    std::any::type_name::<T>()
                );
                return None;
            }
        }
    } else {
        #[cfg(debug_assertions)]
        jit_debug!("[Tier-2 JIT] Unsupported shape type");
        jit_diag!("Unsupported shape type for {}", std::any::type_name::<T>());
        return None;
    };

    // Finalize and get the function pointer
    if let Err(e) = module.finalize_definitions() {
        #[cfg(debug_assertions)]
        jit_debug!("[Tier-2 JIT] finalize_definitions failed: {:?}", e);
        jit_diag!(
            "finalize_definitions failed for {}: {:?}",
            std::any::type_name::<T>(),
            e
        );
        return None;
    }
    let fn_ptr = module.get_finalized_function(func_id);

    Some((module, fn_ptr))
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

/// Compile a Tier-2 list deserializer.
///
/// Generates code that:
/// 1. Calls format helper for seq_begin
/// 2. Loops: check seq_is_end, parse element, push, seq_next
/// 3. Returns new position on success
///
/// This implementation directly calls format-specific helper functions
/// via symbol names provided by `JitFormat::helper_*()` methods.
/// The helper functions are registered via `JitFormat::register_helpers`.
fn compile_list_format_deserializer<F: JitFormat>(
    module: &mut JITModule,
    shape: &'static Shape,
    memo: &mut ShapeMemo,
) -> Option<FuncId> {
    // Check memo first - return cached FuncId if already compiled
    let shape_ptr = shape as *const Shape;
    if let Some(&func_id) = memo.get(&shape_ptr) {
        jit_diag!(
            "compile_list_format_deserializer: using memoized FuncId for shape {:p}",
            shape
        );
        return Some(func_id);
    }

    let Def::List(list_def) = &shape.def else {
        jit_debug!("[compile_list] Not a list");
        return None;
    };

    let elem_shape = list_def.t;
    let elem_kind = match FormatListElementKind::from_shape(elem_shape) {
        Some(k) => k,
        None => {
            jit_debug!("[compile_list] Element type not supported");
            return None;
        }
    };

    // Get Vec vtable functions
    let init_fn = match list_def.init_in_place_with_capacity() {
        Some(f) => f,
        None => {
            jit_debug!("[compile_list] No init_in_place_with_capacity");
            return None;
        }
    };
    let push_fn = match list_def.push() {
        Some(f) => f,
        None => {
            jit_debug!("[compile_list] No push fn");
            return None;
        }
    };

    let pointer_type = module.target_config().pointer_type();

    // Function signature: fn(input_ptr, len, pos, out, scratch) -> isize
    let sig = {
        let mut s = module.make_signature();
        s.params.push(AbiParam::new(pointer_type)); // input_ptr: *const u8
        s.params.push(AbiParam::new(pointer_type)); // len: usize
        s.params.push(AbiParam::new(pointer_type)); // pos: usize
        s.params.push(AbiParam::new(pointer_type)); // out: *mut u8
        s.params.push(AbiParam::new(pointer_type)); // scratch: *mut JitScratch
        s.returns.push(AbiParam::new(pointer_type)); // isize (new pos or error)
        s
    };

    // Vec helper signatures
    let sig_vec_init = {
        let mut s = module.make_signature();
        s.params.push(AbiParam::new(pointer_type)); // out
        s.params.push(AbiParam::new(pointer_type)); // capacity
        s.params.push(AbiParam::new(pointer_type)); // init_fn
        s
    };

    // Direct push signature: fn(vec_ptr: PtrMut, elem_ptr: PtrMut) -> ()
    // This is the actual ListPushFn signature - we call it directly via call_indirect
    // NOTE: PtrMut is a 16-byte struct (TaggedPtr + metadata), so each PtrMut becomes
    // TWO pointer-sized arguments in the C ABI. For thin pointers, metadata is 0.
    let sig_direct_push = {
        let mut s = module.make_signature();
        s.params.push(AbiParam::new(pointer_type)); // vec_ptr.ptr (TaggedPtr)
        s.params.push(AbiParam::new(pointer_type)); // vec_ptr.metadata
        s.params.push(AbiParam::new(pointer_type)); // elem_ptr.ptr (TaggedPtr)
        s.params.push(AbiParam::new(pointer_type)); // elem_ptr.metadata
        s
    };

    // Direct-fill helper signatures
    // jit_vec_set_len(vec_ptr, len, set_len_fn)
    let sig_vec_set_len = {
        let mut s = module.make_signature();
        s.params.push(AbiParam::new(pointer_type)); // vec_ptr
        s.params.push(AbiParam::new(pointer_type)); // len
        s.params.push(AbiParam::new(pointer_type)); // set_len_fn
        s
    };
    // jit_vec_as_mut_ptr_typed(vec_ptr, as_mut_ptr_typed_fn) -> *mut u8
    let sig_vec_as_mut_ptr_typed = {
        let mut s = module.make_signature();
        s.params.push(AbiParam::new(pointer_type)); // vec_ptr
        s.params.push(AbiParam::new(pointer_type)); // as_mut_ptr_typed_fn
        s.returns.push(AbiParam::new(pointer_type)); // *mut u8
        s
    };

    // Element size and alignment from actual element type, not from elem_kind
    // (elem_kind groups types: I64 includes i8/i16/i32/i64, U64 includes u16/u32/u64)
    let elem_layout = elem_shape.layout.sized_layout().ok()?;
    let elem_size = elem_layout.size() as u32;
    let elem_align_shift = elem_layout.align().trailing_zeros() as u8;

    // Declare Vec init helper function
    let vec_init_id =
        match module.declare_function("jit_vec_init_with_capacity", Linkage::Import, &sig_vec_init)
        {
            Ok(id) => id,
            Err(_e) => {
                jit_debug!(
                    "[compile_list] declare jit_vec_init_with_capacity failed: {:?}",
                    _e
                );
                return None;
            }
        };

    // Declare direct-fill helper functions
    let vec_set_len_id =
        match module.declare_function("jit_vec_set_len", Linkage::Import, &sig_vec_set_len) {
            Ok(id) => id,
            Err(_e) => {
                jit_debug!("[compile_list] declare jit_vec_set_len failed: {:?}", _e);
                return None;
            }
        };
    let vec_as_mut_ptr_typed_id = match module.declare_function(
        "jit_vec_as_mut_ptr_typed",
        Linkage::Import,
        &sig_vec_as_mut_ptr_typed,
    ) {
        Ok(id) => id,
        Err(_e) => {
            jit_debug!(
                "[compile_list] declare jit_vec_as_mut_ptr_typed failed: {:?}",
                _e
            );
            return None;
        }
    };

    // Get direct-fill functions from list_def (optional - may be None)
    let set_len_fn = list_def.set_len();
    let as_mut_ptr_typed_fn = list_def.as_mut_ptr_typed();
    // Direct-fill requires:
    // 1. Vec operations (set_len, as_mut_ptr_typed)
    // 2. Scalar element type
    // 3. Format provides accurate count (not delimiter-based like JSON)
    let use_direct_fill = set_len_fn.is_some()
        && as_mut_ptr_typed_fn.is_some()
        && F::PROVIDES_SEQ_COUNT
        && matches!(
            elem_kind,
            FormatListElementKind::Bool
                | FormatListElementKind::U8
                | FormatListElementKind::I64
                | FormatListElementKind::U64
        );

    // No need to declare push helper - we call push_fn directly via call_indirect
    // No format helper functions need to be declared

    // Declare our function with unique name based on shape address (avoids collisions)
    let func_name = format!("jit_deserialize_list_{:x}", shape as *const _ as usize);
    let func_id = match module.declare_function(&func_name, Linkage::Local, &sig) {
        Ok(id) => id,
        Err(_e) => {
            jit_debug!("[compile_list] declare {} failed: {:?}", func_name, _e);
            return None;
        }
    };

    // Insert into memo immediately after declaration (before IR build) to avoid recursion/cycles
    memo.insert(shape_ptr, func_id);
    jit_diag!(
        "compile_list_format_deserializer: memoized FuncId for shape {:p}",
        shape
    );

    let mut ctx = module.make_context();
    ctx.func.signature = sig;

    let mut builder_ctx = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut builder_ctx);

        // Import Vec init helper function
        let vec_init_ref = module.declare_func_in_func(vec_init_id, builder.func);
        let vec_set_len_ref = module.declare_func_in_func(vec_set_len_id, builder.func);
        let vec_as_mut_ptr_typed_ref =
            module.declare_func_in_func(vec_as_mut_ptr_typed_id, builder.func);
        // Import signature for direct push call (call_indirect)
        let sig_direct_push_ref = builder.import_signature(sig_direct_push);

        // Create blocks
        let entry = builder.create_block();
        let seq_begin = builder.create_block();
        let check_seq_begin_err = builder.create_block();
        let init_vec = builder.create_block();
        // Push-based path (for delimiter formats like JSON, or when count==0)
        let loop_check_end = builder.create_block();
        let check_is_end_err = builder.create_block();
        let check_is_end_value = builder.create_block();
        let parse_element = builder.create_block();
        let check_parse_err = builder.create_block();
        let push_element = builder.create_block();
        let seq_next = builder.create_block();
        let check_seq_next_err = builder.create_block();
        // Direct-fill path (for counted formats like postcard when count>0)
        let df_setup = builder.create_block();
        // Bulk copy path (for Vec<u8> when format supports it)
        let df_bulk_copy = builder.create_block();
        let df_bulk_copy_check_err = builder.create_block();
        // Element-by-element loop path
        let df_loop_check = builder.create_block();
        let df_parse = builder.create_block();
        let df_check_parse_err = builder.create_block();
        let df_store = builder.create_block();
        let df_finalize = builder.create_block();
        let success = builder.create_block();
        let error = builder.create_block();
        let nested_error_passthrough = builder.create_block();

        // Entry block: setup parameters
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);

        let input_ptr = builder.block_params(entry)[0];
        let len = builder.block_params(entry)[1];
        let pos_param = builder.block_params(entry)[2];
        let out_ptr = builder.block_params(entry)[3];
        let scratch_ptr = builder.block_params(entry)[4];

        // Create position variable (mutable)
        let pos_var = builder.declare_var(pointer_type);
        builder.def_var(pos_var, pos_param);

        // Variable to hold parsed value (type depends on element kind)
        let parsed_value_type = match elem_kind {
            FormatListElementKind::Bool | FormatListElementKind::U8 => types::I8,
            FormatListElementKind::I64 | FormatListElementKind::U64 => types::I64,
            FormatListElementKind::F64 => types::F64,
            FormatListElementKind::String => types::I64, // placeholder, not used for String
            FormatListElementKind::Struct(_) => types::I64, // placeholder, not used for Struct
            FormatListElementKind::List(_) => types::I64, // placeholder, not used for List
            FormatListElementKind::Map(_) => types::I64, // placeholder, not used for Map
        };
        let parsed_value_var = builder.declare_var(parsed_value_type);
        let zero_val = match elem_kind {
            FormatListElementKind::Bool | FormatListElementKind::U8 => {
                builder.ins().iconst(types::I8, 0)
            }
            FormatListElementKind::I64 | FormatListElementKind::U64 => {
                builder.ins().iconst(types::I64, 0)
            }
            FormatListElementKind::F64 => builder.ins().f64const(0.0),
            FormatListElementKind::String => builder.ins().iconst(types::I64, 0),
            FormatListElementKind::Struct(_) => builder.ins().iconst(types::I64, 0),
            FormatListElementKind::List(_) => builder.ins().iconst(types::I64, 0),
            FormatListElementKind::Map(_) => builder.ins().iconst(types::I64, 0),
        };
        builder.def_var(parsed_value_var, zero_val);

        // Variable for error code (used across blocks)
        let err_var = builder.declare_var(types::I32);
        let zero_i32 = builder.ins().iconst(types::I32, 0);
        builder.def_var(err_var, zero_i32);

        // Variable for is_end flag (used across blocks)
        let is_end_var = builder.declare_var(pointer_type);
        let zero_ptr = builder.ins().iconst(pointer_type, 0);
        builder.def_var(is_end_var, zero_ptr);

        // Store push_fn_ptr in a Variable since it's used in the loop body
        // (Cranelift SSA requires Variable for values used across loop boundaries)
        let push_fn_var = builder.declare_var(pointer_type);
        let push_fn_val = builder
            .ins()
            .iconst(pointer_type, push_fn as *const () as i64);
        builder.def_var(push_fn_var, push_fn_val);

        // Constants (used in entry or blocks directly reachable from entry)
        let init_fn_ptr = builder
            .ins()
            .iconst(pointer_type, init_fn as *const () as i64);
        let zero_cap = builder.ins().iconst(pointer_type, 0);

        // Allocate stack slot for sequence state if the format needs it
        let state_ptr = if F::SEQ_STATE_SIZE > 0 {
            // align_shift is log2(alignment), e.g. for 8-byte alignment: log2(8) = 3
            let align_shift = F::SEQ_STATE_ALIGN.trailing_zeros() as u8;
            let slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                F::SEQ_STATE_SIZE,
                align_shift,
            ));
            builder.ins().stack_addr(pointer_type, slot, 0)
        } else {
            builder.ins().iconst(pointer_type, 0)
        };

        // Allocate stack slot for element storage (used for inline push)
        let elem_slot = builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            elem_size,
            elem_align_shift,
        ));

        // Variable to hold element count from seq_begin (used for preallocation)
        let seq_count_var = builder.declare_var(pointer_type);
        builder.def_var(seq_count_var, zero_cap);

        builder.ins().jump(seq_begin, &[]);
        builder.seal_block(entry);

        // seq_begin: use inline IR for array start (no helper call!)
        builder.switch_to_block(seq_begin);

        // Create cursor for emit_seq_begin
        let mut cursor = JitCursor {
            input_ptr,
            len,
            pos: pos_var,
            ptr_type: pointer_type,
        };

        // Use inline IR for seq_begin
        // Returns (count, error) - count is used for Vec preallocation
        let format = F::default();
        let (seq_count, err_code) =
            format.emit_seq_begin(module, &mut builder, &mut cursor, state_ptr);

        // emit_seq_begin leaves us at its merge block and updates cursor.pos internally
        builder.def_var(err_var, err_code);
        builder.def_var(seq_count_var, seq_count);
        builder.ins().jump(check_seq_begin_err, &[]);
        builder.seal_block(seq_begin);

        // check_seq_begin_err
        builder.switch_to_block(check_seq_begin_err);
        let is_ok = builder.ins().icmp_imm(IntCC::Equal, err_code, 0);
        builder.ins().brif(is_ok, init_vec, &[], error, &[]);
        builder.seal_block(check_seq_begin_err);

        // init_vec: initialize Vec with capacity from seq_begin count
        // This preallocates for length-prefixed formats (postcard) and is 0 for
        // delimiter formats (JSON) where the count isn't known upfront
        builder.switch_to_block(init_vec);
        let capacity = builder.use_var(seq_count_var);
        builder
            .ins()
            .call(vec_init_ref, &[out_ptr, capacity, init_fn_ptr]);

        // Mark output as initialized so wrapper can drop on error
        let one_i8 = builder.ins().iconst(types::I8, 1);
        builder.ins().store(
            MemFlags::trusted(),
            one_i8,
            scratch_ptr,
            JIT_SCRATCH_OUTPUT_INITIALIZED_OFFSET,
        );

        // Branch to either direct-fill or push-based path
        if use_direct_fill {
            // For counted formats: if count > 0, use direct-fill; else success (empty array)
            let count_gt_zero = builder.ins().icmp_imm(IntCC::NotEqual, capacity, 0);
            builder
                .ins()
                .brif(count_gt_zero, df_setup, &[], success, &[]);
        } else {
            // For delimiter formats: always use push-based loop
            builder.ins().jump(loop_check_end, &[]);
        }
        builder.seal_block(init_vec);

        // loop_check_end: use inline IR for seq_is_end
        //
        // VALUE BOUNDARY INVARIANT (format-neutral):
        // At loop entry, cursor.pos is at a valid "value boundary" for the format.
        // This is maintained by format-specific emit_* methods:
        //   - emit_seq_begin leaves cursor ready for first element or end check
        //   - emit_seq_next advances past any element separator, leaving cursor
        //     ready for the next element or end check
        //   - emit_parse_* methods consume exactly one value
        //   - emit_seq_is_end checks (and consumes end marker if present)
        //
        // For delimiter formats (JSON): value boundary = after trivia
        // For counted formats (postcard): value boundary = at next byte (no trivia)
        //
        // Note: loop_check_end is NOT sealed here - it has a back edge from check_seq_next_err
        builder.switch_to_block(loop_check_end);

        // Create cursor for emit methods (reused for seq_is_end and seq_next)
        let mut cursor = JitCursor {
            input_ptr,
            len,
            pos: pos_var,
            ptr_type: pointer_type,
        };

        // Use inline IR for seq_is_end (no helper call!)
        let format = F::default();
        // state_ptr was allocated in entry block - reuse it
        let (is_end_i8, err_code) =
            format.emit_seq_is_end(module, &mut builder, &mut cursor, state_ptr);

        // emit_seq_is_end leaves us at its merge block
        // Store error for error block and check results
        builder.def_var(err_var, err_code);

        // Convert is_end from I8 to check
        let is_end = builder.ins().uextend(pointer_type, is_end_i8);

        builder.ins().jump(check_is_end_err, &[]);
        // Note: loop_check_end will be sealed later, after check_seq_next_err is declared

        // check_is_end_err
        builder.switch_to_block(check_is_end_err);
        let err_ok = builder.ins().icmp_imm(IntCC::Equal, err_code, 0);
        builder
            .ins()
            .brif(err_ok, check_is_end_value, &[], error, &[]);
        builder.seal_block(check_is_end_err);

        // check_is_end_value
        builder.switch_to_block(check_is_end_value);
        let is_end_bool = builder.ins().icmp_imm(IntCC::NotEqual, is_end, 0);
        builder
            .ins()
            .brif(is_end_bool, success, &[], parse_element, &[]);
        builder.seal_block(check_is_end_value);

        // parse_element: use inline IR for parsing
        builder.switch_to_block(parse_element);
        match elem_kind {
            FormatListElementKind::Bool => {
                // Create cursor for emit methods
                let mut cursor = JitCursor {
                    input_ptr,
                    len,
                    pos: pos_var,
                    ptr_type: pointer_type,
                };

                // Use inline IR for bool parsing (no helper call!)
                let format = F::default();
                let (value_i8, err_code) =
                    format.emit_parse_bool(module, &mut builder, &mut cursor);

                // Store parsed value and error
                builder.def_var(parsed_value_var, value_i8);
                builder.def_var(err_var, err_code);

                // emit_parse_bool leaves us in its merge block, jump to check_parse_err
                builder.ins().jump(check_parse_err, &[]);

                // Seal parse_element (its only predecessor check_is_end_value already branched to it)
                builder.seal_block(parse_element);

                // check_parse_err: check error and branch
                builder.switch_to_block(check_parse_err);
                let parse_ok = builder.ins().icmp_imm(IntCC::Equal, err_code, 0);
                builder.ins().brif(parse_ok, push_element, &[], error, &[]);
                builder.seal_block(check_parse_err);
            }
            FormatListElementKind::U8 => {
                let mut cursor = JitCursor {
                    input_ptr,
                    len,
                    pos: pos_var,
                    ptr_type: pointer_type,
                };

                let format = F::default();
                let (value_u8, err_code) = format.emit_parse_u8(module, &mut builder, &mut cursor);

                builder.def_var(parsed_value_var, value_u8);
                builder.def_var(err_var, err_code);

                builder.ins().jump(check_parse_err, &[]);
                builder.seal_block(parse_element);

                builder.switch_to_block(check_parse_err);
                let parse_ok = builder.ins().icmp_imm(IntCC::Equal, err_code, 0);
                builder.ins().brif(parse_ok, push_element, &[], error, &[]);
                builder.seal_block(check_parse_err);
            }
            FormatListElementKind::I64 => {
                let mut cursor = JitCursor {
                    input_ptr,
                    len,
                    pos: pos_var,
                    ptr_type: pointer_type,
                };

                let format = F::default();
                let (value_i64, err_code) =
                    format.emit_parse_i64(module, &mut builder, &mut cursor);

                builder.def_var(parsed_value_var, value_i64);
                builder.def_var(err_var, err_code);

                builder.ins().jump(check_parse_err, &[]);
                builder.seal_block(parse_element);

                builder.switch_to_block(check_parse_err);
                let parse_ok = builder.ins().icmp_imm(IntCC::Equal, err_code, 0);
                builder.ins().brif(parse_ok, push_element, &[], error, &[]);
                builder.seal_block(check_parse_err);
            }
            FormatListElementKind::U64 => {
                let mut cursor = JitCursor {
                    input_ptr,
                    len,
                    pos: pos_var,
                    ptr_type: pointer_type,
                };

                let format = F::default();
                let (value_u64, err_code) =
                    format.emit_parse_u64(module, &mut builder, &mut cursor);

                builder.def_var(parsed_value_var, value_u64);
                builder.def_var(err_var, err_code);

                builder.ins().jump(check_parse_err, &[]);
                builder.seal_block(parse_element);

                builder.switch_to_block(check_parse_err);
                let parse_ok = builder.ins().icmp_imm(IntCC::Equal, err_code, 0);
                builder.ins().brif(parse_ok, push_element, &[], error, &[]);
                builder.seal_block(check_parse_err);
            }
            FormatListElementKind::F64 => {
                let mut cursor = JitCursor {
                    input_ptr,
                    len,
                    pos: pos_var,
                    ptr_type: pointer_type,
                };

                let format = F::default();
                let (value_f64, err_code) =
                    format.emit_parse_f64(module, &mut builder, &mut cursor);

                builder.def_var(parsed_value_var, value_f64);
                builder.def_var(err_var, err_code);

                builder.ins().jump(check_parse_err, &[]);
                builder.seal_block(parse_element);

                builder.switch_to_block(check_parse_err);
                let parse_ok = builder.ins().icmp_imm(IntCC::Equal, err_code, 0);
                builder.ins().brif(parse_ok, push_element, &[], error, &[]);
                builder.seal_block(check_parse_err);
            }
            FormatListElementKind::String => {
                // String parsing returns JitStringValue (ptr, len, cap, owned)
                let mut cursor = JitCursor {
                    input_ptr,
                    len,
                    pos: pos_var,
                    ptr_type: pointer_type,
                };

                let format = F::default();
                let (string_val, err_code) =
                    format.emit_parse_string(module, &mut builder, &mut cursor);

                builder.def_var(err_var, err_code);

                builder.ins().jump(check_parse_err, &[]);
                builder.seal_block(parse_element);

                // check_parse_err: check error and handle String push differently
                builder.switch_to_block(check_parse_err);
                let parse_ok = builder.ins().icmp_imm(IntCC::Equal, err_code, 0);

                // For String, we need to call a helper instead of using push_element block
                // Create a push_string block
                let push_string = builder.create_block();
                builder.ins().brif(parse_ok, push_string, &[], error, &[]);
                builder.seal_block(check_parse_err);

                // push_string: call jit_vec_push_string helper
                builder.switch_to_block(push_string);
                let vec_out_ptr = out_ptr;
                let push_fn_ptr = builder.use_var(push_fn_var);

                // Declare jit_vec_push_string helper
                let helper_sig = {
                    let mut sig = module.make_signature();
                    sig.params.push(AbiParam::new(pointer_type)); // vec_ptr
                    sig.params.push(AbiParam::new(pointer_type)); // push_fn
                    sig.params.push(AbiParam::new(pointer_type)); // str_ptr
                    sig.params.push(AbiParam::new(pointer_type)); // str_len
                    sig.params.push(AbiParam::new(pointer_type)); // str_cap
                    sig.params.push(AbiParam::new(types::I8)); // owned (bool)
                    sig
                };

                let helper_func_id = module
                    .declare_function("jit_vec_push_string", Linkage::Import, &helper_sig)
                    .expect("failed to declare jit_vec_push_string");
                let helper_ref = module.declare_func_in_func(helper_func_id, builder.func);

                // owned is already i8 (1 for owned, 0 for borrowed), use it directly
                // No need to extend since it matches the helper signature

                // Call helper
                builder.ins().call(
                    helper_ref,
                    &[
                        vec_out_ptr,
                        push_fn_ptr,
                        string_val.ptr,
                        string_val.len,
                        string_val.cap,
                        string_val.owned,
                    ],
                );

                // Jump to seq_next
                builder.ins().jump(seq_next, &[]);
                builder.seal_block(push_string);
            }
            FormatListElementKind::Struct(struct_shape) => {
                // Struct parsing: recursively call struct deserializer
                jit_debug!("[compile_list] Parsing struct element");

                // Compile the nested struct deserializer
                let struct_func_id =
                    compile_struct_format_deserializer::<F>(module, struct_shape, memo)?;
                let struct_func_ref = module.declare_func_in_func(struct_func_id, builder.func);

                // Allocate stack slot for struct element
                let struct_layout = struct_shape.layout.sized_layout().ok()?;
                let struct_size = struct_layout.size() as u32;
                let struct_align = struct_layout.align().trailing_zeros() as u8;
                let struct_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    struct_size,
                    struct_align,
                ));
                let struct_elem_ptr = builder.ins().stack_addr(pointer_type, struct_slot, 0);

                // Call struct deserializer: (input_ptr, len, pos, struct_elem_ptr, scratch_ptr)
                let current_pos = builder.use_var(pos_var);
                let call_result = builder.ins().call(
                    struct_func_ref,
                    &[input_ptr, len, current_pos, struct_elem_ptr, scratch_ptr],
                );
                let new_pos = builder.inst_results(call_result)[0];

                // Check for error (new_pos < 0 means error, scratch already written)
                let is_error = builder.ins().icmp_imm(IntCC::SignedLessThan, new_pos, 0);
                let struct_parse_ok = builder.create_block();
                builder.ins().brif(
                    is_error,
                    nested_error_passthrough,
                    &[],
                    struct_parse_ok,
                    &[],
                );
                builder.seal_block(parse_element);

                // On success: update pos_var and push struct element
                builder.switch_to_block(struct_parse_ok);
                builder.def_var(pos_var, new_pos);

                // Push struct element to Vec using push_fn via call_indirect
                let vec_out_ptr = out_ptr;
                let push_fn_ptr = builder.use_var(push_fn_var);

                // Signature for push_fn: PtrMut arguments become two pointer-sized values (ptr + metadata)
                // push_fn(vec_ptr, vec_metadata, elem_ptr, elem_metadata)
                let push_sig = {
                    let mut sig = module.make_signature();
                    sig.params.push(AbiParam::new(pointer_type)); // vec_ptr
                    sig.params.push(AbiParam::new(pointer_type)); // vec_metadata (0 for thin pointers)
                    sig.params.push(AbiParam::new(pointer_type)); // elem_ptr
                    sig.params.push(AbiParam::new(pointer_type)); // elem_metadata (0 for thin pointers)
                    sig
                };
                let push_sig_ref = builder.import_signature(push_sig);

                // Call push_fn indirectly with metadata (0 for thin pointers)
                let null_metadata = builder.ins().iconst(pointer_type, 0);
                builder.ins().call_indirect(
                    push_sig_ref,
                    push_fn_ptr,
                    &[vec_out_ptr, null_metadata, struct_elem_ptr, null_metadata],
                );

                // Jump to seq_next
                builder.ins().jump(seq_next, &[]);
                builder.seal_block(struct_parse_ok);
            }
            FormatListElementKind::List(inner_shape) => {
                // Nested Vec<T> parsing: recursively call list deserializer
                jit_debug!("[compile_list] Parsing nested list element");

                // Compile the nested list deserializer
                let list_func_id =
                    compile_list_format_deserializer::<F>(module, inner_shape, memo)?;
                let list_func_ref = module.declare_func_in_func(list_func_id, builder.func);

                // Allocate stack slot for Vec element (ptr + len + cap)
                let vec_layout = inner_shape.layout.sized_layout().ok()?;
                let vec_size = vec_layout.size() as u32;
                let vec_align = vec_layout.align().trailing_zeros() as u8;
                let vec_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    vec_size,
                    vec_align,
                ));
                let vec_elem_ptr = builder.ins().stack_addr(pointer_type, vec_slot, 0);

                // Call list deserializer: (input_ptr, len, pos, vec_elem_ptr, scratch_ptr)
                let current_pos = builder.use_var(pos_var);
                let call_result = builder.ins().call(
                    list_func_ref,
                    &[input_ptr, len, current_pos, vec_elem_ptr, scratch_ptr],
                );
                let new_pos = builder.inst_results(call_result)[0];

                // Check for error (new_pos < 0 means error, scratch already written)
                let is_error = builder.ins().icmp_imm(IntCC::SignedLessThan, new_pos, 0);
                let list_parse_ok = builder.create_block();
                let list_drop_and_passthrough = builder.create_block();
                builder
                    .ins()
                    .brif(is_error, list_drop_and_passthrough, &[], list_parse_ok, &[]);
                builder.seal_block(parse_element);

                // list_drop_and_passthrough: nested list initialized its output; drop it to avoid leaks,
                // then passthrough error without overwriting scratch.
                builder.switch_to_block(list_drop_and_passthrough);
                let drop_in_place_ref = {
                    let mut s = module.make_signature();
                    s.params.push(AbiParam::new(pointer_type)); // shape_ptr
                    s.params.push(AbiParam::new(pointer_type)); // ptr
                    let id = module
                        .declare_function("jit_drop_in_place", Linkage::Import, &s)
                        .ok()?;
                    module.declare_func_in_func(id, builder.func)
                };
                let shape_ptr = builder
                    .ins()
                    .iconst(pointer_type, inner_shape as *const _ as usize as i64);
                builder
                    .ins()
                    .call(drop_in_place_ref, &[shape_ptr, vec_elem_ptr]);
                builder.ins().jump(nested_error_passthrough, &[]);
                builder.seal_block(list_drop_and_passthrough);

                // On success: update pos_var and push Vec element
                builder.switch_to_block(list_parse_ok);
                builder.def_var(pos_var, new_pos);

                // Push Vec element to outer Vec using push_fn via call_indirect
                let vec_out_ptr = out_ptr;
                let push_fn_ptr = builder.use_var(push_fn_var);

                // Signature for push_fn: PtrMut arguments become two pointer-sized values (ptr + metadata)
                // push_fn(vec_ptr, vec_metadata, elem_ptr, elem_metadata)
                let push_sig = {
                    let mut sig = module.make_signature();
                    sig.params.push(AbiParam::new(pointer_type)); // vec_ptr
                    sig.params.push(AbiParam::new(pointer_type)); // vec_metadata (0 for thin pointers)
                    sig.params.push(AbiParam::new(pointer_type)); // elem_ptr
                    sig.params.push(AbiParam::new(pointer_type)); // elem_metadata (0 for thin pointers)
                    sig
                };
                let push_sig_ref = builder.import_signature(push_sig);

                // Call push_fn indirectly with metadata (0 for thin pointers)
                let null_metadata = builder.ins().iconst(pointer_type, 0);
                builder.ins().call_indirect(
                    push_sig_ref,
                    push_fn_ptr,
                    &[vec_out_ptr, null_metadata, vec_elem_ptr, null_metadata],
                );

                // Jump to seq_next
                builder.ins().jump(seq_next, &[]);
                builder.seal_block(list_parse_ok);
            }
            FormatListElementKind::Map(inner_shape) => {
                // Nested HashMap<K, V> parsing: recursively call map deserializer
                jit_debug!("[compile_list] Parsing nested map element");

                // Compile the nested map deserializer
                let map_func_id = compile_map_format_deserializer::<F>(module, inner_shape, memo)?;
                let map_func_ref = module.declare_func_in_func(map_func_id, builder.func);

                // Allocate stack slot for HashMap element
                let map_layout = inner_shape.layout.sized_layout().ok()?;
                let map_size = map_layout.size() as u32;
                let map_align = map_layout.align().trailing_zeros() as u8;
                let map_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    map_size,
                    map_align,
                ));
                let map_elem_ptr = builder.ins().stack_addr(pointer_type, map_slot, 0);

                // Call map deserializer: (input_ptr, len, pos, map_elem_ptr, scratch_ptr)
                let current_pos = builder.use_var(pos_var);
                let call_result = builder.ins().call(
                    map_func_ref,
                    &[input_ptr, len, current_pos, map_elem_ptr, scratch_ptr],
                );
                let new_pos = builder.inst_results(call_result)[0];

                // Check for error (new_pos < 0 means error, scratch already written)
                let is_error = builder.ins().icmp_imm(IntCC::SignedLessThan, new_pos, 0);
                let map_parse_ok = builder.create_block();
                let map_drop_and_passthrough = builder.create_block();
                builder
                    .ins()
                    .brif(is_error, map_drop_and_passthrough, &[], map_parse_ok, &[]);
                builder.seal_block(parse_element);

                // map_drop_and_passthrough: nested map initialized its output; drop it to avoid leaks,
                // then passthrough error without overwriting scratch.
                builder.switch_to_block(map_drop_and_passthrough);
                let drop_in_place_ref = {
                    let mut s = module.make_signature();
                    s.params.push(AbiParam::new(pointer_type)); // shape_ptr
                    s.params.push(AbiParam::new(pointer_type)); // ptr
                    let id = module
                        .declare_function("jit_drop_in_place", Linkage::Import, &s)
                        .ok()?;
                    module.declare_func_in_func(id, builder.func)
                };
                let shape_ptr = builder
                    .ins()
                    .iconst(pointer_type, inner_shape as *const _ as usize as i64);
                builder
                    .ins()
                    .call(drop_in_place_ref, &[shape_ptr, map_elem_ptr]);
                builder.ins().jump(nested_error_passthrough, &[]);
                builder.seal_block(map_drop_and_passthrough);

                // On success: update pos_var and push HashMap element
                builder.switch_to_block(map_parse_ok);
                builder.def_var(pos_var, new_pos);

                // Push HashMap element to Vec using push_fn via call_indirect
                let vec_out_ptr = out_ptr;
                let push_fn_ptr = builder.use_var(push_fn_var);

                // Signature for push_fn: PtrMut arguments become two pointer-sized values (ptr + metadata)
                // push_fn(vec_ptr, vec_metadata, elem_ptr, elem_metadata)
                let push_sig = {
                    let mut sig = module.make_signature();
                    sig.params.push(AbiParam::new(pointer_type)); // vec_ptr
                    sig.params.push(AbiParam::new(pointer_type)); // vec_metadata (0 for thin pointers)
                    sig.params.push(AbiParam::new(pointer_type)); // elem_ptr
                    sig.params.push(AbiParam::new(pointer_type)); // elem_metadata (0 for thin pointers)
                    sig
                };
                let push_sig_ref = builder.import_signature(push_sig);

                // Call push_fn indirectly with metadata (0 for thin pointers)
                let null_metadata = builder.ins().iconst(pointer_type, 0);
                builder.ins().call_indirect(
                    push_sig_ref,
                    push_fn_ptr,
                    &[vec_out_ptr, null_metadata, map_elem_ptr, null_metadata],
                );

                // Jump to seq_next
                builder.ins().jump(seq_next, &[]);
                builder.seal_block(map_parse_ok);
            }
        }

        // nested_error_passthrough: nested call failed and already wrote scratch,
        // return -1 without overwriting scratch.
        builder.switch_to_block(nested_error_passthrough);
        let neg_one = builder.ins().iconst(pointer_type, -1i64);
        builder.ins().return_(&[neg_one]);
        builder.seal_block(nested_error_passthrough);

        // push_element: store value to stack slot and call push_fn directly
        builder.switch_to_block(push_element);
        let parsed_value = builder.use_var(parsed_value_var);
        let push_fn_ptr = builder.use_var(push_fn_var);

        // Get address of element stack slot
        let elem_ptr = builder.ins().stack_addr(pointer_type, elem_slot, 0);

        // Store the parsed value into the element stack slot
        builder
            .ins()
            .store(MemFlags::trusted(), parsed_value, elem_ptr, 0);

        // Call push_fn directly via call_indirect: push_fn(vec_ptr, elem_ptr)
        // PtrMut is a 16-byte struct (TaggedPtr + metadata), so each PtrMut argument
        // becomes two pointer-sized values. For thin pointers, metadata is 0.
        let null_metadata = builder.ins().iconst(pointer_type, 0);
        builder.ins().call_indirect(
            sig_direct_push_ref,
            push_fn_ptr,
            &[out_ptr, null_metadata, elem_ptr, null_metadata],
        );

        builder.ins().jump(seq_next, &[]);
        builder.seal_block(push_element);

        // seq_next: use inline IR for comma handling
        builder.switch_to_block(seq_next);

        // Reuse cursor (need to recreate since emit_parse_bool may have been called)
        let mut cursor = JitCursor {
            input_ptr,
            len,
            pos: pos_var,
            ptr_type: pointer_type,
        };

        // Use inline IR for seq_next (no helper call!)
        let format = F::default();
        // state_ptr was allocated in entry block - reuse it
        let err_code = format.emit_seq_next(module, &mut builder, &mut cursor, state_ptr);

        // emit_seq_next leaves us at its merge block and updates cursor.pos internally
        builder.def_var(err_var, err_code);
        builder.ins().jump(check_seq_next_err, &[]);
        builder.seal_block(seq_next);

        // check_seq_next_err
        builder.switch_to_block(check_seq_next_err);
        let next_ok = builder.ins().icmp_imm(IntCC::Equal, err_code, 0);
        builder.ins().brif(next_ok, loop_check_end, &[], error, &[]);
        builder.seal_block(check_seq_next_err);

        // Seal loop_check_end - predecessors depend on use_direct_fill:
        // - If push-based: predecessors are init_vec (if !use_direct_fill) and check_seq_next_err
        // - If direct-fill: loop_check_end is never entered, seal it anyway
        if !use_direct_fill {
            builder.seal_block(loop_check_end);
        }

        // =================================================================
        // Direct-fill path (only used when use_direct_fill is true)
        // =================================================================
        if use_direct_fill {
            // df_setup: get base pointer and initialize counter
            builder.switch_to_block(df_setup);
            let set_len_fn_ptr = builder
                .ins()
                .iconst(pointer_type, set_len_fn.unwrap() as *const () as i64);
            let as_mut_ptr_fn_ptr = builder.ins().iconst(
                pointer_type,
                as_mut_ptr_typed_fn.unwrap() as *const () as i64,
            );

            // Get base pointer to vec's buffer
            let call_inst = builder
                .ins()
                .call(vec_as_mut_ptr_typed_ref, &[out_ptr, as_mut_ptr_fn_ptr]);
            let base_ptr = builder.inst_results(call_inst)[0];

            // Store base_ptr and set_len_fn_ptr in variables for use in loop/finalize
            let base_ptr_var = builder.declare_var(pointer_type);
            builder.def_var(base_ptr_var, base_ptr);
            let set_len_fn_var = builder.declare_var(pointer_type);
            builder.def_var(set_len_fn_var, set_len_fn_ptr);

            // Initialize loop counter to 0
            let counter_var = builder.declare_var(pointer_type);
            let zero = builder.ins().iconst(pointer_type, 0);
            builder.def_var(counter_var, zero);

            // For U8: try bulk copy path first
            if elem_kind == FormatListElementKind::U8 {
                builder.ins().jump(df_bulk_copy, &[]);
            } else {
                builder.ins().jump(df_loop_check, &[]);
            }
            builder.seal_block(df_setup);

            // df_bulk_copy: try bulk copy for Vec<u8>
            builder.switch_to_block(df_bulk_copy);
            let format = F::default();
            let count = builder.use_var(seq_count_var);
            let base_ptr = builder.use_var(base_ptr_var);
            let mut cursor = JitCursor {
                input_ptr,
                len,
                pos: pos_var,
                ptr_type: pointer_type,
            };
            if let Some(bulk_err) =
                format.emit_seq_bulk_copy_u8(&mut builder, &mut cursor, count, base_ptr)
            {
                // Format supports bulk copy - check error
                builder.def_var(err_var, bulk_err);
                builder.ins().jump(df_bulk_copy_check_err, &[]);
            } else {
                // Format doesn't support bulk copy, fall back to element-by-element loop
                builder.ins().jump(df_loop_check, &[]);
            }
            builder.seal_block(df_bulk_copy);

            // df_bulk_copy_check_err: check if bulk copy succeeded
            builder.switch_to_block(df_bulk_copy_check_err);
            let bulk_err = builder.use_var(err_var);
            let bulk_ok = builder.ins().icmp_imm(IntCC::Equal, bulk_err, 0);
            // On success: set counter_var = count so df_finalize sets correct length
            let set_counter_block = builder.create_block();
            builder
                .ins()
                .brif(bulk_ok, set_counter_block, &[], error, &[]);
            builder.seal_block(df_bulk_copy_check_err);

            builder.switch_to_block(set_counter_block);
            let count = builder.use_var(seq_count_var);
            builder.def_var(counter_var, count);
            builder.ins().jump(df_finalize, &[]);
            builder.seal_block(set_counter_block);

            // df_loop_check: check if counter < count
            builder.switch_to_block(df_loop_check);
            let counter = builder.use_var(counter_var);
            let count = builder.use_var(seq_count_var);
            let done = builder
                .ins()
                .icmp(IntCC::UnsignedGreaterThanOrEqual, counter, count);
            builder.ins().brif(done, df_finalize, &[], df_parse, &[]);
            // Note: df_loop_check will be sealed after df_store (back edge)

            // df_parse: parse the next element
            builder.switch_to_block(df_parse);
            let mut cursor = JitCursor {
                input_ptr,
                len,
                pos: pos_var,
                ptr_type: pointer_type,
            };

            // Parse based on element type
            let format = F::default();
            let (parsed_val, parse_err) = match elem_kind {
                FormatListElementKind::Bool => {
                    format.emit_parse_bool(module, &mut builder, &mut cursor)
                }
                FormatListElementKind::U8 => {
                    format.emit_parse_u8(module, &mut builder, &mut cursor)
                }
                FormatListElementKind::I64 => {
                    format.emit_parse_i64(module, &mut builder, &mut cursor)
                }
                FormatListElementKind::U64 => {
                    format.emit_parse_u64(module, &mut builder, &mut cursor)
                }
                _ => unreachable!("direct-fill only for scalars"),
            };
            builder.def_var(parsed_value_var, parsed_val);
            builder.def_var(err_var, parse_err);
            builder.ins().jump(df_check_parse_err, &[]);
            builder.seal_block(df_parse);

            // df_check_parse_err
            builder.switch_to_block(df_check_parse_err);
            let parse_ok = builder.ins().icmp_imm(IntCC::Equal, parse_err, 0);
            builder.ins().brif(parse_ok, df_store, &[], error, &[]);
            builder.seal_block(df_check_parse_err);

            // df_store: store parsed value directly into vec buffer
            builder.switch_to_block(df_store);
            let parsed_val = builder.use_var(parsed_value_var);
            let base_ptr = builder.use_var(base_ptr_var);
            let counter = builder.use_var(counter_var);

            // Calculate offset: base_ptr + counter * elem_size
            let elem_size_val = builder.ins().iconst(pointer_type, elem_size as i64);
            let offset = builder.ins().imul(counter, elem_size_val);
            let dest_ptr = builder.ins().iadd(base_ptr, offset);

            // Truncate value if needed and store with the correct width.
            // Note: emit_parse_bool/emit_parse_u8 already return i8, so no truncation needed.
            // Only emit_parse_i64/emit_parse_u64 (which return i64) need truncation for smaller types.
            use facet_core::ScalarType;
            let scalar_type = elem_shape.scalar_type().unwrap();
            let store_val = match scalar_type {
                // Bool/U8/I8: parser returns i8 directly, no truncation needed
                ScalarType::Bool | ScalarType::U8 | ScalarType::I8 => parsed_val,
                // I16/U16/I32/U32: parser returns i64, truncate to correct width
                ScalarType::I16 | ScalarType::U16 => builder.ins().ireduce(types::I16, parsed_val),
                ScalarType::I32 | ScalarType::U32 => builder.ins().ireduce(types::I32, parsed_val),
                // I64/U64: parser returns i64 directly
                ScalarType::I64 | ScalarType::U64 => parsed_val,
                _ => unreachable!("direct-fill only for integers"),
            };
            builder
                .ins()
                .store(MemFlags::trusted(), store_val, dest_ptr, 0);

            // Increment counter
            let one = builder.ins().iconst(pointer_type, 1);
            let new_counter = builder.ins().iadd(counter, one);
            builder.def_var(counter_var, new_counter);

            // Loop back
            builder.ins().jump(df_loop_check, &[]);
            builder.seal_block(df_store);
            builder.seal_block(df_loop_check); // Now we can seal it (back edge from df_store)

            // df_finalize: set the vec's length and go to success
            builder.switch_to_block(df_finalize);
            let final_count = builder.use_var(counter_var);
            let set_len_fn_ptr = builder.use_var(set_len_fn_var);
            builder
                .ins()
                .call(vec_set_len_ref, &[out_ptr, final_count, set_len_fn_ptr]);
            builder.ins().jump(success, &[]);
            builder.seal_block(df_finalize);

            // Seal unused push-based blocks (they have no predecessors in direct-fill mode)
            builder.seal_block(loop_check_end);
        } else {
            // Seal unused direct-fill blocks (they have no predecessors in push-based mode)
            builder.seal_block(df_setup);
            builder.seal_block(df_bulk_copy);
            builder.seal_block(df_bulk_copy_check_err);
            builder.seal_block(df_loop_check);
            builder.seal_block(df_parse);
            builder.seal_block(df_check_parse_err);
            builder.seal_block(df_store);
            builder.seal_block(df_finalize);
        }

        // success: return new position
        builder.switch_to_block(success);
        let final_pos = builder.use_var(pos_var);
        builder.ins().return_(&[final_pos]);
        builder.seal_block(success);

        // error: write scratch and return negative
        builder.switch_to_block(error);
        let err_code = builder.use_var(err_var); // Use actual error code from helper
        let err_pos = builder.use_var(pos_var);
        // Write error_code to scratch
        builder.ins().store(
            MemFlags::trusted(),
            err_code,
            scratch_ptr,
            JIT_SCRATCH_ERROR_CODE_OFFSET,
        );
        // Write error_pos to scratch
        builder.ins().store(
            MemFlags::trusted(),
            err_pos,
            scratch_ptr,
            JIT_SCRATCH_ERROR_POS_OFFSET,
        );
        let neg_one = builder.ins().iconst(pointer_type, -1i64);
        builder.ins().return_(&[neg_one]);
        builder.seal_block(error);

        builder.finalize();
    }

    if let Err(_e) = module.define_function(func_id, &mut ctx) {
        jit_debug!("[compile_list] define_function failed: {:?}", _e);
        return None;
    }

    jit_debug!("[compile_list] SUCCESS - function compiled");
    Some(func_id)
}

/// Compile a Tier-2 HashMap deserializer for HashMap<String, V>.
///
/// Generates code that parses a JSON object and populates the HashMap.
/// Signature: fn(input_ptr, len, pos, out, scratch) -> isize
fn compile_map_format_deserializer<F: JitFormat>(
    module: &mut JITModule,
    shape: &'static Shape,
    memo: &mut ShapeMemo,
) -> Option<FuncId> {
    jit_diag!(
        "compile_map_format_deserializer ENTRY for shape {:p}",
        shape
    );

    // Check memo first - return cached FuncId if already compiled
    let shape_ptr = shape as *const Shape;
    if let Some(&func_id) = memo.get(&shape_ptr) {
        jit_diag!(
            "compile_map_format_deserializer: using memoized FuncId for shape {:p}",
            shape
        );
        return Some(func_id);
    }

    let Def::Map(map_def) = &shape.def else {
        jit_debug!("[compile_map] Not a map");
        return None;
    };

    // Only support String keys for now
    if map_def.k.scalar_type() != Some(facet_core::ScalarType::String) {
        jit_debug!("[compile_map] Only String keys supported");
        return None;
    }

    let value_shape = map_def.v;
    let value_kind = match FormatListElementKind::from_shape(value_shape) {
        Some(k) => k,
        None => {
            jit_debug!("[compile_map] Value type not supported");
            return None;
        }
    };

    // Get HashMap vtable functions
    let init_fn = map_def.vtable.init_in_place_with_capacity;
    let insert_fn = map_def.vtable.insert;

    let pointer_type = module.target_config().pointer_type();

    // Function signature: fn(input_ptr, len, pos, out, scratch) -> isize
    let sig = {
        let mut s = module.make_signature();
        s.params.push(AbiParam::new(pointer_type)); // input_ptr
        s.params.push(AbiParam::new(pointer_type)); // len
        s.params.push(AbiParam::new(pointer_type)); // pos
        s.params.push(AbiParam::new(pointer_type)); // out (map ptr)
        s.params.push(AbiParam::new(pointer_type)); // scratch
        s.returns.push(AbiParam::new(pointer_type)); // isize
        s
    };

    // Map insert signature: fn(map_ptr: PtrMut, key_ptr: PtrMut, value_ptr: PtrMut) -> ()
    let sig_map_insert = {
        let mut s = module.make_signature();
        s.params.push(AbiParam::new(pointer_type)); // map_ptr.ptr
        s.params.push(AbiParam::new(pointer_type)); // map_ptr.metadata
        s.params.push(AbiParam::new(pointer_type)); // key_ptr.ptr
        s.params.push(AbiParam::new(pointer_type)); // key_ptr.metadata
        s.params.push(AbiParam::new(pointer_type)); // value_ptr.ptr
        s.params.push(AbiParam::new(pointer_type)); // value_ptr.metadata
        s
    };

    // Generate unique name for this map deserializer
    let func_name = format!("jit_deserialize_map_{:x}", shape as *const _ as usize);

    let func_id = match module.declare_function(&func_name, Linkage::Local, &sig) {
        Ok(id) => id,
        Err(e) => {
            jit_debug!("[compile_map] declare {} failed: {:?}", func_name, e);
            jit_diag!("declare_function('{}') failed: {:?}", func_name, e);
            return None;
        }
    };

    // Insert into memo immediately after declaration (before IR build) to avoid recursion/cycles
    memo.insert(shape_ptr, func_id);
    jit_diag!(
        "compile_map_format_deserializer: memoized FuncId for shape {:p}",
        shape
    );

    let mut ctx = module.make_context();
    ctx.func.signature = sig;

    let mut builder_ctx = FunctionBuilderContext::new();
    let mut builder = FunctionBuilder::new(&mut ctx.func, &mut builder_ctx);

    let entry = builder.create_block();
    builder.append_block_params_for_function_params(entry);
    builder.switch_to_block(entry);

    let input_ptr = builder.block_params(entry)[0];
    let len = builder.block_params(entry)[1];
    let pos_param = builder.block_params(entry)[2];
    let out_ptr = builder.block_params(entry)[3];
    let scratch_ptr = builder.block_params(entry)[4];

    let pos_var = builder.declare_var(pointer_type);
    builder.def_var(pos_var, pos_param);

    let err_var = builder.declare_var(types::I32);
    let zero_i32 = builder.ins().iconst(types::I32, 0);
    builder.def_var(err_var, zero_i32);

    // Map state pointer (format-specific)
    let state_ptr = if F::MAP_STATE_SIZE > 0 {
        let align_shift = F::MAP_STATE_ALIGN.trailing_zeros() as u8;
        let state_slot = builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            F::MAP_STATE_SIZE,
            align_shift,
        ));
        builder.ins().stack_addr(pointer_type, state_slot, 0)
    } else {
        builder.ins().iconst(pointer_type, 0)
    };

    // Track a pending owned key string so we can drop it on early errors (before insertion).
    let key_ptr_var = builder.declare_var(pointer_type);
    let key_len_var = builder.declare_var(pointer_type);
    let key_cap_var = builder.declare_var(pointer_type);
    let key_owned_var = builder.declare_var(types::I8);
    let zero_ptr = builder.ins().iconst(pointer_type, 0);
    let zero_i8 = builder.ins().iconst(types::I8, 0);
    builder.def_var(key_ptr_var, zero_ptr);
    builder.def_var(key_len_var, zero_ptr);
    builder.def_var(key_cap_var, zero_ptr);
    builder.def_var(key_owned_var, zero_i8);

    // Helpers
    let map_init_ref = {
        // fn(out_ptr: *mut u8, capacity: usize, init_fn: *const u8) -> ()
        let mut s = module.make_signature();
        s.params.push(AbiParam::new(pointer_type)); // out_ptr
        s.params.push(AbiParam::new(pointer_type)); // capacity
        s.params.push(AbiParam::new(pointer_type)); // init_fn
        let id = module
            .declare_function("jit_map_init_with_capacity", Linkage::Import, &s)
            .ok()?;
        module.declare_func_in_func(id, builder.func)
    };

    let write_string_ref = {
        // jit_write_string(out, offset, ptr, len, cap, owned)
        let mut s = module.make_signature();
        s.params.push(AbiParam::new(pointer_type)); // out_ptr
        s.params.push(AbiParam::new(pointer_type)); // offset
        s.params.push(AbiParam::new(pointer_type)); // str_ptr
        s.params.push(AbiParam::new(pointer_type)); // str_len
        s.params.push(AbiParam::new(pointer_type)); // str_cap
        s.params.push(AbiParam::new(types::I8)); // owned
        let id = module
            .declare_function("jit_write_string", Linkage::Import, &s)
            .ok()?;
        module.declare_func_in_func(id, builder.func)
    };

    let drop_owned_string_ref = {
        // jit_drop_owned_string(ptr, len, cap)
        let mut s = module.make_signature();
        s.params.push(AbiParam::new(pointer_type)); // ptr
        s.params.push(AbiParam::new(pointer_type)); // len
        s.params.push(AbiParam::new(pointer_type)); // cap
        let id = module
            .declare_function("jit_drop_owned_string", Linkage::Import, &s)
            .ok()?;
        module.declare_func_in_func(id, builder.func)
    };

    // Allocate stack space for the key String (layout: ptr, len, cap).
    let key_slot = builder.create_sized_stack_slot(StackSlotData::new(
        StackSlotKind::ExplicitSlot,
        3 * pointer_type.bytes(),
        pointer_type.bytes().trailing_zeros() as u8,
    ));
    let key_out_ptr = builder.ins().stack_addr(pointer_type, key_slot, 0);

    // Allocate stack space for the value.
    let value_layout = match value_shape.layout.sized_layout() {
        Ok(layout) => layout,
        Err(_) => {
            jit_debug!("[compile_map] Value shape has unsized layout");
            return None;
        }
    };
    let value_size = value_layout.size() as u32;
    let value_align = value_layout.align().trailing_zeros() as u8;
    let value_slot = builder.create_sized_stack_slot(StackSlotData::new(
        StackSlotKind::ExplicitSlot,
        value_size,
        value_align,
    ));
    let value_ptr = builder.ins().stack_addr(pointer_type, value_slot, 0);

    // Initialize map with capacity 0 (will grow as needed).
    let init_fn_ptr = builder.ins().iconst(pointer_type, init_fn as usize as i64);
    let zero_capacity = builder.ins().iconst(pointer_type, 0);
    builder
        .ins()
        .call(map_init_ref, &[out_ptr, zero_capacity, init_fn_ptr]);

    // Mark output as initialized so wrapper can drop on error.
    let one_i8 = builder.ins().iconst(types::I8, 1);
    builder.ins().store(
        MemFlags::trusted(),
        one_i8,
        scratch_ptr,
        JIT_SCRATCH_OUTPUT_INITIALIZED_OFFSET,
    );

    let format = F::default();
    let mut cursor = JitCursor {
        input_ptr,
        len,
        pos: pos_var,
        ptr_type: pointer_type,
    };

    let loop_check_end = builder.create_block();
    let loop_body = builder.create_block();
    let done = builder.create_block();
    let error = builder.create_block();
    let nested_error_passthrough = builder.create_block();

    // map_begin
    let begin_err = format.emit_map_begin(module, &mut builder, &mut cursor, state_ptr);
    builder.def_var(err_var, begin_err);
    let begin_ok = builder.ins().icmp_imm(IntCC::Equal, begin_err, 0);
    builder
        .ins()
        .brif(begin_ok, loop_check_end, &[], error, &[]);
    builder.seal_block(entry);

    // loop_check_end
    // Note: do NOT seal yet; it has a back edge from loop_body.
    builder.switch_to_block(loop_check_end);
    let (is_end, end_err) = format.emit_map_is_end(module, &mut builder, &mut cursor, state_ptr);
    builder.def_var(err_var, end_err);
    let end_ok = builder.ins().icmp_imm(IntCC::Equal, end_err, 0);
    let check_end_value = builder.create_block();
    builder.ins().brif(end_ok, check_end_value, &[], error, &[]);

    builder.switch_to_block(check_end_value);
    builder.seal_block(check_end_value);
    let is_end_bool = builder.ins().icmp_imm(IntCC::NotEqual, is_end, 0);
    builder.ins().brif(is_end_bool, done, &[], loop_body, &[]);

    // loop_body
    builder.switch_to_block(loop_body);

    // Reset pending key raw parts for this iteration.
    builder.def_var(key_ptr_var, zero_ptr);
    builder.def_var(key_len_var, zero_ptr);
    builder.def_var(key_cap_var, zero_ptr);
    builder.def_var(key_owned_var, zero_i8);

    // read_key
    let (key_value, key_err) =
        format.emit_map_read_key(module, &mut builder, &mut cursor, state_ptr);
    builder.def_var(err_var, key_err);
    let key_ok = builder.ins().icmp_imm(IntCC::Equal, key_err, 0);
    let after_key = builder.create_block();
    builder.ins().brif(key_ok, after_key, &[], error, &[]);

    builder.switch_to_block(after_key);
    builder.seal_block(after_key);
    builder.def_var(key_ptr_var, key_value.ptr);
    builder.def_var(key_len_var, key_value.len);
    builder.def_var(key_cap_var, key_value.cap);
    builder.def_var(key_owned_var, key_value.owned);

    // kv_sep
    let sep_err = format.emit_map_kv_sep(module, &mut builder, &mut cursor, state_ptr);
    builder.def_var(err_var, sep_err);
    let sep_ok = builder.ins().icmp_imm(IntCC::Equal, sep_err, 0);
    let after_sep = builder.create_block();
    builder.ins().brif(sep_ok, after_sep, &[], error, &[]);

    builder.switch_to_block(after_sep);
    builder.seal_block(after_sep);

    // value
    match value_kind {
        FormatListElementKind::Bool => {
            let (value_i8, err) = format.emit_parse_bool(module, &mut builder, &mut cursor);
            builder.def_var(err_var, err);
            let ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);
            let store = builder.create_block();
            builder.ins().brif(ok, store, &[], error, &[]);
            builder.switch_to_block(store);
            builder
                .ins()
                .store(MemFlags::trusted(), value_i8, value_ptr, 0);
            builder.seal_block(store);
        }
        FormatListElementKind::U8 => {
            let (value_u8, err) = format.emit_parse_u8(module, &mut builder, &mut cursor);
            builder.def_var(err_var, err);
            let ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);
            let store = builder.create_block();
            builder.ins().brif(ok, store, &[], error, &[]);
            builder.switch_to_block(store);
            builder
                .ins()
                .store(MemFlags::trusted(), value_u8, value_ptr, 0);
            builder.seal_block(store);
        }
        FormatListElementKind::I64 => {
            use facet_core::ScalarType;
            let (value_i64, err) = format.emit_parse_i64(module, &mut builder, &mut cursor);
            builder.def_var(err_var, err);
            let ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);
            let store = builder.create_block();
            builder.ins().brif(ok, store, &[], error, &[]);
            builder.switch_to_block(store);
            let scalar = value_shape.scalar_type().unwrap();
            let value = match scalar {
                ScalarType::I8 => builder.ins().ireduce(types::I8, value_i64),
                ScalarType::I16 => builder.ins().ireduce(types::I16, value_i64),
                ScalarType::I32 => builder.ins().ireduce(types::I32, value_i64),
                ScalarType::I64 => value_i64,
                _ => value_i64,
            };
            builder
                .ins()
                .store(MemFlags::trusted(), value, value_ptr, 0);
            builder.seal_block(store);
        }
        FormatListElementKind::U64 => {
            use facet_core::ScalarType;
            let (value_u64, err) = format.emit_parse_u64(module, &mut builder, &mut cursor);
            builder.def_var(err_var, err);
            let ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);
            let store = builder.create_block();
            builder.ins().brif(ok, store, &[], error, &[]);
            builder.switch_to_block(store);
            let scalar = value_shape.scalar_type().unwrap();
            let value = match scalar {
                ScalarType::U8 => builder.ins().ireduce(types::I8, value_u64),
                ScalarType::U16 => builder.ins().ireduce(types::I16, value_u64),
                ScalarType::U32 => builder.ins().ireduce(types::I32, value_u64),
                ScalarType::U64 => value_u64,
                _ => value_u64,
            };
            builder
                .ins()
                .store(MemFlags::trusted(), value, value_ptr, 0);
            builder.seal_block(store);
        }
        FormatListElementKind::F64 => {
            use facet_core::ScalarType;
            let (value_f64, err) = format.emit_parse_f64(module, &mut builder, &mut cursor);
            builder.def_var(err_var, err);
            let ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);
            let store = builder.create_block();
            builder.ins().brif(ok, store, &[], error, &[]);
            builder.switch_to_block(store);
            let scalar = value_shape.scalar_type().unwrap();
            let value = if matches!(scalar, ScalarType::F32) {
                builder.ins().fdemote(types::F32, value_f64)
            } else {
                value_f64
            };
            builder
                .ins()
                .store(MemFlags::trusted(), value, value_ptr, 0);
            builder.seal_block(store);
        }
        FormatListElementKind::String => {
            let (string_value, err) = format.emit_parse_string(module, &mut builder, &mut cursor);
            builder.def_var(err_var, err);
            let ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);
            let store = builder.create_block();
            builder.ins().brif(ok, store, &[], error, &[]);
            builder.switch_to_block(store);
            let zero_offset = builder.ins().iconst(pointer_type, 0);
            builder.ins().call(
                write_string_ref,
                &[
                    value_ptr,
                    zero_offset,
                    string_value.ptr,
                    string_value.len,
                    string_value.cap,
                    string_value.owned,
                ],
            );
            builder.seal_block(store);
        }
        FormatListElementKind::Struct(_) => {
            let struct_func_id =
                compile_struct_format_deserializer::<F>(module, value_shape, memo)?;
            let struct_func_ref = module.declare_func_in_func(struct_func_id, builder.func);

            let current_pos = builder.use_var(pos_var);
            let call_result = builder.ins().call(
                struct_func_ref,
                &[input_ptr, len, current_pos, value_ptr, scratch_ptr],
            );
            let new_pos = builder.inst_results(call_result)[0];

            let is_error = builder.ins().icmp_imm(IntCC::SignedLessThan, new_pos, 0);
            let nested_ok = builder.create_block();
            builder
                .ins()
                .brif(is_error, nested_error_passthrough, &[], nested_ok, &[]);

            builder.switch_to_block(nested_ok);
            builder.def_var(pos_var, new_pos);
            builder.seal_block(nested_ok);
        }
        FormatListElementKind::List(_) => {
            let list_func_id = compile_list_format_deserializer::<F>(module, value_shape, memo)?;
            let list_func_ref = module.declare_func_in_func(list_func_id, builder.func);

            let current_pos = builder.use_var(pos_var);
            let call_result = builder.ins().call(
                list_func_ref,
                &[input_ptr, len, current_pos, value_ptr, scratch_ptr],
            );
            let new_pos = builder.inst_results(call_result)[0];

            let is_error = builder.ins().icmp_imm(IntCC::SignedLessThan, new_pos, 0);
            let nested_ok = builder.create_block();
            builder
                .ins()
                .brif(is_error, nested_error_passthrough, &[], nested_ok, &[]);

            builder.switch_to_block(nested_ok);
            builder.def_var(pos_var, new_pos);
            builder.seal_block(nested_ok);
        }
        FormatListElementKind::Map(_) => {
            let map_func_id = compile_map_format_deserializer::<F>(module, value_shape, memo)?;
            let map_func_ref = module.declare_func_in_func(map_func_id, builder.func);

            let current_pos = builder.use_var(pos_var);
            let call_result = builder.ins().call(
                map_func_ref,
                &[input_ptr, len, current_pos, value_ptr, scratch_ptr],
            );
            let new_pos = builder.inst_results(call_result)[0];

            let is_error = builder.ins().icmp_imm(IntCC::SignedLessThan, new_pos, 0);
            let nested_ok = builder.create_block();
            builder
                .ins()
                .brif(is_error, nested_error_passthrough, &[], nested_ok, &[]);

            builder.switch_to_block(nested_ok);
            builder.def_var(pos_var, new_pos);
            builder.seal_block(nested_ok);
        }
    }

    // Materialize key into an owned String right before insertion.
    // This avoids constructing a fake String pointing into the input buffer.
    let zero_offset = builder.ins().iconst(pointer_type, 0);
    let key_ptr_raw = builder.use_var(key_ptr_var);
    let key_len_raw = builder.use_var(key_len_var);
    let key_cap_raw = builder.use_var(key_cap_var);
    let key_owned_raw = builder.use_var(key_owned_var);
    builder.ins().call(
        write_string_ref,
        &[
            key_out_ptr,
            zero_offset,
            key_ptr_raw,
            key_len_raw,
            key_cap_raw,
            key_owned_raw,
        ],
    );
    // Raw parts consumed when owned=1.
    builder.def_var(key_owned_var, zero_i8);

    // insert
    let insert_fn_addr = builder
        .ins()
        .iconst(pointer_type, insert_fn as usize as i64);
    let sig_ref_map_insert = builder.import_signature(sig_map_insert);
    let zero_meta = builder.ins().iconst(pointer_type, 0);
    builder.ins().call_indirect(
        sig_ref_map_insert,
        insert_fn_addr,
        &[
            out_ptr,
            zero_meta,
            key_out_ptr,
            zero_meta,
            value_ptr,
            zero_meta,
        ],
    );

    // next
    let next_err = format.emit_map_next(module, &mut builder, &mut cursor, state_ptr);
    builder.def_var(err_var, next_err);
    let next_ok = builder.ins().icmp_imm(IntCC::Equal, next_err, 0);
    let after_next = builder.create_block();
    builder.ins().brif(next_ok, after_next, &[], error, &[]);

    builder.switch_to_block(after_next);
    builder.seal_block(after_next);
    builder.ins().jump(loop_check_end, &[]);

    builder.seal_block(loop_body);
    builder.seal_block(loop_check_end);

    // done
    builder.switch_to_block(done);
    let final_pos = builder.use_var(pos_var);
    builder.ins().return_(&[final_pos]);
    builder.seal_block(done);

    // nested_error_passthrough: nested call failed, scratch already written.
    // Still drop any pending owned key raw string.
    builder.switch_to_block(nested_error_passthrough);
    let key_owned = builder.use_var(key_owned_var);
    let need_drop = builder.ins().icmp_imm(IntCC::NotEqual, key_owned, 0);
    let drop_key = builder.create_block();
    let nested_after_drop = builder.create_block();
    builder
        .ins()
        .brif(need_drop, drop_key, &[], nested_after_drop, &[]);

    builder.switch_to_block(drop_key);
    let key_ptr_val = builder.use_var(key_ptr_var);
    let key_len_val = builder.use_var(key_len_var);
    let key_cap_val = builder.use_var(key_cap_var);
    builder.ins().call(
        drop_owned_string_ref,
        &[key_ptr_val, key_len_val, key_cap_val],
    );
    builder.ins().jump(nested_after_drop, &[]);
    builder.seal_block(drop_key);

    builder.switch_to_block(nested_after_drop);
    let minus_one = builder.ins().iconst(pointer_type, -1i64);
    builder.ins().return_(&[minus_one]);
    builder.seal_block(nested_after_drop);
    builder.seal_block(nested_error_passthrough);

    // error: drop pending owned key (if any), write scratch and return -1.
    builder.switch_to_block(error);
    let key_owned = builder.use_var(key_owned_var);
    let need_drop = builder.ins().icmp_imm(IntCC::NotEqual, key_owned, 0);
    let drop_key = builder.create_block();
    let after_drop = builder.create_block();
    builder
        .ins()
        .brif(need_drop, drop_key, &[], after_drop, &[]);

    builder.switch_to_block(drop_key);
    let key_ptr_val = builder.use_var(key_ptr_var);
    let key_len_val = builder.use_var(key_len_var);
    let key_cap_val = builder.use_var(key_cap_var);
    builder.ins().call(
        drop_owned_string_ref,
        &[key_ptr_val, key_len_val, key_cap_val],
    );
    builder.ins().jump(after_drop, &[]);
    builder.seal_block(drop_key);

    builder.switch_to_block(after_drop);
    let err_code = builder.use_var(err_var);
    let err_pos = builder.use_var(pos_var);
    builder.ins().store(
        MemFlags::trusted(),
        err_code,
        scratch_ptr,
        JIT_SCRATCH_ERROR_CODE_OFFSET,
    );
    builder.ins().store(
        MemFlags::trusted(),
        err_pos,
        scratch_ptr,
        JIT_SCRATCH_ERROR_POS_OFFSET,
    );
    let minus_one = builder.ins().iconst(pointer_type, -1i64);
    builder.ins().return_(&[minus_one]);
    builder.seal_block(after_drop);
    builder.seal_block(error);

    builder.finalize();

    if let Err(_e) = module.define_function(func_id, &mut ctx) {
        jit_debug!("[compile_map] define_function failed: {:?}", _e);
        return None;
    }

    jit_debug!("[compile_map] SUCCESS - HashMap<String, V> function compiled");
    Some(func_id)
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
    /// Linear scan for small structs (< 10 fields)
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

/// Compile a Tier-2 struct deserializer.
///
/// Generates IR that uses the map protocol to deserialize struct fields:
/// - map_begin() -> is_end() loop -> read_key() -> match field -> deserialize value -> kv_sep() -> next()
/// - Unknown fields are skipped via emit_skip_value()
/// - Missing optional fields (`Option<T>`) are pre-initialized to None
/// - Missing required fields cause an error
fn compile_struct_format_deserializer<F: JitFormat>(
    module: &mut JITModule,
    shape: &'static Shape,
    memo: &mut ShapeMemo,
) -> Option<FuncId> {
    jit_diag!("compile_struct_format_deserializer ENTRY");
    jit_debug!("[compile_struct] ═══ ENTRY ═══");
    jit_debug!("[compile_struct] Shape type: {:?}", shape.ty);

    // Check memo first - return cached FuncId if already compiled
    let shape_ptr = shape as *const Shape;
    if let Some(&func_id) = memo.get(&shape_ptr) {
        jit_diag!(
            "compile_struct_format_deserializer: using memoized FuncId for shape {:p}",
            shape
        );
        return Some(func_id);
    }

    let Type::User(UserType::Struct(struct_def)) = &shape.ty else {
        jit_debug!("[compile_struct] ✗ FAIL: Not a struct");
        jit_diag!("Shape is not a struct");
        return None;
    };

    jit_debug!(
        "[compile_struct] Compiling struct with {} fields",
        struct_def.fields.len()
    );

    // Build field metadata - separate normal fields from flattened enum variants
    //
    // Phase 1: Identify flattened enum fields and assign "seen" bit indices
    let mut enum_field_to_seen_bit: HashMap<usize, u8> = HashMap::new();
    let mut enum_seen_bit_count = 0u8;

    for (field_idx, field) in struct_def.fields.iter().enumerate() {
        if field.is_flattened() {
            let field_shape = field.shape.get();
            if let facet_core::Type::User(facet_core::UserType::Enum(_)) = &field_shape.ty {
                // Assign a unique "seen" bit for this enum field
                enum_field_to_seen_bit.insert(field_idx, enum_seen_bit_count);
                enum_seen_bit_count += 1;
            }
        }
    }

    jit_diag!(
        "Identified {} flattened enum fields requiring 'seen' tracking",
        enum_seen_bit_count
    );

    // Phase 2: Build field_infos and flatten_variants with assigned bit indices
    // Note: Flattened struct fields are added directly to field_infos with combined offsets
    let mut field_infos = Vec::new();
    let mut flatten_variants = Vec::new();
    let mut flatten_map: Option<FlattenedMapInfo> = None;
    let mut required_count = 0u8;

    for (field_idx, field) in struct_def.fields.iter().enumerate() {
        // Get serialized name (prefer rename, fall back to name)
        let name = field.rename.unwrap_or(field.name);

        // Get field shape
        let field_shape = field.shape.get();

        jit_debug!(
            "[compile_struct]   Field '{}': shape.def = {:?}",
            name,
            field_shape.def
        );

        // Check if this is a flattened field
        if field.is_flattened() {
            // Handle flattened enums
            if let facet_core::Type::User(facet_core::UserType::Enum(enum_type)) = &field_shape.ty {
                let enum_seen_bit = *enum_field_to_seen_bit.get(&field_idx).unwrap();

                jit_diag!(
                    "Processing flattened enum field '{}' with {} variants (seen bit={})",
                    name,
                    enum_type.variants.len(),
                    enum_seen_bit
                );

                // Extract all variants and add as dispatch targets
                for variant in enum_type.variants {
                    let variant_name = variant.name;

                    // Get discriminant value (required for #[repr(C)] enums)
                    let discriminant = variant.discriminant.unwrap_or(0) as usize;

                    // Get payload shape and offset (first field of tuple variant)
                    // The offset already accounts for discriminant size/alignment per Variant docs
                    let payload_shape = variant.data.fields[0].shape();
                    let payload_offset_in_enum = variant.data.fields[0].offset;

                    jit_diag!(
                        "  Adding variant '{}' with discriminant {}, payload offset {}",
                        variant_name,
                        discriminant,
                        payload_offset_in_enum
                    );

                    flatten_variants.push(FlattenedVariantInfo {
                        variant_name,
                        enum_field_offset: field.offset,
                        discriminant,
                        payload_shape,
                        payload_offset_in_enum,
                        enum_seen_bit_index: enum_seen_bit,
                    });
                }

                // Don't add flattened enum to field_infos - it's handled via variants
                continue;
            }
            // Handle flattened structs
            else if let facet_core::Type::User(facet_core::UserType::Struct(inner_struct_def)) =
                &field_shape.ty
            {
                jit_diag!(
                    "Processing flattened struct field '{}' with {} inner fields",
                    name,
                    inner_struct_def.fields.len()
                );

                // Add inner fields directly to field_infos with combined offsets
                // This allows us to reuse all the existing field parsing logic
                for inner_field in inner_struct_def.fields {
                    let inner_field_name = inner_field.rename.unwrap_or(inner_field.name);
                    let inner_field_shape = inner_field.shape.get();

                    // Check if inner field type is supported
                    if !is_format_jit_field_type_supported(inner_field_shape) {
                        jit_diag!(
                            "  Flattened struct '{}' contains unsupported field '{}': {:?}",
                            name,
                            inner_field_name,
                            inner_field_shape.def
                        );
                        return None;
                    }

                    // Check if this inner field is Option<T>
                    let is_inner_option = matches!(inner_field_shape.def, Def::Option(_));

                    // Assign required bit index if not Option and no default
                    let inner_required_bit_index = if !is_inner_option && !inner_field.has_default()
                    {
                        let bit = required_count;
                        required_count += 1;
                        Some(bit)
                    } else {
                        None
                    };

                    // Compute combined offset: parent struct offset + inner field offset
                    let combined_offset = field.offset + inner_field.offset;

                    jit_diag!(
                        "  Adding flattened field '{}' at combined offset {} (parent {} + inner {})",
                        inner_field_name,
                        combined_offset,
                        field.offset,
                        inner_field.offset
                    );

                    // Add to field_infos as a normal field with adjusted offset
                    field_infos.push(FieldCodegenInfo {
                        name: inner_field_name,
                        offset: combined_offset,
                        shape: inner_field_shape,
                        is_option: is_inner_option,
                        required_bit_index: inner_required_bit_index,
                    });
                }

                // Don't add the flattened struct itself to field_infos - it's replaced by its fields
                continue;
            }
            // Handle flattened maps (for unknown key capture)
            else if let Def::Map(map_def) = &field_shape.def {
                jit_diag!(
                    "Processing flattened map field '{}' for unknown key capture",
                    name
                );

                // Validate: only one flattened map allowed
                if flatten_map.is_some() {
                    jit_diag!(
                        "Multiple flattened maps are not allowed - field '{}' conflicts with previous flattened map",
                        name
                    );
                    return None;
                }

                // Validate: key must be String
                if map_def.k.scalar_type() != Some(facet_core::ScalarType::String) {
                    jit_diag!(
                        "Flattened map field '{}' must have String keys, found {:?}",
                        name,
                        map_def.k.scalar_type()
                    );
                    return None;
                }

                // Validate: value type must be Tier-2 compatible
                let value_shape = map_def.v;
                let value_kind = match FormatListElementKind::from_shape(value_shape) {
                    Some(kind) => kind,
                    None => {
                        jit_diag!(
                            "Flattened map field '{}' has unsupported value type: {:?}",
                            name,
                            value_shape.def
                        );
                        return None;
                    }
                };

                jit_diag!(
                    "  Flattened map '{}' will capture unknown keys with value type {:?}",
                    name,
                    value_shape.def
                );

                flatten_map = Some(FlattenedMapInfo {
                    map_field_offset: field.offset,
                    value_shape,
                    value_kind,
                });

                // Don't add the flattened map to field_infos - it's handled via unknown_key logic
                continue;
            } else {
                // Unsupported flattened type
                jit_diag!(
                    "Flattened field '{}' has unsupported type: {:?}",
                    name,
                    field_shape.ty
                );
                return None;
            }
        }

        jit_debug!(
            "[compile_struct]   Field '{}': scalar_type = {:?}",
            name,
            field_shape.scalar_type()
        );

        // Check if this is Option<T>
        let is_option = matches!(field_shape.def, Def::Option(_));

        // Assign required bit index if not Option and no default
        let required_bit_index = if !is_option && !field.has_default() {
            let bit = required_count;
            required_count += 1;
            Some(bit)
        } else {
            None
        };

        field_infos.push(FieldCodegenInfo {
            name,
            offset: field.offset,
            shape: field_shape,
            is_option,
            required_bit_index,
        });
    }

    jit_debug!("[compile_struct] Required fields: {}", required_count);
    jit_diag!(
        "Built field metadata: {} fields (including flattened), {} flattened enum variants, {} flattened map",
        field_infos.len(),
        flatten_variants.len(),
        if flatten_map.is_some() { 1 } else { 0 }
    );

    // Check field count limit: we use u64 bitsets for tracking required fields and enum seen bits
    // Valid bit indices are 0-63, so we can track at most 64 bits total
    // (required_count uses bits 0..required_count-1, enum_seen_bit_count uses the remaining bits)
    let total_tracking_bits = required_count as usize + enum_seen_bit_count as usize;
    if total_tracking_bits >= 64 {
        jit_diag!(
            "Struct has too many tracking bits ({} required fields + {} flattened enums = {} total bits) - maximum is 63",
            required_count,
            enum_seen_bit_count,
            total_tracking_bits
        );
        return None;
    }

    // Phase 3: Detect dispatch key collisions (normal fields vs flattened enum variants)
    let mut seen_keys: HashMap<&'static str, &str> = HashMap::new();

    // Check normal field names
    for field_info in &field_infos {
        if let Some(conflicting_source) = seen_keys.insert(field_info.name, "field") {
            jit_diag!(
                "Dispatch collision: field '{}' conflicts with {} key",
                field_info.name,
                conflicting_source
            );
            return None;
        }
    }

    // Check variant names against field names
    for variant_info in &flatten_variants {
        if let Some(conflicting_source) = seen_keys.insert(variant_info.variant_name, "variant") {
            jit_diag!(
                "Dispatch collision: variant '{}' conflicts with {} key",
                variant_info.variant_name,
                conflicting_source
            );
            return None;
        }
    }

    jit_diag!(
        "Dispatch collision check passed: {} unique keys",
        seen_keys.len()
    );

    // Build unified dispatch table: normal fields + flattened enum variants
    let mut dispatch_entries: Vec<(&'static str, DispatchTarget)> = Vec::new();

    for (idx, field_info) in field_infos.iter().enumerate() {
        dispatch_entries.push((field_info.name, DispatchTarget::Field(idx)));
    }

    for (idx, variant_info) in flatten_variants.iter().enumerate() {
        dispatch_entries.push((
            variant_info.variant_name,
            DispatchTarget::FlattenEnumVariant(idx),
        ));
    }

    jit_diag!(
        "Built dispatch table with {} total entries",
        dispatch_entries.len()
    );

    // Analyze and determine key dispatch strategy (using combined dispatch table)
    let dispatch_strategy = if dispatch_entries.len() < 10 {
        KeyDispatchStrategy::Linear
    } else {
        // Prefix dispatch requires that all dispatch keys are at least prefix_len bytes.
        // Otherwise, short keys (e.g. "id") would never match and we'd treat them as unknown.
        let min_key_len = dispatch_entries
            .iter()
            .map(|(name, _)| name.len())
            .min()
            .unwrap_or(0);
        if min_key_len < 4 {
            KeyDispatchStrategy::Linear
        } else {
            KeyDispatchStrategy::PrefixSwitch { prefix_len: 4 }
        }
    };

    let pointer_type = module.target_config().pointer_type();

    // Function signature: fn(input_ptr, len, pos, out, scratch) -> isize
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(pointer_type)); // input_ptr
    sig.params.push(AbiParam::new(pointer_type)); // len
    sig.params.push(AbiParam::new(pointer_type)); // pos
    sig.params.push(AbiParam::new(pointer_type)); // out
    sig.params.push(AbiParam::new(pointer_type)); // scratch
    sig.returns.push(AbiParam::new(pointer_type)); // new_pos or error

    // Create unique function name using shape pointer address
    let func_name = format!("jit_deserialize_struct_{:x}", shape as *const _ as usize);

    let func_id = match module.declare_function(&func_name, Linkage::Export, &sig) {
        Ok(id) => id,
        Err(e) => {
            jit_debug!("[compile_struct] ✗ FAIL: declare_function failed: {:?}", e);
            jit_diag!("declare_function('{}') failed: {:?}", func_name, e);
            return None;
        }
    };
    jit_debug!(
        "[compile_struct] ✓ Function '{}' declared successfully",
        func_name
    );

    // Insert into memo immediately after declaration (before IR build) to avoid recursion/cycles
    memo.insert(shape_ptr, func_id);
    jit_diag!(
        "compile_struct_format_deserializer: memoized FuncId for shape {:p}",
        shape
    );
    jit_diag!("Function declared, starting IR generation");

    let mut ctx = module.make_context();
    ctx.func.signature = sig;

    let mut builder_ctx = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut builder_ctx);
        let entry = builder.create_block();
        builder.switch_to_block(entry);
        builder.append_block_params_for_function_params(entry);

        // Get function parameters
        let input_ptr = builder.block_params(entry)[0];
        let len = builder.block_params(entry)[1];
        let pos_param = builder.block_params(entry)[2];
        let out_ptr = builder.block_params(entry)[3];
        let scratch_ptr = builder.block_params(entry)[4];

        // Create position variable (mutable)
        let pos_var = builder.declare_var(pointer_type);
        builder.def_var(pos_var, pos_param);

        // Variable for error code
        let err_var = builder.declare_var(types::I32);
        let zero_i32 = builder.ins().iconst(types::I32, 0);
        builder.def_var(err_var, zero_i32);

        // Variable for required fields bitset (u64)
        let required_bits_var = builder.declare_var(types::I64);
        let zero_i64 = builder.ins().iconst(types::I64, 0);
        builder.def_var(required_bits_var, zero_i64);

        // Variable for enum "seen" tracking bitset (one bit per flattened enum field)
        let enum_seen_bits_var = builder.declare_var(types::I64);
        builder.def_var(enum_seen_bits_var, zero_i64);

        // Variable for tracking whether flattened map has been initialized (only if flatten_map exists)
        let map_initialized_var = if flatten_map.is_some() {
            let var = builder.declare_var(types::I8);
            let zero_i8 = builder.ins().iconst(types::I8, 0);
            builder.def_var(var, zero_i8);
            Some(var)
        } else {
            None
        };

        // Create basic blocks
        let map_begin = builder.create_block();
        let check_map_begin_err = builder.create_block();
        let init_options = builder.create_block();
        let loop_check_end = builder.create_block();
        let check_is_end_err = builder.create_block();
        let check_is_end_value = builder.create_block();
        let read_key = builder.create_block();
        let check_read_key_err = builder.create_block();
        let key_dispatch = builder.create_block();
        let unknown_key = builder.create_block();
        let after_value = builder.create_block();
        let check_map_next_err = builder.create_block();
        let validate_required = builder.create_block();
        let success = builder.create_block();
        let error = builder.create_block();

        // Allocate stack slot for map state if needed
        let state_ptr = if F::MAP_STATE_SIZE > 0 {
            let align_shift = F::MAP_STATE_ALIGN.trailing_zeros() as u8;
            let slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                F::MAP_STATE_SIZE,
                align_shift,
            ));
            builder.ins().stack_addr(pointer_type, slot, 0)
        } else {
            builder.ins().iconst(pointer_type, 0)
        };

        // Jump to map_begin
        builder.ins().jump(map_begin, &[]);
        builder.seal_block(entry);

        // map_begin: consume map start delimiter
        builder.switch_to_block(map_begin);
        let mut cursor = JitCursor {
            input_ptr,
            len,
            pos: pos_var,
            ptr_type: pointer_type,
        };
        let format = F::default();
        let err_code = format.emit_map_begin(module, &mut builder, &mut cursor, state_ptr);
        builder.def_var(err_var, err_code);
        builder.ins().jump(check_map_begin_err, &[]);
        builder.seal_block(map_begin);

        // check_map_begin_err
        builder.switch_to_block(check_map_begin_err);
        let is_ok = builder.ins().icmp_imm(IntCC::Equal, err_code, 0);
        builder.ins().brif(is_ok, init_options, &[], error, &[]);
        builder.seal_block(check_map_begin_err);

        // init_options: pre-initialize Option fields to None
        builder.switch_to_block(init_options);

        // Declare jit_option_init_none helper signature
        let sig_option_init_none = {
            let mut s = module.make_signature();
            s.params.push(AbiParam::new(pointer_type)); // out_ptr
            s.params.push(AbiParam::new(pointer_type)); // init_none_fn
            s
        };

        let option_init_none_id = match module.declare_function(
            "jit_option_init_none",
            Linkage::Import,
            &sig_option_init_none,
        ) {
            Ok(id) => id,
            Err(e) => {
                jit_debug!(
                    "[compile_struct] declare jit_option_init_none failed: {:?}",
                    e
                );
                jit_diag!("declare_function('jit_option_init_none') failed: {:?}", e);
                return None;
            }
        };
        let option_init_none_ref = module.declare_func_in_func(option_init_none_id, builder.func);

        // Pre-initialize all Option<T> fields to None (normal fields)
        for field_info in &field_infos {
            if field_info.is_option {
                // Get the OptionDef from the field shape
                if let Def::Option(opt_def) = &field_info.shape.def {
                    let field_ptr = builder.ins().iadd_imm(out_ptr, field_info.offset as i64);
                    let init_none_fn_ptr = builder
                        .ins()
                        .iconst(pointer_type, opt_def.vtable.init_none as *const () as i64);
                    builder
                        .ins()
                        .call(option_init_none_ref, &[field_ptr, init_none_fn_ptr]);
                }
            }
        }

        builder.ins().jump(loop_check_end, &[]);
        builder.seal_block(init_options);

        // loop_check_end: check if we're at map end
        builder.switch_to_block(loop_check_end);

        let mut cursor = JitCursor {
            input_ptr,
            len,
            pos: pos_var,
            ptr_type: pointer_type,
        };

        // Call emit_map_is_end to check if we're done
        let format = F::default();
        let (is_end_i8, err_code) =
            format.emit_map_is_end(module, &mut builder, &mut cursor, state_ptr);
        builder.def_var(err_var, err_code);

        builder.ins().jump(check_is_end_err, &[]);
        // Note: loop_check_end will be sealed after check_map_next_err

        // check_is_end_err
        builder.switch_to_block(check_is_end_err);
        let is_ok = builder.ins().icmp_imm(IntCC::Equal, err_code, 0);
        builder
            .ins()
            .brif(is_ok, check_is_end_value, &[], error, &[]);
        builder.seal_block(check_is_end_err);

        // check_is_end_value: branch based on is_end
        builder.switch_to_block(check_is_end_value);
        let is_end = builder.ins().uextend(pointer_type, is_end_i8);
        let is_end_bool = builder.ins().icmp_imm(IntCC::NotEqual, is_end, 0);
        builder
            .ins()
            .brif(is_end_bool, validate_required, &[], read_key, &[]);
        builder.seal_block(check_is_end_value);

        // validate_required: check all required fields were set
        builder.switch_to_block(validate_required);

        if required_count > 0 {
            // Compute required_mask: all bits for required fields
            let required_mask = (1u64 << required_count) - 1;
            let mask_val = builder.ins().iconst(types::I64, required_mask as i64);

            let bits = builder.use_var(required_bits_var);
            let bits_masked = builder.ins().band(bits, mask_val);

            // Check if (bits_masked == mask_val)
            let all_set = builder.ins().icmp(IntCC::Equal, bits_masked, mask_val);

            // If not all set, set error and jump to error block
            let required_ok = builder.create_block();
            let required_fail = builder.create_block();
            builder
                .ins()
                .brif(all_set, required_ok, &[], required_fail, &[]);

            // required_fail: set ERR_MISSING_REQUIRED_FIELD and error
            builder.switch_to_block(required_fail);
            let err = builder
                .ins()
                .iconst(types::I32, helpers::ERR_MISSING_REQUIRED_FIELD as i64);
            builder.def_var(err_var, err);
            builder.ins().jump(error, &[]);
            builder.seal_block(required_fail);

            // required_ok: continue to success
            builder.switch_to_block(required_ok);
            builder.ins().jump(success, &[]);
            builder.seal_block(required_ok);
        } else {
            // No required fields, go straight to success
            builder.ins().jump(success, &[]);
        }

        builder.seal_block(validate_required);

        // success: return new position
        builder.switch_to_block(success);

        // Initialize flattened map to empty if it exists but was never initialized (no unknown keys)
        if let Some(flatten_map_info) = &flatten_map {
            let map_initialized_var = map_initialized_var.unwrap();
            let map_initialized = builder.use_var(map_initialized_var);
            let already_initialized = builder.ins().icmp_imm(IntCC::NotEqual, map_initialized, 0);
            let init_empty_map = builder.create_block();
            let after_empty_init = builder.create_block();
            builder.ins().brif(
                already_initialized,
                after_empty_init,
                &[],
                init_empty_map,
                &[],
            );

            // init_empty_map: initialize to empty HashMap
            builder.switch_to_block(init_empty_map);
            jit_diag!("Initializing flattened map to empty (no unknown keys encountered)");

            let map_ptr = builder
                .ins()
                .iadd_imm(out_ptr, flatten_map_info.map_field_offset as i64);

            // Get map init function (already computed during field metadata building)
            let map_shape = {
                let mut found_shape = None;
                for field in struct_def.fields {
                    if field.is_flattened() {
                        let field_shape = field.shape.get();
                        if let Def::Map(_) = &field_shape.def
                            && field.offset == flatten_map_info.map_field_offset
                        {
                            found_shape = Some(field_shape);
                            break;
                        }
                    }
                }
                found_shape.expect("flattened map shape must exist")
            };

            let map_def = match &map_shape.def {
                Def::Map(m) => m,
                _ => unreachable!("flatten_map_info must be from a Map"),
            };

            let init_fn = map_def.vtable.init_in_place_with_capacity;

            // Declare jit_map_init_with_capacity helper
            let map_init_ref = {
                let mut s = module.make_signature();
                s.params.push(AbiParam::new(pointer_type)); // out_ptr
                s.params.push(AbiParam::new(pointer_type)); // capacity
                s.params.push(AbiParam::new(pointer_type)); // init_fn
                let id = module
                    .declare_function("jit_map_init_with_capacity", Linkage::Import, &s)
                    .ok()?;
                module.declare_func_in_func(id, builder.func)
            };

            // Call jit_map_init_with_capacity(map_ptr, 0, init_fn)
            let zero_capacity = builder.ins().iconst(pointer_type, 0);
            let init_fn_ptr = builder.ins().iconst(pointer_type, init_fn as usize as i64);
            builder
                .ins()
                .call(map_init_ref, &[map_ptr, zero_capacity, init_fn_ptr]);

            builder.ins().jump(after_empty_init, &[]);
            builder.seal_block(init_empty_map);

            // after_empty_init: continue to return
            builder.switch_to_block(after_empty_init);
            builder.seal_block(after_empty_init);
        }

        let final_pos = builder.use_var(pos_var);
        builder.ins().return_(&[final_pos]);
        builder.seal_block(success);

        // error: write scratch and return -1
        builder.switch_to_block(error);
        let err_code = builder.use_var(err_var);
        let err_pos = builder.use_var(pos_var);
        builder.ins().store(
            MemFlags::trusted(),
            err_code,
            scratch_ptr,
            JIT_SCRATCH_ERROR_CODE_OFFSET,
        );
        builder.ins().store(
            MemFlags::trusted(),
            err_pos,
            scratch_ptr,
            JIT_SCRATCH_ERROR_POS_OFFSET,
        );
        let neg_one = builder.ins().iconst(pointer_type, -1i64);
        builder.ins().return_(&[neg_one]);
        // Note: error block will be sealed later, after all branches to it

        // read_key: read the map key
        builder.switch_to_block(read_key);

        let mut cursor = JitCursor {
            input_ptr,
            len,
            pos: pos_var,
            ptr_type: pointer_type,
        };

        let format = F::default();
        let (key_value, err_code) =
            format.emit_map_read_key(module, &mut builder, &mut cursor, state_ptr);
        builder.def_var(err_var, err_code);

        // Store key value in variables for use in dispatch
        let key_ptr_var = builder.declare_var(pointer_type);
        let key_len_var = builder.declare_var(pointer_type);
        let key_cap_var = builder.declare_var(pointer_type);
        let key_owned_var = builder.declare_var(types::I8);
        builder.def_var(key_ptr_var, key_value.ptr);
        builder.def_var(key_len_var, key_value.len);
        builder.def_var(key_cap_var, key_value.cap);
        builder.def_var(key_owned_var, key_value.owned);

        builder.ins().jump(check_read_key_err, &[]);
        builder.seal_block(read_key);

        // check_read_key_err
        builder.switch_to_block(check_read_key_err);
        let is_ok = builder.ins().icmp_imm(IntCC::Equal, err_code, 0);
        builder.ins().brif(is_ok, key_dispatch, &[], error, &[]);
        builder.seal_block(check_read_key_err);

        // key_dispatch: match the key against field names
        builder.switch_to_block(key_dispatch);

        // For each dispatch entry (field or variant), create a match block
        let mut match_blocks = Vec::new();
        for _ in &dispatch_entries {
            match_blocks.push(builder.create_block());
        }

        // Handle empty dispatch table (only flattened map, no normal fields/variants)
        if dispatch_entries.is_empty() {
            builder.ins().jump(unknown_key, &[]);
            builder.seal_block(key_dispatch);
        } else {
            // Get key pointer and length
            let key_ptr = builder.use_var(key_ptr_var);
            let key_len = builder.use_var(key_len_var);

            // Dispatch based on strategy
            match dispatch_strategy {
                KeyDispatchStrategy::Linear => {
                    // Linear scan for small structs
                    let mut current_block = key_dispatch;
                    for (i, (key_name, _target)) in dispatch_entries.iter().enumerate() {
                        if i > 0 {
                            builder.switch_to_block(current_block);
                        }

                        let key_name_len = key_name.len();

                        // First check length
                        let len_matches =
                            builder
                                .ins()
                                .icmp_imm(IntCC::Equal, key_len, key_name_len as i64);

                        let check_content = builder.create_block();
                        let next_check = if i + 1 < dispatch_entries.len() {
                            builder.create_block()
                        } else {
                            unknown_key
                        };

                        builder
                            .ins()
                            .brif(len_matches, check_content, &[], next_check, &[]);
                        if i > 0 {
                            builder.seal_block(current_block);
                        }

                        // check_content: byte-by-byte comparison
                        builder.switch_to_block(check_content);
                        let mut all_match = builder.ins().iconst(types::I8, 1);

                        for (j, &byte) in key_name.as_bytes().iter().enumerate() {
                            let offset = builder.ins().iconst(pointer_type, j as i64);
                            let char_ptr = builder.ins().iadd(key_ptr, offset);
                            let char_val =
                                builder
                                    .ins()
                                    .load(types::I8, MemFlags::trusted(), char_ptr, 0);
                            let expected = builder.ins().iconst(types::I8, byte as i64);
                            let byte_matches = builder.ins().icmp(IntCC::Equal, char_val, expected);
                            let one = builder.ins().iconst(types::I8, 1);
                            let zero = builder.ins().iconst(types::I8, 0);
                            let byte_match_i8 = builder.ins().select(byte_matches, one, zero);
                            all_match = builder.ins().band(all_match, byte_match_i8);
                        }

                        let all_match_bool = builder.ins().icmp_imm(IntCC::NotEqual, all_match, 0);
                        builder
                            .ins()
                            .brif(all_match_bool, match_blocks[i], &[], next_check, &[]);
                        builder.seal_block(check_content);

                        current_block = next_check;
                    }

                    builder.seal_block(key_dispatch);
                    if dispatch_entries.len() > 1 {
                        builder.seal_block(current_block);
                    }
                }
                KeyDispatchStrategy::PrefixSwitch { prefix_len } => {
                    // Prefix-based dispatch for larger structs
                    // Group fields by prefix
                    use std::collections::HashMap;
                    let mut prefix_map: HashMap<u64, Vec<usize>> = HashMap::new();

                    for (i, field_info) in field_infos.iter().enumerate() {
                        let (prefix, _) = compute_field_prefix(field_info.name, prefix_len);
                        prefix_map.entry(prefix).or_default().push(i);
                    }

                    // Load prefix from key (handle short keys gracefully)
                    // Use a variable to hold the prefix value
                    let prefix_var = builder.declare_var(types::I64);

                    // First check if key is long enough for the full prefix
                    let prefix_len_i64 = prefix_len as i64;
                    let has_full_prefix = builder.ins().icmp_imm(
                        IntCC::UnsignedGreaterThanOrEqual,
                        key_len,
                        prefix_len_i64,
                    );

                    let load_full_prefix_block = builder.create_block();
                    let load_partial_prefix_block = builder.create_block();
                    let prefix_loaded_block = builder.create_block();

                    builder.ins().brif(
                        has_full_prefix,
                        load_full_prefix_block,
                        &[],
                        load_partial_prefix_block,
                        &[],
                    );

                    // Load full prefix
                    builder.switch_to_block(load_full_prefix_block);
                    // Note: key_ptr is a *const u8 into input slice, NOT guaranteed aligned
                    // Use unaligned load to avoid UB on some targets
                    let prefix_u64 =
                        builder
                            .ins()
                            .load(types::I64, MemFlags::trusted(), key_ptr, 0);
                    builder.def_var(prefix_var, prefix_u64);
                    builder.ins().jump(prefix_loaded_block, &[]);
                    builder.seal_block(load_full_prefix_block);

                    // Load partial prefix (byte by byte for short keys)
                    builder.switch_to_block(load_partial_prefix_block);
                    let partial_prefix = builder.ins().iconst(types::I64, 0);
                    // For simplicity, just set to 0 for short keys (they'll fall through to linear check)
                    builder.def_var(prefix_var, partial_prefix);
                    builder.ins().jump(prefix_loaded_block, &[]);
                    builder.seal_block(load_partial_prefix_block);

                    // prefix_loaded_block uses the variable
                    builder.switch_to_block(prefix_loaded_block);
                    let loaded_prefix_raw = builder.use_var(prefix_var);

                    // Mask the loaded prefix to only use prefix_len bytes
                    // compute_field_prefix only packs prefix_len bytes, but we load 8 bytes
                    // so we need to mask out the high bytes
                    let loaded_prefix = if prefix_len < 8 {
                        // mask = (1 << (prefix_len * 8)) - 1
                        let mask = (1u64 << (prefix_len * 8)) - 1;
                        let mask_val = builder.ins().iconst(types::I64, mask as i64);
                        builder.ins().band(loaded_prefix_raw, mask_val)
                    } else {
                        // prefix_len == 8, no masking needed
                        loaded_prefix_raw
                    };

                    // Build a switch table (cranelift expects u128 for EntryIndex)
                    // First, create disambiguation blocks for collisions and store them
                    let mut disambig_blocks: HashMap<u64, Block> = HashMap::new();

                    for (prefix_val, field_indices) in &prefix_map {
                        if field_indices.len() > 1 {
                            // Collision - create disambiguation block
                            let disambig_block = builder.create_block();
                            disambig_blocks.insert(*prefix_val, disambig_block);
                        }
                    }

                    // Build the switch table
                    let mut switch_data = cranelift::frontend::Switch::new();
                    let fallback_block = unknown_key;

                    for (prefix_val, field_indices) in &prefix_map {
                        if field_indices.len() == 1 {
                            // Unique prefix - direct match
                            let field_idx = field_indices[0];
                            switch_data.set_entry(*prefix_val as u128, match_blocks[field_idx]);
                        } else {
                            // Collision - use pre-created disambiguation block
                            let disambig_block = disambig_blocks[prefix_val];
                            switch_data.set_entry(*prefix_val as u128, disambig_block);
                        }
                    }

                    switch_data.emit(&mut builder, loaded_prefix, fallback_block);
                    builder.seal_block(prefix_loaded_block);

                    // Generate code for disambiguation blocks
                    for (prefix_val, field_indices) in &prefix_map {
                        if field_indices.len() > 1 {
                            // Collision case - need to check full string
                            let disambig_block = disambig_blocks[prefix_val];
                            builder.switch_to_block(disambig_block);

                            // Seal disambig_block immediately as it only has one predecessor (the switch)
                            builder.seal_block(disambig_block);

                            let mut current_check_block = disambig_block;
                            for (j, &field_idx) in field_indices.iter().enumerate() {
                                if j > 0 {
                                    builder.switch_to_block(current_check_block);
                                }

                                let field_name = field_infos[field_idx].name;
                                let field_name_len = field_name.len();

                                // Check length first
                                let len_matches = builder.ins().icmp_imm(
                                    IntCC::Equal,
                                    key_len,
                                    field_name_len as i64,
                                );

                                let check_full_match = builder.create_block();
                                let next_in_collision = if j + 1 < field_indices.len() {
                                    builder.create_block()
                                } else {
                                    fallback_block
                                };

                                builder.ins().brif(
                                    len_matches,
                                    check_full_match,
                                    &[],
                                    next_in_collision,
                                    &[],
                                );

                                // check_full_match: full string comparison
                                builder.switch_to_block(check_full_match);
                                let mut all_match = builder.ins().iconst(types::I8, 1);

                                for (k, &byte) in field_name.as_bytes().iter().enumerate() {
                                    let offset = builder.ins().iconst(pointer_type, k as i64);
                                    let char_ptr = builder.ins().iadd(key_ptr, offset);
                                    let char_val = builder.ins().load(
                                        types::I8,
                                        MemFlags::trusted(),
                                        char_ptr,
                                        0,
                                    );
                                    let expected = builder.ins().iconst(types::I8, byte as i64);
                                    let byte_matches =
                                        builder.ins().icmp(IntCC::Equal, char_val, expected);
                                    let one = builder.ins().iconst(types::I8, 1);
                                    let zero = builder.ins().iconst(types::I8, 0);
                                    let byte_match_i8 =
                                        builder.ins().select(byte_matches, one, zero);
                                    all_match = builder.ins().band(all_match, byte_match_i8);
                                }

                                let all_match_bool =
                                    builder.ins().icmp_imm(IntCC::NotEqual, all_match, 0);
                                builder.ins().brif(
                                    all_match_bool,
                                    match_blocks[field_idx],
                                    &[],
                                    next_in_collision,
                                    &[],
                                );
                                builder.seal_block(check_full_match);

                                // Now seal next_in_collision - both its predecessors are filled:
                                // 1. The brif from current_check_block's length check
                                // 2. The brif from check_full_match's full match check
                                if next_in_collision != fallback_block {
                                    builder.seal_block(next_in_collision);
                                }

                                // current_check_block was already sealed:
                                // - j=0: it's disambig_block (sealed before loop)
                                // - j>0: it's previous iteration's next_in_collision (sealed above)

                                current_check_block = next_in_collision;
                            }
                        }
                    }

                    // Seal unknown_key now that all predecessors are known:
                    // 1. Switch fallback (line 2196)
                    // 2. All disambiguation chain fallbacks (line 2229)
                    builder.seal_block(unknown_key);

                    builder.seal_block(key_dispatch);
                }
            } // end else (non-empty dispatch table)
        }

        // unknown_key: either insert into flattened map or skip the value
        builder.switch_to_block(unknown_key);

        let mut cursor = JitCursor {
            input_ptr,
            len,
            pos: pos_var,
            ptr_type: pointer_type,
        };

        // First consume the kv separator
        let format = F::default();
        let err_code = format.emit_map_kv_sep(module, &mut builder, &mut cursor, state_ptr);
        builder.def_var(err_var, err_code);

        // Check for error
        let kv_sep_ok = builder.create_block();
        let is_ok = builder.ins().icmp_imm(IntCC::Equal, err_code, 0);
        builder.ins().brif(is_ok, kv_sep_ok, &[], error, &[]);

        builder.switch_to_block(kv_sep_ok);

        // Branch on whether we have a flattened map for unknown key capture
        if let Some(flatten_map_info) = &flatten_map {
            jit_diag!(
                "Unknown key handler: capturing into flattened map at offset {}",
                flatten_map_info.map_field_offset
            );

            // Lazy-init the map if not already initialized
            let map_initialized_var = map_initialized_var.unwrap();
            let map_initialized = builder.use_var(map_initialized_var);
            let already_initialized = builder.ins().icmp_imm(IntCC::NotEqual, map_initialized, 0);
            let init_map = builder.create_block();
            let after_init = builder.create_block();
            builder
                .ins()
                .brif(already_initialized, after_init, &[], init_map, &[]);

            // init_map: initialize the HashMap with capacity 0
            builder.switch_to_block(init_map);
            jit_diag!("  Initializing flattened map on first unknown key");

            // Get map field pointer
            let map_ptr = builder
                .ins()
                .iadd_imm(out_ptr, flatten_map_info.map_field_offset as i64);

            // Get map init function from shape
            let map_shape = {
                // Reconstruct the map shape from the struct field
                // We need to find the corresponding field in struct_def
                let mut found_shape = None;
                for field in struct_def.fields {
                    if field.is_flattened() {
                        let field_shape = field.shape.get();
                        if let Def::Map(_) = &field_shape.def
                            && field.offset == flatten_map_info.map_field_offset
                        {
                            found_shape = Some(field_shape);
                            break;
                        }
                    }
                }
                found_shape.expect("flattened map shape must exist")
            };

            let map_def = match &map_shape.def {
                Def::Map(m) => m,
                _ => unreachable!("flatten_map_info must be from a Map"),
            };

            let init_fn = map_def.vtable.init_in_place_with_capacity;

            // Declare jit_map_init_with_capacity helper
            let map_init_ref = {
                let mut s = module.make_signature();
                s.params.push(AbiParam::new(pointer_type)); // out_ptr
                s.params.push(AbiParam::new(pointer_type)); // capacity
                s.params.push(AbiParam::new(pointer_type)); // init_fn
                let id = module
                    .declare_function("jit_map_init_with_capacity", Linkage::Import, &s)
                    .ok()?;
                module.declare_func_in_func(id, builder.func)
            };

            // Call jit_map_init_with_capacity(map_ptr, 0, init_fn)
            let zero_capacity = builder.ins().iconst(pointer_type, 0);
            let init_fn_ptr = builder.ins().iconst(pointer_type, init_fn as usize as i64);
            builder
                .ins()
                .call(map_init_ref, &[map_ptr, zero_capacity, init_fn_ptr]);

            // Mark map as initialized
            let one_i8 = builder.ins().iconst(types::I8, 1);
            builder.def_var(map_initialized_var, one_i8);

            builder.ins().jump(after_init, &[]);
            builder.seal_block(init_map);

            // after_init: parse the value and insert into map
            builder.switch_to_block(after_init);

            // Get map field pointer for insertion
            let map_ptr = builder
                .ins()
                .iadd_imm(out_ptr, flatten_map_info.map_field_offset as i64);

            // Get map insert function
            let map_def = match &map_shape.def {
                Def::Map(m) => m,
                _ => unreachable!("flatten_map must be a Map"),
            };
            let insert_fn = map_def.vtable.insert;

            // Declare jit_write_string helper for materializing the key
            let write_string_ref = {
                let mut s = module.make_signature();
                s.params.push(AbiParam::new(pointer_type)); // out_ptr
                s.params.push(AbiParam::new(pointer_type)); // offset
                s.params.push(AbiParam::new(pointer_type)); // str_ptr
                s.params.push(AbiParam::new(pointer_type)); // str_len
                s.params.push(AbiParam::new(pointer_type)); // str_cap
                s.params.push(AbiParam::new(types::I8)); // owned
                let id = module
                    .declare_function("jit_write_string", Linkage::Import, &s)
                    .ok()?;
                module.declare_func_in_func(id, builder.func)
            };

            // Create stack slots for key and value
            let key_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                3 * pointer_type.bytes(),
                pointer_type.bytes().trailing_zeros() as u8,
            ));
            let key_out_ptr = builder.ins().stack_addr(pointer_type, key_slot, 0);

            let value_layout = match flatten_map_info.value_shape.layout.sized_layout() {
                Ok(layout) => layout,
                Err(_) => {
                    jit_debug!("[compile_struct] Flattened map value has unsized layout");
                    return None;
                }
            };
            let value_size = value_layout.size() as u32;
            let value_align = value_layout.align().trailing_zeros() as u8;
            let value_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                value_size,
                value_align,
            ));
            let value_ptr = builder.ins().stack_addr(pointer_type, value_slot, 0);

            // Parse the value based on value_kind
            let value_shape = flatten_map_info.value_shape;
            let mut cursor = JitCursor {
                input_ptr,
                len,
                pos: pos_var,
                ptr_type: pointer_type,
            };

            // Create continuation block for after value is parsed and stored
            let value_stored = builder.create_block();

            match flatten_map_info.value_kind {
                FormatListElementKind::Bool => {
                    let (value_i8, err) = format.emit_parse_bool(module, &mut builder, &mut cursor);
                    builder.def_var(err_var, err);
                    let ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);
                    let store = builder.create_block();
                    builder.ins().brif(ok, store, &[], error, &[]);
                    builder.switch_to_block(store);
                    builder
                        .ins()
                        .store(MemFlags::trusted(), value_i8, value_ptr, 0);
                    builder.ins().jump(value_stored, &[]);
                    builder.seal_block(store);
                }
                FormatListElementKind::U8 => {
                    let (value_u8, err) = format.emit_parse_u8(module, &mut builder, &mut cursor);
                    builder.def_var(err_var, err);
                    let ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);
                    let store = builder.create_block();
                    builder.ins().brif(ok, store, &[], error, &[]);
                    builder.switch_to_block(store);
                    builder
                        .ins()
                        .store(MemFlags::trusted(), value_u8, value_ptr, 0);
                    builder.ins().jump(value_stored, &[]);
                    builder.seal_block(store);
                }
                FormatListElementKind::I64 => {
                    use facet_core::ScalarType;
                    let (value_i64, err) = format.emit_parse_i64(module, &mut builder, &mut cursor);
                    builder.def_var(err_var, err);
                    let ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);
                    let store = builder.create_block();
                    builder.ins().brif(ok, store, &[], error, &[]);
                    builder.switch_to_block(store);
                    let scalar = value_shape.scalar_type().unwrap();
                    let value = match scalar {
                        ScalarType::I8 => builder.ins().ireduce(types::I8, value_i64),
                        ScalarType::I16 => builder.ins().ireduce(types::I16, value_i64),
                        ScalarType::I32 => builder.ins().ireduce(types::I32, value_i64),
                        ScalarType::I64 => value_i64,
                        _ => value_i64,
                    };
                    builder
                        .ins()
                        .store(MemFlags::trusted(), value, value_ptr, 0);
                    builder.ins().jump(value_stored, &[]);
                    builder.seal_block(store);
                }
                FormatListElementKind::U64 => {
                    use facet_core::ScalarType;
                    let (value_u64, err) = format.emit_parse_u64(module, &mut builder, &mut cursor);
                    builder.def_var(err_var, err);
                    let ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);
                    let store = builder.create_block();
                    builder.ins().brif(ok, store, &[], error, &[]);
                    builder.switch_to_block(store);
                    let scalar = value_shape.scalar_type().unwrap();
                    let value = match scalar {
                        ScalarType::U8 => builder.ins().ireduce(types::I8, value_u64),
                        ScalarType::U16 => builder.ins().ireduce(types::I16, value_u64),
                        ScalarType::U32 => builder.ins().ireduce(types::I32, value_u64),
                        ScalarType::U64 => value_u64,
                        _ => value_u64,
                    };
                    builder
                        .ins()
                        .store(MemFlags::trusted(), value, value_ptr, 0);
                    builder.ins().jump(value_stored, &[]);
                    builder.seal_block(store);
                }
                FormatListElementKind::F64 => {
                    use facet_core::ScalarType;
                    let (value_f64, err) = format.emit_parse_f64(module, &mut builder, &mut cursor);
                    builder.def_var(err_var, err);
                    let ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);
                    let store = builder.create_block();
                    builder.ins().brif(ok, store, &[], error, &[]);
                    builder.switch_to_block(store);
                    let scalar = value_shape.scalar_type().unwrap();
                    let value = if matches!(scalar, ScalarType::F32) {
                        builder.ins().fdemote(types::F32, value_f64)
                    } else {
                        value_f64
                    };
                    builder
                        .ins()
                        .store(MemFlags::trusted(), value, value_ptr, 0);
                    builder.ins().jump(value_stored, &[]);
                    builder.seal_block(store);
                }
                FormatListElementKind::String => {
                    let (string_value, err) =
                        format.emit_parse_string(module, &mut builder, &mut cursor);
                    builder.def_var(err_var, err);
                    let ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);
                    let store = builder.create_block();
                    builder.ins().brif(ok, store, &[], error, &[]);
                    builder.switch_to_block(store);
                    let zero_offset = builder.ins().iconst(pointer_type, 0);
                    builder.ins().call(
                        write_string_ref,
                        &[
                            value_ptr,
                            zero_offset,
                            string_value.ptr,
                            string_value.len,
                            string_value.cap,
                            string_value.owned,
                        ],
                    );
                    builder.ins().jump(value_stored, &[]);
                    builder.seal_block(store);
                }
                FormatListElementKind::Struct(_) => {
                    let struct_func_id =
                        compile_struct_format_deserializer::<F>(module, value_shape, memo)?;
                    let struct_func_ref = module.declare_func_in_func(struct_func_id, builder.func);
                    let current_pos = builder.use_var(pos_var);
                    let call_result = builder.ins().call(
                        struct_func_ref,
                        &[input_ptr, len, current_pos, value_ptr, scratch_ptr],
                    );
                    let new_pos = builder.inst_results(call_result)[0];
                    let is_error = builder.ins().icmp_imm(IntCC::SignedLessThan, new_pos, 0);
                    let nested_ok = builder.create_block();
                    builder.ins().brif(is_error, error, &[], nested_ok, &[]);
                    builder.switch_to_block(nested_ok);
                    builder.def_var(pos_var, new_pos);
                    builder.ins().jump(value_stored, &[]);
                    builder.seal_block(nested_ok);
                }
                FormatListElementKind::List(_) => {
                    let list_func_id =
                        compile_list_format_deserializer::<F>(module, value_shape, memo)?;
                    let list_func_ref = module.declare_func_in_func(list_func_id, builder.func);
                    let current_pos = builder.use_var(pos_var);
                    let call_result = builder.ins().call(
                        list_func_ref,
                        &[input_ptr, len, current_pos, value_ptr, scratch_ptr],
                    );
                    let new_pos = builder.inst_results(call_result)[0];
                    let is_error = builder.ins().icmp_imm(IntCC::SignedLessThan, new_pos, 0);
                    let nested_ok = builder.create_block();
                    builder.ins().brif(is_error, error, &[], nested_ok, &[]);
                    builder.switch_to_block(nested_ok);
                    builder.def_var(pos_var, new_pos);
                    builder.ins().jump(value_stored, &[]);
                    builder.seal_block(nested_ok);
                }
                FormatListElementKind::Map(_) => {
                    let map_func_id =
                        compile_map_format_deserializer::<F>(module, value_shape, memo)?;
                    let map_func_ref = module.declare_func_in_func(map_func_id, builder.func);
                    let current_pos = builder.use_var(pos_var);
                    let call_result = builder.ins().call(
                        map_func_ref,
                        &[input_ptr, len, current_pos, value_ptr, scratch_ptr],
                    );
                    let new_pos = builder.inst_results(call_result)[0];
                    let is_error = builder.ins().icmp_imm(IntCC::SignedLessThan, new_pos, 0);
                    let nested_ok = builder.create_block();
                    builder.ins().brif(is_error, error, &[], nested_ok, &[]);
                    builder.switch_to_block(nested_ok);
                    builder.def_var(pos_var, new_pos);
                    builder.ins().jump(value_stored, &[]);
                    builder.seal_block(nested_ok);
                }
            }

            // Switch to the continuation block after value is stored
            builder.switch_to_block(value_stored);

            // Materialize key into the stack slot using write_string
            let zero_offset = builder.ins().iconst(pointer_type, 0);
            let key_ptr_raw = builder.use_var(key_ptr_var);
            let key_len_raw = builder.use_var(key_len_var);
            let key_cap_raw = builder.use_var(key_cap_var);
            let key_owned_raw = builder.use_var(key_owned_var);
            builder.ins().call(
                write_string_ref,
                &[
                    key_out_ptr,
                    zero_offset,
                    key_ptr_raw,
                    key_len_raw,
                    key_cap_raw,
                    key_owned_raw,
                ],
            );
            // Key raw parts consumed by write_string when owned=1
            let zero_i8 = builder.ins().iconst(types::I8, 0);
            builder.def_var(key_owned_var, zero_i8);

            // Insert (key, value) into the map
            let insert_fn_addr = builder
                .ins()
                .iconst(pointer_type, insert_fn as usize as i64);
            let sig_map_insert = {
                let mut s = module.make_signature();
                s.params.push(AbiParam::new(pointer_type)); // map_ptr.ptr
                s.params.push(AbiParam::new(pointer_type)); // map_ptr.metadata
                s.params.push(AbiParam::new(pointer_type)); // key_ptr.ptr
                s.params.push(AbiParam::new(pointer_type)); // key_ptr.metadata
                s.params.push(AbiParam::new(pointer_type)); // value_ptr.ptr
                s.params.push(AbiParam::new(pointer_type)); // value_ptr.metadata
                s
            };
            let sig_ref_map_insert = builder.import_signature(sig_map_insert);
            let zero_meta = builder.ins().iconst(pointer_type, 0);
            builder.ins().call_indirect(
                sig_ref_map_insert,
                insert_fn_addr,
                &[
                    map_ptr,
                    zero_meta,
                    key_out_ptr,
                    zero_meta,
                    value_ptr,
                    zero_meta,
                ],
            );

            // Continue to after_value (no error checking for insert, no key cleanup needed)
            builder.ins().jump(after_value, &[]);
            builder.seal_block(value_stored);
            builder.seal_block(after_init);
            builder.seal_block(kv_sep_ok);
        } else {
            // No flattened map - skip the value (original behavior)
            let err_code = format.emit_skip_value(module, &mut builder, &mut cursor);
            builder.def_var(err_var, err_code);

            // Check if owned key needs cleanup
            let key_owned = builder.use_var(key_owned_var);
            let needs_drop = builder.ins().icmp_imm(IntCC::NotEqual, key_owned, 0);
            let drop_key = builder.create_block();
            let after_drop = builder.create_block();
            builder
                .ins()
                .brif(needs_drop, drop_key, &[], after_drop, &[]);

            // drop_key: call jit_drop_owned_string
            builder.switch_to_block(drop_key);
            let key_ptr = builder.use_var(key_ptr_var);
            let key_len = builder.use_var(key_len_var);
            let key_cap = builder.use_var(key_cap_var);

            // Declare jit_drop_owned_string helper
            let sig_drop = {
                let mut s = module.make_signature();
                s.params.push(AbiParam::new(pointer_type)); // ptr
                s.params.push(AbiParam::new(pointer_type)); // len
                s.params.push(AbiParam::new(pointer_type)); // cap
                s
            };
            let drop_id = match module.declare_function(
                "jit_drop_owned_string",
                Linkage::Import,
                &sig_drop,
            ) {
                Ok(id) => id,
                Err(e) => {
                    jit_debug!("[compile_struct] declare jit_drop_owned_string failed");
                    jit_diag!("declare_function('jit_drop_owned_string') failed: {:?}", e);
                    return None;
                }
            };
            let drop_ref = module.declare_func_in_func(drop_id, builder.func);
            builder.ins().call(drop_ref, &[key_ptr, key_len, key_cap]);
            builder.ins().jump(after_drop, &[]);
            builder.seal_block(drop_key);

            // after_drop: check skip_value error and continue
            builder.switch_to_block(after_drop);
            let skip_err = builder.use_var(err_var);
            let is_ok = builder.ins().icmp_imm(IntCC::Equal, skip_err, 0);
            builder.ins().brif(is_ok, after_value, &[], error, &[]);
            builder.seal_block(kv_sep_ok);
            builder.seal_block(after_drop);
        }
        // Note: unknown_key is already sealed by both dispatch strategies:
        // - Linear: sealed as current_block on the last field iteration
        // - PrefixSwitch: sealed after all disambiguation blocks are generated
        // Only seal if we have a single dispatch entry (special case)
        if dispatch_entries.len() == 1 {
            builder.seal_block(unknown_key);
        }

        // Implement match blocks for each dispatch entry (field or variant)
        for (i, (_key_name, target)) in dispatch_entries.iter().enumerate() {
            builder.switch_to_block(match_blocks[i]);

            match target {
                DispatchTarget::Field(field_idx) => {
                    // Normal field parsing (existing logic)
                    let field_info = &field_infos[*field_idx];

                    jit_diag!(
                        "Processing field {}: '{}' type {:?}",
                        i,
                        field_info.name,
                        field_info.shape.def
                    );

                    // First, consume the kv separator (':' in JSON)
                    let mut cursor = JitCursor {
                        input_ptr,
                        len,
                        pos: pos_var,
                        ptr_type: pointer_type,
                    };

                    let format = F::default();
                    let err_code =
                        format.emit_map_kv_sep(module, &mut builder, &mut cursor, state_ptr);
                    builder.def_var(err_var, err_code);

                    // Check for error
                    let kv_sep_ok = builder.create_block();
                    let is_ok = builder.ins().icmp_imm(IntCC::Equal, err_code, 0);
                    builder.ins().brif(is_ok, kv_sep_ok, &[], error, &[]);

                    builder.switch_to_block(kv_sep_ok);

                    // Now parse the field value based on its type
                    let field_shape = field_info.shape;
                    let field_ptr = builder.ins().iadd_imm(out_ptr, field_info.offset as i64);

                    // Duplicate cleanup: drop old value if this required field was already seen
                    // This prevents memory leaks when duplicate keys appear in JSON for owned types
                    // (String, Vec, HashMap, enum payloads)
                    if let Some(bit_index) = field_info.required_bit_index {
                        let bits = builder.use_var(required_bits_var);
                        let mask = builder.ins().iconst(types::I64, 1i64 << bit_index);
                        let already_set_bits = builder.ins().band(bits, mask);
                        let already_set =
                            builder.ins().icmp_imm(IntCC::NotEqual, already_set_bits, 0);

                        let drop_old = builder.create_block();
                        let after_drop = builder.create_block();
                        builder
                            .ins()
                            .brif(already_set, drop_old, &[], after_drop, &[]);

                        // drop_old: call jit_drop_in_place to drop the previous value
                        builder.switch_to_block(drop_old);
                        let field_shape_ptr = builder
                            .ins()
                            .iconst(pointer_type, field_shape as *const Shape as usize as i64);

                        // Declare jit_drop_in_place helper
                        let sig_drop = {
                            let mut s = module.make_signature();
                            s.params.push(AbiParam::new(pointer_type)); // shape_ptr
                            s.params.push(AbiParam::new(pointer_type)); // ptr
                            s
                        };
                        let drop_id = match module.declare_function(
                            "jit_drop_in_place",
                            Linkage::Import,
                            &sig_drop,
                        ) {
                            Ok(id) => id,
                            Err(e) => {
                                jit_debug!("[compile_struct] declare jit_drop_in_place failed");
                                jit_diag!("declare_function('jit_drop_in_place') failed: {:?}", e);
                                return None;
                            }
                        };
                        let drop_ref = module.declare_func_in_func(drop_id, builder.func);
                        builder.ins().call(drop_ref, &[field_shape_ptr, field_ptr]);
                        builder.ins().jump(after_drop, &[]);
                        builder.seal_block(drop_old);

                        // after_drop: proceed with parsing the new value
                        builder.switch_to_block(after_drop);
                        builder.seal_block(after_drop);
                    }

                    // For MVP: only support scalar types
                    // Vec and nested structs will be added later
                    use facet_core::ScalarType;
                    jit_debug!(
                        "[compile_struct]   Parsing field '{}', scalar_type = {:?}",
                        field_info.name,
                        field_shape.scalar_type()
                    );
                    if let Some(scalar_type) = field_shape.scalar_type() {
                        // Parse scalar value
                        let mut cursor = JitCursor {
                            input_ptr,
                            len,
                            pos: pos_var,
                            ptr_type: pointer_type,
                        };

                        let format = F::default();

                        // Create a shared continuation block for all scalar parsing paths
                        let parse_and_store_done = builder.create_block();

                        match scalar_type {
                            ScalarType::Bool => {
                                let (value, err) =
                                    format.emit_parse_bool(module, &mut builder, &mut cursor);
                                builder.def_var(err_var, err);
                                let is_ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);

                                // Create dedicated block for storing this type
                                let bool_store = builder.create_block();
                                builder.ins().brif(is_ok, bool_store, &[], error, &[]);

                                builder.switch_to_block(bool_store);
                                builder
                                    .ins()
                                    .store(MemFlags::trusted(), value, field_ptr, 0);
                                builder.ins().jump(parse_and_store_done, &[]);
                                builder.seal_block(bool_store);
                            }
                            ScalarType::I8
                            | ScalarType::I16
                            | ScalarType::I32
                            | ScalarType::I64 => {
                                let (value_i64, err) =
                                    format.emit_parse_i64(module, &mut builder, &mut cursor);
                                builder.def_var(err_var, err);
                                let is_ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);

                                let int_store = builder.create_block();
                                builder.ins().brif(is_ok, int_store, &[], error, &[]);

                                builder.switch_to_block(int_store);
                                let value = match scalar_type {
                                    ScalarType::I8 => builder.ins().ireduce(types::I8, value_i64),
                                    ScalarType::I16 => builder.ins().ireduce(types::I16, value_i64),
                                    ScalarType::I32 => builder.ins().ireduce(types::I32, value_i64),
                                    ScalarType::I64 => value_i64,
                                    _ => unreachable!(),
                                };
                                builder
                                    .ins()
                                    .store(MemFlags::trusted(), value, field_ptr, 0);
                                builder.ins().jump(parse_and_store_done, &[]);
                                builder.seal_block(int_store);
                            }
                            ScalarType::U8
                            | ScalarType::U16
                            | ScalarType::U32
                            | ScalarType::U64 => {
                                let (value_u64, err) =
                                    format.emit_parse_u64(module, &mut builder, &mut cursor);
                                builder.def_var(err_var, err);
                                let is_ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);

                                let uint_store = builder.create_block();
                                builder.ins().brif(is_ok, uint_store, &[], error, &[]);

                                builder.switch_to_block(uint_store);
                                let value = match scalar_type {
                                    ScalarType::U8 => builder.ins().ireduce(types::I8, value_u64),
                                    ScalarType::U16 => builder.ins().ireduce(types::I16, value_u64),
                                    ScalarType::U32 => builder.ins().ireduce(types::I32, value_u64),
                                    ScalarType::U64 => value_u64,
                                    _ => unreachable!(),
                                };
                                builder
                                    .ins()
                                    .store(MemFlags::trusted(), value, field_ptr, 0);
                                builder.ins().jump(parse_and_store_done, &[]);
                                builder.seal_block(uint_store);
                            }
                            ScalarType::F32 | ScalarType::F64 => {
                                let (value_f64, err) =
                                    format.emit_parse_f64(module, &mut builder, &mut cursor);
                                builder.def_var(err_var, err);
                                let is_ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);

                                let float_store = builder.create_block();
                                builder.ins().brif(is_ok, float_store, &[], error, &[]);

                                builder.switch_to_block(float_store);
                                let value = if matches!(scalar_type, ScalarType::F32) {
                                    builder.ins().fdemote(types::F32, value_f64)
                                } else {
                                    value_f64
                                };
                                builder
                                    .ins()
                                    .store(MemFlags::trusted(), value, field_ptr, 0);
                                builder.ins().jump(parse_and_store_done, &[]);
                                builder.seal_block(float_store);
                            }
                            ScalarType::String => {
                                let (string_val, err) =
                                    format.emit_parse_string(module, &mut builder, &mut cursor);
                                builder.def_var(err_var, err);
                                let is_ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);

                                let string_store = builder.create_block();
                                builder.ins().brif(is_ok, string_store, &[], error, &[]);

                                builder.switch_to_block(string_store);

                                // Write String to field using jit_write_string helper
                                let sig_write_string = {
                                    let mut s = module.make_signature();
                                    s.params.push(AbiParam::new(pointer_type)); // out_ptr
                                    s.params.push(AbiParam::new(pointer_type)); // offset
                                    s.params.push(AbiParam::new(pointer_type)); // str_ptr
                                    s.params.push(AbiParam::new(pointer_type)); // str_len
                                    s.params.push(AbiParam::new(pointer_type)); // str_cap
                                    s.params.push(AbiParam::new(types::I8)); // owned
                                    s
                                };
                                let write_string_id = match module.declare_function(
                                    "jit_write_string",
                                    Linkage::Import,
                                    &sig_write_string,
                                ) {
                                    Ok(id) => id,
                                    Err(_e) => {
                                        jit_debug!(
                                            "[compile_struct] declare jit_write_string failed"
                                        );
                                        return None;
                                    }
                                };
                                let write_string_ref =
                                    module.declare_func_in_func(write_string_id, builder.func);
                                let field_offset =
                                    builder.ins().iconst(pointer_type, field_info.offset as i64);
                                builder.ins().call(
                                    write_string_ref,
                                    &[
                                        out_ptr,
                                        field_offset,
                                        string_val.ptr,
                                        string_val.len,
                                        string_val.cap,
                                        string_val.owned,
                                    ],
                                );
                                builder.ins().jump(parse_and_store_done, &[]);
                                builder.seal_block(string_store);
                            }
                            _ => {
                                // Unsupported scalar type - fall back to Tier-1
                                jit_debug!(
                                    "[compile_struct] Unsupported scalar type: {:?}",
                                    scalar_type
                                );
                                return None;
                            }
                        }

                        // Now switch to parse_and_store_done for the shared code
                        builder.switch_to_block(parse_and_store_done);

                        // Set required bit if this is a required field
                        if let Some(bit_index) = field_info.required_bit_index {
                            let bits = builder.use_var(required_bits_var);
                            let bit_mask = builder.ins().iconst(types::I64, 1i64 << bit_index);
                            let new_bits = builder.ins().bor(bits, bit_mask);
                            builder.def_var(required_bits_var, new_bits);
                        }

                        // Drop owned key if needed
                        let key_owned = builder.use_var(key_owned_var);
                        let needs_drop = builder.ins().icmp_imm(IntCC::NotEqual, key_owned, 0);
                        let drop_key2 = builder.create_block();
                        let after_drop2 = builder.create_block();
                        builder
                            .ins()
                            .brif(needs_drop, drop_key2, &[], after_drop2, &[]);

                        // Seal parse_and_store_done now that it has a terminator (the brif above)
                        builder.seal_block(parse_and_store_done);

                        builder.switch_to_block(drop_key2);
                        let key_ptr = builder.use_var(key_ptr_var);
                        let key_len = builder.use_var(key_len_var);
                        let key_cap = builder.use_var(key_cap_var);
                        // Reuse drop helper signature from earlier
                        let sig_drop = {
                            let mut s = module.make_signature();
                            s.params.push(AbiParam::new(pointer_type));
                            s.params.push(AbiParam::new(pointer_type));
                            s.params.push(AbiParam::new(pointer_type));
                            s
                        };
                        let drop_id = module
                            .declare_function("jit_drop_owned_string", Linkage::Import, &sig_drop)
                            .ok()?;
                        let drop_ref2 = module.declare_func_in_func(drop_id, builder.func);
                        builder.ins().call(drop_ref2, &[key_ptr, key_len, key_cap]);
                        builder.ins().jump(after_drop2, &[]);
                        builder.seal_block(drop_key2);

                        builder.switch_to_block(after_drop2);
                        builder.ins().jump(after_value, &[]);
                        builder.seal_block(kv_sep_ok);
                        builder.seal_block(after_drop2);
                    } else if matches!(field_shape.def, Def::Option(_)) {
                        // Handle Option<T> fields
                        // Strategy: peek to check if null, then either consume null (None) or parse value (Some)
                        jit_debug!(
                            "[compile_struct]   Parsing Option field '{}'",
                            field_info.name
                        );

                        let mut cursor = JitCursor {
                            input_ptr,
                            len,
                            pos: pos_var,
                            ptr_type: pointer_type,
                        };

                        let format = F::default();

                        // Peek to check if the value is null
                        let (is_null_u8, peek_err) =
                            format.emit_peek_null(&mut builder, &mut cursor);
                        builder.def_var(err_var, peek_err);
                        let peek_ok = builder.ins().icmp_imm(IntCC::Equal, peek_err, 0);

                        let check_null_block = builder.create_block();
                        builder
                            .ins()
                            .brif(peek_ok, check_null_block, &[], error, &[]);

                        builder.switch_to_block(check_null_block);
                        builder.seal_block(check_null_block);
                        let is_null = builder.ins().icmp_imm(IntCC::NotEqual, is_null_u8, 0);

                        let handle_none_block = builder.create_block();
                        let handle_some_block = builder.create_block();
                        builder
                            .ins()
                            .brif(is_null, handle_none_block, &[], handle_some_block, &[]);

                        // Handle None case: consume null, drop old value, and re-init to None
                        // This handles duplicate keys like {"opt":"x","opt":null} -> None (no leak)
                        builder.switch_to_block(handle_none_block);
                        let consume_err = format.emit_consume_null(&mut builder, &mut cursor);
                        builder.def_var(err_var, consume_err);
                        let consume_ok = builder.ins().icmp_imm(IntCC::Equal, consume_err, 0);
                        let none_done = builder.create_block();
                        builder.ins().brif(consume_ok, none_done, &[], error, &[]);

                        builder.switch_to_block(none_done);

                        // Drop the previous value (safe even if it's None)
                        let field_shape_ptr = builder
                            .ins()
                            .iconst(pointer_type, field_shape as *const Shape as usize as i64);
                        let sig_drop = {
                            let mut s = module.make_signature();
                            s.params.push(AbiParam::new(pointer_type)); // shape_ptr
                            s.params.push(AbiParam::new(pointer_type)); // ptr
                            s
                        };
                        let drop_id = match module.declare_function(
                            "jit_drop_in_place",
                            Linkage::Import,
                            &sig_drop,
                        ) {
                            Ok(id) => id,
                            Err(e) => {
                                jit_debug!(
                                    "[compile_struct] declare jit_drop_in_place failed (Option None)"
                                );
                                jit_diag!("declare_function('jit_drop_in_place') failed: {:?}", e);
                                return None;
                            }
                        };
                        let drop_ref = module.declare_func_in_func(drop_id, builder.func);
                        builder.ins().call(drop_ref, &[field_shape_ptr, field_ptr]);

                        // Re-initialize to None (ensures valid None state regardless of previous value)
                        let Def::Option(option_def) = &field_shape.def else {
                            unreachable!();
                        };
                        let init_none_fn_ptr = builder.ins().iconst(
                            pointer_type,
                            option_def.vtable.init_none as *const () as i64,
                        );
                        let sig_option_init_none = {
                            let mut s = module.make_signature();
                            s.params.push(AbiParam::new(pointer_type)); // out_ptr
                            s.params.push(AbiParam::new(pointer_type)); // init_none_fn
                            s
                        };
                        let option_init_none_id = match module.declare_function(
                            "jit_option_init_none",
                            Linkage::Import,
                            &sig_option_init_none,
                        ) {
                            Ok(id) => id,
                            Err(e) => {
                                jit_debug!(
                                    "[compile_struct] declare jit_option_init_none failed (duplicate)"
                                );
                                jit_diag!(
                                    "declare_function('jit_option_init_none') failed: {:?}",
                                    e
                                );
                                return None;
                            }
                        };
                        let option_init_none_ref =
                            module.declare_func_in_func(option_init_none_id, builder.func);
                        builder
                            .ins()
                            .call(option_init_none_ref, &[field_ptr, init_none_fn_ptr]);

                        builder.ins().jump(after_value, &[]);
                        builder.seal_block(handle_none_block);
                        builder.seal_block(none_done);

                        // Handle Some case: parse inner value and init to Some
                        builder.switch_to_block(handle_some_block);
                        builder.seal_block(handle_some_block);

                        // Get the inner type of the Option
                        let Def::Option(option_def) = &field_shape.def else {
                            unreachable!();
                        };
                        let inner_shape = option_def.t;

                        // For now, only support Option<scalar> (not Option<Vec> or Option<struct>)
                        if let Some(inner_scalar_type) = inner_shape.scalar_type() {
                            // Allocate stack slot for inner value (256 bytes is enough for any scalar)
                            let value_slot = builder.create_sized_stack_slot(StackSlotData::new(
                                StackSlotKind::ExplicitSlot,
                                256,
                                8,
                            ));
                            let value_ptr = builder.ins().stack_addr(pointer_type, value_slot, 0);

                            // Create block for calling the init_some helper after parsing
                            let call_init_some = builder.create_block();

                            // Parse inner scalar value based on type
                            match inner_scalar_type {
                                ScalarType::Bool => {
                                    let (value, err) =
                                        format.emit_parse_bool(module, &mut builder, &mut cursor);
                                    builder.def_var(err_var, err);
                                    let is_ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);

                                    let bool_store = builder.create_block();
                                    builder.ins().brif(is_ok, bool_store, &[], error, &[]);

                                    builder.switch_to_block(bool_store);
                                    builder
                                        .ins()
                                        .store(MemFlags::trusted(), value, value_ptr, 0);
                                    builder.ins().jump(call_init_some, &[]);
                                    builder.seal_block(bool_store);
                                }
                                ScalarType::I8
                                | ScalarType::I16
                                | ScalarType::I32
                                | ScalarType::I64 => {
                                    let (value_i64, err) =
                                        format.emit_parse_i64(module, &mut builder, &mut cursor);
                                    builder.def_var(err_var, err);
                                    let is_ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);

                                    let int_store = builder.create_block();
                                    builder.ins().brif(is_ok, int_store, &[], error, &[]);

                                    builder.switch_to_block(int_store);
                                    let value = match inner_scalar_type {
                                        ScalarType::I8 => {
                                            builder.ins().ireduce(types::I8, value_i64)
                                        }
                                        ScalarType::I16 => {
                                            builder.ins().ireduce(types::I16, value_i64)
                                        }
                                        ScalarType::I32 => {
                                            builder.ins().ireduce(types::I32, value_i64)
                                        }
                                        ScalarType::I64 => value_i64,
                                        _ => unreachable!(),
                                    };
                                    builder
                                        .ins()
                                        .store(MemFlags::trusted(), value, value_ptr, 0);
                                    builder.ins().jump(call_init_some, &[]);
                                    builder.seal_block(int_store);
                                }
                                ScalarType::U8
                                | ScalarType::U16
                                | ScalarType::U32
                                | ScalarType::U64 => {
                                    let (value_u64, err) =
                                        format.emit_parse_u64(module, &mut builder, &mut cursor);
                                    builder.def_var(err_var, err);
                                    let is_ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);

                                    let uint_store = builder.create_block();
                                    builder.ins().brif(is_ok, uint_store, &[], error, &[]);

                                    builder.switch_to_block(uint_store);
                                    let value = match inner_scalar_type {
                                        ScalarType::U8 => {
                                            builder.ins().ireduce(types::I8, value_u64)
                                        }
                                        ScalarType::U16 => {
                                            builder.ins().ireduce(types::I16, value_u64)
                                        }
                                        ScalarType::U32 => {
                                            builder.ins().ireduce(types::I32, value_u64)
                                        }
                                        ScalarType::U64 => value_u64,
                                        _ => unreachable!(),
                                    };
                                    builder
                                        .ins()
                                        .store(MemFlags::trusted(), value, value_ptr, 0);
                                    builder.ins().jump(call_init_some, &[]);
                                    builder.seal_block(uint_store);
                                }
                                ScalarType::F32 | ScalarType::F64 => {
                                    let (value_f64, err) =
                                        format.emit_parse_f64(module, &mut builder, &mut cursor);
                                    builder.def_var(err_var, err);
                                    let is_ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);

                                    let float_store = builder.create_block();
                                    builder.ins().brif(is_ok, float_store, &[], error, &[]);

                                    builder.switch_to_block(float_store);
                                    let value = if matches!(inner_scalar_type, ScalarType::F32) {
                                        builder.ins().fdemote(types::F32, value_f64)
                                    } else {
                                        value_f64
                                    };
                                    builder
                                        .ins()
                                        .store(MemFlags::trusted(), value, value_ptr, 0);
                                    builder.ins().jump(call_init_some, &[]);
                                    builder.seal_block(float_store);
                                }
                                ScalarType::String => {
                                    // Parse String then materialize it into a temporary stack slot, then
                                    // call init_some which will move it into the Option.
                                    let (string_val, err) =
                                        format.emit_parse_string(module, &mut builder, &mut cursor);
                                    builder.def_var(err_var, err);
                                    let is_ok = builder.ins().icmp_imm(IntCC::Equal, err, 0);

                                    let string_store = builder.create_block();
                                    builder.ins().brif(is_ok, string_store, &[], error, &[]);

                                    builder.switch_to_block(string_store);

                                    // Declare jit_write_string helper
                                    let sig_write_string = {
                                        let mut s = module.make_signature();
                                        s.params.push(AbiParam::new(pointer_type)); // out_ptr
                                        s.params.push(AbiParam::new(pointer_type)); // offset
                                        s.params.push(AbiParam::new(pointer_type)); // str_ptr
                                        s.params.push(AbiParam::new(pointer_type)); // str_len
                                        s.params.push(AbiParam::new(pointer_type)); // str_cap
                                        s.params.push(AbiParam::new(types::I8)); // owned
                                        s
                                    };
                                    let write_string_id = match module.declare_function(
                                        "jit_write_string",
                                        Linkage::Import,
                                        &sig_write_string,
                                    ) {
                                        Ok(id) => id,
                                        Err(_e) => {
                                            jit_debug!(
                                                "[compile_struct] declare jit_write_string failed"
                                            );
                                            return None;
                                        }
                                    };
                                    let write_string_ref =
                                        module.declare_func_in_func(write_string_id, builder.func);
                                    let zero_offset = builder.ins().iconst(pointer_type, 0);
                                    builder.ins().call(
                                        write_string_ref,
                                        &[
                                            value_ptr,
                                            zero_offset,
                                            string_val.ptr,
                                            string_val.len,
                                            string_val.cap,
                                            string_val.owned,
                                        ],
                                    );

                                    builder.ins().jump(call_init_some, &[]);
                                    builder.seal_block(string_store);
                                }
                                _ => {
                                    jit_debug!(
                                        "[compile_struct] Unsupported Option<scalar> type: {:?}",
                                        inner_scalar_type
                                    );
                                    return None;
                                }
                            }

                            // After storing the value, call jit_option_init_some_from_value
                            // This helper takes (field_ptr, value_ptr, init_some_fn)
                            builder.switch_to_block(call_init_some);
                            builder.seal_block(call_init_some);

                            // Drop the previous value before overwriting with new Some
                            // This handles duplicate keys like {"opt":"x","opt":"y"} -> Some("y") (no leak)
                            // Use the Option shape pointer (field_shape), not the inner shape
                            let field_shape_ptr = builder
                                .ins()
                                .iconst(pointer_type, field_shape as *const Shape as usize as i64);

                            // Declare jit_drop_in_place helper
                            let sig_drop = {
                                let mut s = module.make_signature();
                                s.params.push(AbiParam::new(pointer_type)); // shape_ptr
                                s.params.push(AbiParam::new(pointer_type)); // ptr
                                s
                            };
                            let drop_id = match module.declare_function(
                                "jit_drop_in_place",
                                Linkage::Import,
                                &sig_drop,
                            ) {
                                Ok(id) => id,
                                Err(e) => {
                                    jit_debug!(
                                        "[compile_struct] declare jit_drop_in_place failed (Option Some)"
                                    );
                                    jit_diag!(
                                        "declare_function('jit_drop_in_place') failed: {:?}",
                                        e
                                    );
                                    return None;
                                }
                            };
                            let drop_ref = module.declare_func_in_func(drop_id, builder.func);
                            builder.ins().call(drop_ref, &[field_shape_ptr, field_ptr]);

                            let init_some_fn_ptr = option_def.vtable.init_some as *const u8;
                            let init_some_fn_val =
                                builder.ins().iconst(pointer_type, init_some_fn_ptr as i64);

                            let sig_option_init = {
                                let mut s = module.make_signature();
                                s.params.push(AbiParam::new(pointer_type)); // field_ptr
                                s.params.push(AbiParam::new(pointer_type)); // value_ptr
                                s.params.push(AbiParam::new(pointer_type)); // init_some_fn
                                s
                            };
                            let option_init_id = match module.declare_function(
                                "jit_option_init_some_from_value",
                                Linkage::Import,
                                &sig_option_init,
                            ) {
                                Ok(id) => id,
                                Err(_e) => {
                                    jit_debug!(
                                        "[compile_struct] declare jit_option_init_some_from_value failed"
                                    );
                                    return None;
                                }
                            };
                            let option_init_ref =
                                module.declare_func_in_func(option_init_id, builder.func);

                            builder
                                .ins()
                                .call(option_init_ref, &[field_ptr, value_ptr, init_some_fn_val]);
                            builder.ins().jump(after_value, &[]);
                        } else {
                            jit_debug!(
                                "[compile_struct] Option<non-scalar> not supported for field '{}'",
                                field_info.name
                            );
                            return None;
                        }
                        // Seal kv_sep_ok block (similar to scalar handling at line 2277)
                        builder.seal_block(kv_sep_ok);
                    } else if matches!(field_shape.ty, Type::User(UserType::Struct(_))) {
                        // Handle nested struct fields
                        jit_debug!(
                            "[compile_struct]   Parsing nested struct field '{}'",
                            field_info.name
                        );

                        // Recursively compile the nested struct deserializer
                        let nested_func_id =
                            compile_struct_format_deserializer::<F>(module, field_shape, memo)?;
                        let nested_func_ref =
                            module.declare_func_in_func(nested_func_id, builder.func);

                        // Get field pointer (out_ptr + field offset)
                        let field_ptr = builder.ins().iadd_imm(out_ptr, field_info.offset as i64);

                        // Read current pos
                        let current_pos = builder.use_var(pos_var);

                        // Call nested struct deserializer: (input_ptr, len, pos, field_ptr, scratch_ptr)
                        let call_result = builder.ins().call(
                            nested_func_ref,
                            &[input_ptr, len, current_pos, field_ptr, scratch_ptr],
                        );
                        let new_pos = builder.inst_results(call_result)[0];

                        // Check for error (new_pos < 0 means error)
                        let is_error = builder.ins().icmp_imm(IntCC::SignedLessThan, new_pos, 0);

                        let nested_ok = builder.create_block();
                        builder.ins().brif(is_error, error, &[], nested_ok, &[]);

                        // On success: update pos_var and continue
                        builder.switch_to_block(nested_ok);
                        builder.def_var(pos_var, new_pos);

                        // Set required bit if this is a required field
                        if let Some(bit_index) = field_info.required_bit_index {
                            let bits = builder.use_var(required_bits_var);
                            let bit_mask = builder.ins().iconst(types::I64, 1i64 << bit_index);
                            let new_bits = builder.ins().bor(bits, bit_mask);
                            builder.def_var(required_bits_var, new_bits);
                        }

                        builder.ins().jump(after_value, &[]);
                        builder.seal_block(nested_ok);
                        builder.seal_block(kv_sep_ok);
                    } else if let Def::List(_list_def) = &field_shape.def {
                        // Handle Vec<T> fields
                        jit_debug!("[compile_struct]   Parsing Vec field '{}'", field_info.name);

                        // Recursively compile the list deserializer for this Vec shape
                        let list_func_id =
                            compile_list_format_deserializer::<F>(module, field_shape, memo)?;
                        let list_func_ref = module.declare_func_in_func(list_func_id, builder.func);

                        // Get field pointer (out_ptr + field offset)
                        let field_ptr = builder.ins().iadd_imm(out_ptr, field_info.offset as i64);

                        // Read current pos
                        let current_pos = builder.use_var(pos_var);

                        // Call list deserializer: (input_ptr, len, pos, field_ptr, scratch_ptr)
                        let call_result = builder.ins().call(
                            list_func_ref,
                            &[input_ptr, len, current_pos, field_ptr, scratch_ptr],
                        );
                        let new_pos = builder.inst_results(call_result)[0];

                        // Check for error (new_pos < 0 means error)
                        // IMPORTANT: Don't jump to `error` block - that would overwrite scratch!
                        // The nested list deserializer already wrote error details to scratch.
                        // We need an "error passthrough" that just returns -1.
                        let is_error = builder.ins().icmp_imm(IntCC::SignedLessThan, new_pos, 0);

                        let list_ok = builder.create_block();
                        let list_error_passthrough = builder.create_block();
                        builder
                            .ins()
                            .brif(is_error, list_error_passthrough, &[], list_ok, &[]);

                        // Error passthrough: nested call failed, scratch already written, just return -1
                        builder.switch_to_block(list_error_passthrough);
                        let minus_one = builder.ins().iconst(pointer_type, -1);
                        builder.ins().return_(&[minus_one]);
                        builder.seal_block(list_error_passthrough);

                        // On success: update pos_var and continue
                        builder.switch_to_block(list_ok);
                        builder.def_var(pos_var, new_pos);

                        // Set required bit if this is a required field
                        if let Some(bit_index) = field_info.required_bit_index {
                            let bits = builder.use_var(required_bits_var);
                            let bit_mask = builder.ins().iconst(types::I64, 1i64 << bit_index);
                            let new_bits = builder.ins().bor(bits, bit_mask);
                            builder.def_var(required_bits_var, new_bits);
                        }

                        builder.ins().jump(after_value, &[]);
                        builder.seal_block(list_ok);
                        builder.seal_block(kv_sep_ok);
                    } else if let Def::Map(_map_def) = &field_shape.def {
                        // Handle HashMap<String, V> fields
                        jit_debug!(
                            "[compile_struct]   Parsing HashMap field '{}'",
                            field_info.name
                        );

                        // Recursively compile the map deserializer for this HashMap shape
                        jit_diag!("Compiling map deserializer for field '{}'", field_info.name);
                        let map_func_id =
                            match compile_map_format_deserializer::<F>(module, field_shape, memo) {
                                Some(id) => id,
                                None => {
                                    jit_diag!(
                                        "compile_map_format_deserializer failed for field '{}'",
                                        field_info.name
                                    );
                                    return None;
                                }
                            };
                        let map_func_ref = module.declare_func_in_func(map_func_id, builder.func);

                        // Get field pointer (out_ptr + field offset)
                        let field_ptr = builder.ins().iadd_imm(out_ptr, field_info.offset as i64);

                        // Read current pos
                        let current_pos = builder.use_var(pos_var);

                        // Call map deserializer: (input_ptr, len, pos, field_ptr, scratch_ptr)
                        let call_result = builder.ins().call(
                            map_func_ref,
                            &[input_ptr, len, current_pos, field_ptr, scratch_ptr],
                        );
                        let new_pos = builder.inst_results(call_result)[0];

                        // Check for error (new_pos < 0 means error)
                        // Use error passthrough pattern like Vec fields
                        let is_error = builder.ins().icmp_imm(IntCC::SignedLessThan, new_pos, 0);

                        let map_ok = builder.create_block();
                        let map_error_passthrough = builder.create_block();
                        builder
                            .ins()
                            .brif(is_error, map_error_passthrough, &[], map_ok, &[]);

                        // Error passthrough: nested call failed, scratch already written, just return -1
                        builder.switch_to_block(map_error_passthrough);
                        let minus_one = builder.ins().iconst(pointer_type, -1);
                        builder.ins().return_(&[minus_one]);
                        builder.seal_block(map_error_passthrough);

                        // On success: update pos_var and continue
                        builder.switch_to_block(map_ok);
                        builder.def_var(pos_var, new_pos);

                        // Set required bit if this is a required field
                        if let Some(bit_index) = field_info.required_bit_index {
                            let bits = builder.use_var(required_bits_var);
                            let bit_mask = builder.ins().iconst(types::I64, 1i64 << bit_index);
                            let new_bits = builder.ins().bor(bits, bit_mask);
                            builder.def_var(required_bits_var, new_bits);
                        }

                        builder.ins().jump(after_value, &[]);
                        builder.seal_block(map_ok);
                        builder.seal_block(kv_sep_ok);
                    } else if let Type::User(UserType::Enum(enum_def)) = &field_shape.ty {
                        // Handle standalone (non-flattened) enum fields
                        // JSON shape: {"VariantName": {...payload...}}
                        jit_debug!(
                            "[compile_struct]   Parsing enum field '{}' ({} variants)",
                            field_info.name,
                            enum_def.variants.len()
                        );

                        let mut cursor = JitCursor {
                            input_ptr,
                            len,
                            pos: pos_var,
                            ptr_type: pointer_type,
                        };

                        let format = F::default();

                        // Allocate stack slot for map state if needed (for the enum wrapper object)
                        let enum_state_ptr = if F::MAP_STATE_SIZE > 0 {
                            let align_shift = F::MAP_STATE_ALIGN.trailing_zeros() as u8;
                            let slot = builder.create_sized_stack_slot(StackSlotData::new(
                                StackSlotKind::ExplicitSlot,
                                F::MAP_STATE_SIZE,
                                align_shift,
                            ));
                            builder.ins().stack_addr(pointer_type, slot, 0)
                        } else {
                            builder.ins().iconst(pointer_type, 0)
                        };

                        // 1. emit_map_begin for the enum wrapper object
                        let err_code = format.emit_map_begin(
                            module,
                            &mut builder,
                            &mut cursor,
                            enum_state_ptr,
                        );
                        builder.def_var(err_var, err_code);

                        let map_begin_ok = builder.create_block();
                        let is_ok = builder.ins().icmp_imm(IntCC::Equal, err_code, 0);
                        builder.ins().brif(is_ok, map_begin_ok, &[], error, &[]);

                        builder.switch_to_block(map_begin_ok);

                        // 2. emit_map_is_end to reject empty enum objects
                        let (is_end, err_code) = format.emit_map_is_end(
                            module,
                            &mut builder,
                            &mut cursor,
                            enum_state_ptr,
                        );
                        builder.def_var(err_var, err_code);

                        let check_is_end_err = builder.create_block();
                        let is_ok = builder.ins().icmp_imm(IntCC::Equal, err_code, 0);
                        builder.ins().brif(is_ok, check_is_end_err, &[], error, &[]);

                        builder.switch_to_block(check_is_end_err);
                        let is_empty = builder.ins().icmp_imm(IntCC::NotEqual, is_end, 0);

                        let enum_not_empty = builder.create_block();
                        let empty_enum_error = builder.create_block();
                        builder
                            .ins()
                            .brif(is_empty, empty_enum_error, &[], enum_not_empty, &[]);

                        // Empty enum object error
                        builder.switch_to_block(empty_enum_error);
                        let error_msg = format!(
                            "empty enum object for field '{}' - expected exactly one variant key",
                            field_info.name
                        );
                        let error_msg_ptr = error_msg.as_ptr();
                        let error_msg_len = error_msg.len();
                        std::mem::forget(error_msg);

                        let msg_ptr_const =
                            builder.ins().iconst(pointer_type, error_msg_ptr as i64);
                        let msg_len_const =
                            builder.ins().iconst(pointer_type, error_msg_len as i64);

                        let sig_write_error = {
                            let mut s = module.make_signature();
                            s.params.push(AbiParam::new(pointer_type));
                            s.params.push(AbiParam::new(pointer_type));
                            s.params.push(AbiParam::new(pointer_type));
                            s
                        };
                        let write_error_id = match module.declare_function(
                            "jit_write_error_string",
                            Linkage::Import,
                            &sig_write_error,
                        ) {
                            Ok(id) => id,
                            Err(_e) => {
                                jit_debug!(
                                    "[compile_struct] declare jit_write_error_string failed"
                                );
                                return None;
                            }
                        };
                        let write_error_ref =
                            module.declare_func_in_func(write_error_id, builder.func);
                        builder.ins().call(
                            write_error_ref,
                            &[scratch_ptr, msg_ptr_const, msg_len_const],
                        );

                        let minus_one = builder.ins().iconst(pointer_type, -1);
                        builder.ins().return_(&[minus_one]);
                        builder.seal_block(empty_enum_error);

                        // Continue parsing enum
                        builder.switch_to_block(enum_not_empty);

                        // 3. emit_map_read_key to get variant name
                        let (variant_key, err_code) = format.emit_map_read_key(
                            module,
                            &mut builder,
                            &mut cursor,
                            enum_state_ptr,
                        );
                        builder.def_var(err_var, err_code);

                        // Store variant key components in variables for later cleanup
                        let variant_key_ptr_var = builder.declare_var(pointer_type);
                        let variant_key_len_var = builder.declare_var(pointer_type);
                        let variant_key_cap_var = builder.declare_var(pointer_type);
                        let variant_key_owned_var = builder.declare_var(types::I8);

                        builder.def_var(variant_key_ptr_var, variant_key.ptr);
                        builder.def_var(variant_key_len_var, variant_key.len);
                        builder.def_var(variant_key_cap_var, variant_key.cap);
                        builder.def_var(variant_key_owned_var, variant_key.owned);

                        let read_key_ok = builder.create_block();
                        let is_ok = builder.ins().icmp_imm(IntCC::Equal, err_code, 0);
                        builder.ins().brif(is_ok, read_key_ok, &[], error, &[]);

                        builder.switch_to_block(read_key_ok);

                        // 4. Dispatch variant name
                        // Create match blocks for each variant, plus unknown variant block
                        let mut variant_match_blocks = Vec::new();
                        for _ in enum_def.variants {
                            variant_match_blocks.push(builder.create_block());
                        }
                        let unknown_variant_block = builder.create_block();

                        // Linear dispatch for variants (enum variants typically < 10)
                        let variant_dispatch = builder.create_block();
                        builder.ins().jump(variant_dispatch, &[]);

                        builder.switch_to_block(variant_dispatch);
                        let mut current_block = variant_dispatch;

                        for (variant_idx, variant) in enum_def.variants.iter().enumerate() {
                            if variant_idx > 0 {
                                builder.switch_to_block(current_block);
                            }

                            let variant_name = variant.name;
                            let variant_name_len = variant_name.len();

                            // Check length first
                            let len_matches = builder.ins().icmp_imm(
                                IntCC::Equal,
                                variant_key.len,
                                variant_name_len as i64,
                            );

                            let check_content = builder.create_block();
                            let next_check = if variant_idx + 1 < enum_def.variants.len() {
                                builder.create_block()
                            } else {
                                unknown_variant_block
                            };

                            builder
                                .ins()
                                .brif(len_matches, check_content, &[], next_check, &[]);
                            if variant_idx > 0 {
                                builder.seal_block(current_block);
                            }

                            // Byte-by-byte comparison
                            builder.switch_to_block(check_content);
                            let mut all_match = builder.ins().iconst(types::I8, 1);

                            for (byte_idx, &byte) in variant_name.as_bytes().iter().enumerate() {
                                let offset = builder.ins().iconst(pointer_type, byte_idx as i64);
                                let char_ptr = builder.ins().iadd(variant_key.ptr, offset);
                                let char_val =
                                    builder
                                        .ins()
                                        .load(types::I8, MemFlags::trusted(), char_ptr, 0);
                                let expected = builder.ins().iconst(types::I8, byte as i64);
                                let byte_matches =
                                    builder.ins().icmp(IntCC::Equal, char_val, expected);
                                let one = builder.ins().iconst(types::I8, 1);
                                let zero = builder.ins().iconst(types::I8, 0);
                                let byte_match_i8 = builder.ins().select(byte_matches, one, zero);
                                all_match = builder.ins().band(all_match, byte_match_i8);
                            }

                            let all_match_bool =
                                builder.ins().icmp_imm(IntCC::NotEqual, all_match, 0);
                            builder.ins().brif(
                                all_match_bool,
                                variant_match_blocks[variant_idx],
                                &[],
                                next_check,
                                &[],
                            );
                            builder.seal_block(check_content);

                            current_block = next_check;
                        }

                        builder.seal_block(variant_dispatch);
                        if enum_def.variants.len() > 1 {
                            builder.seal_block(current_block);
                        }

                        // Handle unknown variant
                        builder.switch_to_block(unknown_variant_block);
                        let error_msg =
                            format!("unknown variant for enum field '{}'", field_info.name);
                        let error_msg_ptr = error_msg.as_ptr();
                        let error_msg_len = error_msg.len();
                        std::mem::forget(error_msg);

                        let msg_ptr_const =
                            builder.ins().iconst(pointer_type, error_msg_ptr as i64);
                        let msg_len_const =
                            builder.ins().iconst(pointer_type, error_msg_len as i64);

                        let write_error_id = match module.declare_function(
                            "jit_write_error_string",
                            Linkage::Import,
                            &sig_write_error,
                        ) {
                            Ok(id) => id,
                            Err(_e) => {
                                jit_debug!(
                                    "[compile_struct] declare jit_write_error_string failed"
                                );
                                return None;
                            }
                        };
                        let write_error_ref =
                            module.declare_func_in_func(write_error_id, builder.func);
                        builder.ins().call(
                            write_error_ref,
                            &[scratch_ptr, msg_ptr_const, msg_len_const],
                        );

                        let minus_one = builder.ins().iconst(pointer_type, -1);
                        builder.ins().return_(&[minus_one]);
                        // Seal unknown_variant_block (for single variant case, multi-variant sealed above)
                        if enum_def.variants.len() == 1 {
                            builder.seal_block(unknown_variant_block);
                        }

                        // 5. Implement variant match blocks
                        // Block to jump to after variant parsing
                        let enum_parsed = builder.create_block();

                        for (variant_idx, variant) in enum_def.variants.iter().enumerate() {
                            builder.switch_to_block(variant_match_blocks[variant_idx]);

                            // Consume kv_sep before payload
                            let mut cursor = JitCursor {
                                input_ptr,
                                len,
                                pos: pos_var,
                                ptr_type: pointer_type,
                            };
                            let err_code = format.emit_map_kv_sep(
                                module,
                                &mut builder,
                                &mut cursor,
                                enum_state_ptr,
                            );
                            builder.def_var(err_var, err_code);

                            let kv_sep_ok_variant = builder.create_block();
                            let is_ok = builder.ins().icmp_imm(IntCC::Equal, err_code, 0);
                            builder
                                .ins()
                                .brif(is_ok, kv_sep_ok_variant, &[], error, &[]);

                            builder.switch_to_block(kv_sep_ok_variant);

                            // Parse payload struct
                            let payload_shape = variant.data.fields[0].shape();
                            let payload_func_id = compile_struct_format_deserializer::<F>(
                                module,
                                payload_shape,
                                memo,
                            )?;
                            let payload_func_ref =
                                module.declare_func_in_func(payload_func_id, builder.func);

                            // Allocate stack slot for payload
                            let payload_layout = payload_shape.layout.sized_layout().ok()?;
                            let payload_slot = builder.create_sized_stack_slot(StackSlotData::new(
                                StackSlotKind::ExplicitSlot,
                                payload_layout.size() as u32,
                                payload_layout.align() as u8,
                            ));
                            let payload_ptr =
                                builder.ins().stack_addr(pointer_type, payload_slot, 0);

                            // Call payload deserializer
                            let current_pos = builder.use_var(pos_var);
                            let call_result = builder.ins().call(
                                payload_func_ref,
                                &[input_ptr, len, current_pos, payload_ptr, scratch_ptr],
                            );
                            let new_pos = builder.inst_results(call_result)[0];

                            // Check for error
                            let payload_ok = builder.create_block();
                            let is_error =
                                builder.ins().icmp_imm(IntCC::SignedLessThan, new_pos, 0);

                            let error_passthrough = builder.create_block();
                            builder
                                .ins()
                                .brif(is_error, error_passthrough, &[], payload_ok, &[]);

                            builder.switch_to_block(error_passthrough);
                            let minus_one = builder.ins().iconst(pointer_type, -1);
                            builder.ins().return_(&[minus_one]);
                            builder.seal_block(error_passthrough);

                            builder.switch_to_block(payload_ok);
                            builder.def_var(pos_var, new_pos);

                            // Get enum field pointer
                            let enum_ptr =
                                builder.ins().iadd_imm(out_ptr, field_info.offset as i64);

                            // Write discriminant
                            let discriminant = variant.discriminant.unwrap_or(0);
                            let discrim_val = builder.ins().iconst(types::I64, discriminant);
                            builder
                                .ins()
                                .store(MemFlags::trusted(), discrim_val, enum_ptr, 0);

                            // Copy payload
                            let payload_offset_in_enum = variant.data.fields[0].offset;
                            let enum_payload_ptr = builder
                                .ins()
                                .iadd_imm(enum_ptr, payload_offset_in_enum as i64);

                            let sig_memcpy = {
                                let mut s = module.make_signature();
                                s.params.push(AbiParam::new(pointer_type));
                                s.params.push(AbiParam::new(pointer_type));
                                s.params.push(AbiParam::new(pointer_type));
                                s
                            };
                            let memcpy_id = match module.declare_function(
                                "jit_memcpy",
                                Linkage::Import,
                                &sig_memcpy,
                            ) {
                                Ok(id) => id,
                                Err(_e) => {
                                    jit_debug!("[compile_struct] declare jit_memcpy failed");
                                    return None;
                                }
                            };
                            let memcpy_ref = module.declare_func_in_func(memcpy_id, builder.func);
                            let payload_size = builder
                                .ins()
                                .iconst(pointer_type, payload_layout.size() as i64);
                            builder
                                .ins()
                                .call(memcpy_ref, &[enum_payload_ptr, payload_ptr, payload_size]);

                            // Jump to enum_parsed to check for end-of-enum-object
                            builder.ins().jump(enum_parsed, &[]);
                            builder.seal_block(kv_sep_ok_variant);
                            builder.seal_block(payload_ok);
                            builder.seal_block(variant_match_blocks[variant_idx]);
                        }

                        // 6. After parsing variant payload, verify end of enum object
                        builder.switch_to_block(enum_parsed);

                        // emit_map_next to check for closing } or extra keys
                        let mut cursor = JitCursor {
                            input_ptr,
                            len,
                            pos: pos_var,
                            ptr_type: pointer_type,
                        };
                        let err_code =
                            format.emit_map_next(module, &mut builder, &mut cursor, enum_state_ptr);
                        builder.def_var(err_var, err_code);

                        let map_next_ok = builder.create_block();
                        let is_ok = builder.ins().icmp_imm(IntCC::Equal, err_code, 0);
                        builder.ins().brif(is_ok, map_next_ok, &[], error, &[]);

                        builder.switch_to_block(map_next_ok);

                        // emit_map_is_end to verify we're at the closing }
                        let (is_end, err_code) = format.emit_map_is_end(
                            module,
                            &mut builder,
                            &mut cursor,
                            enum_state_ptr,
                        );
                        builder.def_var(err_var, err_code);

                        let check_end_ok = builder.create_block();
                        let is_ok = builder.ins().icmp_imm(IntCC::Equal, err_code, 0);
                        builder.ins().brif(is_ok, check_end_ok, &[], error, &[]);

                        builder.switch_to_block(check_end_ok);
                        let at_end = builder.ins().icmp_imm(IntCC::NotEqual, is_end, 0);

                        let enum_complete = builder.create_block();
                        let extra_keys_error = builder.create_block();
                        builder
                            .ins()
                            .brif(at_end, enum_complete, &[], extra_keys_error, &[]);

                        // Extra keys in enum object error
                        builder.switch_to_block(extra_keys_error);
                        let error_msg = format!(
                            "enum field '{}' has extra keys - expected exactly one variant",
                            field_info.name
                        );
                        let error_msg_ptr = error_msg.as_ptr();
                        let error_msg_len = error_msg.len();
                        std::mem::forget(error_msg);

                        let msg_ptr_const =
                            builder.ins().iconst(pointer_type, error_msg_ptr as i64);
                        let msg_len_const =
                            builder.ins().iconst(pointer_type, error_msg_len as i64);

                        let write_error_id = match module.declare_function(
                            "jit_write_error_string",
                            Linkage::Import,
                            &sig_write_error,
                        ) {
                            Ok(id) => id,
                            Err(_e) => {
                                jit_debug!(
                                    "[compile_struct] declare jit_write_error_string failed"
                                );
                                return None;
                            }
                        };
                        let write_error_ref =
                            module.declare_func_in_func(write_error_id, builder.func);
                        builder.ins().call(
                            write_error_ref,
                            &[scratch_ptr, msg_ptr_const, msg_len_const],
                        );

                        let minus_one = builder.ins().iconst(pointer_type, -1);
                        builder.ins().return_(&[minus_one]);
                        builder.seal_block(extra_keys_error);

                        // Enum successfully parsed
                        builder.switch_to_block(enum_complete);

                        // Clean up owned variant key if needed
                        let key_owned = builder.use_var(variant_key_owned_var);
                        let needs_drop = builder.ins().icmp_imm(IntCC::NotEqual, key_owned, 0);
                        let drop_variant_key = builder.create_block();
                        let after_drop_variant = builder.create_block();
                        builder.ins().brif(
                            needs_drop,
                            drop_variant_key,
                            &[],
                            after_drop_variant,
                            &[],
                        );

                        builder.switch_to_block(drop_variant_key);
                        let key_ptr = builder.use_var(variant_key_ptr_var);
                        let key_len = builder.use_var(variant_key_len_var);
                        let key_cap = builder.use_var(variant_key_cap_var);

                        let sig_drop = {
                            let mut s = module.make_signature();
                            s.params.push(AbiParam::new(pointer_type));
                            s.params.push(AbiParam::new(pointer_type));
                            s.params.push(AbiParam::new(pointer_type));
                            s
                        };
                        let drop_id = match module.declare_function(
                            "jit_drop_owned_string",
                            Linkage::Import,
                            &sig_drop,
                        ) {
                            Ok(id) => id,
                            Err(_e) => {
                                jit_debug!("[compile_struct] declare jit_drop_owned_string failed");
                                return None;
                            }
                        };
                        let drop_ref = module.declare_func_in_func(drop_id, builder.func);
                        builder.ins().call(drop_ref, &[key_ptr, key_len, key_cap]);
                        builder.ins().jump(after_drop_variant, &[]);
                        builder.seal_block(drop_variant_key);

                        builder.switch_to_block(after_drop_variant);

                        // Set required bit if this is a required field
                        if let Some(bit_index) = field_info.required_bit_index {
                            let bits = builder.use_var(required_bits_var);
                            let bit_mask = builder.ins().iconst(types::I64, 1i64 << bit_index);
                            let new_bits = builder.ins().bor(bits, bit_mask);
                            builder.def_var(required_bits_var, new_bits);
                        }

                        builder.ins().jump(after_value, &[]);
                        builder.seal_block(map_begin_ok);
                        builder.seal_block(check_is_end_err);
                        builder.seal_block(enum_not_empty);
                        builder.seal_block(read_key_ok);
                        builder.seal_block(enum_parsed);
                        builder.seal_block(map_next_ok);
                        builder.seal_block(check_end_ok);
                        builder.seal_block(enum_complete);
                        builder.seal_block(after_drop_variant);
                        builder.seal_block(kv_sep_ok);
                    } else {
                        // Unsupported field type (Set, etc.)
                        jit_debug!(
                            "[compile_struct] Field {} has unsupported type (not scalar/Option/struct/Vec/Map/Enum)",
                            field_info.name
                        );
                        jit_diag!(
                            "Field '{}' has unsupported type: {:?}",
                            field_info.name,
                            field_info.shape.def
                        );
                        return None;
                    }
                }
                DispatchTarget::FlattenEnumVariant(variant_idx) => {
                    // Flattened enum variant parsing
                    let variant_info = &flatten_variants[*variant_idx];

                    jit_diag!(
                        "Processing flattened variant '{}' for enum at offset {} (seen_bit={})",
                        variant_info.variant_name,
                        variant_info.enum_field_offset,
                        variant_info.enum_seen_bit_index
                    );

                    // 0. Check if this enum has already been set (duplicate variant key error)
                    let enum_bit_mask = builder
                        .ins()
                        .iconst(types::I64, 1i64 << variant_info.enum_seen_bit_index);
                    let current_seen_bits = builder.use_var(enum_seen_bits_var);
                    let already_seen = builder.ins().band(current_seen_bits, enum_bit_mask);
                    let is_duplicate = builder.ins().icmp_imm(IntCC::NotEqual, already_seen, 0);

                    let enum_not_seen = builder.create_block();
                    let duplicate_variant_error = builder.create_block();

                    builder.ins().brif(
                        is_duplicate,
                        duplicate_variant_error,
                        &[],
                        enum_not_seen,
                        &[],
                    );

                    // Duplicate variant key: write error to scratch and return -1
                    builder.switch_to_block(duplicate_variant_error);
                    let error_msg = format!(
                        "duplicate variant key '{}' for enum field",
                        variant_info.variant_name
                    );
                    let error_msg_ptr = error_msg.as_ptr();
                    let error_msg_len = error_msg.len();
                    std::mem::forget(error_msg); // Leak the string so it lives for the lifetime of the JIT code

                    let msg_ptr_const = builder.ins().iconst(pointer_type, error_msg_ptr as i64);
                    let msg_len_const = builder.ins().iconst(pointer_type, error_msg_len as i64);

                    // Call jit_write_error_string to write error to scratch buffer
                    let sig_write_error = {
                        let mut s = module.make_signature();
                        s.params.push(AbiParam::new(pointer_type)); // scratch_ptr
                        s.params.push(AbiParam::new(pointer_type)); // msg_ptr
                        s.params.push(AbiParam::new(pointer_type)); // msg_len
                        s
                    };
                    let write_error_id = match module.declare_function(
                        "jit_write_error_string",
                        Linkage::Import,
                        &sig_write_error,
                    ) {
                        Ok(id) => id,
                        Err(_e) => {
                            jit_debug!("[compile_struct] declare jit_write_error_string failed");
                            return None;
                        }
                    };
                    let write_error_ref = module.declare_func_in_func(write_error_id, builder.func);
                    builder.ins().call(
                        write_error_ref,
                        &[scratch_ptr, msg_ptr_const, msg_len_const],
                    );

                    let minus_one = builder.ins().iconst(pointer_type, -1);
                    builder.ins().return_(&[minus_one]);
                    builder.seal_block(duplicate_variant_error);

                    // Continue normal parsing
                    builder.switch_to_block(enum_not_seen);
                    builder.seal_block(enum_not_seen);

                    // 1. Consume kv_sep
                    let mut cursor = JitCursor {
                        input_ptr,
                        len,
                        pos: pos_var,
                        ptr_type: pointer_type,
                    };

                    let format = F::default();
                    let err_code =
                        format.emit_map_kv_sep(module, &mut builder, &mut cursor, state_ptr);
                    builder.def_var(err_var, err_code);

                    let kv_sep_ok = builder.create_block();
                    let is_ok = builder.ins().icmp_imm(IntCC::Equal, err_code, 0);
                    builder.ins().brif(is_ok, kv_sep_ok, &[], error, &[]);

                    builder.switch_to_block(kv_sep_ok);

                    // 2. Compile nested struct deserializer for payload
                    let payload_func_id = compile_struct_format_deserializer::<F>(
                        module,
                        variant_info.payload_shape,
                        memo,
                    )?;
                    let payload_func_ref =
                        module.declare_func_in_func(payload_func_id, builder.func);

                    // 3. Allocate stack slot for payload struct
                    let payload_layout = variant_info.payload_shape.layout.sized_layout().ok()?;
                    let payload_slot = builder.create_sized_stack_slot(StackSlotData::new(
                        StackSlotKind::ExplicitSlot,
                        payload_layout.size() as u32,
                        payload_layout.align() as u8,
                    ));
                    let payload_ptr = builder.ins().stack_addr(pointer_type, payload_slot, 0);

                    // 4. Call payload deserializer
                    let current_pos = builder.use_var(pos_var);
                    let call_result = builder.ins().call(
                        payload_func_ref,
                        &[input_ptr, len, current_pos, payload_ptr, scratch_ptr],
                    );
                    let new_pos = builder.inst_results(call_result)[0];

                    // 5. Check for error (passthrough pattern)
                    let payload_ok = builder.create_block();
                    let is_error = builder.ins().icmp_imm(IntCC::SignedLessThan, new_pos, 0);

                    let error_passthrough = builder.create_block();
                    builder
                        .ins()
                        .brif(is_error, error_passthrough, &[], payload_ok, &[]);

                    // Error passthrough: nested call failed, scratch already written, just return -1
                    builder.switch_to_block(error_passthrough);
                    let minus_one = builder.ins().iconst(pointer_type, -1);
                    builder.ins().return_(&[minus_one]);
                    builder.seal_block(error_passthrough);

                    builder.switch_to_block(payload_ok);
                    builder.def_var(pos_var, new_pos);

                    // 6. Initialize enum at field offset
                    // For #[repr(C)] enums: discriminant at offset 0, payload after discriminant
                    let enum_ptr = builder
                        .ins()
                        .iadd_imm(out_ptr, variant_info.enum_field_offset as i64);

                    // Write discriminant (use i64 for #[repr(C)] which uses isize by default)
                    let discrim_val = builder
                        .ins()
                        .iconst(types::I64, variant_info.discriminant as i64);
                    builder
                        .ins()
                        .store(MemFlags::trusted(), discrim_val, enum_ptr, 0);

                    // Copy payload from stack to enum using actual payload offset
                    // The offset accounts for discriminant size/alignment per the shape metadata
                    let enum_payload_ptr = builder
                        .ins()
                        .iadd_imm(enum_ptr, variant_info.payload_offset_in_enum as i64);

                    // Use memcpy to copy payload
                    let sig_memcpy = {
                        let mut s = module.make_signature();
                        s.params.push(AbiParam::new(pointer_type)); // dest
                        s.params.push(AbiParam::new(pointer_type)); // src
                        s.params.push(AbiParam::new(pointer_type)); // len
                        s
                    };
                    let memcpy_id =
                        match module.declare_function("jit_memcpy", Linkage::Import, &sig_memcpy) {
                            Ok(id) => id,
                            Err(_e) => {
                                jit_debug!("[compile_struct] declare jit_memcpy failed");
                                return None;
                            }
                        };
                    let memcpy_ref = module.declare_func_in_func(memcpy_id, builder.func);
                    let payload_size = builder
                        .ins()
                        .iconst(pointer_type, payload_layout.size() as i64);
                    builder
                        .ins()
                        .call(memcpy_ref, &[enum_payload_ptr, payload_ptr, payload_size]);

                    // 7. Mark this enum as seen (prevent duplicate variant keys)
                    let current_seen = builder.use_var(enum_seen_bits_var);
                    let new_seen = builder.ins().bor(current_seen, enum_bit_mask);
                    builder.def_var(enum_seen_bits_var, new_seen);

                    // 8. Jump to after_value
                    builder.ins().jump(after_value, &[]);
                    builder.seal_block(kv_sep_ok);
                    builder.seal_block(payload_ok);
                }
            }

            builder.seal_block(match_blocks[i]);
        }

        // after_value: advance to next entry
        builder.switch_to_block(after_value);

        let mut cursor = JitCursor {
            input_ptr,
            len,
            pos: pos_var,
            ptr_type: pointer_type,
        };

        let format = F::default();
        let err_code = format.emit_map_next(module, &mut builder, &mut cursor, state_ptr);
        builder.def_var(err_var, err_code);

        builder.ins().jump(check_map_next_err, &[]);
        builder.seal_block(after_value);

        // check_map_next_err
        builder.switch_to_block(check_map_next_err);
        let is_ok = builder.ins().icmp_imm(IntCC::Equal, err_code, 0);
        builder.ins().brif(is_ok, loop_check_end, &[], error, &[]);
        builder.seal_block(check_map_next_err);

        // Now seal loop_check_end and error (all predecessors known)
        builder.seal_block(loop_check_end);
        builder.seal_block(error);

        builder.finalize();
    }

    // Debug: print the generated IR
    if std::env::var("FACET_JIT_TRACE").is_ok() {
        eprintln!("[compile_struct] Generated Cranelift IR:");
        eprintln!("{}", ctx.func.display());
    }

    if let Err(e) = module.define_function(func_id, &mut ctx) {
        jit_debug!("[compile_struct] define_function failed: {:?}", e);
        jit_diag!("define_function failed: {:?}", e);
        return None;
    }

    jit_debug!("[compile_struct] SUCCESS - function compiled");
    jit_diag!("compile_struct_format_deserializer SUCCESS");
    Some(func_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_jit_compatibility() {
        // Vec<bool> should be supported
        assert!(is_format_jit_compatible(<Vec<bool>>::SHAPE));

        // Vec of integer types should be supported
        assert!(is_format_jit_compatible(<Vec<i8>>::SHAPE));
        assert!(is_format_jit_compatible(<Vec<i16>>::SHAPE));
        assert!(is_format_jit_compatible(<Vec<i32>>::SHAPE));
        assert!(is_format_jit_compatible(<Vec<i64>>::SHAPE));
        assert!(is_format_jit_compatible(<Vec<u8>>::SHAPE));
        assert!(is_format_jit_compatible(<Vec<u16>>::SHAPE));
        assert!(is_format_jit_compatible(<Vec<u32>>::SHAPE));
        assert!(is_format_jit_compatible(<Vec<u64>>::SHAPE));

        // Vec of float types are supported
        assert!(is_format_jit_compatible(<Vec<f32>>::SHAPE));
        assert!(is_format_jit_compatible(<Vec<f64>>::SHAPE));

        // Vec<String> is supported
        assert!(is_format_jit_compatible(<Vec<String>>::SHAPE));

        // Primitive types alone are not supported (need to be in a container)
        assert!(!is_format_jit_compatible(bool::SHAPE));
        assert!(!is_format_jit_compatible(i64::SHAPE));
    }

    /// Compile-time verification that the ABI signature is correct.
    ///
    /// This test documents and verifies the Tier-2 ABI contract:
    /// - Compiled function has the expected `extern "C"` signature
    /// - Takes (input_ptr, len, pos, out, scratch) parameters
    /// - Returns isize (new position on success >= 0, error code on failure < 0)
    ///
    /// For runtime ABI contract tests (error handling, initialization), see:
    /// - `facet-format-json/tests/jit_tier2_tests.rs`
    #[test]
    fn test_abi_signature_compiles() {
        use crate::jit::format::JitScratch;

        // Define the expected ABI signature
        type ExpectedAbi = unsafe extern "C" fn(
            input_ptr: *const u8,
            len: usize,
            pos: usize,
            out: *mut u8,
            scratch: *mut JitScratch,
        ) -> isize;

        // Verify the signature compiles (type-level contract)
        // This ensures the compiled function pointer can be cast to ExpectedAbi
        let _verify_signature = |fn_ptr: *const u8| {
            let _typed_fn: ExpectedAbi = unsafe { std::mem::transmute(fn_ptr) };
        };
    }

    #[test]
    fn test_vec_string_compatibility() {
        let shape = <Vec<String>>::SHAPE;
        let compatible = is_format_jit_compatible(shape);
        eprintln!("Vec<String> is_format_jit_compatible: {}", compatible);
        assert!(compatible, "Vec<String> should be Tier-2 compatible");

        // Also check the element shape
        if let facet_core::Def::List(list_def) = &shape.def {
            let elem_shape = list_def.t;
            eprintln!(
                "  elem_shape.is_type::<String>(): {}",
                elem_shape.is_type::<String>()
            );
            eprintln!("  elem_shape.scalar_type(): {:?}", elem_shape.scalar_type());

            // Check FormatListElementKind::from_shape
            let elem_kind = FormatListElementKind::from_shape(elem_shape);
            eprintln!("  FormatListElementKind::from_shape(): {:?}", elem_kind);
            assert_eq!(
                elem_kind,
                Some(FormatListElementKind::String),
                "Should detect String element type"
            );
        }
    }
}
