//! In-memory cache of JIT-compiled encoders and decoders.
//!
//! Entries are stored in a process-local hash map keyed by `DecodeCacheKey`
//! / `EncodeCacheKey`. No persistence across restarts.
//!
//! The cache owns the Cranelift `JITModule` which holds the memory maps for
//! all compiled functions. They are identified by stable function IDs
//! obtained from the module.
//!
//! Compiled entries are leaked at insertion time: `Box::leak` mints a
//! `&'static CompiledEncoder` / `&'static CompiledDecoder` that lives for
//! the process lifetime. Entries are never evicted, so reference counting
//! would only buy us atomic-clone overhead on every hot-path lookup.

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
// Compiled encoder/decoder entries
// ---------------------------------------------------------------------------

/// A JIT-compiled decoder that can be called via the JIT ABI.
pub struct CompiledDecoder {
    /// The owned-mode function pointer (or None if only borrowed mode was compiled).
    pub owned_fn: Option<OwnedDecodeFn>,
    /// The borrowed-mode function pointer (or None if only owned mode was compiled).
    pub borrowed_fn: Option<BorrowedDecodeFn>,
    /// The key that produced this decoder (for debugging / cache validation).
    pub key: DecodeCacheKey,
}

/// A JIT-compiled encoder for one shape.
pub struct CompiledEncoder {
    pub key: EncodeCacheKey,
    pub encode_fn: EncodeFn,
    /// Largest encoded output observed for this shape. Used to seed the
    /// initial `EncodeCtx` capacity so the hot path avoids `realloc`
    /// churn after the first encode of a given shape.
    pub size_hint: AtomicUsize,
    /// Lowered IR kept alive so parent compiles can inline this encoder's
    /// body instead of emitting a `call_indirect` to `encode_fn`.
    pub program: Arc<EncodeProgram>,
    /// Compiled encoders for this shape's direct child shapes (the
    /// `WriteShape` ops in `program`). Swapped into
    /// `EncodeCtx_.child_encoders` when this shape is inlined into a parent,
    /// so its grandchildren resolve too.
    pub child_encoders: Arc<crate::codegen::ChildEncoderMap>,
}

// ---------------------------------------------------------------------------
// Compiled-encoder/decoder cache
// ---------------------------------------------------------------------------

/// Process-local, in-memory cache of compiled encoders and decoders.
///
/// The structural (slow-path) maps live behind a `Mutex`; the shape-keyed
/// fast paths live behind `ArcSwap` so the steady-state lookup is one atomic
/// load + one pointer copy, no locking. Entries are never evicted — they're
/// `Box::leak`ed at insertion time and live for the process lifetime.
pub struct CompiledCache {
    slow: Mutex<CompiledCacheSlow>,
    encode_fast: ArcSwap<HashMap<&'static Shape, &'static CompiledEncoder>>,
    decode_fast: ArcSwap<HashMap<FastDecodeKey, &'static CompiledDecoder>>,
}

impl Default for CompiledCache {
    fn default() -> Self {
        Self {
            slow: Mutex::new(CompiledCacheSlow::default()),
            encode_fast: ArcSwap::from_pointee(HashMap::new()),
            decode_fast: ArcSwap::from_pointee(HashMap::new()),
        }
    }
}

#[derive(Default)]
struct CompiledCacheSlow {
    decoders: HashMap<DecodeCacheKey, &'static CompiledDecoder>,
    encoders: HashMap<EncodeCacheKey, &'static CompiledEncoder>,
}

impl CompiledCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Look up a compiled decoder by key.
    ///
    /// Returns None if the decoder is not yet compiled or if the cache key's
    /// calibration generation is stale.
    pub fn get_decode(&self, key: &DecodeCacheKey) -> Option<&'static CompiledDecoder> {
        let guard = self.slow.lock().unwrap();
        guard.decoders.get(key).copied()
    }

    /// Insert a compiled decoder. The entry is leaked (`Box::leak`) and the
    /// resulting `&'static` is stored both here and returned to the caller.
    ///
    /// If a decoder already exists for this key, it is replaced (should not
    /// happen in normal operation — callers check first).
    pub fn insert_decode(
        &self,
        key: DecodeCacheKey,
        decoder: CompiledDecoder,
    ) -> &'static CompiledDecoder {
        let leaked: &'static CompiledDecoder = Box::leak(Box::new(decoder));
        let mut guard = self.slow.lock().unwrap();
        guard.decoders.insert(key, leaked);
        leaked
    }

    /// Look up a compiled encoder by key.
    pub fn get_encode(&self, key: &EncodeCacheKey) -> Option<&'static CompiledEncoder> {
        let guard = self.slow.lock().unwrap();
        guard.encoders.get(key).copied()
    }

    /// Insert a compiled encoder. The entry is leaked (`Box::leak`) and the
    /// resulting `&'static` is stored both here and returned to the caller.
    pub fn insert_encode(
        &self,
        key: EncodeCacheKey,
        encoder: CompiledEncoder,
    ) -> &'static CompiledEncoder {
        let leaked: &'static CompiledEncoder = Box::leak(Box::new(encoder));
        let mut guard = self.slow.lock().unwrap();
        guard.encoders.insert(key, leaked);
        leaked
    }

    /// Fast-path encoder lookup by shape identity (ConstTypeId). One atomic
    /// load and (on hit) one pointer copy — no locking.
    pub fn get_encode_fast(&self, shape: &'static Shape) -> Option<&'static CompiledEncoder> {
        self.encode_fast.load().get(shape).copied()
    }

    /// Record a compiled encoder in the fast path.
    pub fn insert_encode_fast(&self, shape: &'static Shape, encoder: &'static CompiledEncoder) {
        self.encode_fast.rcu(|cur| {
            let mut next = (**cur).clone();
            next.insert(shape, encoder);
            next
        });
    }

    /// Fast-path decoder lookup keyed on shape identity + borrow mode +
    /// remote schema id. One atomic load and (on hit) one pointer copy.
    pub fn get_decode_fast(
        &self,
        shape: &'static Shape,
        borrow_mode: BorrowMode,
        remote_schema_id: u64,
    ) -> Option<&'static CompiledDecoder> {
        self.decode_fast
            .load()
            .get(&FastDecodeKey {
                shape,
                borrow_mode,
                remote_schema_id,
            })
            .copied()
    }

    /// Record a compiled decoder in the fast path.
    pub fn insert_decode_fast(
        &self,
        shape: &'static Shape,
        borrow_mode: BorrowMode,
        remote_schema_id: u64,
        decoder: &'static CompiledDecoder,
    ) {
        let key = FastDecodeKey {
            shape,
            borrow_mode,
            remote_schema_id,
        };
        self.decode_fast.rcu(|cur| {
            let mut next = (**cur).clone();
            next.insert(key, decoder);
            next
        });
    }

    /// Number of compiled decoders currently cached.
    pub fn decoder_count(&self) -> usize {
        self.slow.lock().unwrap().decoders.len()
    }
}
