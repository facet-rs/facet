//! Red certificate for the checked-call ABI of an aggregate that contains an
//! array. The result must retain its structural value-shape when it crosses the
//! hidden array outcome, rather than being treated as a bare handle word.

use vix::ratchet::run_source;

const SOURCE: &str = r#"
struct Boxed { values: [Int] }
fn boxed() -> Boxed { Boxed { values: [1, 2, 3] } }
#[test]
fn t() -> Stream<Check> {
    yield expect_eq(boxed().values.len(), 3);
}
"#;

#[test]
fn aggregate_array_call_outcome_is_an_exact_red_boundary() {
    let error = run_source(SOURCE).expect_err("aggregate return currently reaches the typed ABI bug");
    assert!(
        format!("{error:?}").contains("StructuralFieldSourceMismatch"),
        "expected the checked-call structural-field boundary, got {error:?}"
    );
}
