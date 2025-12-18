//! Tier-2 Format JIT Compiler
//!
//! This module compiles deserializers that parse bytes directly using format-specific
//! IR generation, bypassing the event abstraction for maximum performance.
//!
//! ## ABI
//!
//! All Tier-2 compiled functions share this signature:
//! ```ignore
//! unsafe extern "C" fn(
//!     input_ptr: *const u8,
//!     len: usize,
//!     pos: usize,
//!     out: *mut u8,
//!     scratch: *mut JitScratch,
//! ) -> isize
//! ```
//!
//! Returns:
//! - `>= 0`: new cursor position (success)
//! - `< 0`: failure; error details written to `scratch`

use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::sync::Arc;

use cranelift::prelude::*;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, Linkage, Module};

use facet_core::{Def, Facet, Shape};

use super::format::{
    JIT_SCRATCH_ERROR_CODE_OFFSET, JIT_SCRATCH_ERROR_POS_OFFSET, JitCursor, JitFormat, JitScratch,
};
use super::helpers;
use super::jit_debug;
use crate::DeserializeError;
use crate::jit::FormatJitParser;

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
    /// Shared reference to the cached module (keeps code memory alive)
    cached: Arc<CachedFormatModule>,
    /// Phantom data for type safety
    _phantom: PhantomData<fn(&mut P) -> T>,
}

// Safety: The compiled code is thread-safe (no mutable static state)
unsafe impl<T, P> Send for CompiledFormatDeserializer<T, P> {}
unsafe impl<T, P> Sync for CompiledFormatDeserializer<T, P> {}

impl<T, P> CompiledFormatDeserializer<T, P> {
    /// Create from a cached module.
    pub fn from_cached(cached: Arc<CachedFormatModule>) -> Self {
        Self {
            cached,
            _phantom: PhantomData,
        }
    }

    /// Get the raw function pointer.
    pub fn fn_ptr(&self) -> *const u8 {
        self.cached.fn_ptr()
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
            // Error: convert via parser's error handler
            jit_debug!(
                "[Tier-2] Error: code={}, pos={}",
                scratch.error_code,
                scratch.error_pos
            );
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

        // TODO: Add struct support later
        false
    }
}

/// Check if a Vec element type is supported for Tier-2.
///
/// For MVP, only `bool` is fully implemented. Other types will be added
/// once their parse helpers are implemented in format crates.
fn is_format_jit_element_supported(elem_shape: &'static Shape) -> bool {
    use facet_core::ScalarType;

    if let Some(scalar_type) = elem_shape.scalar_type() {
        // MVP: Only bool is fully implemented
        // TODO: Add i64/u64/f64/String support when helpers are implemented
        return matches!(scalar_type, ScalarType::Bool);
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
    } else {
        #[cfg(debug_assertions)]
        jit_debug!("[Tier-2 JIT] Not a list type");
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
    builder.symbol("jit_vec_push_i64", helpers::jit_vec_push_i64 as *const u8);
    builder.symbol("jit_vec_push_u64", helpers::jit_vec_push_u64 as *const u8);
    builder.symbol("jit_vec_push_f64", helpers::jit_vec_push_f64 as *const u8);
    builder.symbol(
        "jit_vec_push_string",
        helpers::jit_vec_push_string as *const u8,
    );

    // Tier-2 specific helpers
    builder.symbol(
        "jit_drop_owned_string",
        helpers::jit_drop_owned_string as *const u8,
    );
}

/// Element type for Tier-2 list codegen.
#[derive(Debug, Clone, Copy)]
enum FormatListElementKind {
    Bool,
    I64,
    U64,
    F64,
    String,
}

impl FormatListElementKind {
    fn from_shape(shape: &Shape) -> Option<Self> {
        use facet_core::ScalarType;
        let scalar_type = shape.scalar_type()?;
        match scalar_type {
            ScalarType::Bool => Some(Self::Bool),
            ScalarType::I8 | ScalarType::I16 | ScalarType::I32 | ScalarType::I64 => Some(Self::I64),
            ScalarType::U8 | ScalarType::U16 | ScalarType::U32 | ScalarType::U64 => Some(Self::U64),
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

    let sig_vec_push = match elem_kind {
        FormatListElementKind::Bool => {
            let mut s = module.make_signature();
            s.params.push(AbiParam::new(pointer_type)); // vec_ptr
            s.params.push(AbiParam::new(pointer_type)); // push_fn
            s.params.push(AbiParam::new(types::I8)); // value
            s
        }
        FormatListElementKind::I64 | FormatListElementKind::U64 => {
            let mut s = module.make_signature();
            s.params.push(AbiParam::new(pointer_type)); // vec_ptr
            s.params.push(AbiParam::new(pointer_type)); // push_fn
            s.params.push(AbiParam::new(types::I64)); // value
            s
        }
        FormatListElementKind::F64 => {
            let mut s = module.make_signature();
            s.params.push(AbiParam::new(pointer_type)); // vec_ptr
            s.params.push(AbiParam::new(pointer_type)); // push_fn
            s.params.push(AbiParam::new(types::F64)); // value
            s
        }
        FormatListElementKind::String => {
            let mut s = module.make_signature();
            s.params.push(AbiParam::new(pointer_type)); // vec_ptr
            s.params.push(AbiParam::new(pointer_type)); // push_fn
            s.params.push(AbiParam::new(pointer_type)); // ptr
            s.params.push(AbiParam::new(pointer_type)); // len
            s.params.push(AbiParam::new(pointer_type)); // cap
            s.params.push(AbiParam::new(types::I8)); // owned
            s
        }
    };

    // All format-specific operations are now inlined via format.emit_*()
    // No format helper signatures needed

    // Declare Vec helper functions
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

    let push_fn_name = match elem_kind {
        FormatListElementKind::Bool => "jit_vec_push_bool",
        FormatListElementKind::I64 => "jit_vec_push_i64",
        FormatListElementKind::U64 => "jit_vec_push_u64",
        FormatListElementKind::F64 => "jit_vec_push_f64",
        FormatListElementKind::String => "jit_vec_push_string",
    };
    let vec_push_id = match module.declare_function(push_fn_name, Linkage::Import, &sig_vec_push) {
        Ok(id) => id,
        Err(_e) => {
            jit_debug!("[compile_list] declare {} failed: {:?}", push_fn_name, _e);
            return None;
        }
    };

    // All format-specific operations are now inlined via format.emit_*()
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

        // Import Vec helper functions (format operations are inlined via emit_*)
        let vec_init_ref = module.declare_func_in_func(vec_init_id, builder.func);
        let vec_push_ref = module.declare_func_in_func(vec_push_id, builder.func);

        // Create blocks
        let entry = builder.create_block();
        let seq_begin = builder.create_block();
        let check_seq_begin_err = builder.create_block();
        let init_vec = builder.create_block();
        let loop_check_end = builder.create_block();
        let check_is_end_err = builder.create_block();
        let check_is_end_value = builder.create_block();
        let parse_element = builder.create_block();
        let check_parse_err = builder.create_block();
        let push_element = builder.create_block();
        let seq_next = builder.create_block();
        let check_seq_next_err = builder.create_block();
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

        // Variable to hold parsed bool value
        let parsed_value_var = builder.declare_var(types::I8);
        let zero_i8 = builder.ins().iconst(types::I8, 0);
        builder.def_var(parsed_value_var, zero_i8);

        // Variable for error code (used across blocks)
        let err_var = builder.declare_var(types::I32);
        let zero_i32 = builder.ins().iconst(types::I32, 0);
        builder.def_var(err_var, zero_i32);

        // Variable for is_end flag (used across blocks)
        let is_end_var = builder.declare_var(pointer_type);
        let zero_ptr = builder.ins().iconst(pointer_type, 0);
        builder.def_var(is_end_var, zero_ptr);

        // Constants
        let init_fn_ptr = builder
            .ins()
            .iconst(pointer_type, init_fn as *const () as i64);
        let push_fn_ptr = builder
            .ins()
            .iconst(pointer_type, push_fn as *const () as i64);
        let zero_cap = builder.ins().iconst(pointer_type, 0);

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
        let format = F::default();
        let state_ptr = builder.ins().iconst(pointer_type, 0); // Unused for JSON
        let err_code = format.emit_seq_begin(&mut builder, &mut cursor, state_ptr);

        // emit_seq_begin leaves us at its merge block and updates cursor.pos internally
        builder.def_var(err_var, err_code);
        builder.ins().jump(check_seq_begin_err, &[]);
        builder.seal_block(seq_begin);

        // check_seq_begin_err
        builder.switch_to_block(check_seq_begin_err);
        let is_ok = builder.ins().icmp_imm(IntCC::Equal, err_code, 0);
        builder.ins().brif(is_ok, init_vec, &[], error, &[]);
        builder.seal_block(check_seq_begin_err);

        // init_vec: initialize Vec with capacity 0
        builder.switch_to_block(init_vec);
        builder
            .ins()
            .call(vec_init_ref, &[out_ptr, zero_cap, init_fn_ptr]);
        builder.ins().jump(loop_check_end, &[]);
        builder.seal_block(init_vec);

        // loop_check_end: use inline IR for seq_is_end
        //
        // WHITESPACE INVARIANT: At loop entry, pos always points to a non-whitespace byte
        // (or EOF). This is maintained by:
        //   - seq_begin skips whitespace after '['
        //   - seq_next skips whitespace after ',' and before the separator check
        //   - emit_parse_bool does NOT skip leading whitespace (relies on this invariant)
        //   - emit_seq_is_end does NOT skip leading whitespace (relies on this invariant)
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
        let state_ptr = builder.ins().iconst(pointer_type, 0); // Unused for JSON
        let (is_end_i8, err_code) = format.emit_seq_is_end(&mut builder, &mut cursor, state_ptr);

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

        // parse_element: use inline IR for parsing (for MVP, only bool supported)
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
                let (value_i8, err_code) = format.emit_parse_bool(&mut builder, &mut cursor);

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
            _ => {
                // For other types, set a constant error code and jump to error
                let unsupported_err = builder.ins().iconst(types::I32, T2_ERR_UNSUPPORTED as i64);
                builder.def_var(err_var, unsupported_err);
                builder.ins().jump(error, &[]);
                builder.seal_block(parse_element);
                builder.switch_to_block(check_parse_err);
                builder.ins().jump(error, &[]);
                builder.seal_block(check_parse_err);
            }
        }

        // push_element: push to vec
        builder.switch_to_block(push_element);
        let parsed_value = builder.use_var(parsed_value_var);
        builder
            .ins()
            .call(vec_push_ref, &[out_ptr, push_fn_ptr, parsed_value]);
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
        let state_ptr = builder.ins().iconst(pointer_type, 0); // Unused for JSON
        let err_code = format.emit_seq_next(&mut builder, &mut cursor, state_ptr);

        // emit_seq_next leaves us at its merge block and updates cursor.pos internally
        builder.def_var(err_var, err_code);
        builder.ins().jump(check_seq_next_err, &[]);
        builder.seal_block(seq_next);

        // check_seq_next_err
        builder.switch_to_block(check_seq_next_err);
        let next_ok = builder.ins().icmp_imm(IntCC::Equal, err_code, 0);
        builder.ins().brif(next_ok, loop_check_end, &[], error, &[]);
        builder.seal_block(check_seq_next_err);
        // Now we can seal loop_check_end since all predecessors (init_vec, check_seq_next_err) are declared
        builder.seal_block(loop_check_end);

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_jit_compatibility() {
        // Vec<bool> should be supported (MVP)
        assert!(is_format_jit_compatible(<Vec<bool>>::SHAPE));

        // Vec<i64> is NOT supported yet (parse_i64 helper not implemented)
        assert!(!is_format_jit_compatible(<Vec<i64>>::SHAPE));

        // Primitive types alone are not supported (need to be in a container)
        assert!(!is_format_jit_compatible(bool::SHAPE));
        assert!(!is_format_jit_compatible(i64::SHAPE));
    }
}
