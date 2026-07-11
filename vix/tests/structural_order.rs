//! Structural total order for enums and arrays, on the production verified path.
//!
//! Enum order is declaration/variant order first (`E::A < E::B`), then
//! lexicographic payload order within equal variants. Array order is
//! lexicographic by element with array length as the exhausted-prefix tie-break
//! (`[1,2] < [1,2,3]`). Both recurse through nested products/enums/arrays and
//! reuse the machine's structural comparison — no host comparator, no
//! handle/hash order.
//!
//! This file starts as a RED CERTIFICATE: it pins the exact typed boundary at
//! which enum and array ordering are currently rejected. When the comparison
//! lowering lands, these assertions flip to exercising the real order.

use vix::ratchet::{RunError, run_source};

fn rejection(source: &str) -> (vix::diagnostic::DiagnosticCode, String) {
    match run_source(source) {
        Err(RunError::Diagnostics(d)) => {
            assert_eq!(d.entries.len(), 1, "one typed boundary, got {:?}", d.entries);
            (d.entries[0].code, d.entries[0].message())
        }
        other => panic!("expected a typed rejection, got {other:?}"),
    }
}

const ENUM_ORDER: &str = r#"
enum E { A, B(Int) }
#[test]
fn t() -> Stream<Check> { yield expect(E::A < E::B(1)); }
"#;

const ARRAY_ORDER: &str = r#"
#[test]
fn t() -> Stream<Check> { yield expect([1, 2] < [1, 3]); }
"#;

#[test]
fn enum_order_is_currently_a_typed_red_boundary() {
    let (code, message) = rejection(ENUM_ORDER);
    assert_eq!(code, vix::diagnostic::DiagnosticCode::LoweringUnsupported);
    assert!(
        message.contains("enum order needs variant-directed typed lowering"),
        "unexpected message: {message}"
    );
}

#[test]
fn array_order_is_currently_a_typed_red_boundary() {
    let (code, _message) = rejection(ARRAY_ORDER);
    assert_eq!(code, vix::diagnostic::DiagnosticCode::TypeMismatch);
}
