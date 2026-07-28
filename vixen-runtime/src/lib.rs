//! vixen-runtime — the runnable Vix system.
//!
//! `vix-core` is the language (V8): it parses, compiles, lowers, and executes,
//! but ships no primitives and no prelude. `vixen-primitives` is the batteries.
//! This crate is the assembly (Node): it installs the concrete primitives into
//! the engine's dispatcher ([`default_primitive_dispatcher`]), supplies the
//! stdlib prelude ([`default_config`]), hosts the program-running harness
//! ([`ratchet`]/[`budget`]), and re-exports the whole surface so a consumer can
//! depend on this one crate for a working system.
//!
//! [`module_graph`] loads a directory of `.vix` files into owned module
//! sources for the compiler and runner.
//!
//! # Re-exports keep the seam visible
//!
//! Depending on this one crate is enough to get a working system, but the two
//! halves are re-exported as *named* crates ([`vix`], [`vixen_primitives`])
//! rather than glob-flattened into this root. A consumer therefore always spells
//! which side of the language/domain line a name comes from —
//! `vixen_runtime::vix::compiler::Compiler` is language,
//! `vixen_runtime::vixen_primitives::HOST_TYPES` is domain — and a new name in
//! `vix-core` can never silently collide with one in `vixen-primitives`.

pub mod budget;
pub mod module_graph;
pub mod ratchet;

use std::sync::Arc;

// The two halves, named — not `pub use vix::*` / `pub use vixen_primitives::*`.
pub use vix;
pub use vixen_primitives;

pub use module_graph::{
    DEFAULT_ROOT_MODULE, ModuleFile, ModuleGraph, ModuleGraphError, ModuleRoot, load_module_graph,
};

use vix::compiler::CompilerConfig;
use vix::runtime::{
    CodataRegistry, EventSink, PrimitiveDispatcher, PrimitiveRegistry, RawPrimitive, Runtime,
};
use vixen_primitives::{
    BlobGunzipPrimitive, BlobLenPrimitive, DecodePrimitive, ExecPrimitive, PinnedFetchPrimitive,
    RegistryUrlPrimitive, TreeGlobPrimitive, TreeReadPrimitive, TypedAdapter, UntarPrimitive,
};

/// The built-in registered primitives, as data: this is the *one* place that
/// lists them. Adding a primitive is one entry here, not a second
/// hand-registration. (The compiler's matching surface *contracts* are language
/// data in `vix::runtime::builtin_primitive_surfaces`.)
#[must_use]
pub fn builtin_primitives<Ctx>() -> Vec<Arc<dyn RawPrimitive<Ctx>>> {
    vec![
        Arc::new(DecodePrimitive::default()),
        Arc::new(TypedAdapter::new::<Ctx>(PinnedFetchPrimitive)),
        Arc::new(TreeReadPrimitive::default()),
        Arc::new(RegistryUrlPrimitive::default()),
        Arc::new(TypedAdapter::new::<Ctx>(BlobGunzipPrimitive)),
        Arc::new(BlobLenPrimitive::default()),
        Arc::new(TypedAdapter::new::<Ctx>(UntarPrimitive)),
        // The process-running effect, on the registered rail (exec-rail.md):
        // the capability-role request keys the demand, the `ExecBackend`
        // service owns the process boundary, and `vix-core` spells no tool
        // name.
        Arc::new(ExecPrimitive::default()),
    ]
}

/// A dispatcher carrying the builtin primitives — what `vix-core`'s empty
/// dispatcher is replaced with to get a runnable system.
#[must_use]
pub fn default_primitive_dispatcher<Ctx>() -> PrimitiveDispatcher<Ctx> {
    // The builtin primitives include `Tree`-typed descriptors (`tree-read`) whose
    // construction hashes `Host("Tree")`, so the host-extern names must be
    // reserved before they are built — independent of any compiler config. This
    // is the schema-registration seam that keeps `Tree`/`TreeEntry` out of
    // `vix-core`'s hardcoded builtin list (idempotent).
    vixen_primitives::register_host_types();
    let mut registry = PrimitiveRegistry::default();
    for primitive in builtin_primitives::<Ctx>() {
        registry
            .register(primitive)
            .expect("built-in primitives register once, each under a distinct id");
    }
    PrimitiveDispatcher::new(Arc::new(registry))
}

/// A registry carrying the builtin codata primitives (`glob`) — what `vix-core`'s
/// empty [`CodataRegistry`] is replaced with so effect streams can be realized.
/// Codata primitives are not `Ctx`-generic (draining a stream needs nothing from
/// the embedder context), so this is the single, monomorphic builtin list.
#[must_use]
pub fn default_codata_registry() -> CodataRegistry {
    // `TreeGlobPrimitive`'s descriptor hashes `Host("Tree")`, so reserve the
    // host-extern names before building it (idempotent) — the same seam as
    // [`default_primitive_dispatcher`].
    vixen_primitives::register_host_types();
    let mut registry = CodataRegistry::default();
    registry
        .register(Arc::new(TreeGlobPrimitive::default()))
        .expect("built-in codata primitives register once, each under a distinct id");
    registry
}

/// Install the builtin primitives into a runtime constructed by `vix-core` (which
/// ships an empty dispatcher and an empty codata registry).
pub fn install_builtins<S: EventSink, Ctx>(runtime: &mut Runtime<S, Ctx>) {
    runtime.set_primitive_dispatcher(default_primitive_dispatcher());
    runtime.set_codata_registry(default_codata_registry());
}

/// The compiler for the runnable system: [`default_config`] plus the free-function
/// primitive surfaces `vixen` injects ([`vixen_primitives::injected_primitive_surfaces`]).
///
/// Constructing the compiler through the config alone is not enough any more —
/// an injected surface is what makes `untar` resolve at all, now that it is a
/// primitive in `vixen-primitives` rather than an intrinsic in `vix-core`.
#[must_use]
pub fn default_compiler() -> vix::compiler::Compiler {
    vix::compiler::Compiler::with_config_and_primitive_surfaces(
        default_config(),
        vixen_primitives::injected_primitive_surfaces(),
    )
}

/// The compiler config for the runnable system: the `vix-core` defaults plus the
/// vixen stdlib prelude. `vix-core`'s own `CompilerConfig::default()` ships an
/// empty prelude (the bare language). Prefer [`default_compiler`], which also
/// carries the injected primitive surfaces.
#[must_use]
pub fn default_config() -> CompilerConfig {
    // Reserve the domain host-type names with the core schema batch before they
    // are declared — the schema-registration seam that keeps `Tree`/`TreeEntry`
    // out of `vix-core`'s hardcoded builtin list (idempotent).
    vixen_primitives::register_host_types();
    CompilerConfig {
        prelude: vixen_primitives::stdlib::PRELUDE_SOURCES,
        methods: vixen_primitives::DOMAIN_METHODS,
        host_types: vixen_primitives::HOST_TYPES,
        capabilities: vixen_primitives::CAPABILITY_TYPES,
        ..CompilerConfig::default()
    }
}
