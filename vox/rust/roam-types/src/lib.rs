macro_rules! declare_id {
    ($(#[$meta:meta])* $name:ident, $inner:ty) => {
        $(#[$meta])*
        #[derive(Facet, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Clone, Copy)]
        #[repr(transparent)]
        #[facet(transparent)]
        pub struct $name(pub $inner);

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl $name {
            /// Returns `true` if this ID has the given parity (even or odd).
            pub fn has_parity(self, parity: crate::Parity) -> bool {
                match parity {
                    crate::Parity::Even => self.0.is_multiple_of(2),
                    crate::Parity::Odd => !self.0.is_multiple_of(2),
                }
            }
        }

        impl crate::IdType for $name {
            fn from_raw(raw: u64) -> Self {
                Self(raw as $inner)
            }
        }

    };
}

/// Trait implemented by all `declare_id!` types, enabling generic ID allocation.
pub trait IdType: Copy {
    fn from_raw(raw: u64) -> Self;
}

/// Allocates IDs with a given parity (odd or even), stepping by 2.
///
/// Odd parity: 1, 3, 5, 7, ...
/// Even parity: 2, 4, 6, 8, ...
// r[impl rpc.request.id-allocation]
pub struct IdAllocator<T: IdType> {
    next: u64,
    _phantom: std::marker::PhantomData<T>,
}

impl<T: IdType> IdAllocator<T> {
    /// Create a new allocator for the given parity.
    pub fn new(parity: Parity) -> Self {
        let next = match parity {
            Parity::Odd => 1,
            Parity::Even => 2,
        };
        Self {
            next,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Allocate the next ID.
    pub fn alloc(&mut self) -> T {
        let id = T::from_raw(self.next);
        self.next += 2;
        id
    }
}

mod rpc_plan;
pub use rpc_plan::*;

mod roam_error;
pub use roam_error::*;

mod services;
pub use services::*;

mod requests;
pub use requests::*;

mod message;
pub use message::*;

mod selfref;
pub use selfref::*;

mod link;
pub use link::*;

mod conduit;
pub use conduit::*;

mod metadata;
pub use metadata::*;

mod calls;
pub use calls::*;

mod channel;
pub use channel::*;

#[cfg(not(target_arch = "wasm32"))]
mod channel_binding;
#[cfg(not(target_arch = "wasm32"))]
pub use channel_binding::*;

mod shape_classify;
pub use shape_classify::*;
