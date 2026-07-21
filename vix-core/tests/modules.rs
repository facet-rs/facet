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
