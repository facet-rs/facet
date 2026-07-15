//! Phase 03 — a registered primitive is callable from vix source through a
//! generalized `where`-call, with typed diagnostics for every mis-call.

use vix::compiler::{Compilation, Compiler, PrimitiveManifest, PrimitiveSignature};
use vix::diagnostic::{DiagnosticCode, Diagnostics};
use vix::vir::{EffectId, Op, RecordField, RecordType, Type};

/// Manifest with one primitive:
/// `probe_version where { text: String, deep: Bool } -> Version { major: Int }`.
fn probe_manifest() -> PrimitiveManifest {
    let mut manifest = PrimitiveManifest::new();
    manifest.insert(
        "probe_version",
        PrimitiveSignature {
            effect: EffectId([7u8; 32]),
            request: Type::Record(RecordType {
                name: "ProbeRequest@0000000000000001".into(),
                fields: vec![
                    RecordField {
                        name: "text".into(),
                        ty: Type::String,
                    },
                    RecordField {
                        name: "deep".into(),
                        ty: Type::Bool,
                    },
                ],
            }),
            response: Type::Record(RecordType {
                name: "Version@0000000000000002".into(),
                fields: vec![RecordField {
                    name: "major".into(),
                    ty: Type::Int,
                }],
            }),
        },
    );
    manifest
}

fn compile(source: &str) -> Result<Compilation, Diagnostics> {
    Compiler::new()
        .with_primitives(probe_manifest())
        .compile(source)
}

/// Wrap a where-call in a minimal compilable test function. Lowering the binding
/// reaches the where-call; on error, compile returns before the yield is checked.
fn program(call: &str) -> String {
    format!("#[test]\nfn t() -> Stream<Check> {{\n    let v = {call};\n    yield expect_eq(v.major, 1);\n}}\n")
}

fn diagnostic_codes(source: &str) -> Vec<DiagnosticCode> {
    match compile(source) {
        Err(diagnostics) => diagnostics.entries.iter().map(|entry| entry.code).collect(),
        Ok(_) => panic!("expected a diagnostic, got a successful compile"),
    }
}

#[test]
fn registered_primitive_lowers_to_an_effect_request() {
    let source = program("probe_version where { text: \"1.2.3\", deep: true }");
    let compilation = compile(&source).expect("registered primitive compiles");
    let found = compilation
        .module
        .functions
        .iter()
        .flat_map(|function| function.nodes.iter())
        .any(|node| matches!(node.op, Op::EffectRequest { primitive } if primitive == EffectId([7u8; 32])));
    assert!(found, "the call lowers to an Op::EffectRequest for the registered primitive");
}

#[test]
fn wrong_field_type_is_a_type_mismatch() {
    let source = program("probe_version where { text: 5, deep: true }");
    assert!(diagnostic_codes(&source).contains(&DiagnosticCode::TypeMismatch));
}

#[test]
fn missing_field_is_a_missing_field() {
    let source = program("probe_version where { text: \"x\" }");
    assert!(diagnostic_codes(&source).contains(&DiagnosticCode::MissingField));
}

#[test]
fn extra_field_is_an_unknown_field() {
    let source = program("probe_version where { text: \"x\", deep: true, extra: 1 }");
    assert!(diagnostic_codes(&source).contains(&DiagnosticCode::UnknownField));
}

#[test]
fn duplicate_field_is_a_duplicate_field() {
    let source = program("probe_version where { text: \"x\", text: \"y\", deep: true }");
    assert!(diagnostic_codes(&source).contains(&DiagnosticCode::DuplicateField));
}

#[test]
fn unregistered_name_is_an_unknown_name() {
    let source = program("not_a_primitive where { text: \"x\" }");
    assert!(diagnostic_codes(&source).contains(&DiagnosticCode::UnknownName));
}
