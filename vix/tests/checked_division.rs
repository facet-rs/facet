//! Checked Int division: a zero divisor is a typed language failure with a
//! source site, never a wrong value and never a crash. The failure arises only
//! when the division is demanded; an undemanded division is never entered and
//! never fails. Interpreter and native JIT share the semantics and the typed
//! fault (this suite runs on both lanes via `WEAVY_JIT`).

use vix::ratchet::run_source;
use vix::runtime::FailureValue;

/// A demanded `100 / 0` is a typed `DivisionByZero` failure carrying its source
/// site; a demanded `100 / 4` is the ordinary value `25`.
#[test]
fn demanded_division_by_zero_is_a_typed_failure() {
    let ok = run_source(
        r#"
fn risky(n: Int) -> Int { 100 / n }
#[test]
fn t() -> Stream<Check> {
    yield expect_eq(risky(4), 25);
}
"#,
    )
    .expect("non-zero division runs through production");
    assert!(ok.passed(), "100 / 4 == 25");
    assert!(ok.agrees());

    let boom = run_source(
        r#"
fn risky(n: Int) -> Int { 100 / n }
#[test]
fn t() -> Stream<Check> {
    yield expect_eq(risky(0), 999);
}
"#,
    )
    .expect("zero division is a language failure, never a machine crash");
    assert!(!boom.passed(), "a demanded zero divisor fails the check");
    assert!(boom.agrees(), "plain and chaos agree on the typed failure");
    let failure = boom.plain.checks[0]
        .failure
        .clone()
        .expect("the demanded division published a typed failure");
    assert!(
        matches!(failure, FailureValue::DivisionByZero { .. }),
        "zero divisor is DivisionByZero, got {failure:?}",
    );
}

/// Rung 056's negative control: the unused `landmine = 1 / 0` is never demanded,
/// so it never executes and never fails; demanding the same division does
/// produce the typed failure. The division fault is a property of demand, not of
/// description.
#[test]
fn undemanded_division_by_zero_is_free_but_demanding_it_fails() {
    let free =
        run_source(include_str!("ratchet/056-undemanded-is-free.vix")).expect("rung 056 runs");
    assert!(
        free.passed(),
        "the unused landmine never executes or faults"
    );
    assert!(free.agrees());

    let demanded = run_source(
        r#"
fn cheap() -> Int { 1 }
fn landmine() -> Int { 1 / 0 }
#[test]
fn t() -> Stream<Check> {
    yield expect_eq(landmine(), 7);
}
"#,
    )
    .expect("demanding the landmine runs and fails on the language plane");
    assert!(!demanded.passed(), "the demanded landmine fails");
    assert!(demanded.agrees());
    assert!(
        matches!(
            demanded.plain.checks[0].failure,
            Some(FailureValue::DivisionByZero { .. })
        ),
        "demanding 1 / 0 is a typed DivisionByZero failure",
    );
}
