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

use facet_core::Shape;
use vox_jit_abi::{BorrowedDecodeFn, DecodeCacheKey, EncodeCacheKey, EncodeFn, OwnedDecodeFn};
use vox_jit_cal::BorrowMode;

/// Pointer-identity key for a `&'static Shape`.
///
/// The structural cache keys (`EncodeCacheKey` / `DecodeCacheKey`) hash the
/// shape's *content* — that walks every field of every nested type and is the
/// same machinery postcard's reflective path uses. Since we only ever see
/// `&'static Shape` references that come from compile-time `Facet` impls, the
/// pointer address uniquely identifies the type within a process. Hashing the
/// pointer is ~one instruction; hashing the content is thousands.
#[derive(Clone, Copy)]
struct ShapePtr(&'static Shape);

impl std::hash::Hash for ShapePtr {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        (self.0 as *const Shape as usize).hash(state);
    }
}

impl PartialEq for ShapePtr {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.0, other.0)
    }
}

impl Eq for ShapePtr {}

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
struct FastEncodeKey(ShapePtr);

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
struct FastDecodeKey {
    shape: ShapePtr,
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
}

// ---------------------------------------------------------------------------
// Stub cache
// ---------------------------------------------------------------------------

/// Process-local, in-memory stub cache.
///
/// Thread-safe via a Mutex. Stubs are never evicted — the cache lives for the
/// process lifetime.
#[derive(Default)]
pub struct StubCache {
    inner: Mutex<StubCacheInner>,
}

#[derive(Default)]
struct StubCacheInner {
    decode_stubs: HashMap<DecodeCacheKey, Arc<CompiledDecodeStub>>,
    encode_stubs: HashMap<EncodeCacheKey, Arc<CompiledEncodeStub>>,
    /// Pointer-identity fast path for encode lookups. Populated alongside
    /// `encode_stubs` so repeat calls skip the expensive shape-tree walk
    /// and structural hash that the full `EncodeCacheKey` requires.
    encode_fast: HashMap<FastEncodeKey, Arc<CompiledEncodeStub>>,
    /// Pointer-identity fast path for decode lookups.
    decode_fast: HashMap<FastDecodeKey, Arc<CompiledDecodeStub>>,
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
        let guard = self.inner.lock().unwrap();
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
        let mut guard = self.inner.lock().unwrap();
        guard.decode_stubs.insert(key, arc.clone());
        arc
    }

    /// Look up an encode stub by key.
    pub fn get_encode(&self, key: &EncodeCacheKey) -> Option<Arc<CompiledEncodeStub>> {
        let guard = self.inner.lock().unwrap();
        guard.encode_stubs.get(key).cloned()
    }

    /// Insert a compiled encode stub.
    pub fn insert_encode(
        &self,
        key: EncodeCacheKey,
        stub: CompiledEncodeStub,
    ) -> Arc<CompiledEncodeStub> {
        let arc = Arc::new(stub);
        let mut guard = self.inner.lock().unwrap();
        guard.encode_stubs.insert(key, arc.clone());
        arc
    }

    /// Fast-path encode lookup by shape pointer identity. Returns `None` if
    /// no stub has been compiled and fast-cached for this shape yet.
    pub fn get_encode_fast(&self, shape: &'static Shape) -> Option<Arc<CompiledEncodeStub>> {
        let guard = self.inner.lock().unwrap();
        guard
            .encode_fast
            .get(&FastEncodeKey(ShapePtr(shape)))
            .cloned()
    }

    /// Record a compiled encode stub in the pointer-identity fast path.
    pub fn insert_encode_fast(&self, shape: &'static Shape, stub: Arc<CompiledEncodeStub>) {
        let mut guard = self.inner.lock().unwrap();
        guard
            .encode_fast
            .insert(FastEncodeKey(ShapePtr(shape)), stub);
    }

    /// Fast-path decode lookup keyed on shape pointer + borrow mode +
    /// remote schema id.
    pub fn get_decode_fast(
        &self,
        shape: &'static Shape,
        borrow_mode: BorrowMode,
        remote_schema_id: u64,
    ) -> Option<Arc<CompiledDecodeStub>> {
        let guard = self.inner.lock().unwrap();
        guard
            .decode_fast
            .get(&FastDecodeKey {
                shape: ShapePtr(shape),
                borrow_mode,
                remote_schema_id,
            })
            .cloned()
    }

    /// Record a compiled decode stub in the pointer-identity fast path.
    pub fn insert_decode_fast(
        &self,
        shape: &'static Shape,
        borrow_mode: BorrowMode,
        remote_schema_id: u64,
        stub: Arc<CompiledDecodeStub>,
    ) {
        let mut guard = self.inner.lock().unwrap();
        guard.decode_fast.insert(
            FastDecodeKey {
                shape: ShapePtr(shape),
                borrow_mode,
                remote_schema_id,
            },
            stub,
        );
    }

    /// Number of decode stubs currently cached.
    pub fn decode_stub_count(&self) -> usize {
        self.inner.lock().unwrap().decode_stubs.len()
    }
}
