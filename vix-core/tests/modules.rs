use vix::compiler::Compiler;
use vix::diagnostic::{DiagnosticCode, DiagnosticPayload};
use vix::modules::ModuleSource;
use vixen_runtime::ratchet::run_source_with_modules;

const GEOMETRY: &str = r#"
pub struct Point { x: Int, y: Int }

pub fn magnitude_sq(p: Point) -> Int { p.x * p.x + p.y * p.y }

fn private_helper(n: Int) -> Int { n }
"#;

#[test]
fn declared_module_supports_qualified_types_construction_and_calls() {
    let source = r#"
mod geometry;

#[test]
fn qualified_module_access() -> Stream<Check> {
    let p: geometry::Point = geometry::Point { x: 3, y: 4 };
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
fn mod_must_name_a_supplied_module() {
    let diagnostics = Compiler::new()
        .compile("mod missing;")
        .expect_err("an absent module is rejected");

    assert_eq!(diagnostics.entries.len(), 1);
    assert_eq!(diagnostics.entries[0].code, DiagnosticCode::UnknownName);
    assert_eq!(
        diagnostics.entries[0].payload,
        DiagnosticPayload::Name {
            name: "missing".to_owned(),
        }
    );
}

#[test]
fn qualified_access_requires_a_mod_declaration() {
    let source = r#"
fn area() -> Int {
    geometry::magnitude_sq(geometry::Point { x: 3, y: 4 })
}
"#;
    let modules = [ModuleSource {
        name: "geometry",
        source: GEOMETRY,
    }];
    let diagnostics = Compiler::new()
        .compile_with_modules(source, &modules)
        .expect_err("a supplied module is not implicitly in scope");

    assert_eq!(diagnostics.entries.len(), 1);
    assert_eq!(diagnostics.entries[0].code, DiagnosticCode::UnknownName);
    assert_eq!(
        diagnostics.entries[0].payload,
        DiagnosticPayload::Name {
            name: "geometry::magnitude_sq".to_owned(),
        }
    );
}

#[test]
fn qualified_access_preserves_module_privacy() {
    let source = r#"
mod geometry;

fn secret() -> Int {
    geometry::private_helper(42)
}
"#;
    let modules = [ModuleSource {
        name: "geometry",
        source: GEOMETRY,
    }];
    let diagnostics = Compiler::new()
        .compile_with_modules(source, &modules)
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
fn qualified_access_rejects_an_unknown_public_item() {
    let source = r#"
mod geometry;

fn missing() -> Int {
    geometry::not_there()
}
"#;
    let modules = [ModuleSource {
        name: "geometry",
        source: GEOMETRY,
    }];
    let diagnostics = Compiler::new()
        .compile_with_modules(source, &modules)
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
fn one_library_module_can_qualify_another() {
    let facade = r#"
mod geometry;

pub fn three_four_five() -> Int {
    geometry::magnitude_sq(geometry::Point { x: 3, y: 4 })
}
"#;
    let source = r#"
mod facade;

#[test]
fn transitive_qualified_access() -> Stream<Check> {
    yield expect_eq(facade::three_four_five(), 25);
}
"#;
    let modules = [
        ModuleSource {
            name: "geometry",
            source: GEOMETRY,
        },
        ModuleSource {
            name: "facade",
            source: facade,
        },
    ];

    let report = run_source_with_modules(source, &modules).expect("module set compiles and runs");
    assert!(
        report.passed(),
        "transitive qualified call passes: {report:?}"
    );
    assert!(report.agrees(), "plain and chaos execution agree");
}
