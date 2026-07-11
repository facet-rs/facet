//! Rodin-readiness lane: canonical Vix rungs 083 (version parse/order) and 084
//! (VersionSet algebra) pulled forward and run through the production
//! compiler/runtime path. This lane may be green above the consecutive
//! canonical prefix; it does not renumber or weaken the fixtures.

use vix::ratchet::run_source;

const STD_VERSION: &str = include_str!("../std/version.vix");
const RUNG_083: &str = include_str!("ratchet/083-version-parse.vix");
const RUNG_084: &str = include_str!("ratchet/084-version-sets.vix");

/// The production path has no ambient prelude yet, so the lane presents the
/// vix-native version substrate ahead of the rung under test. The substrate is
/// ordinary vix source — records-at-offsets values, not a host bridge.
fn lane_source(rung: &str) -> String {
    format!("{STD_VERSION}\n{rung}")
}

#[test]
fn rung_083_version_parse_runs_through_production_path() {
    let report =
        run_source(&lane_source(RUNG_083)).expect("rung 083 compiles and runs in production");
    assert!(report.passed(), "rung 083 checks pass: {:?}", report.plain.checks);
    assert!(report.agrees(), "plain and chaos lanes agree");
    assert_eq!(report.plain.checks.len(), 5);
    assert_eq!(report.plain.checks, report.chaos.checks);
}

#[test]
fn rung_084_version_sets_runs_through_production_path() {
    let report =
        run_source(&lane_source(RUNG_084)).expect("rung 084 compiles and runs in production");
    assert!(report.passed(), "rung 084 checks pass: {:?}", report.plain.checks);
    assert!(report.agrees(), "plain and chaos lanes agree");
    assert_eq!(report.plain.checks.len(), 6);
    assert_eq!(report.plain.checks, report.chaos.checks);
}
