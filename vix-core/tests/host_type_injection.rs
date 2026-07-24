//! An embedder-injected host type's nominal identity must be a builtin schema
//! name the core reserves (the schema *registration* is the one piece of a host
//! type that still lives in `vix-core`, since it anchors byte-stable identity).
//! A name the batch does not reserve is rejected with a compile diagnostic here,
//! at the injection seam — not a panic deep inside schema hashing when the host
//! type's `schema_ref()` is first computed.

use vix::binding::HostTypeDecl;
use vix::compiler::{Compiler, CompilerConfig};

const UNREGISTERED: &[HostTypeDecl] = &[HostTypeDecl {
    name: "NotARegisteredSchema",
}];

const TREE: &[HostTypeDecl] = &[HostTypeDecl {
    name: vix::binding::TREE,
}];

const PROGRAM: &str = "fn answer() -> Int {\n    1\n}\n";

#[test]
fn an_unregistered_host_type_is_a_diagnostic_not_a_panic() {
    let compiler = Compiler::with_config(CompilerConfig {
        host_types: UNREGISTERED,
        ..CompilerConfig::default()
    });
    let error = compiler
        .compile(PROGRAM)
        .expect_err("an unregistered host type is rejected rather than panicking");
    let error = format!("{error:?}");
    assert!(
        error.contains("NotARegisteredSchema"),
        "the diagnostic names the offending host type: {error}"
    );
}

#[test]
fn a_registered_host_type_compiles() {
    let compiler = Compiler::with_config(CompilerConfig {
        host_types: TREE,
        ..CompilerConfig::default()
    });
    compiler
        .compile(PROGRAM)
        .unwrap_or_else(|diagnostics| panic!("a reserved host type name compiles: {diagnostics:#?}"));
}

const CORE_COLLIDING: &[HostTypeDecl] = &[HostTypeDecl { name: "Blob" }];

#[test]
fn a_core_colliding_host_type_is_rejected() {
    // `Blob` *is* a reserved builtin schema, so it passes the registration
    // check — but the core resolves its own spelling first, so the declaration
    // could never be reached. It is rejected rather than silently shadowed.
    let compiler = Compiler::with_config(CompilerConfig {
        host_types: CORE_COLLIDING,
        ..CompilerConfig::default()
    });
    let error = compiler
        .compile(PROGRAM)
        .expect_err("a core-colliding host type is rejected rather than shadowed");
    let error = format!("{error:?}");
    assert!(
        error.contains("Blob") && error.contains("collides"),
        "the diagnostic names the colliding host type: {error}"
    );
}

/// The `(tree / seg).text()` projection read is domain surface syntax, active
/// only when the embedder declares the `Tree` host type. The bare language
/// ships the machinery but no spelling reaches it.
const TREE_TEXT_PROGRAM: &str = concat!(
    "#[test]\n",
    "fn t() -> Stream<Check> {\n",
    "    let tree = fixture_tree(\"small-crate\");\n",
    "    yield expect(((tree / \"Cargo.toml\").text()).contains(\"[package]\"));\n",
    "}\n",
);

#[test]
fn tree_text_projection_requires_the_declared_tree_host_type() {
    assert!(
        Compiler::new().compile(TREE_TEXT_PROGRAM).is_err(),
        "the bare language does not spell the tree projection read"
    );
    let compiler = Compiler::with_config(CompilerConfig {
        host_types: TREE,
        ..CompilerConfig::default()
    });
    compiler.compile(TREE_TEXT_PROGRAM).unwrap_or_else(|diagnostics| {
        panic!("the projection read is active once `Tree` is declared: {diagnostics:#?}")
    });
}
