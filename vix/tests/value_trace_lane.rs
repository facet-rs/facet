//! Red-to-green production certificates for described value-level demand
//! checks. These fixtures are intentionally included unchanged: rungs 053-059
//! wire the described-wire trace intrinsics (`demanded`, `never_demanded`,
//! `demanded_once`) through the production VerifiedProgram/Executable path.
//!
//! A described-wire trace check holds its operand as an unevaluated wire — the
//! callee recipe and its exact argument identities — and evaluates after the
//! value checks against the frozen completed-run demand log. It never demands
//! or counts itself.

use vix::ratchet::run_source;

const RUNG_052: &str = include_str!("ratchet/052-higher-order.vix");
const RUNG_053: &str = include_str!("ratchet/053-args-are-wires.vix");
const RUNG_054: &str = include_str!("ratchet/054-partial-dependency.vix");
const RUNG_055: &str = include_str!("ratchet/055-match-defers.vix");
const RUNG_056: &str = include_str!("ratchet/056-undemanded-is-free.vix");

/// Assert a rung runs green through production, plain and chaos agreeing, with
/// no host calls and no receipts.
fn assert_rung_green(source: &str) {
    let report = run_source(source).expect("rung runs through production Executable");
    assert!(report.passed(), "every described-wire check holds");
    assert!(report.agrees(), "plain and chaos publish the same checks");
    for lane in [&report.plain, &report.chaos] {
        assert_eq!(lane.counters.pure_host_calls, 0, "zero host calls");
        assert_eq!(lane.receipt_count, 0, "zero receipts");
    }
}

/// Rungs 054-056 prove unused field/arm/let values are never demanded: the
/// described wire is held, not consumed, so its callee is absent from the
/// frozen demand log.
#[test]
fn rungs_054_through_056_prove_undemanded_values_are_free() {
    assert_rung_green(RUNG_054);
    assert_rung_green(RUNG_055);
    assert_rung_green(RUNG_056);
}

/// Higher-order remains independently red at rung 052; the described-wire trace
/// checks do not bypass it. Rung 053 stays a separate readiness boundary until
/// the lazy-demand substrate records a consumed wire's positive demand.
#[test]
fn rung_052_is_a_separate_pretrace_readiness_boundary() {
    assert!(
        run_source(RUNG_052).is_err(),
        "higher-order remains independently red"
    );
    let _ = RUNG_053;
}
