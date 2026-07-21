use std::fs;

use tempfile::tempdir;
use vixen_runtime::module_graph::{ModuleGraphError, load_module_graph};
use vixen_runtime::ratchet::run_source_with_modules;

#[test]
fn loads_direct_vix_children_in_stable_module_order() {
    let directory = tempdir().expect("temporary module directory");
    fs::write(
        directory.path().join("main.vix"),
        r#"
#[test]
fn directory_modules() -> Stream<Check> {
    yield expect_eq(geometry::answer(), 42);
}
"#,
    )
    .expect("write root module");
    fs::write(
        directory.path().join("geometry.vix"),
        "pub fn answer() -> Int { 42 }\n",
    )
    .expect("write library module");
    fs::write(directory.path().join("notes.txt"), "not vix\n").expect("write ignored file");
    fs::create_dir(directory.path().join("nested")).expect("create ignored subdirectory");
    fs::write(
        directory.path().join("nested/hidden.vix"),
        "pub fn hidden() -> Int { 0 }\n",
    )
    .expect("write nested module");

    let graph = load_module_graph(directory.path()).expect("load module graph");
    let names = graph.iter().map(|module| module.name()).collect::<Vec<_>>();
    assert_eq!(names, ["geometry", "main"]);
    assert_eq!(graph.len(), 2);
    assert!(graph.get("hidden").is_none());

    let root = graph.default_root().expect("main module exists");
    assert_eq!(root.name(), "main");
    assert_eq!(root.modules().len(), 1);
    assert_eq!(root.modules()[0].name, "geometry");
    let report = run_source_with_modules(root.source(), root.modules()).expect("module graph runs");
    assert!(report.passed(), "directory-loaded test passes: {report:?}");
    assert!(report.agrees(), "plain and chaos runs agree");
}

#[test]
fn rejects_file_stems_that_are_not_vix_identifiers() {
    let directory = tempdir().expect("temporary module directory");
    let path = directory.path().join("not-valid.vix");
    fs::write(&path, "pub fn value() -> Int { 1 }\n").expect("write invalid module name");

    let error = load_module_graph(directory.path()).expect_err("invalid module name is rejected");
    assert!(matches!(
        error,
        ModuleGraphError::InvalidModuleName { path: invalid } if invalid == path
    ));
}

#[test]
fn reports_an_unreadable_module_directory() {
    let directory = tempdir().expect("temporary parent directory");
    let missing = directory.path().join("missing");

    let error = load_module_graph(&missing).expect_err("missing directory is rejected");
    assert!(matches!(
        error,
        ModuleGraphError::ReadDirectory { path, .. } if path == missing
    ));
}
