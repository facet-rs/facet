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
            if scratch.output_initialized != 0 {
                // SAFETY: The compiled code set output_initialized=1 after calling init,
                // so output contains a valid, initialized value that needs dropping.
                unsafe { output.assume_init_drop() };
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
        // Check for Vec<T> types
        if let Def::List(list_def) = &shape.def {
            return is_format_jit_element_supported(list_def.t);
        }

        // Check for simple struct types
        if let Type::User(UserType::Struct(struct_def)) = &shape.ty {
            return is_format_jit_struct_supported(struct_def);
        }

        false
    }
}

/// Check if a struct type is supported for Tier-2 (simple struct subset).
///
/// Simple struct subset:
/// - Named fields only (StructKind::Struct)
/// - No flatten fields
/// - ≤64 fields (for bitset tracking)
/// - Fields can be: scalars, Option<T>, Vec<T>, or nested simple structs
/// - No custom defaults (only Option pre-initialization)
fn is_format_jit_struct_supported(struct_def: &StructType) -> bool {
    use facet_core::StructKind;

    // Only named structs (not tuples or unit)
    if !matches!(struct_def.kind, StructKind::Struct) {
        return false;
    }

    // Must fit in u64 bitset
    if struct_def.fields.len() > 64 {
        return false;
    }

    // Check all fields are compatible
    for field in struct_def.fields {
        // No flatten support in simple subset
        if field.is_flattened() {
            return false;
        }

        // No custom defaults in simple subset (Option pre-init is OK)
        if field.has_default() {
            return false;
        }

        // Field type must be supported
        if !is_format_jit_field_type_supported(field.shape()) {
            return false;
        }
    }

    true
}

/// Check if a field type is supported for Tier-2.
///
/// Supported types:
/// - Scalars (bool, integers, floats, String)
/// - Option<T> where T is supported
/// - Vec<T> where T is scalar
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
    let shape = T::SHAPE;

    if !is_format_jit_compatible(shape) {
        #[cfg(debug_assertions)]
        jit_debug!("[Tier-2 JIT] Shape not compatible");
        return None;
    }

    // Build the JIT module
    let builder = match JITBuilder::new(cranelift_module::default_libcall_names()) {
        Ok(b) => b,
        Err(_e) => {
            jit_debug!("[Tier-2 JIT] JITBuilder::new failed: {:?}", _e);
            return None;
        }
    };

    let mut builder = builder;

    // Check budget limits before compilation to avoid expensive work on pathological shapes
    let budget = BudgetLimits::from_env();
    if !budget.check_shape(shape) {
        #[cfg(debug_assertions)]
        jit_debug!("[Tier-2 JIT] Shape exceeds budget, refusing compilation");
        return None;
    }

    // Register shared helpers
    register_helpers(&mut builder);

    // Register format-specific helpers
    P::FormatJit::register_helpers(&mut builder);

    let mut module = JITModule::new(builder);

    // Compile based on shape
    let func_id = if let Def::List(_) = &shape.def {
        match compile_list_format_deserializer::<P::FormatJit>(&mut module, shape) {
            Some(id) => id,
            None => {
                #[cfg(debug_assertions)]
                jit_debug!("[Tier-2 JIT] compile_list_format_deserializer returned None");
                return None;
            }
        }
    } else if let Type::User(UserType::Struct(_)) = &shape.ty {
        match compile_struct_format_deserializer::<P::FormatJit>(&mut module, shape) {
            Some(id) => id,
            None => {
                #[cfg(debug_assertions)]
                jit_debug!("[Tier-2 JIT] compile_struct_format_deserializer returned None");
                return None;
            }
        }
    } else {
        #[cfg(debug_assertions)]
        jit_debug!("[Tier-2 JIT] Unsupported shape type");
        return None;
    };

    // Finalize and get the function pointer
    if let Err(e) = module.finalize_definitions() {
        #[cfg(debug_assertions)]
        jit_debug!("[Tier-2 JIT] finalize_definitions failed: {:?}", e);
        let _ = e; // suppress unused warning in release
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

    // Tier-2 specific helpers
    builder.symbol(
        "jit_drop_owned_string",
        helpers::jit_drop_owned_string as *const u8,
    );
    builder.symbol(
        "jit_option_init_none",
        helpers::jit_option_init_none as *const u8,
    );
    builder.symbol("jit_write_string", helpers::jit_write_string as *const u8);
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
}

impl FormatListElementKind {
    fn from_shape(shape: &Shape) -> Option<Self> {
        use facet_core::ScalarType;

        // Check for String first (not a scalar type)
        if shape.is_type::<String>() {
            return Some(Self::String);
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
) -> Option<FuncId> {
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

    // Declare our function
    let func_id = match module.declare_function("jit_format_deserialize_list", Linkage::Local, &sig)
    {
        Ok(id) => id,
        Err(_e) => {
            jit_debug!(
                "[compile_list] declare jit_format_deserialize_list failed: {:?}",
                _e
            );
            return None;
        }
    };

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
        }

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

/// Field codegen information for struct compilation.
#[derive(Debug)]
struct FieldCodegenInfo {
    /// Serialized name to match in the input
    name: &'static str,
    /// Byte offset within the struct
    offset: usize,
    /// Field shape for recursive compilation
    shape: &'static Shape,
    /// Is this field Option<T>?
    is_option: bool,
    /// If not Option and no default, this is required - track with this bit index
    required_bit_index: Option<u8>,
}

/// Compile a Tier-2 struct deserializer.
///
/// Generates IR that uses the map protocol to deserialize struct fields:
/// - map_begin() -> is_end() loop -> read_key() -> match field -> deserialize value -> kv_sep() -> next()
/// - Unknown fields are skipped via emit_skip_value()
/// - Missing optional fields (Option<T>) are pre-initialized to None
/// - Missing required fields cause an error
fn compile_struct_format_deserializer<F: JitFormat>(
    module: &mut JITModule,
    shape: &'static Shape,
) -> Option<FuncId> {
    jit_debug!("[compile_struct] ═══ ENTRY ═══");
    jit_debug!("[compile_struct] Shape type: {:?}", shape.ty);

    let Type::User(UserType::Struct(struct_def)) = &shape.ty else {
        jit_debug!("[compile_struct] ✗ FAIL: Not a struct");
        return None;
    };

    jit_debug!(
        "[compile_struct] Compiling struct with {} fields",
        struct_def.fields.len()
    );

    // Build field metadata
    let mut field_infos = Vec::new();
    let mut required_count = 0u8;

    for field in struct_def.fields {
        // Get serialized name (prefer rename, fall back to name)
        let name = field.rename.unwrap_or(field.name);

        // Get field shape
        let field_shape = field.shape.get();

        jit_debug!(
            "[compile_struct]   Field '{}': shape.def = {:?}",
            name,
            field_shape.def
        );
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

    let pointer_type = module.target_config().pointer_type();

    // Function signature: fn(input_ptr, len, pos, out, scratch) -> isize
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(pointer_type)); // input_ptr
    sig.params.push(AbiParam::new(pointer_type)); // len
    sig.params.push(AbiParam::new(pointer_type)); // pos
    sig.params.push(AbiParam::new(pointer_type)); // out
    sig.params.push(AbiParam::new(pointer_type)); // scratch
    sig.returns.push(AbiParam::new(pointer_type)); // new_pos or error

    let func_id = match module.declare_function("jit_deserialize_struct", Linkage::Export, &sig) {
        Ok(id) => id,
        Err(_e) => {
            jit_debug!("[compile_struct] ✗ FAIL: declare_function failed: {:?}", _e);
            return None;
        }
    };
    jit_debug!("[compile_struct] ✓ Function declared successfully");

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
            Err(_e) => {
                jit_debug!(
                    "[compile_struct] declare jit_option_init_none failed: {:?}",
                    _e
                );
                return None;
            }
        };
        let option_init_none_ref = module.declare_func_in_func(option_init_none_id, builder.func);

        // Pre-initialize all Option<T> fields to None
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

        // For each field, create a match block
        let mut match_blocks = Vec::new();
        for _ in &field_infos {
            match_blocks.push(builder.create_block());
        }

        // Start with the first field
        let key_ptr = builder.use_var(key_ptr_var);
        let key_len = builder.use_var(key_len_var);

        // Note: We're already in key_dispatch from line 1783, so don't switch on first iteration
        let mut current_block = key_dispatch;
        for (i, field_info) in field_infos.iter().enumerate() {
            if i > 0 {
                builder.switch_to_block(current_block);
            }

            let field_name = field_info.name;
            let field_name_len = field_name.len();

            // First check length
            let len_matches = builder
                .ins()
                .icmp_imm(IntCC::Equal, key_len, field_name_len as i64);

            // If length doesn't match, skip to next field
            let check_content = builder.create_block();
            let next_check = if i + 1 < field_infos.len() {
                builder.create_block()
            } else {
                unknown_key
            };

            builder
                .ins()
                .brif(len_matches, check_content, &[], next_check, &[]);
            // Don't seal key_dispatch - it's a loop target from check_read_key_err
            if i > 0 {
                builder.seal_block(current_block);
            }

            // check_content: lengths match, now compare bytes
            builder.switch_to_block(check_content);
            // For simplicity in MVP, do byte-by-byte comparison inline
            // (Could use memcmp helper for longer strings in future)
            let mut all_match = builder.ins().iconst(types::I8, 1); // true

            for (j, &byte) in field_name.as_bytes().iter().enumerate() {
                let offset = builder.ins().iconst(pointer_type, j as i64);
                let char_ptr = builder.ins().iadd(key_ptr, offset);
                let char_val = builder
                    .ins()
                    .load(types::I8, MemFlags::trusted(), char_ptr, 0);
                let expected = builder.ins().iconst(types::I8, byte as i64);
                let byte_matches = builder.ins().icmp(IntCC::Equal, char_val, expected);
                // Convert bool to i8: select(cond, 1, 0)
                let one = builder.ins().iconst(types::I8, 1);
                let zero = builder.ins().iconst(types::I8, 0);
                let byte_match_i8 = builder.ins().select(byte_matches, one, zero);
                all_match = builder.ins().band(all_match, byte_match_i8);
            }

            // Convert all_match back to bool for branch
            let all_match_bool = builder.ins().icmp_imm(IntCC::NotEqual, all_match, 0);
            builder
                .ins()
                .brif(all_match_bool, match_blocks[i], &[], next_check, &[]);
            builder.seal_block(check_content);

            current_block = next_check;
        }

        // Seal key_dispatch now that we're done filling it
        // (Its only predecessor is check_read_key_err from line 1779)
        builder.seal_block(key_dispatch);

        // Seal the last next_check block if we created any
        // (On last iteration, current_block = unknown_key, so we seal it here)
        if field_infos.len() > 1 {
            builder.seal_block(current_block);
        }

        // unknown_key: skip the value
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

        // Skip the value
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
        let drop_id =
            match module.declare_function("jit_drop_owned_string", Linkage::Import, &sig_drop) {
                Ok(id) => id,
                Err(_e) => {
                    jit_debug!("[compile_struct] declare jit_drop_owned_string failed");
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
        // Note: unknown_key may already be sealed if field_infos.len() > 1
        // (it's sealed as current_block on the last field iteration)
        if field_infos.len() == 1 {
            builder.seal_block(unknown_key);
        }

        // Implement match blocks for each field
        // This is where we parse the field value based on its type
        for (i, field_info) in field_infos.iter().enumerate() {
            builder.switch_to_block(match_blocks[i]);

            // First, consume the kv separator (':' in JSON)
            let mut cursor = JitCursor {
                input_ptr,
                len,
                pos: pos_var,
                ptr_type: pointer_type,
            };

            let format = F::default();
            let err_code = format.emit_map_kv_sep(module, &mut builder, &mut cursor, state_ptr);
            builder.def_var(err_var, err_code);

            // Check for error
            let kv_sep_ok = builder.create_block();
            let is_ok = builder.ins().icmp_imm(IntCC::Equal, err_code, 0);
            builder.ins().brif(is_ok, kv_sep_ok, &[], error, &[]);

            builder.switch_to_block(kv_sep_ok);

            // Now parse the field value based on its type
            let field_shape = field_info.shape;
            let field_ptr = builder.ins().iadd_imm(out_ptr, field_info.offset as i64);

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
                    ScalarType::I8 | ScalarType::I16 | ScalarType::I32 | ScalarType::I64 => {
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
                    ScalarType::U8 | ScalarType::U16 | ScalarType::U32 | ScalarType::U64 => {
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
                                jit_debug!("[compile_struct] declare jit_write_string failed");
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
            } else {
                // Non-scalar field (Vec, Option, nested struct) - MVP doesn't support yet
                // Fall back to Tier-1 for now
                jit_debug!(
                    "[compile_struct] Field {} has unsupported type (Vec/Option/struct)",
                    field_info.name
                );
                return None;
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

    if let Err(_e) = module.define_function(func_id, &mut ctx) {
        jit_debug!("[compile_struct] define_function failed: {:?}", _e);
        return None;
    }

    jit_debug!("[compile_struct] SUCCESS - function compiled");
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
