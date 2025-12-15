//! Cranelift-based compiler for deserializers.
//!
//! This module takes a `Shape` and generates native code that consumes
//! `ParseEvent`s and writes directly to struct memory.

use std::marker::PhantomData;
use std::mem::MaybeUninit;

use cranelift::prelude::*;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, Linkage, Module};

use facet_core::{Def, Facet, Field, Shape, Type as FacetType, UserType};

use super::helpers::{self, JitContext};
use crate::{DeserializeError, FormatParser};

/// A compiled deserializer for a specific type and parser.
pub struct CompiledDeserializer<T, P> {
    /// Pointer to the compiled function
    fn_ptr: *const u8,
    /// Phantom data for type safety
    _phantom: PhantomData<fn(&mut P) -> T>,
}

// Safety: The compiled code is thread-safe
unsafe impl<T, P> Send for CompiledDeserializer<T, P> {}
unsafe impl<T, P> Sync for CompiledDeserializer<T, P> {}

impl<T, P> CompiledDeserializer<T, P> {
    /// Create from a raw function pointer.
    ///
    /// # Safety
    /// The pointer must point to a valid compiled function with the correct signature.
    pub unsafe fn from_ptr(fn_ptr: *const u8) -> Self {
        Self {
            fn_ptr,
            _phantom: PhantomData,
        }
    }

    /// Get the raw function pointer.
    pub fn as_ptr(&self) -> *const u8 {
        self.fn_ptr
    }
}

impl<'de, T: Facet<'static>, P: FormatParser<'de>> CompiledDeserializer<T, P> {
    /// Execute the compiled deserializer.
    pub fn deserialize(&self, parser: &mut P) -> Result<T, DeserializeError<P::Error>> {
        // Create output storage
        let mut output: MaybeUninit<T> = MaybeUninit::uninit();

        // Create JIT context
        let mut ctx = JitContext {
            parser: parser as *mut P,
            current_event: None,
        };

        // Call the compiled function
        // Signature: fn(ctx: *mut JitContext<P>, out: *mut T) -> i32
        type CompiledFn<P, T> = unsafe extern "C" fn(*mut JitContext<P>, *mut T) -> i32;
        let func: CompiledFn<P, T> = unsafe { std::mem::transmute(self.fn_ptr) };

        let result = unsafe { func(&mut ctx, output.as_mut_ptr()) };

        if result == 0 {
            Ok(unsafe { output.assume_init() })
        } else {
            Err(DeserializeError::Unsupported(format!(
                "JIT deserialization failed with code {}",
                result
            )))
        }
    }
}

/// Check if a shape is JIT-compatible.
///
/// Currently only supports simple structs without:
/// - Flatten fields
/// - Untagged enums
/// - Generic parameters that aren't concrete
pub fn is_jit_compatible(shape: &'static Shape) -> bool {
    // Check if it's a struct via shape.ty
    let FacetType::User(UserType::Struct(struct_def)) = &shape.ty else {
        return false;
    };

    // Check for flatten
    if struct_def.fields.iter().any(|f| f.is_flattened()) {
        return false;
    }

    // Check that all field types are supported
    struct_def.fields.iter().all(|f| is_field_type_supported(f))
}

/// Check if a field type is supported for JIT compilation.
fn is_field_type_supported(field: &Field) -> bool {
    let shape = field.shape();
    match &shape.def {
        // Primitive types (scalars)
        Def::Scalar => true,
        // TODO: Add more types (Vec, Option, nested structs)
        _ => {
            // Check for common types by name
            let type_name = shape.type_identifier;
            matches!(
                type_name,
                "bool"
                    | "u8"
                    | "u16"
                    | "u32"
                    | "u64"
                    | "i8"
                    | "i16"
                    | "i32"
                    | "i64"
                    | "f32"
                    | "f64"
                    | "String"
                    | "&str"
            )
        }
    }
}

/// Try to compile a deserializer for the given type and parser.
pub fn try_compile<'de, T: Facet<'static>, P: FormatParser<'de>>()
-> Option<CompiledDeserializer<T, P>> {
    let shape = T::SHAPE;

    if !is_jit_compatible(shape) {
        return None;
    }

    // Build the JIT module
    let mut builder = JITBuilder::new(cranelift_module::default_libcall_names()).ok()?;

    // Register helper functions
    register_helpers(&mut builder);

    let mut module = JITModule::new(builder);

    // Compile the deserializer
    let func_id = compile_deserializer::<T, P>(&mut module, shape)?;

    // Finalize and get the function pointer
    module.finalize_definitions().ok()?;
    let fn_ptr = module.get_finalized_function(func_id);

    Some(CompiledDeserializer {
        fn_ptr: fn_ptr as *const u8,
        _phantom: PhantomData,
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
}

/// Compile a deserializer function for a struct.
fn compile_deserializer<'de, T, P: FormatParser<'de>>(
    module: &mut JITModule,
    shape: &'static Shape,
) -> Option<FuncId> {
    let FacetType::User(UserType::Struct(_struct_def)) = &shape.ty else {
        return None;
    };

    let pointer_type = module.target_config().pointer_type();

    // Function signature: fn(ctx: *mut JitContext, out: *mut T) -> i32
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(pointer_type)); // ctx
    sig.params.push(AbiParam::new(pointer_type)); // out
    sig.returns.push(AbiParam::new(types::I32)); // result

    let func_id = module
        .declare_function("jit_deserialize", Linkage::Local, &sig)
        .ok()?;

    let mut ctx = module.make_context();
    ctx.func.signature = sig;

    // Build the function body
    let mut builder_ctx = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut builder_ctx);

        // Create entry block
        let entry_block = builder.create_block();
        builder.append_block_params_for_function_params(entry_block);
        builder.switch_to_block(entry_block);
        builder.seal_block(entry_block);

        // Get parameters
        let _ctx_ptr = builder.block_params(entry_block)[0];
        let _out_ptr = builder.block_params(entry_block)[1];

        // For now, just return success (placeholder)
        // TODO: Generate actual parsing code
        let zero = builder.ins().iconst(types::I32, 0);
        builder.ins().return_(&[zero]);

        builder.finalize();
    }

    module.define_function(func_id, &mut ctx).ok()?;

    Some(func_id)
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
}

#[allow(dead_code)]
impl WriteKind {
    fn from_shape(shape: &Shape) -> Option<Self> {
        let type_name = shape.type_identifier;
        match type_name {
            "bool" => Some(WriteKind::Bool),
            "u8" => Some(WriteKind::U8),
            "u16" => Some(WriteKind::U16),
            "u32" => Some(WriteKind::U32),
            "u64" => Some(WriteKind::U64),
            "i8" => Some(WriteKind::I8),
            "i16" => Some(WriteKind::I16),
            "i32" => Some(WriteKind::I32),
            "i64" => Some(WriteKind::I64),
            "f32" => Some(WriteKind::F32),
            "f64" => Some(WriteKind::F64),
            "String" | "alloc::string::String" => Some(WriteKind::String),
            _ => None,
        }
    }
}
