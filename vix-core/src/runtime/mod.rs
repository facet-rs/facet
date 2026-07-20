//! Runtime ownership boundaries for the new Vix compiler path.
//!
//! This is the language's execution engine and the *raw* primitive protocol
//! (`RawPrimitive`, `PrimitiveDispatcher`, `EffectCtx`, the descriptor types in
//! [`primitive_bindings`]). It ships no concrete primitives — those, and the
//! typed convenience layer they are written against, live in `vixen-primitives`
//! and are installed by `vixen-runtime`.

mod abi;
mod blob_persistence;
mod decode_value;
mod error;
mod fixture;
mod identity;
mod model;
mod observe;
mod primitive;
mod primitive_bindings;
mod scheduler;
mod store;

pub use abi::*;
pub use blob_persistence::*;
pub use decode_value::*;
pub use error::*;
pub use fixture::*;
pub use identity::*;
pub use model::*;
pub use observe::*;
pub use primitive::*;
pub use primitive_bindings::*;
pub use scheduler::*;
pub use store::*;
