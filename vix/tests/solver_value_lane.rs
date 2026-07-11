//! Solver-value readiness for canonical rungs 085-088.
//!
//! Canonical prefix remains blocked at rung 050. This file reports a separate
//! readiness track: original rungs execute unchanged through `run_source`, in
//! production trace mode, with ordinary Vix fixture values prepended where the
//! canonical source intentionally names a fixture provider.

use vix::diagnostic::{DiagnosticCode, DiagnosticPayload};
use vix::ratchet::{RunError, run_source};

const STD_VERSION: &str = include_str!("../std/version.vix");
const RUNG_085: &str = include_str!("ratchet/085-index-rows.vix");
const RUNG_086: &str = include_str!("ratchet/086-domains.vix");
const RUNG_087: &str = include_str!("ratchet/087-propagate-narrows.vix");
const RUNG_088: &str = include_str!("ratchet/088-propagate-conflicts.vix");
const RUNG_089: &str = include_str!("ratchet/089-mini-solve-trivial.vix");
const RUNG_090: &str = include_str!("ratchet/090-backtracking.vix");
const RUNG_091: &str = include_str!("ratchet/091-exhaustion-is-none.vix");
const RUNG_092: &str = include_str!("ratchet/092-learning-prunes.vix");
const RUNG_093: &str = include_str!("ratchet/093-solve-is-deterministic.vix");

// The rung's `IndexRow.vers: String` is an adapter-only historical surface.
// `fixture_index` parses it only at the rung's `by_key` demand; solver state
// stays in typed Version/VersionSet values. The fixture is literal Vix data,
// not a host index, sparse-index decode, or an alternate Index representation.
const INDEX_FIXTURE: &str = r#"
struct FixtureIndex { libb: Map<Int, IndexRow> }

fn empty_rows() -> Map<Int, IndexRow> { %{} }

fn fixture_index() -> FixtureIndex {
    FixtureIndex {
        libb: %{
            0 => IndexRow { name: "libb", vers: "1.0.0", deps: [], yanked: false },
            1 => IndexRow { name: "libb", vers: "1.5.0", deps: [], yanked: false },
            2 => IndexRow { name: "libb", vers: "2.0.0", deps: [], yanked: false },
        },
    }
}

fn rows(index: FixtureIndex) where { name: String } -> Map<Int, IndexRow> {
    if name == "libb" { index.libb } else { empty_rows() }
}
"#;

// The solver fixture is one typed package universe: rows retain source-aware
// identity, typed Versions, dependency requirements, and policy-bearing fields.
// The canonical rungs retain their historical name-keyed roots and result map;
// the kernel resolves those roots into PackageId-keyed state without making
// name-only identity a universe representation.
const SOLVER_FIXTURE: &str = r#"
struct PackageSource { canonical: String }
struct PackageId { source: PackageSource, name: String }
struct Dependency { package: PackageId, requirement: VersionSet, optional: Bool, cfg: Option<String> }
struct PackageRow { package: PackageId, version: Version, dependencies: [Dependency], features: Map<String, [String]>, yanked: Bool }
struct PackageUniverse { rows: Map<PackageId, [PackageRow]> }

fn registry(name: String) -> PackageId {
    PackageId { source: PackageSource { canonical: "registry:https://index.crates.io" }, name }
}

fn fixture_index() -> PackageUniverse {
    let liba = registry("liba");
    let libb = registry("libb");
    let libc = registry("libc");
    let libd = registry("libd");
    PackageUniverse {
        rows: %{
            liba => [
                PackageRow { package: liba, version: parse_version("1.2.0"), dependencies: [Dependency { package: libb, requirement: parse_req("^1.0"), optional: false, cfg: None }], features: %{}, yanked: false },
                PackageRow { package: liba, version: parse_version("1.3.0"), dependencies: [Dependency { package: libb, requirement: parse_req("^2.0"), optional: false, cfg: None }], features: %{}, yanked: false },
            ],
            libb => [
                PackageRow { package: libb, version: parse_version("1.0.0"), dependencies: [], features: %{}, yanked: false },
                PackageRow { package: libb, version: parse_version("2.0.0"), dependencies: [], features: %{}, yanked: false },
            ],
            libc => [
                PackageRow { package: libc, version: parse_version("1.0.0"), dependencies: [Dependency { package: libb, requirement: parse_req("^1.0"), optional: false, cfg: None }], features: %{}, yanked: false },
            ],
            libd => [
                PackageRow { package: libd, version: parse_version("3.0.0"), dependencies: [Dependency { package: libb, requirement: parse_req("^1.0"), optional: false, cfg: None }], features: %{}, yanked: false },
            ],
        },
    }
}
"#;

fn version_lane(rung: &str) -> String {
    format!("{STD_VERSION}\n{rung}")
}

fn index_lane() -> String {
    format!("{STD_VERSION}\n{INDEX_FIXTURE}\n{RUNG_085}")
}

fn solver_lane(rung: &str) -> String {
    format!("{STD_VERSION}\n{SOLVER_FIXTURE}\n{rung}")
}

fn unknown_name(source: &str) -> String {
    match run_source(source) {
        Err(RunError::Diagnostics(diagnostics)) => {
            assert_eq!(diagnostics.entries.len(), 1, "one red boundary");
            let entry = &diagnostics.entries[0];
            assert_eq!(entry.code, DiagnosticCode::UnknownName);
            let DiagnosticPayload::Name { name } = &entry.payload else {
                panic!("UnknownName carries a name payload: {entry:?}");
            };
            name.clone()
        }
        other => panic!("expected the preserved name boundary, got {other:?}"),
    }
}

fn all_pass(source: &str, checks: usize) {
    let report = run_source(source).expect("source compiles and executes through VerifiedProgram");
    assert!(report.passed(), "checks pass: {:?}", report.plain.checks);
    assert!(report.agrees(), "plain and chaos agree");
    assert_eq!(report.plain.checks.len(), checks);
    assert_eq!(report.plain.checks, report.chaos.checks);
    assert_eq!(report.plain.counters.pure_host_calls, 0);
    assert_eq!(report.plain.receipt_count, 0);
    assert_eq!(report.chaos.counters.pure_host_calls, 0);
    assert_eq!(report.chaos.receipt_count, 0);
}

#[test]
fn unchanged_rung_085_preserves_its_fixture_provider_red_boundary() {
    assert_eq!(unknown_name(RUNG_085), "fixture_index");
}

#[test]
fn unchanged_rungs_086_through_088_preserve_the_version_set_type_red_boundary() {
    for rung in [RUNG_086, RUNG_087, RUNG_088] {
        assert_eq!(unknown_name(rung), "VersionSet");
    }
}

#[test]
fn rung_089_preserves_the_typed_mini_solve_red_boundary() {
    assert_eq!(unknown_name(&solver_lane(RUNG_089)), "mini_solve");
}

#[test]
fn rung_085_index_rows_runs_with_a_typed_fixture_adapter() {
    all_pass(&index_lane(), 2);
}

#[test]
fn rung_086_domains_runs_with_typed_version_sets() {
    all_pass(&version_lane(RUNG_086), 2);
}

#[test]
fn rung_087_immutable_narrowing_runs_with_map_with() {
    all_pass(&version_lane(RUNG_087), 2);
}

#[test]
fn rung_088_conflict_value_runs_with_typed_version_sets() {
    all_pass(&version_lane(RUNG_088), 1);
}

#[test]
fn typed_package_universe_keeps_same_name_sources_distinct() {
    all_pass(
        &version_lane(
            r#"
struct PackageSource { canonical: String }
struct PackageId { source: PackageSource, name: String }
struct Dependency { package: PackageId, requirement: VersionSet, optional: Bool, cfg: Option<String> }
struct PackageRow { package: PackageId, version: Version, dependencies: [Dependency], features: Map<String, [String]>, yanked: Bool }
struct PackageUniverse { rows: Map<PackageId, [PackageRow]> }

#[test]
fn sources_are_domain_identity() -> Stream<Check> {
    let registry = PackageId { source: PackageSource { canonical: "registry:https://index.crates.io" }, name: "same" };
    let git = PackageId { source: PackageSource { canonical: "git:https://example.invalid/same#abc123" }, name: "same" };
    let row = PackageRow { package: registry, version: parse_version("1.0.0"), dependencies: [], features: %{}, yanked: false };
    let universe = PackageUniverse { rows: %{registry => [row], git => []} };
    yield expect(registry != git);
    yield expect_eq(universe.rows.len(), 2);
    yield expect_eq(universe.rows.get(registry).len(), 1);
    yield expect_eq(universe.rows.get(git).len(), 0);
}
"#,
        ),
        4,
    );
}

#[test]
fn sorted_by_key_orders_nested_enum_array_keys_language_wide() {
    all_pass(
        r#"
enum Tag { Before, After([Int]) }
struct Row { key: Tag, name: String }
#[test]
fn t() -> Stream<Check> {
    let rows = [
        Row { key: Tag::After([1, 3]), name: "third" },
        Row { key: Tag::Before, name: "first" },
        Row { key: Tag::After([1, 2]), name: "second" },
        Row { key: Tag::After([1]), name: "prefix" },
        Row { key: Tag::After([1, 2]), name: "second-again" },
    ];
    let sorted = rows.sorted where { order: by_key(|row| row.key) };
    yield expect_eq(sorted.len(), 5);
    yield expect_eq(sorted[0].name, "first");
    yield expect_eq(sorted[1].name, "prefix");
    yield expect_eq(sorted[2].name, "second");
    yield expect_eq(sorted[3].name, "second-again");
    yield expect_eq(sorted[4].name, "third");
}
"#,
        6,
    );
}
