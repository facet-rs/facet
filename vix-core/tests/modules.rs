use vix::compiler::Compiler;
use vix::diagnostic::{DiagnosticCode, DiagnosticPayload};
use vix::modules::ModuleSource;
use vixen_runtime::ratchet::run_source_with_modules;

const GEOMETRY: &str = r#"
pub struct Point { x: Int, y: Int }

pub fn magnitude_sq(p: Point) -> Int { square(p.x) + square(p.y) }

fn square(n: Int) -> Int { n * n }
"#;

#[test]
fn inline_module_supports_qualified_types_construction_and_calls() {
    let source = r#"
mod geometry {
    pub struct Point { x: Int, y: Int }

    pub fn magnitude_sq(p: Point) -> Int {
        square(p.x) + square(p.y)
    }

    fn square(n: Int) -> Int { n * n }
}

#[test]
fn qualified_module_access() -> Stream<Check> {
    let p: geometry::Point = geometry::Point { x: 3, y: 4 };
    yield expect_eq(geometry::magnitude_sq(p), 25);
}
"#;

    let report = run_source_with_modules(source, &[]).expect("inline module compiles and runs");
    assert!(report.passed(), "qualified call passes: {report:?}");
    assert!(report.agrees(), "plain and chaos execution agree");
}

#[test]
fn qualified_functions_support_named_arguments_callbacks_and_private_self_access() {
    let source = r#"
mod arithmetic {
    pub fn add(a: Int) where { b: Int } -> Int { a + b }

    pub fn twice(n: Int) -> Int { n * 2 }

    fn hidden() -> Int { 42 }

    pub fn reveal() -> Int { arithmetic::hidden() }
}

#[test]
fn qualified_function_surfaces() -> Stream<Check> {
    yield expect_eq(arithmetic::add(1) where { b: 2 }, 3);
    yield expect_eq([1, 2].map(arithmetic::twice), [2, 4]);
    yield expect_eq(arithmetic::reveal(), 42);
}
"#;

    let report =
        run_source_with_modules(source, &[]).expect("qualified function forms compile and run");
    assert!(
        report.passed(),
        "qualified function checks pass: {report:?}"
    );
    assert!(report.agrees(), "plain and chaos execution agree");
}

#[test]
fn runtime_std_module_executes_vix_functions() {
    let source = r#"
struct Row { name: String }

#[test]
fn std_module_access() -> Stream<Check> {
    let row: Row = std::json_decode("{\"name\":\"vix\"}");
    yield expect_eq(row.name, "vix");
}
"#;

    let report = run_source_with_modules(source, &[]).expect("std module compiles and runs");
    assert!(report.passed(), "std module checks pass: {report:?}");
    assert!(report.agrees(), "plain and chaos execution agree");
}

#[test]
fn supplied_module_supports_qualified_access_without_an_import() {
    let source = r#"
#[test]
fn qualified_module_access() -> Stream<Check> {
    let p = geometry::Point { x: 3, y: 4 };
    yield expect_eq(geometry::magnitude_sq(p), 25);
}
"#;
    let modules = [ModuleSource {
        name: "geometry",
        source: GEOMETRY,
    }];

    let report = run_source_with_modules(source, &modules).expect("module set compiles and runs");
    assert!(report.passed(), "qualified call passes: {report:?}");
    assert!(report.agrees(), "plain and chaos execution agree");
}

#[test]
fn qualified_access_preserves_inline_module_privacy() {
    let source = r#"
mod geometry {
    fn private_helper(n: Int) -> Int { n }
}

fn secret() -> Int {
    geometry::private_helper(42)
}
"#;
    let diagnostics = Compiler::new()
        .compile(source)
        .expect_err("private items stay private through qualified access");

    assert_eq!(diagnostics.entries.len(), 1);
    assert_eq!(diagnostics.entries[0].code, DiagnosticCode::PrivateImport);
    assert_eq!(
        diagnostics.entries[0].payload,
        DiagnosticPayload::Name {
            name: "geometry::private_helper".to_owned(),
        }
    );
}

#[test]
fn qualified_access_rejects_an_unknown_inline_item() {
    let source = r#"
mod geometry {
    pub fn present() -> Int { 42 }
}

fn missing() -> Int {
    geometry::not_there()
}
"#;
    let diagnostics = Compiler::new()
        .compile(source)
        .expect_err("unknown qualified items are rejected by module resolution");

    assert_eq!(diagnostics.entries.len(), 1);
    assert_eq!(diagnostics.entries[0].code, DiagnosticCode::UnknownName);
    assert_eq!(
        diagnostics.entries[0].payload,
        DiagnosticPayload::Name {
            name: "geometry::not_there".to_owned(),
        }
    );
}

#[test]
fn one_inline_module_can_qualify_another() {
    let source = r#"
mod geometry {
    pub struct Point { x: Int, y: Int }
    pub fn magnitude_sq(p: Point) -> Int { p.x * p.x + p.y * p.y }
}

mod facade {
    pub fn three_four_five() -> Int {
        geometry::magnitude_sq(geometry::Point { x: 3, y: 4 })
    }
}

#[test]
fn transitive_qualified_access() -> Stream<Check> {
    yield expect_eq(facade::three_four_five(), 25);
}
"#;

    let report = run_source_with_modules(source, &[]).expect("inline modules compile and run");
    assert!(
        report.passed(),
        "transitive qualified call passes: {report:?}"
    );
    assert!(report.agrees(), "plain and chaos execution agree");
}

#[test]
fn duplicate_inline_module_names_are_rejected() {
    let source = r#"
mod geometry {}
mod geometry {}
"#;
    let diagnostics = Compiler::new()
        .compile(source)
        .expect_err("duplicate inline module names are rejected");

    assert_eq!(diagnostics.entries.len(), 1);
    assert_eq!(
        diagnostics.entries[0].code,
        DiagnosticCode::DuplicateDefinition
    );
    assert_eq!(
        diagnostics.entries[0].payload,
        DiagnosticPayload::Name {
            name: "geometry".to_owned(),
        }
    );
}

#[test]
fn nested_inline_modules_are_rejected_instead_of_hoisted() {
    let source = r#"
mod outer {
    mod inner {
        pub fn answer() -> Int { 42 }
    }
}

fn root() -> Int { inner::answer() }
"#;
    let diagnostics = Compiler::new()
        .compile(source)
        .expect_err("nested inline modules are not silently hoisted");

    assert_eq!(diagnostics.entries.len(), 1);
    assert_eq!(
        diagnostics.entries[0].code,
        DiagnosticCode::UnsupportedExpression
    );
    assert_eq!(
        diagnostics.entries[0].payload,
        DiagnosticPayload::Unsupported {
            construct: "nested inline modules are not supported".to_owned(),
        }
    );
}
