//! In-memory stub cache.
//!
//! Compiled stubs are stored in a process-local hash map keyed by
//! `DecodeCacheKey` / `EncodeCacheKey`. No persistence across restarts.
//!
//! The cache owns the Cranelift `JITModule` which holds the memory maps for
//! all compiled stubs. Stubs are identified by stable function IDs obtained
//! from the module.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use vox_jit_abi::{BorrowedDecodeFn, DecodeCacheKey, EncodeCacheKey, EncodeFn, OwnedDecodeFn};

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
    pub fn insert_decode(&self, key: DecodeCacheKey, stub: CompiledDecodeStub) {
        let mut guard = self.inner.lock().unwrap();
        guard.decode_stubs.insert(key, Arc::new(stub));
    }

    /// Look up an encode stub by key.
    pub fn get_encode(&self, key: &EncodeCacheKey) -> Option<Arc<CompiledEncodeStub>> {
        let guard = self.inner.lock().unwrap();
        guard.encode_stubs.get(key).cloned()
    }

    /// Insert a compiled encode stub.
    pub fn insert_encode(&self, key: EncodeCacheKey, stub: CompiledEncodeStub) {
        let mut guard = self.inner.lock().unwrap();
        guard.encode_stubs.insert(key, Arc::new(stub));
    }

    /// Number of decode stubs currently cached.
    pub fn decode_stub_count(&self) -> usize {
        self.inner.lock().unwrap().decode_stubs.len()
    }
}
