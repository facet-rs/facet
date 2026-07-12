//! Red-to-green production certificates for described value-level demand
//! checks. These fixtures are intentionally included unchanged: rungs 053-059
//! wire the described-wire trace intrinsics (`demanded`, `never_demanded`,
//! `demanded_once`) through the production VerifiedProgram/Executable path.
//!
//! A described-wire trace check holds its operand as an unevaluated wire — the
//! callee identity and its exact scalar argument identities — and evaluates
//! after the value checks against the frozen completed-run demand log. It never
//! demands or counts itself. A consumed wire is demanded through the scheduler's
//! DemandPreimage + Location memo path: the awaiting task parks, the runtime
//! evaluates and memoizes the invocation, and the task resumes with the typed
//! scalar result. An unconsumed wire issues no demand.

use vix::ratchet::run_source;

const RUNG_052: &str = include_str!("ratchet/052-higher-order.vix");
const RUNG_053: &str = include_str!("ratchet/053-args-are-wires.vix");
const RUNG_054: &str = include_str!("ratchet/054-partial-dependency.vix");
const RUNG_055: &str = include_str!("ratchet/055-match-defers.vix");
const RUNG_056: &str = include_str!("ratchet/056-undemanded-is-free.vix");
const RUNG_057: &str = include_str!("ratchet/057-element-independence.vix");
const RUNG_058: &str = include_str!("ratchet/058-memo-within-run.vix");
const RUNG_059: &str = include_str!("ratchet/059-distinct-args-distinct-demands.vix");

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

/// Rung 053 proves a function's arguments are wires: `pick(true)` demands only
/// the consumed argument (`cheap`) and never the unconsumed one (`expensive`).
#[test]
fn rung_053_proves_arguments_are_wires() {
    assert_rung_green(RUNG_053);
}

/// Rungs 054-056 prove unused field/arm/let values issue no demand: the
/// described wire is held, not consumed, so its callee is absent from the frozen
/// demand log.
#[test]
fn rungs_054_through_056_prove_undemanded_values_are_free() {
    assert_rung_green(RUNG_054);
    assert_rung_green(RUNG_055);
    assert_rung_green(RUNG_056);
}

/// Rung 057 proves Array.map is keyed lazy codata at the element boundary:
/// projecting `ys[1]` from `[2, 3, 4].map(slow_square)` demands only
/// `slow_square(3)` — one keyed application, never an eager whole-array map.
#[test]
fn rung_057_proves_array_map_element_independence() {
    assert_rung_green(RUNG_057);
}

/// Rung 058 proves same recipe+argument memoizes to one computation:
/// `costly(7)` aliased twice executes once. Rung 059 proves distinct arguments
/// stay distinct: `costly(1)` and `costly(2)` are separate demands.
#[test]
fn rungs_058_059_prove_wire_memoization_and_distinct_arguments() {
    assert_rung_green(RUNG_058);
    assert_rung_green(RUNG_059);
}

/// Higher-order remains independently red at rung 052; the described-wire trace
/// checks do not bypass it, and the described-wire rungs 053-059 never use it.
#[test]
fn rung_052_remains_a_separate_pretrace_boundary() {
    assert!(
        run_source(RUNG_052).is_err(),
        "higher-order remains independently red"
    );
}
