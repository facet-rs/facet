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
