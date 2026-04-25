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
pub(crate) mod jitdump;

pub use cache::StubCache;
pub use codegen::{ChildEncoderMap, CodegenError, CraneliftBackend, host_isa_name};
pub use vox_jit_abi as abi;
pub use vox_jit_cal as cal;

use std::cell::RefCell;
use std::collections::HashSet;
use std::mem::MaybeUninit;
use std::sync::{Arc, Mutex, Once, OnceLock};

use facet_core::{Def, Facet, Shape, Type, UserType};
use vox_jit_abi::DescriptorHandle;
use vox_jit_abi::{DecodeCacheKey, DecodeCtx, DecodeStatus, EncodeCacheKey};
use vox_jit_cal::{BorrowMode, CalibrationRegistry};
use vox_postcard::TranslationPlan;
use vox_postcard::error::DeserializeError;
use vox_postcard::error::SerializeError;
use vox_postcard::ir::{
    DecodeOp, DecodeProgram, EncodeOp, EncodeProgram, from_slice_ir, from_slice_ir_borrowed,
    lower_encode, lower_with_cal,
};
use vox_schema::SchemaRegistry;

// ---------------------------------------------------------------------------
// Codec mode — `VOX_CODEC` selects between reflect / interp / jit
// ---------------------------------------------------------------------------

/// Which decoder/encoder the RPC layer should use for this process.
///
/// Selected via the `VOX_CODEC` environment variable:
/// - `reflect` — facet-reflect oracle (slow, correctness baseline, Miri-safe).
/// - `interp`  — IR interpreter (shares lowering with JIT, Miri-safe).
/// - `jit`     — Cranelift JIT, falling back to `reflect` for shapes the JIT
///               cannot compile. This is the default.
///
/// Reading the env var is one-shot per process (OnceLock-cached).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecMode {
    Reflect,
    Interp,
    Jit,
}

impl CodecMode {
    pub fn from_env() -> Self {
        static CACHED: OnceLock<CodecMode> = OnceLock::new();
        *CACHED.get_or_init(|| match std::env::var("VOX_CODEC").ok().as_deref() {
            Some("reflect") => CodecMode::Reflect,
            Some("interp") => CodecMode::Interp,
            Some("jit") | None => CodecMode::Jit,
            Some(other) => {
                panic!("VOX_CODEC must be one of 'reflect', 'interp', 'jit' (got {other:?})")
            }
        })
    }
}

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

    /// Returns `true` when the current [`CodecMode`] is not `Jit`. Callers use
    /// this to short-circuit JIT compilation and fall through to the reflective
    /// or IR-interpreter path.
    pub fn force_fallback() -> bool {
        CodecMode::from_env() != CodecMode::Jit
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
        if let Some(stub) = self
            .cache
            .get_decode_fast(local_shape, borrow_mode, remote_schema_id)
        {
            return Some(stub);
        }

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
            self.cache
                .insert_decode_fast(local_shape, borrow_mode, remote_schema_id, stub.clone());
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
            let borrowed_fn = match backend.compile_decode_borrowed(local_shape, &program, &cal) {
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
            let owned_fn = match backend.compile_decode_owned(local_shape, &program, &cal) {
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
        let stub = self.cache.insert_decode(key.clone(), stub);
        self.cache
            .insert_decode_fast(local_shape, borrow_mode, remote_schema_id, stub.clone());
        Some(stub)
    }

    fn prepare_encode_stub(&self, shape: &'static Shape) -> Option<Arc<cache::CompiledEncodeStub>> {
        // Fast path: pointer-identity lookup. No shape-tree walk, no cal
        // mutex, no structural hashing. This is the hot path for repeated
        // encode calls of the same type (i.e., every benchmark iteration).
        if let Some(stub) = self.cache.get_encode_fast(shape) {
            return Some(stub);
        }

        if Self::force_fallback() {
            return None;
        }

        // Cycle guard: if we're already compiling this shape higher up the
        // stack (recursive type), bail so the caller falls back to the
        // runtime helper for that one nested reference. The outer frame
        // will complete and cache the stub; subsequent encodes hit the
        // fast path directly.
        if !encode_in_progress_insert(shape) {
            return None;
        }
        let _guard = InProgressGuard(shape);

        // Phase 1: lower the IR and collect nested shapes under cal lock.
        let (program, children, key) = {
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
                self.cache.insert_encode_fast(shape, stub.clone());
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
            let children = codegen::collect_write_shape_children(&program);
            (Arc::new(program), children, key)
        };

        // Phase 2: recurse for each nested WriteShape child with cal+backend
        // unlocked. Direct self-references and cycles are filtered by the
        // in-progress guard above; children that fail to compile fall back
        // to the runtime helper path at the WriteShape site.
        let mut child_encoders = ChildEncoderMap::new();
        for child in children {
            if std::ptr::eq(child, shape) {
                continue;
            }
            if let Some(child_stub) = self.prepare_encode_stub(child) {
                child_encoders.insert(cache::ShapePtr(child), child_stub);
            }
        }
        let child_encoders = Arc::new(child_encoders);

        // Phase 3: reacquire locks and emit machine code for this shape,
        // embedding child fn pointers directly into the generated call
        // sites.
        let cal = self.cal.lock().unwrap();
        let mut backend = self.backend.lock().unwrap();
        let encode_fn = match backend.compile_encode(shape, &program, &cal, child_encoders.clone()) {
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
        let stub = self.cache.insert_encode(
            key.clone(),
            cache::CompiledEncodeStub {
                key: key.clone(),
                encode_fn,
                size_hint: std::sync::atomic::AtomicUsize::new(0),
                program: program.clone(),
                child_encoders,
            },
        );
        self.cache.insert_encode_fast(shape, stub.clone());
        Some(stub)
    }

    pub fn try_decode_owned<T: Facet<'static>>(
        &self,
        input: &[u8],
        remote_schema_id: u64,
        plan: &TranslationPlan,
        registry: &SchemaRegistry,
    ) -> Option<Result<T, DeserializeError>> {
        match CodecMode::from_env() {
            CodecMode::Reflect => return None,
            CodecMode::Interp => {
                let cal = self.cal.lock().unwrap();
                return Some(from_slice_ir::<T>(input, plan, registry, Some(&cal)));
            }
            CodecMode::Jit => {}
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
        match CodecMode::from_env() {
            CodecMode::Reflect => return None,
            CodecMode::Interp => {
                let cal = self.cal.lock().unwrap();
                return Some(from_slice_ir_borrowed::<T>(
                    input,
                    plan,
                    registry,
                    Some(&cal),
                ));
            }
            CodecMode::Jit => {}
        }

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
        use std::sync::atomic::Ordering;

        let stub = self.prepare_encode_stub(shape)?;
        let hint = stub.size_hint.load(Ordering::Relaxed);
        let mut ctx = vox_jit_abi::EncodeCtx::with_capacity(hint);
        let ok = unsafe { (stub.encode_fn)(&mut ctx as *mut _, ptr.as_ptr()) };
        if ok {
            let bytes = ctx.into_vec();
            if bytes.len() > hint {
                stub.size_hint.store(bytes.len(), Ordering::Relaxed);
            }
            Some(Ok(bytes))
        } else {
            Some(Err(SerializeError::ReflectError(
                "JIT encode returned false (OOM)".into(),
            )))
        }
    }
}

thread_local! {
    /// Shapes currently being compiled on this thread (DFS stack).
    ///
    /// Used to short-circuit cyclic recursion when `prepare_encode_stub`
    /// walks a shape's nested `WriteShape` children. A child that's
    /// already on the stack would deadlock / loop if we tried to
    /// pre-compile it; instead we return `None` for that child so the
    /// parent emits a runtime helper call for that one reference.
    static ENCODE_IN_PROGRESS: RefCell<HashSet<*const Shape>> = RefCell::new(HashSet::new());
}

/// Try to mark `shape` as "in progress" for encode compile on this thread.
/// Returns `true` if inserted, `false` if already present (cycle detected).
fn encode_in_progress_insert(shape: &'static Shape) -> bool {
    ENCODE_IN_PROGRESS.with(|set| set.borrow_mut().insert(shape as *const _))
}

fn encode_in_progress_remove(shape: &'static Shape) {
    ENCODE_IN_PROGRESS.with(|set| {
        set.borrow_mut().remove(&(shape as *const _));
    });
}

struct InProgressGuard(&'static Shape);

impl Drop for InProgressGuard {
    fn drop(&mut self) {
        encode_in_progress_remove(self.0);
    }
}

pub(crate) fn require_pure_jit() -> bool {
    static CACHED: OnceLock<bool> = OnceLock::new();
    *CACHED.get_or_init(|| std::env::var_os("VOX_JIT_REQUIRE_PURE").is_some_and(|v| v == "1"))
}

pub(crate) fn abort_on_slow_path() -> bool {
    static CACHED: OnceLock<bool> = OnceLock::new();
    *CACHED.get_or_init(|| std::env::var_os("VOX_JIT_ABORT_ON_SLOW_PATH").is_some_and(|v| v == "1"))
}

/// When set, the Cranelift backend prints each compiled function's CLIF IR
/// and machine-code disassembly to stderr. Useful when reasoning about why
/// the JIT is (or isn't) as fast as expected.
pub fn dump_compiled() -> bool {
    static CACHED: OnceLock<bool> = OnceLock::new();
    *CACHED.get_or_init(|| std::env::var_os("VOX_JIT_DUMP").is_some_and(|v| v == "1"))
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
    global_runtime().try_encode_ptr(ptr, shape).transpose()
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
