//! Sketch: Cranelift-based JIT deserializer for facet-json
//!
//! This is a design sketch, not working code yet.

use cranelift::prelude::*;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{DataDescription, Linkage, Module};
use std::any::TypeId;
use std::collections::HashMap;
use std::sync::RwLock;

// =============================================================================
// Cache: TypeId -> JIT'd deserializer function
// =============================================================================

type DeserializeFn = unsafe fn(
    input: *const u8,      // JSON input pointer
    input_len: usize,      // JSON input length
    output: *mut u8,       // Output struct pointer
) -> Result<usize, u32>;   // Ok(bytes_consumed) or Err(error_code)

static CACHE: RwLock<Option<JitCache>> = RwLock::new(None);

struct JitCache {
    module: JITModule,
    functions: HashMap<TypeId, DeserializeFn>,
}

// =============================================================================
// Helper functions the JIT code calls back into
// =============================================================================

/// Parse a JSON string, write to output, return bytes consumed
extern "C" fn helper_parse_string(
    input: *const u8,
    input_len: usize,
    output: *mut String,
) -> isize {
    // This is Rust code that does actual JSON string parsing
    // Returns bytes consumed, or negative for error
    todo!()
}

/// Parse a JSON f64, return bytes consumed
extern "C" fn helper_parse_f64(
    input: *const u8,
    input_len: usize,
    output: *mut f64,
) -> isize {
    todo!()
}

/// Parse a JSON i64, return bytes consumed
extern "C" fn helper_parse_i64(
    input: *const u8,
    input_len: usize,
    output: *mut i64,
) -> isize {
    todo!()
}

/// Skip a JSON value (for unknown fields), return bytes consumed
extern "C" fn helper_skip_value(
    input: *const u8,
    input_len: usize,
) -> isize {
    todo!()
}

/// Parse field name, return hash + bytes consumed
extern "C" fn helper_parse_field_name(
    input: *const u8,
    input_len: usize,
    hash_out: *mut u64,
) -> isize {
    todo!()
}

// =============================================================================
// JIT Compiler
// =============================================================================

struct JitCompiler {
    builder_ctx: FunctionBuilderContext,
    ctx: codegen::Context,
    module: JITModule,
}

impl JitCompiler {
    fn new() -> Self {
        let mut builder = JITBuilder::new(cranelift_module::default_libcall_names()).unwrap();

        // Register helper functions so JIT code can call them
        builder.symbol("helper_parse_string", helper_parse_string as *const u8);
        builder.symbol("helper_parse_f64", helper_parse_f64 as *const u8);
        builder.symbol("helper_parse_i64", helper_parse_i64 as *const u8);
        builder.symbol("helper_skip_value", helper_skip_value as *const u8);
        builder.symbol("helper_parse_field_name", helper_parse_field_name as *const u8);

        let module = JITModule::new(builder);

        Self {
            builder_ctx: FunctionBuilderContext::new(),
            ctx: module.make_context(),
            module,
        }
    }

    /// Compile a deserializer for a struct type
    fn compile_struct_deserializer(
        &mut self,
        shape: &facet_core::Shape,
    ) -> DeserializeFn {
        let ptr_type = self.module.target_config().pointer_type();

        // Function signature: (input_ptr, input_len, output_ptr) -> i64
        self.ctx.func.signature.params.push(AbiParam::new(ptr_type)); // input
        self.ctx.func.signature.params.push(AbiParam::new(types::I64)); // input_len
        self.ctx.func.signature.params.push(AbiParam::new(ptr_type)); // output
        self.ctx.func.signature.returns.push(AbiParam::new(types::I64)); // result

        let mut builder = FunctionBuilder::new(&mut self.ctx.func, &mut self.builder_ctx);

        // Create entry block
        let entry_block = builder.create_block();
        builder.append_block_params_for_function_params(entry_block);
        builder.switch_to_block(entry_block);
        builder.seal_block(entry_block);

        let input_ptr = builder.block_params(entry_block)[0];
        let input_len = builder.block_params(entry_block)[1];
        let output_ptr = builder.block_params(entry_block)[2];

        // Track current position in input
        let pos = builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            8,
            0,
        ));
        let zero = builder.ins().iconst(types::I64, 0);
        builder.ins().stack_store(zero, pos, 0);

        // Get struct fields from shape
        if let facet_core::Type::User(facet_core::UserType::Struct(struct_def)) = &shape.ty {
            // Generate field parsing code
            self.generate_struct_body(
                &mut builder,
                struct_def,
                input_ptr,
                input_len,
                output_ptr,
                pos,
            );
        }

        // Return bytes consumed
        let bytes_consumed = builder.ins().stack_load(types::I64, pos, 0);
        builder.ins().return_(&[bytes_consumed]);

        builder.finalize();

        // Declare and define the function
        let func_id = self.module
            .declare_function("deserialize", Linkage::Local, &self.ctx.func.signature)
            .unwrap();
        self.module.define_function(func_id, &mut self.ctx).unwrap();
        self.module.clear_context(&mut self.ctx);
        self.module.finalize_definitions().unwrap();

        // Get function pointer
        let code_ptr = self.module.get_finalized_function(func_id);
        unsafe { std::mem::transmute(code_ptr) }
    }

    fn generate_struct_body(
        &mut self,
        builder: &mut FunctionBuilder,
        struct_def: &facet_core::StructType,
        input_ptr: Value,
        input_len: Value,
        output_ptr: Value,
        pos: StackSlot,
    ) {
        let ptr_type = self.module.target_config().pointer_type();

        // For each field, generate:
        // 1. Call helper_parse_field_name to get field hash
        // 2. Switch on hash to jump to appropriate field handler
        // 3. Call appropriate helper_parse_* and write to output_ptr + offset

        for field in struct_def.fields {
            let field_offset = builder.ins().iconst(types::I64, field.offset as i64);
            let field_ptr = builder.ins().iadd(output_ptr, field_offset);

            // Based on field type, call appropriate helper
            match &field.shape().ty {
                facet_core::Type::Primitive(facet_core::PrimitiveType::Numeric(num_ty)) => {
                    match num_ty {
                        facet_core::NumericType::Float => {
                            // Call helper_parse_f64(input + pos, len - pos, field_ptr)
                            // This is simplified - real code needs to:
                            // 1. Load current pos
                            // 2. Add to input_ptr
                            // 3. Subtract from input_len
                            // 4. Call helper
                            // 5. Add result to pos
                        }
                        facet_core::NumericType::Integer { .. } => {
                            // Call helper_parse_i64
                        }
                    }
                }
                facet_core::Type::Primitive(facet_core::PrimitiveType::String(_)) => {
                    // Call helper_parse_string
                }
                facet_core::Type::Sequence(_) => {
                    // Arrays need special handling - could:
                    // 1. Call another JIT'd function for Vec<T>
                    // 2. Or have a specialized helper for Vec<f64> etc.
                }
                facet_core::Type::User(facet_core::UserType::Struct(_)) => {
                    // Recursive: call JIT'd deserializer for nested struct
                }
                _ => {
                    // Call helper_skip_value for unsupported types
                }
            }
        }
    }
}

// =============================================================================
// Public API
// =============================================================================

/// JIT-accelerated deserialization
pub fn from_str_jit<'de, T: facet_core::Facet>(input: &'de str) -> Result<T, crate::JsonError> {
    let type_id = TypeId::of::<T>();

    // Check cache
    let cache = CACHE.read().unwrap();
    if let Some(cache) = cache.as_ref() {
        if let Some(&func) = cache.functions.get(&type_id) {
            // Fast path: use cached JIT'd function
            let mut output = std::mem::MaybeUninit::<T>::uninit();
            unsafe {
                let result = func(
                    input.as_ptr(),
                    input.len(),
                    output.as_mut_ptr() as *mut u8,
                );
                match result {
                    Ok(_) => return Ok(output.assume_init()),
                    Err(code) => return Err(error_from_code(code)),
                }
            }
        }
    }
    drop(cache);

    // Slow path: compile and cache
    // ... compile using JitCompiler, insert into cache ...

    // Fallback to interpreter for now
    crate::from_str(input)
}

fn error_from_code(_code: u32) -> crate::JsonError {
    todo!()
}

// =============================================================================
// Key optimizations this enables
// =============================================================================

// 1. NO runtime Shape inspection per field
//    - Field offsets are baked into the generated code as immediates
//    - Field types determine which helper is called at compile time
//
// 2. NO virtual dispatch through Partial
//    - Direct memory writes: `mov [output + offset], value`
//
// 3. NO begin_list_item/end overhead
//    - For Vec<f64>, could generate tight loop:
//      while !eof { parse_f64(); vec.push(val); }
//
// 4. Perfect hash for field names
//    - Pre-compute hash at JIT time
//    - Single comparison per field instead of linear scan
//
// 5. Specialized Vec<primitive> fast paths
//    - Instead of generic list machinery, tight parsing loops

// =============================================================================
// Vec<f64> specialization sketch
// =============================================================================

/// What a JIT'd Vec<f64> deserializer might look like (pseudocode):
///
/// ```text
/// deserialize_vec_f64:
///     ; expect '['
///     call helper_expect_array_start
///
///     ; allocate vec with some initial capacity
///     mov rdi, 16                    ; initial capacity
///     call helper_vec_new_f64
///     mov r12, rax                   ; r12 = vec ptr
///
/// .loop:
///     ; check for ']'
///     call helper_peek_byte
///     cmp al, ']'
///     je .done
///
///     ; parse f64 directly into temp
///     lea rdi, [rsp - 8]
///     call helper_parse_f64_fast
///
///     ; push to vec (inlined or helper)
///     mov rdi, r12
///     movsd xmm0, [rsp - 8]
///     call helper_vec_push_f64
///
///     ; skip comma if present
///     call helper_skip_comma_maybe
///     jmp .loop
///
/// .done:
///     ; skip ']'
///     call helper_expect_array_end
///     mov rax, r12
///     ret
/// ```
