//! Rodin-readiness lane: canonical Vix rungs 083 (version parse/order) and 084
//! (VersionSet algebra), pulled forward and executed through the production
//! `run_source` path (plain + chaos). This lane is independent of the
//! consecutive canonical prefix — it may go green above it — and it does not
//! renumber or weaken the fixtures: the rung sources are consumed verbatim.
//!
//! The substrate is deliberately NOT the retiring Machine-path representation:
//! no host/semver call, no raw evaluator, no string/integer kind tags, no
//! parallel release/prerelease columns, no private interner or cache. `Version`
//! is a records-at-offsets value whose structural declaration order is intended
//! to reproduce SemVer precedence; `VersionSet` will be a single normalized union
//! of half-open intervals over that total order, carrying cargo's
//! prerelease-admission rule (not a release-only approximation).
//!
//! Current typed boundaries (each pinned or ignored below, never silently
//! drifting):
//!   * The raw rung sources stop at `parse_version` / `parse_req` being
//!     unresolved (the production compiler has no ambient prelude), pinned by
//!     the `*_stops_at_the_*_boundary` tests.
//!   * With `std/version.vix` presented ahead of it, rung 083 parses and
//!     constructs `Version` but cannot be *ordered*: `<` on `Version` needs the
//!     production compiler to structurally compare the enum (`PreTag`,
//!     `PreIdent`) and array (`[PreIdent]`) it carries. The equality lowering
//!     already walks these shapes with existing weavy ops; structural comparison
//!     is the same walk with a three-way short-circuit. That is the single
//!     primitive the ignored rung-083 green target waits on.
//!   * Rung 084 additionally needs the vix-native `VersionSet` interval algebra
//!     (`parse_req`, `contains`/`intersect`/`is_empty`) with method dispatch.

use vix::diagnostic::{DiagnosticCode, DiagnosticPayload};
use vix::ratchet::{RunError, run_source};

/// The vix-native version substrate, authored in the production surface as
/// records-at-offsets values over the checked string/array/comparison
/// vocabulary. The production compiler has no ambient prelude yet, so the lane
/// presents the substrate ahead of the rung under test; it is ordinary vix
/// source compiled through the same path, never a host bridge.
const STD_VERSION: &str = include_str!("../std/version.vix");
const RUNG_083: &str = include_str!("ratchet/083-version-parse.vix");
const RUNG_084: &str = include_str!("ratchet/084-version-sets.vix");

fn lane_source(rung: &str) -> String {
    format!("{STD_VERSION}\n{rung}")
}

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
/// through the production path, plain and chaos agreeing. Ignored until
/// structural comparison of enums and arrays lands (the `<` on `Version`);
/// drop the `#[ignore]` to advance the lane.
#[test]
#[ignore = "readiness lane: green once structural comparison of enums/arrays lands"]
fn rung_083_version_parse_runs_through_production_path() {
    let report =
        run_source(&lane_source(RUNG_083)).expect("rung 083 compiles and runs in production");
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
