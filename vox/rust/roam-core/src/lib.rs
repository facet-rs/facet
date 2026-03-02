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

use facet_format::{FormatDeserializer, MetaSource};
use facet_postcard::PostcardParser;
use facet_reflect::Partial;
use roam_types::{Backing, SelfRef};

/// Return a process-global cached `&'static RpcPlan` for type `T`.
/// FIXME: requiring 'static here is wrong
/// FIXME: this function is now useless since we have RpcPlan::for_type
pub fn rpc_plan<T: facet::Facet<'static>>() -> &'static roam_types::RpcPlan {
    roam_types::RpcPlan::for_type::<T>()
}

/// Deserialize postcard-encoded `backing` bytes into `T` in place, returning a
/// [`roam_types::SelfRef`] that keeps the backing storage alive for the value.
// r[impl zerocopy.framing.value]
pub(crate) fn deserialize_postcard<T: facet::Facet<'static>>(
    backing: Backing,
) -> Result<SelfRef<T>, facet_format::DeserializeError> {
    // SAFETY: backing is heap-allocated with a stable address.
    // The SelfRef::try_new contract guarantees value is dropped before backing.
    SelfRef::try_new(backing, |bytes| {
        let mut value = std::mem::MaybeUninit::<T>::uninit();
        let ptr = facet_core::PtrUninit::from_maybe_uninit(&mut value);

        // SAFETY: ptr points to valid, aligned, properly-sized memory for T.
        #[allow(unsafe_code)]
        let partial: Partial<'_, true> = unsafe { Partial::from_raw_with_shape(ptr, T::SHAPE) }
            .map_err(facet_format::DeserializeError::from)?;

        let mut parser = PostcardParser::new(bytes);
        let mut deserializer = FormatDeserializer::new(&mut parser);
        let partial = deserializer.deserialize_into(partial, MetaSource::FromEvents)?;

        partial
            .finish_in_place()
            .map_err(facet_format::DeserializeError::from)?;

        // SAFETY: finish_in_place succeeded, so value is fully initialized.
        #[allow(unsafe_code)]
        Ok(unsafe { value.assume_init() })
    })
}

#[cfg(test)]
mod tests;
