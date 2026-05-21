//! Core implementations for the vox connectivity layer.
//!
//! This crate provides concrete implementations of the traits defined in
//! [`vox_types`]. The only conduit shape is [`BareConduit`]: wraps a raw
//! `Link` with postcard serialization. No reconnect, no reliability —
//! reconnect was removed (StableConduit deleted) because the abstraction
//! had no real users.

mod bare_conduit;
pub use bare_conduit::*;
pub use vox_types::TransportMode;

mod handshake;
pub use handshake::*;

mod into_conduit;
pub use into_conduit::*;

mod operation_store;
pub use operation_store::*;

mod transport_prologue;
pub use transport_prologue::*;

mod link_source;
pub use link_source::*;

#[cfg(not(target_arch = "wasm32"))]
mod memory_link;
#[cfg(not(target_arch = "wasm32"))]
pub use memory_link::*;

#[cfg(not(target_arch = "wasm32"))]
use std::panic::AssertUnwindSafe;

mod session;
pub use session::*;

mod driver;
pub use driver::*;

use vox_types::{Backing, SelfRef};

/// Pre-built translation plan for deserializing the `Message` wire type.
///
/// Built once from the peer's schema (received during handshake) and our
/// local schema. Stored in the conduit's Rx half and used for every
/// incoming message.
pub struct MessagePlan {
    pub remote_schema_id: u64,
    pub plan: vox_postcard::plan::TranslationPlan,
    pub registry: vox_types::SchemaRegistry,
}

impl MessagePlan {
    /// Build a message plan from the handshake result's schema exchange.
    pub fn from_handshake(result: &vox_types::HandshakeResult) -> Result<Self, String> {
        use vox_postcard::plan::{PlanInput, SchemaSet, build_plan};

        if result.peer_schema.is_empty() || result.our_schema.is_empty() {
            // No schemas exchanged — fall back to identity plan
            let plan = vox_postcard::build_identity_plan(
                <vox_types::Message<'static> as facet::Facet<'static>>::SHAPE,
            );
            return Ok(MessagePlan {
                remote_schema_id: 0,
                plan,
                registry: vox_types::SchemaRegistry::new(),
            });
        }

        let remote = SchemaSet::from_schemas(result.peer_schema.clone());
        let local = SchemaSet::from_schemas(result.our_schema.clone());

        let plan = build_plan(&PlanInput {
            remote: &remote,
            local: &local,
        })
        .map_err(|e| format!("failed to build message translation plan: {e}"))?;

        Ok(MessagePlan {
            remote_schema_id: remote.root.id.0,
            plan,
            registry: remote.registry,
        })
    }
}

/// Deserialize postcard-encoded `backing` bytes into `T` in place, returning
/// a [`vox_types::SelfRef`] that keeps the backing storage alive for the
/// value. Uses the identity plan; for plan-aware decoding, use
/// [`deserialize_postcard_with_plan`].
// r[impl zerocopy.framing.value]
#[allow(dead_code)]
pub(crate) fn deserialize_postcard<T: facet::Facet<'static>>(
    backing: Backing,
) -> Result<SelfRef<T>, vox_postcard::DeserializeError> {
    let plan = vox_postcard::build_identity_plan(T::SHAPE);
    let registry = vox_types::SchemaRegistry::new();
    deserialize_postcard_with_plan(backing, &plan, &registry)
}

/// Deserialize postcard-encoded `backing` bytes into `T` using a pre-built
/// translation plan and schema registry for the remote peer's type layout.
// r[impl zerocopy.framing.value]
#[allow(dead_code)]
pub(crate) fn deserialize_postcard_with_plan<T: facet::Facet<'static>>(
    backing: Backing,
    plan: &vox_postcard::plan::TranslationPlan,
    registry: &vox_types::SchemaRegistry,
) -> Result<SelfRef<T>, vox_postcard::DeserializeError> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        SelfRef::try_new(backing, |bytes| {
            match std::panic::catch_unwind(AssertUnwindSafe(|| {
                vox_jit::global_runtime().try_decode_owned::<T>(bytes, 0, plan, registry)
            })) {
                Ok(Some(result)) => result,
                Ok(None) => vox_postcard::from_slice_with_plan::<T>(bytes, plan, registry),
                Err(payload) => {
                    tracing::warn!(
                        shape = %T::SHAPE,
                        panic = %panic_payload_message(&payload),
                        "vox message JIT decode panicked; falling back"
                    );
                    vox_postcard::from_slice_with_plan::<T>(bytes, plan, registry)
                }
            }
        })
    }
    #[cfg(target_arch = "wasm32")]
    {
        SelfRef::try_new(backing, |bytes| {
            vox_postcard::from_slice_with_plan::<T>(bytes, plan, registry)
        })
    }
}

/// Like [`deserialize_postcard`] but uses an already-resolved JIT decoder,
/// skipping the global cache lookup. Used by conduits that resolved their
/// decoder at construction.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn deserialize_postcard_with_decoder<T: facet::Facet<'static>>(
    backing: Backing,
    decoder: Option<&'static vox_jit::cache::CompiledDecoder>,
    plan: &vox_postcard::plan::TranslationPlan,
    registry: &vox_types::SchemaRegistry,
) -> Result<SelfRef<T>, vox_postcard::DeserializeError> {
    SelfRef::try_new(backing, |bytes| {
        let Some(decoder) = decoder else {
            return vox_postcard::from_slice_with_plan::<T>(bytes, plan, registry);
        };
        let Some(decode_fn) = decoder.owned_fn_ptr() else {
            tracing::warn!(
                shape = %T::SHAPE,
                "vox message JIT decoder missing function pointer; falling back"
            );
            return vox_postcard::from_slice_with_plan::<T>(bytes, plan, registry);
        };
        match std::panic::catch_unwind(AssertUnwindSafe(|| {
            vox_jit::decode_owned_with::<T>(decode_fn, bytes)
        })) {
            Ok(result) => result,
            Err(payload) => {
                tracing::warn!(
                    shape = %T::SHAPE,
                    panic = %panic_payload_message(&payload),
                    "vox message JIT decode panicked; falling back"
                );
                vox_postcard::from_slice_with_plan::<T>(bytes, plan, registry)
            }
        }
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn panic_payload_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&'static str>() {
        (*message).to_owned()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "non-string panic payload".to_owned()
    }
}

pub mod testing;

#[cfg(test)]
mod tests;
