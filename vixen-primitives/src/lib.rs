//! vixen-primitives — the concrete primitives and pure-vix stdlib the `vixen`
//! runtime installs into the `vix` language.
//!
//! `vix-core` is the bare language: it knows the *shape* of the builtin
//! primitives (their [`vix::runtime`] descriptors) but ships no implementations
//! and no prelude. This crate is the other half — the `RawPrimitive`
//! implementations behind `decode`/`fetch`/`observe`/`tree-read`, the typed
//! convenience layer they are written against, and the [`stdlib`] prelude
//! functions ([`stdlib::PRELUDE_SOURCES`]) — assembled into a runnable system by
//! `vixen-runtime`.

pub mod typed_primitive;

mod decode_primitive;
mod leaf_overrides;
mod fetch_primitive;
mod primitive_value_decode;
mod tree_glob_primitive;
mod tree_read_primitive;

pub mod stdlib;

/// Single import root for the primitive modules: the core raw effect protocol
/// (`vix::runtime` — `RawPrimitive`, `EffectCtx`, the descriptor types, …) plus
/// this crate's typed convenience layer. The moved primitive sources reach both
/// through `crate::rt::…`, so they need no per-symbol classification of which
/// half a name came from.
pub(crate) mod rt {
    pub use crate::typed_primitive::*;
    pub use vix::runtime::*;
}

pub use decode_primitive::*;
pub use fetch_primitive::*;
pub use leaf_overrides::LEAF_OVERRIDES;
pub use primitive_value_decode::*;
pub use tree_glob_primitive::*;
pub use tree_read_primitive::*;
pub use typed_primitive::*;
