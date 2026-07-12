//! Checked-call ABI for an aggregate that contains an array. The result retains
//! its structural value-shape when it crosses the hidden array outcome.

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
fn aggregate_array_call_outcome_retains_its_structural_value_shape() {
    let report =
        run_source(SOURCE).expect("aggregate return compiles and runs through the checked ABI");
    assert!(report.passed(), "checks pass: {:?}", report.plain.checks);
    assert!(report.agrees(), "plain and chaos lanes agree");
    assert_eq!(report.plain.checks.len(), 1);
    assert_eq!(report.plain.checks, report.chaos.checks);
    assert_eq!(report.plain.counters.pure_host_calls, 0);
}
