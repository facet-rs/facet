//! Core implementations for the roam connectivity layer.
//!
//! This crate provides concrete implementations of the traits defined in
//! [`roam_types`]:
//!
//! - [`BareConduit`]: wraps a raw `Link` with postcard serialization.
//!   No reconnect, no reliability. For localhost, SHM, testing.
//! - `StableConduit` (TODO): wraps a Link + seq/ack/replay with
//!   bytes-based replay buffer. Handles reconnect transparently.

mod bare_conduit;
pub use bare_conduit::*;
pub use roam_types::TransportMode;

mod handshake;
pub use handshake::*;

mod into_conduit;
pub use into_conduit::*;

mod operation_store;
pub use operation_store::*;

mod transport_prologue;
pub use transport_prologue::*;

#[cfg(not(target_arch = "wasm32"))]
mod stable_conduit;
#[cfg(not(target_arch = "wasm32"))]
pub use stable_conduit::*;

#[cfg(not(target_arch = "wasm32"))]
mod memory_link;
#[cfg(not(target_arch = "wasm32"))]
pub use memory_link::*;

mod session;
pub use session::*;

#[cfg(not(target_arch = "wasm32"))]
mod driver;
#[cfg(not(target_arch = "wasm32"))]
pub use driver::*;

#[cfg(target_arch = "wasm32")]
mod wasm_driver;
#[cfg(target_arch = "wasm32")]
pub use wasm_driver::*;

use facet_reflect::Partial;
use roam_types::{Backing, SelfRef};

/// Return a process-global cached `&'static RpcPlan` for type `T`.
/// FIXME: requiring 'static here is wrong
/// FIXME: this function is now useless since we have RpcPlan::for_type
pub fn rpc_plan<T: facet::Facet<'static>>() -> &'static roam_types::RpcPlan {
    roam_types::RpcPlan::for_type::<T>()
}

/// Pre-built translation plan for deserializing the `Message` wire type.
///
/// Built once from the peer's schema (received during handshake) and our
/// local schema. Stored in the conduit's Rx half and used for every
/// incoming message.
pub struct MessagePlan {
    pub plan: roam_postcard::plan::TranslationPlan,
    pub registry: roam_types::SchemaRegistry,
}

impl MessagePlan {
    /// Build a message plan from the handshake result's schema exchange.
    pub fn from_handshake(result: &roam_types::HandshakeResult) -> Result<Self, String> {
        use roam_postcard::plan::{PlanInput, SchemaSet, build_plan};

        if result.peer_schema.is_empty() || result.our_schema.is_empty() {
            // No schemas exchanged — fall back to identity plan
            let plan = roam_postcard::build_identity_plan(
                <roam_types::Message<'static> as facet::Facet<'static>>::SHAPE,
            );
            return Ok(MessagePlan {
                plan,
                registry: roam_types::SchemaRegistry::new(),
            });
        }

        let remote = SchemaSet::from_extracted(result.peer_schema.clone());
        let local = SchemaSet::from_extracted(result.our_schema.clone());

        let plan = build_plan(&PlanInput {
            remote: &remote,
            local: &local,
        })
        .map_err(|e| format!("failed to build message translation plan: {e}"))?;

        Ok(MessagePlan {
            plan,
            registry: remote.registry,
        })
    }
}

/// Deserialize postcard-encoded `backing` bytes into `T` in place, returning a
/// [`roam_types::SelfRef`] that keeps the backing storage alive for the value.
// r[impl zerocopy.framing.value]
pub(crate) fn deserialize_postcard<T: facet::Facet<'static>>(
    backing: Backing,
) -> Result<SelfRef<T>, roam_postcard::DeserializeError> {
    let plan = roam_postcard::build_identity_plan(T::SHAPE);
    let registry = roam_types::SchemaRegistry::new();
    deserialize_postcard_with_plan(backing, &plan, &registry)
}

/// Deserialize postcard-encoded `backing` bytes into `T` using a pre-built
/// translation plan and schema registry for the remote peer's type layout.
// r[impl zerocopy.framing.value]
pub(crate) fn deserialize_postcard_with_plan<T: facet::Facet<'static>>(
    backing: Backing,
    plan: &roam_postcard::plan::TranslationPlan,
    registry: &roam_types::SchemaRegistry,
) -> Result<SelfRef<T>, roam_postcard::DeserializeError> {
    // SAFETY: backing is heap-allocated with a stable address.
    // The SelfRef::try_new contract guarantees value is dropped before backing.
    SelfRef::try_new(backing, |bytes| {
        let mut value = std::mem::MaybeUninit::<T>::uninit();
        let ptr = facet_core::PtrUninit::from_maybe_uninit(&mut value);

        // SAFETY: ptr points to valid, aligned, properly-sized memory for T.
        #[allow(unsafe_code)]
        let partial: Partial<'_, true> = unsafe { Partial::from_raw_with_shape(ptr, T::SHAPE) }
            .map_err(|e| roam_postcard::DeserializeError::ReflectError(e.to_string()))?;

        let partial = roam_postcard::deserialize_into(partial, bytes, plan, registry)?;

        partial
            .finish_in_place()
            .map_err(|e| roam_postcard::DeserializeError::ReflectError(e.to_string()))?;

        // SAFETY: finish_in_place succeeded, so value is fully initialized.
        #[allow(unsafe_code)]
        Ok(unsafe { value.assume_init() })
    })
}

pub mod testing;

#[cfg(test)]
mod tests;
