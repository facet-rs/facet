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
//! one entry across all callers. Decoders also need `borrow_mode` and the
//! `remote_schema_id` (the content-addressed `SchemaHash` of the peer's
//! root type) so two peers with different remote schemas get distinct
//! compiled programs sharing the same local shape.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

use arc_swap::ArcSwap;
use facet_core::Shape;
use vox_jit_abi::{BorrowedDecodeFn, EncodeFn, OwnedDecodeFn};
use vox_jit_cal::BorrowMode;
use vox_postcard::ir::EncodeProgram;

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
struct DecodeKey {
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
    /// Local Rust shape this decoder produces values for.
    pub local_shape: &'static Shape,
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
    encoders: ArcSwap<HashMap<&'static Shape, &'static CompiledEncoder>>,
    decoders: ArcSwap<HashMap<DecodeKey, &'static CompiledDecoder>>,
}

impl Default for CompiledCache {
    fn default() -> Self {
        Self {
            encoders: ArcSwap::from_pointee(HashMap::new()),
            decoders: ArcSwap::from_pointee(HashMap::new()),
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

    /// Look up a compiled decoder keyed on `(local_shape, borrow_mode,
    /// remote_schema_id)`. One atomic load and (on hit) one pointer copy.
    pub fn get_decode(
        &self,
        shape: &'static Shape,
        borrow_mode: BorrowMode,
        remote_schema_id: u64,
    ) -> Option<&'static CompiledDecoder> {
        self.decoders
            .load()
            .get(&DecodeKey {
                shape,
                borrow_mode,
                remote_schema_id,
            })
            .copied()
    }

    /// Insert a compiled decoder. `Box::leak`s it and returns the resulting
    /// `&'static`. The entry lives for the process lifetime.
    pub fn insert_decode(
        &self,
        shape: &'static Shape,
        borrow_mode: BorrowMode,
        remote_schema_id: u64,
        decoder: CompiledDecoder,
    ) -> &'static CompiledDecoder {
        let leaked: &'static CompiledDecoder = Box::leak(Box::new(decoder));
        self.decoders.rcu(|cur| {
            let mut next = (**cur).clone();
            next.insert(
                DecodeKey {
                    shape,
                    borrow_mode,
                    remote_schema_id,
                },
                leaked,
            );
            next
        });
        leaked
    }

    /// Number of compiled decoders currently cached.
    pub fn decoder_count(&self) -> usize {
        self.decoders.load().len()
    }
}
