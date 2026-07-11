//! Rodin-readiness lane: canonical Vix rungs 083 (version parse/order) and 084
//! (VersionSet algebra), pulled forward and executed through the production
//! `run_source` path (plain + chaos). This lane is independent of the
//! consecutive canonical prefix — it may go green above it — and it does not
//! renumber or weaken the fixtures: the rung sources are consumed verbatim.
//!
//! `Version` and `VersionSet` are already builtin types in the production
//! binder/module, but the production compiler/runtime has no lowering for the
//! version substrate (`parse_version`, `parse_req`, and the VersionSet algebra),
//! and no string-operation vocabulary (rung 045) that any faithful parse needs.
//! So the rungs currently stop at a precise typed boundary: `parse_version` /
//! `parse_req` do not resolve. These tests pin that exact boundary so it cannot
//! silently drift, and the `#[ignore]`d green targets flip to passing — a
//! one-line change each — the moment the vix-native substrate lands.
//!
//! The substrate this lane demands into existence is deliberately NOT the
//! retiring Machine-path representation: no host/semver call, no raw evaluator,
//! no string/integer kind tags, no parallel release/prerelease columns, no
//! private interner or cache. `Version` is a records-at-offsets value whose
//! structural declaration order reproduces SemVer precedence; `VersionSet` is a
//! single normalized union of half-open intervals over that total order,
//! carrying cargo's prerelease-admission rule (not a release-only approximation).

use vix::diagnostic::{DiagnosticCode, DiagnosticPayload};
use vix::ratchet::{RunError, run_source};

const RUNG_083: &str = include_str!("ratchet/083-version-parse.vix");
const RUNG_084: &str = include_str!("ratchet/084-version-sets.vix");

/// The single unresolved free-function name at which a rung currently stops.
fn unresolved_name(rung: &str) -> String {
    match run_source(rung) {
        Err(RunError::Diagnostics(diagnostics)) => {
            assert_eq!(
                diagnostics.entries.len(),
                1,
                "readiness-lane boundary is a single typed diagnostic, got {:?}",
                diagnostics.entries,
            );
            let entry = &diagnostics.entries[0];
            assert_eq!(
                entry.code,
                DiagnosticCode::UnknownName,
                "readiness-lane boundary is an unresolved name",
            );
            let DiagnosticPayload::Name { name } = &entry.payload else {
                panic!("UnknownName diagnostic carries a Name payload, got {entry:?}");
            };
            name.clone()
        }
        other => panic!("expected a typed UnknownName boundary, got {other:?}"),
    }
}

#[test]
fn rung_083_stops_at_the_parse_version_boundary() {
    assert_eq!(unresolved_name(RUNG_083), "parse_version");
}

#[test]
fn rung_084_stops_at_the_parse_req_boundary() {
    assert_eq!(unresolved_name(RUNG_084), "parse_req");
}

/// Green target for rung 083: the version value substrate parses and orders
/// through the production path. Ignored until the substrate lands; drop the
/// `#[ignore]` to advance the lane.
#[test]
#[ignore = "readiness lane: green once the vix-native version substrate lands"]
fn rung_083_version_parse_runs_through_production_path() {
    let report = run_source(RUNG_083).expect("rung 083 compiles and runs in production");
    assert!(
        report.passed(),
        "rung 083 checks pass: {:?}",
        report.plain.checks
    );
    assert!(report.agrees(), "plain and chaos lanes agree");
    assert_eq!(report.plain.checks.len(), 5);
    assert_eq!(report.plain.checks, report.chaos.checks);
}

/// Green target for rung 084: the VersionSet interval algebra
/// (contains / intersect / is_empty) runs through the production path. Ignored
/// until the substrate lands; drop the `#[ignore]` to advance the lane.
#[test]
#[ignore = "readiness lane: green once the vix-native VersionSet substrate lands"]
fn rung_084_version_sets_runs_through_production_path() {
    let report = run_source(RUNG_084).expect("rung 084 compiles and runs in production");
    assert!(
        report.passed(),
        "rung 084 checks pass: {:?}",
        report.plain.checks
    );
    assert!(report.agrees(), "plain and chaos lanes agree");
    assert_eq!(report.plain.checks.len(), 6);
    assert_eq!(report.plain.checks, report.chaos.checks);
}
