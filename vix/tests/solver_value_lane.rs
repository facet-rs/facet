//! Solver-value readiness for canonical rungs 085-088.
//!
//! Canonical prefix remains blocked at rung 050. This file reports a separate
//! readiness track: original rungs execute unchanged through `run_source`, in
//! production trace mode, with ordinary Vix fixture values prepended where the
//! canonical source intentionally names a fixture provider.

use vix::diagnostic::{DiagnosticCode, DiagnosticPayload};
use vix::ratchet::{RunError, prepare_source, run_source};
use vix::runtime::{EventKind, MemoVerdict, PersistentClaimRejectionReason};

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
const RUNG_094: &str = include_str!("ratchet/094-index-fetched-lazily.vix");
const RUNG_099: &str = include_str!("ratchet/099-warm-restart.vix");

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

// The first native kernel is deliberately a pure Vix value computation. Its
// search state is keyed by full PackageId values; the name-keyed input/output
// is only the unchanged canonical adapter surface. A name collision is not
// collapsed into a domain or result map: it returns None at that adapter seam.
const MINI_SOLVE_KERNEL: &str = r#"
struct SolverState {
    domains: Map<PackageId, VersionSet>,
    selected: Map<PackageId, Version>,
    learned: [DeadRegion],
}

struct DeadRegion { selections: Map<PackageId, Version> }
struct Choice { package: PackageId, candidates: Int }
struct SearchResult { solution: Option<Map<String, Version>>, learned: [DeadRegion] }
enum SolveStep { Pass(SolverState), Conflict(DeadRegion) }

fn empty_state() -> SolverState {
    SolverState { domains: %{}, selected: %{}, learned: [] }
}

fn find_package(universe: PackageUniverse) where { name: String } -> Option<PackageId> {
    find_package_in(universe.rows.keys()) where { name, found: None }
}

fn find_package_in(keys: [PackageId]) where { name: String, found: Option<PackageId> } -> Option<PackageId> {
    match keys.split_last() {
        None => found,
        Some((pkg, rest)) => {
            if pkg.name != name {
                find_package_in(rest) where { name, found }
            } else {
                match found {
                    None => find_package_in(rest) where { name, found: Some(pkg) },
                    Some(_) => None,
                }
            }
        },
    }
}

fn narrow(index: PackageUniverse) where { state: SolverState, package: PackageId, requirement: VersionSet } -> Option<SolverState> {
    if !index.rows.has(package) {
        None
    } else {
        let prior = if state.domains.has(package) { state.domains.get(package) } else { universe() };
        let allowed = prior.intersect(requirement);
        if allowed.is_empty() {
            None
        } else if state.selected.has(package) && !allowed.contains(state.selected.get(package)) {
            None
        } else {
            Some(SolverState { domains: state.domains.with (package, allowed), ..state })
        }
    }
}

fn seed_requirements(universe: PackageUniverse) where { names: [String], requirements: Map<String, VersionSet>, state: SolverState } -> Option<SolverState> {
    match names.split_last() {
        None => Some(state),
        Some((name, rest)) => match find_package(universe) where { name } {
            None => None,
            Some(package) => match narrow(universe) where { state, package, requirement: requirements.get(name) } {
                None => None,
                Some(next) => seed_requirements(universe) where { names: rest, requirements, state: next },
            },
        },
    }
}

fn eligible_rows(universe: PackageUniverse) where { state: SolverState, package: PackageId } -> [PackageRow] {
    eligible_from(universe.rows.get(package)) where { allowed: state.domains.get(package), out: [] }
}

fn eligible_from(rows: [PackageRow]) where { allowed: VersionSet, out: [PackageRow] } -> [PackageRow] {
    match rows.split_last() {
        None => out,
        Some((row, rest)) => {
            let next = if !row.yanked && allowed.contains(row.version) { out + row } else { out };
            eligible_from(rest) where { allowed, out: next }
        },
    }
}

fn higher_version(left: Version) where { right: Version } -> Bool {
    match version_precedence(left) where { right } {
        Ordering::Greater => true,
        Ordering::Less => false,
        Ordering::Equal => (left <=> right) == Ordering::Greater,
    }
}

fn highest_row(rows: [PackageRow]) -> Option<PackageRow> {
    highest_from(rows) where { best: None }
}

fn highest_from(rows: [PackageRow]) where { best: Option<PackageRow> } -> Option<PackageRow> {
    match rows.split_last() {
        None => best,
        Some((row, rest)) => match best {
            None => highest_from(rest) where { best: Some(row) },
            Some(current) => {
                let next = if higher_version(row.version) where { right: current.version } { row } else { current };
                highest_from(rest) where { best: Some(next) }
            },
        },
    }
}

fn without_version(rows: [PackageRow]) where { version: Version, out: [PackageRow] } -> [PackageRow] {
    match rows.split_last() {
        None => out,
        Some((row, rest)) => {
            let next = if row.version == version { out } else { out + row };
            without_version(rest) where { version, out: next }
        },
    }
}

fn better_choice(candidate: Choice) where { best: Choice } -> Bool {
    candidate.candidates < best.candidates
        || (candidate.candidates == best.candidates && (candidate.package <=> best.package) == Ordering::Less)
}

fn choose_undecided(universe: PackageUniverse) where { state: SolverState } -> Option<Choice> {
    choose_from(universe) where { state, packages: state.domains.keys(), best: None }
}

fn choose_from(universe: PackageUniverse) where { state: SolverState, packages: [PackageId], best: Option<Choice> } -> Option<Choice> {
    match packages.split_last() {
        None => best,
        Some((package, rest)) => {
            if state.selected.has(package) {
                choose_from(universe) where { state, packages: rest, best }
            } else {
                let rows = eligible_rows(universe) where { state, package };
                let candidate = Choice { package, candidates: rows.len() };
                match best {
                    None => choose_from(universe) where { state, packages: rest, best: Some(candidate) },
                    Some(current) => {
                        let next = if better_choice(candidate) where { best: current } { candidate } else { current };
                        choose_from(universe) where { state, packages: rest, best: Some(next) }
                    },
                }
            }
        },
    }
}

fn conflict_analysis(state: SolverState) -> DeadRegion {
    DeadRegion { selections: state.selected }
}

fn region_contains(region: DeadRegion) where { state: SolverState } -> Bool {
    region_contains_keys(region.selections.keys()) where { region, state }
}

fn region_contains_keys(packages: [PackageId]) where { region: DeadRegion, state: SolverState } -> Bool {
    match packages.split_last() {
        None => true,
        Some((package, rest)) => state.selected.has(package)
            && state.selected.get(package) == region.selections.get(package)
            && region_contains_keys(rest) where { region, state },
    }
}

fn state_is_learned(learned: [DeadRegion]) where { state: SolverState } -> Bool {
    match learned.split_last() {
        None => false,
        Some((region, rest)) => region_contains(region) where { state } || state_is_learned(rest) where { state },
    }
}

fn remember(state: SolverState) where { region: DeadRegion } -> SolverState {
    SolverState { learned: state.learned + region, ..state }
}

fn apply_dependencies(universe: PackageUniverse) where { state: SolverState, dependencies: [Dependency] } -> SolveStep {
    match dependencies.split_last() {
        None => SolveStep::Pass(state),
        Some((dependency, rest)) => {
            if dependency.optional {
                apply_dependencies(universe) where { state, dependencies: rest }
            } else {
                match narrow(universe) where { state, package: dependency.package, requirement: dependency.requirement } {
                    None => SolveStep::Conflict(conflict_analysis(state)),
                    Some(next) => apply_dependencies(universe) where { state: next, dependencies: rest },
                }
            }
        },
    }
}

fn select_row(universe: PackageUniverse) where { state: SolverState, package: PackageId, row: PackageRow } -> SolveStep {
    let selected = SolverState { selected: state.selected.with (package, row.version), ..state };
    apply_dependencies(universe) where { state: selected, dependencies: row.dependencies }
}

fn selected_result(packages: [PackageId]) where { state: SolverState, out: Map<String, Version> } -> Option<Map<String, Version>> {
    match packages.split_last() {
        None => Some(out),
        Some((package, rest)) => {
            if !state.selected.has(package) {
                selected_result(rest) where { state, out }
            } else if out.has(package.name) {
                None
            } else {
                selected_result(rest) where { state, out: out.with (package.name, state.selected.get(package)) }
            }
        },
    }
}

fn search(universe: PackageUniverse) where { state: SolverState } -> SearchResult {
    match choose_undecided(universe) where { state } {
        None => SearchResult { solution: selected_result(state.domains.keys()) where { state, out: %{} }, learned: state.learned },
        Some(choice) => try_rows(universe) where { state, package: choice.package, rows: eligible_rows(universe) where { state, package: choice.package } },
    }
}

fn try_rows(universe: PackageUniverse) where { state: SolverState, package: PackageId, rows: [PackageRow] } -> SearchResult {
    match highest_row(rows) {
        None => SearchResult { solution: None, learned: state.learned },
        Some(row) => {
            let rest = without_version(rows) where { version: row.version, out: [] };
            let branch = SolverState { selected: state.selected.with (package, row.version), ..state };
            if state_is_learned(state.learned) where { state: branch } {
                try_rows(universe) where { state, package, rows: rest }
            } else {
                match select_row(universe) where { state, package, row } {
                    SolveStep::Conflict(region) => try_rows(universe) where { state: remember(state) where { region }, package, rows: rest },
                    SolveStep::Pass(next) => {
                        let nested = search(universe) where { state: next };
                        match nested.solution {
                            Some(solution) => SearchResult { solution: Some(solution), learned: nested.learned },
                            None => try_rows(universe) where { state: SolverState { learned: nested.learned, ..state }, package, rows: rest },
                        }
                    },
                }
            }
        },
    }
}

fn mini_solve(universe: PackageUniverse) where { requirements: Map<String, VersionSet> } -> Option<Map<String, Version>> {
    match seed_requirements(universe) where { names: requirements.keys(), requirements, state: empty_state() } {
        None => None,
        Some(state) => (search(universe) where { state }).solution,
    }
}
"#;

const LAZY_SOLVER_FIXTURE: &str = r#"
struct PackageSource { canonical: String }
struct PackageId { source: PackageSource, name: String }
struct Dependency { package: PackageId, requirement: VersionSet, optional: Bool, cfg: Option<String> }
struct PackageRow { package: PackageId, version: Version, dependencies: [Dependency], features: Map<String, [String]>, yanked: Bool }
struct PackageUniverse { marker: Int }
struct FixtureWorkspace { marker: Int }

fn registry(name: String) -> PackageId {
    PackageId { source: PackageSource { canonical: "registry:https://index.crates.io" }, name }
}

fn fixture_index() -> PackageUniverse {
    PackageUniverse { marker: 0 }
}

fn fixture_workspace(name: String) -> FixtureWorkspace {
    FixtureWorkspace { marker: 0 }
}

fn requirements(workspace: FixtureWorkspace) -> Map<String, VersionSet> {
    let text = (fixture_tree("kitchen-sink") / "requirements.txt").text();
    if text.contains("libd") {
        %{"liba" => parse_req(">=1.0"), "libd" => parse_req("^3.0")}
    } else {
        %{"liba" => parse_req(">=1.0"), "libc" => parse_req("^1.0")}
    }
}
"#;

const LAZY_MINI_SOLVE_KERNEL: &str = r#"
struct SolverState {
    domains: Map<PackageId, VersionSet>,
    selected: Map<PackageId, Version>,
    learned: [DeadRegion],
}

struct DeadRegion { selections: Map<PackageId, Version> }
struct Choice { package: PackageId, candidates: Int }
struct SearchResult { solution: Option<Map<String, Version>>, learned: [DeadRegion] }
enum SolveStep { Pass(SolverState), Conflict(DeadRegion) }

fn empty_state() -> SolverState {
    SolverState { domains: %{}, selected: %{}, learned: [] }
}

fn package_known(name: String) -> Bool {
    name == "liba" || name == "libb" || name == "libc" || name == "libd"
}

fn find_package(universe: PackageUniverse) where { name: String } -> Option<PackageId> {
    if package_known(name) { Some(registry(name)) } else { None }
}

fn package_rows(universe: PackageUniverse) where { package: PackageId } -> [PackageRow] {
    if package.name == "liba" {
        let text = (fixture_tree("index") / "liba").text();
        let libb = registry("libb");
        let row12 = if text.contains("liba 1.2.0") {
            [PackageRow { package, version: parse_version("1.2.0"), dependencies: [Dependency { package: libb, requirement: parse_req("^1.0"), optional: false, cfg: None }], features: %{}, yanked: false }]
        } else { [] };
        let row13 = if text.contains("liba 1.3.0") {
            [PackageRow { package, version: parse_version("1.3.0"), dependencies: [Dependency { package: libb, requirement: parse_req("^2.0"), optional: false, cfg: None }], features: %{}, yanked: false }]
        } else { [] };
        row12 ++ row13
    } else if package.name == "libb" {
        let text = (fixture_tree("index") / "libb").text();
        let row10 = if text.contains("libb 1.0.0") {
            [PackageRow { package, version: parse_version("1.0.0"), dependencies: [], features: %{}, yanked: false }]
        } else { [] };
        let row20 = if text.contains("libb 2.0.0") {
            [PackageRow { package, version: parse_version("2.0.0"), dependencies: [], features: %{}, yanked: false }]
        } else { [] };
        row10 ++ row20
    } else if package.name == "libc" {
        let text = (fixture_tree("index") / "libc").text();
        let libb = registry("libb");
        if text.contains("libc 1.0.0") {
            [PackageRow { package, version: parse_version("1.0.0"), dependencies: [Dependency { package: libb, requirement: parse_req("^1.0"), optional: false, cfg: None }], features: %{}, yanked: false }]
        } else { [] }
    } else if package.name == "libd" {
        let text = (fixture_tree("index") / "libd").text();
        let libb = registry("libb");
        if text.contains("libd 3.0.0") {
            [PackageRow { package, version: parse_version("3.0.0"), dependencies: [Dependency { package: libb, requirement: parse_req("^1.0"), optional: false, cfg: None }], features: %{}, yanked: false }]
        } else { [] }
    } else {
        []
    }
}

fn narrow(index: PackageUniverse) where { state: SolverState, package: PackageId, requirement: VersionSet } -> Option<SolverState> {
    let prior = if state.domains.has(package) { state.domains.get(package) } else { universe() };
    let allowed = prior.intersect(requirement);
    if allowed.is_empty() {
        None
    } else if state.selected.has(package) && !allowed.contains(state.selected.get(package)) {
        None
    } else {
        Some(SolverState { domains: state.domains.with (package, allowed), ..state })
    }
}

fn seed_requirements(universe: PackageUniverse) where { names: [String], requirements: Map<String, VersionSet>, state: SolverState } -> Option<SolverState> {
    match names.split_last() {
        None => Some(state),
        Some((name, rest)) => match find_package(universe) where { name } {
            None => None,
            Some(package) => match narrow(universe) where { state, package, requirement: requirements.get(name) } {
                None => None,
                Some(next) => seed_requirements(universe) where { names: rest, requirements, state: next },
            },
        },
    }
}

fn eligible_rows(universe: PackageUniverse) where { state: SolverState, package: PackageId } -> [PackageRow] {
    eligible_from(package_rows(universe) where { package }) where { allowed: state.domains.get(package), out: [] }
}

fn eligible_from(rows: [PackageRow]) where { allowed: VersionSet, out: [PackageRow] } -> [PackageRow] {
    match rows.split_last() {
        None => out,
        Some((row, rest)) => {
            let next = if !row.yanked && allowed.contains(row.version) { out + row } else { out };
            eligible_from(rest) where { allowed, out: next }
        },
    }
}

fn higher_version(left: Version) where { right: Version } -> Bool {
    match version_precedence(left) where { right } {
        Ordering::Greater => true,
        Ordering::Less => false,
        Ordering::Equal => (left <=> right) == Ordering::Greater,
    }
}

fn highest_row(rows: [PackageRow]) -> Option<PackageRow> {
    highest_from(rows) where { best: None }
}

fn highest_from(rows: [PackageRow]) where { best: Option<PackageRow> } -> Option<PackageRow> {
    match rows.split_last() {
        None => best,
        Some((row, rest)) => match best {
            None => highest_from(rest) where { best: Some(row) },
            Some(current) => {
                let next = if higher_version(row.version) where { right: current.version } { row } else { current };
                highest_from(rest) where { best: Some(next) }
            },
        },
    }
}

fn without_version(rows: [PackageRow]) where { version: Version, out: [PackageRow] } -> [PackageRow] {
    match rows.split_last() {
        None => out,
        Some((row, rest)) => {
            let next = if row.version == version { out } else { out + row };
            without_version(rest) where { version, out: next }
        },
    }
}

fn better_choice(candidate: Choice) where { best: Choice } -> Bool {
    candidate.candidates < best.candidates
        || (candidate.candidates == best.candidates && (candidate.package <=> best.package) == Ordering::Less)
}

fn choose_undecided(universe: PackageUniverse) where { state: SolverState } -> Option<Choice> {
    choose_from(universe) where { state, packages: state.domains.keys(), best: None }
}

fn choose_from(universe: PackageUniverse) where { state: SolverState, packages: [PackageId], best: Option<Choice> } -> Option<Choice> {
    match packages.split_last() {
        None => best,
        Some((package, rest)) => {
            if state.selected.has(package) {
                choose_from(universe) where { state, packages: rest, best }
            } else {
                let candidate = Choice { package, candidates: eligible_rows(universe) where { state, package }.len() };
                let next = match best {
                    None => Some(candidate),
                    Some(current) => if better_choice(candidate) where { best: current } { Some(candidate) } else { Some(current) },
                };
                choose_from(universe) where { state, packages: rest, best: next }
            }
        },
    }
}

fn finalize_selection(selected: Map<PackageId, Version>) -> Option<Map<String, Version>> {
    finalize_selected(selected.keys()) where { selected, out: %{} }
}

fn finalize_selected(packages: [PackageId]) where { selected: Map<PackageId, Version>, out: Map<String, Version> } -> Option<Map<String, Version>> {
    match packages.split_last() {
        None => Some(out),
        Some((package, rest)) => {
            if out.has(package.name) {
                None
            } else {
                finalize_selected(rest) where { selected, out: out.with(package.name, selected.get(package)) }
            }
        },
    }
}

fn region_matches(region: DeadRegion) where { selected: Map<PackageId, Version> } -> Bool {
    region.selections.keys().all(|package| selected.has(package) && selected.get(package) == region.selections.get(package))
}

fn blocked(state: SolverState) -> Bool {
    state.learned.any(|region| region_matches(region) where { selected: state.selected })
}

fn conflict_region(selected: Map<PackageId, Version>) -> DeadRegion {
    DeadRegion { selections: selected }
}

fn apply_dependencies(universe: PackageUniverse) where { state: SolverState, dependencies: [Dependency] } -> SolveStep {
    match dependencies.split_last() {
        None => if blocked(state) { SolveStep::Conflict(conflict_region(state.selected)) } else { SolveStep::Pass(state) },
        Some((dependency, rest)) => match narrow(universe) where { state, package: dependency.package, requirement: dependency.requirement } {
            None => SolveStep::Conflict(conflict_region(state.selected)),
            Some(next) => apply_dependencies(universe) where { state: next, dependencies: rest },
        },
    }
}

fn select_row(universe: PackageUniverse) where { state: SolverState, package: PackageId, row: PackageRow } -> SolveStep {
    if state.selected.has(package) && state.selected.get(package) != row.version {
        SolveStep::Conflict(conflict_region(state.selected))
    } else {
        let selected = state.selected.with(package, row.version);
        apply_dependencies(universe) where { state: SolverState { selected, ..state }, dependencies: row.dependencies }
    }
}

fn search(universe: PackageUniverse) where { state: SolverState } -> SearchResult {
    match choose_undecided(universe) where { state } {
        None => SearchResult { solution: finalize_selection(state.selected), learned: state.learned },
        Some(choice) => try_rows(universe) where { state, package: choice.package, rows: eligible_rows(universe) where { state, package: choice.package } },
    }
}

fn try_rows(universe: PackageUniverse) where { state: SolverState, package: PackageId, rows: [PackageRow] } -> SearchResult {
    match highest_row(rows) {
        None => SearchResult { solution: None, learned: state.learned + conflict_region(state.selected) },
        Some(row) => {
            let rest = without_version(rows) where { version: row.version, out: [] };
            match select_row(universe) where { state, package, row } {
                SolveStep::Conflict(region) =>
                    try_rows(universe) where { state: SolverState { learned: state.learned + region, ..state }, package, rows: rest },
                SolveStep::Pass(next) => {
                    let nested = search(universe) where { state: next };
                    match nested.solution {
                        Some(solution) => SearchResult { solution: Some(solution), learned: nested.learned },
                        None => try_rows(universe) where { state: SolverState { learned: nested.learned, ..state }, package, rows: rest },
                    }
                },
            }
        },
    }
}

fn mini_solve(universe: PackageUniverse) where { requirements: Map<String, VersionSet> } -> Option<Map<String, Version>> {
    match seed_requirements(universe) where { names: requirements.keys(), requirements, state: empty_state() } {
        None => None,
        Some(state) => (search(universe) where { state }).solution,
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
    format!("{STD_VERSION}\n{SOLVER_FIXTURE}\n{MINI_SOLVE_KERNEL}\n{rung}")
}

fn lazy_solver_lane(rung: &str) -> String {
    format!("{STD_VERSION}\n{LAZY_SOLVER_FIXTURE}\n{LAZY_MINI_SOLVE_KERNEL}\n{rung}")
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
fn rung_089_mini_solve_runs_over_the_typed_package_universe() {
    all_pass(&solver_lane(RUNG_089), 2);
}

#[test]
fn rung_090_mini_solve_backtracks_from_the_old_state() {
    all_pass(&solver_lane(RUNG_090), 1);
}

#[test]
fn rung_091_mini_solve_exhaustion_is_none() {
    all_pass(&solver_lane(RUNG_091), 1);
}

#[test]
fn rung_092_shares_solution_between_generator_control_and_selected_check() {
    let report = run_source(&solver_lane(RUNG_092))
        .expect("rung 092 compiles and exposes its completed-run trace verdict");

    for lane in [&report.plain, &report.chaos] {
        assert_eq!(lane.checks.len(), 2);
        assert!(lane.checks[0].passed, "the selected solution is valid");
        assert!(
            lane.checks[1].trace_failure.is_none(),
            "the unchanged name-level demanded_times check observes one shared solve"
        );
        assert!(
            lane.checks[1].passed,
            "demanded_times(conflict_analysis, 1)"
        );
    }
    assert!(report.passed());
    assert!(report.agrees());
}

#[test]
fn rung_093_solve_is_deterministic() {
    // `demanded_once(solve)` selects the let-bound `mini_solve` invocation — a
    // composite-argument preimage with a where-clause requirement Map — by its
    // canonical preimage in the authored graph. Both demands of `solve` are one
    // computation, one answer; the observer changes nothing about execution.
    let report = run_source(&solver_lane(RUNG_093))
        .expect("rung 093 compiles and executes through VerifiedProgram");
    for lane in [&report.plain, &report.chaos] {
        assert_eq!(lane.checks.len(), 2);
        assert!(
            lane.checks[0].passed,
            "the two solve demands are one answer"
        );
        assert!(
            lane.checks[1].trace_failure.is_none(),
            "demanded_once(solve) observes exactly one realization: {:?}",
            lane.checks[1].trace_failure
        );
        assert!(lane.checks[1].passed, "demanded_once(solve)");
        assert_eq!(lane.counters.pure_host_calls, 0);
        assert_eq!(lane.receipt_count, 0);
    }
    assert!(report.passed());
    assert!(report.agrees());
}

#[test]
fn rung_094_mini_solve_reads_only_visited_package_rows() {
    let report = run_source(&lazy_solver_lane(RUNG_094))
        .expect("rung 094 compiles and executes through effect-backed package rows");
    assert!(report.passed(), "rung 094 report: {report:#?}");
    assert!(report.agrees(), "plain and chaos agree");
    for lane in [&report.plain, &report.chaos] {
        assert_eq!(lane.checks.len(), 2);
        assert!(lane.checks.iter().all(|check| check.passed));
        assert!(
            lane.receipt_count > 0,
            "row access is proven by receipt-bearing effect demands: {lane:#?}",
        );
        assert_eq!(lane.counters.pure_host_calls, 0);
        assert!(
            lane.counters.effect_spawns > 0,
            "package rows are read through the production effect plane"
        );
    }
}

#[test]
fn rung_099_one_req_bumped_recomputes_changed_root_and_reuses_untouched_rows() {
    let report = prepare_source(&lazy_solver_lane(RUNG_099))
        .expect("rung 099 prepares")
        .execute_persistence_audit()
        .expect("rung 099 persistence audit executes");
    assert!(
        report.second.checks.iter().all(|check| check.passed),
        "second run checks pass: {report:#?}",
    );
    assert!(
        report.load.claims_loaded > 0,
        "unchanged receipt-backed claims load only after verification: {report:#?}",
    );
    assert!(
        report
            .load
            .rejected_claims
            .iter()
            .any(|claim| claim.reason == PersistentClaimRejectionReason::UnverifiableReceipt),
        "the bumped workspace requirement rejects the stale root claim: {report:#?}",
    );
    assert!(
        report.second.counters.memo_misses > 0,
        "changed root recomputes as a demanded miss: {report:#?}",
    );
    assert!(
        report.second.counters.memo_hits_exact + report.second.counters.memo_hits_projection > 0,
        "untouched package work is served by memo after receipt verification: {report:#?}",
    );
    assert!(
        report.second.counters.memo_misses < report.first.counters.memo_misses,
        "the rerun is not recompute-and-compare: {report:#?}",
    );
    assert!(!report.nondeterministic, "{report:#?}");
}

#[test]
fn lazy_package_row_claim_rejects_when_relevant_row_changes() {
    let source = lazy_solver_lane(
        r#"
#[test { rerun_with: "liba-row-bumped" }]
fn changed_row() -> Stream<Check> {
    let reqs = %{"liba" => parse_req("^1.0"), "libc" => parse_req("^1.0")};
    let solution = mini_solve(fixture_index()) where { requirements: reqs };
    yield expect_some(solution);
    yield read(p"index/liba");
}
"#,
    );
    let report = prepare_source(&source)
        .expect("changed-row source prepares")
        .execute_persistence_audit()
        .expect("changed-row persistence audit executes");
    assert!(report.second.checks.iter().all(|check| check.passed));
    assert!(
        report
            .load
            .rejected_claims
            .iter()
            .any(|claim| claim.reason == PersistentClaimRejectionReason::UnverifiableReceipt),
        "changed relevant row invalidates persisted claims: {report:#?}",
    );
    assert!(
        report.second.counters.memo_misses > 0,
        "changed relevant row recomputes through the row demand: {report:#?}",
    );
}

#[test]
fn lazy_package_row_claim_loads_as_verified_hit_when_subtree_is_unchanged() {
    let source = lazy_solver_lane(
        r#"
#[test]
fn unchanged_row() -> Stream<Check> {
    let reqs = %{"liba" => parse_req("^1.0"), "libc" => parse_req("^1.0")};
    let solution = mini_solve(fixture_index()) where { requirements: reqs };
    yield expect_some(solution);
}
"#,
    );
    let report = prepare_source(&source)
        .expect("unchanged-row source prepares")
        .execute_persistence_audit()
        .expect("unchanged-row persistence audit executes");
    assert!(report.second.checks.iter().all(|check| check.passed));
    assert!(
        report.load.claims_loaded > 0,
        "unchanged row claims load only after receipt verification: {report:#?}",
    );
    assert_eq!(
        report.load.claims_rejected, 0,
        "unchanged row world rejects no receipt-backed claims: {report:#?}",
    );
    assert_eq!(
        report.second.counters.memo_misses, 0,
        "unchanged subtree is a hit, not recompute-and-compare: {report:#?}",
    );
    let memo_hits = report
        .second
        .events
        .iter()
        .filter(|event| {
            matches!(
                event.kind,
                EventKind::Memo {
                    verdict: MemoVerdict::Exact | MemoVerdict::Projection,
                    ..
                }
            )
        })
        .count();
    assert!(memo_hits > 0, "second run has real memo hits: {report:#?}");
    assert!(!report.nondeterministic, "{report:#?}");
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
