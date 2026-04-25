//! In-memory cache of JIT-compiled encoders and decoders.
//!
//! Process-local. No persistence across restarts. Compiled entries are
//! leaked at insertion time: `Box::leak` mints a `&'static CompiledEncoder`
//! / `&'static CompiledDecoder` that lives for the process lifetime.
//! Entries are never evicted, so reference counting would only buy us
//! atomic-clone overhead on every hot-path lookup.
//!
//! Encoders are keyed by `&'static Shape` alone — `Shape: Hash + Eq` via
//! its compiler-issued `ConstTypeId`, so the same Rust type collapses to
//! one entry across all callers. Decoders also need the `remote_schema_id`
//! (the content-addressed `SchemaHash` of the peer's root type) so two
//! peers with different remote schemas get distinct compiled programs
//! sharing the same local shape. Owned and borrowed compile outputs share
//! one entry per `(shape, schema_id)` and lazy-fill `OnceLock` slots.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::AtomicUsize;

use arc_swap::ArcSwap;
use facet_core::Shape;
use museair::FixedState;
use vox_jit_abi::{BorrowedDecodeFn, EncodeFn, OwnedDecodeFn};
use vox_postcard::ir::EncodeProgram;

/// Hasher used by `CompiledCache`'s HashMaps. The cache is process-local, never
/// receives untrusted input, and keys are stable code-segment pointers + small
/// integers — cryptographic resistance buys nothing here, so we ditch
/// `RandomState` (SipHash13) for `museair::FixedState`. Visible in nperf
/// profiles: SipHash on a `(&'static Shape, u64)` key was ~34% of every
/// `try_decode_owned` call.
type CacheHasher = FixedState;
type CacheMap<K, V> = HashMap<K, V, CacheHasher>;

fn new_cache_map<K, V>() -> CacheMap<K, V> {
    HashMap::with_hasher(FixedState::new(0))
}

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
struct DecodeKey {
    shape: &'static Shape,
    remote_schema_id: u64,
}

// ---------------------------------------------------------------------------
// Compiled encoder/decoder entries
// ---------------------------------------------------------------------------

/// A JIT-compiled decoder that can be called via the JIT ABI.
///
/// Owned and borrowed function pointers are filled lazily — both share one
/// entry per `(shape, remote_schema_id)`. A caller asking for owned only
/// triggers compilation of `owned_fn`; a later borrowed call fills
/// `borrowed_fn` in the same entry.
pub struct CompiledDecoder {
    /// Local Rust shape this decoder produces values for.
    pub local_shape: &'static Shape,
    pub owned_fn: OnceLock<OwnedDecodeFn>,
    pub borrowed_fn: OnceLock<BorrowedDecodeFn>,
}

impl CompiledDecoder {
    fn new_empty(local_shape: &'static Shape) -> Self {
        Self {
            local_shape,
            owned_fn: OnceLock::new(),
            borrowed_fn: OnceLock::new(),
        }
    }
}

/// A JIT-compiled encoder for one shape.
pub struct CompiledEncoder {
    /// Local Rust shape this encoder takes input values from.
    pub local_shape: &'static Shape,
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

/// Process-local cache of compiled encoders and decoders.
///
/// Steady-state lookup is one atomic load + one pointer copy via
/// `ArcSwap` — no locking. Insertions copy-on-write the inner `HashMap`
/// via `ArcSwap::rcu`, so they're rare-path operations only (one per
/// distinct shape ever seen by this process).
pub struct CompiledCache {
    encoders: ArcSwap<CacheMap<&'static Shape, &'static CompiledEncoder>>,
    decoders: ArcSwap<CacheMap<DecodeKey, &'static CompiledDecoder>>,
}

impl Default for CompiledCache {
    fn default() -> Self {
        Self {
            encoders: ArcSwap::from_pointee(new_cache_map()),
            decoders: ArcSwap::from_pointee(new_cache_map()),
        }
    }
}

impl CompiledCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Look up a compiled encoder by local shape. One atomic load and (on
    /// hit) one pointer copy — no locking.
    pub fn get_encode(&self, shape: &'static Shape) -> Option<&'static CompiledEncoder> {
        self.encoders.load().get(shape).copied()
    }

    /// Insert a compiled encoder. `Box::leak`s it and returns the resulting
    /// `&'static`. The entry lives for the process lifetime.
    pub fn insert_encode(
        &self,
        shape: &'static Shape,
        encoder: CompiledEncoder,
    ) -> &'static CompiledEncoder {
        let leaked: &'static CompiledEncoder = Box::leak(Box::new(encoder));
        self.encoders.rcu(|cur| {
            let mut next = (**cur).clone();
            next.insert(shape, leaked);
            next
        });
        leaked
    }

    /// Look up the consolidated decoder entry for `(shape, remote_schema_id)`.
    /// One atomic load and (on hit) one pointer copy. Returns `None` if no
    /// entry exists yet — call `get_or_insert_decode_entry` to create one
    /// before lazy-filling owned/borrowed slots.
    pub fn get_decode(
        &self,
        shape: &'static Shape,
        remote_schema_id: u64,
    ) -> Option<&'static CompiledDecoder> {
        self.decoders
            .load()
            .get(&DecodeKey {
                shape,
                remote_schema_id,
            })
            .copied()
    }

    /// Look up or create the consolidated decoder entry for
    /// `(shape, remote_schema_id)`. The returned `&'static CompiledDecoder`
    /// has empty `owned_fn`/`borrowed_fn` slots when freshly created;
    /// callers fill them via `OnceLock::set` / `get_or_init`.
    pub fn get_or_insert_decode_entry(
        &self,
        shape: &'static Shape,
        remote_schema_id: u64,
    ) -> &'static CompiledDecoder {
        if let Some(entry) = self.get_decode(shape, remote_schema_id) {
            return entry;
        }
        let leaked: &'static CompiledDecoder =
            Box::leak(Box::new(CompiledDecoder::new_empty(shape)));
        let mut inserted = leaked;
        self.decoders.rcu(|cur| {
            let key = DecodeKey {
                shape,
                remote_schema_id,
            };
            if let Some(&existing) = cur.get(&key) {
                inserted = existing;
                return (**cur).clone();
            }
            let mut next = (**cur).clone();
            next.insert(key, leaked);
            inserted = leaked;
            next
        });
        inserted
    }

    /// Number of compiled decoder entries currently cached.
    pub fn decoder_count(&self) -> usize {
        self.decoders.load().len()
    }
}
