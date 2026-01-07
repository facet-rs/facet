//! Cranelift-based compiler for deserializers.
//!
//! This module takes a `Shape` and generates native code that consumes
//! `ParseEvent`s and writes directly to struct memory.

use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::sync::Arc;

use cranelift::prelude::*;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, Linkage, Module};

use facet_core::{Def, Facet, Field, Shape, Type as FacetType, UserType};

use super::format::make_c_sig;
use super::helpers::{self, JitContext, ParserVTable};
use super::jit_debug;
use crate::{DeserializeError, FormatParser};

/// Cached JIT module(s) that own the compiled code memory.
///
/// This struct keeps the JITModule alive to prevent the compiled code from being freed.
/// It also stores nested modules for complex types (e.g., structs with nested struct fields).
pub struct CachedModule {
    /// The main JIT module that owns the compiled code
    #[allow(dead_code)]
    module: JITModule,
    /// Nested modules for complex types (structs with nested structs)
    #[allow(dead_code)]
    nested_modules: Vec<JITModule>,
    /// The function pointer to the compiled deserializer
    fn_ptr: *const u8,
}

impl CachedModule {
    /// Create a new cached module.
    pub fn new(module: JITModule, nested_modules: Vec<JITModule>, fn_ptr: *const u8) -> Self {
        Self {
            module,
            nested_modules,
            fn_ptr,
        }
    }

    /// Get the function pointer.
    pub fn fn_ptr(&self) -> *const u8 {
        self.fn_ptr
    }
}

// Safety: The compiled code is thread-safe (no mutable state)
unsafe impl Send for CachedModule {}
unsafe impl Sync for CachedModule {}

/// A compiled deserializer for a specific type and parser.
pub struct CompiledDeserializer<T, P> {
    /// Pointer to the compiled function
    fn_ptr: *const u8,
    /// VTable for calling parser methods from JIT code
    vtable: ParserVTable,
    /// Reference to the cached module that owns the compiled code.
    /// This keeps the JIT memory alive while the deserializer is in use.
    _cached: Arc<CachedModule>,
    /// Phantom data for type safety
    _phantom: PhantomData<fn(&mut P) -> T>,
}

// Safety: The compiled code is thread-safe
unsafe impl<T, P> Send for CompiledDeserializer<T, P> {}
unsafe impl<T, P> Sync for CompiledDeserializer<T, P> {}

impl<T, P> CompiledDeserializer<T, P> {
    /// Create from a cached module and vtable.
    pub fn from_cached(cached: Arc<CachedModule>, vtable: ParserVTable) -> Self {
        let fn_ptr = cached.fn_ptr();
        Self {
            fn_ptr,
            vtable,
            _cached: cached,
            _phantom: PhantomData,
        }
    }

    /// Get the raw function pointer.
    pub fn as_ptr(&self) -> *const u8 {
        self.fn_ptr
    }

    /// Get the vtable.
    pub fn vtable(&self) -> &ParserVTable {
        &self.vtable
    }
}

impl<'de, T: Facet<'de>, P: FormatParser<'de>> CompiledDeserializer<T, P> {
    /// Execute the compiled deserializer.
    pub fn deserialize(&self, parser: &mut P) -> Result<T, DeserializeError<P::Error>> {
        // Create output storage
        let mut output: MaybeUninit<T> = MaybeUninit::uninit();

        // Create JIT context with parser pointer and vtable
        let mut ctx = JitContext {
            parser: parser as *mut P as *mut (),
            vtable: &self.vtable,
            peeked_event: None,
            fields_seen: 0, // Tracks which fields have been initialized (for cleanup on error)
        };

        if super::jit_debug_enabled() {
            jit_debug!("About to call compiled function at {:p}", self.fn_ptr);
            jit_debug!("  ctx: {:p}", &mut ctx);
            jit_debug!("  out: {:p}", output.as_mut_ptr());

            // Dump first 16 bytes of the function to see if it looks like code
            let code_bytes = unsafe { std::slice::from_raw_parts(self.fn_ptr, 16) };
            let bytes_str: String = code_bytes.iter().map(|b| format!("{:02x} ", b)).collect();
            jit_debug!("  First 16 bytes of function: {}", bytes_str);
        }

        // Call the compiled function
        // Signature: fn(ctx: *mut JitContext, out: *mut T) -> i32
        type CompiledFn<T> = unsafe extern "C" fn(*mut JitContext, *mut T) -> i32;
        let func: CompiledFn<T> = unsafe { std::mem::transmute(self.fn_ptr) };

        let result = unsafe { func(&mut ctx, output.as_mut_ptr()) };

        #[cfg(debug_assertions)]
        eprintln!("[JIT] Compiled function returned: {}", result);

        if result == 0 {
            // Safe: the JIT code validates all required fields are set via bitmask tracking
            // before returning 0 (success). Required fields (non-Option) must all be present,
            // otherwise the JIT returns ERR_MISSING_REQUIRED_FIELD.
            Ok(unsafe { output.assume_init() })
        } else {
            // Error path: clean up any partially-initialized fields
            // The JIT stored which fields were written in ctx.fields_seen
            let fields_seen = ctx.fields_seen;
            if fields_seen != 0 {
                // Drop any fields that were initialized before the error
                unsafe {
                    cleanup_partial_struct::<T>(output.as_mut_ptr() as *mut u8, fields_seen);
                }
            }

            if result == helpers::ERR_MISSING_REQUIRED_FIELD {
                Err(DeserializeError::MissingField {
                    field: "unknown", // TODO: Track which field is missing
                    type_name: T::SHAPE.type_identifier,
                    span: None, // JIT doesn't have span info
                    path: None, // JIT doesn't have path info
                })
            } else {
                Err(DeserializeError::Unsupported(format!(
                    "JIT deserialization failed with code {}",
                    result
                )))
            }
        }
    }
}

/// Clean up a partially-initialized struct by dropping fields that were written.
///
/// # Safety
/// - `ptr` must point to valid memory for type T
/// - `fields_seen` must accurately reflect which fields have been initialized
/// - Only fields that actually need Drop will be dropped
unsafe fn cleanup_partial_struct<'a, T: Facet<'a>>(ptr: *mut u8, fields_seen: u64) {
    use facet_core::PtrMut;

    let shape = T::SHAPE;
    let FacetType::User(UserType::Struct(struct_def)) = &shape.ty else {
        return; // Only structs need cleanup
    };

    for (idx, field) in struct_def.fields.iter().enumerate() {
        // Check if this field was initialized
        if (fields_seen & (1u64 << idx)) == 0 {
            continue;
        }

        // Get the field's shape to check if it needs Drop
        let field_shape = field.shape();

        // Drop the field if it has a drop_in_place function
        // SAFETY: The field was initialized by the JIT, so it's safe to drop
        unsafe {
            let field_ptr = ptr.add(field.offset);
            let _ = field_shape.call_drop_in_place(PtrMut::new(field_ptr));
        }
    }
}

/// Check if a shape is JIT-compatible.
///
/// Currently supports:
/// - Simple structs without flatten fields or untagged enums
/// - `Vec<T>` where T is a supported element type (scalars, strings, nested Vecs, JIT-compatible structs)
pub fn is_jit_compatible(shape: &'static Shape) -> bool {
    // Check for Vec<T> types
    if let Def::List(list_def) = &shape.def {
        return is_vec_element_supported(list_def.t);
    }

    // Check if it's a struct via shape.ty
    let FacetType::User(UserType::Struct(struct_def)) = &shape.ty else {
        return false;
    };

    // Check for flatten
    if struct_def.fields.iter().any(|f| f.is_flattened()) {
        return false;
    }

    // Check that all field types are supported
    struct_def.fields.iter().all(is_field_type_supported)
}

/// Check if a field type is supported for JIT compilation.
fn is_field_type_supported(field: &Field) -> bool {
    // Just check if WriteKind::from_shape can handle this type
    WriteKind::from_shape(field.shape()).is_some()
}

/// Check if a Vec element type is supported for JIT compilation.
///
/// Supports:
/// - Primitives (f64, i64, etc.)
/// - Strings
/// - Nested Vecs (if their elements are also supported)
/// - JIT-compatible structs
fn is_vec_element_supported(elem_shape: &'static Shape) -> bool {
    use facet_core::ScalarType;

    // Check for supported scalar types
    if let Some(scalar_type) = elem_shape.scalar_type() {
        return matches!(
            scalar_type,
            ScalarType::Bool
                | ScalarType::U8
                | ScalarType::U16
                | ScalarType::U32
                | ScalarType::U64
                | ScalarType::I8
                | ScalarType::I16
                | ScalarType::I32
                | ScalarType::I64
                | ScalarType::F32
                | ScalarType::F64
                | ScalarType::String
        );
    }

    // Check for nested Vec
    if let Def::List(list_def) = &elem_shape.def {
        return is_vec_element_supported(list_def.t);
    }

    // Check for JIT-compatible struct
    if let FacetType::User(UserType::Struct(_)) = &elem_shape.ty {
        return is_jit_compatible(elem_shape);
    }

    false
}

/// Result of compiling a module, containing everything needed for caching.
pub struct CompileResult {
    /// The main JIT module
    pub module: JITModule,
    /// Nested modules for complex types
    pub nested_modules: Vec<JITModule>,
    /// Function pointer to the compiled deserializer
    pub fn_ptr: *const u8,
}

/// Try to compile a module for the given type.
///
/// Returns the module, nested modules, and function pointer on success.
/// The caller is responsible for keeping the modules alive.
pub fn try_compile_module<'de, T: Facet<'de>>() -> Option<CompileResult> {
    let shape = T::SHAPE;

    if !is_jit_compatible(shape) {
        return None;
    }

    // Build the JIT module
    let mut builder = JITBuilder::new(cranelift_module::default_libcall_names()).ok()?;

    // Register helper functions
    register_helpers(&mut builder);

    let mut module = JITModule::new(builder);

    // Compile the deserializer based on type
    let (func_id, nested_modules) = if let Def::List(_) = &shape.def {
        let func_id = compile_list_deserializer(&mut module, shape)?;
        (func_id, Vec::new())
    } else {
        compile_deserializer(&mut module, shape)?
    };

    // Finalize and get the function pointer
    module.finalize_definitions().ok()?;
    let fn_ptr = module.get_finalized_function(func_id);

    Some(CompileResult {
        module,
        nested_modules,
        fn_ptr,
    })
}

/// Register helper functions with the JIT module.
fn register_helpers(builder: &mut JITBuilder) {
    // Register the write helpers
    builder.symbol("jit_write_u8", helpers::jit_write_u8 as *const u8);
    builder.symbol("jit_write_u16", helpers::jit_write_u16 as *const u8);
    builder.symbol("jit_write_u32", helpers::jit_write_u32 as *const u8);
    builder.symbol("jit_write_u64", helpers::jit_write_u64 as *const u8);
    builder.symbol("jit_write_i8", helpers::jit_write_i8 as *const u8);
    builder.symbol("jit_write_i16", helpers::jit_write_i16 as *const u8);
    builder.symbol("jit_write_i32", helpers::jit_write_i32 as *const u8);
    builder.symbol("jit_write_i64", helpers::jit_write_i64 as *const u8);
    builder.symbol("jit_write_f32", helpers::jit_write_f32 as *const u8);
    builder.symbol("jit_write_f64", helpers::jit_write_f64 as *const u8);
    builder.symbol("jit_write_bool", helpers::jit_write_bool as *const u8);
    builder.symbol("jit_write_string", helpers::jit_write_string as *const u8);
    builder.symbol("jit_memcpy", helpers::jit_memcpy as *const u8);
    builder.symbol(
        "jit_write_error_string",
        helpers::jit_write_error_string as *const u8,
    );
    builder.symbol("jit_field_matches", helpers::jit_field_matches as *const u8);
    builder.symbol(
        "jit_deserialize_nested",
        helpers::jit_deserialize_nested as *const u8,
    );
    builder.symbol("jit_peek_event", helpers::jit_peek_event as *const u8);
    builder.symbol("jit_next_event", helpers::jit_next_event as *const u8);
    builder.symbol(
        "jit_option_init_none",
        helpers::jit_option_init_none as *const u8,
    );
    builder.symbol(
        "jit_option_init_some_from_value",
        helpers::jit_option_init_some_from_value as *const u8,
    );
    builder.symbol(
        "jit_vec_init_with_capacity",
        helpers::jit_vec_init_with_capacity as *const u8,
    );
    builder.symbol("jit_vec_push", helpers::jit_vec_push as *const u8);
    builder.symbol(
        "jit_deserialize_vec",
        helpers::jit_deserialize_vec as *const u8,
    );
    builder.symbol(
        "jit_deserialize_list_by_shape",
        helpers::jit_deserialize_list_by_shape as *const u8,
    );
    // Specialized push helpers for Vec JIT
    builder.symbol("jit_vec_push_bool", helpers::jit_vec_push_bool as *const u8);
    builder.symbol("jit_vec_push_i64", helpers::jit_vec_push_i64 as *const u8);
    builder.symbol("jit_vec_push_u64", helpers::jit_vec_push_u64 as *const u8);
    builder.symbol("jit_vec_push_f64", helpers::jit_vec_push_f64 as *const u8);
    builder.symbol(
        "jit_vec_push_string",
        helpers::jit_vec_push_string as *const u8,
    );
}

/// Element type classification for JIT code generation.
#[derive(Debug, Clone, Copy)]
enum ListElementKind {
    Bool,
    I64,
    U64,
    F64,
    String,
}

impl ListElementKind {
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

    /// Returns the expected ScalarTag values for this element kind.
    /// For integers, we accept both I64 and U64 since JSON doesn't distinguish them.
    fn expected_scalar_tags(&self) -> (u8, Option<u8>) {
        use helpers::ScalarTag;
        match self {
            ListElementKind::Bool => (ScalarTag::Bool as u8, None),
            // Accept both I64 and U64 for integer types
            ListElementKind::I64 | ListElementKind::U64 => {
                (ScalarTag::I64 as u8, Some(ScalarTag::U64 as u8))
            }
            ListElementKind::F64 => (ScalarTag::F64 as u8, None),
            ListElementKind::String => (ScalarTag::Str as u8, None),
        }
    }
}

/// Compile a deserializer function for a Vec/List type.
///
/// Generates specialized JIT code for the element type - no generic helper calls.
fn compile_list_deserializer(module: &mut JITModule, shape: &'static Shape) -> Option<FuncId> {
    let Def::List(list_def) = &shape.def else {
        return None;
    };

    let elem_shape = list_def.t;
    let elem_kind = ListElementKind::from_shape(elem_shape)?;

    // Get the init and push functions from the list def
    let init_fn = list_def.init_in_place_with_capacity()?;
    let push_fn = list_def.push()?;

    let pointer_type = module.target_config().pointer_type();

    // Function signature: fn(ctx: *mut JitContext, out: *mut T) -> i32
    let sig = {
        let mut s = make_c_sig(module);
        s.params.push(AbiParam::new(pointer_type)); // ctx: *mut JitContext
        s.params.push(AbiParam::new(pointer_type)); // out: *mut T
        s.returns.push(AbiParam::new(types::I32)); // result
        s
    };

    // Helper signatures
    let sig_next_event = {
        let mut s = make_c_sig(module);
        s.params.push(AbiParam::new(pointer_type)); // ctx
        s.params.push(AbiParam::new(pointer_type)); // out: *mut RawEvent
        s.returns.push(AbiParam::new(types::I32)); // result
        s
    };

    let sig_peek_event = sig_next_event.clone();

    let sig_vec_init = {
        let mut s = make_c_sig(module);
        s.params.push(AbiParam::new(pointer_type)); // out: *mut u8
        s.params.push(AbiParam::new(pointer_type)); // capacity: usize
        s.params.push(AbiParam::new(pointer_type)); // init_fn: *const u8
        s
    };

    // Push signatures vary by type
    let sig_vec_push_scalar = {
        let mut s = make_c_sig(module);
        s.params.push(AbiParam::new(pointer_type)); // vec_ptr
        s.params.push(AbiParam::new(pointer_type)); // push_fn
        match elem_kind {
            ListElementKind::Bool => s.params.push(AbiParam::new(types::I8)), // bool as i8
            ListElementKind::I64 => s.params.push(AbiParam::new(types::I64)),
            ListElementKind::U64 => s.params.push(AbiParam::new(types::I64)), // u64 passed as i64
            ListElementKind::F64 => s.params.push(AbiParam::new(types::F64)),
            ListElementKind::String => {
                s.params.push(AbiParam::new(pointer_type)); // ptr
                s.params.push(AbiParam::new(pointer_type)); // len
                s.params.push(AbiParam::new(pointer_type)); // capacity
                s.params.push(AbiParam::new(types::I8)); // owned
            }
        }
        s
    };

    // Declare helper functions
    let next_event_id = module
        .declare_function("jit_next_event", Linkage::Import, &sig_next_event)
        .ok()?;
    let _peek_event_id = module
        .declare_function("jit_peek_event", Linkage::Import, &sig_peek_event)
        .ok()?;
    let vec_init_id = module
        .declare_function("jit_vec_init_with_capacity", Linkage::Import, &sig_vec_init)
        .ok()?;

    let push_fn_name = match elem_kind {
        ListElementKind::Bool => "jit_vec_push_bool",
        ListElementKind::I64 => "jit_vec_push_i64",
        ListElementKind::U64 => "jit_vec_push_u64",
        ListElementKind::F64 => "jit_vec_push_f64",
        ListElementKind::String => "jit_vec_push_string",
    };
    let vec_push_id = module
        .declare_function(push_fn_name, Linkage::Import, &sig_vec_push_scalar)
        .ok()?;

    // Declare our function
    let func_id = module
        .declare_function("jit_deserialize_list", Linkage::Local, &sig)
        .ok()?;

    let mut ctx = module.make_context();
    ctx.func.signature = sig;

    let mut builder_ctx = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut builder_ctx);

        // Import helper functions
        let next_event_ref = module.declare_func_in_func(next_event_id, builder.func);
        let vec_init_ref = module.declare_func_in_func(vec_init_id, builder.func);
        let vec_push_ref = module.declare_func_in_func(vec_push_id, builder.func);

        // Create all blocks upfront
        let entry = builder.create_block();
        let check_array_start = builder.create_block();
        let init_vec = builder.create_block();
        let loop_peek = builder.create_block(); // Named loop_peek for clarity, but uses next_event
        let check_end = builder.create_block();
        let push_elem = builder.create_block();
        let success = builder.create_block();
        let error = builder.create_block();

        // Entry block
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);

        let ctx_ptr = builder.block_params(entry)[0];
        let out_ptr = builder.block_params(entry)[1];

        // Allocate stack slot for RawEvent
        let raw_event_slot = builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            helpers::RAW_EVENT_SIZE as u32,
            8,
        ));
        let raw_event_ptr = builder.ins().stack_addr(pointer_type, raw_event_slot, 0);

        // Constants
        let init_fn_ptr = builder
            .ins()
            .iconst(pointer_type, init_fn as *const () as i64);
        let push_fn_ptr = builder
            .ins()
            .iconst(pointer_type, push_fn as *const () as i64);
        let zero_cap = builder.ins().iconst(pointer_type, 0);

        // Read first event (should be ArrayStart)
        let call = builder
            .ins()
            .call(next_event_ref, &[ctx_ptr, raw_event_ptr]);
        let result = builder.inst_results(call)[0];
        let is_ok = builder.ins().icmp_imm(IntCC::Equal, result, 0);
        builder
            .ins()
            .brif(is_ok, check_array_start, &[], error, &[]);
        builder.seal_block(entry);

        // Check ArrayStart
        builder.switch_to_block(check_array_start);
        let tag = builder.ins().load(
            types::I8,
            MemFlags::trusted(),
            raw_event_ptr,
            helpers::RAW_EVENT_TAG_OFFSET as i32,
        );
        let is_array_start =
            builder
                .ins()
                .icmp_imm(IntCC::Equal, tag, helpers::EventTag::ArrayStart as i64);
        builder
            .ins()
            .brif(is_array_start, init_vec, &[], error, &[]);
        builder.seal_block(check_array_start);

        // Initialize Vec
        builder.switch_to_block(init_vec);
        builder
            .ins()
            .call(vec_init_ref, &[out_ptr, zero_cap, init_fn_ptr]);
        builder.ins().jump(loop_peek, &[]);
        builder.seal_block(init_vec);

        // Loop: get next event (combines peek+consume into single call)
        builder.switch_to_block(loop_peek);
        let call = builder
            .ins()
            .call(next_event_ref, &[ctx_ptr, raw_event_ptr]);
        let result = builder.inst_results(call)[0];
        let is_ok = builder.ins().icmp_imm(IntCC::Equal, result, 0);
        builder.ins().brif(is_ok, check_end, &[], error, &[]);

        // Check for ArrayEnd - if so, we're done; otherwise push the element
        builder.switch_to_block(check_end);
        let tag = builder.ins().load(
            types::I8,
            MemFlags::trusted(),
            raw_event_ptr,
            helpers::RAW_EVENT_TAG_OFFSET as i32,
        );
        let is_end = builder
            .ins()
            .icmp_imm(IntCC::Equal, tag, helpers::EventTag::ArrayEnd as i64);
        builder.ins().brif(is_end, success, &[], push_elem, &[]);
        builder.seal_block(check_end);

        // Push element - extract scalar from payload and call push
        // The event data is already in raw_event_ptr from next_event above
        builder.switch_to_block(push_elem);

        // Validate scalar tag before reading payload to prevent type confusion
        let (primary_tag, alt_tag) = elem_kind.expected_scalar_tags();
        let actual_tag = builder.ins().load(
            types::I8,
            MemFlags::trusted(),
            raw_event_ptr,
            helpers::RAW_EVENT_SCALAR_TAG_OFFSET as i32,
        );
        let primary_matches = builder
            .ins()
            .icmp_imm(IntCC::Equal, actual_tag, primary_tag as i64);

        let validated_block = builder.create_block();
        if let Some(alt) = alt_tag {
            // For integers, accept both I64 and U64
            let check_alt_block = builder.create_block();
            builder
                .ins()
                .brif(primary_matches, validated_block, &[], check_alt_block, &[]);
            builder.switch_to_block(check_alt_block);
            let alt_matches = builder.ins().icmp_imm(IntCC::Equal, actual_tag, alt as i64);
            builder
                .ins()
                .brif(alt_matches, validated_block, &[], error, &[]);
            builder.seal_block(check_alt_block);
        } else {
            builder
                .ins()
                .brif(primary_matches, validated_block, &[], error, &[]);
        }
        builder.switch_to_block(validated_block);

        let payload_ptr = builder
            .ins()
            .iadd_imm(raw_event_ptr, helpers::RAW_EVENT_PAYLOAD_OFFSET as i64);

        match elem_kind {
            ListElementKind::Bool => {
                let val = builder
                    .ins()
                    .load(types::I8, MemFlags::trusted(), payload_ptr, 0);
                builder
                    .ins()
                    .call(vec_push_ref, &[out_ptr, push_fn_ptr, val]);
            }
            ListElementKind::I64 => {
                let val = builder
                    .ins()
                    .load(types::I64, MemFlags::trusted(), payload_ptr, 0);
                builder
                    .ins()
                    .call(vec_push_ref, &[out_ptr, push_fn_ptr, val]);
            }
            ListElementKind::U64 => {
                let val = builder
                    .ins()
                    .load(types::I64, MemFlags::trusted(), payload_ptr, 0);
                builder
                    .ins()
                    .call(vec_push_ref, &[out_ptr, push_fn_ptr, val]);
            }
            ListElementKind::F64 => {
                let val = builder
                    .ins()
                    .load(types::F64, MemFlags::trusted(), payload_ptr, 0);
                builder
                    .ins()
                    .call(vec_push_ref, &[out_ptr, push_fn_ptr, val]);
            }
            ListElementKind::String => {
                // Load string payload fields
                let str_ptr = builder.ins().load(
                    pointer_type,
                    MemFlags::trusted(),
                    payload_ptr,
                    helpers::STRING_PTR_OFFSET as i32,
                );
                let str_len = builder.ins().load(
                    pointer_type,
                    MemFlags::trusted(),
                    payload_ptr,
                    helpers::STRING_LEN_OFFSET as i32,
                );
                let str_cap = builder.ins().load(
                    pointer_type,
                    MemFlags::trusted(),
                    payload_ptr,
                    helpers::STRING_CAPACITY_OFFSET as i32,
                );
                let str_owned = builder.ins().load(
                    types::I8,
                    MemFlags::trusted(),
                    payload_ptr,
                    helpers::STRING_OWNED_OFFSET as i32,
                );
                builder.ins().call(
                    vec_push_ref,
                    &[out_ptr, push_fn_ptr, str_ptr, str_len, str_cap, str_owned],
                );
            }
        }
        builder.ins().jump(loop_peek, &[]);
        builder.seal_block(push_elem);
        builder.seal_block(validated_block);

        // Now we can seal loop_peek since all predecessors are known
        builder.seal_block(loop_peek);

        // Success: return 0 (ArrayEnd was already consumed by next_event)
        builder.switch_to_block(success);
        let zero = builder.ins().iconst(types::I32, 0);
        builder.ins().return_(&[zero]);
        builder.seal_block(success);

        // Error: return -10
        builder.switch_to_block(error);
        let err = builder.ins().iconst(types::I32, -10);
        builder.ins().return_(&[err]);
        builder.seal_block(error);

        builder.finalize();
    }

    // Define the function
    module.define_function(func_id, &mut ctx).ok()?;

    Some(func_id)
}

/// Compile a deserializer function for a struct.
///
/// Returns the function ID and a Vec of all nested JITModules that must be kept alive.
fn compile_deserializer(
    module: &mut JITModule,
    shape: &'static Shape,
) -> Option<(FuncId, Vec<JITModule>)> {
    let FacetType::User(UserType::Struct(struct_def)) = &shape.ty else {
        return None;
    };

    // DEBUG: Log the shape we're compiling
    #[cfg(debug_assertions)]
    {
        eprintln!("[JIT DEBUG] ========================================");
        eprintln!(
            "[JIT DEBUG] Compiling deserializer for: {}",
            shape.type_identifier
        );
        eprintln!("[JIT DEBUG] Shape pointer: {:p}", shape);
        eprintln!("[JIT DEBUG] Shape.id (ConstTypeId): {:?}", shape.id);
        eprintln!("[JIT DEBUG] Shape.layout: {:?}", shape.layout);
        eprintln!("[JIT DEBUG] struct_def pointer: {:p}", struct_def);
        eprintln!(
            "[JIT DEBUG] struct_def.fields pointer: {:p}",
            struct_def.fields.as_ptr()
        );
        eprintln!(
            "[JIT DEBUG] struct_def.fields.len() = {}",
            struct_def.fields.len()
        );
        eprintln!("[JIT DEBUG] Fields from Shape (struct_def.fields):");
        for (i, f) in struct_def.fields.iter().enumerate() {
            let field_ptr = f as *const facet_core::Field;
            // Also get the field's inner shape to see if it's pointing to something weird
            let field_shape = f.shape();
            eprintln!(
                "[JIT DEBUG]   [{}] field_ptr={:p}, name='{}', offset={}",
                i, field_ptr, f.name, f.offset
            );
            eprintln!(
                "[JIT DEBUG]       field_shape_ptr={:p}, field_shape_type='{}'",
                field_shape as *const _, field_shape.type_identifier
            );
        }

        // If this is UserSparse, also print what the ACTUAL offsets should be
        if shape.type_identifier == "UserSparse" {
            eprintln!("[JIT DEBUG] *** UserSparse detected - checking actual memory layout ***");
            eprintln!(
                "[JIT DEBUG] sizeof(UserSparse shape) struct_def layout says: {:?}",
                shape.layout
            );
            eprintln!("[JIT DEBUG] struct_def.repr: {:?}", struct_def.repr);
            eprintln!("[JIT DEBUG] struct_def.kind: {:?}", struct_def.kind);
        }
        eprintln!("[JIT DEBUG] ----------------------------------------");
    }

    // Extract field info for code generation
    // Track required fields (non-Option) for bitmask validation
    let mut required_bit_counter = 0usize;
    let fields: Vec<FieldCodegenInfo> = struct_def
        .fields
        .iter()
        .filter_map(|f| {
            let write_kind = WriteKind::from_shape(f.shape())?;
            let is_required = !matches!(write_kind, WriteKind::Option(_));
            let required_bit_index = if is_required {
                let idx = required_bit_counter;
                required_bit_counter += 1;
                Some(idx)
            } else {
                None
            };
            Some(FieldCodegenInfo {
                name: f.name,
                offset: f.offset,
                write_kind,
                required_bit_index,
            })
        })
        .collect();

    // DEBUG: Log the extracted fields
    #[cfg(debug_assertions)]
    {
        eprintln!(
            "[JIT DEBUG] Extracted FieldCodegenInfo ({} fields):",
            fields.len()
        );
        for (i, f) in fields.iter().enumerate() {
            eprintln!(
                "[JIT DEBUG]   [{}] name='{}', offset={}",
                i, f.name, f.offset
            );
        }
        eprintln!("[JIT DEBUG] ========================================");
    }

    // Check if we have too many required fields for the bitmask (u64 can track 0-63, max 64 fields)
    // Note: 64 required fields would need `1u64 << 64` which overflows, so max is 63.
    if required_bit_counter >= 64 {
        jit_debug!(
            "[Tier-2 JIT] Too many required fields ({} >= 64, max 63 for u64 bitmask)",
            required_bit_counter
        );
        return None;
    }

    // Calculate the expected bitmask for all required fields
    let required_fields_mask: u64 = if required_bit_counter > 0 {
        (1u64 << required_bit_counter) - 1
    } else {
        0
    };

    // Pre-compile nested types (structs, Option inner types, Vec element types).
    // We do this before building the parent to avoid any potential issues.
    // Each type gets compiled once and cached.
    // IMPORTANT: We must keep the JITModules alive, so we collect them in all_nested_modules.
    let mut nested_lookup: std::collections::HashMap<*const Shape, *const u8> =
        std::collections::HashMap::new();
    let mut all_nested_modules: Vec<JITModule> = Vec::new();

    // Helper to compile a shape and cache it
    let mut compile_and_cache = |shape: &'static Shape| -> Option<*const u8> {
        #[cfg(debug_assertions)]
        eprintln!(
            "[JIT DEBUG] compile_and_cache called for nested shape: {} at {:p}",
            shape.type_identifier, shape
        );

        let ptr = shape as *const Shape;
        if let std::collections::hash_map::Entry::Vacant(e) = nested_lookup.entry(ptr) {
            #[cfg(debug_assertions)]
            eprintln!("[JIT DEBUG]   -> compiling nested shape (not in cache)");

            let mut nested_builder =
                JITBuilder::new(cranelift_module::default_libcall_names()).ok()?;
            register_helpers(&mut nested_builder);
            let mut nested_module = JITModule::new(nested_builder);
            let (nested_func_id, sub_nested_modules) =
                compile_deserializer(&mut nested_module, shape)?;
            nested_module.finalize_definitions().ok()?;
            let fn_ptr = nested_module.get_finalized_function(nested_func_id);
            e.insert(fn_ptr);
            // Keep the nested module alive by storing it
            all_nested_modules.push(nested_module);
            // Also keep any sub-nested modules (for deeply nested types)
            all_nested_modules.extend(sub_nested_modules);
            Some(fn_ptr)
        } else {
            #[cfg(debug_assertions)]
            eprintln!("[JIT DEBUG]   -> using cached nested shape");
            Some(nested_lookup[&ptr])
        }
    };

    for field in &fields {
        match field.write_kind {
            WriteKind::NestedStruct(nested_shape) => {
                compile_and_cache(nested_shape);
            }
            WriteKind::Option(option_shape) => {
                // Pre-compile the inner type if it's a struct
                if let Def::Option(option_def) = &option_shape.def {
                    let inner_shape = option_def.t;
                    if let FacetType::User(UserType::Struct(_)) = &inner_shape.ty
                        && is_jit_compatible(inner_shape)
                    {
                        compile_and_cache(inner_shape);
                    }
                }
            }
            WriteKind::Vec(vec_shape) => {
                // Pre-compile the element type if it's a struct
                if let Def::List(list_def) = &vec_shape.def {
                    let elem_shape = list_def.t;
                    if let FacetType::User(UserType::Struct(_)) = &elem_shape.ty
                        && is_jit_compatible(elem_shape)
                    {
                        compile_and_cache(elem_shape);
                    }
                }
            }
            _ => {}
        }
    }

    let pointer_type = module.target_config().pointer_type();

    // Function signature: fn(ctx: *mut JitContext, out: *mut T) -> i32
    let mut sig = make_c_sig(module);
    sig.params.push(AbiParam::new(pointer_type)); // ctx
    sig.params.push(AbiParam::new(pointer_type)); // out
    sig.returns.push(AbiParam::new(types::I32)); // result

    // Create unique function name
    static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let func_name = format!(
        "jit_deserialize_{}",
        COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    );

    let func_id = module
        .declare_function(&func_name, Linkage::Local, &sig)
        .ok()?;

    // Declare helper function signatures
    let sig_next_event = {
        let mut s = make_c_sig(module);
        s.params.push(AbiParam::new(pointer_type)); // ctx: *mut JitContext
        s.params.push(AbiParam::new(pointer_type)); // out: *mut RawEvent
        s.returns.push(AbiParam::new(types::I32)); // result
        s
    };

    let sig_peek_event = {
        let mut s = make_c_sig(module);
        s.params.push(AbiParam::new(pointer_type)); // ctx: *mut JitContext
        s.params.push(AbiParam::new(pointer_type)); // out: *mut RawEvent
        s.returns.push(AbiParam::new(types::I32)); // result
        s
    };

    let sig_skip_value = {
        let mut s = make_c_sig(module);
        s.params.push(AbiParam::new(pointer_type)); // parser
        s.returns.push(AbiParam::new(types::I32)); // result
        s
    };

    let sig_field_matches = {
        let mut s = make_c_sig(module);
        s.params.push(AbiParam::new(pointer_type)); // name_ptr
        s.params.push(AbiParam::new(pointer_type)); // name_len
        s.params.push(AbiParam::new(pointer_type)); // expected_ptr
        s.params.push(AbiParam::new(pointer_type)); // expected_len
        s.returns.push(AbiParam::new(types::I32)); // 1 if match, 0 otherwise
        s
    };

    let sig_write_i64 = {
        let mut s = make_c_sig(module);
        s.params.push(AbiParam::new(pointer_type)); // out
        s.params.push(AbiParam::new(pointer_type)); // offset
        s.params.push(AbiParam::new(types::I64)); // value
        s
    };

    let sig_write_u64 = sig_write_i64.clone();

    let sig_write_i8 = {
        let mut s = make_c_sig(module);
        s.params.push(AbiParam::new(pointer_type)); // out
        s.params.push(AbiParam::new(pointer_type)); // offset
        s.params.push(AbiParam::new(types::I8)); // value
        s
    };

    let sig_write_i16 = {
        let mut s = make_c_sig(module);
        s.params.push(AbiParam::new(pointer_type)); // out
        s.params.push(AbiParam::new(pointer_type)); // offset
        s.params.push(AbiParam::new(types::I16)); // value
        s
    };

    let sig_write_i32 = {
        let mut s = make_c_sig(module);
        s.params.push(AbiParam::new(pointer_type)); // out
        s.params.push(AbiParam::new(pointer_type)); // offset
        s.params.push(AbiParam::new(types::I32)); // value
        s
    };

    let sig_write_u8 = sig_write_i8.clone();

    let sig_write_u16 = sig_write_i16.clone();

    let sig_write_u32 = sig_write_i32.clone();

    let sig_write_f64 = {
        let mut s = make_c_sig(module);
        s.params.push(AbiParam::new(pointer_type)); // out
        s.params.push(AbiParam::new(pointer_type)); // offset
        s.params.push(AbiParam::new(types::F64)); // value
        s
    };

    let sig_write_bool = {
        let mut s = make_c_sig(module);
        s.params.push(AbiParam::new(pointer_type)); // out
        s.params.push(AbiParam::new(pointer_type)); // offset
        s.params.push(AbiParam::new(types::I8)); // value (bool as i8)
        s
    };

    let sig_write_string = {
        let mut s = make_c_sig(module);
        s.params.push(AbiParam::new(pointer_type)); // out
        s.params.push(AbiParam::new(pointer_type)); // offset
        s.params.push(AbiParam::new(pointer_type)); // ptr
        s.params.push(AbiParam::new(pointer_type)); // len
        s.params.push(AbiParam::new(pointer_type)); // capacity
        s.params.push(AbiParam::new(types::I8)); // owned (bool as i8)
        s
    };

    let sig_deserialize_nested = {
        let mut s = make_c_sig(module);
        s.params.push(AbiParam::new(pointer_type)); // ctx: *mut JitContext
        s.params.push(AbiParam::new(pointer_type)); // out: *mut u8
        s.params.push(AbiParam::new(pointer_type)); // func_ptr: *const u8
        s.returns.push(AbiParam::new(types::I32)); // result
        s
    };

    let sig_option_init_none = {
        let mut s = make_c_sig(module);
        s.params.push(AbiParam::new(pointer_type)); // out: *mut u8
        s.params.push(AbiParam::new(pointer_type)); // init_none_fn: *const u8
        s
    };

    let sig_option_init_some_from_value = {
        let mut s = make_c_sig(module);
        s.params.push(AbiParam::new(pointer_type)); // out: *mut u8
        s.params.push(AbiParam::new(pointer_type)); // value_ptr: *const u8
        s.params.push(AbiParam::new(pointer_type)); // init_some_fn: *const u8
        s
    };

    let sig_vec_init_with_capacity = {
        let mut s = make_c_sig(module);
        s.params.push(AbiParam::new(pointer_type)); // out: *mut u8
        s.params.push(AbiParam::new(pointer_type)); // capacity: usize
        s.params.push(AbiParam::new(pointer_type)); // init_fn: *const u8
        s
    };

    let sig_vec_push = {
        let mut s = make_c_sig(module);
        s.params.push(AbiParam::new(pointer_type)); // ctx: *mut JitContext
        s.params.push(AbiParam::new(pointer_type)); // vec_ptr: *mut u8
        s.params.push(AbiParam::new(pointer_type)); // push_fn: *const u8
        s.params.push(AbiParam::new(pointer_type)); // item_deserializer: *const u8
        s.returns.push(AbiParam::new(types::I32)); // result
        s
    };

    let sig_deserialize_vec = {
        let mut s = make_c_sig(module);
        s.params.push(AbiParam::new(pointer_type)); // ctx: *mut JitContext
        s.params.push(AbiParam::new(pointer_type)); // out: *mut u8
        s.params.push(AbiParam::new(pointer_type)); // init_fn: *const u8
        s.params.push(AbiParam::new(pointer_type)); // push_fn: *const u8
        s.params.push(AbiParam::new(pointer_type)); // elem_size: usize
        s.params.push(AbiParam::new(pointer_type)); // elem_deserializer: *const u8
        s.params.push(AbiParam::new(types::I8)); // scalar_tag: u8
        s.returns.push(AbiParam::new(types::I32)); // result
        s
    };

    // Simpler signature for recursive list deserialization by shape
    let sig_deserialize_list_by_shape = {
        let mut s = make_c_sig(module);
        s.params.push(AbiParam::new(pointer_type)); // ctx: *mut JitContext
        s.params.push(AbiParam::new(pointer_type)); // out: *mut u8
        s.params.push(AbiParam::new(pointer_type)); // list_shape: *const Shape
        s.params.push(AbiParam::new(pointer_type)); // elem_struct_deserializer: *const u8 (or null)
        s.returns.push(AbiParam::new(types::I32)); // result
        s
    };

    // Declare all helper functions
    let field_matches_id = module
        .declare_function("jit_field_matches", Linkage::Import, &sig_field_matches)
        .ok()?;
    let write_i64_id = module
        .declare_function("jit_write_i64", Linkage::Import, &sig_write_i64)
        .ok()?;
    let write_u64_id = module
        .declare_function("jit_write_u64", Linkage::Import, &sig_write_u64)
        .ok()?;
    let write_i8_id = module
        .declare_function("jit_write_i8", Linkage::Import, &sig_write_i8)
        .ok()?;
    let write_i16_id = module
        .declare_function("jit_write_i16", Linkage::Import, &sig_write_i16)
        .ok()?;
    let write_i32_id = module
        .declare_function("jit_write_i32", Linkage::Import, &sig_write_i32)
        .ok()?;
    let write_u8_id = module
        .declare_function("jit_write_u8", Linkage::Import, &sig_write_u8)
        .ok()?;
    let write_u16_id = module
        .declare_function("jit_write_u16", Linkage::Import, &sig_write_u16)
        .ok()?;
    let write_u32_id = module
        .declare_function("jit_write_u32", Linkage::Import, &sig_write_u32)
        .ok()?;
    let write_f64_id = module
        .declare_function("jit_write_f64", Linkage::Import, &sig_write_f64)
        .ok()?;
    let write_bool_id = module
        .declare_function("jit_write_bool", Linkage::Import, &sig_write_bool)
        .ok()?;
    let write_string_id = module
        .declare_function("jit_write_string", Linkage::Import, &sig_write_string)
        .ok()?;
    let deserialize_nested_id = module
        .declare_function(
            "jit_deserialize_nested",
            Linkage::Import,
            &sig_deserialize_nested,
        )
        .ok()?;
    let peek_event_id = module
        .declare_function("jit_peek_event", Linkage::Import, &sig_peek_event)
        .ok()?;
    let next_event_id = module
        .declare_function("jit_next_event", Linkage::Import, &sig_next_event)
        .ok()?;
    let option_init_none_id = module
        .declare_function(
            "jit_option_init_none",
            Linkage::Import,
            &sig_option_init_none,
        )
        .ok()?;
    let option_init_some_from_value_id = module
        .declare_function(
            "jit_option_init_some_from_value",
            Linkage::Import,
            &sig_option_init_some_from_value,
        )
        .ok()?;
    let vec_init_with_capacity_id = module
        .declare_function(
            "jit_vec_init_with_capacity",
            Linkage::Import,
            &sig_vec_init_with_capacity,
        )
        .ok()?;
    let vec_push_id = module
        .declare_function("jit_vec_push", Linkage::Import, &sig_vec_push)
        .ok()?;
    let deserialize_vec_id = module
        .declare_function("jit_deserialize_vec", Linkage::Import, &sig_deserialize_vec)
        .ok()?;
    let deserialize_list_by_shape_id = module
        .declare_function(
            "jit_deserialize_list_by_shape",
            Linkage::Import,
            &sig_deserialize_list_by_shape,
        )
        .ok()?;

    let mut ctx = module.make_context();
    ctx.func.signature = sig;

    // Build the function body
    let mut builder_ctx = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut builder_ctx);

        // Import helper functions
        let field_matches_ref = module.declare_func_in_func(field_matches_id, builder.func);
        let write_i64_ref = module.declare_func_in_func(write_i64_id, builder.func);
        let write_u64_ref = module.declare_func_in_func(write_u64_id, builder.func);
        let write_i8_ref = module.declare_func_in_func(write_i8_id, builder.func);
        let write_i16_ref = module.declare_func_in_func(write_i16_id, builder.func);
        let write_i32_ref = module.declare_func_in_func(write_i32_id, builder.func);
        let write_u8_ref = module.declare_func_in_func(write_u8_id, builder.func);
        let write_u16_ref = module.declare_func_in_func(write_u16_id, builder.func);
        let write_u32_ref = module.declare_func_in_func(write_u32_id, builder.func);
        let write_f64_ref = module.declare_func_in_func(write_f64_id, builder.func);
        let write_bool_ref = module.declare_func_in_func(write_bool_id, builder.func);
        let write_string_ref = module.declare_func_in_func(write_string_id, builder.func);
        let deserialize_nested_ref =
            module.declare_func_in_func(deserialize_nested_id, builder.func);
        let peek_event_ref = module.declare_func_in_func(peek_event_id, builder.func);
        let next_event_ref = module.declare_func_in_func(next_event_id, builder.func);
        let option_init_none_ref = module.declare_func_in_func(option_init_none_id, builder.func);
        let option_init_some_from_value_ref =
            module.declare_func_in_func(option_init_some_from_value_id, builder.func);
        let _vec_init_with_capacity_ref =
            module.declare_func_in_func(vec_init_with_capacity_id, builder.func);
        let _vec_push_ref = module.declare_func_in_func(vec_push_id, builder.func);
        let _deserialize_vec_ref = module.declare_func_in_func(deserialize_vec_id, builder.func);
        let deserialize_list_by_shape_ref =
            module.declare_func_in_func(deserialize_list_by_shape_id, builder.func);

        // Create entry block
        let entry_block = builder.create_block();
        builder.append_block_params_for_function_params(entry_block);
        builder.switch_to_block(entry_block);
        builder.seal_block(entry_block);

        // Get parameters
        let ctx_ptr = builder.block_params(entry_block)[0];
        let out_ptr = builder.block_params(entry_block)[1];

        // Create variable to track which required fields have been set (bitmask)
        let required_fields_seen = builder.declare_var(types::I64);
        let zero_i64 = builder.ins().iconst(types::I64, 0);
        builder.def_var(required_fields_seen, zero_i64);

        // Allocate stack slot for RawEvent
        let raw_event_slot = builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            helpers::RAW_EVENT_SIZE as u32,
            8, // alignment
        ));
        let raw_event_ptr = builder.ins().stack_addr(pointer_type, raw_event_slot, 0);

        // Load parser pointer from ctx
        let parser_ptr = builder.ins().load(
            pointer_type,
            MemFlags::trusted(),
            ctx_ptr,
            helpers::JIT_CONTEXT_PARSER_OFFSET as i32,
        );

        // Load vtable pointer from ctx
        let vtable_ptr = builder.ins().load(
            pointer_type,
            MemFlags::trusted(),
            ctx_ptr,
            helpers::JIT_CONTEXT_VTABLE_OFFSET as i32,
        );

        // Load skip_value function pointer from vtable
        let skip_value_fn = builder.ins().load(
            pointer_type,
            MemFlags::trusted(),
            vtable_ptr,
            helpers::VTABLE_SKIP_VALUE_OFFSET as i32,
        );

        // Create blocks
        let error_block = builder.create_block();
        let success_block = builder.create_block();
        let field_loop = builder.create_block();

        // Import the signature for indirect calls (skip_value only)
        let sig_skip_value_ref = builder.import_signature(sig_skip_value.clone());

        // Call jit_next_event to get StructStart (uses JitContext for peek buffer)
        let call_result = builder
            .ins()
            .call(next_event_ref, &[ctx_ptr, raw_event_ptr]);
        let result = builder.inst_results(call_result)[0];

        // Check for error
        let is_error = builder.ins().icmp_imm(IntCC::SignedLessThan, result, 0);
        let check_struct_start = builder.create_block();
        builder
            .ins()
            .brif(is_error, error_block, &[], check_struct_start, &[]);

        builder.switch_to_block(check_struct_start);
        // Load tag and check it's StructStart (0)
        let tag = builder.ins().load(
            types::I8,
            MemFlags::trusted(),
            raw_event_ptr,
            helpers::RAW_EVENT_TAG_OFFSET as i32,
        );
        let is_struct_start = builder.ins().icmp_imm(IntCC::Equal, tag, 0); // EventTag::StructStart = 0

        // Create block to initialize Option fields to None before parsing
        let init_options_block = builder.create_block();
        builder
            .ins()
            .brif(is_struct_start, init_options_block, &[], error_block, &[]);

        // Initialize all Option fields to None before deserialization
        // This ensures missing Option fields are properly initialized (not UB)
        builder.switch_to_block(init_options_block);
        for field in &fields {
            if let WriteKind::Option(option_shape) = field.write_kind {
                let Def::Option(option_def) = &option_shape.def else {
                    continue;
                };
                let init_none_fn_ptr = option_def.vtable.init_none as *const u8;
                let init_none_fn_val = builder.ins().iconst(pointer_type, init_none_fn_ptr as i64);
                let offset_val = builder.ins().iconst(pointer_type, field.offset as i64);
                let field_ptr = builder.ins().iadd(out_ptr, offset_val);
                builder
                    .ins()
                    .call(option_init_none_ref, &[field_ptr, init_none_fn_val]);
            }
        }
        builder.ins().jump(field_loop, &[]);
        builder.seal_block(init_options_block);

        // Field loop
        builder.switch_to_block(field_loop);

        // Call jit_next_event (uses JitContext for peek buffer)
        let call_result = builder
            .ins()
            .call(next_event_ref, &[ctx_ptr, raw_event_ptr]);
        let result = builder.inst_results(call_result)[0];

        // Check for error
        let is_error = builder.ins().icmp_imm(IntCC::SignedLessThan, result, 0);
        let check_event_tag = builder.create_block();
        builder
            .ins()
            .brif(is_error, error_block, &[], check_event_tag, &[]);

        builder.switch_to_block(check_event_tag);
        // Load tag
        let tag = builder.ins().load(
            types::I8,
            MemFlags::trusted(),
            raw_event_ptr,
            helpers::RAW_EVENT_TAG_OFFSET as i32,
        );

        // Check for StructEnd (1)
        let is_struct_end = builder.ins().icmp_imm(IntCC::Equal, tag, 1); // EventTag::StructEnd = 1
        let check_field_key = builder.create_block();
        builder
            .ins()
            .brif(is_struct_end, success_block, &[], check_field_key, &[]);

        builder.switch_to_block(check_field_key);
        // Check for FieldKey (4)
        let is_field_key = builder.ins().icmp_imm(IntCC::Equal, tag, 4); // EventTag::FieldKey = 4
        let process_field = builder.create_block();
        builder
            .ins()
            .brif(is_field_key, process_field, &[], error_block, &[]);

        builder.switch_to_block(process_field);

        // Load field name ptr and len from payload
        let payload_ptr = builder
            .ins()
            .iadd_imm(raw_event_ptr, helpers::RAW_EVENT_PAYLOAD_OFFSET as i64);
        let name_ptr = builder.ins().load(
            pointer_type,
            MemFlags::trusted(),
            payload_ptr,
            helpers::FIELD_NAME_PTR_OFFSET as i32,
        );
        let name_len = builder.ins().load(
            pointer_type,
            MemFlags::trusted(),
            payload_ptr,
            helpers::FIELD_NAME_LEN_OFFSET as i32,
        );

        // Create blocks for each field and a default (skip) block
        let field_blocks: Vec<Block> = fields.iter().map(|_| builder.create_block()).collect();
        let skip_field_block = builder.create_block();
        let after_field = builder.create_block();

        // Create "set bit and continue" blocks for each required field
        // These blocks OR in the required bit and then jump to after_field
        let set_bit_blocks: Vec<Option<Block>> = fields
            .iter()
            .map(|f| {
                if f.required_bit_index.is_some() {
                    Some(builder.create_block())
                } else {
                    None
                }
            })
            .collect();

        // Track compare blocks for sealing
        let mut compare_blocks: Vec<Block> = Vec::new();

        // Linear scan: compare field name against each expected field
        for (i, field) in fields.iter().enumerate() {
            let next_compare = if i + 1 < fields.len() {
                let block = builder.create_block();
                compare_blocks.push(block);
                block
            } else {
                skip_field_block
            };

            // Embed field name pointer as a constant (the string is 'static)
            let expected_ptr = builder
                .ins()
                .iconst(pointer_type, field.name.as_ptr() as i64);
            let expected_len = builder.ins().iconst(pointer_type, field.name.len() as i64);

            // Call jit_field_matches
            let call_result = builder.ins().call(
                field_matches_ref,
                &[name_ptr, name_len, expected_ptr, expected_len],
            );
            let matches = builder.inst_results(call_result)[0];

            // If matches, jump to field block; otherwise continue to next compare
            let is_match = builder.ins().icmp_imm(IntCC::NotEqual, matches, 0);
            builder
                .ins()
                .brif(is_match, field_blocks[i], &[], next_compare, &[]);

            if i + 1 < fields.len() {
                builder.switch_to_block(next_compare);
            }
        }

        // Skip field block: skip unknown field value
        builder.switch_to_block(skip_field_block);
        // Call skip_value to properly skip the entire value (including nested objects/arrays)
        let call_result =
            builder
                .ins()
                .call_indirect(sig_skip_value_ref, skip_value_fn, &[parser_ptr]);
        let result = builder.inst_results(call_result)[0];
        let is_error = builder.ins().icmp_imm(IntCC::SignedLessThan, result, 0);
        builder
            .ins()
            .brif(is_error, error_block, &[], after_field, &[]);

        // Generate field parsing blocks
        for (i, field) in fields.iter().enumerate() {
            builder.switch_to_block(field_blocks[i]);

            let offset_val = builder.ins().iconst(pointer_type, field.offset as i64);

            #[cfg(debug_assertions)]
            eprintln!(
                "[JIT COMPILE] Field '{}' at offset {}",
                field.name, field.offset
            );

            // Determine the target block after successfully writing this field
            // For required fields, go through set_bit_block to mark the field as seen
            // For optional fields, go directly to after_field
            let continue_target = set_bit_blocks[i].unwrap_or(after_field);

            // Handle field based on type
            match field.write_kind {
                WriteKind::NestedStruct(nested_shape) => {
                    // For nested structs, DON'T call next_event first!
                    // The nested deserializer will consume the StructStart itself.
                    let func_ptr = nested_lookup[&(nested_shape as *const Shape)];
                    let func_ptr_val = builder.ins().iconst(pointer_type, func_ptr as i64);

                    // Calculate the nested struct's output pointer
                    let nested_out_ptr = builder.ins().iadd(out_ptr, offset_val);

                    // Call jit_deserialize_nested(ctx_ptr, nested_out_ptr, func_ptr)
                    let call_result = builder.ins().call(
                        deserialize_nested_ref,
                        &[ctx_ptr, nested_out_ptr, func_ptr_val],
                    );
                    let nested_result = builder.inst_results(call_result)[0];

                    // Check if nested deserialization failed
                    let nested_is_error =
                        builder
                            .ins()
                            .icmp_imm(IntCC::SignedLessThan, nested_result, 0);
                    builder
                        .ins()
                        .brif(nested_is_error, error_block, &[], continue_target, &[]);
                }
                WriteKind::Option(option_shape) => {
                    // Option field: peek to check for Null, then consume and handle
                    // Call jit_peek_event(ctx, raw_event_ptr)
                    let call_result = builder
                        .ins()
                        .call(peek_event_ref, &[ctx_ptr, raw_event_ptr]);
                    let result = builder.inst_results(call_result)[0];

                    // Check for error
                    let is_error = builder.ins().icmp_imm(IntCC::SignedLessThan, result, 0);
                    let check_null_block = builder.create_block();
                    builder
                        .ins()
                        .brif(is_error, error_block, &[], check_null_block, &[]);

                    builder.switch_to_block(check_null_block);

                    // Check if peeked value is Null (scalar_tag == ScalarTag::Null = 1)
                    let scalar_tag = builder.ins().load(
                        types::I8,
                        MemFlags::trusted(),
                        raw_event_ptr,
                        1, // scalar_tag offset
                    );
                    let is_null = builder.ins().icmp_imm(IntCC::Equal, scalar_tag, 1);

                    let handle_none_block = builder.create_block();
                    let handle_some_block = builder.create_block();
                    builder
                        .ins()
                        .brif(is_null, handle_none_block, &[], handle_some_block, &[]);

                    // Handle None case: consume Null event and init to None
                    builder.switch_to_block(handle_none_block);
                    // Consume the peeked Null event
                    let _consume_result = builder
                        .ins()
                        .call(next_event_ref, &[ctx_ptr, raw_event_ptr]);

                    // Get Option vtable
                    let Def::Option(option_def) = &option_shape.def else {
                        unreachable!();
                    };
                    let init_none_fn_ptr = option_def.vtable.init_none as *const u8;
                    let init_none_fn_val =
                        builder.ins().iconst(pointer_type, init_none_fn_ptr as i64);

                    // Calculate field pointer
                    let field_ptr = builder.ins().iadd(out_ptr, offset_val);

                    // Call jit_option_init_none(field_ptr, init_none_fn)
                    builder
                        .ins()
                        .call(option_init_none_ref, &[field_ptr, init_none_fn_val]);

                    builder.ins().jump(after_field, &[]);

                    // Handle Some case: deserialize inner value, init to Some
                    builder.switch_to_block(handle_some_block);

                    // Get Option info
                    let Def::Option(option_def) = &option_shape.def else {
                        unreachable!();
                    };
                    let inner_shape = option_def.t;

                    // Determine how to deserialize the inner value
                    let inner_write_kind = WriteKind::from_shape(inner_shape);
                    if inner_write_kind.is_none() {
                        // Inner type not supported in Tier-2, bail
                        builder.ins().jump(error_block, &[]);
                        builder.seal_block(check_null_block);
                        builder.seal_block(handle_some_block);
                        continue;
                    }
                    let inner_write_kind = inner_write_kind.unwrap();

                    // Allocate stack slot for the inner value (max 256 bytes)
                    let value_slot = builder.create_sized_stack_slot(StackSlotData::new(
                        StackSlotKind::ExplicitSlot,
                        256,
                        8,
                    ));
                    let value_ptr = builder.ins().stack_addr(pointer_type, value_slot, 0);
                    let value_offset = builder.ins().iconst(pointer_type, 0);

                    // Zero out the stack slot to avoid uninitialized data
                    let zero_i64 = builder.ins().iconst(types::I64, 0);
                    for offset in (0..256).step_by(8) {
                        builder
                            .ins()
                            .store(MemFlags::trusted(), zero_i64, value_ptr, offset);
                    }

                    // Consume the peeked event and deserialize the value
                    // Use the same deserialization pattern as regular fields
                    let call_result = builder
                        .ins()
                        .call(next_event_ref, &[ctx_ptr, raw_event_ptr]);
                    let result = builder.inst_results(call_result)[0];

                    // Check for error
                    let is_error = builder.ins().icmp_imm(IntCC::SignedLessThan, result, 0);
                    let write_value_block = builder.create_block();
                    builder
                        .ins()
                        .brif(is_error, error_block, &[], write_value_block, &[]);

                    builder.switch_to_block(write_value_block);

                    // Validate scalar tag before reading payload to prevent type confusion
                    let validated_block = if let Some((primary_tag, alt_tag)) =
                        inner_write_kind.expected_scalar_tags()
                    {
                        let actual_tag = builder.ins().load(
                            types::I8,
                            MemFlags::trusted(),
                            raw_event_ptr,
                            helpers::RAW_EVENT_SCALAR_TAG_OFFSET as i32,
                        );
                        let primary_matches =
                            builder
                                .ins()
                                .icmp_imm(IntCC::Equal, actual_tag, primary_tag as i64);

                        let validated_block = builder.create_block();
                        if let Some(alt) = alt_tag {
                            // For integers, accept both I64 and U64
                            let check_alt_block = builder.create_block();
                            builder.ins().brif(
                                primary_matches,
                                validated_block,
                                &[],
                                check_alt_block,
                                &[],
                            );
                            builder.switch_to_block(check_alt_block);
                            let alt_matches =
                                builder.ins().icmp_imm(IntCC::Equal, actual_tag, alt as i64);
                            builder
                                .ins()
                                .brif(alt_matches, validated_block, &[], error_block, &[]);
                            builder.seal_block(check_alt_block);
                        } else {
                            builder.ins().brif(
                                primary_matches,
                                validated_block,
                                &[],
                                error_block,
                                &[],
                            );
                        }
                        builder.switch_to_block(validated_block);
                        Some(validated_block)
                    } else {
                        None
                    };

                    // Get the payload pointer
                    let payload_ptr = builder
                        .ins()
                        .iadd_imm(raw_event_ptr, helpers::RAW_EVENT_PAYLOAD_OFFSET as i64);

                    // Deserialize based on inner type and write to stack slot
                    match inner_write_kind {
                        WriteKind::I8 => {
                            let value =
                                builder
                                    .ins()
                                    .load(types::I8, MemFlags::trusted(), payload_ptr, 0);
                            builder
                                .ins()
                                .call(write_i8_ref, &[value_ptr, value_offset, value]);
                        }
                        WriteKind::I16 => {
                            let value =
                                builder
                                    .ins()
                                    .load(types::I64, MemFlags::trusted(), payload_ptr, 0);
                            let value_i16 = builder.ins().ireduce(types::I16, value);
                            builder
                                .ins()
                                .call(write_i16_ref, &[value_ptr, value_offset, value_i16]);
                        }
                        WriteKind::I32 => {
                            let value =
                                builder
                                    .ins()
                                    .load(types::I64, MemFlags::trusted(), payload_ptr, 0);
                            let value_i32 = builder.ins().ireduce(types::I32, value);
                            builder
                                .ins()
                                .call(write_i32_ref, &[value_ptr, value_offset, value_i32]);
                        }
                        WriteKind::I64 => {
                            let value =
                                builder
                                    .ins()
                                    .load(types::I64, MemFlags::trusted(), payload_ptr, 0);
                            builder
                                .ins()
                                .call(write_i64_ref, &[value_ptr, value_offset, value]);
                        }
                        WriteKind::U8 => {
                            let value =
                                builder
                                    .ins()
                                    .load(types::I8, MemFlags::trusted(), payload_ptr, 0);
                            builder
                                .ins()
                                .call(write_u8_ref, &[value_ptr, value_offset, value]);
                        }
                        WriteKind::U16 => {
                            let value =
                                builder
                                    .ins()
                                    .load(types::I64, MemFlags::trusted(), payload_ptr, 0);
                            let value_u16 = builder.ins().ireduce(types::I16, value);
                            builder
                                .ins()
                                .call(write_u16_ref, &[value_ptr, value_offset, value_u16]);
                        }
                        WriteKind::U32 => {
                            let value =
                                builder
                                    .ins()
                                    .load(types::I64, MemFlags::trusted(), payload_ptr, 0);
                            let value_u32 = builder.ins().ireduce(types::I32, value);
                            builder
                                .ins()
                                .call(write_u32_ref, &[value_ptr, value_offset, value_u32]);
                        }
                        WriteKind::U64 => {
                            let value =
                                builder
                                    .ins()
                                    .load(types::I64, MemFlags::trusted(), payload_ptr, 0);
                            builder
                                .ins()
                                .call(write_u64_ref, &[value_ptr, value_offset, value]);
                        }
                        WriteKind::F64 | WriteKind::F32 => {
                            let value =
                                builder
                                    .ins()
                                    .load(types::F64, MemFlags::trusted(), payload_ptr, 0);
                            builder
                                .ins()
                                .call(write_f64_ref, &[value_ptr, value_offset, value]);
                        }
                        WriteKind::Bool => {
                            let value =
                                builder
                                    .ins()
                                    .load(types::I8, MemFlags::trusted(), payload_ptr, 0);
                            builder
                                .ins()
                                .call(write_bool_ref, &[value_ptr, value_offset, value]);
                        }
                        WriteKind::String => {
                            let str_ptr = builder.ins().load(
                                pointer_type,
                                MemFlags::trusted(),
                                payload_ptr,
                                0,
                            );
                            let str_len = builder.ins().load(
                                pointer_type,
                                MemFlags::trusted(),
                                payload_ptr,
                                8,
                            );
                            let str_capacity = builder.ins().load(
                                pointer_type,
                                MemFlags::trusted(),
                                payload_ptr,
                                16,
                            );
                            let str_owned =
                                builder
                                    .ins()
                                    .load(types::I8, MemFlags::trusted(), payload_ptr, 24);
                            builder.ins().call(
                                write_string_ref,
                                &[
                                    value_ptr,
                                    value_offset,
                                    str_ptr,
                                    str_len,
                                    str_capacity,
                                    str_owned,
                                ],
                            );
                        }
                        WriteKind::NestedStruct(_) | WriteKind::Option(_) | WriteKind::Vec(_) => {
                            // Nested structs, Options, and Vecs in Options not yet supported
                            // TODO: implement nested struct deserialization for Option<NestedStruct>
                            builder.ins().jump(error_block, &[]);
                            builder.seal_block(check_null_block);
                            builder.seal_block(handle_some_block);
                            builder.seal_block(write_value_block);
                            if let Some(vb) = validated_block {
                                builder.seal_block(vb);
                            }
                            continue;
                        }
                    }

                    // Get init_some function
                    let init_some_fn_ptr = option_def.vtable.init_some as *const u8;
                    let init_some_fn_val =
                        builder.ins().iconst(pointer_type, init_some_fn_ptr as i64);

                    // Calculate field pointer
                    let field_ptr = builder.ins().iadd(out_ptr, offset_val);

                    // Call jit_option_init_some_from_value(field_ptr, value_ptr, init_some_fn)
                    builder.ins().call(
                        option_init_some_from_value_ref,
                        &[field_ptr, value_ptr, init_some_fn_val],
                    );

                    builder.ins().jump(after_field, &[]);
                    builder.seal_block(check_null_block);
                    builder.seal_block(handle_none_block);
                    builder.seal_block(handle_some_block);
                    builder.seal_block(write_value_block);
                    if let Some(vb) = validated_block {
                        builder.seal_block(vb);
                    }
                }
                WriteKind::Vec(vec_shape) => {
                    // Vec field: call jit_deserialize_list_by_shape which handles
                    // nested Vecs recursively
                    let vec_shape_ptr = vec_shape as *const Shape;
                    let vec_shape_val = builder.ins().iconst(pointer_type, vec_shape_ptr as i64);

                    // Calculate the field pointer
                    let field_ptr = builder.ins().iadd(out_ptr, offset_val);

                    // Check if Vec element is a struct and get its deserializer
                    let elem_deserializer_val = if let Def::List(list_def) = &vec_shape.def {
                        let elem_shape = list_def.t;
                        if let FacetType::User(UserType::Struct(_)) = &elem_shape.ty {
                            // Get the pre-compiled deserializer for this struct type
                            if let Some(&func_ptr) =
                                nested_lookup.get(&(elem_shape as *const Shape))
                            {
                                builder.ins().iconst(pointer_type, func_ptr as i64)
                            } else {
                                // Struct element but no deserializer - will fail in helper
                                builder.ins().iconst(pointer_type, 0)
                            }
                        } else {
                            // Not a struct element
                            builder.ins().iconst(pointer_type, 0)
                        }
                    } else {
                        builder.ins().iconst(pointer_type, 0)
                    };

                    // Call jit_deserialize_list_by_shape(ctx_ptr, field_ptr, vec_shape, elem_deserializer)
                    let call_result = builder.ins().call(
                        deserialize_list_by_shape_ref,
                        &[ctx_ptr, field_ptr, vec_shape_val, elem_deserializer_val],
                    );
                    let vec_result = builder.inst_results(call_result)[0];

                    // Check if deserialization failed
                    let vec_is_error = builder.ins().icmp_imm(IntCC::SignedLessThan, vec_result, 0);
                    builder
                        .ins()
                        .brif(vec_is_error, error_block, &[], continue_target, &[]);
                }
                _ => {
                    // For scalars/strings, call next_event to get the value
                    // Call jit_next_event (uses JitContext for peek buffer)
                    let call_result = builder
                        .ins()
                        .call(next_event_ref, &[ctx_ptr, raw_event_ptr]);
                    let result = builder.inst_results(call_result)[0];

                    // Check for error
                    let is_error = builder.ins().icmp_imm(IntCC::SignedLessThan, result, 0);
                    let write_value_block = builder.create_block();
                    builder
                        .ins()
                        .brif(is_error, error_block, &[], write_value_block, &[]);

                    builder.switch_to_block(write_value_block);

                    // Validate scalar tag before reading payload to prevent type confusion
                    // (e.g., reading a string pointer as a u64 value)
                    let validated_block = if let Some((primary_tag, alt_tag)) =
                        field.write_kind.expected_scalar_tags()
                    {
                        let actual_tag = builder.ins().load(
                            types::I8,
                            MemFlags::trusted(),
                            raw_event_ptr,
                            helpers::RAW_EVENT_SCALAR_TAG_OFFSET as i32,
                        );
                        let primary_matches =
                            builder
                                .ins()
                                .icmp_imm(IntCC::Equal, actual_tag, primary_tag as i64);

                        let validated_block = builder.create_block();
                        if let Some(alt) = alt_tag {
                            // For integers, accept both I64 and U64
                            let check_alt_block = builder.create_block();
                            builder.ins().brif(
                                primary_matches,
                                validated_block,
                                &[],
                                check_alt_block,
                                &[],
                            );
                            builder.switch_to_block(check_alt_block);
                            let alt_matches =
                                builder.ins().icmp_imm(IntCC::Equal, actual_tag, alt as i64);
                            builder
                                .ins()
                                .brif(alt_matches, validated_block, &[], error_block, &[]);
                            builder.seal_block(check_alt_block);
                        } else {
                            builder.ins().brif(
                                primary_matches,
                                validated_block,
                                &[],
                                error_block,
                                &[],
                            );
                        }
                        builder.switch_to_block(validated_block);
                        Some(validated_block)
                    } else {
                        None
                    };

                    // Get the payload pointer
                    let payload_ptr = builder
                        .ins()
                        .iadd_imm(raw_event_ptr, helpers::RAW_EVENT_PAYLOAD_OFFSET as i64);

                    // Write the value based on field type
                    match field.write_kind {
                        WriteKind::I8 => {
                            let value =
                                builder
                                    .ins()
                                    .load(types::I8, MemFlags::trusted(), payload_ptr, 0);
                            builder
                                .ins()
                                .call(write_i8_ref, &[out_ptr, offset_val, value]);
                        }
                        WriteKind::I16 => {
                            let value =
                                builder
                                    .ins()
                                    .load(types::I64, MemFlags::trusted(), payload_ptr, 0);
                            let value_i16 = builder.ins().ireduce(types::I16, value);
                            builder
                                .ins()
                                .call(write_i16_ref, &[out_ptr, offset_val, value_i16]);
                        }
                        WriteKind::I32 => {
                            let value =
                                builder
                                    .ins()
                                    .load(types::I64, MemFlags::trusted(), payload_ptr, 0);
                            let value_i32 = builder.ins().ireduce(types::I32, value);
                            builder
                                .ins()
                                .call(write_i32_ref, &[out_ptr, offset_val, value_i32]);
                        }
                        WriteKind::I64 => {
                            let value =
                                builder
                                    .ins()
                                    .load(types::I64, MemFlags::trusted(), payload_ptr, 0);
                            builder
                                .ins()
                                .call(write_i64_ref, &[out_ptr, offset_val, value]);
                        }
                        WriteKind::U8 => {
                            let value =
                                builder
                                    .ins()
                                    .load(types::I8, MemFlags::trusted(), payload_ptr, 0);
                            builder
                                .ins()
                                .call(write_u8_ref, &[out_ptr, offset_val, value]);
                        }
                        WriteKind::U16 => {
                            let value =
                                builder
                                    .ins()
                                    .load(types::I64, MemFlags::trusted(), payload_ptr, 0);
                            let value_u16 = builder.ins().ireduce(types::I16, value);
                            builder
                                .ins()
                                .call(write_u16_ref, &[out_ptr, offset_val, value_u16]);
                        }
                        WriteKind::U32 => {
                            let value =
                                builder
                                    .ins()
                                    .load(types::I64, MemFlags::trusted(), payload_ptr, 0);
                            let value_u32 = builder.ins().ireduce(types::I32, value);
                            builder
                                .ins()
                                .call(write_u32_ref, &[out_ptr, offset_val, value_u32]);
                        }
                        WriteKind::U64 => {
                            let value =
                                builder
                                    .ins()
                                    .load(types::I64, MemFlags::trusted(), payload_ptr, 0);
                            builder
                                .ins()
                                .call(write_u64_ref, &[out_ptr, offset_val, value]);
                        }
                        WriteKind::F64 | WriteKind::F32 => {
                            // Load f64 value from scalar payload
                            let value =
                                builder
                                    .ins()
                                    .load(types::F64, MemFlags::trusted(), payload_ptr, 0);
                            builder
                                .ins()
                                .call(write_f64_ref, &[out_ptr, offset_val, value]);
                        }
                        WriteKind::Bool => {
                            // Load bool value from scalar payload (as u8)
                            let value =
                                builder
                                    .ins()
                                    .load(types::I8, MemFlags::trusted(), payload_ptr, 0);
                            builder
                                .ins()
                                .call(write_bool_ref, &[out_ptr, offset_val, value]);
                        }
                        WriteKind::String => {
                            // Load string payload: ptr, len, capacity, owned
                            // StringPayload layout: ptr (0), len (8), capacity (16), owned (24)
                            let str_ptr = builder.ins().load(
                                pointer_type,
                                MemFlags::trusted(),
                                payload_ptr,
                                0,
                            );
                            let str_len = builder.ins().load(
                                pointer_type,
                                MemFlags::trusted(),
                                payload_ptr,
                                8, // offset to len field
                            );
                            let str_capacity = builder.ins().load(
                                pointer_type,
                                MemFlags::trusted(),
                                payload_ptr,
                                16, // offset to capacity field
                            );
                            let str_owned = builder.ins().load(
                                types::I8,
                                MemFlags::trusted(),
                                payload_ptr,
                                24, // offset to owned field
                            );
                            builder.ins().call(
                                write_string_ref,
                                &[
                                    out_ptr,
                                    offset_val,
                                    str_ptr,
                                    str_len,
                                    str_capacity,
                                    str_owned,
                                ],
                            );
                        }
                        WriteKind::NestedStruct(_) => {
                            unreachable!("Nested struct should be handled in outer match");
                        }
                        WriteKind::Option(_) => {
                            unreachable!("Option should be handled in outer match");
                        }
                        WriteKind::Vec(_) => {
                            unreachable!("Vec should be handled in outer match");
                        }
                    }

                    builder.ins().jump(continue_target, &[]);
                    builder.seal_block(write_value_block);
                    if let Some(vb) = validated_block {
                        builder.seal_block(vb);
                    }
                }
            }
        }

        // Generate set_bit_blocks: OR in the required bit and jump to after_field
        for (i, field) in fields.iter().enumerate() {
            if let Some(bit_index) = field.required_bit_index {
                let set_bit_block = set_bit_blocks[i].unwrap();
                builder.switch_to_block(set_bit_block);

                // Load current bitmask
                let current = builder.use_var(required_fields_seen);

                // OR in the bit for this field
                let bit = builder.ins().iconst(types::I64, 1i64 << bit_index);
                let updated = builder.ins().bor(current, bit);

                // Store back
                builder.def_var(required_fields_seen, updated);

                // Jump to after_field
                builder.ins().jump(after_field, &[]);
            }
        }

        // After field: loop back
        builder.switch_to_block(after_field);
        builder.ins().jump(field_loop, &[]);

        // Success block: validate that all required fields have been seen
        builder.switch_to_block(success_block);

        if required_fields_mask != 0 {
            // Check if all required fields were provided
            let seen = builder.use_var(required_fields_seen);
            let expected = builder
                .ins()
                .iconst(types::I64, required_fields_mask as i64);
            let all_seen = builder.ins().icmp(IntCC::Equal, seen, expected);

            // Create a block for the final success return
            let return_success = builder.create_block();
            let missing_field_error = builder.create_block();

            builder
                .ins()
                .brif(all_seen, return_success, &[], missing_field_error, &[]);

            // Missing field error: store seen mask and return ERR_MISSING_REQUIRED_FIELD
            builder.switch_to_block(missing_field_error);
            // Store the fields_seen mask to ctx for cleanup of partially-initialized struct
            let seen_for_error = builder.use_var(required_fields_seen);
            builder.ins().store(
                MemFlags::trusted(),
                seen_for_error,
                ctx_ptr,
                helpers::JIT_CONTEXT_FIELDS_SEEN_OFFSET as i32,
            );
            let err_missing = builder
                .ins()
                .iconst(types::I32, helpers::ERR_MISSING_REQUIRED_FIELD as i64);
            builder.ins().return_(&[err_missing]);

            // Final success return
            builder.switch_to_block(return_success);
            let zero = builder.ins().iconst(types::I32, 0);
            builder.ins().return_(&[zero]);

            builder.seal_block(return_success);
            builder.seal_block(missing_field_error);
        } else {
            // No required fields - just return success
            let zero = builder.ins().iconst(types::I32, 0);
            builder.ins().return_(&[zero]);
        }

        // Error block: store seen mask for cleanup and return error
        builder.switch_to_block(error_block);
        // Store the fields_seen mask to ctx for cleanup of partially-initialized struct
        let seen_for_cleanup = builder.use_var(required_fields_seen);
        builder.ins().store(
            MemFlags::trusted(),
            seen_for_cleanup,
            ctx_ptr,
            helpers::JIT_CONTEXT_FIELDS_SEEN_OFFSET as i32,
        );
        let err = builder.ins().iconst(types::I32, -1);
        builder.ins().return_(&[err]);

        // Seal all blocks
        builder.seal_block(check_struct_start);
        builder.seal_block(field_loop);
        builder.seal_block(check_event_tag);
        builder.seal_block(check_field_key);
        builder.seal_block(process_field);
        builder.seal_block(skip_field_block);
        builder.seal_block(after_field);
        builder.seal_block(success_block);
        builder.seal_block(error_block);
        for block in &field_blocks {
            builder.seal_block(*block);
        }
        for block in &compare_blocks {
            builder.seal_block(*block);
        }
        for block in set_bit_blocks.iter().flatten() {
            builder.seal_block(*block);
        }

        builder.finalize();
    }

    module.define_function(func_id, &mut ctx).ok()?;

    Some((func_id, all_nested_modules))
}

/// Field info for code generation.
#[allow(dead_code)]
struct FieldCodegenInfo {
    /// Field name (for matching)
    name: &'static str,
    /// Byte offset in the struct
    offset: usize,
    /// Type of write operation needed
    write_kind: WriteKind,
    /// Index in the required-field bitmask (None if optional)
    required_bit_index: Option<usize>,
}

/// What kind of write operation is needed for a field.
#[allow(dead_code)]
enum WriteKind {
    U8,
    U16,
    U32,
    U64,
    I8,
    I16,
    I32,
    I64,
    F32,
    F64,
    Bool,
    String,
    /// Nested struct that needs to be deserialized via a separate compiled function
    NestedStruct(&'static Shape),
    /// `Option<T>` - shape is the Option shape (includes inner type)
    Option(&'static Shape),
    /// `Vec<T>` - shape is the Vec shape (includes element type)
    Vec(&'static Shape),
}

#[allow(dead_code)]
impl WriteKind {
    /// Returns the expected ScalarTag values for this WriteKind, if it's a scalar type.
    /// Returns None for non-scalar types (NestedStruct, Option, Vec).
    /// For numeric types, we accept both I64 and U64 since JSON doesn't distinguish
    /// between signed and unsigned integers.
    fn expected_scalar_tags(&self) -> Option<(u8, Option<u8>)> {
        use helpers::ScalarTag;
        match self {
            // All integer types accept both I64 and U64 since JSON parsers
            // typically emit all integers as I64 (JSON has no unsigned type)
            WriteKind::I8
            | WriteKind::I16
            | WriteKind::I32
            | WriteKind::I64
            | WriteKind::U8
            | WriteKind::U16
            | WriteKind::U32
            | WriteKind::U64 => Some((ScalarTag::I64 as u8, Some(ScalarTag::U64 as u8))),
            // Floats come as F64
            WriteKind::F32 | WriteKind::F64 => Some((ScalarTag::F64 as u8, None)),
            WriteKind::Bool => Some((ScalarTag::Bool as u8, None)),
            WriteKind::String => Some((ScalarTag::Str as u8, None)),
            // Non-scalar types
            WriteKind::NestedStruct(_) | WriteKind::Option(_) | WriteKind::Vec(_) => None,
        }
    }

    fn from_shape(shape: &'static Shape) -> Option<Self> {
        use facet_core::ScalarType;

        // Check for scalar types via shape.scalar_type()
        if let Some(scalar_type) = shape.scalar_type() {
            return match scalar_type {
                ScalarType::Bool => Some(WriteKind::Bool),
                ScalarType::U8 => Some(WriteKind::U8),
                ScalarType::U16 => Some(WriteKind::U16),
                ScalarType::U32 => Some(WriteKind::U32),
                ScalarType::U64 => Some(WriteKind::U64),
                ScalarType::I8 => Some(WriteKind::I8),
                ScalarType::I16 => Some(WriteKind::I16),
                ScalarType::I32 => Some(WriteKind::I32),
                ScalarType::I64 => Some(WriteKind::I64),
                ScalarType::F32 => Some(WriteKind::F32),
                ScalarType::F64 => Some(WriteKind::F64),
                ScalarType::String => Some(WriteKind::String),
                _ => None, // Other scalar types not yet supported
            };
        }

        // Check by Def (Option, Vec, nested structs)
        match &shape.def {
            Def::Option(_option_def) => {
                // For now, support Option<T> where T is JIT-compatible
                // TODO: Could recursively check inner type compatibility
                Some(WriteKind::Option(shape))
            }
            Def::List(list_def) => {
                // Check if we can handle the element type
                if is_vec_element_supported(list_def.t) {
                    Some(WriteKind::Vec(shape))
                } else {
                    None
                }
            }
            _ => {
                // Check for nested struct
                if let FacetType::User(UserType::Struct(_)) = &shape.ty {
                    // Recursively check if the nested struct is JIT-compatible
                    if is_jit_compatible(shape) {
                        return Some(WriteKind::NestedStruct(shape));
                    }
                }
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jit_compatibility_primitives() {
        // Test that primitive types are not JIT-compatible (they're not structs)
        assert!(!is_jit_compatible(i64::SHAPE));
        assert!(!is_jit_compatible(String::SHAPE));
        assert!(!is_jit_compatible(bool::SHAPE));
    }

    #[test]
    fn test_vec_element_supported() {
        // Direct tests for is_vec_element_supported
        assert!(is_vec_element_supported(i64::SHAPE));
        assert!(is_vec_element_supported(f64::SHAPE));
        assert!(is_vec_element_supported(bool::SHAPE));
        assert!(is_vec_element_supported(String::SHAPE));

        // Nested Vec<f64>
        assert!(is_vec_element_supported(<Vec<f64>>::SHAPE));

        // Deeply nested Vec<Vec<f64>>
        assert!(is_vec_element_supported(<Vec<Vec<f64>>>::SHAPE));

        // Triple nested Vec<Vec<Vec<f64>>>
        assert!(is_vec_element_supported(<Vec<Vec<Vec<f64>>>>::SHAPE));
    }
}
