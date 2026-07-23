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

pub mod budget;
pub mod module_graph;
pub mod ratchet;

use std::sync::Arc;

pub use vix::*;
pub use vixen_primitives::*;

pub use module_graph::{
    DEFAULT_ROOT_MODULE, ModuleFile, ModuleGraph, ModuleGraphError, ModuleRoot, load_module_graph,
};

use vix::compiler::CompilerConfig;
use vix::runtime::{EventSink, PrimitiveDispatcher, PrimitiveRegistry, RawPrimitive, Runtime};
// DecodePrimitive / PinnedFetchPrimitive / TreeReadPrimitive / TypedAdapter come
// from the `pub use vixen_primitives::*` above.

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
    ]
}

/// A dispatcher carrying the builtin primitives — what `vix-core`'s empty
/// dispatcher is replaced with to get a runnable system.
#[must_use]
pub fn default_primitive_dispatcher<Ctx>() -> PrimitiveDispatcher<Ctx> {
    let mut registry = PrimitiveRegistry::default();
    for primitive in builtin_primitives::<Ctx>() {
        registry
            .register(primitive)
            .expect("built-in primitives register once, each under a distinct id");
    }
    PrimitiveDispatcher::new(Arc::new(registry))
}

/// Install the builtin primitives into a runtime constructed by `vix-core` (which
/// ships an empty dispatcher).
pub fn install_builtins<S: EventSink, Ctx>(runtime: &mut Runtime<S, Ctx>) {
    runtime.set_primitive_dispatcher(default_primitive_dispatcher());
}

/// The compiler config for the runnable system: the `vix-core` defaults plus the
/// vixen stdlib prelude. `vix-core`'s own `CompilerConfig::default()` ships an
/// empty prelude (the bare language).
#[must_use]
pub fn default_config() -> CompilerConfig {
    CompilerConfig {
        prelude: vixen_primitives::stdlib::PRELUDE_SOURCES,
        methods: vixen_primitives::DOMAIN_METHODS,
        host_types: vixen_primitives::HOST_TYPES,
        ..CompilerConfig::default()
    }
}
