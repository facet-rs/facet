//! Red-to-green production certificates for described value-level demand
//! checks. These fixtures are intentionally included unchanged.

use vix::diagnostic::{DiagnosticCode, DiagnosticPayload};
use vix::ratchet::{RunError, run_source};

const RUNG_052: &str = include_str!("ratchet/052-higher-order.vix");
const RUNG_053: &str = include_str!("ratchet/053-args-are-wires.vix");
const RUNG_054: &str = include_str!("ratchet/054-partial-dependency.vix");
const RUNG_055: &str = include_str!("ratchet/055-match-defers.vix");
const RUNG_056: &str = include_str!("ratchet/056-undemanded-is-free.vix");
const RUNG_057: &str = include_str!("ratchet/057-element-independence.vix");
const RUNG_058: &str = include_str!("ratchet/058-memo-within-run.vix");
const RUNG_059: &str = include_str!("ratchet/059-distinct-args-distinct-demands.vix");

fn unknown_check(source: &str) -> String {
    let Err(RunError::Diagnostics(diagnostics)) = run_source(source) else {
        panic!("the absent trace constructor remains the red boundary");
    };
    assert_eq!(diagnostics.entries.len(), 1, "one source boundary");
    let entry = &diagnostics.entries[0];
    assert_eq!(entry.code, DiagnosticCode::UnknownName);
    let DiagnosticPayload::Name { name } = &entry.payload else {
        panic!("unknown trace constructor has a typed name payload: {entry:?}");
    };
    name.clone()
}

#[test]
fn rungs_053_through_059_preserve_described_wire_red_boundaries() {
    for (source, constructor) in [
        (RUNG_053, "demanded"),
        (RUNG_054, "never_demanded"),
        (RUNG_055, "never_demanded"),
        (RUNG_056, "never_demanded"),
        (RUNG_057, "demanded_once"),
        (RUNG_058, "demanded_once"),
        (RUNG_059, "demanded_once"),
    ] {
        assert_eq!(unknown_check(source), constructor);
    }
}

#[test]
fn rung_052_is_a_separate_pretrace_readiness_boundary() {
    assert!(
        run_source(RUNG_052).is_err(),
        "higher-order remains independently red; described trace checks do not bypass it"
    );
}
