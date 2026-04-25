//! In-memory stub cache.
//!
//! Compiled stubs are stored in a process-local hash map keyed by
//! `DecodeCacheKey` / `EncodeCacheKey`. No persistence across restarts.
//!
//! The cache owns the Cranelift `JITModule` which holds the memory maps for
//! all compiled stubs. Stubs are identified by stable function IDs obtained
//! from the module.

use std::collections::HashMap;
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex};

use arc_swap::ArcSwap;
use facet_core::Shape;
use vox_jit_abi::{BorrowedDecodeFn, DecodeCacheKey, EncodeCacheKey, EncodeFn, OwnedDecodeFn};
use vox_jit_cal::BorrowMode;
use vox_postcard::ir::EncodeProgram;

/// Cheap fast-cache key for a `&'static Shape`.
///
/// The structural cache keys (`EncodeCacheKey` / `DecodeCacheKey`) hash the
/// shape's *content* — that walks every field of every nested type and is the
/// same machinery postcard's reflective path uses. The fast cache instead
/// keys on the shape itself: `Shape: Hash + Eq` via its compiler-issued
/// `ConstTypeId`, which is cheap and *correct* across translation units
/// (unlike pointer identity, which is not guaranteed to be stable).
#[derive(Clone, Copy, Hash, PartialEq, Eq)]
struct FastDecodeKey {
    shape: &'static Shape,
    borrow_mode: BorrowMode,
    remote_schema_id: u64,
}

// ---------------------------------------------------------------------------
// Compiled stub entries
// ---------------------------------------------------------------------------

/// A compiled decode stub that can be called via the JIT ABI.
#[derive(Clone)]
pub struct CompiledDecodeStub {
    /// The owned-mode function pointer (or None if only borrowed mode was compiled).
    pub owned_fn: Option<OwnedDecodeFn>,
    /// The borrowed-mode function pointer (or None if only owned mode was compiled).
    pub borrowed_fn: Option<BorrowedDecodeFn>,
    /// The key that produced this stub (for debugging / cache validation).
    pub key: DecodeCacheKey,
}

/// A compiled encode stub — reserved for task #17 (deferred).
pub struct CompiledEncodeStub {
    pub key: EncodeCacheKey,
    pub encode_fn: EncodeFn,
    /// Largest encoded output observed for this shape. Used to seed the
    /// initial `EncodeCtx` capacity so the hot path avoids `realloc`
    /// churn after the first encode of a given shape.
    pub size_hint: AtomicUsize,
    /// Lowered IR kept alive so parent compiles can inline this encoder's
    /// body instead of emitting a `call_indirect` to `encode_fn`.
    pub program: Arc<EncodeProgram>,
    /// Stubs for this shape's direct child shapes (the `WriteShape` ops in
    /// `program`). Swapped into `EncodeCtx_.child_encoders` when this
    /// shape is inlined into a parent, so its grandchildren resolve too.
    pub child_encoders: Arc<crate::codegen::ChildEncoderMap>,
}

// ---------------------------------------------------------------------------
// Stub cache
// ---------------------------------------------------------------------------

/// Process-local, in-memory stub cache.
///
/// The structural (slow-path) maps live behind a `Mutex`; the pointer-identity
/// fast paths live behind `ArcSwap` so the steady-state lookup is one atomic
/// load + one Arc clone, no locking. Stubs are never evicted — the cache lives
/// for the process lifetime.
pub struct StubCache {
    slow: Mutex<StubCacheSlow>,
    encode_fast: ArcSwap<HashMap<&'static Shape, Arc<CompiledEncodeStub>>>,
    decode_fast: ArcSwap<HashMap<FastDecodeKey, Arc<CompiledDecodeStub>>>,
}

impl Default for StubCache {
    fn default() -> Self {
        Self {
            slow: Mutex::new(StubCacheSlow::default()),
            encode_fast: ArcSwap::from_pointee(HashMap::new()),
            decode_fast: ArcSwap::from_pointee(HashMap::new()),
        }
    }
}

#[derive(Default)]
struct StubCacheSlow {
    decode_stubs: HashMap<DecodeCacheKey, Arc<CompiledDecodeStub>>,
    encode_stubs: HashMap<EncodeCacheKey, Arc<CompiledEncodeStub>>,
}

impl StubCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Look up a decode stub by key.
    ///
    /// Returns a cloned Arc if found. Returns None if the stub is not yet
    /// compiled or if the cache key's calibration generation is stale.
    pub fn get_decode(&self, key: &DecodeCacheKey) -> Option<Arc<CompiledDecodeStub>> {
        let guard = self.slow.lock().unwrap();
        guard.decode_stubs.get(key).cloned()
    }

    /// Insert a compiled decode stub.
    ///
    /// If a stub already exists for this key, it is replaced (should not
    /// happen in normal operation — callers check first).
    pub fn insert_decode(
        &self,
        key: DecodeCacheKey,
        stub: CompiledDecodeStub,
    ) -> Arc<CompiledDecodeStub> {
        let arc = Arc::new(stub);
        let mut guard = self.slow.lock().unwrap();
        guard.decode_stubs.insert(key, arc.clone());
        arc
    }

    /// Look up an encode stub by key.
    pub fn get_encode(&self, key: &EncodeCacheKey) -> Option<Arc<CompiledEncodeStub>> {
        let guard = self.slow.lock().unwrap();
        guard.encode_stubs.get(key).cloned()
    }

    /// Insert a compiled encode stub.
    pub fn insert_encode(
        &self,
        key: EncodeCacheKey,
        stub: CompiledEncodeStub,
    ) -> Arc<CompiledEncodeStub> {
        let arc = Arc::new(stub);
        let mut guard = self.slow.lock().unwrap();
        guard.encode_stubs.insert(key, arc.clone());
        arc
    }

    /// Fast-path encode lookup by shape identity (ConstTypeId). One atomic
    /// load and (on hit) one Arc clone — no locking.
    pub fn get_encode_fast(&self, shape: &'static Shape) -> Option<Arc<CompiledEncodeStub>> {
        self.encode_fast.load().get(shape).cloned()
    }

    /// Record a compiled encode stub in the fast path.
    pub fn insert_encode_fast(&self, shape: &'static Shape, stub: Arc<CompiledEncodeStub>) {
        self.encode_fast.rcu(|cur| {
            let mut next = (**cur).clone();
            next.insert(shape, stub.clone());
            next
        });
    }

    /// Fast-path decode lookup keyed on shape identity + borrow mode +
    /// remote schema id. One atomic load and (on hit) one Arc clone.
    pub fn get_decode_fast(
        &self,
        shape: &'static Shape,
        borrow_mode: BorrowMode,
        remote_schema_id: u64,
    ) -> Option<Arc<CompiledDecodeStub>> {
        self.decode_fast
            .load()
            .get(&FastDecodeKey {
                shape,
                borrow_mode,
                remote_schema_id,
            })
            .cloned()
    }

    /// Record a compiled decode stub in the fast path.
    pub fn insert_decode_fast(
        &self,
        shape: &'static Shape,
        borrow_mode: BorrowMode,
        remote_schema_id: u64,
        stub: Arc<CompiledDecodeStub>,
    ) {
        let key = FastDecodeKey {
            shape,
            borrow_mode,
            remote_schema_id,
        };
        self.decode_fast.rcu(|cur| {
            let mut next = (**cur).clone();
            next.insert(key, stub.clone());
            next
        });
    }

    /// Number of decode stubs currently cached.
    pub fn decode_stub_count(&self) -> usize {
        self.slow.lock().unwrap().decode_stubs.len()
    }
}
