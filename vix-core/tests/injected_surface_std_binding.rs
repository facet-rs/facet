//! An embedder-injected primitive surface must resolve through its `std::`
//! qualified path — via `use std::{name}` and via a `std::name(...)` call —
//! the same way a surface bundled into `builtin_primitive_surfaces` does.
//!
//! This is the module-resolution seam: before it existed, module import and
//! qualified-path resolution consulted only the static binding table, so the
//! moment a primitive's surface left `builtin_primitive_surfaces` (the point of
//! pushing the domain out of `vix-core`) its `std::name` spelling stopped
//! resolving even though the *call* lowered fine. The compiler now threads its
//! injected surfaces into module resolution alongside the static table.

use vix::binding::{MethodDecl, MethodOp, ReceiverType};
use vix::compiler::{Compiler, CompilerConfig};
use vix::modules::ModuleSource;
use vix::runtime::{PrimitiveSurface, builtin_primitive_surfaces};

/// `Registry.url(name) -> PinnedUrl` — an embedder-injected method (the domain
/// method declarations left `vix-core` in issue 2520), supplied here so the test
/// programs can build a pinned url argument for the `grab` surface.
const TEST_METHODS: &[MethodDecl] = &[MethodDecl {
    receiver: ReceiverType::Registry,
    name: "url",
    arity: 1,
    op: MethodOp::RegistryUrl,
}];

/// A surface standing in for one an embedder injects rather than bundling into
/// `builtin_primitive_surfaces`: its name (`grab`) is deliberately absent from
/// the static binding table. It reuses `fetch`'s real shape (one value arg of a
/// pinned url, yielding a `Blob`) so a lowered call is well-typed.
fn injected_grab_surface() -> PrimitiveSurface {
    let fetch = builtin_primitive_surfaces()
        .into_iter()
        .next()
        .expect("fetch is a bundled surface");
    PrimitiveSurface {
        surface_name: "grab",
        ..fetch
    }
}

/// A compiler carrying the injected `grab` surface. An (empty) `std` module is
/// supplied so `std::` qualification is in scope, exactly as the vixen runtime
/// installs its `mod std { … }` stdlib.
fn grab_compiler() -> Compiler {
    Compiler::with_config(CompilerConfig {
        methods: TEST_METHODS,
        ..CompilerConfig::default()
    })
    .with_primitive_surfaces([injected_grab_surface()])
}

const STD_MODULE: ModuleSource<'static> = ModuleSource {
    name: "std",
    source: "",
};

#[test]
fn injected_surface_resolves_through_a_qualified_std_call() {
    let program = r#"
fn grab_fixture() -> Blob {
    std::grab(fixture_registry().url("case.crate"))
}
"#;

    grab_compiler()
        .compile_with_modules(program, &[STD_MODULE])
        .unwrap_or_else(|diagnostics| {
            panic!("injected `std::grab` resolves and lowers: {diagnostics:#?}")
        });
}

#[test]
fn injected_surface_can_be_imported_from_std() {
    let program = r#"
import std::{grab};

fn grab_fixture() -> Blob {
    grab(fixture_registry().url("case.crate"))
}
"#;

    grab_compiler()
        .compile_with_modules(program, &[STD_MODULE])
        .unwrap_or_else(|diagnostics| {
            panic!("`use std::{{grab}}` resolves the injected surface: {diagnostics:#?}")
        });
}

#[test]
fn a_non_injected_std_name_is_still_rejected() {
    // The seam widens resolution only for names an injected surface actually
    // projects; an unrelated `std::` leaf remains an unknown name.
    let program = r#"
fn use_missing() -> Blob {
    std::not_a_primitive(fixture_registry().url("case.crate"))
}
"#;

    let error = grab_compiler()
        .compile_with_modules(program, &[STD_MODULE])
        .expect_err("std::not_a_primitive is not a binding");
    let error = format!("{error:?}");
    assert!(
        error.contains("not_a_primitive"),
        "the unknown qualified name is diagnosed: {error}"
    );
}
