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

/// The host types `vixen` declares — the domain types `Tree` and `TreeEntry`,
/// injected into the compiler through [`vix::compiler::CompilerConfig::host_types`]
/// so the bare language no longer hardcodes them as `ExternKind` variants (issue
/// 2520). They remain extern-backed opaque values with byte-stable `vix.Tree` /
/// `vix.TreeEntry` identity; only the *declaration* moves here. `TreeEntry` is now
/// vestigial — the standalone `receiver / segment` projection and `TreeEntry.text()`
/// method were retired once exec-origin reads moved onto the `tree_read` primitive
/// — but it stays declared to keep its nominal identity stable.
pub const HOST_TYPES: &[vix::binding::HostTypeDecl] = &[
    vix::binding::HostTypeDecl {
        name: vix::binding::TREE,
    },
    vix::binding::HostTypeDecl {
        name: vix::binding::TREE_ENTRY,
    },
];

/// The host-type methods `vixen` declares on the domain types, injected into the
/// compiler through [`vix::compiler::CompilerConfig::methods`]. The dedicated ops
/// they name are still lowered and executed by `vix-core` (the machine engine);
/// only the *declaration* lives here, so `vix-core` no longer hardcodes
/// `.glob`/`.len`/`.url` (issue 2520). (`.text()` is no longer a dedicated method:
/// exec-origin tree reads spell the `tree_read` primitive request directly.)
pub const DOMAIN_METHODS: &[vix::binding::MethodDecl] = &[
    vix::binding::MethodDecl {
        receiver: vix::binding::ReceiverType::Host(vix::binding::TREE),
        name: "glob",
        arity: 1,
        op: vix::binding::MethodOp::TreeGlob,
    },
    vix::binding::MethodDecl {
        receiver: vix::binding::ReceiverType::Blob,
        name: "len",
        arity: 0,
        op: vix::binding::MethodOp::BlobLen,
    },
    vix::binding::MethodDecl {
        receiver: vix::binding::ReceiverType::Registry,
        name: "url",
        arity: 1,
        op: vix::binding::MethodOp::RegistryUrl,
    },
];

pub use decode_primitive::*;
pub use fetch_primitive::*;
pub use leaf_overrides::LEAF_OVERRIDES;
pub use primitive_value_decode::*;
pub use tree_glob_primitive::*;
pub use tree_read_primitive::*;
pub use typed_primitive::*;
