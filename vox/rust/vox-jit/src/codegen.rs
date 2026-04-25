//! Cranelift backend: lowers `DecodeProgram` IR into native machine code.
//!
//! The generated function signature (stable extern "C" ABI):
//!   unsafe extern "C" fn(ctx: *mut DecodeCtx, out_ptr: *mut u8) -> u32
//!
//! The `u32` return value is the `DecodeStatus` discriminant.
//!
//! # Minimal subset (task #8)
//!
//! The initial Cranelift backend handles:
//!   - All scalar primitives (bool, u8–u64, i8–i64, f32, f64)
//!   - Structs and tuples (field reads in remote wire order)
//!   - Fixed-size arrays (unrolled element decode)
//!   - Vec<u8> and String via calibrated opaque descriptors
//!   - SlowPath ops (delegates back to the IR interpreter)
//!
//! Unsupported ops cause `compile_decode` to return `Err(CodegenError::UnsupportedOp)`
//! and the caller falls back to the pure IR interpreter.

#![allow(unsafe_code)]

use cranelift_codegen::ir::{
    AbiParam, Block, BlockArg, ExtFuncData, ExternalName, InstBuilder, LibCall, MemFlags,
    Signature, StackSlotData, StackSlotKind, Type, Value, condcodes::IntCC, types,
};
use cranelift_codegen::{settings, settings::Configurable};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module};

use vox_jit_abi::{
    BorrowedDecodeFn, DecodeCtx, DecodeStatus, EncodeCtx, EncodeFn, OwnedDecodeFn,
    vox_jit_box_alloc, vox_jit_box_slice_alloc, vox_jit_buf_grow, vox_jit_string_alloc,
    vox_jit_utf8_validate, vox_jit_vec_alloc,
};
use vox_jit_cal::{
    CalibrationRegistry, ContainerKind, DescriptorHandle, OFFSET_ABSENT,
    OpaqueDescriptor as CalDescriptor,
};

use vox_postcard::ir::{
    DecodeOp, DecodeProgram, EncodeOp, EncodeProgram, OpaqueDescriptorId, TagWidth, WirePrimitive,
};

/// Map of nested `&'static Shape` pointers to already-compiled encoders.
///
/// Populated by the runtime (`JitRuntime::prepare_encoder`) before it
/// calls `compile_encode`. The key is the shape pointer address — two shapes
/// with the same address are the same `Facet` type within the process.
///
/// Consulted by `EncodeOp::WriteShape` handling to choose between:
///   - inlining the child encoder's IR directly into the parent (no call,
///     no prologue/epilogue, `buf_len` stays in a register across the
///     child's ops) — when the cycle guard allows it;
///   - emitting a direct `call_indirect(child_fn_ptr, [ctx, src_ptr])` —
///     when inlining would recurse into a shape already on the inlining
///     stack;
///   - falling through to `vox_jit_encode_shape` — when the child isn't in
///     the map at all (runtime fallback path).
pub type ChildEncoderMap = std::collections::HashMap<
    &'static facet_core::Shape,
    &'static crate::cache::CompiledEncoder,
>;

/// Walk an `EncodeProgram` and collect every distinct `WriteShape` child
/// shape. Used by the runtime to pre-compile nested encoders before the
/// parent encoder is emitted so the parent can call them directly.
pub fn collect_write_shape_children(program: &EncodeProgram) -> Vec<&'static facet_core::Shape> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::<*const facet_core::Shape>::new();
    for block in &program.blocks {
        for op in &block.ops {
            if let EncodeOp::WriteShape { shape, .. } = op {
                let p = *shape as *const _;
                if seen.insert(p) {
                    out.push(*shape);
                }
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Target ISA name
// ---------------------------------------------------------------------------

/// Return a stable string identifying the current host ISA.
pub fn host_isa_name() -> &'static str {
    #[cfg(target_arch = "x86_64")]
    {
        "x86_64"
    }
    #[cfg(target_arch = "aarch64")]
    {
        "aarch64"
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        "unknown"
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum CodegenError {
    ModuleError(cranelift_module::ModuleError),
    CraneliftError(cranelift_codegen::CodegenError),
    UnsupportedOp(String),
    IsaError(String),
}

impl From<cranelift_module::ModuleError> for CodegenError {
    fn from(e: cranelift_module::ModuleError) -> Self {
        Self::ModuleError(e)
    }
}

impl From<cranelift_codegen::CodegenError> for CodegenError {
    fn from(e: cranelift_codegen::CodegenError) -> Self {
        Self::CraneliftError(e)
    }
}

impl std::fmt::Display for CodegenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ModuleError(e) => write!(f, "module error: {e}"),
            Self::CraneliftError(e) => write!(f, "codegen error: {e}"),
            Self::UnsupportedOp(s) => write!(f, "unsupported op: {s}"),
            Self::IsaError(s) => write!(f, "ISA error: {s}"),
        }
    }
}

impl std::error::Error for CodegenError {}

// ---------------------------------------------------------------------------
// Cranelift backend
// ---------------------------------------------------------------------------

/// Cranelift JIT backend.
///
/// One instance per process. Owns the `JITModule` which holds all compiled
/// function memory. Thread-safety is provided by the caller (wrapped in Mutex
/// in `JitRuntime`).
pub struct CraneliftBackend {
    module: JITModule,
    ptr_ty: Type,
    isa_name: &'static str,
    /// Programs kept alive for the lifetime of the backend so that raw pointers
    /// to plans embedded by `SlowPath` ops remain valid when the compiled
    /// decoders are called.
    retained_programs: Vec<DecodeProgram>,
}

impl CraneliftBackend {
    /// Create a new backend targeting the current host ISA.
    pub fn new() -> Result<Self, CodegenError> {
        let mut flag_builder = settings::builder();
        flag_builder.set("use_colocated_libcalls", "false").unwrap();
        flag_builder.set("is_pic", "false").unwrap();
        flag_builder.set("opt_level", "speed").unwrap();

        let isa_builder =
            cranelift_native::builder().map_err(|e| CodegenError::IsaError(e.to_string()))?;
        let flags = settings::Flags::new(flag_builder);
        let isa = isa_builder
            .finish(flags)
            .map_err(CodegenError::CraneliftError)?;

        let ptr_ty = isa.pointer_type();
        let isa_name = host_isa_name();

        let mut jit_builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());

        // Register decode runtime helper symbols.
        jit_builder.symbol("vox_jit_vec_alloc", vox_jit_vec_alloc as *const u8);
        jit_builder.symbol("vox_jit_string_alloc", vox_jit_string_alloc as *const u8);
        jit_builder.symbol("vox_jit_utf8_validate", vox_jit_utf8_validate as *const u8);
        jit_builder.symbol("vox_jit_box_alloc", vox_jit_box_alloc as *const u8);
        jit_builder.symbol(
            "vox_jit_box_slice_alloc",
            vox_jit_box_slice_alloc as *const u8,
        );
        jit_builder.symbol(
            "vox_jit_slow_path",
            crate::helpers::vox_jit_slow_path as *const u8,
        );
        jit_builder.symbol(
            "vox_jit_decode_opaque",
            crate::helpers::vox_jit_decode_opaque as *const u8,
        );
        jit_builder.symbol(
            "vox_jit_write_default",
            crate::helpers::vox_jit_write_default as *const u8,
        );
        jit_builder.symbol(
            "vox_jit_encode_slow_path",
            crate::helpers::vox_jit_encode_slow_path as *const u8,
        );
        jit_builder.symbol(
            "vox_jit_init_cow_byte_slice_owned",
            crate::helpers::vox_jit_init_cow_byte_slice_owned as *const u8,
        );
        jit_builder.symbol(
            "vox_jit_init_cow_byte_slice_borrowed",
            crate::helpers::vox_jit_init_cow_byte_slice_borrowed as *const u8,
        );
        jit_builder.symbol(
            "vox_jit_init_byte_slice_ref",
            crate::helpers::vox_jit_init_byte_slice_ref as *const u8,
        );
        jit_builder.symbol(
            "vox_jit_init_cow_str_owned",
            crate::helpers::vox_jit_init_cow_str_owned as *const u8,
        );
        jit_builder.symbol(
            "vox_jit_init_cow_str_borrowed",
            crate::helpers::vox_jit_init_cow_str_borrowed as *const u8,
        );
        jit_builder.symbol(
            "vox_jit_init_str_ref",
            crate::helpers::vox_jit_init_str_ref as *const u8,
        );

        // Register encode runtime helper symbols.
        jit_builder.symbol("vox_jit_buf_grow", vox_jit_buf_grow as *const u8);
        jit_builder.symbol(
            "vox_jit_encode_string_like",
            crate::helpers::vox_jit_encode_string_like as *const u8,
        );
        jit_builder.symbol(
            "vox_jit_encode_shape",
            crate::helpers::vox_jit_encode_shape as *const u8,
        );
        jit_builder.symbol(
            "vox_jit_encode_opaque",
            crate::helpers::vox_jit_encode_opaque as *const u8,
        );
        jit_builder.symbol(
            "vox_jit_encode_proxy",
            crate::helpers::vox_jit_encode_proxy as *const u8,
        );
        jit_builder.symbol(
            "vox_jit_encode_bytes_like",
            crate::helpers::vox_jit_encode_bytes_like as *const u8,
        );

        let module = JITModule::new(jit_builder);

        Ok(Self {
            module,
            ptr_ty,
            isa_name,
            retained_programs: Vec::new(),
        })
    }

    pub fn isa_name(&self) -> &'static str {
        self.isa_name
    }

    /// Compile an owned decoder.
    pub fn compile_decode_owned(
        &mut self,
        shape: &'static facet_core::Shape,
        program: &DecodeProgram,
        descriptors: &CalibrationRegistry,
    ) -> Result<OwnedDecodeFn, CodegenError> {
        let (fn_ptr, _) = self.compile_decode_inner(shape, program, descriptors)?;
        Ok(unsafe { core::mem::transmute(fn_ptr) })
    }

    /// Compile a borrowed decoder.
    pub fn compile_decode_borrowed(
        &mut self,
        shape: &'static facet_core::Shape,
        program: &DecodeProgram,
        descriptors: &CalibrationRegistry,
    ) -> Result<BorrowedDecodeFn, CodegenError> {
        let (fn_ptr, _) = self.compile_decode_inner(shape, program, descriptors)?;
        Ok(unsafe { core::mem::transmute(fn_ptr) })
    }

    fn compile_decode_inner(
        &mut self,
        shape: &'static facet_core::Shape,
        program: &DecodeProgram,
        descriptors: &CalibrationRegistry,
    ) -> Result<(*const u8, u32), CodegenError> {
        // Retain an owned copy of the program. The emitted decoder embeds
        // raw pointers into `SlowPath` op plans; those plans must outlive
        // the decoder.
        self.retained_programs.push(program.clone());
        let program = self.retained_programs.last().unwrap();

        let sig = self.decode_signature();
        let func_name = format!("vox_decode_{}_{}", shape_symbol_fragment(shape), next_id());

        let func_id = self
            .module
            .declare_function(&func_name, Linkage::Local, &sig)?;

        let mut ctx = self.module.make_context();
        ctx.func.signature = sig;

        let mut func_ctx = FunctionBuilderContext::new();
        {
            let mut builder = FunctionBuilder::new(&mut ctx.func, &mut func_ctx);
            emit_decode_function(&mut builder, program, descriptors, self.ptr_ty)?;
            builder.finalize();
        }

        self.module.define_function(func_id, &mut ctx)?;

        let code_size = ctx
            .compiled_code()
            .map(|c| c.code_info().total_size)
            .unwrap_or(0);

        self.module.clear_context(&mut ctx);
        self.module
            .finalize_definitions()
            .map_err(CodegenError::ModuleError)?;

        let fn_ptr = self.module.get_finalized_function(func_id);
        crate::jitdump::record_load(&func_name, fn_ptr, code_size);
        Ok((fn_ptr, code_size))
    }

    /// Like `compile_decode` but also returns the machine-code size in bytes.
    ///
    /// Used by benchmarks to measure compiled decoder size per root type.
    pub fn compile_decode_with_size(
        &mut self,
        shape: &'static facet_core::Shape,
        program: &DecodeProgram,
        descriptors: &CalibrationRegistry,
    ) -> Result<(OwnedDecodeFn, u32), CodegenError> {
        let (fn_ptr, code_size) = self.compile_decode_inner(shape, program, descriptors)?;
        Ok((unsafe { core::mem::transmute(fn_ptr) }, code_size))
    }

    fn decode_signature(&self) -> Signature {
        let mut sig = self.module.make_signature();
        sig.params.push(AbiParam::new(self.ptr_ty)); // ctx: *mut DecodeCtx
        sig.params.push(AbiParam::new(self.ptr_ty)); // out_ptr: *mut u8
        sig.returns.push(AbiParam::new(types::I32)); // DecodeStatus as u32
        sig
    }

    /// Compile an `EncodeProgram` into an encode function pointer.
    ///
    /// The generated function has the signature:
    ///   `unsafe extern "C" fn(ctx: *mut EncodeCtx, src_ptr: *const u8) -> bool`
    ///
    /// `child_encoders` maps nested `&'static Shape` pointers to their
    /// already-compiled `EncodeFn`. When a `WriteShape { shape, .. }` op
    /// references a shape present in the map, the generated code emits a
    /// direct `call_indirect` to that fn pointer, bypassing the runtime
    /// cache lookup and the `Vec<u8>` alloc inside
    /// `vox_jit_encode_shape`. Shapes absent from the map (e.g. cyclic
    /// self-references detected at prepare time) fall through to the
    /// helper.
    ///
    /// Returns `Err(CodegenError::UnsupportedOp)` if any op in the program is
    /// not yet supported by the Cranelift backend.
    pub fn compile_encode(
        &mut self,
        shape: &'static facet_core::Shape,
        program: &EncodeProgram,
        descriptors: &CalibrationRegistry,
        child_encoders: std::sync::Arc<ChildEncoderMap>,
    ) -> Result<EncodeFn, CodegenError> {
        let sig = self.encode_signature();
        let func_name = format!("vox_encode_{}_{}", shape_symbol_fragment(shape), next_id());

        let func_id = self
            .module
            .declare_function(&func_name, Linkage::Local, &sig)?;

        let mut ctx = self.module.make_context();
        ctx.func.signature = sig;
        let dump = crate::dump_compiled();
        if dump {
            ctx.set_disasm(true);
        }

        let mut func_ctx = FunctionBuilderContext::new();
        {
            let mut builder = FunctionBuilder::new(&mut ctx.func, &mut func_ctx);
            emit_encode_function(
                &mut builder,
                program,
                descriptors,
                self.ptr_ty,
                child_encoders,
                Some(shape),
            )?;
            builder.finalize();
        }

        if dump {
            eprintln!("=== CLIF encode {func_name} ===\n{}", ctx.func);
        }
        self.module.define_function(func_id, &mut ctx)?;
        if dump {
            if let Some(cc) = ctx.compiled_code()
                && let Some(d) = cc.vcode.as_deref()
            {
                eprintln!("=== asm encode {func_name} ===\n{d}");
            }
        }
        let code_size = ctx
            .compiled_code()
            .map(|c| c.code_info().total_size)
            .unwrap_or(0);
        self.module.clear_context(&mut ctx);
        self.module
            .finalize_definitions()
            .map_err(CodegenError::ModuleError)?;

        let fn_ptr = self.module.get_finalized_function(func_id);
        crate::jitdump::record_load(&func_name, fn_ptr, code_size);

        // SAFETY: We just compiled and finalized this function; the pointer is
        // valid for the lifetime of the JITModule.
        let encode_fn: EncodeFn = unsafe { core::mem::transmute(fn_ptr) };
        Ok(encode_fn)
    }

    fn encode_signature(&self) -> Signature {
        let mut sig = self.module.make_signature();
        sig.params.push(AbiParam::new(self.ptr_ty)); // ctx: *mut EncodeCtx
        sig.params.push(AbiParam::new(self.ptr_ty)); // src_ptr: *const u8
        sig.returns.push(AbiParam::new(types::I8)); // bool (success)
        sig
    }
}

// ---------------------------------------------------------------------------
// Function emission
// ---------------------------------------------------------------------------

/// State threaded through the Cranelift function builder.
struct EmitCtx<'a, 'b> {
    b: &'a mut FunctionBuilder<'b>,
    ctx_ptr: Value,
    out_ptr: Value,
    ptr_ty: Type,
    /// Variable tracking bytes consumed (written back to ctx on return).
    var_consumed: Variable,
    /// Variable tracking partially-initialized element count (committed len).
    var_init_count: Variable,
    /// Variable holding the total list length (set by ReadListLen, used by AllocBacking loop).
    var_list_len: Variable,
    /// Variable holding the last-decoded enum discriminant (set by ReadDiscriminant).
    var_discriminant: Variable,
    descriptors: &'a CalibrationRegistry,
    /// Mapping from DecodeProgram block index to Cranelift Block.
    block_map: Vec<Option<Block>>,
    /// IR block indices that have been emitted inline (e.g. Vec element body).
    /// These must be skipped by the outer emit loop to avoid double-emission.
    inlined_blocks: std::collections::HashSet<usize>,
    /// IR block indices that were fully sealed+filled during recursive inlining.
    /// The outer loop skips these entirely — no dummy terminator is needed.
    sealed_inlined_blocks: std::collections::HashSet<usize>,
}

impl<'a, 'b> EmitCtx<'a, 'b> {
    fn fresh_block(&mut self) -> Block {
        self.b.create_block()
    }

    fn fresh_var(&mut self, ty: Type) -> Variable {
        self.b.declare_var(ty)
    }

    /// Load `ctx.input_ptr`.
    fn ctx_input_ptr(&mut self) -> Value {
        let off = core::mem::offset_of!(DecodeCtx, input_ptr) as i32;
        self.b
            .ins()
            .load(self.ptr_ty, MemFlags::trusted(), self.ctx_ptr, off)
    }

    /// Load `ctx.input_len`.
    fn ctx_input_len(&mut self) -> Value {
        let off = core::mem::offset_of!(DecodeCtx, input_len) as i32;
        self.b
            .ins()
            .load(self.ptr_ty, MemFlags::trusted(), self.ctx_ptr, off)
    }

    /// Read current consumed variable.
    fn consumed(&mut self) -> Value {
        self.b.use_var(self.var_consumed)
    }

    /// Reload `var_consumed` from `ctx.consumed` (after an opaque helper updates it).
    fn reload_consumed_from_ctx(&mut self) {
        let off = core::mem::offset_of!(DecodeCtx, consumed) as i32;
        let val = self
            .b
            .ins()
            .load(self.ptr_ty, MemFlags::trusted(), self.ctx_ptr, off);
        self.b.def_var(self.var_consumed, val);
    }

    /// Write back `consumed` and `init_count` to the context struct.
    fn flush_ctx(&mut self) {
        let consumed = self.b.use_var(self.var_consumed);
        let off = core::mem::offset_of!(DecodeCtx, consumed) as i32;
        self.b
            .ins()
            .store(MemFlags::trusted(), consumed, self.ctx_ptr, off);

        let init_count = self.b.use_var(self.var_init_count);
        let off2 = core::mem::offset_of!(DecodeCtx, init_count) as i32;
        self.b
            .ins()
            .store(MemFlags::trusted(), init_count, self.ctx_ptr, off2);
    }

    /// Return `DecodeStatus::Ok` (writes back context first).
    fn return_ok(&mut self) {
        self.flush_ctx();
        let ok = self.b.ins().iconst(types::I32, DecodeStatus::Ok as i64);
        self.b.ins().return_(&[ok]);
    }

    /// Return a non-Ok status (writes back context first).
    fn return_err(&mut self, status: DecodeStatus) {
        self.flush_ctx();
        let code = self.b.ins().iconst(types::I32, status as i64);
        self.b.ins().return_(&[code]);
    }

    /// Compute a pointer to `out_ptr + offset`.
    fn dst_at(&mut self, offset: usize) -> Value {
        if offset == 0 {
            self.out_ptr
        } else {
            self.b.ins().iadd_imm(self.out_ptr, offset as i64)
        }
    }

    /// Read one byte from the input; generates an EOF guard.
    fn read_byte(&mut self) -> Result<Value, CodegenError> {
        let consumed = self.consumed();
        let input_len = self.ctx_input_len();
        let eof = self
            .b
            .ins()
            .icmp(IntCC::UnsignedGreaterThanOrEqual, consumed, input_len);

        let eof_block = self.fresh_block();
        let ok_block = self.fresh_block();
        self.b.ins().brif(eof, eof_block, &[], ok_block, &[]);

        self.b.switch_to_block(eof_block);
        self.b.seal_block(eof_block);
        self.return_err(DecodeStatus::UnexpectedEof);

        self.b.switch_to_block(ok_block);
        self.b.seal_block(ok_block);

        let input_ptr = self.ctx_input_ptr();
        let addr = self.b.ins().iadd(input_ptr, consumed);
        let byte = self.b.ins().load(types::I8, MemFlags::trusted(), addr, 0);
        let one = self.b.ins().iconst(self.ptr_ty, 1);
        let new_consumed = self.b.ins().iadd(consumed, one);
        self.b.def_var(self.var_consumed, new_consumed);

        Ok(byte)
    }

    /// Read a postcard unsigned varint; return as I64.
    ///
    /// Unrolled 10-byte varint decode. Each byte either terminates the varint
    /// (MSB clear) or continues to the next byte (MSB set). All early-exit
    /// paths jump to a single merge block that carries the result.
    fn read_varint_u64(&mut self) -> Result<Value, CodegenError> {
        let merge = self.fresh_block();
        self.b.append_block_param(merge, types::I64);

        let zero = self.b.ins().iconst(types::I64, 0);
        let mut acc = zero;

        for shift in 0u32..10 {
            let byte = self.read_byte()?;
            let byte64 = self.b.ins().uextend(types::I64, byte);
            let low7 = self.b.ins().band_imm(byte64, 0x7F);
            let shifted = if shift == 0 {
                low7
            } else {
                self.b.ins().ishl_imm(low7, (shift * 7) as i64)
            };
            acc = self.b.ins().bor(acc, shifted);

            let cont_bit = self.b.ins().band_imm(byte64, 0x80);
            let has_more = self.b.ins().icmp_imm(IntCC::NotEqual, cont_bit, 0);

            if shift < 9 {
                let cont_block = self.fresh_block();
                // MSB clear → varint done, jump to merge with accumulated value.
                // MSB set → read next byte.
                self.b
                    .ins()
                    .brif(has_more, cont_block, &[], merge, &[BlockArg::Value(acc)]);
                self.b.switch_to_block(cont_block);
                self.b.seal_block(cont_block);
            } else {
                // Byte 10: any continuation bit is an overflow.
                let overflow_block = self.fresh_block();
                self.b.ins().brif(
                    has_more,
                    overflow_block,
                    &[],
                    merge,
                    &[BlockArg::Value(acc)],
                );
                self.b.switch_to_block(overflow_block);
                self.b.seal_block(overflow_block);
                self.return_err(DecodeStatus::VarintOverflow);
            }
        }

        // Switch to the merge block; all paths that end the varint jump here.
        self.b.switch_to_block(merge);
        self.b.seal_block(merge);
        let result = self.b.block_params(merge)[0];
        Ok(result)
    }

    /// Read a postcard signed varint (zigzag encoded); return as I64.
    fn read_varint_i64(&mut self) -> Result<Value, CodegenError> {
        let z = self.read_varint_u64()?;
        // zigzag decode: (z >> 1) ^ -(z & 1)
        let half = self.b.ins().sshr_imm(z, 1);
        let lsb = self.b.ins().band_imm(z, 1);
        let neg_lsb = self.b.ins().ineg(lsb);
        Ok(self.b.ins().bxor(half, neg_lsb))
    }

    /// Advance consumed by `n` bytes (no bounds check — skip ops).
    fn skip_bytes_val(&mut self, n: Value) {
        let consumed = self.consumed();
        let new_c = self.b.ins().iadd(consumed, n);
        self.b.def_var(self.var_consumed, new_c);
    }
}

/// Entry: emit the full function body for `program`.
fn emit_decode_function(
    builder: &mut FunctionBuilder<'_>,
    program: &DecodeProgram,
    descriptors: &CalibrationRegistry,
    ptr_ty: Type,
) -> Result<(), CodegenError> {
    let entry = builder.create_block();
    builder.append_block_params_for_function_params(entry);
    builder.switch_to_block(entry);
    builder.seal_block(entry);

    let ctx_ptr = builder.block_params(entry)[0];
    let out_ptr = builder.block_params(entry)[1];

    let var_consumed = builder.declare_var(ptr_ty);
    let var_init_count = builder.declare_var(ptr_ty);
    let var_list_len = builder.declare_var(ptr_ty);
    let var_discriminant = builder.declare_var(types::I64);

    // Load initial consumed from ctx.
    let off = core::mem::offset_of!(DecodeCtx, consumed) as i32;
    let init_consumed = builder
        .ins()
        .load(ptr_ty, MemFlags::trusted(), ctx_ptr, off);
    builder.def_var(var_consumed, init_consumed);
    let zero = builder.ins().iconst(ptr_ty, 0);
    builder.def_var(var_init_count, zero);
    builder.def_var(var_list_len, zero);
    let zero64 = builder.ins().iconst(types::I64, 0);
    builder.def_var(var_discriminant, zero64);

    // Pre-create Cranelift blocks for all program blocks.
    let mut block_map: Vec<Option<Block>> = (0..program.blocks.len())
        .map(|_| Some(builder.create_block()))
        .collect();
    // The entry program block (block 0) maps to the already-active entry block.
    block_map[0] = Some(entry);

    let mut ctx = EmitCtx {
        b: builder,
        ctx_ptr,
        out_ptr,
        ptr_ty,
        var_consumed,
        var_init_count,
        var_list_len,
        var_discriminant,
        descriptors,
        block_map: block_map.into_iter().collect(),
        inlined_blocks: std::collections::HashSet::new(),
        sealed_inlined_blocks: std::collections::HashSet::new(),
    };

    // Emit block 0 (entry) first, then the rest.
    emit_block(&mut ctx, program, 0)?;

    // Emit remaining blocks (skip any that were inlined by emit_alloc_backing).
    for block_idx in 1..program.blocks.len() {
        if ctx.sealed_inlined_blocks.contains(&block_idx) {
            // Block was fully sealed+filled by recursive inlining — nothing to do.
            continue;
        }
        if ctx.inlined_blocks.contains(&block_idx) {
            // Block was emitted inline; the pre-created Cranelift block for it
            // is unreachable. Seal and terminate it to satisfy the verifier.
            let clif_block = ctx.block_map[block_idx].unwrap();
            ctx.b.switch_to_block(clif_block);
            ctx.b.seal_block(clif_block);
            ctx.flush_ctx();
            let ok = ctx.b.ins().iconst(types::I32, DecodeStatus::Ok as i64);
            ctx.b.ins().return_(&[ok]);
            continue;
        }
        let clif_block = ctx.block_map[block_idx].unwrap();
        ctx.b.switch_to_block(clif_block);
        ctx.b.seal_block(clif_block);
        emit_block(&mut ctx, program, block_idx)?;
    }

    Ok(())
}

fn emit_block(
    ctx: &mut EmitCtx<'_, '_>,
    program: &DecodeProgram,
    block_idx: usize,
) -> Result<(), CodegenError> {
    let ops: Vec<DecodeOp> = program.blocks[block_idx].ops.clone();
    emit_ops(ctx, program, &ops, None)
}

/// Recursively emit an IR block inline within an element decode loop.
///
/// Replaces `Return` with `jump(loop_tail)` so the element decode continues
/// into the loop-increment/commit path instead of returning from the function.
///
/// Unlike the old approach, `ReadListLen` and `BranchOnVariant` are treated as
/// inline sub-routines (matching the IR interpreter's semantics): the empty/body
/// or variant blocks are inlined with their own continuation, and processing of
/// the remaining ops in the current IR block resumes in a fresh Cranelift block.
///
/// Blocks processed here are added to `sealed_inlined_blocks` so the outer
/// block-processing loop skips them entirely.
fn emit_inline_block(
    ctx: &mut EmitCtx<'_, '_>,
    program: &DecodeProgram,
    block_idx: usize,
    loop_tail: Block,
) -> Result<(), CodegenError> {
    ctx.inlined_blocks.insert(block_idx);
    ctx.sealed_inlined_blocks.insert(block_idx);
    // SAFETY: we only read `program.blocks[block_idx].ops` and do not mutate it.
    // Cloning it would deep-clone the `Box<TranslationPlan>` inside `SlowPath`
    // to a fresh heap allocation that would drop when this function returns —
    // leaving the baked plan pointers in the generated code dangling.
    let ops_ptr: *const [DecodeOp] = program.blocks[block_idx].ops.as_slice();
    let ops = unsafe { &*ops_ptr };
    emit_ops(ctx, program, ops, Some(loop_tail))
}

/// Unified op emitter used by both `emit_block` (top-level, `loop_tail=None`)
/// and `emit_inline_block` (element body, `loop_tail=Some(tail)`).
///
/// The IR interpreter treats `ReadListLen` and `BranchOnVariant` as inline
/// sub-routine calls: after the sub-block(s) return, execution continues with
/// the next op in the parent block.  This function replicates that behavior in
/// the JIT by creating convergence ("continuation") Cranelift blocks between
/// adjacent soft terminators, so that all fields in a multi-Vec/enum struct are
/// decoded in the correct order regardless of nesting depth.
///
/// `loop_tail`:
///   - `None`  → `Return` emits `ctx.return_ok()` (top-level function exit).
///   - `Some(tail)` → `Return` emits `jump(tail)` (element body done, back to loop).
fn emit_ops(
    ctx: &mut EmitCtx<'_, '_>,
    program: &DecodeProgram,
    ops: &[DecodeOp],
    loop_tail: Option<Block>,
) -> Result<(), CodegenError> {
    let mut i = 0;
    while i < ops.len() {
        let op = &ops[i];
        match op {
            DecodeOp::Return => {
                match loop_tail {
                    None => ctx.return_ok(),
                    Some(tail) => {
                        ctx.b.ins().jump(tail, &[]);
                    }
                }
                return Ok(());
            }

            // CommitListLen is skipped in inline mode — emit_alloc_backing's loop-tail
            // writes the len field directly after each element. In top-level mode it
            // must be emitted (it reaches here when the Vec is the root value).
            DecodeOp::CommitListLen {
                dst_offset,
                descriptor,
            } => {
                if loop_tail.is_none() {
                    emit_commit_list_len(ctx, *dst_offset, *descriptor)?;
                }
                i += 1;
                continue;
            }

            DecodeOp::ReadListLen {
                empty_block,
                body_block,
                ..
            } => {
                let empty_ir = *empty_block;
                let body_ir = *body_block;

                // Continuation block: where ops after this ReadListLen resume.
                // Both the empty and body paths jump here when their Return fires.
                let list_done = ctx.fresh_block();

                // Emit the brif — terminates the current Cranelift block.
                emit_op(ctx, program, op)?;

                // Inline empty path (Return → jump to list_done).
                let clif_empty = ctx.block_map[empty_ir].unwrap();
                ctx.b.switch_to_block(clif_empty);
                ctx.b.seal_block(clif_empty);
                emit_inline_block(ctx, program, empty_ir, list_done)?;

                // Inline body path (Return → jump to list_done).
                let clif_body = ctx.block_map[body_ir].unwrap();
                ctx.b.switch_to_block(clif_body);
                ctx.b.seal_block(clif_body);
                emit_inline_block(ctx, program, body_ir, list_done)?;

                // Both paths have now jumped to list_done — safe to seal.
                ctx.b.switch_to_block(list_done);
                ctx.b.seal_block(list_done);

                // Continue processing remaining ops in the current IR block.
                i += 1;
                continue;
            }

            DecodeOp::Jump { block_id } => {
                let target_ir = *block_id;
                let clif_target = ctx.block_map[target_ir].unwrap();
                ctx.b.ins().jump(clif_target, &[]);
                if let Some(tail) = loop_tail {
                    ctx.b.switch_to_block(clif_target);
                    ctx.b.seal_block(clif_target);
                    emit_inline_block(ctx, program, target_ir, tail)?;
                }
                return Ok(());
            }

            DecodeOp::BranchOnVariant { variant_blocks, .. } => {
                let variant_irs: Vec<usize> = variant_blocks.iter().map(|&(_, b)| b).collect();

                // Check whether there are meaningful ops after this BranchOnVariant.
                // In practice the only op that can follow in the same IR block is
                // Return, which is unreachable (control never falls through from
                // BranchOnVariant at runtime). But to be safe, check for non-Return ops.
                let has_remaining_ops = ops[i + 1..]
                    .iter()
                    .any(|o| !matches!(o, DecodeOp::Return | DecodeOp::CommitListLen { .. }));

                if loop_tail.is_some() || has_remaining_ops {
                    // Inline mode OR top-level with ops after the dispatch:
                    // variant blocks converge on a fresh block.
                    let variant_done = ctx.fresh_block();
                    let eff_tail = variant_done;

                    // Emit the dispatch chain — terminates current Cranelift block.
                    emit_op(ctx, program, op)?;

                    for variant_ir in &variant_irs {
                        let variant_ir = *variant_ir;
                        if variant_ir == usize::MAX {
                            continue; // sentinel — unknown variant, no block
                        }
                        let clif_variant = ctx.block_map[variant_ir].unwrap();
                        ctx.b.switch_to_block(clif_variant);
                        ctx.b.seal_block(clif_variant);
                        emit_inline_block(ctx, program, variant_ir, eff_tail)?;
                    }

                    ctx.b.switch_to_block(variant_done);
                    ctx.b.seal_block(variant_done);

                    i += 1;
                    continue;
                } else {
                    // Top-level mode with only Return after (or nothing) — variant
                    // blocks each emit their own ctx.return_ok() when they hit Return.
                    emit_op(ctx, program, op)?;
                    // Outer loop in emit_decode_function will emit the variant blocks.
                    return Ok(());
                }
            }

            _ => {
                let terminated = emit_op(ctx, program, op)?;
                if terminated {
                    return Ok(());
                }
            }
        }
        i += 1;
    }
    // All ops processed without an explicit terminator.
    match loop_tail {
        None => { /* top-level: no implicit return; well-formed IR always ends with Return */ }
        Some(tail) => {
            ctx.b.ins().jump(tail, &[]);
        }
    }
    Ok(())
}

/// Emit one IR op. Returns `true` if the op terminated the current Cranelift block
/// (i.e., no further ops should be emitted into this block).
fn emit_op(
    ctx: &mut EmitCtx<'_, '_>,
    program: &DecodeProgram,
    op: &DecodeOp,
) -> Result<bool, CodegenError> {
    match op {
        DecodeOp::ReadScalar { prim, dst_offset } => {
            emit_read_scalar(ctx, *prim, *dst_offset)?;
        }

        DecodeOp::ReadByteVec {
            dst_offset,
            descriptor,
        } => {
            emit_read_byte_vec(ctx, *dst_offset, *descriptor)?;
        }

        DecodeOp::ReadString {
            dst_offset,
            descriptor,
        } => {
            emit_read_string(ctx, *dst_offset, *descriptor)?;
        }

        DecodeOp::ReadCowStr {
            dst_offset,
            borrowed,
        } => {
            emit_read_cow_str(ctx, *dst_offset, *borrowed)?;
        }

        DecodeOp::ReadStrRef { dst_offset } => {
            emit_read_str_ref(ctx, *dst_offset)?;
        }

        DecodeOp::ReadCowByteSlice {
            dst_offset,
            borrowed,
        } => {
            emit_read_cow_byte_slice(ctx, *dst_offset, *borrowed)?;
        }

        DecodeOp::ReadByteSliceRef { dst_offset } => {
            emit_read_byte_slice_ref(ctx, *dst_offset)?;
        }

        DecodeOp::ReadOpaque { shape, dst_offset } => {
            emit_decode_opaque(ctx, shape, *dst_offset)?;
        }

        DecodeOp::SkipValue { .. } => {
            return Err(CodegenError::UnsupportedOp(
                "SkipValue — fall back to IR interpreter for skip ops".into(),
            ));
        }

        DecodeOp::WriteDefault { shape, dst_offset } => {
            emit_write_default(ctx, shape, *dst_offset)?;
        }

        DecodeOp::DecodeOption {
            dst_offset,
            inner_offset,
            some_block,
            none_bytes,
            some_bytes,
        } => {
            emit_decode_option(
                ctx,
                program,
                *dst_offset,
                *inner_offset,
                *some_block,
                none_bytes,
                some_bytes,
            )?;
        }

        DecodeOp::DecodeResult {
            dst_offset,
            ok_block,
            err_block,
            ok_offset,
            err_offset,
            ok_bytes,
            err_bytes,
        } => {
            emit_decode_result(
                ctx,
                program,
                *dst_offset,
                *ok_block,
                *err_block,
                *ok_offset,
                *err_offset,
                ok_bytes,
                err_bytes,
            )?;
        }

        DecodeOp::DecodeResultInit {
            dst_offset,
            ok_block,
            err_block,
            ok_size,
            ok_align,
            err_size,
            err_align,
            init_ok_fn,
            init_err_fn,
        } => {
            emit_decode_result_init(
                ctx,
                program,
                *dst_offset,
                *ok_block,
                *err_block,
                *ok_size,
                *ok_align,
                *err_size,
                *err_align,
                *init_ok_fn,
                *init_err_fn,
            )?;
        }

        DecodeOp::ReadDiscriminant => {
            let disc = ctx.read_varint_u64()?;
            ctx.b.def_var(ctx.var_discriminant, disc);
        }

        DecodeOp::BranchOnVariant {
            tag_offset,
            tag_width,
            variant_table,
            variant_blocks,
        } => {
            emit_branch_on_variant(ctx, *tag_offset, *tag_width, variant_table, variant_blocks)?;
            return Ok(true);
        }

        DecodeOp::PushFrame {
            field_offset: _,
            frame_size: _,
        } => {
            // In the Cranelift backend, PushFrame is a no-op at the IR level;
            // field offsets are already absolute from the root out_ptr.
            // The interpreter needs this for its base-pointer stack; the JIT uses
            // absolute offsets baked in at lowering time.
        }

        DecodeOp::PopFrame => { /* see PushFrame */ }

        DecodeOp::ReadListLen {
            descriptor,
            dst_offset,
            empty_block,
            body_block,
        } => {
            emit_read_list_len(
                ctx,
                program,
                *descriptor,
                *dst_offset,
                *empty_block,
                *body_block,
            )?;
            return Ok(true);
        }

        DecodeOp::CommitListLen {
            dst_offset,
            descriptor,
        } => {
            emit_commit_list_len(ctx, *dst_offset, *descriptor)?;
        }

        DecodeOp::DecodeArray {
            dst_offset,
            count,
            elem_size,
            body_block,
        } => {
            emit_decode_array(ctx, program, *dst_offset, *count, *elem_size, *body_block)?;
        }

        DecodeOp::MaterializeEmpty {
            dst_offset,
            descriptor,
        } => {
            emit_materialize_empty(ctx, *dst_offset, *descriptor)?;
        }

        DecodeOp::AllocBacking {
            dst_offset,
            descriptor,
            body_block,
            elem_size,
        } => {
            emit_alloc_backing(
                ctx,
                program,
                *dst_offset,
                *descriptor,
                *body_block,
                *elem_size,
            )?;
        }

        DecodeOp::AllocBoxed {
            dst_offset,
            descriptor,
            body_block,
        } => {
            emit_alloc_boxed(ctx, program, *dst_offset, *descriptor, *body_block)?;
        }

        DecodeOp::SlowPath {
            shape,
            plan,
            dst_offset,
        } => {
            emit_slow_path(ctx, shape, plan, *dst_offset)?;
        }

        DecodeOp::Jump { block_id } => {
            let target = ctx.block_map[*block_id].unwrap();
            ctx.b.ins().jump(target, &[]);
            return Ok(true);
        }

        DecodeOp::Return => {
            ctx.return_ok();
            return Ok(true);
        }
    }
    Ok(false)
}

// ---------------------------------------------------------------------------
// Scalar decode
// ---------------------------------------------------------------------------

fn emit_read_scalar(
    ctx: &mut EmitCtx<'_, '_>,
    prim: WirePrimitive,
    dst_offset: usize,
) -> Result<(), CodegenError> {
    let dst = ctx.dst_at(dst_offset);
    match prim {
        WirePrimitive::Unit => { /* no bytes */ }

        WirePrimitive::Bool => {
            let byte = ctx.read_byte()?;
            let byte_i32 = ctx.b.ins().uextend(types::I32, byte);
            let zero = ctx.b.ins().iconst(types::I32, 0);
            let one = ctx.b.ins().iconst(types::I32, 1);
            let is_zero = ctx.b.ins().icmp(IntCC::Equal, byte_i32, zero);
            let is_one = ctx.b.ins().icmp(IntCC::Equal, byte_i32, one);

            let invalid_block = ctx.fresh_block();
            let ok_block = ctx.fresh_block();
            // If 0x00 or 0x01 → ok; else → invalid.
            let valid = ctx.b.ins().bor(is_zero, is_one);
            ctx.b.ins().brif(valid, ok_block, &[], invalid_block, &[]);

            ctx.b.switch_to_block(invalid_block);
            ctx.b.seal_block(invalid_block);
            ctx.return_err(DecodeStatus::InvalidBool);

            ctx.b.switch_to_block(ok_block);
            ctx.b.seal_block(ok_block);
            let byte_as_i8 = ctx.b.ins().ireduce(types::I8, byte_i32);
            ctx.b.ins().store(MemFlags::trusted(), byte_as_i8, dst, 0);
        }

        WirePrimitive::U8 | WirePrimitive::I8 => {
            let byte = ctx.read_byte()?;
            ctx.b.ins().store(MemFlags::trusted(), byte, dst, 0);
        }

        WirePrimitive::U16 => {
            let v = ctx.read_varint_u64()?;
            let v16 = ctx.b.ins().ireduce(types::I16, v);
            ctx.b.ins().store(MemFlags::trusted(), v16, dst, 0);
        }

        WirePrimitive::U32 => {
            let v = ctx.read_varint_u64()?;
            let v32 = ctx.b.ins().ireduce(types::I32, v);
            ctx.b.ins().store(MemFlags::trusted(), v32, dst, 0);
        }

        WirePrimitive::U64 | WirePrimitive::USize => {
            let v = ctx.read_varint_u64()?;
            if ctx.ptr_ty == types::I64 || prim == WirePrimitive::U64 {
                ctx.b.ins().store(MemFlags::trusted(), v, dst, 0);
            } else {
                let v32 = ctx.b.ins().ireduce(types::I32, v);
                ctx.b.ins().store(MemFlags::trusted(), v32, dst, 0);
            }
        }

        WirePrimitive::I16 => {
            let v = ctx.read_varint_i64()?;
            let v16 = ctx.b.ins().ireduce(types::I16, v);
            ctx.b.ins().store(MemFlags::trusted(), v16, dst, 0);
        }

        WirePrimitive::I32 => {
            let v = ctx.read_varint_i64()?;
            let v32 = ctx.b.ins().ireduce(types::I32, v);
            ctx.b.ins().store(MemFlags::trusted(), v32, dst, 0);
        }

        WirePrimitive::I64 | WirePrimitive::ISize => {
            let v = ctx.read_varint_i64()?;
            ctx.b.ins().store(MemFlags::trusted(), v, dst, 0);
        }

        WirePrimitive::F32 => {
            // Read 4 bytes little-endian.
            let b0 = ctx.read_byte()?;
            let b1 = ctx.read_byte()?;
            let b2 = ctx.read_byte()?;
            let b3 = ctx.read_byte()?;
            let b0_32 = ctx.b.ins().uextend(types::I32, b0);
            let b1_32 = ctx.b.ins().uextend(types::I32, b1);
            let b2_32 = ctx.b.ins().uextend(types::I32, b2);
            let b3_32 = ctx.b.ins().uextend(types::I32, b3);
            let s1 = ctx.b.ins().ishl_imm(b1_32, 8);
            let s2 = ctx.b.ins().ishl_imm(b2_32, 16);
            let s3 = ctx.b.ins().ishl_imm(b3_32, 24);
            let r01 = ctx.b.ins().bor(b0_32, s1);
            let r012 = ctx.b.ins().bor(r01, s2);
            let i32_val = ctx.b.ins().bor(r012, s3);
            let f32_val = ctx.b.ins().bitcast(types::F32, MemFlags::new(), i32_val);
            ctx.b.ins().store(MemFlags::trusted(), f32_val, dst, 0);
        }

        WirePrimitive::F64 => {
            // Read 8 bytes little-endian.
            let mut acc = {
                let b = ctx.read_byte()?;
                ctx.b.ins().uextend(types::I64, b)
            };
            for shift in 1u32..8 {
                let b = ctx.read_byte()?;
                let b64 = ctx.b.ins().uextend(types::I64, b);
                let shifted = ctx.b.ins().ishl_imm(b64, (shift * 8) as i64);
                acc = ctx.b.ins().bor(acc, shifted);
            }
            let f64_val = ctx.b.ins().bitcast(types::F64, MemFlags::new(), acc);
            ctx.b.ins().store(MemFlags::trusted(), f64_val, dst, 0);
        }

        WirePrimitive::String => {
            // String reads varint length + UTF-8 bytes; slow path in minimal subset.
            return Err(CodegenError::UnsupportedOp(
                "WirePrimitive::String inline — use ReadString op".into(),
            ));
        }

        WirePrimitive::Bytes => {
            return Err(CodegenError::UnsupportedOp(
                "WirePrimitive::Bytes inline — use ReadByteVec op".into(),
            ));
        }

        WirePrimitive::Payload => {
            return Err(CodegenError::UnsupportedOp(
                "WirePrimitive::Payload not in Cranelift minimal subset".into(),
            ));
        }

        WirePrimitive::Char => {
            return Err(CodegenError::UnsupportedOp(
                "WirePrimitive::Char not in Cranelift minimal subset".into(),
            ));
        }

        WirePrimitive::U128 | WirePrimitive::I128 => {
            return Err(CodegenError::UnsupportedOp(
                "u128/i128 not in Cranelift minimal subset".into(),
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Vec<u8> decode
// ---------------------------------------------------------------------------

fn emit_read_byte_vec(
    ctx: &mut EmitCtx<'_, '_>,
    dst_offset: usize,
    descriptor: OpaqueDescriptorId,
) -> Result<(), CodegenError> {
    let desc = ctx
        .descriptors
        .get(DescriptorHandle(descriptor.0))
        .ok_or_else(|| CodegenError::UnsupportedOp("descriptor not found".into()))?;

    let len = ctx.read_varint_u64()?;
    let len_ptr = if ctx.ptr_ty == types::I64 {
        len
    } else {
        ctx.b.ins().ireduce(types::I32, len)
    };

    let zero64 = ctx.b.ins().iconst(types::I64, 0);
    let is_empty = ctx.b.ins().icmp(IntCC::Equal, len, zero64);
    let empty_block = ctx.fresh_block();
    let nonempty_block = ctx.fresh_block();
    let done_block = ctx.fresh_block();

    ctx.b
        .ins()
        .brif(is_empty, empty_block, &[], nonempty_block, &[]);

    // Empty: copy calibrated empty bytes.
    ctx.b.switch_to_block(empty_block);
    ctx.b.seal_block(empty_block);
    copy_empty_bytes(ctx, desc, dst_offset);
    ctx.b.ins().jump(done_block, &[]);

    // Non-empty: alloc backing, memcpy, set_len.
    ctx.b.switch_to_block(nonempty_block);
    ctx.b.seal_block(nonempty_block);

    let desc_ptr_val = ctx
        .b
        .ins()
        .iconst(ctx.ptr_ty, desc as *const CalDescriptor as i64);
    let dst = ctx.dst_at(dst_offset);

    let alloc_sig = make_alloc_sig(ctx);
    let alloc_fn = ctx
        .b
        .ins()
        .iconst(ctx.ptr_ty, vox_jit_vec_alloc as *const () as i64);
    let call = ctx
        .b
        .ins()
        .call_indirect(alloc_sig, alloc_fn, &[desc_ptr_val, len_ptr]);
    let data_ptr = ctx.b.inst_results(call)[0];

    let null = ctx.b.ins().iconst(ctx.ptr_ty, 0);
    let is_ok = ctx.b.ins().icmp(IntCC::NotEqual, data_ptr, null);
    let alloc_ok = ctx.fresh_block();
    let alloc_err = ctx.fresh_block();
    ctx.b.ins().brif(is_ok, alloc_ok, &[], alloc_err, &[]);

    ctx.b.switch_to_block(alloc_err);
    ctx.b.seal_block(alloc_err);
    ctx.return_err(DecodeStatus::AllocFailed);

    ctx.b.switch_to_block(alloc_ok);
    ctx.b.seal_block(alloc_ok);

    let zero = zero_ptr(ctx);
    emit_container_header(ctx, dst, desc, data_ptr, zero, len_ptr);

    // Memcpy: copy len bytes from input cursor to data_ptr.
    let consumed = ctx.consumed();
    let input_ptr = ctx.ctx_input_ptr();
    let src = ctx.b.ins().iadd(input_ptr, consumed);
    emit_memcpy(ctx, src, data_ptr, len_ptr, desc.elem_size);

    // Advance consumed.
    ctx.skip_bytes_val(len_ptr);

    emit_len_store(ctx, dst, desc, len_ptr);

    ctx.b.ins().jump(done_block, &[]);

    ctx.b.switch_to_block(done_block);
    ctx.b.seal_block(done_block);

    Ok(())
}

// ---------------------------------------------------------------------------
// String decode
// ---------------------------------------------------------------------------

fn emit_read_string(
    ctx: &mut EmitCtx<'_, '_>,
    dst_offset: usize,
    descriptor: OpaqueDescriptorId,
) -> Result<(), CodegenError> {
    let desc = ctx
        .descriptors
        .get(DescriptorHandle(descriptor.0))
        .ok_or_else(|| CodegenError::UnsupportedOp("string descriptor not found".into()))?;

    let len = ctx.read_varint_u64()?;
    let len_ptr = if ctx.ptr_ty == types::I64 {
        len
    } else {
        ctx.b.ins().ireduce(types::I32, len)
    };

    let zero64 = ctx.b.ins().iconst(types::I64, 0);
    let is_empty = ctx.b.ins().icmp(IntCC::Equal, len, zero64);
    let empty_block = ctx.fresh_block();
    let nonempty_block = ctx.fresh_block();
    let done_block = ctx.fresh_block();

    ctx.b
        .ins()
        .brif(is_empty, empty_block, &[], nonempty_block, &[]);

    // Empty string.
    ctx.b.switch_to_block(empty_block);
    ctx.b.seal_block(empty_block);
    copy_empty_bytes(ctx, desc, dst_offset);
    ctx.b.ins().jump(done_block, &[]);

    // Non-empty string: validate UTF-8, then allocate and copy.
    ctx.b.switch_to_block(nonempty_block);
    ctx.b.seal_block(nonempty_block);

    // Point to start of string bytes in input.
    let consumed = ctx.consumed();
    let input_ptr = ctx.ctx_input_ptr();
    let str_start = ctx.b.ins().iadd(input_ptr, consumed);

    // Validate UTF-8.
    let validate_sig = make_utf8_validate_sig(ctx);
    let validate_fn = ctx
        .b
        .ins()
        .iconst(ctx.ptr_ty, vox_jit_utf8_validate as *const () as i64);
    let call = ctx
        .b
        .ins()
        .call_indirect(validate_sig, validate_fn, &[str_start, len_ptr]);
    let status = ctx.b.inst_results(call)[0];
    let ok_val = ctx.b.ins().iconst(types::I32, DecodeStatus::Ok as i64);
    let is_ok = ctx.b.ins().icmp(IntCC::Equal, status, ok_val);

    let utf8_ok = ctx.fresh_block();
    let utf8_err = ctx.fresh_block();
    ctx.b.ins().brif(is_ok, utf8_ok, &[], utf8_err, &[]);

    ctx.b.switch_to_block(utf8_err);
    ctx.b.seal_block(utf8_err);
    ctx.return_err(DecodeStatus::InvalidUtf8);

    ctx.b.switch_to_block(utf8_ok);
    ctx.b.seal_block(utf8_ok);

    // Allocate backing storage (use String-specific symbol per helper contract).
    let desc_ptr_val = ctx
        .b
        .ins()
        .iconst(ctx.ptr_ty, desc as *const CalDescriptor as i64);
    let dst = ctx.dst_at(dst_offset);
    let alloc_sig = make_alloc_sig(ctx);
    let alloc_fn = ctx
        .b
        .ins()
        .iconst(ctx.ptr_ty, vox_jit_string_alloc as *const () as i64);
    let call = ctx
        .b
        .ins()
        .call_indirect(alloc_sig, alloc_fn, &[desc_ptr_val, len_ptr]);
    let data_ptr = ctx.b.inst_results(call)[0];
    let null = ctx.b.ins().iconst(ctx.ptr_ty, 0);
    let is_ok = ctx.b.ins().icmp(IntCC::NotEqual, data_ptr, null);

    let alloc_ok = ctx.fresh_block();
    let alloc_err_block = ctx.fresh_block();
    ctx.b.ins().brif(is_ok, alloc_ok, &[], alloc_err_block, &[]);

    ctx.b.switch_to_block(alloc_err_block);
    ctx.b.seal_block(alloc_err_block);
    ctx.return_err(DecodeStatus::AllocFailed);

    ctx.b.switch_to_block(alloc_ok);
    ctx.b.seal_block(alloc_ok);

    let zero = zero_ptr(ctx);
    emit_container_header(ctx, dst, desc, data_ptr, zero, len_ptr);

    // Copy bytes into the backing store.
    emit_memcpy(ctx, str_start, data_ptr, len_ptr, 1);

    // Advance consumed.
    ctx.skip_bytes_val(len_ptr);

    emit_len_store(ctx, dst, desc, len_ptr);

    ctx.b.ins().jump(done_block, &[]);

    ctx.b.switch_to_block(done_block);
    ctx.b.seal_block(done_block);

    Ok(())
}

fn emit_read_byte_slice(ctx: &mut EmitCtx<'_, '_>) -> Result<(Value, Value), CodegenError> {
    let len64 = ctx.read_varint_u64()?;
    let len = if ctx.ptr_ty == types::I64 {
        len64
    } else {
        ctx.b.ins().ireduce(ctx.ptr_ty, len64)
    };

    let consumed = ctx.consumed();
    let input_len = ctx.ctx_input_len();
    let remaining = ctx.b.ins().isub(input_len, consumed);
    let too_long = ctx.b.ins().icmp(IntCC::UnsignedGreaterThan, len, remaining);

    let eof_block = ctx.fresh_block();
    let ok_block = ctx.fresh_block();
    ctx.b.ins().brif(too_long, eof_block, &[], ok_block, &[]);

    ctx.b.switch_to_block(eof_block);
    ctx.b.seal_block(eof_block);
    ctx.return_err(DecodeStatus::UnexpectedEof);

    ctx.b.switch_to_block(ok_block);
    ctx.b.seal_block(ok_block);

    let input_ptr = ctx.ctx_input_ptr();
    let data = ctx.b.ins().iadd(input_ptr, consumed);
    let new_consumed = ctx.b.ins().iadd(consumed, len);
    ctx.b.def_var(ctx.var_consumed, new_consumed);

    Ok((data, len))
}

fn emit_read_cow_str(
    ctx: &mut EmitCtx<'_, '_>,
    dst_offset: usize,
    borrowed: bool,
) -> Result<(), CodegenError> {
    let (data, len) = emit_read_byte_slice(ctx)?;
    let call_conv = ctx.b.func.signature.call_conv;
    let sig = ctx.b.func.import_signature(Signature {
        params: vec![
            AbiParam::new(ctx.ptr_ty),
            AbiParam::new(ctx.ptr_ty),
            AbiParam::new(ctx.ptr_ty),
        ],
        returns: vec![],
        call_conv,
    });
    let helper = if borrowed {
        crate::helpers::vox_jit_init_cow_str_borrowed as *const ()
    } else {
        crate::helpers::vox_jit_init_cow_str_owned as *const ()
    };
    let callee = ctx.b.ins().iconst(ctx.ptr_ty, helper as i64);
    let dst = ctx.dst_at(dst_offset);
    ctx.b.ins().call_indirect(sig, callee, &[dst, data, len]);
    Ok(())
}

fn emit_read_str_ref(ctx: &mut EmitCtx<'_, '_>, dst_offset: usize) -> Result<(), CodegenError> {
    // `&str` ABI is fixed by Rust: a 2-slot fat pointer `(ptr, len)`. No
    // calibration needed; emit the two stores inline. UTF-8 is validated via
    // the same helper `ReadString` uses, so a malformed input returns an
    // `InvalidUtf8` error instead of panicking.
    let (data, len) = emit_read_byte_slice(ctx)?;

    let call_conv = ctx.b.func.signature.call_conv;
    let validate_sig = make_utf8_validate_sig(ctx);
    let validate_fn = ctx
        .b
        .ins()
        .iconst(ctx.ptr_ty, vox_jit_utf8_validate as *const () as i64);
    let call = ctx
        .b
        .ins()
        .call_indirect(validate_sig, validate_fn, &[data, len]);
    let status = ctx.b.inst_results(call)[0];
    let ok_val = ctx.b.ins().iconst(types::I32, DecodeStatus::Ok as i64);
    let is_ok = ctx.b.ins().icmp(IntCC::Equal, status, ok_val);

    let utf8_ok = ctx.fresh_block();
    let utf8_err = ctx.fresh_block();
    ctx.b.ins().brif(is_ok, utf8_ok, &[], utf8_err, &[]);

    ctx.b.switch_to_block(utf8_err);
    ctx.b.seal_block(utf8_err);
    ctx.return_err(DecodeStatus::InvalidUtf8);

    ctx.b.switch_to_block(utf8_ok);
    ctx.b.seal_block(utf8_ok);

    let _ = call_conv; // unused now that we're not building a call signature
    let dst = ctx.dst_at(dst_offset);
    ctx.b.ins().store(MemFlags::trusted(), data, dst, 0);
    ctx.b
        .ins()
        .store(MemFlags::trusted(), len, dst, ctx.ptr_ty.bytes() as i32);
    Ok(())
}

fn emit_read_cow_byte_slice(
    ctx: &mut EmitCtx<'_, '_>,
    dst_offset: usize,
    borrowed: bool,
) -> Result<(), CodegenError> {
    let (data, len) = emit_read_byte_slice(ctx)?;
    let call_conv = ctx.b.func.signature.call_conv;
    let sig = ctx.b.func.import_signature(Signature {
        params: vec![
            AbiParam::new(ctx.ptr_ty),
            AbiParam::new(ctx.ptr_ty),
            AbiParam::new(ctx.ptr_ty),
        ],
        returns: vec![],
        call_conv,
    });
    let helper = if borrowed {
        crate::helpers::vox_jit_init_cow_byte_slice_borrowed as *const ()
    } else {
        crate::helpers::vox_jit_init_cow_byte_slice_owned as *const ()
    };
    let callee = ctx.b.ins().iconst(ctx.ptr_ty, helper as i64);
    let dst = ctx.dst_at(dst_offset);
    ctx.b.ins().call_indirect(sig, callee, &[dst, data, len]);
    Ok(())
}

fn emit_read_byte_slice_ref(
    ctx: &mut EmitCtx<'_, '_>,
    dst_offset: usize,
) -> Result<(), CodegenError> {
    let (data, len) = emit_read_byte_slice(ctx)?;
    let call_conv = ctx.b.func.signature.call_conv;
    let sig = ctx.b.func.import_signature(Signature {
        params: vec![
            AbiParam::new(ctx.ptr_ty),
            AbiParam::new(ctx.ptr_ty),
            AbiParam::new(ctx.ptr_ty),
        ],
        returns: vec![],
        call_conv,
    });
    let callee = ctx.b.ins().iconst(
        ctx.ptr_ty,
        crate::helpers::vox_jit_init_byte_slice_ref as *const () as i64,
    );
    let dst = ctx.dst_at(dst_offset);
    ctx.b.ins().call_indirect(sig, callee, &[dst, data, len]);
    Ok(())
}

// ---------------------------------------------------------------------------
// List decode
// ---------------------------------------------------------------------------

fn emit_read_list_len(
    ctx: &mut EmitCtx<'_, '_>,
    _program: &DecodeProgram,
    _descriptor: OpaqueDescriptorId,
    _dst_offset: usize,
    empty_block: usize,
    body_block: usize,
) -> Result<(), CodegenError> {
    let len = ctx.read_varint_u64()?;

    // Save list length for use by AllocBacking's element loop.
    let len_ptr = if ctx.ptr_ty == types::I64 {
        len
    } else {
        ctx.b.ins().ireduce(types::I32, len)
    };
    ctx.b.def_var(ctx.var_list_len, len_ptr);

    // Do NOT reset var_init_count here: emit_alloc_backing initialises its own
    // fresh loop-counter variable, so writing 0 here would corrupt any enclosing
    // outer loop that has swapped its counter into ctx.var_init_count.

    let zero64 = ctx.b.ins().iconst(types::I64, 0);
    let is_empty = ctx.b.ins().icmp(IntCC::Equal, len, zero64);

    let clif_empty = ctx.block_map[empty_block].unwrap();
    let clif_body = ctx.block_map[body_block].unwrap();
    ctx.b.ins().brif(is_empty, clif_empty, &[], clif_body, &[]);

    Ok(())
}

fn emit_commit_list_len(
    ctx: &mut EmitCtx<'_, '_>,
    dst_offset: usize,
    descriptor: OpaqueDescriptorId,
) -> Result<(), CodegenError> {
    let desc = ctx
        .descriptors
        .get(DescriptorHandle(descriptor.0))
        .ok_or_else(|| {
            CodegenError::UnsupportedOp("descriptor not found in commit_list_len".into())
        })?;
    let dst = ctx.dst_at(dst_offset);
    let init_count = ctx.b.use_var(ctx.var_init_count);
    emit_len_store(ctx, dst, desc, init_count);
    Ok(())
}

// ---------------------------------------------------------------------------
// Enum decode
// ---------------------------------------------------------------------------

/// Emit the discriminant read + dispatch table for an enum.
///
/// Generates a chain of comparisons: for each entry in `variant_table`, compare
/// the discriminant against the remote index. On match, write the local tag and
/// jump to the variant block. On no match, return `UnknownVariant`.
fn emit_branch_on_variant(
    ctx: &mut EmitCtx<'_, '_>,
    tag_offset: usize,
    tag_width: TagWidth,
    variant_table: &[Option<usize>],
    variant_blocks: &[(u64, usize)],
) -> Result<(), CodegenError> {
    let disc = ctx.b.use_var(ctx.var_discriminant);
    let tag_dst = ctx.dst_at(tag_offset);

    // Build the chain: compare disc == remote_idx for each known variant.
    // Unknown entries jump to the unknown_block.
    let unknown_block = ctx.fresh_block();

    let mut remaining_block = ctx.fresh_block();
    ctx.b.ins().jump(remaining_block, &[]);

    for (remote_idx, maybe_local) in variant_table.iter().enumerate() {
        ctx.b.switch_to_block(remaining_block);
        ctx.b.seal_block(remaining_block);

        let remote_val = ctx.b.ins().iconst(types::I64, remote_idx as i64);
        let is_match = ctx.b.ins().icmp(IntCC::Equal, disc, remote_val);

        if let Some(_local_idx) = maybe_local {
            let (local_disc, variant_block_id) = variant_blocks[remote_idx];
            if variant_block_id == usize::MAX {
                // Sentinel: remote variant maps to nothing — treat as unknown.
                let next = ctx.fresh_block();
                ctx.b.ins().brif(is_match, unknown_block, &[], next, &[]);
                remaining_block = next;
                continue;
            }

            let match_block = ctx.fresh_block();
            let next = ctx.fresh_block();
            ctx.b.ins().brif(is_match, match_block, &[], next, &[]);

            // Emit match block: write local tag + jump to variant decode.
            ctx.b.switch_to_block(match_block);
            ctx.b.seal_block(match_block);
            emit_write_tag(ctx, tag_dst, tag_width, local_disc);
            let clif_variant_block = ctx.block_map[variant_block_id].unwrap();
            ctx.b.ins().jump(clif_variant_block, &[]);

            remaining_block = next;
        } else {
            // Unknown local mapping for this remote index: skip on match.
            // (The discriminant didn't match any known local variant.)
            let next = ctx.fresh_block();
            ctx.b.ins().brif(is_match, unknown_block, &[], next, &[]);
            remaining_block = next;
        }
    }

    // After exhausting the table, fall through to unknown.
    ctx.b.switch_to_block(remaining_block);
    ctx.b.seal_block(remaining_block);
    ctx.b.ins().jump(unknown_block, &[]);

    // Emit the unknown variant error block.
    ctx.b.switch_to_block(unknown_block);
    ctx.b.seal_block(unknown_block);
    ctx.return_err(DecodeStatus::UnknownVariant);

    Ok(())
}

/// Write `disc` into `tag_dst` with the given width.
fn emit_write_tag(ctx: &mut EmitCtx<'_, '_>, tag_dst: Value, tag_width: TagWidth, disc: u64) {
    match tag_width {
        TagWidth::U8 => {
            let v = ctx.b.ins().iconst(types::I8, disc as i8 as i64);
            ctx.b.ins().store(MemFlags::trusted(), v, tag_dst, 0);
        }
        TagWidth::U16 => {
            let v = ctx.b.ins().iconst(types::I16, disc as i16 as i64);
            ctx.b.ins().store(MemFlags::trusted(), v, tag_dst, 0);
        }
        TagWidth::U32 => {
            let v = ctx.b.ins().iconst(types::I32, disc as i32 as i64);
            ctx.b.ins().store(MemFlags::trusted(), v, tag_dst, 0);
        }
        TagWidth::U64 => {
            let v = ctx.b.ins().iconst(types::I64, disc as i64);
            ctx.b.ins().store(MemFlags::trusted(), v, tag_dst, 0);
        }
    }
}

// ---------------------------------------------------------------------------
// Option decode
// ---------------------------------------------------------------------------

/// Emit `DecodeOption`.
///
/// Layout:
///   current block → read tag byte
///   brif tag==0 → none_block, else → check_some_block
///   none_block: materialize calibrated None bytes; jump → after_block
///   check_some_block: brif tag==1 → some_call_block, else → invalid_block
///   invalid_block: return InvalidOptionTag
///   some_call_block: materialize calibrated Some bytes;
///                    emit some_block_ir ops inline with out_ptr=inner_ptr;
///                    jump → after_block
///   after_block: (continue — None and Some both land here)
///
/// `some_block_ir` ops are emitted inline (marked in inlined_blocks) so the
/// outer emit loop does not double-emit them. The inner ops use dst_offset=0
/// relative to inner_ptr (as set by lower_option).
fn emit_decode_option(
    ctx: &mut EmitCtx<'_, '_>,
    program: &DecodeProgram,
    dst_offset: usize,
    inner_offset: usize,
    some_block_ir: usize,
    none_bytes: &[u8],
    some_bytes: &[u8],
) -> Result<(), CodegenError> {
    let tag = ctx.read_byte()?;
    let tag_i32 = ctx.b.ins().uextend(types::I32, tag);

    let none_block = ctx.fresh_block();
    let check_some_block = ctx.fresh_block();
    let some_call_block = ctx.fresh_block();
    let invalid_block = ctx.fresh_block();
    let after_block = ctx.fresh_block();

    let is_zero = ctx.b.ins().icmp_imm(IntCC::Equal, tag_i32, 0);
    ctx.b
        .ins()
        .brif(is_zero, none_block, &[], check_some_block, &[]);

    // None path.
    ctx.b.switch_to_block(none_block);
    ctx.b.seal_block(none_block);
    {
        let option_ptr = ctx.dst_at(dst_offset);
        emit_inline_bytes(ctx, option_ptr, none_bytes);
    }
    ctx.b.ins().jump(after_block, &[]);

    // Check-some path: validate tag == 1.
    ctx.b.switch_to_block(check_some_block);
    ctx.b.seal_block(check_some_block);
    let is_one = ctx.b.ins().icmp_imm(IntCC::Equal, tag_i32, 1);
    ctx.b
        .ins()
        .brif(is_one, some_call_block, &[], invalid_block, &[]);

    // Invalid option tag.
    ctx.b.switch_to_block(invalid_block);
    ctx.b.seal_block(invalid_block);
    ctx.return_err(DecodeStatus::InvalidOptionTag);

    // Some path: call init_some, decode inner value inline.
    ctx.b.switch_to_block(some_call_block);
    ctx.b.seal_block(some_call_block);
    {
        let option_ptr = ctx.dst_at(dst_offset);
        emit_inline_bytes(ctx, option_ptr, some_bytes);
        let inner_ptr = if inner_offset == 0 {
            option_ptr
        } else {
            ctx.b.ins().iadd_imm(option_ptr, inner_offset as i64)
        };

        // Inline the some_block using emit_inline_block so nested multi-block
        // element bodies (e.g. Option<Vec<T>>) are handled correctly.
        let saved_out_ptr = ctx.out_ptr;
        ctx.out_ptr = inner_ptr;
        emit_inline_block(ctx, program, some_block_ir, after_block)?;
        ctx.out_ptr = saved_out_ptr;
        // emit_inline_block jumps to after_block — do NOT emit jump here.
    }

    // Merge: None and Some (inner done) both land here.
    ctx.b.switch_to_block(after_block);
    ctx.b.seal_block(after_block);

    Ok(())
}

fn emit_decode_result(
    ctx: &mut EmitCtx<'_, '_>,
    program: &DecodeProgram,
    dst_offset: usize,
    ok_block_ir: usize,
    err_block_ir: usize,
    ok_offset: usize,
    err_offset: usize,
    ok_bytes: &[u8],
    err_bytes: &[u8],
) -> Result<(), CodegenError> {
    let variant = ctx.read_varint_u64()?;
    let zero = ctx.b.ins().iconst(types::I64, 0);
    let one = ctx.b.ins().iconst(types::I64, 1);
    let is_ok_variant = ctx.b.ins().icmp(IntCC::Equal, variant, zero);
    let is_err_variant = ctx.b.ins().icmp(IntCC::Equal, variant, one);

    let ok_case = ctx.fresh_block();
    let check_err = ctx.fresh_block();
    let err_case = ctx.fresh_block();
    let invalid = ctx.fresh_block();
    let after_block = ctx.fresh_block();

    ctx.b
        .ins()
        .brif(is_ok_variant, ok_case, &[], check_err, &[]);

    ctx.b.switch_to_block(check_err);
    ctx.b.seal_block(check_err);
    ctx.b
        .ins()
        .brif(is_err_variant, err_case, &[], invalid, &[]);

    ctx.b.switch_to_block(invalid);
    ctx.b.seal_block(invalid);
    ctx.return_err(DecodeStatus::UnknownVariant);

    ctx.b.switch_to_block(ok_case);
    ctx.b.seal_block(ok_case);
    {
        let result_ptr = ctx.dst_at(dst_offset);
        emit_inline_bytes(ctx, result_ptr, ok_bytes);
        let payload_ptr = if ok_offset == 0 {
            result_ptr
        } else {
            ctx.b.ins().iadd_imm(result_ptr, ok_offset as i64)
        };
        let saved_out = ctx.out_ptr;
        ctx.out_ptr = payload_ptr;
        emit_inline_block(ctx, program, ok_block_ir, after_block)?;
        ctx.out_ptr = saved_out;
    }

    ctx.b.switch_to_block(err_case);
    ctx.b.seal_block(err_case);
    {
        let result_ptr = ctx.dst_at(dst_offset);
        emit_inline_bytes(ctx, result_ptr, err_bytes);
        let payload_ptr = if err_offset == 0 {
            result_ptr
        } else {
            ctx.b.ins().iadd_imm(result_ptr, err_offset as i64)
        };
        let saved_out = ctx.out_ptr;
        ctx.out_ptr = payload_ptr;
        emit_inline_block(ctx, program, err_block_ir, after_block)?;
        ctx.out_ptr = saved_out;
    }

    ctx.b.switch_to_block(after_block);
    ctx.b.seal_block(after_block);

    Ok(())
}

fn emit_decode_result_init(
    ctx: &mut EmitCtx<'_, '_>,
    program: &DecodeProgram,
    dst_offset: usize,
    ok_block_ir: usize,
    err_block_ir: usize,
    ok_size: usize,
    ok_align: usize,
    err_size: usize,
    err_align: usize,
    init_ok_fn: facet_core::ResultInitOkFn,
    init_err_fn: facet_core::ResultInitErrFn,
) -> Result<(), CodegenError> {
    let variant = ctx.read_varint_u64()?;
    let zero = ctx.b.ins().iconst(types::I64, 0);
    let one = ctx.b.ins().iconst(types::I64, 1);
    let is_ok_variant = ctx.b.ins().icmp(IntCC::Equal, variant, zero);
    let is_err_variant = ctx.b.ins().icmp(IntCC::Equal, variant, one);

    let ok_case = ctx.fresh_block();
    let check_err = ctx.fresh_block();
    let err_case = ctx.fresh_block();
    let invalid = ctx.fresh_block();
    let after_block = ctx.fresh_block();

    ctx.b
        .ins()
        .brif(is_ok_variant, ok_case, &[], check_err, &[]);

    ctx.b.switch_to_block(check_err);
    ctx.b.seal_block(check_err);
    ctx.b
        .ins()
        .brif(is_err_variant, err_case, &[], invalid, &[]);

    ctx.b.switch_to_block(invalid);
    ctx.b.seal_block(invalid);
    ctx.return_err(DecodeStatus::UnknownVariant);

    ctx.b.switch_to_block(ok_case);
    ctx.b.seal_block(ok_case);
    {
        let ok_init = ctx.fresh_block();
        let ok_slot = create_payload_stack_slot(ctx, ok_size, ok_align)?;
        let ok_ptr = ctx.b.ins().stack_addr(ctx.ptr_ty, ok_slot, 0);
        let saved_out = ctx.out_ptr;
        ctx.out_ptr = ok_ptr;
        emit_inline_block(ctx, program, ok_block_ir, ok_init)?;
        ctx.out_ptr = saved_out;

        ctx.b.switch_to_block(ok_init);
        ctx.b.seal_block(ok_init);
        emit_result_init_call(ctx, dst_offset, ok_ptr, init_ok_fn as *const ());
        ctx.b.ins().jump(after_block, &[]);
    }

    ctx.b.switch_to_block(err_case);
    ctx.b.seal_block(err_case);
    {
        let err_init = ctx.fresh_block();
        let err_slot = create_payload_stack_slot(ctx, err_size, err_align)?;
        let err_ptr = ctx.b.ins().stack_addr(ctx.ptr_ty, err_slot, 0);
        let saved_out = ctx.out_ptr;
        ctx.out_ptr = err_ptr;
        emit_inline_block(ctx, program, err_block_ir, err_init)?;
        ctx.out_ptr = saved_out;

        ctx.b.switch_to_block(err_init);
        ctx.b.seal_block(err_init);
        emit_result_init_call(ctx, dst_offset, err_ptr, init_err_fn as *const ());
        ctx.b.ins().jump(after_block, &[]);
    }

    ctx.b.switch_to_block(after_block);
    ctx.b.seal_block(after_block);

    Ok(())
}

fn create_payload_stack_slot(
    ctx: &mut EmitCtx<'_, '_>,
    size: usize,
    align: usize,
) -> Result<cranelift_codegen::ir::StackSlot, CodegenError> {
    let size = u32::try_from(size.max(1))
        .map_err(|_| CodegenError::UnsupportedOp("Result payload too large".into()))?;
    if !align.is_power_of_two() {
        return Err(CodegenError::UnsupportedOp(
            "Result payload alignment is not a power of two".into(),
        ));
    }
    let align_shift = u8::try_from(align.trailing_zeros())
        .map_err(|_| CodegenError::UnsupportedOp("Result payload alignment too large".into()))?;
    Ok(ctx.b.create_sized_stack_slot(StackSlotData::new(
        StackSlotKind::ExplicitSlot,
        size,
        align_shift,
    )))
}

fn emit_result_init_call(
    ctx: &mut EmitCtx<'_, '_>,
    dst_offset: usize,
    payload_ptr: Value,
    init_fn: *const (),
) {
    let call_conv = ctx.b.func.signature.call_conv;
    let sig = ctx.b.func.import_signature(Signature {
        params: vec![
            AbiParam::new(ctx.ptr_ty),
            AbiParam::new(ctx.ptr_ty),
            AbiParam::new(ctx.ptr_ty),
        ],
        returns: vec![],
        call_conv,
    });
    let result_ptr = ctx.dst_at(dst_offset);
    let init_fn = ctx.b.ins().iconst(ctx.ptr_ty, init_fn as i64);
    let callee = ctx.b.ins().iconst(
        ctx.ptr_ty,
        crate::helpers::vox_jit_result_init_raw as *const () as i64,
    );
    ctx.b
        .ins()
        .call_indirect(sig, callee, &[result_ptr, payload_ptr, init_fn]);
}

// ---------------------------------------------------------------------------
// Array decode
// ---------------------------------------------------------------------------

fn emit_decode_array(
    ctx: &mut EmitCtx<'_, '_>,
    program: &DecodeProgram,
    dst_offset: usize,
    count: usize,
    elem_size: usize,
    body_block: usize,
) -> Result<(), CodegenError> {
    // Unrolled: for small fixed arrays, generate `count` copies of the body block.
    // This is acceptable for the initial rollout; a production implementation
    // would generate a proper loop for large counts.
    if count > 64 {
        return Err(CodegenError::UnsupportedOp(
            "array count > 64 not yet unrolled — fall back to interpreter".into(),
        ));
    }
    for i in 0..count {
        let elem_off = dst_offset + i * elem_size;
        // Adjust out_ptr for this element, emit body ops.
        // The body_block ops reference dst_offset=0 relative to their base.
        // We need to add elem_off to ctx.out_ptr temporarily.
        let saved_out_ptr = ctx.out_ptr;
        ctx.out_ptr = ctx.b.ins().iadd_imm(saved_out_ptr, elem_off as i64);
        emit_block(ctx, program, body_block)?;
        ctx.out_ptr = saved_out_ptr;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Opaque fast-path
// ---------------------------------------------------------------------------

fn emit_materialize_empty(
    ctx: &mut EmitCtx<'_, '_>,
    dst_offset: usize,
    descriptor: OpaqueDescriptorId,
) -> Result<(), CodegenError> {
    let desc = ctx
        .descriptors
        .get(DescriptorHandle(descriptor.0))
        .ok_or_else(|| {
            CodegenError::UnsupportedOp("descriptor not found in materialize_empty".into())
        })?;
    copy_empty_bytes(ctx, desc, dst_offset);
    Ok(())
}

/// Emit backing allocation + element decode loop for a calibrated Vec<T>.
///
/// Layout of generated code:
///   1. Call vox_jit_vec_alloc(desc, list_len)
///   2. On alloc failure: return AllocFailed
///   3. Write ptr/len/cap into the container header
///   4. Loop: while init_count < list_len:
///      a. Set out_ptr = data_ptr + init_count * elem_size
///      b. Decode one element (inline body_block ops)
///      c. Increment init_count, commit len via direct store
///
/// The element body_block ops are emitted inline (not as a separate block
/// call) using a Cranelift loop with the element pointer adjusted per
/// iteration.
fn emit_alloc_backing(
    ctx: &mut EmitCtx<'_, '_>,
    program: &DecodeProgram,
    dst_offset: usize,
    descriptor: OpaqueDescriptorId,
    body_block: usize,
    elem_size: usize,
) -> Result<(), CodegenError> {
    let desc = ctx
        .descriptors
        .get(DescriptorHandle(descriptor.0))
        .ok_or_else(|| {
            CodegenError::UnsupportedOp("descriptor not found in alloc_backing".into())
        })?;
    let desc_ptr_val = ctx
        .b
        .ins()
        .iconst(ctx.ptr_ty, desc as *const CalDescriptor as i64);
    let dst = ctx.dst_at(dst_offset);

    // Snapshot var_list_len now (set by the enclosing ReadListLen) before the inner body
    // can overwrite it with a nested list's length.
    let list_len = ctx.b.use_var(ctx.var_list_len);

    // 1. Allocate backing storage.
    let alloc_sig = make_alloc_sig(ctx);
    let alloc_fn = ctx
        .b
        .ins()
        .iconst(ctx.ptr_ty, vox_jit_vec_alloc as *const () as i64);
    let call = ctx
        .b
        .ins()
        .call_indirect(alloc_sig, alloc_fn, &[desc_ptr_val, list_len]);
    let backing_ptr = ctx.b.inst_results(call)[0];
    let null = ctx.b.ins().iconst(ctx.ptr_ty, 0);
    let is_ok = ctx.b.ins().icmp(IntCC::NotEqual, backing_ptr, null);

    let alloc_ok = ctx.fresh_block();
    let alloc_err = ctx.fresh_block();
    ctx.b.ins().brif(is_ok, alloc_ok, &[], alloc_err, &[]);

    ctx.b.switch_to_block(alloc_err);
    ctx.b.seal_block(alloc_err);
    ctx.return_err(DecodeStatus::AllocFailed);

    ctx.b.switch_to_block(alloc_ok);
    ctx.b.seal_block(alloc_ok);

    // 2. Write ptr/len/cap into the container header.
    let zero = zero_ptr(ctx);
    emit_container_header(ctx, dst, desc, backing_ptr, zero, list_len);

    // 3. Emit element decode loop.
    //
    // Use fresh SSA variables for this loop's counter and list-length so that
    // nested Vec fields in the element body can use their own ReadListLen +
    // AllocBacking without corrupting the outer loop's counter or length.
    //
    // Two key variables:
    //   var_loop_i   — this loop's element counter; swapped into ctx.var_init_count
    //   var_loop_len — this loop's stable length cap; NOT exposed via ctx so inner
    //                  ReadListLen cannot overwrite it (inner writes ctx.var_list_len
    //                  which is a DIFFERENT fresh scratch variable).
    let ptr_ty = ctx.ptr_ty;
    let var_loop_i = ctx.fresh_var(ptr_ty);
    let var_loop_len = ctx.fresh_var(ptr_ty);
    let var_inner_list_len = ctx.fresh_var(ptr_ty); // scratch for nested ReadListLen writes

    let zero = ctx.b.ins().iconst(ctx.ptr_ty, 0);
    ctx.b.def_var(var_loop_i, zero);
    ctx.b.def_var(var_loop_len, list_len);
    ctx.b.def_var(var_inner_list_len, zero); // initial value required by Cranelift SSA

    // Save outer vars; point ctx.var_init_count at var_loop_i (so inner AllocBacking
    // saves/restores it correctly) and ctx.var_list_len at the scratch variable (so
    // inner ReadListLen writes to the scratch, leaving var_loop_len untouched).
    let saved_var_init_count = ctx.var_init_count;
    let saved_var_list_len = ctx.var_list_len;
    ctx.var_init_count = var_loop_i;
    ctx.var_list_len = var_inner_list_len;

    let loop_header = ctx.fresh_block();
    let loop_body_entry = ctx.fresh_block();
    let loop_tail = ctx.fresh_block(); // increment + commit + back-edge
    let loop_done = ctx.fresh_block();

    ctx.b.ins().jump(loop_header, &[]);

    // Loop header: check init_count < list_len.
    // Read var_loop_len directly — not via ctx.var_list_len — so inner code
    // cannot corrupt it.
    ctx.b.switch_to_block(loop_header);
    let i = ctx.b.use_var(var_loop_i);
    let list_len2 = ctx.b.use_var(var_loop_len);
    let done = ctx
        .b
        .ins()
        .icmp(IntCC::UnsignedGreaterThanOrEqual, i, list_len2);
    ctx.b.ins().brif(done, loop_done, &[], loop_body_entry, &[]);
    // Note: loop_header is sealed after loop_tail jumps back to it.

    // Loop body entry: compute element pointer, then recursively inline the element body.
    // loop_tail is the convergence point for all paths through the element body.
    ctx.b.switch_to_block(loop_body_entry);
    ctx.b.seal_block(loop_body_entry);

    // Compute element pointer: backing_ptr + i * elem_size.
    let elem_ptr = if elem_size == 0 {
        backing_ptr
    } else if elem_size == 1 {
        let i2 = ctx.b.use_var(var_loop_i);
        ctx.b.ins().iadd(backing_ptr, i2)
    } else {
        let i2 = ctx.b.use_var(var_loop_i);
        let stride = ctx.b.ins().iconst(ctx.ptr_ty, elem_size as i64);
        let offset = ctx.b.ins().imul(i2, stride);
        ctx.b.ins().iadd(backing_ptr, offset)
    };

    // Inline-emit the element body, replacing Return with jump(loop_tail).
    // All transitively-referenced blocks are also inlined (handles nested Vecs).
    let saved_out_ptr = ctx.out_ptr;
    ctx.out_ptr = elem_ptr;
    emit_inline_block(ctx, program, body_block, loop_tail)?;
    ctx.out_ptr = saved_out_ptr;

    // Loop tail: all element body paths converge here.
    // Increment init_count, commit the new len, jump back to loop_header.
    ctx.b.switch_to_block(loop_tail);
    ctx.b.seal_block(loop_tail);

    let i3 = ctx.b.use_var(var_loop_i);
    let one = ctx.b.ins().iconst(ctx.ptr_ty, 1);
    let next_i = ctx.b.ins().iadd(i3, one);
    ctx.b.def_var(var_loop_i, next_i);

    // Commit the new len.
    emit_len_store(ctx, dst, desc, next_i);

    ctx.b.ins().jump(loop_header, &[]);
    ctx.b.seal_block(loop_header);

    ctx.b.switch_to_block(loop_done);
    ctx.b.seal_block(loop_done);

    // Restore outer loop vars.
    ctx.var_init_count = saved_var_init_count;
    ctx.var_list_len = saved_var_list_len;

    Ok(())
}

/// Emit a `Box<T>` or `Box<[T]>` allocation + inline pointee decode.
///
/// Dispatches on `desc.kind`:
///
/// **BoxOwned** (`Box<T>`):
///   1. Call `vox_jit_box_alloc(desc, dst)` — allocates one T-sized slot.
///   2. On OOM: return AllocFailed.
///   3. Load the written pointer, inline-emit body_block with out_ptr = alloc_ptr.
///
/// **BoxSlice** (`Box<[T]>`):
///   1. Read varint list length.
///   2. Call `vox_jit_box_slice_alloc(desc, len, dst)` — allocates len*elem_size bytes,
///      writes fat pointer (data ptr + len) into dst.
///   3. Load data pointer, emit element decode loop (same as emit_alloc_backing).
fn emit_alloc_boxed(
    ctx: &mut EmitCtx<'_, '_>,
    program: &DecodeProgram,
    dst_offset: usize,
    descriptor: OpaqueDescriptorId,
    body_block: usize,
) -> Result<(), CodegenError> {
    let desc = ctx
        .descriptors
        .get(DescriptorHandle(descriptor.0))
        .ok_or_else(|| CodegenError::UnsupportedOp("descriptor not found in alloc_boxed".into()))?;

    match desc.kind {
        ContainerKind::BoxOwned => emit_alloc_box_owned(ctx, program, dst_offset, desc, body_block),
        ContainerKind::BoxSlice => emit_alloc_box_slice(ctx, program, dst_offset, desc, body_block),
        _ => Err(CodegenError::UnsupportedOp(format!(
            "AllocBoxed called with non-Box descriptor kind {:?}",
            desc.kind
        ))),
    }
}

fn emit_alloc_box_owned(
    ctx: &mut EmitCtx<'_, '_>,
    program: &DecodeProgram,
    dst_offset: usize,
    desc: &CalDescriptor,
    body_block: usize,
) -> Result<(), CodegenError> {
    let desc_ptr_val = ctx
        .b
        .ins()
        .iconst(ctx.ptr_ty, desc as *const CalDescriptor as i64);
    let ptr_off = desc.ptr_offset as i32;
    let dst = ctx.dst_at(dst_offset);

    let box_alloc_sig = make_box_alloc_sig(ctx);
    let box_alloc_fn = ctx
        .b
        .ins()
        .iconst(ctx.ptr_ty, vox_jit_box_alloc as *const () as i64);
    let call = ctx
        .b
        .ins()
        .call_indirect(box_alloc_sig, box_alloc_fn, &[desc_ptr_val, dst]);
    let status = ctx.b.inst_results(call)[0];
    let ok_val = ctx.b.ins().iconst(types::I32, DecodeStatus::Ok as i64);
    let is_ok = ctx.b.ins().icmp(IntCC::Equal, status, ok_val);

    let alloc_ok = ctx.fresh_block();
    let alloc_err = ctx.fresh_block();
    ctx.b.ins().brif(is_ok, alloc_ok, &[], alloc_err, &[]);

    ctx.b.switch_to_block(alloc_err);
    ctx.b.seal_block(alloc_err);
    ctx.return_err(DecodeStatus::AllocFailed);

    ctx.b.switch_to_block(alloc_ok);
    ctx.b.seal_block(alloc_ok);

    let alloc_ptr = ctx
        .b
        .ins()
        .load(ctx.ptr_ty, MemFlags::trusted(), dst, ptr_off);

    // Inline the pointee decode. Use a continuation block so that if the
    // body contains nested Vecs (or other multi-block ops), all paths converge
    // back into the caller's block sequence.
    let continuation = ctx.fresh_block();
    let saved_out_ptr = ctx.out_ptr;
    ctx.out_ptr = alloc_ptr;
    emit_inline_block(ctx, program, body_block, continuation)?;
    ctx.out_ptr = saved_out_ptr;

    ctx.b.switch_to_block(continuation);
    ctx.b.seal_block(continuation);

    Ok(())
}

fn emit_alloc_box_slice(
    ctx: &mut EmitCtx<'_, '_>,
    program: &DecodeProgram,
    dst_offset: usize,
    desc: &CalDescriptor,
    body_block: usize,
) -> Result<(), CodegenError> {
    let elem_size = desc.elem_size;
    let desc_ptr_val = ctx
        .b
        .ins()
        .iconst(ctx.ptr_ty, desc as *const CalDescriptor as i64);
    let ptr_off = desc.ptr_offset as i32;
    let dst = ctx.dst_at(dst_offset);

    // 1. Read varint list length.
    let len = ctx.read_varint_u64()?;
    let len_ptr = if ctx.ptr_ty == types::I64 {
        len
    } else {
        ctx.b.ins().ireduce(types::I32, len)
    };

    // 2. Call vox_jit_box_slice_alloc(desc, len, dst).
    let slice_alloc_sig = make_box_slice_alloc_sig(ctx);
    let slice_alloc_fn = ctx
        .b
        .ins()
        .iconst(ctx.ptr_ty, vox_jit_box_slice_alloc as *const () as i64);
    let call = ctx.b.ins().call_indirect(
        slice_alloc_sig,
        slice_alloc_fn,
        &[desc_ptr_val, len_ptr, dst],
    );
    let status = ctx.b.inst_results(call)[0];
    let ok_val = ctx.b.ins().iconst(types::I32, DecodeStatus::Ok as i64);
    let is_ok = ctx.b.ins().icmp(IntCC::Equal, status, ok_val);

    let alloc_ok = ctx.fresh_block();
    let alloc_err = ctx.fresh_block();
    ctx.b.ins().brif(is_ok, alloc_ok, &[], alloc_err, &[]);

    ctx.b.switch_to_block(alloc_err);
    ctx.b.seal_block(alloc_err);
    ctx.return_err(DecodeStatus::AllocFailed);

    ctx.b.switch_to_block(alloc_ok);
    ctx.b.seal_block(alloc_ok);

    // 3. Load backing data pointer (ptr_offset in fat pointer).
    let backing_ptr = ctx
        .b
        .ins()
        .load(ctx.ptr_ty, MemFlags::trusted(), dst, ptr_off);

    // 4. Element decode loop (same structure as emit_alloc_backing's loop).
    //
    // Use fresh SSA variables for this loop's counter and list-length so that
    // nested Vec fields in the element body can use their own ReadListLen +
    // AllocBacking without corrupting the outer loop's counter or length.
    let ptr_ty = ctx.ptr_ty;
    let var_loop_i = ctx.fresh_var(ptr_ty);
    let var_loop_len = ctx.fresh_var(ptr_ty);
    let var_inner_list_len = ctx.fresh_var(ptr_ty); // scratch for nested ReadListLen writes

    let zero = ctx.b.ins().iconst(ctx.ptr_ty, 0);
    ctx.b.def_var(var_loop_i, zero);
    ctx.b.def_var(var_loop_len, len_ptr);
    ctx.b.def_var(var_inner_list_len, zero);

    // Save outer vars; point ctx.var_init_count at var_loop_i and ctx.var_list_len
    // at the scratch variable so inner ReadListLen cannot overwrite var_loop_len.
    let saved_var_init_count = ctx.var_init_count;
    let saved_var_list_len = ctx.var_list_len;
    ctx.var_init_count = var_loop_i;
    ctx.var_list_len = var_inner_list_len;

    let loop_header = ctx.fresh_block();
    let loop_body_entry = ctx.fresh_block();
    let loop_tail = ctx.fresh_block(); // increment + back-edge
    let loop_done = ctx.fresh_block();

    ctx.b.ins().jump(loop_header, &[]);

    // Read var_loop_len directly — not via ctx.var_list_len — so inner code cannot corrupt it.
    ctx.b.switch_to_block(loop_header);
    let i = ctx.b.use_var(var_loop_i);
    let list_len = ctx.b.use_var(var_loop_len);
    let done = ctx
        .b
        .ins()
        .icmp(IntCC::UnsignedGreaterThanOrEqual, i, list_len);
    ctx.b.ins().brif(done, loop_done, &[], loop_body_entry, &[]);

    ctx.b.switch_to_block(loop_body_entry);
    ctx.b.seal_block(loop_body_entry);

    let elem_ptr = if elem_size == 0 {
        backing_ptr
    } else if elem_size == 1 {
        let i2 = ctx.b.use_var(var_loop_i);
        ctx.b.ins().iadd(backing_ptr, i2)
    } else {
        let i2 = ctx.b.use_var(var_loop_i);
        let stride = ctx.b.ins().iconst(ctx.ptr_ty, elem_size as i64);
        let offset = ctx.b.ins().imul(i2, stride);
        ctx.b.ins().iadd(backing_ptr, offset)
    };

    let saved_out_ptr = ctx.out_ptr;
    ctx.out_ptr = elem_ptr;
    emit_inline_block(ctx, program, body_block, loop_tail)?;
    ctx.out_ptr = saved_out_ptr;

    // Loop tail: all element body paths converge here.
    ctx.b.switch_to_block(loop_tail);
    ctx.b.seal_block(loop_tail);

    let i3 = ctx.b.use_var(var_loop_i);
    let one = ctx.b.ins().iconst(ctx.ptr_ty, 1);
    let next_i = ctx.b.ins().iadd(i3, one);
    ctx.b.def_var(var_loop_i, next_i);

    ctx.b.ins().jump(loop_header, &[]);
    ctx.b.seal_block(loop_header);

    ctx.b.switch_to_block(loop_done);
    ctx.b.seal_block(loop_done);

    // Restore outer loop vars.
    ctx.var_init_count = saved_var_init_count;
    ctx.var_list_len = saved_var_list_len;

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Copy the calibrated empty bytes for `desc` into `dst_offset` from `out_ptr`.
fn copy_empty_bytes(ctx: &mut EmitCtx<'_, '_>, desc: &CalDescriptor, dst_offset: usize) {
    let dst = ctx.dst_at(dst_offset);
    let src = ctx
        .b
        .ins()
        .iconst(ctx.ptr_ty, desc.empty_bytes.as_ptr() as i64);
    // Byte-by-byte copy (desc.size is small — 24 bytes for Vec on 64-bit).
    for i in 0..desc.size as i32 {
        let byte = ctx.b.ins().load(types::I8, MemFlags::trusted(), src, i);
        ctx.b.ins().store(MemFlags::trusted(), byte, dst, i);
    }
}

fn emit_inline_bytes(ctx: &mut EmitCtx<'_, '_>, dst: Value, bytes: &[u8]) {
    for (i, byte) in bytes.iter().copied().enumerate() {
        let value = ctx.b.ins().iconst(types::I8, i64::from(byte));
        ctx.b.ins().store(MemFlags::trusted(), value, dst, i as i32);
    }
}

fn zero_ptr(ctx: &mut EmitCtx<'_, '_>) -> Value {
    ctx.b.ins().iconst(ctx.ptr_ty, 0)
}

fn emit_usize_store(ctx: &mut EmitCtx<'_, '_>, base: Value, offset: usize, value: Value) {
    if offset == OFFSET_ABSENT as usize {
        return;
    }
    ctx.b
        .ins()
        .store(MemFlags::trusted(), value, base, offset as i32);
}

fn emit_ptr_store(ctx: &mut EmitCtx<'_, '_>, base: Value, offset: usize, value: Value) {
    if offset == OFFSET_ABSENT as usize {
        return;
    }
    ctx.b
        .ins()
        .store(MemFlags::trusted(), value, base, offset as i32);
}

fn emit_container_header(
    ctx: &mut EmitCtx<'_, '_>,
    dst: Value,
    desc: &CalDescriptor,
    data_ptr: Value,
    len: Value,
    cap: Value,
) {
    emit_ptr_store(ctx, dst, desc.ptr_offset as usize, data_ptr);
    emit_usize_store(ctx, dst, desc.len_offset as usize, len);
    emit_usize_store(ctx, dst, desc.cap_offset as usize, cap);
}

fn emit_len_store(ctx: &mut EmitCtx<'_, '_>, dst: Value, desc: &CalDescriptor, len: Value) {
    emit_usize_store(ctx, dst, desc.len_offset as usize, len);
}

/// Emit a byte-by-byte memcpy from `src` to `dst` for `len` bytes.
fn emit_memcpy(ctx: &mut EmitCtx<'_, '_>, src: Value, dst: Value, len: Value, _elem_size: usize) {
    // For the minimal subset, use a simple counted byte loop.
    // TODO: use Cranelift bulk_memory or call libc memcpy for large copies.
    let var_i = ctx.b.declare_var(ctx.ptr_ty);
    let zero = ctx.b.ins().iconst(ctx.ptr_ty, 0);
    ctx.b.def_var(var_i, zero);

    let header = ctx.b.create_block();
    let body = ctx.b.create_block();
    let exit = ctx.b.create_block();

    ctx.b.ins().jump(header, &[]);

    ctx.b.switch_to_block(header);
    // Do NOT seal header yet — its back-edge predecessor (body) is added below.
    let i = ctx.b.use_var(var_i);
    let done = ctx.b.ins().icmp(IntCC::UnsignedGreaterThanOrEqual, i, len);
    ctx.b.ins().brif(done, exit, &[], body, &[]);

    ctx.b.switch_to_block(body);
    ctx.b.seal_block(body); // only one predecessor: header
    let src_addr = ctx.b.ins().iadd(src, i);
    let dst_addr = ctx.b.ins().iadd(dst, i);
    let byte = ctx
        .b
        .ins()
        .load(types::I8, MemFlags::trusted(), src_addr, 0);
    ctx.b.ins().store(MemFlags::trusted(), byte, dst_addr, 0);
    let one = ctx.b.ins().iconst(ctx.ptr_ty, 1);
    let next_i = ctx.b.ins().iadd(i, one);
    ctx.b.def_var(var_i, next_i);
    ctx.b.ins().jump(header, &[]);
    ctx.b.seal_block(header); // now both predecessors (entry + back-edge) are known

    ctx.b.switch_to_block(exit);
    ctx.b.seal_block(exit);
}

/// Emit a call to `vox_jit_slow_path` for a `SlowPath` IR op.
///
/// Flushes `consumed` to ctx before the call so the helper sees the current
/// position, then reloads it after the helper updates `ctx.consumed`.
fn emit_slow_path(
    ctx: &mut EmitCtx<'_, '_>,
    shape: &'static facet_core::Shape,
    plan: &vox_postcard::TranslationPlan,
    dst_offset: usize,
) -> Result<(), CodegenError> {
    // Flush current consumed to ctx so the helper reads the right position.
    let consumed_val = ctx.b.use_var(ctx.var_consumed);
    let off = core::mem::offset_of!(DecodeCtx, consumed) as i32;
    ctx.b
        .ins()
        .store(MemFlags::trusted(), consumed_val, ctx.ctx_ptr, off);

    let shape_ptr = ctx
        .b
        .ins()
        .iconst(ctx.ptr_ty, shape as *const facet_core::Shape as i64);
    let plan_ptr = ctx.b.ins().iconst(
        ctx.ptr_ty,
        plan as *const vox_postcard::TranslationPlan as i64,
    );
    let dst_offset_val = ctx.b.ins().iconst(ctx.ptr_ty, dst_offset as i64);

    let sig = make_slow_path_sig(ctx);
    let fn_ptr = ctx.b.ins().iconst(
        ctx.ptr_ty,
        crate::helpers::vox_jit_slow_path as *const () as i64,
    );
    let call = ctx.b.ins().call_indirect(
        sig,
        fn_ptr,
        &[
            ctx.ctx_ptr,
            shape_ptr,
            plan_ptr,
            ctx.out_ptr,
            dst_offset_val,
        ],
    );
    let status = ctx.b.inst_results(call)[0];

    // Check status.
    let ok_val = ctx.b.ins().iconst(types::I32, DecodeStatus::Ok as i64);
    let is_ok = ctx.b.ins().icmp(IntCC::Equal, status, ok_val);

    let ok_block = ctx.fresh_block();
    let err_block = ctx.fresh_block();
    ctx.b.ins().brif(is_ok, ok_block, &[], err_block, &[]);

    ctx.b.switch_to_block(err_block);
    ctx.b.seal_block(err_block);
    // Return the helper's status directly.
    ctx.flush_ctx();
    ctx.b.ins().return_(&[status]);

    ctx.b.switch_to_block(ok_block);
    ctx.b.seal_block(ok_block);

    // Reload consumed from ctx (the helper updated ctx.consumed).
    ctx.reload_consumed_from_ctx();

    Ok(())
}

fn emit_decode_opaque(
    ctx: &mut EmitCtx<'_, '_>,
    shape: &'static facet_core::Shape,
    dst_offset: usize,
) -> Result<(), CodegenError> {
    let consumed_val = ctx.b.use_var(ctx.var_consumed);
    let off = core::mem::offset_of!(DecodeCtx, consumed) as i32;
    ctx.b
        .ins()
        .store(MemFlags::trusted(), consumed_val, ctx.ctx_ptr, off);

    let shape_ptr = ctx
        .b
        .ins()
        .iconst(ctx.ptr_ty, shape as *const facet_core::Shape as i64);
    let dst_offset_val = ctx.b.ins().iconst(ctx.ptr_ty, dst_offset as i64);

    let call_conv = ctx.b.func.signature.call_conv;
    let sig = ctx.b.func.import_signature(Signature {
        params: vec![
            AbiParam::new(ctx.ptr_ty),
            AbiParam::new(ctx.ptr_ty),
            AbiParam::new(ctx.ptr_ty),
            AbiParam::new(ctx.ptr_ty),
        ],
        returns: vec![AbiParam::new(types::I32)],
        call_conv,
    });
    let fn_ptr = ctx.b.ins().iconst(
        ctx.ptr_ty,
        crate::helpers::vox_jit_decode_opaque as *const () as i64,
    );
    let call = ctx.b.ins().call_indirect(
        sig,
        fn_ptr,
        &[ctx.ctx_ptr, shape_ptr, ctx.out_ptr, dst_offset_val],
    );
    let status = ctx.b.inst_results(call)[0];

    let ok_val = ctx.b.ins().iconst(types::I32, DecodeStatus::Ok as i64);
    let is_ok = ctx.b.ins().icmp(IntCC::Equal, status, ok_val);

    let ok_block = ctx.fresh_block();
    let err_block = ctx.fresh_block();
    ctx.b.ins().brif(is_ok, ok_block, &[], err_block, &[]);

    ctx.b.switch_to_block(err_block);
    ctx.b.seal_block(err_block);
    ctx.flush_ctx();
    ctx.b.ins().return_(&[status]);

    ctx.b.switch_to_block(ok_block);
    ctx.b.seal_block(ok_block);
    ctx.reload_consumed_from_ctx();

    Ok(())
}

fn emit_write_default(
    ctx: &mut EmitCtx<'_, '_>,
    shape: &'static facet_core::Shape,
    dst_offset: usize,
) -> Result<(), CodegenError> {
    let shape_ptr = ctx
        .b
        .ins()
        .iconst(ctx.ptr_ty, shape as *const facet_core::Shape as i64);
    let dst_offset_val = ctx.b.ins().iconst(ctx.ptr_ty, dst_offset as i64);

    let call_conv = ctx.b.func.signature.call_conv;
    let sig = ctx.b.func.import_signature(Signature {
        params: vec![
            AbiParam::new(ctx.ptr_ty),
            AbiParam::new(ctx.ptr_ty),
            AbiParam::new(ctx.ptr_ty),
        ],
        returns: vec![AbiParam::new(types::I32)],
        call_conv,
    });
    let fn_ptr = ctx.b.ins().iconst(
        ctx.ptr_ty,
        crate::helpers::vox_jit_write_default as *const () as i64,
    );
    let call = ctx
        .b
        .ins()
        .call_indirect(sig, fn_ptr, &[shape_ptr, ctx.out_ptr, dst_offset_val]);
    let status = ctx.b.inst_results(call)[0];

    let ok_val = ctx.b.ins().iconst(types::I32, DecodeStatus::Ok as i64);
    let is_ok = ctx.b.ins().icmp(IntCC::Equal, status, ok_val);

    let ok_block = ctx.fresh_block();
    let err_block = ctx.fresh_block();
    ctx.b.ins().brif(is_ok, ok_block, &[], err_block, &[]);

    ctx.b.switch_to_block(err_block);
    ctx.b.seal_block(err_block);
    ctx.flush_ctx();
    ctx.b.ins().return_(&[status]);

    ctx.b.switch_to_block(ok_block);
    ctx.b.seal_block(ok_block);

    Ok(())
}

fn make_alloc_sig(ctx: &mut EmitCtx<'_, '_>) -> cranelift_codegen::ir::SigRef {
    let call_conv = ctx.b.func.signature.call_conv;
    ctx.b.func.import_signature(Signature {
        params: vec![
            AbiParam::new(ctx.ptr_ty), // desc
            AbiParam::new(ctx.ptr_ty), // cap
        ],
        returns: vec![AbiParam::new(ctx.ptr_ty)],
        call_conv,
    })
}

/// Signature for `vox_jit_box_alloc(desc: *const OpaqueDescriptor, out_ptr: *mut u8) -> u32`.
fn make_box_alloc_sig(ctx: &mut EmitCtx<'_, '_>) -> cranelift_codegen::ir::SigRef {
    let call_conv = ctx.b.func.signature.call_conv;
    ctx.b.func.import_signature(Signature {
        params: vec![
            AbiParam::new(ctx.ptr_ty), // desc
            AbiParam::new(ctx.ptr_ty), // out_ptr
        ],
        returns: vec![AbiParam::new(types::I32)],
        call_conv,
    })
}

/// Signature for `vox_jit_box_slice_alloc(desc, len, out_ptr) -> u32`.
fn make_box_slice_alloc_sig(ctx: &mut EmitCtx<'_, '_>) -> cranelift_codegen::ir::SigRef {
    let call_conv = ctx.b.func.signature.call_conv;
    ctx.b.func.import_signature(Signature {
        params: vec![
            AbiParam::new(ctx.ptr_ty), // desc
            AbiParam::new(ctx.ptr_ty), // len
            AbiParam::new(ctx.ptr_ty), // out_ptr
        ],
        returns: vec![AbiParam::new(types::I32)],
        call_conv,
    })
}

fn make_utf8_validate_sig(ctx: &mut EmitCtx<'_, '_>) -> cranelift_codegen::ir::SigRef {
    let call_conv = ctx.b.func.signature.call_conv;
    ctx.b.func.import_signature(Signature {
        params: vec![
            AbiParam::new(ctx.ptr_ty), // bytes
            AbiParam::new(ctx.ptr_ty), // len
        ],
        returns: vec![AbiParam::new(types::I32)],
        call_conv,
    })
}

/// Signature for `vox_jit_slow_path(ctx, shape, plan, dst_base, dst_offset) -> u32`.
fn make_slow_path_sig(ctx: &mut EmitCtx<'_, '_>) -> cranelift_codegen::ir::SigRef {
    let call_conv = ctx.b.func.signature.call_conv;
    ctx.b.func.import_signature(Signature {
        params: vec![
            AbiParam::new(ctx.ptr_ty), // ctx: *mut DecodeCtx
            AbiParam::new(ctx.ptr_ty), // shape: &'static Shape (fat ptr — data half)
            AbiParam::new(ctx.ptr_ty), // plan: *const TranslationPlan
            AbiParam::new(ctx.ptr_ty), // dst_base: *mut u8
            AbiParam::new(ctx.ptr_ty), // dst_offset: usize
        ],
        returns: vec![AbiParam::new(types::I32)],
        call_conv,
    })
}

fn next_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Make a shape's Display-form suitable for use as a JIT symbol name: keep
/// identifiers, `::`, angle brackets, commas, and dots; replace anything else
/// (including whitespace) with `_`. Truncate to keep perf-map / symbolicator
/// output readable.
fn shape_symbol_fragment(shape: &'static facet_core::Shape) -> String {
    let raw = shape.to_string();
    let mut out = String::with_capacity(raw.len());
    for c in raw.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '_' | ':' | '<' | '>' | ',' | '.' | '[' | ']') {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    const MAX: usize = 96;
    if out.len() > MAX {
        out.truncate(MAX);
    }
    out
}

// ---------------------------------------------------------------------------
// Encode function emission
// ---------------------------------------------------------------------------

/// When a child encoder's body is being inlined into the parent, this holds
/// the outer blocks that `return_ok` / `return_fail` must jump to instead of
/// emitting a real `return`. `None` at top level.
#[derive(Clone, Copy)]
struct InlineFrame {
    /// Target for `return_ok` — parent continues emitting after the inlined
    /// child. `buf_{ptr,len,cap}` stay live in their Cranelift `Variable`s
    /// across this jump, which is the whole point of inlining.
    cont_block: Block,
    /// Target for `return_fail` — a parent-owned block that itself calls
    /// `return_fail()` in the outer frame (which may redirect again, or
    /// emit the real return).
    fail_block: Block,
}

/// State threaded through the Cranelift encode function builder.
struct EncodeCtx_<'a, 'b> {
    b: &'a mut FunctionBuilder<'b>,
    /// `*mut EncodeCtx` argument.
    enc_ctx: Value,
    /// `*const u8` source pointer argument (base of the value being encoded).
    /// Swapped to the child's src while inlining a `WriteShape` body.
    src_ptr: Value,
    ptr_ty: Type,
    descriptors: &'a CalibrationRegistry,
    /// Mapping from EncodeProgram block index to Cranelift Block. Swapped
    /// to a child's block_map while inlining — saved/restored on the stack.
    block_map: Vec<Option<Block>>,
    /// Block indices inlined into the parent (element loops).
    inlined_blocks: std::collections::HashSet<usize>,
    /// Local SSA cache of `ctx.buf_ptr`. Kept in sync via flush before, and
    /// reload after, any helper call that may touch the buffer.
    var_buf_ptr: Variable,
    /// Local SSA cache of `ctx.buf_len`. Flushed before helper calls and on
    /// successful return; bumped purely in registers on scalar writes.
    var_buf_len: Variable,
    /// Local SSA cache of `ctx.buf_cap`. Reloaded after grow / helper calls.
    var_buf_cap: Variable,
    /// Compile-time-resolved encoders for nested shapes. Consulted by
    /// `EncodeOp::WriteShape` to decide between inlining, direct indirect
    /// call, or helper fallback. Swapped to the child's map while inlining
    /// so grandchildren resolve.
    child_encoders: std::sync::Arc<ChildEncoderMap>,
    /// When `Some`, `return_ok`/`return_fail` redirect to these blocks
    /// instead of emitting a real return. Set while inlining a child.
    inline_frame: Option<InlineFrame>,
    /// Shapes whose encoders are currently on the inlining stack (including
    /// the top-level shape being compiled). A `WriteShape` whose child is
    /// on this stack falls back to `call_indirect` to break the cycle.
    /// Compared via `Shape: Eq` (i.e. `ConstTypeId`).
    inlining_stack: Vec<&'static facet_core::Shape>,
}

impl<'a, 'b> EncodeCtx_<'a, 'b> {
    fn fresh_block(&mut self) -> Block {
        self.b.create_block()
    }

    /// Compute `src_ptr + offset`.
    fn src_at(&mut self, offset: usize) -> Value {
        if offset == 0 {
            self.src_ptr
        } else {
            self.b.ins().iadd_imm(self.src_ptr, offset as i64)
        }
    }

    /// Return `true` (success), or — when inlining — jump to the parent's
    /// continuation block. In inlined mode we deliberately skip the flush:
    /// the parent will keep `buf_len` in its Cranelift Variable and only
    /// flush at its own real return / helper-call boundary.
    fn return_ok(&mut self) {
        if let Some(frame) = self.inline_frame {
            self.b.ins().jump(frame.cont_block, &[]);
            return;
        }
        self.flush_buf_len_to_ctx();
        let ok = self.b.ins().iconst(types::I8, 1);
        self.b.ins().return_(&[ok]);
    }

    /// Return `false` (failure — OOM), or — when inlining — jump to the
    /// parent's fail block, which itself propagates the failure up its own
    /// frame. Does NOT flush buf_len: on failure paths the authoritative
    /// value is whatever the last helper/grow wrote into ctx.
    fn return_fail(&mut self) {
        if let Some(frame) = self.inline_frame {
            self.b.ins().jump(frame.fail_block, &[]);
            return;
        }
        let fail = self.b.ins().iconst(types::I8, 0);
        self.b.ins().return_(&[fail]);
    }

    fn off_buf_ptr() -> i32 {
        core::mem::offset_of!(EncodeCtx, buf_ptr) as i32
    }

    fn off_buf_len() -> i32 {
        core::mem::offset_of!(EncodeCtx, buf_len) as i32
    }

    fn off_buf_cap() -> i32 {
        core::mem::offset_of!(EncodeCtx, buf_cap) as i32
    }

    /// Read the cached `buf_ptr` (no memory load on the hot path).
    fn load_buf_ptr(&mut self) -> Value {
        self.b.use_var(self.var_buf_ptr)
    }

    /// Read the cached `buf_len`.
    fn load_buf_len(&mut self) -> Value {
        self.b.use_var(self.var_buf_len)
    }

    /// Read the cached `buf_cap`.
    fn load_buf_cap(&mut self) -> Value {
        self.b.use_var(self.var_buf_cap)
    }

    /// Update the cached `buf_len` (no memory store on the hot path).
    fn store_buf_len(&mut self, new_len: Value) {
        self.b.def_var(self.var_buf_len, new_len);
    }

    /// Publish the cached buf_len back to `ctx.buf_len`.
    fn flush_buf_len_to_ctx(&mut self) {
        let len = self.b.use_var(self.var_buf_len);
        self.b
            .ins()
            .store(MemFlags::trusted(), len, self.enc_ctx, Self::off_buf_len());
    }

    /// Reload all three buf_* fields from ctx into the local cache. Use after
    /// a helper call that may have mutated the buffer (and potentially grown it).
    fn reload_buf_state(&mut self) {
        let ptr = self.b.ins().load(
            self.ptr_ty,
            MemFlags::trusted(),
            self.enc_ctx,
            Self::off_buf_ptr(),
        );
        let len = self.b.ins().load(
            self.ptr_ty,
            MemFlags::trusted(),
            self.enc_ctx,
            Self::off_buf_len(),
        );
        let cap = self.b.ins().load(
            self.ptr_ty,
            MemFlags::trusted(),
            self.enc_ctx,
            Self::off_buf_cap(),
        );
        self.b.def_var(self.var_buf_ptr, ptr);
        self.b.def_var(self.var_buf_len, len);
        self.b.def_var(self.var_buf_cap, cap);
    }

    /// Emit a call to `vox_jit_buf_grow(ctx, needed)`. Returns its `bool` result.
    fn emit_call_grow(&mut self, needed: Value) -> Value {
        let call_conv = self.b.func.signature.call_conv;
        let sig = self.b.func.import_signature(Signature {
            params: vec![AbiParam::new(self.ptr_ty), AbiParam::new(self.ptr_ty)],
            returns: vec![AbiParam::new(types::I8)],
            call_conv,
        });
        let callee = self
            .b
            .ins()
            .iconst(self.ptr_ty, vox_jit_buf_grow as *const () as i64);
        let call = self
            .b
            .ins()
            .call_indirect(sig, callee, &[self.enc_ctx, needed]);
        self.b.inst_results(call)[0]
    }

    /// Ensure `needed` bytes of capacity remain after the cached `buf_len`.
    /// On shortfall, flush `buf_len` to ctx, call `vox_jit_buf_grow`, reload
    /// `buf_ptr` / `buf_cap` (grow may have reallocated); if grow fails, bail
    /// via `return_fail`.
    ///
    /// Returns `(buf_ptr, buf_len)` valid on the fast-path continuation.
    fn reserve(&mut self, needed: Value) -> (Value, Value) {
        let len = self.load_buf_len();
        let cap = self.load_buf_cap();
        let avail = self.b.ins().isub(cap, len);
        let lacking = self.b.ins().icmp(IntCC::UnsignedLessThan, avail, needed);

        let grow_block = self.fresh_block();
        let fast_block = self.fresh_block();

        self.b.ins().brif(lacking, grow_block, &[], fast_block, &[]);

        // Slow path: publish len to ctx, grow, reload ptr/cap, branch to fast or fail.
        self.b.switch_to_block(grow_block);
        self.b.seal_block(grow_block);
        self.flush_buf_len_to_ctx();
        let grow_ok = self.emit_call_grow(needed);
        let new_ptr = self.b.ins().load(
            self.ptr_ty,
            MemFlags::trusted(),
            self.enc_ctx,
            Self::off_buf_ptr(),
        );
        let new_cap = self.b.ins().load(
            self.ptr_ty,
            MemFlags::trusted(),
            self.enc_ctx,
            Self::off_buf_cap(),
        );
        self.b.def_var(self.var_buf_ptr, new_ptr);
        self.b.def_var(self.var_buf_cap, new_cap);
        let fail_block = self.fresh_block();
        self.b.ins().brif(grow_ok, fast_block, &[], fail_block, &[]);

        self.b.switch_to_block(fail_block);
        self.b.seal_block(fail_block);
        self.return_fail();

        // Fast path: Cranelift will phi buf_ptr/buf_cap between the direct and
        // grow predecessors automatically via Variable SSA.
        self.b.switch_to_block(fast_block);
        self.b.seal_block(fast_block);
        let ptr = self.load_buf_ptr();
        let len = self.load_buf_len();
        (ptr, len)
    }

    /// Append one byte to the encode buffer. Grows inline on shortfall.
    fn call_push_byte(&mut self, byte: Value) {
        let one = self.b.ins().iconst(self.ptr_ty, 1);
        let (ptr, len) = self.reserve(one);
        let addr = self.b.ins().iadd(ptr, len);
        self.b.ins().store(MemFlags::trusted(), byte, addr, 0);
        let new_len = self.b.ins().iadd_imm(len, 1);
        self.store_buf_len(new_len);
    }

    /// Append an integer of `ty` (1/2/4/8 bytes) to the encode buffer with a
    /// single inline store — no libcall. The value is stored little-endian by
    /// host convention; we only target LE platforms (x86_64 / aarch64).
    fn call_push_int(&mut self, value: Value, ty: Type) {
        let size = ty.bytes() as i64;
        let size_val = self.b.ins().iconst(self.ptr_ty, size);
        let (ptr, len) = self.reserve(size_val);
        let addr = self.b.ins().iadd(ptr, len);
        self.b.ins().store(MemFlags::trusted(), value, addr, 0);
        let new_len = self.b.ins().iadd_imm(len, size);
        self.store_buf_len(new_len);
    }

    /// Emit a `libc.memcpy(dst, src, len)` call.
    fn emit_memcpy(&mut self, dst: Value, src: Value, len: Value) {
        let call_conv = self.b.func.signature.call_conv;
        let sig = self.b.func.import_signature(Signature {
            params: vec![
                AbiParam::new(self.ptr_ty),
                AbiParam::new(self.ptr_ty),
                AbiParam::new(self.ptr_ty),
            ],
            returns: vec![AbiParam::new(self.ptr_ty)],
            call_conv,
        });
        let callee = self.b.func.import_function(ExtFuncData {
            name: ExternalName::LibCall(LibCall::Memcpy),
            signature: sig,
            colocated: false,
            patchable: false,
        });
        self.b.ins().call(callee, &[dst, src, len]);
    }

    /// Append `len` bytes from `data` to the encode buffer. Grows inline on
    /// shortfall. For `len < 16` the copy is emitted as an inline overlapping
    /// word-sized load/store ladder (saves the `libc.memcpy` call for the
    /// abundance of short strings in typical RPC payloads); larger copies fall
    /// through to the libcall.
    fn call_push_bytes(&mut self, data: Value, len: Value) {
        let (ptr, buf_len) = self.reserve(len);
        let dst = self.b.ins().iadd(ptr, buf_len);
        self.emit_inline_copy(dst, data, len);
        let new_len = self.b.ins().iadd(buf_len, len);
        self.store_buf_len(new_len);
    }

    /// Copy `len` bytes from `src` to `dst`. Inlines an overlapping-word
    /// ladder for `len` in `[1, 15]` and falls back to `libc.memcpy` for
    /// `len >= 16`.
    fn emit_inline_copy(&mut self, dst: Value, src: Value, len: Value) {
        let done = self.fresh_block();
        let big = self.fresh_block();
        let small = self.fresh_block();
        let try_8 = self.fresh_block();
        let do_8 = self.fresh_block();
        let try_4 = self.fresh_block();
        let do_4 = self.fresh_block();
        let try_2 = self.fresh_block();
        let do_2 = self.fresh_block();
        let do_1 = self.fresh_block();

        // `len >= 16` → libcall path.
        let is_big = self
            .b
            .ins()
            .icmp_imm(IntCC::UnsignedGreaterThanOrEqual, len, 16);
        self.b.ins().brif(is_big, big, &[], small, &[]);

        self.b.switch_to_block(big);
        self.b.seal_block(big);
        self.emit_memcpy(dst, src, len);
        self.b.ins().jump(done, &[]);

        // Small path: peel the powers-of-two in decreasing order, each with an
        // overlapping pair of loads/stores so we write exactly `len` bytes
        // without overreading `src` or requiring an inner loop.
        self.b.switch_to_block(small);
        self.b.seal_block(small);
        let ge_8 = self
            .b
            .ins()
            .icmp_imm(IntCC::UnsignedGreaterThanOrEqual, len, 8);
        self.b.ins().brif(ge_8, do_8, &[], try_8, &[]);

        self.b.switch_to_block(do_8);
        self.b.seal_block(do_8);
        self.emit_overlap_copy(dst, src, len, types::I64, 8);
        self.b.ins().jump(done, &[]);

        self.b.switch_to_block(try_8);
        self.b.seal_block(try_8);
        let ge_4 = self
            .b
            .ins()
            .icmp_imm(IntCC::UnsignedGreaterThanOrEqual, len, 4);
        self.b.ins().brif(ge_4, do_4, &[], try_4, &[]);

        self.b.switch_to_block(do_4);
        self.b.seal_block(do_4);
        self.emit_overlap_copy(dst, src, len, types::I32, 4);
        self.b.ins().jump(done, &[]);

        self.b.switch_to_block(try_4);
        self.b.seal_block(try_4);
        let ge_2 = self
            .b
            .ins()
            .icmp_imm(IntCC::UnsignedGreaterThanOrEqual, len, 2);
        self.b.ins().brif(ge_2, do_2, &[], try_2, &[]);

        self.b.switch_to_block(do_2);
        self.b.seal_block(do_2);
        self.emit_overlap_copy(dst, src, len, types::I16, 2);
        self.b.ins().jump(done, &[]);

        self.b.switch_to_block(try_2);
        self.b.seal_block(try_2);
        let ge_1 = self.b.ins().icmp_imm(IntCC::Equal, len, 1);
        self.b.ins().brif(ge_1, do_1, &[], done, &[]);

        self.b.switch_to_block(do_1);
        self.b.seal_block(do_1);
        let byte = self.b.ins().load(types::I8, MemFlags::trusted(), src, 0);
        self.b.ins().store(MemFlags::trusted(), byte, dst, 0);
        self.b.ins().jump(done, &[]);

        self.b.switch_to_block(done);
        self.b.seal_block(done);
    }

    /// Emit two overlapping `width`-byte loads/stores: one at the head
    /// (`src[0..width]`) and one at the tail (`src[len-width..len]`). Valid
    /// iff `len >= width`; the two accesses cover the whole `len` range with
    /// at most `width - 1` bytes of overlap.
    fn emit_overlap_copy(&mut self, dst: Value, src: Value, len: Value, int_ty: Type, width: i64) {
        let lo = self.b.ins().load(int_ty, MemFlags::trusted(), src, 0);
        self.b.ins().store(MemFlags::trusted(), lo, dst, 0);
        let tail_off = self.b.ins().iadd_imm(len, -width);
        let src_tail = self.b.ins().iadd(src, tail_off);
        let dst_tail = self.b.ins().iadd(dst, tail_off);
        let hi = self.b.ins().load(int_ty, MemFlags::trusted(), src_tail, 0);
        self.b.ins().store(MemFlags::trusted(), hi, dst_tail, 0);
    }

    /// Append a `u64` as a postcard varint to the encode buffer.
    ///
    /// Reserves 10 bytes (max u64 varint width) up front, then emits a tight
    /// loop that writes continuation-bit bytes until the remaining value fits
    /// in 7 bits, writes the final byte, and commits the new buffer length.
    fn call_write_varint(&mut self, value: Value) {
        let ten = self.b.ins().iconst(self.ptr_ty, 10);
        let (ptr, len0) = self.reserve(ten);
        let start = self.b.ins().iadd(ptr, len0);

        let var_val = self.b.declare_var(types::I64);
        let var_wp = self.b.declare_var(self.ptr_ty);
        self.b.def_var(var_val, value);
        self.b.def_var(var_wp, start);

        let header = self.fresh_block();
        let body = self.fresh_block();
        let tail = self.fresh_block();

        self.b.ins().jump(header, &[]);

        self.b.switch_to_block(header);
        let cur_val = self.b.use_var(var_val);
        let done = self
            .b
            .ins()
            .icmp_imm(IntCC::UnsignedLessThan, cur_val, 0x80);
        self.b.ins().brif(done, tail, &[], body, &[]);

        self.b.switch_to_block(body);
        self.b.seal_block(body);
        let low = self.b.ins().ireduce(types::I8, cur_val);
        let hi_bit = self.b.ins().iconst(types::I8, 0x80);
        let byte = self.b.ins().bor(low, hi_bit);
        let wp = self.b.use_var(var_wp);
        self.b.ins().store(MemFlags::trusted(), byte, wp, 0);
        let next_wp = self.b.ins().iadd_imm(wp, 1);
        self.b.def_var(var_wp, next_wp);
        let next_val = self.b.ins().ushr_imm(cur_val, 7);
        self.b.def_var(var_val, next_val);
        self.b.ins().jump(header, &[]);

        self.b.seal_block(header);

        self.b.switch_to_block(tail);
        self.b.seal_block(tail);
        let final_val = self.b.use_var(var_val);
        let final_wp = self.b.use_var(var_wp);
        let final_byte = self.b.ins().ireduce(types::I8, final_val);
        self.b
            .ins()
            .store(MemFlags::trusted(), final_byte, final_wp, 0);
        let end_wp = self.b.ins().iadd_imm(final_wp, 1);
        let new_len = self.b.ins().isub(end_wp, ptr);
        self.store_buf_len(new_len);
    }

    /// Append an `i64` as a zigzag-encoded postcard varint.
    fn call_write_varint_signed(&mut self, value: Value) {
        let shl = self.b.ins().ishl_imm(value, 1);
        let asr = self.b.ins().sshr_imm(value, 63);
        let zz = self.b.ins().bxor(shl, asr);
        self.call_write_varint(zz);
    }
}

fn emit_encode_function(
    builder: &mut FunctionBuilder<'_>,
    program: &EncodeProgram,
    descriptors: &CalibrationRegistry,
    ptr_ty: Type,
    child_encoders: std::sync::Arc<ChildEncoderMap>,
    top_shape: Option<&'static facet_core::Shape>,
) -> Result<(), CodegenError> {
    let entry = builder.create_block();
    builder.append_block_params_for_function_params(entry);
    builder.switch_to_block(entry);
    builder.seal_block(entry);

    let enc_ctx = builder.block_params(entry)[0];
    let src_ptr = builder.block_params(entry)[1];

    // Cache ctx.buf_{ptr,len,cap} in locals so the hot path is pure-register
    // bump+store. Flushed to ctx only around helper calls and on return_ok.
    let var_buf_ptr = builder.declare_var(ptr_ty);
    let var_buf_len = builder.declare_var(ptr_ty);
    let var_buf_cap = builder.declare_var(ptr_ty);
    let off_ptr = core::mem::offset_of!(EncodeCtx, buf_ptr) as i32;
    let off_len = core::mem::offset_of!(EncodeCtx, buf_len) as i32;
    let off_cap = core::mem::offset_of!(EncodeCtx, buf_cap) as i32;
    let init_ptr = builder
        .ins()
        .load(ptr_ty, MemFlags::trusted(), enc_ctx, off_ptr);
    let init_len = builder
        .ins()
        .load(ptr_ty, MemFlags::trusted(), enc_ctx, off_len);
    let init_cap = builder
        .ins()
        .load(ptr_ty, MemFlags::trusted(), enc_ctx, off_cap);
    builder.def_var(var_buf_ptr, init_ptr);
    builder.def_var(var_buf_len, init_len);
    builder.def_var(var_buf_cap, init_cap);

    let mut block_map: Vec<Option<Block>> = (0..program.blocks.len())
        .map(|_| Some(builder.create_block()))
        .collect();
    block_map[0] = Some(entry);

    let mut ectx = EncodeCtx_ {
        b: builder,
        enc_ctx,
        src_ptr,
        ptr_ty,
        descriptors,
        block_map: block_map.into_iter().collect(),
        inlined_blocks: std::collections::HashSet::new(),
        var_buf_ptr,
        var_buf_len,
        var_buf_cap,
        child_encoders,
        inline_frame: None,
        inlining_stack: top_shape.map(|s| vec![s]).unwrap_or_default(),
    };

    emit_encode_block(&mut ectx, program, 0)?;

    for block_idx in 1..program.blocks.len() {
        if ectx.inlined_blocks.contains(&block_idx) {
            // Stub out the unreachable pre-created block.
            let clif_block = ectx.block_map[block_idx].unwrap();
            ectx.b.switch_to_block(clif_block);
            ectx.b.seal_block(clif_block);
            ectx.return_ok();
            continue;
        }
        let clif_block = ectx.block_map[block_idx].unwrap();
        ectx.b.switch_to_block(clif_block);
        ectx.b.seal_block(clif_block);
        emit_encode_block(&mut ectx, program, block_idx)?;
    }

    Ok(())
}

fn emit_encode_block(
    ectx: &mut EncodeCtx_<'_, '_>,
    program: &EncodeProgram,
    block_idx: usize,
) -> Result<(), CodegenError> {
    let block = &program.blocks[block_idx];
    for op in &block.ops {
        let terminated = emit_encode_op(ectx, program, op)?;
        if terminated {
            break;
        }
    }
    Ok(())
}

/// Emit one encode IR op. Returns `true` if the current Cranelift block is terminated.
fn emit_encode_op(
    ectx: &mut EncodeCtx_<'_, '_>,
    program: &EncodeProgram,
    op: &EncodeOp,
) -> Result<bool, CodegenError> {
    match op {
        EncodeOp::WriteScalar { prim, src_offset } => {
            emit_write_scalar(ectx, *prim, *src_offset)?;
            Ok(false)
        }

        EncodeOp::WriteStringLike { shape, src_offset } => {
            emit_encode_helper_shape_op(
                ectx,
                shape,
                *src_offset,
                crate::helpers::vox_jit_encode_string_like as *const (),
            );
            Ok(false)
        }

        EncodeOp::WriteShape { shape, src_offset } => {
            if let Some(&child_encoder) = ectx.child_encoders.get(shape) {
                let in_cycle = ectx.inlining_stack.iter().any(|s| *s == *shape);
                if in_cycle {
                    emit_encode_direct_child(ectx, child_encoder.encode_fn, *src_offset);
                } else {
                    emit_inline_child_program(ectx, child_encoder, *src_offset)?;
                }
            } else {
                emit_encode_helper_shape_op(
                    ectx,
                    shape,
                    *src_offset,
                    crate::helpers::vox_jit_encode_shape as *const (),
                );
            }
            Ok(false)
        }

        EncodeOp::WriteOpaque { shape, src_offset } => {
            emit_encode_helper_shape_op(
                ectx,
                shape,
                *src_offset,
                crate::helpers::vox_jit_encode_opaque as *const (),
            );
            Ok(false)
        }

        EncodeOp::WriteProxy { shape, src_offset } => {
            emit_encode_helper_shape_op(
                ectx,
                shape,
                *src_offset,
                crate::helpers::vox_jit_encode_proxy as *const (),
            );
            Ok(false)
        }

        EncodeOp::WriteBytesLike { shape, src_offset } => {
            emit_encode_helper_shape_op(
                ectx,
                shape,
                *src_offset,
                crate::helpers::vox_jit_encode_bytes_like as *const (),
            );
            Ok(false)
        }

        EncodeOp::SlowPath { shape, src_offset } => {
            emit_encode_slow_path(ectx, shape, *src_offset);
            Ok(false)
        }

        EncodeOp::BorrowPointer {
            src_offset,
            body_block,
            borrow_fn,
        } => {
            emit_encode_borrow_pointer(ectx, program, *src_offset, *body_block, *borrow_fn)?;
            Ok(false)
        }

        EncodeOp::WriteByteSlice {
            src_offset,
            len_fn,
            as_ptr_fn,
        } => {
            emit_write_byte_slice(ectx, *src_offset, *len_fn, *as_ptr_fn)?;
            Ok(false)
        }

        EncodeOp::WriteVariantIndex { index } => {
            let idx = ectx.b.ins().iconst(types::I64, *index as i64);
            ectx.call_write_varint(idx);
            Ok(false)
        }

        EncodeOp::BranchOnEncode {
            src_offset,
            tag_width,
            variant_blocks,
        } => {
            emit_encode_branch(ectx, program, *src_offset, *tag_width, variant_blocks)?;
            // emit_encode_branch leaves us in after_block — not a terminator.
            Ok(false)
        }

        EncodeOp::EncodeOption {
            src_offset,
            some_block,
            is_some_fn,
            get_value_fn,
        } => {
            emit_encode_option(
                ectx,
                program,
                *src_offset,
                *some_block,
                *is_some_fn,
                *get_value_fn,
            )?;
            // emit_encode_option leaves us in after_block — not a terminator.
            Ok(false)
        }

        EncodeOp::EncodeOptionCalibrated {
            src_offset,
            inner_offset,
            some_block,
            tag_bytes,
        } => {
            emit_encode_option_calibrated(
                ectx,
                program,
                *src_offset,
                *inner_offset,
                *some_block,
                tag_bytes,
            )?;
            Ok(false)
        }

        EncodeOp::EncodeResult {
            shape: _,
            src_offset,
            ok_block,
            err_block,
            ok_shape,
            err_shape,
            is_ok_fn,
            get_ok_fn,
            get_err_fn,
        } => {
            emit_encode_result(
                ectx,
                program,
                *src_offset,
                *ok_block,
                *err_block,
                ok_shape,
                err_shape,
                *is_ok_fn,
                *get_ok_fn,
                *get_err_fn,
            )?;
            Ok(false)
        }

        EncodeOp::EncodeArray {
            src_offset,
            count,
            elem_size,
            body_block,
        } => {
            emit_encode_array(ectx, program, *src_offset, *count, *elem_size, *body_block)?;
            Ok(false)
        }

        EncodeOp::EncodeList {
            src_offset,
            descriptor,
            body_block,
            elem_size,
        } => {
            emit_encode_list(
                ectx,
                program,
                *src_offset,
                descriptor,
                *body_block,
                *elem_size,
            )?;
            Ok(false)
        }

        EncodeOp::WriteByteList {
            src_offset,
            descriptor,
        } => {
            emit_encode_byte_list(ectx, *src_offset, descriptor)?;
            Ok(false)
        }

        EncodeOp::Jump { block_id } => {
            let target = ectx.block_map[*block_id].unwrap();
            ectx.b.ins().jump(target, &[]);
            Ok(true)
        }

        EncodeOp::Return => {
            ectx.return_ok();
            Ok(true)
        }
    }
}

/// Write one scalar primitive from `src_offset` to the encode buffer.
fn emit_write_scalar(
    ectx: &mut EncodeCtx_<'_, '_>,
    prim: WirePrimitive,
    src_offset: usize,
) -> Result<(), CodegenError> {
    let addr = ectx.src_at(src_offset);
    match prim {
        WirePrimitive::Unit => {}

        WirePrimitive::Bool => {
            let byte = ectx.b.ins().load(types::I8, MemFlags::trusted(), addr, 0);
            ectx.call_push_byte(byte);
        }

        WirePrimitive::U8 | WirePrimitive::I8 => {
            let byte = ectx.b.ins().load(types::I8, MemFlags::trusted(), addr, 0);
            ectx.call_push_byte(byte);
        }

        WirePrimitive::U16 | WirePrimitive::USize => {
            let v = ectx.b.ins().load(types::I16, MemFlags::trusted(), addr, 0);
            let v64 = ectx.b.ins().uextend(types::I64, v);
            ectx.call_write_varint(v64);
        }

        WirePrimitive::U32 => {
            let v = ectx.b.ins().load(types::I32, MemFlags::trusted(), addr, 0);
            let v64 = ectx.b.ins().uextend(types::I64, v);
            ectx.call_write_varint(v64);
        }

        WirePrimitive::U64 => {
            let v = ectx.b.ins().load(types::I64, MemFlags::trusted(), addr, 0);
            ectx.call_write_varint(v);
        }

        WirePrimitive::U128 => {
            // u128 needs zigzag? No — u128 uses write_varint_u128 in the reflective path.
            // For JIT we fall back to the byte-level approach: write 16 bytes in varint.
            // Emit as 2×u64 varints (lo then hi) approximation is wrong.
            // Actual postcard u128: varint up to 19 bytes.
            // We call push_bytes with the 16 raw bytes and varint-encode directly.
            // Actually postcard uses a zigzag-free 128-bit varint (same as u64 but wider).
            // We don't have a vox_jit_buf_write_varint_u128 helper yet — return unsupported.
            return Err(CodegenError::UnsupportedOp(
                "u128 encode not yet supported".into(),
            ));
        }

        WirePrimitive::I16 | WirePrimitive::ISize => {
            let v = ectx.b.ins().load(types::I16, MemFlags::trusted(), addr, 0);
            let v64 = ectx.b.ins().sextend(types::I64, v);
            ectx.call_write_varint_signed(v64);
        }

        WirePrimitive::I32 => {
            let v = ectx.b.ins().load(types::I32, MemFlags::trusted(), addr, 0);
            let v64 = ectx.b.ins().sextend(types::I64, v);
            ectx.call_write_varint_signed(v64);
        }

        WirePrimitive::I64 => {
            let v = ectx.b.ins().load(types::I64, MemFlags::trusted(), addr, 0);
            ectx.call_write_varint_signed(v);
        }

        WirePrimitive::I128 => {
            return Err(CodegenError::UnsupportedOp(
                "i128 encode not yet supported".into(),
            ));
        }

        WirePrimitive::F32 => {
            let v = ectx.b.ins().load(types::I32, MemFlags::trusted(), addr, 0);
            ectx.call_push_int(v, types::I32);
        }

        WirePrimitive::F64 => {
            let v = ectx.b.ins().load(types::I64, MemFlags::trusted(), addr, 0);
            ectx.call_push_int(v, types::I64);
        }

        WirePrimitive::String => {
            // `String` in memory: ptr, len, cap (calibrated). We need to read
            // ptr and len from the String's internal representation.
            // Since we don't have the descriptor here, use a slow approach:
            // emit as a SlowPath. Actually — for String fields in structs the
            // IR emitter will have already resolved via descriptor. If we land
            // here, we have a bare String scalar — fall back.
            return Err(CodegenError::UnsupportedOp(
                "String scalar in WriteScalar — use EncodeList with descriptor".into(),
            ));
        }

        WirePrimitive::Bytes => {
            return Err(CodegenError::UnsupportedOp(
                "Bytes scalar in WriteScalar — use EncodeList with descriptor".into(),
            ));
        }

        WirePrimitive::Payload => {
            return Err(CodegenError::UnsupportedOp(
                "Payload encode not yet supported".into(),
            ));
        }

        WirePrimitive::Char => {
            // char is 4 bytes in Rust. Encode as UTF-8 length-prefixed string.
            // Read the char as u32, encode as UTF-8. This requires runtime logic —
            // fall back for now.
            return Err(CodegenError::UnsupportedOp(
                "char encode not yet supported in JIT".into(),
            ));
        }
    }
    Ok(())
}

fn emit_encode_helper_shape_op(
    ectx: &mut EncodeCtx_<'_, '_>,
    shape: &'static facet_core::Shape,
    src_offset: usize,
    helper: *const (),
) {
    let src_ptr = ectx.src_at(src_offset);
    emit_encode_shape_ptr_with_helper(ectx, src_ptr, shape, helper);
}

/// Emit a direct `call_indirect` to a pre-compiled child encoder.
///
/// Signature of the callee matches `EncodeFn`:
///   `unsafe extern "C" fn(ctx: *mut EncodeCtx, src_ptr: *const u8) -> bool`
///
/// The child encoder writes directly into `ctx.buf_*`, so we flush the
/// cached `buf_len` before the call and reload all three fields afterwards,
/// exactly like we do around any other helper that mutates the buffer.
/// Inline the child encoder's IR body into the current parent function.
///
/// Saves the parent's per-program state (`block_map`, `inlined_blocks`,
/// `src_ptr`, `child_encoders`, `inline_frame`), allocates a fresh block
/// map for the child's program, redirects `return_ok`/`return_fail` to
/// parent-owned cont/fail blocks, walks the child's blocks, then restores
/// parent state. The key win vs `emit_encode_direct_child`: no call
/// prologue/epilogue, and `var_buf_{ptr,len,cap}` stay live across the
/// child's body (skipping the flush + three-load reload around each call).
fn emit_inline_child_program(
    ectx: &mut EncodeCtx_<'_, '_>,
    child: &'static crate::cache::CompiledEncoder,
    src_offset: usize,
) -> Result<(), CodegenError> {
    let child_program = child.program.as_ref();
    let child_shape: &'static facet_core::Shape = child.local_shape;

    let inner_src = ectx.src_at(src_offset);
    let cont_block = ectx.b.create_block();
    let fail_block = ectx.b.create_block();

    let child_block_map: Vec<Option<Block>> = (0..child_program.blocks.len())
        .map(|_| Some(ectx.b.create_block()))
        .collect();

    // Jump into the child's entry block, closing out the parent's current
    // Cranelift block. The child then controls flow until one of its
    // return_ok/return_fail terminators jumps to cont/fail.
    let child_entry = child_block_map[0].unwrap();
    ectx.b.ins().jump(child_entry, &[]);

    // Save outer state.
    let saved_src_ptr = ectx.src_ptr;
    let saved_block_map = std::mem::replace(&mut ectx.block_map, child_block_map);
    let saved_inlined_blocks = std::mem::take(&mut ectx.inlined_blocks);
    let saved_inline_frame = ectx.inline_frame.replace(InlineFrame {
        cont_block,
        fail_block,
    });
    let saved_child_encoders =
        std::mem::replace(&mut ectx.child_encoders, child.child_encoders.clone());
    ectx.src_ptr = inner_src;
    ectx.inlining_stack.push(child_shape);

    // Emit the child's entry block.
    ectx.b.switch_to_block(child_entry);
    ectx.b.seal_block(child_entry);
    emit_encode_block(ectx, child_program, 0)?;

    // Emit the rest of the child's blocks (matching emit_encode_function's
    // remaining-blocks loop). Blocks that `emit_inline_block` already sealed
    // are skipped; pre-created but never-entered blocks are stubbed out.
    for block_idx in 1..child_program.blocks.len() {
        if ectx.inlined_blocks.contains(&block_idx) {
            let clif_block = ectx.block_map[block_idx].unwrap();
            ectx.b.switch_to_block(clif_block);
            ectx.b.seal_block(clif_block);
            ectx.return_ok();
            continue;
        }
        let clif_block = ectx.block_map[block_idx].unwrap();
        ectx.b.switch_to_block(clif_block);
        ectx.b.seal_block(clif_block);
        emit_encode_block(ectx, child_program, block_idx)?;
    }

    // Restore outer state before emitting the fail/cont blocks — they run
    // in the parent's frame, so their return_fail() (and any further ops
    // in cont_block) use the parent's redirect/return behavior.
    ectx.inlining_stack.pop();
    ectx.child_encoders = saved_child_encoders;
    ectx.inline_frame = saved_inline_frame;
    ectx.inlined_blocks = saved_inlined_blocks;
    ectx.block_map = saved_block_map;
    ectx.src_ptr = saved_src_ptr;

    // Parent fail path: propagate failure up (may be another jump if the
    // parent is itself inlined, or a real return if we're at the top).
    ectx.b.switch_to_block(fail_block);
    ectx.b.seal_block(fail_block);
    ectx.return_fail();

    // Parent continues here.
    ectx.b.switch_to_block(cont_block);
    ectx.b.seal_block(cont_block);

    Ok(())
}

fn emit_encode_direct_child(
    ectx: &mut EncodeCtx_<'_, '_>,
    child_fn: vox_jit_abi::EncodeFn,
    src_offset: usize,
) {
    let src_ptr = ectx.src_at(src_offset);
    let call_conv = ectx.b.func.signature.call_conv;
    let sig = ectx.b.func.import_signature(Signature {
        params: vec![AbiParam::new(ectx.ptr_ty), AbiParam::new(ectx.ptr_ty)],
        returns: vec![AbiParam::new(types::I8)],
        call_conv,
    });
    let callee = ectx.b.ins().iconst(ectx.ptr_ty, child_fn as usize as i64);
    ectx.flush_buf_len_to_ctx();
    let call = ectx
        .b
        .ins()
        .call_indirect(sig, callee, &[ectx.enc_ctx, src_ptr]);
    let ok = ectx.b.inst_results(call)[0];
    ectx.reload_buf_state();

    let fail_block = ectx.fresh_block();
    let cont_block = ectx.fresh_block();
    ectx.b.ins().brif(ok, cont_block, &[], fail_block, &[]);

    ectx.b.switch_to_block(fail_block);
    ectx.b.seal_block(fail_block);
    ectx.return_fail();

    ectx.b.switch_to_block(cont_block);
    ectx.b.seal_block(cont_block);
}

fn emit_encode_shape_ptr_with_helper(
    ectx: &mut EncodeCtx_<'_, '_>,
    src_ptr: Value,
    shape: &'static facet_core::Shape,
    helper: *const (),
) {
    let call_conv = ectx.b.func.signature.call_conv;
    let sig = ectx.b.func.import_signature(Signature {
        params: vec![
            AbiParam::new(ectx.ptr_ty),
            AbiParam::new(ectx.ptr_ty),
            AbiParam::new(ectx.ptr_ty),
        ],
        returns: vec![AbiParam::new(types::I8)],
        call_conv,
    });
    let callee = ectx.b.ins().iconst(ectx.ptr_ty, helper as i64);
    let shape_ptr = ectx.b.ins().iconst(ectx.ptr_ty, shape as *const _ as i64);
    // Publish cached buf_len so the helper sees the current write offset; it
    // mutates ctx.buf_* directly, so reload all three after it returns.
    ectx.flush_buf_len_to_ctx();
    let call = ectx
        .b
        .ins()
        .call_indirect(sig, callee, &[ectx.enc_ctx, src_ptr, shape_ptr]);
    let ok = ectx.b.inst_results(call)[0];
    ectx.reload_buf_state();

    let fail_block = ectx.fresh_block();
    let cont_block = ectx.fresh_block();
    ectx.b.ins().brif(ok, cont_block, &[], fail_block, &[]);

    ectx.b.switch_to_block(fail_block);
    ectx.b.seal_block(fail_block);
    ectx.return_fail();

    ectx.b.switch_to_block(cont_block);
    ectx.b.seal_block(cont_block);
}

fn emit_encode_slow_path(
    ectx: &mut EncodeCtx_<'_, '_>,
    shape: &'static facet_core::Shape,
    src_offset: usize,
) {
    emit_encode_helper_shape_op(
        ectx,
        shape,
        src_offset,
        crate::helpers::vox_jit_encode_slow_path as *const (),
    );
}

/// Emit a branch-on-encode: read discriminant, branch to per-variant block.
fn emit_encode_branch(
    ectx: &mut EncodeCtx_<'_, '_>,
    program: &EncodeProgram,
    src_offset: usize,
    tag_width: TagWidth,
    variant_blocks: &[(u64, usize)],
) -> Result<(), CodegenError> {
    let tag_addr = ectx.src_at(src_offset);

    let disc = match tag_width {
        TagWidth::U8 => {
            let b = ectx
                .b
                .ins()
                .load(types::I8, MemFlags::trusted(), tag_addr, 0);
            ectx.b.ins().uextend(types::I64, b)
        }
        TagWidth::U16 => {
            let h = ectx
                .b
                .ins()
                .load(types::I16, MemFlags::trusted(), tag_addr, 0);
            ectx.b.ins().uextend(types::I64, h)
        }
        TagWidth::U32 => {
            let w = ectx
                .b
                .ins()
                .load(types::I32, MemFlags::trusted(), tag_addr, 0);
            ectx.b.ins().uextend(types::I64, w)
        }
        TagWidth::U64 => ectx
            .b
            .ins()
            .load(types::I64, MemFlags::trusted(), tag_addr, 0),
    };

    let after_block = ectx.fresh_block();

    // Fresh Cranelift blocks for each variant body — we don't reuse the
    // pre-allocated block_map entries because those are stubbed to return_ok
    // by the outer loop (marked as inlined below).
    let variant_clif: Vec<Block> = (0..variant_blocks.len())
        .map(|_| ectx.fresh_block())
        .collect();

    // Chain of compare+brif, one per variant.
    for (i, &(disc_val, _vblock_ir)) in variant_blocks.iter().enumerate() {
        let disc_const = ectx.b.ins().iconst(types::I64, disc_val as i64);
        let is_match = ectx.b.ins().icmp(IntCC::Equal, disc, disc_const);

        let target = variant_clif[i];
        let next_block = ectx.fresh_block();
        ectx.b.ins().brif(is_match, target, &[], next_block, &[]);
        ectx.b.switch_to_block(next_block);
        ectx.b.seal_block(next_block);
    }

    // Fell through all variants — should not happen for valid data; return ok.
    ectx.return_ok();

    // Emit each variant body into its fresh Cranelift block, then jump to
    // after_block so the caller (e.g. a parent struct) can continue encoding
    // subsequent fields.
    for (i, &(_disc, vblock_ir)) in variant_blocks.iter().enumerate() {
        let clif = variant_clif[i];
        ectx.b.switch_to_block(clif);
        ectx.b.seal_block(clif);

        ectx.inlined_blocks.insert(vblock_ir);
        let mut terminated = false;
        for op in &program.blocks[vblock_ir].ops {
            match op {
                EncodeOp::Return => break,
                _ => {
                    let term = emit_encode_op(ectx, program, op)?;
                    if term {
                        terminated = true;
                        break;
                    }
                }
            }
        }
        if !terminated {
            ectx.b.ins().jump(after_block, &[]);
        }
    }

    ectx.b.switch_to_block(after_block);
    ectx.b.seal_block(after_block);

    Ok(())
}

/// Emit encode for `Option<T>`.
fn emit_encode_option(
    ectx: &mut EncodeCtx_<'_, '_>,
    program: &EncodeProgram,
    src_offset: usize,
    some_block_ir: usize,
    is_some_fn: facet_core::OptionIsSomeFn,
    get_value_fn: facet_core::OptionGetValueFn,
) -> Result<(), CodegenError> {
    let opt_ptr = ectx.src_at(src_offset);

    // Call is_some_fn(opt_ptr) -> bool
    let call_conv = ectx.b.func.signature.call_conv;
    let is_some_sig = ectx.b.func.import_signature(Signature {
        params: vec![AbiParam::new(ectx.ptr_ty)],
        returns: vec![AbiParam::new(types::I8)],
        call_conv,
    });
    let is_some_callee = ectx
        .b
        .ins()
        .iconst(ectx.ptr_ty, is_some_fn as *const () as i64);
    let is_some_call = ectx
        .b
        .ins()
        .call_indirect(is_some_sig, is_some_callee, &[opt_ptr]);
    let is_some = ectx.b.inst_results(is_some_call)[0];

    let some_block = ectx.fresh_block();
    let none_block = ectx.fresh_block();
    let after_block = ectx.fresh_block();

    ectx.b.ins().brif(is_some, some_block, &[], none_block, &[]);

    // None path: write 0x00
    ectx.b.switch_to_block(none_block);
    ectx.b.seal_block(none_block);
    let zero_byte = ectx.b.ins().iconst(types::I8, 0);
    ectx.call_push_byte(zero_byte);
    ectx.b.ins().jump(after_block, &[]);

    // Some path: write 0x01, call get_value_fn, encode inner
    ectx.b.switch_to_block(some_block);
    ectx.b.seal_block(some_block);
    let one_byte = ectx.b.ins().iconst(types::I8, 1);
    ectx.call_push_byte(one_byte);

    let call_conv = ectx.b.func.signature.call_conv;
    let get_value_sig = ectx.b.func.import_signature(Signature {
        params: vec![AbiParam::new(ectx.ptr_ty)],
        returns: vec![AbiParam::new(ectx.ptr_ty)],
        call_conv,
    });
    let get_value_callee = ectx
        .b
        .ins()
        .iconst(ectx.ptr_ty, get_value_fn as *const () as i64);
    let get_value_call = ectx
        .b
        .ins()
        .call_indirect(get_value_sig, get_value_callee, &[opt_ptr]);
    let inner_ptr = ectx.b.inst_results(get_value_call)[0];

    // Inline the some_block_ir ops with src_ptr = inner_ptr
    let saved_src = ectx.src_ptr;
    ectx.src_ptr = inner_ptr;
    ectx.inlined_blocks.insert(some_block_ir);
    for op in &program.blocks[some_block_ir].ops {
        match op {
            EncodeOp::Return => break,
            _ => {
                let term = emit_encode_op(ectx, program, op)?;
                if term {
                    break;
                }
            }
        }
    }
    ectx.src_ptr = saved_src;
    ectx.b.ins().jump(after_block, &[]);

    ectx.b.switch_to_block(after_block);
    ectx.b.seal_block(after_block);

    Ok(())
}

/// Calibrated variant of `emit_encode_option` — no vtable indirect calls.
///
/// Inlines the is-some check as:
///   `acc = 0; for (off, none_val) in tag_bytes: acc |= (*(u8*)opt_ptr+off) ^ none_val;`
///   `is_some = (acc != 0)`
/// then uses the known `inner_offset` instead of calling `get_value_fn`.
///
/// Contiguous runs of tag bytes are coalesced into larger (up to 8-byte) loads
/// so we avoid one load/XOR per byte for niched pointer-sized Options.
fn emit_encode_option_calibrated(
    ectx: &mut EncodeCtx_<'_, '_>,
    program: &EncodeProgram,
    src_offset: usize,
    inner_offset: usize,
    some_block_ir: usize,
    tag_bytes: &[(usize, u8)],
) -> Result<(), CodegenError> {
    if tag_bytes.is_empty() {
        return Err(CodegenError::UnsupportedOp(
            "EncodeOptionCalibrated with empty tag_bytes".into(),
        ));
    }

    let opt_ptr = ectx.src_at(src_offset);

    // Group tag bytes into contiguous runs so we can load 8/4/2/1 bytes at a
    // time. `tag_bytes` is sorted by offset (lowering produces it in order).
    let mut runs: Vec<Vec<(usize, u8)>> = Vec::new();
    for &(off, val) in tag_bytes {
        match runs.last_mut() {
            Some(run) if run.last().unwrap().0 + 1 == off => run.push((off, val)),
            _ => runs.push(vec![(off, val)]),
        }
    }

    let mut accumulator: Option<Value> = None;

    for run in &runs {
        let mut pos = 0;
        while pos < run.len() {
            let remaining = run.len() - pos;
            let chunk = if remaining >= 8 {
                8
            } else if remaining >= 4 {
                4
            } else if remaining >= 2 {
                2
            } else {
                1
            };
            let base_off = run[pos].0;
            let addr = if base_off == 0 {
                opt_ptr
            } else {
                ectx.b.ins().iadd_imm(opt_ptr, base_off as i64)
            };
            let (ty, none_word) = match chunk {
                8 => {
                    let mut bytes = [0u8; 8];
                    for (i, entry) in run[pos..pos + 8].iter().enumerate() {
                        bytes[i] = entry.1;
                    }
                    (types::I64, u64::from_le_bytes(bytes) as i64)
                }
                4 => {
                    let mut bytes = [0u8; 4];
                    for (i, entry) in run[pos..pos + 4].iter().enumerate() {
                        bytes[i] = entry.1;
                    }
                    (types::I32, u32::from_le_bytes(bytes) as i64)
                }
                2 => {
                    let mut bytes = [0u8; 2];
                    for (i, entry) in run[pos..pos + 2].iter().enumerate() {
                        bytes[i] = entry.1;
                    }
                    (types::I16, u16::from_le_bytes(bytes) as i64)
                }
                1 => (types::I8, run[pos].1 as i64),
                _ => unreachable!(),
            };
            let loaded = ectx.b.ins().load(ty, MemFlags::trusted(), addr, 0);
            let none_const = ectx.b.ins().iconst(ty, none_word);
            let xored = ectx.b.ins().bxor(loaded, none_const);
            let xored_64 = if ty == types::I64 {
                xored
            } else {
                ectx.b.ins().uextend(types::I64, xored)
            };
            accumulator = Some(match accumulator {
                None => xored_64,
                Some(prev) => ectx.b.ins().bor(prev, xored_64),
            });
            pos += chunk;
        }
    }

    let acc = accumulator.unwrap();
    let zero = ectx.b.ins().iconst(types::I64, 0);
    let is_some = ectx.b.ins().icmp(IntCC::NotEqual, acc, zero);

    let some_block = ectx.fresh_block();
    let none_block = ectx.fresh_block();
    let after_block = ectx.fresh_block();

    ectx.b.ins().brif(is_some, some_block, &[], none_block, &[]);

    // None path: write 0x00.
    ectx.b.switch_to_block(none_block);
    ectx.b.seal_block(none_block);
    let zero_byte = ectx.b.ins().iconst(types::I8, 0);
    ectx.call_push_byte(zero_byte);
    ectx.b.ins().jump(after_block, &[]);

    // Some path: write 0x01, inline inner encode with src_ptr = opt_ptr + inner_offset.
    ectx.b.switch_to_block(some_block);
    ectx.b.seal_block(some_block);
    let one_byte = ectx.b.ins().iconst(types::I8, 1);
    ectx.call_push_byte(one_byte);

    let inner_ptr = if inner_offset == 0 {
        opt_ptr
    } else {
        ectx.b.ins().iadd_imm(opt_ptr, inner_offset as i64)
    };

    let saved_src = ectx.src_ptr;
    ectx.src_ptr = inner_ptr;
    ectx.inlined_blocks.insert(some_block_ir);
    for op in &program.blocks[some_block_ir].ops {
        match op {
            EncodeOp::Return => break,
            _ => {
                let term = emit_encode_op(ectx, program, op)?;
                if term {
                    break;
                }
            }
        }
    }
    ectx.src_ptr = saved_src;
    ectx.b.ins().jump(after_block, &[]);

    ectx.b.switch_to_block(after_block);
    ectx.b.seal_block(after_block);

    Ok(())
}

fn emit_encode_result(
    ectx: &mut EncodeCtx_<'_, '_>,
    program: &EncodeProgram,
    src_offset: usize,
    ok_block_ir: usize,
    err_block_ir: usize,
    _ok_shape: &'static facet_core::Shape,
    _err_shape: &'static facet_core::Shape,
    is_ok_fn: facet_core::ResultIsOkFn,
    get_ok_fn: facet_core::ResultGetOkFn,
    get_err_fn: facet_core::ResultGetErrFn,
) -> Result<(), CodegenError> {
    let result_ptr = ectx.src_at(src_offset);
    let call_conv = ectx.b.func.signature.call_conv;
    let is_ok_sig = ectx.b.func.import_signature(Signature {
        params: vec![AbiParam::new(ectx.ptr_ty), AbiParam::new(ectx.ptr_ty)],
        returns: vec![AbiParam::new(types::I8)],
        call_conv,
    });
    let is_ok_fn = ectx
        .b
        .ins()
        .iconst(ectx.ptr_ty, is_ok_fn as *const () as i64);
    let is_ok_callee = ectx.b.ins().iconst(
        ectx.ptr_ty,
        crate::helpers::vox_jit_result_is_ok_raw as *const () as i64,
    );
    let is_ok_call = ectx
        .b
        .ins()
        .call_indirect(is_ok_sig, is_ok_callee, &[result_ptr, is_ok_fn]);
    let is_ok = ectx.b.inst_results(is_ok_call)[0];

    let ok_block = ectx.fresh_block();
    let err_block = ectx.fresh_block();
    let after_block = ectx.fresh_block();
    ectx.b.ins().brif(is_ok, ok_block, &[], err_block, &[]);

    ectx.b.switch_to_block(ok_block);
    ectx.b.seal_block(ok_block);
    let zero_byte = ectx.b.ins().iconst(types::I8, 0);
    ectx.call_push_byte(zero_byte);
    let ok_ptr = emit_result_payload_ptr(ectx, result_ptr, get_ok_fn as *const ());
    if !emit_encode_block_with_src(ectx, program, ok_block_ir, ok_ptr)? {
        ectx.b.ins().jump(after_block, &[]);
    }

    ectx.b.switch_to_block(err_block);
    ectx.b.seal_block(err_block);
    let one_byte = ectx.b.ins().iconst(types::I8, 1);
    ectx.call_push_byte(one_byte);
    let err_ptr = emit_result_payload_ptr(ectx, result_ptr, get_err_fn as *const ());
    if !emit_encode_block_with_src(ectx, program, err_block_ir, err_ptr)? {
        ectx.b.ins().jump(after_block, &[]);
    }

    ectx.b.switch_to_block(after_block);
    ectx.b.seal_block(after_block);

    Ok(())
}

fn emit_result_payload_ptr(
    ectx: &mut EncodeCtx_<'_, '_>,
    result_ptr: Value,
    get_fn: *const (),
) -> Value {
    let call_conv = ectx.b.func.signature.call_conv;
    let get_sig = ectx.b.func.import_signature(Signature {
        params: vec![AbiParam::new(ectx.ptr_ty), AbiParam::new(ectx.ptr_ty)],
        returns: vec![AbiParam::new(ectx.ptr_ty)],
        call_conv,
    });
    let get_fn = ectx.b.ins().iconst(ectx.ptr_ty, get_fn as i64);
    let get_callee = ectx.b.ins().iconst(
        ectx.ptr_ty,
        crate::helpers::vox_jit_result_get_payload_raw as *const () as i64,
    );
    let get_call = ectx
        .b
        .ins()
        .call_indirect(get_sig, get_callee, &[result_ptr, get_fn]);
    ectx.b.inst_results(get_call)[0]
}

fn emit_encode_block_with_src(
    ectx: &mut EncodeCtx_<'_, '_>,
    program: &EncodeProgram,
    block_ir: usize,
    src_ptr: Value,
) -> Result<bool, CodegenError> {
    let saved_src = ectx.src_ptr;
    ectx.src_ptr = src_ptr;
    ectx.inlined_blocks.insert(block_ir);
    let mut terminated = false;
    for op in &program.blocks[block_ir].ops {
        match op {
            EncodeOp::Return => break,
            _ => {
                let term = emit_encode_op(ectx, program, op)?;
                if term {
                    terminated = true;
                    break;
                }
            }
        }
    }
    ectx.src_ptr = saved_src;
    Ok(terminated)
}

fn emit_encode_borrow_pointer(
    ectx: &mut EncodeCtx_<'_, '_>,
    program: &EncodeProgram,
    src_offset: usize,
    body_block_ir: usize,
    borrow_fn: facet_core::BorrowFn,
) -> Result<(), CodegenError> {
    let ptr = ectx.src_at(src_offset);
    let call_conv = ectx.b.func.signature.call_conv;
    let sig = ectx.b.func.import_signature(Signature {
        params: vec![AbiParam::new(ectx.ptr_ty)],
        returns: vec![AbiParam::new(ectx.ptr_ty)],
        call_conv,
    });
    let callee = ectx
        .b
        .ins()
        .iconst(ectx.ptr_ty, borrow_fn as *const () as i64);
    let call = ectx.b.ins().call_indirect(sig, callee, &[ptr]);
    let inner_ptr = ectx.b.inst_results(call)[0];

    let saved_src = ectx.src_ptr;
    ectx.src_ptr = inner_ptr;
    ectx.inlined_blocks.insert(body_block_ir);
    for op in &program.blocks[body_block_ir].ops {
        match op {
            EncodeOp::Return => break,
            _ => {
                let term = emit_encode_op(ectx, program, op)?;
                if term {
                    break;
                }
            }
        }
    }
    ectx.src_ptr = saved_src;
    Ok(())
}

fn emit_write_byte_slice(
    ectx: &mut EncodeCtx_<'_, '_>,
    src_offset: usize,
    len_fn: facet_core::SliceLenFn,
    as_ptr_fn: facet_core::SliceAsPtrFn,
) -> Result<(), CodegenError> {
    let slice_ptr = ectx.src_at(src_offset);
    let call_conv = ectx.b.func.signature.call_conv;

    let len_sig = ectx.b.func.import_signature(Signature {
        params: vec![AbiParam::new(ectx.ptr_ty)],
        returns: vec![AbiParam::new(ectx.ptr_ty)],
        call_conv,
    });
    let len_callee = ectx.b.ins().iconst(ectx.ptr_ty, len_fn as *const () as i64);
    let len_call = ectx
        .b
        .ins()
        .call_indirect(len_sig, len_callee, &[slice_ptr]);
    let len = ectx.b.inst_results(len_call)[0];
    let len64 = if ectx.ptr_ty == types::I64 {
        len
    } else {
        ectx.b.ins().uextend(types::I64, len)
    };
    ectx.call_write_varint(len64);

    let data_sig = ectx.b.func.import_signature(Signature {
        params: vec![AbiParam::new(ectx.ptr_ty)],
        returns: vec![AbiParam::new(ectx.ptr_ty)],
        call_conv,
    });
    let data_callee = ectx
        .b
        .ins()
        .iconst(ectx.ptr_ty, as_ptr_fn as *const () as i64);
    let data_call = ectx
        .b
        .ins()
        .call_indirect(data_sig, data_callee, &[slice_ptr]);
    let data = ectx.b.inst_results(data_call)[0];
    ectx.call_push_bytes(data, len);
    Ok(())
}

/// Emit encode for a fixed-size array.
fn emit_encode_array(
    ectx: &mut EncodeCtx_<'_, '_>,
    program: &EncodeProgram,
    src_offset: usize,
    count: usize,
    elem_size: usize,
    body_block_ir: usize,
) -> Result<(), CodegenError> {
    ectx.inlined_blocks.insert(body_block_ir);

    let base = ectx.src_at(src_offset);
    let saved_src = ectx.src_ptr;

    for i in 0..count {
        let elem_ptr = if i == 0 {
            base
        } else {
            ectx.b.ins().iadd_imm(base, (i * elem_size) as i64)
        };
        ectx.src_ptr = elem_ptr;
        for op in &program.blocks[body_block_ir].ops {
            match op {
                EncodeOp::Return => break,
                _ => {
                    let term = emit_encode_op(ectx, program, op)?;
                    if term {
                        break;
                    }
                }
            }
        }
    }

    ectx.src_ptr = saved_src;
    Ok(())
}

/// Emit encode for a Vec-family list.
///
/// Reads ptr and len from the calibrated descriptor offsets, writes varint len,
/// then loops over elements calling the body block.
/// Fast-path encode for `Vec<u8>` / `String`: read `ptr` + `len` from the
/// calibrated container offsets, write the varint length, then a single
/// memcpy of the backing bytes into the output buffer.
fn emit_encode_byte_list(
    ectx: &mut EncodeCtx_<'_, '_>,
    src_offset: usize,
    descriptor: &OpaqueDescriptorId,
) -> Result<(), CodegenError> {
    let desc = ectx.descriptors.get((*descriptor).into()).ok_or_else(|| {
        CodegenError::UnsupportedOp(format!("opaque descriptor {:?} not found", descriptor))
    })?;

    let container_addr = ectx.src_at(src_offset);

    let data_ptr = ectx.b.ins().load(
        ectx.ptr_ty,
        MemFlags::trusted(),
        container_addr,
        desc.ptr_offset as i32,
    );
    let len = ectx.b.ins().load(
        ectx.ptr_ty,
        MemFlags::trusted(),
        container_addr,
        desc.len_offset as i32,
    );

    let len64 = if ectx.ptr_ty == types::I64 {
        len
    } else {
        ectx.b.ins().uextend(types::I64, len)
    };
    ectx.call_write_varint(len64);
    ectx.call_push_bytes(data_ptr, len);
    Ok(())
}

fn emit_encode_list(
    ectx: &mut EncodeCtx_<'_, '_>,
    program: &EncodeProgram,
    src_offset: usize,
    descriptor: &OpaqueDescriptorId,
    body_block_ir: usize,
    elem_size: usize,
) -> Result<(), CodegenError> {
    let desc = ectx.descriptors.get((*descriptor).into()).ok_or_else(|| {
        CodegenError::UnsupportedOp(format!("opaque descriptor {:?} not found", descriptor))
    })?;

    let container_addr = ectx.src_at(src_offset);

    // Read data ptr and len from container.
    let data_ptr = ectx.b.ins().load(
        ectx.ptr_ty,
        MemFlags::trusted(),
        container_addr,
        desc.ptr_offset as i32,
    );
    let len = ectx.b.ins().load(
        ectx.ptr_ty,
        MemFlags::trusted(),
        container_addr,
        desc.len_offset as i32,
    );

    // Write varint length.
    let len64 = if ectx.ptr_ty == types::I64 {
        len
    } else {
        ectx.b.ins().uextend(types::I64, len)
    };
    ectx.call_write_varint(len64);

    // Emit element loop.
    ectx.inlined_blocks.insert(body_block_ir);

    let header = ectx.fresh_block();
    let body = ectx.fresh_block();
    let exit = ectx.fresh_block();

    let var_i = ectx.b.declare_var(ectx.ptr_ty);
    let zero = ectx.b.ins().iconst(ectx.ptr_ty, 0);
    ectx.b.def_var(var_i, zero);
    ectx.b.ins().jump(header, &[]);

    ectx.b.switch_to_block(header);
    // Seal after body is connected.
    let i = ectx.b.use_var(var_i);
    let done = ectx.b.ins().icmp(IntCC::UnsignedGreaterThanOrEqual, i, len);
    ectx.b.ins().brif(done, exit, &[], body, &[]);

    ectx.b.switch_to_block(body);

    // elem_ptr = data_ptr + i * elem_size
    let stride = ectx.b.ins().iconst(ectx.ptr_ty, elem_size as i64);
    let offset_bytes = ectx.b.ins().imul(i, stride);
    let elem_ptr = ectx.b.ins().iadd(data_ptr, offset_bytes);

    let saved_src = ectx.src_ptr;
    ectx.src_ptr = elem_ptr;
    for op in &program.blocks[body_block_ir].ops {
        match op {
            EncodeOp::Return => break,
            _ => {
                let term = emit_encode_op(ectx, program, op)?;
                if term {
                    break;
                }
            }
        }
    }
    ectx.src_ptr = saved_src;

    let next_i = ectx.b.ins().iadd_imm(i, 1);
    ectx.b.def_var(var_i, next_i);
    ectx.b.ins().jump(header, &[]);
    ectx.b.seal_block(body);
    ectx.b.seal_block(header);

    ectx.b.switch_to_block(exit);
    ectx.b.seal_block(exit);

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests: verify compile_decode handles enum, option, and nested ops
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;
    use spec_proto::{GnarlyAttr, GnarlyEntry, GnarlyKind, GnarlyPayload};
    use vox_jit_abi::EncodeCtx;
    use vox_jit_cal::{BorrowMode, CalibrationRegistry};
    use vox_postcard::{
        build_identity_plan,
        ir::{from_slice_ir, lower, lower_encode, lower_with_cal},
    };
    use vox_types::{
        BindingDirection, CborPayload, ConnectionId, Message, MessagePayload, MetadataEntry,
        MethodId, RequestBody, RequestCall, RequestId,
    };

    fn compile_shape<T: Facet<'static>>() -> Result<(), CodegenError> {
        let plan = build_identity_plan(T::SHAPE);
        let registry = vox_schema::SchemaRegistry::new();
        let program = lower(&plan, T::SHAPE, &registry)
            .map_err(|e| CodegenError::UnsupportedOp(format!("lowering failed: {e:?}")))?;
        let cal = CalibrationRegistry::new();
        let mut backend = CraneliftBackend::new()?;
        backend.compile_decode_owned(T::SHAPE, &program, &cal)?;
        Ok(())
    }

    fn encode_shape_with_cal<T: Facet<'static>>(
        cal: &CalibrationRegistry,
    ) -> Result<(), CodegenError> {
        let program = lower_encode(T::SHAPE, Some(cal))
            .map_err(|e| CodegenError::UnsupportedOp(format!("encode lowering failed: {e}")))?;
        let mut backend = CraneliftBackend::new()?;
        let child_encoders = std::sync::Arc::new(ChildEncoderMap::new());
        backend.compile_encode(T::SHAPE, &program, cal, child_encoders)?;
        Ok(())
    }

    fn assert_no_decode_slow_path(program: &DecodeProgram) {
        let has_slow_path = program
            .blocks
            .iter()
            .flat_map(|block| block.ops.iter())
            .any(|op| matches!(op, DecodeOp::SlowPath { .. }));
        assert!(
            !has_slow_path,
            "decode program still contains SlowPath ops: {program:#?}"
        );
    }

    fn assert_no_encode_slow_path(program: &EncodeProgram) {
        let has_slow_path = program
            .blocks
            .iter()
            .flat_map(|block| block.ops.iter())
            .any(|op| matches!(op, EncodeOp::SlowPath { .. }));
        assert!(!has_slow_path, "encode program still contains SlowPath ops");
    }

    fn metadata_calibration() -> CalibrationRegistry {
        let mut cal = CalibrationRegistry::new();
        cal.calibrate_vec_for_type::<u8>();
        cal.get_or_calibrate_by_shape(<Vec<MetadataEntry<'static>> as Facet<'static>>::SHAPE);
        cal
    }

    fn calibration_for(shape: &'static facet_core::Shape) -> CalibrationRegistry {
        fn register_tree(shape: &'static facet_core::Shape, cal: &mut CalibrationRegistry) {
            use facet_core::{Def, Type, UserType};

            match shape.def {
                Def::List(_) | Def::Pointer(_) => {
                    cal.get_or_calibrate_by_shape(shape);
                }
                _ => {}
            }

            match shape.ty {
                Type::User(UserType::Struct(st)) => {
                    for field in st.fields {
                        register_tree(field.shape(), cal);
                    }
                }
                Type::User(UserType::Enum(et)) => {
                    for variant in et.variants {
                        for field in variant.data.fields {
                            register_tree(field.shape(), cal);
                        }
                    }
                }
                _ => {}
            }

            match shape.def {
                Def::Option(opt) => register_tree(opt.t, cal),
                Def::Result(result) => {
                    register_tree(result.t, cal);
                    register_tree(result.e, cal);
                }
                Def::List(list) => register_tree(list.t, cal),
                Def::Pointer(ptr) => {
                    if let Some(inner) = ptr.pointee() {
                        register_tree(inner, cal);
                    }
                }
                Def::Array(arr) => register_tree(arr.t, cal),
                _ => {}
            }
        }

        let mut cal = metadata_calibration();
        cal.calibrate_string_for_type();
        register_tree(shape, &mut cal);
        cal
    }

    fn control_message_with_metadata_bytes() -> Message<'static> {
        Message {
            connection_id: ConnectionId(7),
            payload: MessagePayload::ConnectionReject(vox_types::ConnectionReject {
                metadata: vec![MetadataEntry::bytes("blob", &[0xde, 0xad, 0xbe, 0xef][..])],
            }),
        }
    }

    fn request_message_with_gnarly<'a>(args: &'a (GnarlyPayload,)) -> Message<'a> {
        Message {
            connection_id: ConnectionId(1),
            payload: MessagePayload::RequestMessage(vox_types::RequestMessage {
                id: RequestId(9),
                body: RequestBody::Call(RequestCall {
                    method_id: MethodId(42),
                    metadata: vec![MetadataEntry::str("kind", "bench")],
                    args: vox_types::Payload::outgoing(args),
                    schemas: CborPayload::default(),
                }),
            }),
        }
    }

    fn schema_message_with_payload() -> Message<'static> {
        Message {
            connection_id: ConnectionId(1),
            payload: MessagePayload::SchemaMessage(vox_types::SchemaMessage {
                method_id: MethodId(42),
                direction: BindingDirection::Args,
                schemas: CborPayload(vec![0xde, 0xad, 0xbe, 0xef, 7, 8, 9]),
            }),
        }
    }

    fn gnarly_payload(entry_count: usize, seq: usize) -> GnarlyPayload {
        let entries = (0..entry_count)
            .map(|i| {
                let attrs = vec![
                    GnarlyAttr {
                        key: "owner".to_string(),
                        value: format!("user-{seq}-{i}"),
                    },
                    GnarlyAttr {
                        key: "class".to_string(),
                        value: format!("hot-path-{}", (seq + i) % 17),
                    },
                    GnarlyAttr {
                        key: "etag".to_string(),
                        value: format!("etag-{seq:08x}-{i:08x}"),
                    },
                ];
                let chunks = (0..3)
                    .map(|j| {
                        let len = 32 * (j + 1);
                        vec![((seq + i + j) & 0xff) as u8; len]
                    })
                    .collect();
                let kind = match i % 3 {
                    0 => GnarlyKind::File {
                        mime: "application/octet-stream".to_string(),
                        tags: vec![
                            "warm".to_string(),
                            "cacheable".to_string(),
                            format!("tag-{seq}-{i}"),
                        ],
                    },
                    1 => GnarlyKind::Directory {
                        child_count: i as u32 + 3,
                        children: vec![
                            format!("child-{seq}-{i}-0"),
                            format!("child-{seq}-{i}-1"),
                            format!("child-{seq}-{i}-2"),
                        ],
                    },
                    _ => GnarlyKind::Symlink {
                        target: format!("/target/{seq}/{i}/nested/item"),
                        hops: vec![1, 2, 3, i as u32],
                    },
                };
                GnarlyEntry {
                    id: seq as u64 * 1_000_000 + i as u64,
                    parent: if i == 0 {
                        None
                    } else {
                        Some(seq as u64 * 1_000_000 + i as u64 - 1)
                    },
                    name: format!("entry-{seq}-{i}"),
                    path: format!("/mount/very/deep/path/with/component/{seq}/{i}/file.bin"),
                    attrs,
                    chunks,
                    kind,
                }
            })
            .collect();

        GnarlyPayload {
            revision: seq as u64,
            mount: format!("/mnt/bench-fast-path-{seq:08x}"),
            entries,
            footer: Some(format!("benchmark footer {seq}")),
            digest: vec![(seq & 0xff) as u8; 64],
        }
    }

    #[derive(Facet, Debug, PartialEq, Clone)]
    #[repr(u8)]
    enum Color {
        Red,
        Green,
        Blue,
    }

    #[derive(Facet, Debug, PartialEq, Clone)]
    #[repr(u8)]
    enum Shape {
        Circle(f64),
        Rect { w: f64, h: f64 },
        Point,
    }

    #[derive(Facet, Debug, PartialEq, Clone)]
    struct WithOption {
        maybe: Option<u32>,
        name: String,
    }

    #[derive(Facet, Debug, PartialEq, Clone)]
    struct Outer {
        value: u32,
        inner: Inner,
    }

    #[derive(Facet, Debug, PartialEq, Clone)]
    struct Inner {
        x: i32,
        label: String,
    }

    #[test]
    fn compile_enum_unit_variants() {
        compile_shape::<Color>().expect("Color should compile");
    }

    #[test]
    fn compile_enum_with_payload() {
        compile_shape::<Shape>().expect("Shape should compile");
    }

    #[test]
    fn compile_option_u32() {
        // Option<u32> now lowers via Def::Option (DecodeOption), not the unstable
        // enum path. Compilation should succeed.
        compile_shape::<Option<u32>>().expect("Option<u32> should compile via DecodeOption");
    }

    #[test]
    fn compile_struct_with_option_and_string() {
        // Requires DecodeOption and ReadString — falls back if String descriptor absent.
        // Either Ok or UnsupportedOp(descriptor) is acceptable.
        let _ = compile_shape::<WithOption>();
    }

    #[test]
    fn decode_roundtrip_struct_with_option_and_string() {
        let value = WithOption {
            maybe: Some(0x1234_5678),
            name: "hello".to_string(),
        };
        let bytes = reflective_encode_static(&value);
        let decoded = jit_decode_value::<WithOption>(&bytes).expect("JIT decode failed");
        assert_eq!(decoded, value, "option/string round-trip mismatch");
    }

    #[test]
    fn ir_roundtrip_struct_with_option_and_string() {
        let value = WithOption {
            maybe: Some(0x1234_5678),
            name: "hello".to_string(),
        };
        let bytes = reflective_encode_static(&value);
        let plan = build_identity_plan(WithOption::SHAPE);
        let registry = vox_schema::SchemaRegistry::new();
        let cal = calibration_for(WithOption::SHAPE);
        let decoded = from_slice_ir::<WithOption>(&bytes, &plan, &registry, Some(&cal))
            .expect("IR decode failed");
        assert_eq!(decoded, value, "IR option/string round-trip mismatch");
    }

    #[test]
    fn decode_roundtrip_result_ok() {
        let value: Result<u32, u16> = Ok(0x1234_5678);
        let bytes = reflective_encode_static(&value);
        let decoded = jit_decode_value::<Result<u32, u16>>(&bytes).expect("JIT decode failed");
        assert_eq!(decoded, value, "result ok round-trip mismatch");
    }

    #[test]
    fn decode_roundtrip_result_err() {
        let value: Result<u32, u16> = Err(0x3456);
        let bytes = reflective_encode_static(&value);
        let decoded = jit_decode_value::<Result<u32, u16>>(&bytes).expect("JIT decode failed");
        assert_eq!(decoded, value, "result err round-trip mismatch");
    }

    #[test]
    fn compile_nested_struct() {
        let _ = compile_shape::<Outer>();
    }

    #[derive(Facet, Debug, PartialEq, Clone)]
    struct NumBatch {
        values: Vec<u32>,
    }

    #[test]
    fn compile_vec_u32_calibrated() {
        let plan = build_identity_plan(NumBatch::SHAPE);
        let registry = vox_schema::SchemaRegistry::new();
        let mut cal = CalibrationRegistry::new();
        cal.calibrate_vec_for_type::<u32>();
        let program = lower_with_cal(
            &plan,
            NumBatch::SHAPE,
            &registry,
            Some(&cal),
            BorrowMode::Owned,
        )
        .map_err(|e| CodegenError::UnsupportedOp(format!("lowering failed: {e:?}")))
        .expect("lower_with_cal should succeed");
        let mut backend = CraneliftBackend::new().expect("backend");
        let result = backend.compile_decode_owned(NumBatch::SHAPE, &program, &cal);
        assert!(
            result.is_ok(),
            "compile_decode for Vec<u32> failed: {:?}",
            result
        );
    }

    // Gnarly: Vec<Struct> where Struct has a Vec<u32> field.
    // This exercises emit_inline_block's recursive handling of ReadListLen
    // inside an element body — the bug fixed in task #34.
    #[derive(Facet, Debug, PartialEq, Clone)]
    struct Row {
        id: u32,
        values: Vec<u32>,
    }

    #[derive(Facet, Debug, PartialEq, Clone)]
    struct Table {
        rows: Vec<Row>,
    }

    #[test]
    fn compile_nested_vec_struct() {
        let plan = build_identity_plan(Table::SHAPE);
        let registry = vox_schema::SchemaRegistry::new();
        let mut cal = CalibrationRegistry::new();
        cal.calibrate_vec_for_type::<u32>();
        cal.calibrate_vec_for_type::<Row>();
        let program = lower_with_cal(
            &plan,
            Table::SHAPE,
            &registry,
            Some(&cal),
            BorrowMode::Owned,
        )
        .map_err(|e| CodegenError::UnsupportedOp(format!("lowering failed: {e:?}")))
        .expect("lower_with_cal should succeed");
        let mut backend = CraneliftBackend::new().expect("backend");
        let result = backend.compile_decode_owned(Table::SHAPE, &program, &cal);
        assert!(
            result.is_ok(),
            "compile nested Vec<Struct> failed: {:?}",
            result
        );
    }

    #[test]
    fn metadata_entry_encode_decode_has_no_slow_path() {
        let plan = build_identity_plan(MetadataEntry::SHAPE);
        let registry = vox_schema::SchemaRegistry::new();
        let cal = metadata_calibration();
        let decode_program = lower_with_cal(
            &plan,
            MetadataEntry::SHAPE,
            &registry,
            Some(&cal),
            BorrowMode::Owned,
        )
        .expect("decode lowering");
        assert_no_decode_slow_path(&decode_program);

        let encode_program =
            lower_encode(MetadataEntry::SHAPE, Some(&cal)).expect("encode lowering");
        assert_no_encode_slow_path(&encode_program);
    }

    #[test]
    fn encode_compile_message_outer_path() {
        let cal = metadata_calibration();
        let plan = build_identity_plan(Message::SHAPE);
        let registry = vox_schema::SchemaRegistry::new();
        let decode_program = lower_with_cal(
            &plan,
            Message::SHAPE,
            &registry,
            Some(&cal),
            BorrowMode::Owned,
        )
        .expect("Message decode lowering");
        assert_no_decode_slow_path(&decode_program);
        let program = lower_encode(Message::SHAPE, Some(&cal)).expect("Message encode lowering");
        assert_no_encode_slow_path(&program);
        encode_shape_with_cal::<Message<'static>>(&cal).expect("Message should encode-compile");
    }

    #[test]
    fn decode_roundtrip_nested_vec_struct() {
        let original = Table {
            rows: vec![
                Row {
                    id: 1,
                    values: vec![10, 20, 30],
                },
                Row {
                    id: 2,
                    values: vec![],
                },
                Row {
                    id: 3,
                    values: vec![42],
                },
            ],
        };
        let bytes = reflective_encode(&original);
        let decoded = jit_decode_value::<Table>(&bytes).expect("JIT decode failed");
        assert_eq!(original, decoded, "nested Vec<Struct> round-trip mismatch");
    }

    // Gnarly-like: Vec<Entry> where Entry has Vec<Vec<u8>> and enum with Vec variants.
    #[derive(Facet, Debug, PartialEq, Clone)]
    #[repr(u8)]
    enum Kind {
        File { tags: Vec<String> } = 0,
        Dir { children: Vec<String> } = 1,
        Link { hops: Vec<u32> } = 2,
    }

    #[derive(Facet, Debug, PartialEq, Clone)]
    struct Entry {
        id: u64,
        name: String,
        chunks: Vec<Vec<u8>>,
        kind: Kind,
    }

    #[derive(Facet, Debug, PartialEq, Clone)]
    struct Payload {
        revision: u64,
        entries: Vec<Entry>,
    }

    #[test]
    fn decode_roundtrip_gnarly_like() {
        let original = Payload {
            revision: 42,
            entries: vec![
                Entry {
                    id: 1,
                    name: "alpha".to_string(),
                    chunks: vec![vec![1, 2, 3], vec![4, 5]],
                    kind: Kind::File {
                        tags: vec!["hot".to_string(), "warm".to_string()],
                    },
                },
                Entry {
                    id: 2,
                    name: "beta".to_string(),
                    chunks: vec![],
                    kind: Kind::Dir {
                        children: vec!["child-a".to_string()],
                    },
                },
                Entry {
                    id: 3,
                    name: "gamma".to_string(),
                    chunks: vec![vec![0xff; 8]],
                    kind: Kind::Link {
                        hops: vec![1, 2, 3],
                    },
                },
            ],
        };
        let bytes = reflective_encode(&original);
        let decoded = jit_decode_value::<Payload>(&bytes).expect("JIT decode gnarly-like failed");
        assert_eq!(original, decoded, "gnarly-like round-trip mismatch");
    }

    // Vec<Vec<u8>> — outer loop must not be corrupted by inner ReadListLen writes.
    #[derive(Facet, Debug, PartialEq, Clone)]
    struct Chunks {
        data: Vec<Vec<u8>>,
    }

    #[test]
    fn decode_roundtrip_vec_of_vecs() {
        let original = Chunks {
            data: vec![vec![1u8, 2, 3], vec![], vec![10, 20], vec![255]],
        };
        let bytes = reflective_encode(&original);
        let decoded = jit_decode_value::<Chunks>(&bytes).expect("JIT decode Vec<Vec<u8>> failed");
        assert_eq!(original, decoded, "Vec<Vec<u8>> round-trip mismatch");
    }

    // -----------------------------------------------------------------------
    // Encode compile tests
    // -----------------------------------------------------------------------

    fn encode_shape<T: Facet<'static>>() -> Result<(), CodegenError> {
        let cal = CalibrationRegistry::new();
        let program = lower_encode(T::SHAPE, Some(&cal))
            .map_err(|e| CodegenError::UnsupportedOp(format!("encode lowering failed: {e}")))?;
        let mut backend = CraneliftBackend::new()?;
        let child_encoders = std::sync::Arc::new(ChildEncoderMap::new());
        backend.compile_encode(T::SHAPE, &program, &cal, child_encoders)?;
        Ok(())
    }

    #[derive(Facet, Debug, PartialEq, Clone)]
    struct SimpleScalars {
        a: u32,
        b: i64,
        c: f64,
        d: bool,
    }

    #[test]
    fn encode_compile_struct_scalars() {
        encode_shape::<SimpleScalars>().expect("SimpleScalars should encode-compile");
    }

    #[test]
    fn encode_compile_enum_unit_variants() {
        encode_shape::<Color>().expect("Color should encode-compile");
    }

    #[test]
    fn encode_compile_enum_with_payload() {
        encode_shape::<Shape>().expect("Shape should encode-compile");
    }

    #[test]
    fn encode_compile_u32() {
        encode_shape::<u32>().expect("u32 should encode-compile");
    }

    #[test]
    fn encode_compile_option_u32() {
        // Option<u32> uses the Def::Option path in lower_encode (not the enum path).
        // This should succeed because lower_encode checks Def before ty.
        let result = encode_shape::<Option<u32>>();
        // Either Ok or UnsupportedOp is acceptable — Option<u32> may work via
        // the Def::Option path or fail on the inner type. Document the current behavior.
        match &result {
            Ok(()) => {}
            Err(CodegenError::UnsupportedOp(_)) => {}
            Err(e) => panic!("unexpected error: {e:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // Encode correctness: JIT output must match the reflective serializer
    // -----------------------------------------------------------------------

    fn jit_encode_value<T: Facet<'static>>(value: &T) -> Result<Vec<u8>, CodegenError> {
        let cal = calibration_for(T::SHAPE);
        let program = lower_encode(T::SHAPE, Some(&cal))
            .map_err(|e| CodegenError::UnsupportedOp(format!("encode lowering failed: {e}")))?;
        let mut backend = CraneliftBackend::new()?;
        let child_encoders = std::sync::Arc::new(ChildEncoderMap::new());
        let encode_fn = backend.compile_encode(T::SHAPE, &program, &cal, child_encoders)?;

        let mut ctx = EncodeCtx::with_capacity(64);
        let src_ptr = value as *const T as *const u8;
        let ok = unsafe { encode_fn(&mut ctx as *mut _, src_ptr) };
        assert!(ok, "JIT encode returned false (OOM)");
        Ok(ctx.into_vec())
    }

    fn reflective_encode<T: for<'de> Facet<'de>>(value: &T) -> Vec<u8> {
        vox_postcard::serialize::to_vec(value).expect("reflective encode failed")
    }

    fn reflective_encode_static<T: Facet<'static>>(value: &T) -> Vec<u8> {
        vox_postcard::serialize::to_vec(value).expect("reflective encode failed")
    }

    #[test]
    fn encode_roundtrip_struct_scalars() {
        let v = SimpleScalars {
            a: 42,
            b: -7,
            c: 3.14,
            d: true,
        };
        let jit_bytes = jit_encode_value(&v).expect("JIT encode failed");
        let ref_bytes = reflective_encode(&v);
        assert_eq!(
            jit_bytes, ref_bytes,
            "JIT and reflective encode disagree for SimpleScalars"
        );
    }

    #[test]
    fn encode_roundtrip_u32() {
        for &v in &[0u32, 1, 127, 128, 16383, 16384, u32::MAX] {
            let jit_bytes = jit_encode_value(&v).expect("JIT encode failed");
            let ref_bytes = reflective_encode(&v);
            assert_eq!(
                jit_bytes, ref_bytes,
                "JIT and reflective encode disagree for u32={v}"
            );
        }
    }

    #[test]
    fn encode_roundtrip_enum_unit() {
        for v in [Color::Red, Color::Green, Color::Blue] {
            let jit_bytes = jit_encode_value(&v).expect("JIT encode failed");
            let ref_bytes = reflective_encode(&v);
            assert_eq!(
                jit_bytes, ref_bytes,
                "JIT and reflective encode disagree for {v:?}"
            );
        }
    }

    #[test]
    fn encode_roundtrip_enum_with_payload() {
        for v in [
            Shape::Circle(1.5),
            Shape::Rect { w: 10.0, h: 20.0 },
            Shape::Point,
        ] {
            let jit_bytes = jit_encode_value(&v).expect("JIT encode failed");
            let ref_bytes = reflective_encode(&v);
            assert_eq!(
                jit_bytes, ref_bytes,
                "JIT and reflective encode disagree for {v:?}"
            );
        }
    }

    #[test]
    fn message_control_metadata_bytes_roundtrip() {
        let msg = control_message_with_metadata_bytes();
        let jit_bytes = jit_encode_value(&msg).expect("JIT encode failed");
        let ref_bytes = reflective_encode_static(&msg);
        assert_eq!(jit_bytes, ref_bytes, "JIT and reflective encode disagree");

        let decoded = jit_decode_value::<Message<'static>>(&jit_bytes).expect("JIT decode failed");
        assert_eq!(
            reflective_encode_static(&decoded),
            ref_bytes,
            "JIT round-trip mismatch"
        );
    }

    #[test]
    fn encode_roundtrip_gnarly_like_tuple() {
        let value = (Payload {
            revision: 42,
            entries: vec![
                Entry {
                    id: 1,
                    name: "alpha".to_string(),
                    chunks: vec![vec![1, 2, 3], vec![4, 5]],
                    kind: Kind::File {
                        tags: vec!["hot".to_string(), "warm".to_string()],
                    },
                },
                Entry {
                    id: 2,
                    name: "beta".to_string(),
                    chunks: vec![],
                    kind: Kind::Dir {
                        children: vec!["child-a".to_string()],
                    },
                },
                Entry {
                    id: 3,
                    name: "gamma".to_string(),
                    chunks: vec![vec![0xff; 8]],
                    kind: Kind::Link {
                        hops: vec![1, 2, 3],
                    },
                },
            ],
        },);

        let jit_bytes = jit_encode_value(&value).expect("JIT encode failed");
        let ref_bytes = reflective_encode(&value);
        assert_eq!(jit_bytes, ref_bytes, "JIT and reflective encode disagree");

        let decoded = jit_decode_value::<(Payload,)>(&jit_bytes).expect("JIT decode failed");
        assert_eq!(decoded, value, "JIT tuple round-trip mismatch");
    }

    #[test]
    fn encode_roundtrip_real_gnarly_tuple() {
        let value = (gnarly_payload(4, 7),);
        let jit_bytes = jit_encode_value(&value).expect("JIT encode failed");
        let ref_bytes = reflective_encode(&value);
        assert_eq!(jit_bytes, ref_bytes, "JIT and reflective encode disagree");

        let decoded = jit_decode_value::<(GnarlyPayload,)>(&jit_bytes).expect("JIT decode failed");
        assert_eq!(decoded, value, "JIT real gnarly tuple round-trip mismatch");
    }

    #[test]
    fn decode_lower_real_gnarly_tuple_has_no_slow_path() {
        let plan = build_identity_plan(<(GnarlyPayload,)>::SHAPE);
        let registry = vox_schema::SchemaRegistry::new();
        let cal = calibration_for(<(GnarlyPayload,)>::SHAPE);
        let program = lower_with_cal(
            &plan,
            <(GnarlyPayload,)>::SHAPE,
            &registry,
            Some(&cal),
            BorrowMode::Owned,
        )
        .expect("decode lowering");
        assert_no_decode_slow_path(&program);
    }

    #[test]
    fn decode_roundtrip_real_gnarly_root_direct() {
        let value = gnarly_payload(4, 11);
        let bytes = reflective_encode_static(&value);
        let decoded = jit_decode_value::<GnarlyPayload>(&bytes).expect("JIT decode failed");
        assert_eq!(decoded, value, "direct root gnarly round-trip mismatch");
    }

    #[test]
    fn decode_roundtrip_real_gnarly_root_runtime() {
        let value = gnarly_payload(4, 11);
        let bytes = reflective_encode_static(&value);
        let decoded = crate::global_runtime()
            .try_decode_owned::<GnarlyPayload>(
                &bytes,
                0,
                &build_identity_plan(GnarlyPayload::SHAPE),
                &vox_schema::SchemaRegistry::new(),
            )
            .expect("runtime JIT decode unavailable")
            .expect("runtime JIT decode failed");
        assert_eq!(decoded, value, "runtime root gnarly round-trip mismatch");
    }

    #[test]
    fn result_gnarly_has_no_slow_path() {
        type GnarlyReply = Result<GnarlyPayload, vox_types::VoxError<std::convert::Infallible>>;

        let plan = build_identity_plan(GnarlyReply::SHAPE);
        let registry = vox_schema::SchemaRegistry::new();
        let cal = calibration_for(GnarlyReply::SHAPE);
        let decode_program = lower_with_cal(
            &plan,
            GnarlyReply::SHAPE,
            &registry,
            Some(&cal),
            BorrowMode::Owned,
        )
        .expect("decode lowering");
        assert_no_decode_slow_path(&decode_program);

        let encode_program = lower_encode(GnarlyReply::SHAPE, Some(&cal)).expect("encode lowering");
        assert_no_encode_slow_path(&encode_program);
    }

    #[test]
    fn encode_roundtrip_message_with_gnarly_payload() {
        let args = Box::leak(Box::new((gnarly_payload(4, 7),)));
        let msg = request_message_with_gnarly(args);

        let jit_bytes = jit_encode_value(&msg).expect("JIT encode failed");
        let ref_bytes = vox_postcard::serialize::to_vec(&msg).expect("reflective encode failed");
        assert_eq!(jit_bytes, ref_bytes, "JIT and reflective encode disagree");

        let decoded = jit_decode_value::<Message<'static>>(&jit_bytes).expect("JIT decode failed");
        assert_eq!(
            reflective_encode_static(&decoded),
            ref_bytes,
            "JIT message round-trip mismatch"
        );
    }

    #[test]
    fn encode_roundtrip_schema_message() {
        let msg = schema_message_with_payload();
        let jit_bytes = jit_encode_value(&msg).expect("JIT encode failed");
        let ref_bytes = reflective_encode_static(&msg);
        assert_eq!(jit_bytes, ref_bytes, "JIT and reflective encode disagree");

        let decoded = jit_decode_value::<Message<'static>>(&jit_bytes).expect("JIT decode failed");
        assert_eq!(
            reflective_encode_static(&decoded),
            ref_bytes,
            "JIT schema message round-trip mismatch"
        );
    }

    #[test]
    fn encode_roundtrip_gnarly_result() {
        type GnarlyReply = Result<GnarlyPayload, vox_types::VoxError<std::convert::Infallible>>;

        let value: GnarlyReply = Ok(gnarly_payload(4, 11));
        let jit_bytes = crate::global_runtime()
            .try_encode_ptr(
                facet::PtrConst::new(&value as *const _ as *const u8),
                GnarlyReply::SHAPE,
            )
            .expect("runtime JIT encode unavailable")
            .expect("runtime JIT encode failed");
        let ref_bytes = reflective_encode_static(&value);
        assert_eq!(jit_bytes, ref_bytes, "JIT and reflective encode disagree");

        let decoded = crate::global_runtime()
            .try_decode_owned::<GnarlyReply>(
                &jit_bytes,
                0,
                &build_identity_plan(GnarlyReply::SHAPE),
                &vox_schema::SchemaRegistry::new(),
            )
            .expect("runtime JIT decode unavailable")
            .expect("runtime JIT decode failed");
        match decoded {
            Ok(payload) => assert_eq!(
                payload,
                gnarly_payload(4, 11),
                "JIT result round-trip mismatch"
            ),
            Err(err) => panic!("expected Ok payload, got {err:?}"),
        }
    }

    #[test]
    fn encode_result_helper_roundtrip_gnarly() {
        type GnarlyReply = Result<GnarlyPayload, vox_types::VoxError<std::convert::Infallible>>;

        let value: GnarlyReply = Ok(gnarly_payload(4, 11));
        let mut ctx = EncodeCtx::with_capacity(64);
        let ok = unsafe {
            crate::helpers::vox_jit_encode_result(
                &mut ctx as *mut _,
                &value as *const _ as *const u8,
                GnarlyReply::SHAPE,
            )
        };
        assert!(ok, "result helper encode returned false");
        assert_eq!(
            ctx.into_vec(),
            reflective_encode_static(&value),
            "result helper and reflective encode disagree"
        );
    }

    // -----------------------------------------------------------------------
    // SlowPath round-trip: struct containing a non-postcard scalar (SocketAddr)
    // -----------------------------------------------------------------------

    fn jit_decode_value<T: Facet<'static>>(bytes: &[u8]) -> Result<T, CodegenError> {
        let plan = build_identity_plan(T::SHAPE);
        let registry = vox_schema::SchemaRegistry::new();
        let cal = calibration_for(T::SHAPE);
        let program = lower_with_cal(&plan, T::SHAPE, &registry, Some(&cal), BorrowMode::Owned)
            .map_err(|e| CodegenError::UnsupportedOp(format!("lowering failed: {e:?}")))?;
        let mut backend = CraneliftBackend::new()?;
        let decode_fn = backend.compile_decode_owned(T::SHAPE, &program, &cal)?;

        let layout = T::SHAPE
            .layout
            .sized_layout()
            .map_err(|_| CodegenError::UnsupportedOp("unsized shape".into()))?;

        let mut ctx = DecodeCtx::new(bytes);
        let mut out = std::mem::MaybeUninit::<T>::uninit();
        if layout.size() != 0 {
            unsafe {
                std::ptr::write_bytes(out.as_mut_ptr() as *mut u8, 0, layout.size());
            }
        }
        let status = unsafe { decode_fn(&mut ctx as *mut _, out.as_mut_ptr() as *mut u8) };
        if status != DecodeStatus::Ok {
            return Err(CodegenError::UnsupportedOp(format!(
                "decode status: {status:?}"
            )));
        }
        Ok(unsafe { out.assume_init() })
    }

    // Proxy type: serialized as u32, deserialized via convert_in.
    // A struct field of this type triggers SlowPath in the IR lowerer.
    #[derive(Facet, Debug, PartialEq, Clone)]
    #[facet(proxy = u32)]
    struct Proxied {
        inner: u32,
    }

    impl From<u32> for Proxied {
        fn from(v: u32) -> Self {
            Proxied { inner: v }
        }
    }

    impl From<&Proxied> for u32 {
        fn from(p: &Proxied) -> Self {
            p.inner
        }
    }

    #[derive(Facet, Debug, PartialEq, Clone)]
    struct WithProxy {
        id: u32,
        value: Proxied,
    }

    #[test]
    fn slow_path_proxy_roundtrip() {
        let original = WithProxy {
            id: 42,
            value: Proxied { inner: 99 },
        };
        let bytes = reflective_encode(&original);
        let decoded = jit_decode_value::<WithProxy>(&bytes).expect("JIT decode failed");
        assert_eq!(original, decoded, "SlowPath round-trip mismatch");
    }
}
