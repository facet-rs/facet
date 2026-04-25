//! Cranelift JIT backend for vox.
//!
//! Replaces the reflective interpreter on the hot decode path with
//! Cranelift-generated encoders/decoders. Falls back to the interpreter for
//! unsupported shapes, unsupported opaque types, or when calibration is
//! unavailable.
//!
//! # Architecture
//!
//! - `vox_postcard::ir` — canonical IR types + pure interpreter (ir-architect)
//! - `codegen` — Cranelift backend that lowers `DecodeProgram` to machine code
//! - `cache` — in-memory cache of compiled encoders/decoders (keyed by shape
//!   value + calibration generation)
//!
//! Entry point for callers: `JitRuntime`.

#![allow(unsafe_code)]

pub mod cache;
pub mod codegen;
pub mod helpers;
pub(crate) mod jitdump;

pub use cache::CompiledCache;
pub use codegen::{ChildEncoderMap, CodegenError, CraneliftBackend, host_isa_name};
pub use vox_jit_abi as abi;
pub use vox_jit_cal as cal;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::mem::MaybeUninit;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, Once, OnceLock};

use arc_swap::ArcSwap;

use facet_core::{Def, Facet, Shape, Type, UserType};
use vox_jit_abi::{DecodeCtx, DecodeStatus};
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
/// Holds the calibration registry, the Cranelift backend, and the cache of
/// compiled encoders/decoders. Thread-safe via internal mutexes.
#[allow(dead_code)]
pub struct JitRuntime {
    cal: Mutex<CalibrationRegistry>,
    backend: Mutex<CraneliftBackend>,
    cache: CompiledCache,
}

impl JitRuntime {
    /// Create a new runtime. Fails if the host ISA is not supported by Cranelift.
    pub fn new() -> Result<Self, CodegenError> {
        Ok(Self {
            cal: Mutex::new(CalibrationRegistry::new()),
            backend: Mutex::new(CraneliftBackend::new()?),
            cache: CompiledCache::new(),
        })
    }

    /// Get the in-memory cache of compiled encoders/decoders.
    pub fn cache(&self) -> &CompiledCache {
        &self.cache
    }

    /// Returns `true` when the current [`CodecMode`] is not `Jit`. Callers use
    /// this to short-circuit JIT compilation and fall through to the reflective
    /// or IR-interpreter path.
    pub fn force_fallback() -> bool {
        CodecMode::from_env() != CodecMode::Jit
    }

    /// Compile (or look up) the decoder for `local_shape` translating from
    /// the given remote schema, returning a `&'static` reference owned by
    /// this runtime's process-wide cache. Conduits call this once at
    /// construction to skip the global cache lookup on every hot-path
    /// decode.
    pub fn prepare_decoder(
        &self,
        remote_schema_id: u64,
        local_shape: &'static Shape,
        plan: &TranslationPlan,
        registry: &SchemaRegistry,
        borrow_mode: BorrowMode,
    ) -> Option<&'static cache::CompiledDecoder> {
        if let Some(decoder) = self
            .cache
            .get_decode(local_shape, borrow_mode, remote_schema_id)
        {
            return Some(decoder);
        }

        if Self::force_fallback() {
            return None;
        }

        let mut cal = self.cal.lock().unwrap();
        register_shape_tree(local_shape, &mut cal);

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
        let decoder = if borrow_mode == BorrowMode::Borrowed {
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
            cache::CompiledDecoder {
                owned_fn: None,
                borrowed_fn: Some(borrowed_fn),
                local_shape,
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
            cache::CompiledDecoder {
                owned_fn: Some(owned_fn),
                borrowed_fn: None,
                local_shape,
            }
        };
        Some(self.cache.insert_decode(local_shape, borrow_mode, remote_schema_id, decoder))
    }

    /// Compile (or look up) the encoder for `shape`, returning a `&'static`
    /// reference owned by this runtime's process-wide cache. Conduits call
    /// this once at construction to skip the global cache lookup on every
    /// hot-path encode.
    pub fn prepare_encoder(&self, shape: &'static Shape) -> Option<&'static cache::CompiledEncoder> {
        if let Some(encoder) = self.cache.get_encode(shape) {
            return Some(encoder);
        }

        if Self::force_fallback() {
            return None;
        }

        // Cycle guard: if we're already compiling this shape higher up the
        // stack (recursive type), bail so the caller falls back to the
        // runtime helper for that one nested reference. The outer frame
        // will complete and cache the encoder; subsequent encodes hit the
        // fast path directly.
        if !encode_in_progress_insert(shape) {
            return None;
        }
        let _guard = InProgressGuard(shape);

        // Phase 1: lower the IR and collect nested shapes under cal lock.
        let (program, children) = {
            let mut cal = self.cal.lock().unwrap();
            register_shape_tree(shape, &mut cal);

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
            (Arc::new(program), children)
        };

        // Phase 2: recurse for each nested WriteShape child with cal+backend
        // unlocked. Direct self-references and cycles are filtered by the
        // in-progress guard above; children that fail to compile fall back
        // to the runtime helper path at the WriteShape site.
        let mut child_encoders = ChildEncoderMap::new();
        for child in children {
            if child == shape {
                continue;
            }
            if let Some(child_encoder) = self.prepare_encoder(child) {
                child_encoders.insert(child, child_encoder);
            }
        }
        let child_encoders = Arc::new(child_encoders);

        // Phase 3: reacquire locks and emit machine code for this shape,
        // embedding child fn pointers directly into the generated call
        // sites.
        let cal = self.cal.lock().unwrap();
        let mut backend = self.backend.lock().unwrap();
        let encode_fn = match backend.compile_encode(shape, &program, &cal, child_encoders.clone())
        {
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
        Some(self.cache.insert_encode(
            shape,
            cache::CompiledEncoder {
                local_shape: shape,
                encode_fn,
                size_hint: std::sync::atomic::AtomicUsize::new(0),
                program: program.clone(),
                child_encoders,
            },
        ))
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

        // Try the JIT path; fall back to the IR interpreter when the lowered
        // program contains an op the JIT can't compile (e.g. `SkipValue` for
        // skipping unknown remote fields). The IR interpreter shares lowering
        // with the JIT, so it sees the same plan.
        if let Some(stub) = self.prepare_decoder(
            remote_schema_id,
            T::SHAPE,
            plan,
            registry,
            BorrowMode::Owned,
        ) && let Some(decode_fn) = stub.owned_fn
        {
            let mut ctx = DecodeCtx::new(input);
            let mut out = MaybeUninit::<T>::uninit();
            let status = unsafe { decode_fn(&mut ctx as *mut _, out.as_mut_ptr() as *mut u8) };
            return if status == DecodeStatus::Ok {
                Some(Ok(unsafe { out.assume_init() }))
            } else {
                Some(Err(decode_status_to_error(status, &ctx, input)))
            };
        }

        let cal = self.cal.lock().unwrap();
        Some(from_slice_ir::<T>(input, plan, registry, Some(&cal)))
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

        if let Some(stub) = self.prepare_decoder(
            remote_schema_id,
            T::SHAPE,
            plan,
            registry,
            BorrowMode::Borrowed,
        ) && let Some(decode_fn) = stub.borrowed_fn
        {
            let mut ctx = DecodeCtx::new(input);
            let mut out = MaybeUninit::<T>::uninit();
            let status = unsafe { decode_fn(&mut ctx as *mut _, out.as_mut_ptr() as *mut u8) };
            return if status == DecodeStatus::Ok {
                Some(Ok(unsafe { out.assume_init() }))
            } else {
                Some(Err(decode_status_to_error(status, &ctx, input)))
            };
        }

        let cal = self.cal.lock().unwrap();
        Some(from_slice_ir_borrowed::<T>(input, plan, registry, Some(&cal)))
    }

    pub fn try_encode_ptr(
        &self,
        ptr: facet::PtrConst,
        shape: &'static Shape,
    ) -> Option<Result<Vec<u8>, SerializeError>> {
        let encoder = self.prepare_encoder(shape)?;
        Some(encode_with(encoder, ptr))
    }
}

// ---------------------------------------------------------------------------
// Direct-call helpers — for callers that hold a pre-resolved `&'static`
// encoder/decoder (e.g. conduits that resolved once at construction).
//
// These bypass `prepare_*` entirely: no cache lookup, no codec-mode check,
// no `Option`. The caller already decided to use the JIT and proved it can
// by holding a reference.
// ---------------------------------------------------------------------------

/// Run a pre-resolved compiled encoder against `ptr`.
pub fn encode_with(
    encoder: &cache::CompiledEncoder,
    ptr: facet::PtrConst,
) -> Result<Vec<u8>, SerializeError> {
    let hint = encoder.size_hint.load(Ordering::Relaxed);
    let mut ctx = vox_jit_abi::EncodeCtx::with_capacity(hint);
    let ok = unsafe { (encoder.encode_fn)(&mut ctx as *mut _, ptr.as_ptr()) };
    if ok {
        let bytes = ctx.into_vec();
        if bytes.len() > hint {
            encoder.size_hint.store(bytes.len(), Ordering::Relaxed);
        }
        Ok(bytes)
    } else {
        Err(SerializeError::ReflectError(
            "JIT encode returned false (OOM)".into(),
        ))
    }
}

/// Run a pre-resolved owned-mode decoder against `input`.
pub fn decode_owned_with<T: Facet<'static>>(
    decoder: &cache::CompiledDecoder,
    input: &[u8],
) -> Result<T, DeserializeError> {
    let decode_fn = decoder
        .owned_fn
        .expect("owned decode_fn missing on owned decoder");
    let mut ctx = DecodeCtx::new(input);
    let mut out = MaybeUninit::<T>::uninit();
    let status = unsafe { decode_fn(&mut ctx as *mut _, out.as_mut_ptr() as *mut u8) };
    if status == DecodeStatus::Ok {
        Ok(unsafe { out.assume_init() })
    } else {
        Err(decode_status_to_error(status, &ctx, input))
    }
}

/// Run a pre-resolved borrowed-mode decoder against `input`.
pub fn decode_borrowed_with<'input, 'facet, T: Facet<'facet>>(
    decoder: &cache::CompiledDecoder,
    input: &'input [u8],
) -> Result<T, DeserializeError>
where
    'input: 'facet,
{
    let decode_fn = decoder
        .borrowed_fn
        .expect("borrowed decode_fn missing on borrowed decoder");
    let mut ctx = DecodeCtx::new(input);
    let mut out = MaybeUninit::<T>::uninit();
    let status = unsafe { decode_fn(&mut ctx as *mut _, out.as_mut_ptr() as *mut u8) };
    if status == DecodeStatus::Ok {
        Ok(unsafe { out.assume_init() })
    } else {
        Err(decode_status_to_error(status, &ctx, input))
    }
}

// ---------------------------------------------------------------------------
// Per-call-site typed entry points (macro form)
// ---------------------------------------------------------------------------
//
// `try_encode_ptr` / `try_decode_*` look the compiled encoder/decoder up in
// the global `CompiledCache` on every call: ArcSwap load + HashMap lookup
// keyed on the shape's ConstTypeId. That's ~10% of bench time across the 4
// codec calls per echo round-trip.
//
// Most call sites know the type statically (generated dispatchers/handlers,
// conduits, schema-deser). For those, the `vox_jit::encode!` /
// `vox_jit::decode_owned!` / `vox_jit::decode_borrowed!` macros expand to a
// fresh `static OnceLock` at the call site, capturing the encoder/decoder
// for that site's type on first call. The hot path is then one acquire load
// + one indirect call — no shape hashing, no global cache traffic.
//
// Why a macro and not a generic function? `static` items inside a generic
// function are *shared across all monomorphizations*, not duplicated per
// instantiation — so a generic helper would silently use the first call
// site's encoder/decoder for every subsequent T. Macro expansion gives a
// literal per-call-site item, which is correct.

#[doc(hidden)]
pub fn __encode_with_slot<'a, T: Facet<'a>>(
    slot: &'static OnceLock<&'static cache::CompiledEncoder>,
    value: &T,
) -> Result<Vec<u8>, SerializeError> {
    let encoder = *slot.get_or_init(|| {
        global_runtime()
            .prepare_encoder(T::SHAPE)
            .expect("JIT encode unavailable for T")
    });
    let ptr = facet::PtrConst::new((value as *const T).cast::<u8>());
    encode_with(encoder, ptr)
}

/// Per-`(call site, BorrowMode)` slot mapping `remote_schema_id` to a
/// compiled decoder. Steady-state for one peer holds a single entry.
#[doc(hidden)]
pub type DecoderSlot = OnceLock<ArcSwap<HashMap<u64, &'static cache::CompiledDecoder>>>;

#[doc(hidden)]
pub fn __decode_owned_with_slot<T: Facet<'static>>(
    slot: &'static DecoderSlot,
    input: &[u8],
    remote_schema_id: u64,
    plan: &TranslationPlan,
    registry: &SchemaRegistry,
) -> Result<T, DeserializeError> {
    let decoder =
        lookup_or_insert_decoder::<T>(slot, remote_schema_id, plan, registry, BorrowMode::Owned);
    decode_owned_with::<T>(decoder, input)
}

#[doc(hidden)]
pub fn __decode_borrowed_with_slot<'input, 'facet, T: Facet<'facet>>(
    slot: &'static DecoderSlot,
    input: &'input [u8],
    remote_schema_id: u64,
    plan: &TranslationPlan,
    registry: &SchemaRegistry,
) -> Result<T, DeserializeError>
where
    'input: 'facet,
{
    let decoder =
        lookup_or_insert_decoder::<T>(slot, remote_schema_id, plan, registry, BorrowMode::Borrowed);
    decode_borrowed_with::<T>(decoder, input)
}

fn lookup_or_insert_decoder<'a, T: Facet<'a>>(
    slot: &'static DecoderSlot,
    remote_schema_id: u64,
    plan: &TranslationPlan,
    registry: &SchemaRegistry,
    borrow_mode: BorrowMode,
) -> &'static cache::CompiledDecoder {
    let cache = slot.get_or_init(|| ArcSwap::from_pointee(HashMap::new()));
    if let Some(&decoder) = cache.load().get(&remote_schema_id) {
        return decoder;
    }
    let decoder = global_runtime()
        .prepare_decoder(remote_schema_id, T::SHAPE, plan, registry, borrow_mode)
        .expect("JIT decode unavailable for T");
    cache.rcu(|cur| {
        let mut next = (**cur).clone();
        next.insert(remote_schema_id, decoder);
        next
    });
    decoder
}

/// Encode a typed value via the JIT, caching the compiled encoder at the
/// call-site `static`. The first invocation compiles the encoder; every
/// subsequent invocation is one atomic load + one indirect call.
#[macro_export]
macro_rules! encode {
    ($value:expr) => {{
        static SLOT: ::std::sync::OnceLock<&'static $crate::cache::CompiledEncoder> =
            ::std::sync::OnceLock::new();
        $crate::__encode_with_slot(&SLOT, $value)
    }};
}

/// Decode an owned typed value via the JIT, caching the decoder at the
/// call-site `static`. The decoder is keyed on `remote_schema_id`; one call
/// site typically sees a single id per peer.
#[macro_export]
macro_rules! decode_owned {
    ($input:expr, $remote_schema_id:expr, $plan:expr, $registry:expr $(,)?) => {{
        static SLOT: $crate::DecoderSlot = ::std::sync::OnceLock::new();
        $crate::__decode_owned_with_slot(&SLOT, $input, $remote_schema_id, $plan, $registry)
    }};
}

/// Decode a borrowed typed value via the JIT, caching the decoder at the
/// call-site `static`.
#[macro_export]
macro_rules! decode_borrowed {
    ($input:expr, $remote_schema_id:expr, $plan:expr, $registry:expr $(,)?) => {{
        static SLOT: $crate::DecoderSlot = ::std::sync::OnceLock::new();
        $crate::__decode_borrowed_with_slot(&SLOT, $input, $remote_schema_id, $plan, $registry)
    }};
}

thread_local! {
    /// Shapes currently being compiled on this thread (DFS stack).
    ///
    /// Used to short-circuit cyclic recursion when `prepare_encoder`
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
    /// The decoder ran and produced `ctx.consumed` bytes consumed.
    Ok { bytes_consumed: usize },
    /// The decoder ran but failed; `ctx` holds error position and init_count.
    Err {
        status: DecodeStatus,
        ctx: DecodeCtx,
    },
    /// No decoder is compiled for this key; caller must use the IR interpreter.
    FallBack,
}
