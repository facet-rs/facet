//! Cranelift JIT backend for vox.
//!
//! Replaces the reflective interpreter on the hot decode path with
//! Cranelift-generated stubs. Falls back to the interpreter for unsupported
//! shapes, unsupported opaque types, or when calibration is unavailable.
//!
//! # Architecture
//!
//! - `vox_postcard::ir` — canonical IR types + pure interpreter (ir-architect)
//! - `codegen` — Cranelift backend that lowers `DecodeProgram` to machine code
//! - `cache` — in-memory stub cache (keyed by shape value + calibration generation)
//!
//! Entry point for callers: `JitRuntime`.

#![allow(unsafe_code)]

pub mod cache;
pub mod codegen;
pub mod helpers;

pub use cache::StubCache;
pub use codegen::{CodegenError, CraneliftBackend, host_isa_name};
pub use vox_jit_abi as abi;
pub use vox_jit_cal as cal;

use std::mem::MaybeUninit;
use std::sync::{Arc, Mutex, Once, OnceLock};

use facet_core::{Def, Facet, Shape, Type, UserType};
use vox_jit_abi::DescriptorHandle;
use vox_jit_abi::{
    DecodeCacheKey, DecodeCtx, DecodeStatus, EncodeCacheKey, EncodeCtx, vox_jit_buf_push_bytes,
    vox_jit_buf_write_varint,
};
use vox_jit_cal::{BorrowMode, CalibrationRegistry};
use vox_postcard::error::DeserializeError;
use vox_postcard::error::SerializeError;
use vox_postcard::ir::{
    DecodeOp, DecodeProgram, EncodeOp, EncodeProgram, lower_encode, lower_with_cal,
};
use vox_postcard::{TranslationPlan, build_identity_plan};
use vox_schema::SchemaRegistry;

// ---------------------------------------------------------------------------
// JIT runtime — top-level handle
// ---------------------------------------------------------------------------

/// Process-local JIT runtime.
///
/// Holds the calibration registry, the Cranelift backend, and the stub cache.
/// Thread-safe via internal mutexes.
#[allow(dead_code)]
pub struct JitRuntime {
    cal: Mutex<CalibrationRegistry>,
    backend: Mutex<CraneliftBackend>,
    cache: StubCache,
}

impl JitRuntime {
    /// Create a new runtime. Fails if the host ISA is not supported by Cranelift.
    pub fn new() -> Result<Self, CodegenError> {
        Ok(Self {
            cal: Mutex::new(CalibrationRegistry::new()),
            backend: Mutex::new(CraneliftBackend::new()?),
            cache: StubCache::new(),
        })
    }

    /// Get the in-memory stub cache.
    pub fn cache(&self) -> &StubCache {
        &self.cache
    }

    /// Returns `true` if the `VOX_JIT_DISABLE` environment variable is set to `1`.
    ///
    /// When this returns `true`, all JIT paths should return `JitDecodeResult::FallBack`
    /// immediately, routing execution through the reflective interpreter. This is
    /// useful for differential testing (run the same input through both paths) and for
    /// production bisection without recompilation.
    pub fn force_fallback() -> bool {
        std::env::var_os("VOX_JIT_DISABLE").map_or(false, |v| v == "1")
    }

    /// Build a `DecodeCacheKey` for the given parameters.
    ///
    /// Per §Caching: if `descriptor_handle` is `None` (required calibration
    /// unavailable), the caller should NOT insert a stub — fall back to the
    /// IR interpreter instead. This method still builds the key for lookup
    /// purposes, but insert is the caller's responsibility to gate.
    ///
    /// The `local_shape` field uses `Shape`'s own `PartialEq`/`Hash` impls —
    /// not the pointer address. Two shapes at different addresses for the same
    /// Rust type produce the same cache key.
    pub fn decode_cache_key(
        &self,
        remote_schema_id: u64,
        local_shape: &'static facet_core::Shape,
        borrow_mode: BorrowMode,
        descriptor_handle: Option<vox_jit_abi::DescriptorHandle>,
    ) -> DecodeCacheKey {
        DecodeCacheKey {
            remote_schema_id,
            local_shape,
            borrow_mode,
            target_isa: host_isa_name(),
            descriptor_handle,
        }
    }

    fn prepare_decode_stub(
        &self,
        remote_schema_id: u64,
        local_shape: &'static Shape,
        plan: &TranslationPlan,
        registry: &SchemaRegistry,
        borrow_mode: BorrowMode,
    ) -> Option<Arc<cache::CompiledDecodeStub>> {
        if Self::force_fallback() {
            return None;
        }

        let mut cal = self.cal.lock().unwrap();
        register_shape_tree(local_shape, &mut cal);
        let descriptor_handle = calibration_token(&cal);
        let key = self.decode_cache_key(
            remote_schema_id,
            local_shape,
            borrow_mode,
            descriptor_handle,
        );

        if let Some(stub) = self.cache.get_decode(&key) {
            return Some(stub);
        }

        let program = match lower_with_cal(plan, local_shape, registry, Some(&cal), borrow_mode) {
            Ok(program) => program,
            Err(err) => {
                if require_pure_jit() {
                    panic!(
                        "VOX_JIT_REQUIRE_PURE=1 and lowered decode program for '{}' failed: {:?}",
                        local_shape, err
                    );
                }
                return None;
            }
        };
        if require_pure_jit() && decode_program_has_slow_path(&program) {
            panic!(
                "VOX_JIT_REQUIRE_PURE=1 and lowered decode program for '{}' contains SlowPath",
                local_shape
            );
        }
        let mut backend = self.backend.lock().unwrap();
        let stub = if borrow_mode == BorrowMode::Borrowed {
            let borrowed_fn = match backend.compile_decode_borrowed(&program, &cal) {
                Ok(f) => f,
                Err(err) => {
                    if require_pure_jit() {
                        panic!(
                            "VOX_JIT_REQUIRE_PURE=1 and decode compile failed for '{}': {}",
                            local_shape, err
                        );
                    }
                    return None;
                }
            };
            cache::CompiledDecodeStub {
                owned_fn: None,
                borrowed_fn: Some(borrowed_fn),
                key: key.clone(),
            }
        } else {
            let owned_fn = match backend.compile_decode_owned(&program, &cal) {
                Ok(f) => f,
                Err(err) => {
                    if require_pure_jit() {
                        panic!(
                            "VOX_JIT_REQUIRE_PURE=1 and decode compile failed for '{}': {}",
                            local_shape, err
                        );
                    }
                    return None;
                }
            };
            cache::CompiledDecodeStub {
                owned_fn: Some(owned_fn),
                borrowed_fn: None,
                key: key.clone(),
            }
        };
        self.cache.insert_decode(key.clone(), stub);
        self.cache.get_decode(&key)
    }

    fn prepare_encode_stub(&self, shape: &'static Shape) -> Option<Arc<cache::CompiledEncodeStub>> {
        if Self::force_fallback() {
            return None;
        }

        let mut cal = self.cal.lock().unwrap();
        register_shape_tree(shape, &mut cal);
        let descriptor_handle = calibration_token(&cal);
        let key = EncodeCacheKey {
            local_shape: shape,
            borrow_mode: BorrowMode::Owned,
            target_isa: host_isa_name(),
            descriptor_handle,
        };

        if let Some(stub) = self.cache.get_encode(&key) {
            return Some(stub);
        }

        let program = match lower_encode(shape, Some(&cal)) {
            Ok(program) => program,
            Err(err) => {
                if require_pure_jit() {
                    panic!(
                        "VOX_JIT_REQUIRE_PURE=1 and lowered encode program for '{}' failed: {}",
                        shape, err
                    );
                }
                return None;
            }
        };
        if require_pure_jit() && encode_program_has_slow_path(&program) {
            panic!(
                "VOX_JIT_REQUIRE_PURE=1 and lowered encode program for '{}' contains SlowPath",
                shape
            );
        }
        let mut backend = self.backend.lock().unwrap();
        let encode_fn = match backend.compile_encode(&program, &cal) {
            Ok(f) => f,
            Err(err) => {
                if require_pure_jit() {
                    panic!(
                        "VOX_JIT_REQUIRE_PURE=1 and encode compile failed for '{}': {}",
                        shape, err
                    );
                }
                return None;
            }
        };
        let stub = cache::CompiledEncodeStub {
            key: key.clone(),
            encode_fn,
        };
        self.cache.insert_encode(key.clone(), stub);
        self.cache.get_encode(&key)
    }

    pub fn try_decode_owned<T: Facet<'static>>(
        &self,
        input: &[u8],
        remote_schema_id: u64,
        plan: &TranslationPlan,
        registry: &SchemaRegistry,
    ) -> Option<Result<T, DeserializeError>> {
        if let Def::Result(result_def) = T::SHAPE.def {
            return self.try_decode_result::<T>(
                input,
                remote_schema_id,
                plan,
                registry,
                BorrowMode::Owned,
                result_def,
            );
        }
        let stub = self.prepare_decode_stub(
            remote_schema_id,
            T::SHAPE,
            plan,
            registry,
            BorrowMode::Owned,
        )?;
        let decode_fn = stub.owned_fn?;
        let mut ctx = DecodeCtx::new(input);
        let mut out = MaybeUninit::<T>::uninit();
        let layout = T::SHAPE.layout.sized_layout().ok()?;
        if layout.size() != 0 {
            unsafe {
                std::ptr::write_bytes(out.as_mut_ptr() as *mut u8, 0, layout.size());
            }
        }
        let status = unsafe { decode_fn(&mut ctx as *mut _, out.as_mut_ptr() as *mut u8) };
        if status == DecodeStatus::Ok {
            Some(Ok(unsafe { out.assume_init() }))
        } else {
            Some(Err(decode_status_to_error(status, &ctx, input)))
        }
    }

    pub fn try_decode_borrowed<'input, 'facet, T: Facet<'facet>>(
        &self,
        input: &'input [u8],
        remote_schema_id: u64,
        plan: &TranslationPlan,
        registry: &SchemaRegistry,
    ) -> Option<Result<T, DeserializeError>>
    where
        'input: 'facet,
    {
        let stub = self.prepare_decode_stub(
            remote_schema_id,
            T::SHAPE,
            plan,
            registry,
            BorrowMode::Borrowed,
        )?;
        let decode_fn = stub.borrowed_fn?;
        let mut ctx = DecodeCtx::new(input);
        let mut out = MaybeUninit::<T>::uninit();
        let layout = T::SHAPE.layout.sized_layout().ok()?;
        if layout.size() != 0 {
            unsafe {
                std::ptr::write_bytes(out.as_mut_ptr() as *mut u8, 0, layout.size());
            }
        }
        let status = unsafe { decode_fn(&mut ctx as *mut _, out.as_mut_ptr() as *mut u8) };
        if status == DecodeStatus::Ok {
            Some(Ok(unsafe { out.assume_init() }))
        } else {
            Some(Err(decode_status_to_error(status, &ctx, input)))
        }
    }

    pub fn try_encode_ptr(
        &self,
        ptr: facet::PtrConst,
        shape: &'static Shape,
    ) -> Option<Result<Vec<u8>, SerializeError>> {
        if let Def::Result(result_def) = shape.def {
            let mut ctx = EncodeCtx::with_capacity(64);
            let ok = unsafe { (result_def.vtable.is_ok)(ptr) };
            if ok {
                if !unsafe { vox_jit_buf_write_varint(&mut ctx as *mut _, 0) } {
                    return Some(Err(SerializeError::ReflectError(
                        "JIT encode returned false (OOM)".into(),
                    )));
                }
                let ok_ptr = unsafe { (result_def.vtable.get_ok)(ptr) };
                if ok_ptr.is_null() {
                    return Some(Err(SerializeError::ReflectError(
                        "Result::Ok arm had null payload pointer".into(),
                    )));
                }
                let inner = match self.try_encode_ptr(facet::PtrConst::new(ok_ptr), result_def.t) {
                    Some(Ok(bytes)) => bytes,
                    Some(Err(err)) => return Some(Err(err)),
                    None => {
                        if require_pure_jit() {
                            panic!(
                                "VOX_JIT_REQUIRE_PURE=1 and result Ok encode for '{}' could not stay on JIT",
                                result_def.t
                            );
                        }
                        return None;
                    }
                };
                if !unsafe {
                    vox_jit_buf_push_bytes(&mut ctx as *mut _, inner.as_ptr(), inner.len())
                } {
                    return Some(Err(SerializeError::ReflectError(
                        "JIT encode returned false (OOM)".into(),
                    )));
                }
                return Some(Ok(ctx.into_vec()));
            }

            if !unsafe { vox_jit_buf_write_varint(&mut ctx as *mut _, 1) } {
                return Some(Err(SerializeError::ReflectError(
                    "JIT encode returned false (OOM)".into(),
                )));
            }
            let err_ptr = unsafe { (result_def.vtable.get_err)(ptr) };
            if err_ptr.is_null() {
                return Some(Err(SerializeError::ReflectError(
                    "Result::Err arm had null payload pointer".into(),
                )));
            }
            let inner = match self.try_encode_ptr(facet::PtrConst::new(err_ptr), result_def.e) {
                Some(Ok(bytes)) => bytes,
                Some(Err(err)) => return Some(Err(err)),
                None => {
                    if require_pure_jit() {
                        panic!(
                            "VOX_JIT_REQUIRE_PURE=1 and result Err encode for '{}' could not stay on JIT",
                            result_def.e
                        );
                    }
                    return None;
                }
            };
            if !unsafe { vox_jit_buf_push_bytes(&mut ctx as *mut _, inner.as_ptr(), inner.len()) } {
                return Some(Err(SerializeError::ReflectError(
                    "JIT encode returned false (OOM)".into(),
                )));
            }
            return Some(Ok(ctx.into_vec()));
        }

        let stub = self.prepare_encode_stub(shape)?;
        let mut ctx = vox_jit_abi::EncodeCtx::with_capacity(64);
        let ok = unsafe { (stub.encode_fn)(&mut ctx as *mut _, ptr.as_ptr()) };
        if ok {
            Some(Ok(ctx.into_vec()))
        } else {
            Some(Err(SerializeError::ReflectError(
                "JIT encode returned false (OOM)".into(),
            )))
        }
    }

    fn try_decode_into_ptr(
        &self,
        input: &[u8],
        remote_schema_id: u64,
        shape: &'static Shape,
        plan: &TranslationPlan,
        registry: &SchemaRegistry,
        borrow_mode: BorrowMode,
        dst: *mut u8,
    ) -> Option<Result<(), DeserializeError>> {
        let stub =
            self.prepare_decode_stub(remote_schema_id, shape, plan, registry, borrow_mode)?;
        let decode_fn = if borrow_mode == BorrowMode::Borrowed {
            stub.borrowed_fn?
        } else {
            stub.owned_fn?
        };
        let mut ctx = DecodeCtx::new(input);
        let status = unsafe { decode_fn(&mut ctx as *mut _, dst) };
        if status == DecodeStatus::Ok {
            Some(Ok(()))
        } else {
            Some(Err(decode_status_to_error(status, &ctx, input)))
        }
    }

    fn try_decode_result<T: Facet<'static>>(
        &self,
        input: &[u8],
        remote_schema_id: u64,
        plan: &TranslationPlan,
        registry: &SchemaRegistry,
        borrow_mode: BorrowMode,
        result_def: facet_core::ResultDef,
    ) -> Option<Result<T, DeserializeError>> {
        let ok_identity = build_identity_plan(result_def.t);
        let err_identity = build_identity_plan(result_def.e);
        let (variant_index, prefix_len) = match read_varint_prefix(input) {
            Some(pair) => pair,
            None => {
                return Some(Err(DeserializeError::UnexpectedEof { pos: input.len() }));
            }
        };

        let (inner_shape, inner_plan, init_fn, layout) = match variant_index {
            0 => {
                let inner_plan = match plan {
                    TranslationPlan::Enum { nested, .. } => nested.get(&0).unwrap_or(&ok_identity),
                    TranslationPlan::Identity => &ok_identity,
                    _ => {
                        if require_pure_jit() {
                            panic!(
                                "VOX_JIT_REQUIRE_PURE=1 and result decode for '{}' had unsupported translation plan {:?}",
                                T::SHAPE,
                                plan
                            );
                        }
                        return None;
                    }
                };
                let layout = match result_def.t.layout.sized_layout() {
                    Ok(layout) => layout,
                    Err(_) => return None,
                };
                (result_def.t, inner_plan, result_def.vtable.init_ok, layout)
            }
            1 => {
                let inner_plan = match plan {
                    TranslationPlan::Enum { nested, .. } => nested.get(&1).unwrap_or(&err_identity),
                    TranslationPlan::Identity => &err_identity,
                    _ => {
                        if require_pure_jit() {
                            panic!(
                                "VOX_JIT_REQUIRE_PURE=1 and result decode for '{}' had unsupported translation plan {:?}",
                                T::SHAPE,
                                plan
                            );
                        }
                        return None;
                    }
                };
                let layout = match result_def.e.layout.sized_layout() {
                    Ok(layout) => layout,
                    Err(_) => return None,
                };
                (result_def.e, inner_plan, result_def.vtable.init_err, layout)
            }
            other => {
                return Some(Err(DeserializeError::UnknownVariant {
                    remote_index: other as usize,
                }));
            }
        };

        let tmp = facet_core::alloc_for_layout(layout);
        let tmp_ptr = unsafe { tmp.assume_init() };
        if layout.size() != 0 {
            unsafe {
                std::ptr::write_bytes(tmp_ptr.as_mut_byte_ptr(), 0, layout.size());
            }
        }
        let inner_input = &input[prefix_len..];
        let decode_result = self.try_decode_into_ptr(
            inner_input,
            remote_schema_id,
            inner_shape,
            inner_plan,
            registry,
            borrow_mode,
            tmp_ptr.as_mut_byte_ptr(),
        );
        let decode_result = match decode_result {
            Some(result) => result,
            None => {
                unsafe { facet_core::dealloc_for_layout(tmp_ptr, layout) };
                if require_pure_jit() {
                    panic!(
                        "VOX_JIT_REQUIRE_PURE=1 and result decode for '{}' could not stay on JIT",
                        inner_shape
                    );
                }
                return None;
            }
        };
        if let Err(err) = decode_result {
            unsafe { facet_core::dealloc_for_layout(tmp_ptr, layout) };
            return Some(Err(err));
        }

        let mut out = MaybeUninit::<T>::uninit();
        unsafe {
            init_fn(
                facet_core::PtrUninit::new(out.as_mut_ptr() as *mut u8),
                tmp_ptr,
            );
            facet_core::dealloc_for_layout(tmp_ptr, layout);
        }
        Some(Ok(unsafe { out.assume_init() }))
    }
}

pub(crate) fn require_pure_jit() -> bool {
    std::env::var_os("VOX_JIT_REQUIRE_PURE").is_some_and(|v| v == "1")
}

pub(crate) fn abort_on_slow_path() -> bool {
    std::env::var_os("VOX_JIT_ABORT_ON_SLOW_PATH").is_some_and(|v| v == "1")
}

fn decode_program_has_slow_path(program: &DecodeProgram) -> bool {
    program
        .blocks
        .iter()
        .flat_map(|block| block.ops.iter())
        .any(|op| matches!(op, DecodeOp::SlowPath { .. }))
}

fn encode_program_has_slow_path(program: &EncodeProgram) -> bool {
    program
        .blocks
        .iter()
        .flat_map(|block| block.ops.iter())
        .any(|op| matches!(op, EncodeOp::SlowPath { .. }))
}

pub fn global_runtime() -> &'static JitRuntime {
    static GLOBAL_RUNTIME: OnceLock<JitRuntime> = OnceLock::new();
    GLOBAL_RUNTIME.get_or_init(|| JitRuntime::new().expect("create JIT runtime"))
}

pub fn install_postcard_encode_hook() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        vox_postcard::serialize::set_runtime_encode_hook(runtime_encode_hook);
    });
}

fn runtime_encode_hook(
    ptr: facet::PtrConst,
    shape: &'static Shape,
) -> Result<Option<Vec<u8>>, SerializeError> {
    Ok(global_runtime().try_encode_ptr(ptr, shape).transpose()?)
}

fn calibration_token(cal: &CalibrationRegistry) -> Option<DescriptorHandle> {
    cal.iter()
        .last()
        .map(|(handle, _)| handle)
        .or_else(|| cal.string_descriptor_handle())
}

fn register_shape_tree(shape: &'static Shape, cal: &mut CalibrationRegistry) {
    if shape == <String as Facet<'static>>::SHAPE
        && cal
            .lookup_by_shape(<String as Facet<'static>>::SHAPE)
            .is_none()
    {
        cal.calibrate_string_for_type();
    }

    match shape.def {
        Def::List(_) => {
            cal.get_or_calibrate_by_shape(shape);
        }
        Def::Pointer(ptr) if ptr.known == Some(facet_core::KnownPointer::Box) => {
            cal.get_or_calibrate_by_shape(shape);
        }
        _ => {}
    }

    match shape.ty {
        Type::User(UserType::Struct(st)) => {
            for field in st.fields {
                register_shape_tree(field.shape(), cal);
            }
        }
        Type::User(UserType::Enum(et)) => {
            for variant in et.variants {
                for field in variant.data.fields {
                    register_shape_tree(field.shape(), cal);
                }
            }
        }
        _ => {}
    }

    match shape.def {
        Def::Option(opt) => register_shape_tree(opt.t, cal),
        Def::Result(result) => {
            register_shape_tree(result.t, cal);
            register_shape_tree(result.e, cal);
        }
        Def::List(list) => register_shape_tree(list.t, cal),
        Def::Pointer(ptr) => {
            if let Some(inner) = ptr.pointee() {
                register_shape_tree(inner, cal);
            }
        }
        Def::Array(arr) => register_shape_tree(arr.t, cal),
        _ => {}
    }
}

fn decode_status_to_error(status: DecodeStatus, ctx: &DecodeCtx, input: &[u8]) -> DeserializeError {
    match status {
        DecodeStatus::Ok => DeserializeError::Custom("JIT returned Ok in error path".into()),
        DecodeStatus::UnexpectedEof => DeserializeError::UnexpectedEof { pos: ctx.error_pos },
        DecodeStatus::VarintOverflow => DeserializeError::VarintOverflow { pos: ctx.error_pos },
        DecodeStatus::InvalidBool => DeserializeError::InvalidBool {
            pos: ctx.error_pos,
            got: input.get(ctx.error_pos).copied().unwrap_or(0),
        },
        DecodeStatus::InvalidUtf8 => DeserializeError::InvalidUtf8 { pos: ctx.error_pos },
        DecodeStatus::InvalidOptionTag => DeserializeError::InvalidOptionTag {
            pos: ctx.error_pos,
            got: input.get(ctx.error_pos).copied().unwrap_or(0),
        },
        DecodeStatus::InvalidEnumDiscriminant => DeserializeError::Custom(format!(
            "JIT invalid enum discriminant at byte {}",
            ctx.error_pos
        )),
        DecodeStatus::UnknownVariant => {
            let remote_index = read_varint_at(input, ctx.error_pos).unwrap_or(0) as usize;
            DeserializeError::UnknownVariant { remote_index }
        }
        DecodeStatus::AllocFailed => {
            DeserializeError::Custom("JIT allocation failed during decode".into())
        }
    }
}

fn read_varint_at(input: &[u8], pos: usize) -> Option<u64> {
    let mut value = 0u64;
    let mut shift = 0u32;
    let mut i = pos;
    while i < input.len() && shift < 64 {
        let byte = input[i];
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
        shift += 7;
        i += 1;
    }
    None
}

fn read_varint_prefix(input: &[u8]) -> Option<(u64, usize)> {
    let mut value = 0u64;
    let mut shift = 0u32;
    let mut i = 0usize;
    while i < input.len() && shift < 64 {
        let byte = input[i];
        value |= u64::from(byte & 0x7f) << shift;
        i += 1;
        if byte & 0x80 == 0 {
            return Some((value, i));
        }
        shift += 7;
    }
    None
}

// ---------------------------------------------------------------------------
// Fallback interface
// ---------------------------------------------------------------------------

/// Result of a JIT decode attempt.
pub enum JitDecodeResult {
    /// The stub ran and produced `ctx.consumed` bytes consumed.
    Ok { bytes_consumed: usize },
    /// The stub ran but failed; `ctx` holds error position and init_count.
    Err {
        status: DecodeStatus,
        ctx: DecodeCtx,
    },
    /// No stub is compiled for this key; caller must use the IR interpreter.
    FallBack,
}
