//! `vx r` end to end — the MVP acceptance of `vixen.package.run-mvp`, driven
//! through [`vixen_runtime::package::prepare_command`], the same seam the CLI
//! uses, over the SHIPPED demo package (`cargo-vix/hello-pkg/`), so the
//! example and the proof are one source.

#![cfg(unix)]

use std::path::{Path, PathBuf};

use vixen_runtime::package::{PackageError, prepare_command};

fn hello_pkg() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../cargo-vix/hello-pkg")
}

/// `PreparedCommand` holds a compiled run and is deliberately not `Debug`,
/// so `expect_err` is unavailable; this is its refusal-side twin.
fn expect_refusal(directory: &Path, command: &str) -> PackageError {
    match prepare_command(directory, command) {
        Err(error) => error,
        Ok(_) => panic!("expected `{command}` to be refused"),
    }
}

#[test]
fn a_package_command_runs_exactly_its_named_root() {
    let prepared = prepare_command(&hello_pkg(), "greet").expect("greet resolves and prepares");
    assert_eq!(prepared.package, "hello");
    assert_eq!(prepared.fn_ref, "hello::greet");

    let report = prepared.run.execute().expect("greet runs");
    assert!(report.passed(), "greet passes: {:?}", report.plain.checks);
    assert!(report.agrees(), "lanes agree");
    // ONE check and ONE spawn: `farewell` (a second declared root in the same
    // file) did not run — selection is by name, not by file.
    assert_eq!(report.plain.checks.len(), 1, "only greet's check ran");
    assert_eq!(
        report.plain.counters.effect_spawns, 1,
        "only greet's process spawned"
    );
}

#[test]
fn an_unknown_command_is_refused_with_the_declared_list() {
    let error = expect_refusal(&hello_pkg(), "bulid");
    let PackageError::UnknownCommand {
        package,
        command,
        declared,
    } = &error
    else {
        panic!("expected UnknownCommand, got {error}");
    };
    assert_eq!(package, "hello");
    assert_eq!(command, "bulid");
    assert_eq!(declared, &["build".to_owned(), "greet".to_owned()]);
}

#[test]
fn a_fn_without_root_standing_is_refused_by_name() {
    // `build` exists, compiles, and returns Tree — exactly the shape the real
    // facet package wants — and the MVP refuses it with the deferral's name
    // rather than running or silently skipping it.
    let error = expect_refusal(&hello_pkg(), "build");
    let PackageError::MissingRootStanding {
        command,
        fn_ref,
        declared_roots,
    } = &error
    else {
        panic!("expected MissingRootStanding, got {error}");
    };
    assert_eq!(command, "build");
    assert_eq!(fn_ref, "hello::build");
    assert_eq!(declared_roots, &["greet".to_owned(), "farewell".to_owned()]);
}

#[test]
fn a_directory_without_a_manifest_is_not_a_package() {
    let directory = tempfile::tempdir().expect("tempdir");
    let error = expect_refusal(directory.path(), "greet");
    assert!(
        matches!(error, PackageError::ManifestMissing { .. }),
        "expected ManifestMissing, got {error}"
    );
}

#[test]
fn a_dangling_fn_ref_names_the_missing_module() {
    let directory = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        directory.path().join("package.styx"),
        "package {\n  name broken\n  version \"0.0.1\"\n}\ncommand {\n  go { fn nowhere::go }\n}\n",
    )
    .expect("write manifest");
    let error = expect_refusal(directory.path(), "go");
    let PackageError::ModuleMissing { module, .. } = &error else {
        panic!("expected ModuleMissing, got {error}");
    };
    assert_eq!(module, "nowhere");
}
