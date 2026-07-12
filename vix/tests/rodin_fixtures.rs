//! The rodin fixture corpus: one isolated cargo-vs-rodin.vix differential per
//! resolver behavior. Each fixture is a tiny offline path-dependency Cargo
//! workspace describing exactly one behavior; it is materialized on disk and fed
//! to two consumers:
//!
//!   * cargo — the oracle. `cargo tree -e normal,build --target <triple>` gives
//!     the per-target dependency graph cargo would build.
//!   * rodin.vix — the system under test (SUT). The identical on-disk workspace
//!     is resolved by the vix implementation.
//!
//! A fixture check asserts rodin.vix's per-target selection equals cargo's.
//!
//! Ported from vixenware `rodin-fixtures`, retargeted from the Rust reference
//! resolver (`rodin-core`) to rodin.vix. The cargo-oracle side works today; the
//! `vix_selected` SUT seam runs the current WIP `rodin.vix` as rodin grows
//! (rodin/docs/40-search). The behaviors these differentials pin are catalogued
//! in rodin/docs/05-fixture-corpus.md.
//!
//! ## The live-Cargo oracle lane (typed, serde-free)
//!
//! Below the differential corpus, this file also carries an *independent*
//! live-Cargo oracle harness — the one true oracle is cargo, not the deleted
//! Rust resolver and not any recorded expected-selection (rodin/docs/00-oracle,
//! rodin/PLAN.md). It reuses the fixture DSL (`Fixture`/`materialize`) and reads
//! both cargo oracles from `cargo metadata` (parsed via `facet-json`, no serde),
//! Cargo's own machine surface — chosen over parsing `cargo tree`/`Cargo.lock`
//! display text precisely because metadata exposes the *exact, unambiguous*
//! package id, the typed source (with a path coordinate for path deps), the per-
//! edge dependency kind, and `--filter-platform` cfg projection:
//!
//!   * Oracle 1 — version selection (the lockfile): `cargo metadata`'s resolved
//!     package closure, one exact [`CargoPackageId`] per locked package. This is
//!     the same selection `cargo generate-lockfile` writes to `Cargo.lock`.
//!   * Oracle 2 — the target-projected enabled graph: `cargo metadata
//!     --filter-platform <triple>`, walked from the root over `normal`/`build`
//!     edges (dev excluded), yielding typed [`GraphEdge`]s that keep both exact
//!     endpoints and the edge [`DepKind`]. This is the same projection `cargo
//!     tree -e normal,build --target <triple>` shows.
//!
//! The exact Cargo package identity `(source, name, version)` is kept *separate*
//! from Rodin's [`ResolutionDomain`] `(source, name, compat-class)`: the former
//! is what Cargo actually locked (never collapsed), the latter the coexistence
//! bucket a version competes in. Rodin's domain is a *model* of cargo's own
//! version bucketing, not a proven identity with it, so multiple exact packages
//! projecting to one domain is a valid Cargo case Rodin's single-version-per-
//! domain model does not represent — surfaced as a typed
//! [`Discrepancy::DomainMultiplicity`], never a silent last-wins collapse and
//! never asserted impossible. An unrecognized Cargo source scheme is a typed
//! parse error, never a silent registry.
//!
//! The native Vix kernel emits a typed `SolveResult` with typed `Version` values.
//! The harness adapter below projects Cargo's two metadata oracles into typed Vix
//! values and compares them inside the production run, while the Cargo-facing
//! [`SolveResult`] comparator retains Cargo's textual version spelling for
//! minimizable structured discrepancy reports. No recorded selection is an
//! authority on either path.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use facet::Facet;
use semver::Version as SemVer;
use vix::compiler::Compiler;
use vix::machine::{DriveEvent, Machine, MachineArg, NamedArg, RenderedValue};
use vix::ratchet::{prepare_source, prepare_source_with_lane, run_source};
use vix::runtime::EventKind;
use weavy::exec::LaneRequest;

const LINUX: &str = "x86_64-unknown-linux-gnu";
const WINDOWS: &str = "x86_64-pc-windows-msvc";
const STD_VERSION: &str = include_str!("../std/version.vix");
const NATIVE_RODIN_KERNEL: &str = include_str!("../../rodin/kernel.vix");

/// Which dependency table a dependency lives in.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DepKind {
    Normal,
    Build,
    Dev,
}

impl DepKind {
    fn cargo_table(self) -> &'static str {
        match self {
            Self::Normal => "dependencies",
            Self::Build => "build-dependencies",
            Self::Dev => "dev-dependencies",
        }
    }
}

/// A single dependency edge in a fixture crate.
#[derive(Clone, Debug)]
struct FixtureDep {
    name: String,
    req: String,
    kind: DepKind,
    optional: bool,
    default_features: bool,
    features: Vec<String>,
    /// `cfg(...)` expression or explicit target triple; `None` = unconditional.
    target: Option<String>,
}

impl FixtureDep {
    fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            req: "0.1.0".to_owned(),
            kind: DepKind::Normal,
            optional: false,
            default_features: true,
            features: Vec::new(),
            target: None,
        }
    }

    fn kind(mut self, kind: DepKind) -> Self {
        self.kind = kind;
        self
    }

    fn req(mut self, req: impl Into<String>) -> Self {
        self.req = req.into();
        self
    }

    fn optional(mut self) -> Self {
        self.optional = true;
        self
    }

    fn default_features(mut self, enabled: bool) -> Self {
        self.default_features = enabled;
        self
    }

    fn feature(mut self, feature: impl Into<String>) -> Self {
        self.features.push(feature.into());
        self
    }

    fn target(mut self, target: impl Into<String>) -> Self {
        self.target = Some(target.into());
        self
    }
}

/// A crate in a fixture workspace.
#[derive(Clone, Debug)]
struct FixtureCrate {
    name: String,
    version: String,
    vix_versions: Vec<String>,
    vix_version_deps: BTreeMap<String, Vec<FixtureDep>>,
    features: BTreeMap<String, Vec<String>>,
    deps: Vec<FixtureDep>,
}

impl FixtureCrate {
    fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: "0.1.0".to_owned(),
            vix_versions: vec!["0.1.0".to_owned()],
            vix_version_deps: BTreeMap::new(),
            features: BTreeMap::new(),
            deps: Vec::new(),
        }
    }

    fn version(mut self, version: impl Into<String>) -> Self {
        let version = version.into();
        self.version = version.clone();
        if self.vix_versions == ["0.1.0"] && version != "0.1.0" {
            self.vix_versions = vec![version];
        } else if !self.vix_versions.contains(&version) {
            self.vix_versions.push(version);
        }
        self
    }

    fn vix_version(mut self, version: impl Into<String>) -> Self {
        let version = version.into();
        if !self.vix_versions.contains(&version) {
            self.vix_versions.push(version);
        }
        self
    }

    fn feature(mut self, name: impl Into<String>, enables: &[&str]) -> Self {
        self.features.insert(
            name.into(),
            enables.iter().map(|s| (*s).to_owned()).collect(),
        );
        self
    }

    fn dep(mut self, dep: FixtureDep) -> Self {
        self.deps.push(dep);
        self
    }

    fn vix_version_dep(mut self, version: impl Into<String>, dep: FixtureDep) -> Self {
        let version = version.into();
        if !self.vix_versions.contains(&version) {
            self.vix_versions.push(version.clone());
        }
        self.vix_version_deps.entry(version).or_default().push(dep);
        self
    }
}

/// A whole fixture: a set of crates, the root to build, and its features.
#[derive(Clone, Debug)]
struct Fixture {
    name: String,
    crates: Vec<FixtureCrate>,
    root: String,
}

impl Fixture {
    fn new(name: impl Into<String>, root: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            crates: Vec::new(),
            root: root.into(),
        }
    }

    fn krate(mut self, krate: FixtureCrate) -> Self {
        self.crates.push(krate);
        self
    }

    /// Assert rodin.vix's per-target selection equals cargo's. Returns the shared
    /// selection on success; on mismatch, an error naming the symmetric diff.
    fn assert_selection_matches(&self, triple: &str) -> Result<BTreeSet<String>, String> {
        let workspace = self.materialize()?;
        let cargo = cargo_selected(&workspace, &self.root, triple)?;
        let vix = self.vix_selected(triple)?;
        if cargo != vix {
            let only_cargo: Vec<_> = cargo.difference(&vix).cloned().collect();
            let only_vix: Vec<_> = vix.difference(&cargo).cloned().collect();
            return Err(format!(
                "fixture `{}` selection diverged for {triple}:\n  \
                 cargo-only (rodin.vix missed): {only_cargo:?}\n  \
                 rodin.vix-only (over-selected): {only_vix:?}",
                self.name
            ));
        }
        Ok(cargo)
    }

    /// The system under test: rodin.vix's per-target selection for this fixture.
    fn vix_selected(&self, triple: &str) -> Result<BTreeSet<String>, String> {
        let source = format!("{}\n\n{}", rodin_source()?, self.vix_fixture_source());
        let mut machine = Machine::load(&source)?;
        machine.set_force_molten_copy(true);
        let selected = machine
            .call(
                "fixture_selected",
                &[NamedArg {
                    name: "target".to_owned(),
                    value: MachineArg::String(triple.to_owned()),
                }],
            )
            .map_err(|err| format!("call fixture_selected: {err}\n{}", trace_tail(&machine)))?;
        let rendered = machine
            .render_result("fixture_selected", selected.0)
            .map_err(|err| format!("render fixture_selected: {err}"))?;
        let selected = rendered_name_set(rendered)?;
        if selected.is_empty() {
            return Err(format!(
                "rodin.vix selected no packages\n{}",
                trace_tail(&machine)
            ));
        }
        Ok(selected)
    }

    fn vix_learned_count(&self, triple: &str) -> Result<i64, String> {
        let source = format!("{}\n\n{}", rodin_source()?, self.vix_fixture_source());
        let mut machine = Machine::load(&source)?;
        machine.set_force_molten_copy(true);
        let learned = machine
            .call(
                "fixture_learned_count",
                &[NamedArg {
                    name: "target".to_owned(),
                    value: MachineArg::String(triple.to_owned()),
                }],
            )
            .map_err(|err| {
                format!(
                    "call fixture_learned_count: {err}\n{}",
                    trace_tail(&machine)
                )
            })?;
        let RenderedValue::Int { value } = machine
            .render_result("fixture_learned_count", learned.0)
            .map_err(|err| format!("render fixture_learned_count: {err}"))?
        else {
            return Err("fixture_learned_count did not render as Int".to_owned());
        };
        Ok(value)
    }

    fn rodin_trace_counts(
        &self,
        triple: &str,
        force_tail_invoke: bool,
    ) -> Result<TraceCounts, String> {
        let source = format!("{}\n\n{}", rodin_source()?, self.vix_fixture_source());
        let mut machine = Machine::load(&source)?;
        machine.set_force_molten_copy(true);
        if force_tail_invoke {
            machine
                .set_force_tail_invoke(true)
                .map_err(|err| format!("force tail invoke: {err}"))?;
        }
        machine
            .call(
                "fixture_selected",
                &[NamedArg {
                    name: "target".to_owned(),
                    value: MachineArg::String(triple.to_owned()),
                }],
            )
            .map_err(|err| format!("call fixture_selected: {err}\n{}", trace_tail(&machine)))?;
        trace_counts(
            &machine,
            &[
                "propagate",
                "filter_allowed",
                "candidates_from_rows",
                "force_singletons_over",
                "selected_from_state",
            ],
        )
    }

    /// Write the fixture as a real path-dependency Cargo workspace in a fresh
    /// temp directory; return the workspace root.
    fn materialize(&self) -> Result<PathBuf, String> {
        let root = unique_temp_dir(&self.name);
        std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        let mut members = String::new();
        for krate in &self.crates {
            writeln!(members, "    \"{}\",", krate.name).ok();
        }
        let workspace_toml = format!("[workspace]\nresolver = \"2\"\nmembers = [\n{members}]\n");
        std::fs::write(root.join("Cargo.toml"), workspace_toml).map_err(|e| e.to_string())?;

        for krate in &self.crates {
            let dir = root.join(&krate.name);
            std::fs::create_dir_all(dir.join("src")).map_err(|e| e.to_string())?;
            std::fs::write(dir.join("src").join("lib.rs"), "").map_err(|e| e.to_string())?;
            std::fs::write(dir.join("Cargo.toml"), self.crate_manifest(krate))
                .map_err(|e| e.to_string())?;
        }
        Ok(root)
    }

    fn crate_manifest(&self, krate: &FixtureCrate) -> String {
        let mut toml = String::new();
        writeln!(toml, "[package]").ok();
        writeln!(toml, "name = \"{}\"", krate.name).ok();
        writeln!(toml, "version = \"{}\"", krate.version).ok();
        writeln!(toml, "edition = \"2021\"").ok();

        if !krate.features.is_empty() {
            writeln!(toml, "\n[features]").ok();
            for (feature, enables) in &krate.features {
                let list = enables
                    .iter()
                    .map(|e| format!("\"{e}\""))
                    .collect::<Vec<_>>()
                    .join(", ");
                writeln!(toml, "{feature} = [{list}]").ok();
            }
        }

        // Group deps by (target, kind) into the correct cargo table.
        let mut tables: BTreeMap<(Option<String>, &'static str), Vec<&FixtureDep>> =
            BTreeMap::new();
        for dep in &krate.deps {
            tables
                .entry((dep.target.clone(), dep.kind.cargo_table()))
                .or_default()
                .push(dep);
        }
        for ((target, table), deps) in &tables {
            let header = match target {
                None => format!("[{table}]"),
                Some(cfg) => format!("[target.'{cfg}'.{table}]"),
            };
            writeln!(toml, "\n{header}").ok();
            for dep in deps {
                writeln!(toml, "{}", dep_line(dep)).ok();
            }
        }
        toml
    }

    fn vix_fixture_source(&self) -> String {
        let mut source = String::new();
        let package_indices = self.package_indices();
        let feature_indices = self.feature_indices();
        let version_rows = self.vix_version_rows();

        writeln!(source, "use vix::{{Version, VersionSet, Map}};").ok();

        for krate in &self.crates {
            writeln!(
                source,
                "fn {}() -> PackageId {{ stored_package(PackageId {{ source: Source::Path({}), name: {}, compat: Some(CompatClass::Minor(1)) }}) }}",
                pkg_fn(&krate.name),
                vix_string(&krate.name),
                vix_string(&krate.name),
            )
            .ok();
        }

        writeln!(source, "\nfn fixture_index() -> Index {{").ok();
        writeln!(source, "    let names: Map<Int, String> = {{}};").ok();
        for krate in &self.crates {
            let pkg_id = package_indices[&krate.name];
            writeln!(
                source,
                "    let names = names.insert({pkg_id}, {});",
                vix_string(&krate.name)
            )
            .ok();
        }
        writeln!(source, "    let version_pkgs: Map<Int, Int> = {{}};").ok();
        writeln!(source, "    let version_values: Map<Int, String> = {{}};").ok();
        for (version_id, (krate_name, version)) in version_rows.iter().enumerate() {
            let pkg_id = package_indices[krate_name];
            writeln!(
                source,
                "    let version_pkgs = version_pkgs.insert({version_id}, {pkg_id});"
            )
            .ok();
            writeln!(
                source,
                "    let version_values = version_values.insert({version_id}, {});",
                vix_string(version)
            )
            .ok();
        }
        let version_ids = (0..version_rows.len())
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let packages = self
            .crates
            .iter()
            .map(|krate| package_indices[&krate.name].to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let clauses = self.vix_clauses();
        for declaration in [
            "guard_clause_ids: Map<Int, Int>",
            "guard_tags: Map<Int, String>",
            "guard_kinds: Map<Int, Int>",
            "guard_pkgs: Map<Int, Int>",
            "guard_version_values: Map<Int, String>",
            "guard_features: Map<Int, Int>",
            "consequent_tags: Map<Int, String>",
            "consequent_pkgs: Map<Int, Int>",
            "consequent_version_sets: Map<Int, VersionSet>",
            "consequent_features: Map<Int, Int>",
            "gate_kinds: Map<Int, String>",
            "gate_targets: Map<Int, String>",
        ] {
            writeln!(source, "    let {declaration} = {{}};").ok();
        }
        for insert in &clauses.inserts {
            writeln!(source, "    {insert}").ok();
        }
        writeln!(
            source,
            "    Index {{ packages: [{packages}], names: names, version_ids: [{version_ids}], version_pkgs: version_pkgs, version_values: version_values, clause_ids: [{}], guard_ids: [{}], guard_clause_ids: guard_clause_ids, guard_tags: guard_tags, guard_kinds: guard_kinds, guard_pkgs: guard_pkgs, guard_version_values: guard_version_values, guard_features: guard_features, consequent_tags: consequent_tags, consequent_pkgs: consequent_pkgs, consequent_version_sets: consequent_version_sets, consequent_features: consequent_features, gate_kinds: gate_kinds, gate_targets: gate_targets }}",
            clauses.ids.join(", "),
            clauses.guard_ids.join(", ")
        )
        .ok();
        writeln!(source, "}}").ok();

        writeln!(source, "\nfn fixture_problem() -> Problem {{").ok();
        writeln!(
            source,
            "    Problem {{ root_pkg: {}, root_req: VersionSet::from_req(\"*\"), root_features: [], root_default_feature: {}, root_default_features: true }}",
            package_indices[&self.root],
            feature_indices[&(self.root.clone(), "default".to_owned())]
        )
        .ok();
        writeln!(source, "}}").ok();

        writeln!(
            source,
            "\npub fn fixture_selected(target: String) -> String {{\n    solve_selected_names_text(fixture_index(), fixture_problem(), target)\n}}"
        )
        .ok();
        writeln!(
            source,
            "\npub fn fixture_learned_count(target: String) -> Int {{\n    solve_learned_count(fixture_index(), fixture_problem(), target)\n}}"
        )
        .ok();
        source
    }

    fn vix_version_rows(&self) -> Vec<(String, String)> {
        let mut rows = Vec::new();
        for krate in &self.crates {
            for version in &krate.vix_versions {
                rows.push((krate.name.clone(), version.clone()));
            }
        }
        rows
    }

    fn vix_clauses(&self) -> VixClauses {
        let mut clauses = VixClauses::new(self.package_indices(), self.feature_indices());
        let mut next_id = 0;

        for krate in &self.crates {
            let explicit_optional_deps = explicit_optional_dep_features(krate);
            for version in &krate.vix_versions {
                for dep in &krate.deps {
                    for clause in self.dependency_edge_clauses(krate, version, dep) {
                        push_generated_clause(&mut clauses, &mut next_id, clause);
                    }
                    if dep.optional && !explicit_optional_deps.contains(&dep.name) {
                        let enable = format!("dep:{}", dep.name);
                        for clause in self
                            .feature_enable_clauses(krate, version, dep.kind, &dep.name, &enable)
                        {
                            push_generated_clause(&mut clauses, &mut next_id, clause);
                        }
                    }
                }

                if let Some(deps) = krate.vix_version_deps.get(version) {
                    for dep in deps {
                        for clause in self.dependency_edge_clauses(krate, version, dep) {
                            push_generated_clause(&mut clauses, &mut next_id, clause);
                        }
                        if dep.optional && !explicit_optional_deps.contains(&dep.name) {
                            let enable = format!("dep:{}", dep.name);
                            for clause in self.feature_enable_clauses(
                                krate, version, dep.kind, &dep.name, &enable,
                            ) {
                                push_generated_clause(&mut clauses, &mut next_id, clause);
                            }
                        }
                    }
                }

                for (feature, enables) in &krate.features {
                    for scope in feature_scopes() {
                        for enable in enables {
                            for clause in
                                self.feature_enable_clauses(krate, version, scope, feature, enable)
                            {
                                push_generated_clause(&mut clauses, &mut next_id, clause);
                            }
                        }
                    }
                }
            }
        }

        clauses
    }

    fn dependency_edge_clauses(
        &self,
        krate: &FixtureCrate,
        version: &str,
        dep: &FixtureDep,
    ) -> Vec<VixClause> {
        if dep.optional {
            return Vec::new();
        }
        let guards = vec![
            guard_in_graph(&krate.name),
            guard_selected(&krate.name, version),
        ];
        self.dependency_activation_clauses(krate, dep, &guards, None)
    }

    fn dependency_activation_clauses(
        &self,
        krate: &FixtureCrate,
        dep: &FixtureDep,
        guards: &[VixGuard],
        requested_feature: Option<&str>,
    ) -> Vec<VixClause> {
        let gate = Some(dep_gate(krate, dep));
        let mut clauses = vec![
            vix_clause(0, guards, consequent_in_graph(&dep.name), gate.clone()),
            vix_clause(
                0,
                guards,
                consequent_version(&dep.name, &dep.req),
                gate.clone(),
            ),
        ];
        self.push_dependency_requested_feature_clauses(krate, dep, guards, &mut clauses);
        if let Some(feature) = requested_feature {
            let scoped = scoped_feature_name(dep.kind, feature);
            clauses.push(vix_clause(
                0,
                guards,
                consequent_feature(&dep.name, &scoped),
                gate,
            ));
        }
        clauses
    }

    fn push_dependency_requested_feature_clauses(
        &self,
        krate: &FixtureCrate,
        dep: &FixtureDep,
        guards: &[VixGuard],
        clauses: &mut Vec<VixClause>,
    ) {
        let gate = Some(dep_gate(krate, dep));
        if dep.default_features {
            let default = scoped_feature_name(dep.kind, "default");
            clauses.push(vix_clause(
                0,
                guards,
                consequent_feature(&dep.name, &default),
                gate.clone(),
            ));
        }
        for feature in &dep.features {
            let scoped = scoped_feature_name(dep.kind, feature);
            clauses.push(vix_clause(
                0,
                guards,
                consequent_feature(&dep.name, &scoped),
                gate.clone(),
            ));
        }
    }

    fn feature_enable_clauses(
        &self,
        krate: &FixtureCrate,
        version: &str,
        scope: DepKind,
        feature: &str,
        enable: &str,
    ) -> Vec<VixClause> {
        let scoped_feature = scoped_feature_name(scope, feature);
        let base_guards = vec![
            guard_in_graph(&krate.name),
            guard_selected(&krate.name, version),
            guard_feature(&krate.name, &scoped_feature),
        ];
        if let Some(dep_name) = enable.strip_prefix("dep:") {
            let Some(dep) = self.feature_dep(krate, version, dep_name, scope) else {
                return Vec::new();
            };
            return self.dependency_activation_clauses(krate, dep, &base_guards, None);
        }

        if let Some((dep_name, dep_feature)) = enable.split_once("?/") {
            let Some(dep) = self.feature_dep(krate, version, dep_name, scope) else {
                return Vec::new();
            };
            let mut guards = base_guards;
            guards.push(guard_in_graph(dep_name));
            let scoped_dep_feature = scoped_feature_name(scope, dep_feature);
            return vec![vix_clause(
                0,
                &guards,
                consequent_feature(dep_name, &scoped_dep_feature),
                Some(dep_gate(krate, dep)),
            )];
        }

        if let Some((dep_name, dep_feature)) = enable.split_once('/') {
            let Some(dep) = self.feature_dep(krate, version, dep_name, scope) else {
                return Vec::new();
            };
            return self.dependency_activation_clauses(krate, dep, &base_guards, Some(dep_feature));
        }

        let local_feature = scoped_feature_name(scope, enable);
        vec![vix_clause(
            0,
            &base_guards,
            consequent_feature(&krate.name, &local_feature),
            None,
        )]
    }

    fn feature_dep<'a>(
        &'a self,
        krate: &'a FixtureCrate,
        version: &str,
        name: &str,
        scope: DepKind,
    ) -> Option<&'a FixtureDep> {
        krate
            .deps
            .iter()
            .find(|dep| dep.name == name && dep.kind == scope && dep.optional)
            .or_else(|| {
                krate
                    .deps
                    .iter()
                    .find(|dep| dep.name == name && dep.kind == scope)
            })
            .or_else(|| {
                krate.vix_version_deps.get(version).and_then(|deps| {
                    deps.iter()
                        .find(|dep| dep.name == name && dep.kind == scope && dep.optional)
                        .or_else(|| {
                            deps.iter()
                                .find(|dep| dep.name == name && dep.kind == scope)
                        })
                })
            })
    }

    fn package_indices(&self) -> BTreeMap<String, usize> {
        self.crates
            .iter()
            .enumerate()
            .map(|(index, krate)| (krate.name.clone(), index))
            .collect()
    }

    fn feature_indices(&self) -> BTreeMap<(String, String), usize> {
        let mut features = BTreeMap::new();
        for krate in &self.crates {
            for scope in feature_scopes() {
                register_feature(
                    &mut features,
                    &krate.name,
                    &scoped_feature_name(scope, "default"),
                );
                for feature in krate.features.keys() {
                    register_feature(
                        &mut features,
                        &krate.name,
                        &scoped_feature_name(scope, feature),
                    );
                }
            }
            let explicit_optional_deps = explicit_optional_dep_features(krate);
            for dep in krate
                .deps
                .iter()
                .chain(krate.vix_version_deps.values().flatten())
            {
                if dep.optional && !explicit_optional_deps.contains(&dep.name) {
                    register_feature(
                        &mut features,
                        &krate.name,
                        &scoped_feature_name(dep.kind, &dep.name),
                    );
                }
                if !dep.optional {
                    if dep.default_features {
                        register_feature(
                            &mut features,
                            &dep.name,
                            &scoped_feature_name(dep.kind, "default"),
                        );
                    }
                    for feature in &dep.features {
                        register_feature(
                            &mut features,
                            &dep.name,
                            &scoped_feature_name(dep.kind, feature),
                        );
                    }
                }
            }
            for version in &krate.vix_versions {
                for (feature, enables) in &krate.features {
                    for scope in feature_scopes() {
                        register_feature(
                            &mut features,
                            &krate.name,
                            &scoped_feature_name(scope, feature),
                        );
                        for enable in enables {
                            if let Some(dep_name) = enable.strip_prefix("dep:") {
                                if let Some(dep) = self.feature_dep(krate, version, dep_name, scope)
                                {
                                    register_activation_features(&mut features, dep);
                                }
                            } else if let Some((dep_name, dep_feature)) = enable.split_once("?/") {
                                register_feature(
                                    &mut features,
                                    dep_name,
                                    &scoped_feature_name(scope, dep_feature),
                                );
                            } else if let Some((dep_name, dep_feature)) = enable.split_once('/') {
                                if let Some(dep) = self.feature_dep(krate, version, dep_name, scope)
                                {
                                    register_activation_features(&mut features, dep);
                                    register_feature(
                                        &mut features,
                                        dep_name,
                                        &scoped_feature_name(scope, dep_feature),
                                    );
                                }
                            } else {
                                register_feature(
                                    &mut features,
                                    &krate.name,
                                    &scoped_feature_name(scope, enable),
                                );
                            }
                        }
                    }
                }
            }
        }
        features
    }
}

fn register_feature(
    features: &mut BTreeMap<(String, String), usize>,
    package: &str,
    feature: &str,
) {
    let key = (package.to_owned(), feature.to_owned());
    if !features.contains_key(&key) {
        features.insert(key, features.len());
    }
}

fn register_activation_features(
    features: &mut BTreeMap<(String, String), usize>,
    dep: &FixtureDep,
) {
    if dep.default_features {
        register_feature(
            features,
            &dep.name,
            &scoped_feature_name(dep.kind, "default"),
        );
    }
    for feature in &dep.features {
        register_feature(features, &dep.name, &scoped_feature_name(dep.kind, feature));
    }
}

fn explicit_optional_dep_features(krate: &FixtureCrate) -> BTreeSet<String> {
    krate
        .features
        .values()
        .flat_map(|enables| enables.iter())
        .filter_map(|enable| enable.strip_prefix("dep:"))
        .map(str::to_owned)
        .collect()
}

fn feature_scopes() -> [DepKind; 3] {
    [DepKind::Normal, DepKind::Build, DepKind::Dev]
}

fn scoped_feature_name(scope: DepKind, feature: &str) -> String {
    match scope {
        DepKind::Normal => feature.to_owned(),
        DepKind::Build => format!("build:{feature}"),
        DepKind::Dev => format!("dev:{feature}"),
    }
}

struct VixClauses {
    ids: Vec<String>,
    guard_ids: Vec<String>,
    inserts: Vec<String>,
    next_guard_id: usize,
    package_indices: BTreeMap<String, usize>,
    feature_indices: BTreeMap<(String, String), usize>,
}

impl VixClauses {
    fn new(
        package_indices: BTreeMap<String, usize>,
        feature_indices: BTreeMap<(String, String), usize>,
    ) -> Self {
        Self {
            ids: Vec::new(),
            guard_ids: Vec::new(),
            inserts: Vec::new(),
            next_guard_id: 0,
            package_indices,
            feature_indices,
        }
    }

    fn push(&mut self, clause: VixClause) {
        let id = self.ids.len();
        self.ids.push(id.to_string());
        for guard in clause.antecedents {
            let guard_id = self.next_guard_id;
            self.next_guard_id += 1;
            self.guard_ids.push(guard_id.to_string());
            self.inserts.push(format!(
                "let guard_clause_ids = guard_clause_ids.insert({guard_id}, {id});"
            ));
            self.inserts.push(format!(
                "let guard_tags = guard_tags.insert({guard_id}, {});",
                vix_string(guard.tag())
            ));
            self.inserts.push(format!(
                "let guard_kinds = guard_kinds.insert({guard_id}, {});",
                guard.kind()
            ));
            self.inserts.push(format!(
                "let guard_pkgs = guard_pkgs.insert({guard_id}, {});",
                self.pkg_id(guard.pkg())
            ));
            if let Some(version) = guard.version() {
                self.inserts.push(format!(
                    "let guard_version_values = guard_version_values.insert({guard_id}, {});",
                    vix_string(version)
                ));
            }
            self.inserts.push(format!(
                "let guard_features = guard_features.insert({guard_id}, {});",
                self.feature_id_or_zero(guard.pkg(), guard.feature())
            ));
        }
        self.inserts.push(format!(
            "let consequent_tags = consequent_tags.insert({id}, {});",
            vix_string(clause.consequent.tag())
        ));
        self.inserts.push(format!(
            "let consequent_pkgs = consequent_pkgs.insert({id}, {});",
            self.pkg_id(clause.consequent.pkg())
        ));
        self.inserts.push(format!(
            "let consequent_version_sets = consequent_version_sets.insert({id}, {});",
            clause.consequent.version_set_expr()
        ));
        self.inserts.push(format!(
            "let consequent_features = consequent_features.insert({id}, {});",
            self.feature_id_or_zero(clause.consequent.pkg(), clause.consequent.feature())
        ));
        let kind = clause
            .gate
            .as_ref()
            .map_or("normal", |gate| gate.kind.as_str());
        self.inserts.push(format!(
            "let gate_kinds = gate_kinds.insert({id}, {});",
            vix_string(kind)
        ));
        if let Some(target) = clause.gate.as_ref().and_then(|gate| gate.target.as_deref()) {
            self.inserts.push(format!(
                "let gate_targets = gate_targets.insert({id}, {});",
                vix_string(target)
            ));
        }
    }

    fn pkg_id(&self, name: &str) -> usize {
        *self
            .package_indices
            .get(name)
            .unwrap_or_else(|| panic!("fixture references unknown package `{name}`"))
    }

    fn feature_id(&self, package: &str, feature: &str) -> usize {
        *self
            .feature_indices
            .get(&(package.to_owned(), feature.to_owned()))
            .unwrap_or_else(|| panic!("fixture references unknown feature `{package}/{feature}`"))
    }

    fn feature_id_or_zero(&self, package: &str, feature: &str) -> usize {
        match feature.is_empty() {
            true => 0,
            false => self.feature_id(package, feature),
        }
    }
}

#[derive(Clone)]
struct VixClause {
    antecedents: Vec<VixGuard>,
    consequent: VixConsequent,
    gate: Option<VixGate>,
}

#[derive(Clone)]
enum VixGuard {
    InGraph { name: String },
    Selected { name: String, version: String },
    Feature { name: String, feature: String },
}

impl VixGuard {
    fn kind(&self) -> i32 {
        match self {
            Self::InGraph { .. } => 0,
            Self::Selected { .. } => 1,
            Self::Feature { .. } => 2,
        }
    }

    fn tag(&self) -> &'static str {
        match self {
            Self::InGraph { .. } => "in_graph",
            Self::Selected { .. } => "selected",
            Self::Feature { .. } => "feature",
        }
    }

    fn pkg(&self) -> &str {
        match self {
            Self::InGraph { name } | Self::Selected { name, .. } | Self::Feature { name, .. } => {
                name
            }
        }
    }

    fn version(&self) -> Option<&str> {
        match self {
            Self::Selected { version, .. } => Some(version),
            Self::InGraph { .. } | Self::Feature { .. } => None,
        }
    }

    fn feature(&self) -> &str {
        match self {
            Self::Feature { feature, .. } => feature,
            Self::InGraph { .. } | Self::Selected { .. } => "",
        }
    }
}

#[derive(Clone)]
enum VixConsequent {
    InGraph { name: String },
    VersionSet { name: String, req: String },
    Feature { name: String, feature: String },
}

impl VixConsequent {
    fn tag(&self) -> &'static str {
        match self {
            Self::InGraph { .. } => "in_graph",
            Self::VersionSet { .. } => "version_set",
            Self::Feature { .. } => "feature",
        }
    }

    fn pkg(&self) -> &str {
        match self {
            Self::InGraph { name } | Self::VersionSet { name, .. } | Self::Feature { name, .. } => {
                name
            }
        }
    }

    fn version_set_expr(&self) -> String {
        match self {
            Self::VersionSet { req, .. } => format!("VersionSet::from_req({})", vix_string(req)),
            Self::InGraph { .. } | Self::Feature { .. } => "VersionSet::from_req(\"*\")".to_owned(),
        }
    }

    fn feature(&self) -> &str {
        match self {
            Self::Feature { feature, .. } => feature,
            Self::InGraph { .. } | Self::VersionSet { .. } => "",
        }
    }
}

#[derive(Clone)]
struct VixGate {
    kind: String,
    target: Option<String>,
}

fn dep_line(dep: &FixtureDep) -> String {
    let mut attrs = vec![
        format!("path = \"../{}\"", dep.name),
        format!("version = \"{}\"", dep.req),
    ];
    if dep.optional {
        attrs.push("optional = true".to_owned());
    }
    if !dep.default_features {
        attrs.push("default-features = false".to_owned());
    }
    if !dep.features.is_empty() {
        let list = dep
            .features
            .iter()
            .map(|f| format!("\"{f}\""))
            .collect::<Vec<_>>()
            .join(", ");
        attrs.push(format!("features = [{list}]"));
    }
    format!("{} = {{ {} }}", dep.name, attrs.join(", "))
}

/// The set of crate names cargo includes in the graph for `triple`, offline.
fn cargo_selected(workspace: &Path, root: &str, triple: &str) -> Result<BTreeSet<String>, String> {
    let output = Command::new("cargo")
        .args([
            "tree",
            "-e",
            "normal,build",
            "--target",
            triple,
            "--prefix",
            "none",
            "--offline",
            "-p",
            root,
        ])
        .current_dir(workspace)
        .output()
        .map_err(|e| format!("spawning cargo tree: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "cargo tree failed for {root} on {triple}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let text = String::from_utf8(output.stdout).map_err(|e| e.to_string())?;
    Ok(text
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect())
}

fn unique_temp_dir(name: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nonce = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "rodin-fixture-{name}-{}-{nonce}",
        std::process::id()
    ))
}

fn rodin_source() -> Result<String, String> {
    std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../rodin/rodin.vix"))
        .map_err(|err| format!("read rodin.vix: {err}"))
}

fn rendered_name_set(value: RenderedValue) -> Result<BTreeSet<String>, String> {
    let RenderedValue::String { value } = value else {
        return Err(format!(
            "fixture_selected rendered as {value:?}, not String"
        ));
    };
    Ok(value
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect())
}

#[derive(Clone, Copy, Debug)]
struct TraceCounts {
    demanded: usize,
    spawned_invocations: usize,
}

impl TraceCounts {
    fn total(self) -> usize {
        self.demanded + self.spawned_invocations
    }
}

fn trace_counts(machine: &Machine, names: &[&str]) -> Result<TraceCounts, String> {
    let hashes = names
        .iter()
        .map(|name| {
            machine
                .fn_hash(name)
                .ok_or_else(|| format!("missing function hash for {name}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut demanded = 0;
    let mut spawned_invocations = 0;
    for event in machine.trace() {
        match event {
            DriveEvent::Demanded { fn_hash } if hashes.contains(fn_hash) => demanded += 1,
            DriveEvent::SpawnedInvocation { fn_hash, .. } if hashes.contains(fn_hash) => {
                spawned_invocations += 1;
            }
            _ => {}
        }
    }
    Ok(TraceCounts {
        demanded,
        spawned_invocations,
    })
}

fn trace_tail(machine: &Machine) -> String {
    let names = machine
        .fn_hashes()
        .into_iter()
        .map(|(name, hash)| (hash, name))
        .collect::<BTreeMap<_, _>>();
    let trace = machine.trace();
    let start = trace.len().saturating_sub(180);
    let mut out = String::from("trace tail:");
    for (index, event) in trace.iter().enumerate().skip(start) {
        let name = match event {
            DriveEvent::Demanded { fn_hash }
            | DriveEvent::Spawned { fn_hash }
            | DriveEvent::ParkedOn { fn_hash }
            | DriveEvent::Completed { fn_hash }
            | DriveEvent::SpawnedInvocation { fn_hash, .. }
            | DriveEvent::MemoHit { fn_hash }
            | DriveEvent::MemoProjectionHit { fn_hash, .. }
            | DriveEvent::MemoSemanticHit { fn_hash, .. } => names.get(fn_hash).map(String::as_str),
            _ => None,
        }
        .unwrap_or("<unknown>");
        write!(out, "\n  {index}: {name}: {event:?}").ok();
    }
    out
}

fn pkg_fn(name: &str) -> String {
    let mut out = String::from("pkg_");
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    out
}

fn vix_string(value: &str) -> String {
    format!("{value:?}")
}

fn guard_in_graph(name: &str) -> VixGuard {
    VixGuard::InGraph {
        name: name.to_owned(),
    }
}

fn guard_selected(name: &str, version: &str) -> VixGuard {
    VixGuard::Selected {
        name: name.to_owned(),
        version: version.to_owned(),
    }
}

fn guard_feature(name: &str, feature: &str) -> VixGuard {
    VixGuard::Feature {
        name: name.to_owned(),
        feature: feature.to_owned(),
    }
}

fn consequent_in_graph(name: &str) -> VixConsequent {
    VixConsequent::InGraph {
        name: name.to_owned(),
    }
}

fn consequent_version(name: &str, req: &str) -> VixConsequent {
    VixConsequent::VersionSet {
        name: name.to_owned(),
        req: req.to_owned(),
    }
}

fn consequent_feature(name: &str, feature: &str) -> VixConsequent {
    VixConsequent::Feature {
        name: name.to_owned(),
        feature: feature.to_owned(),
    }
}

fn dep_gate(_parent: &FixtureCrate, dep: &FixtureDep) -> VixGate {
    let kind = match dep.kind {
        DepKind::Normal => "normal",
        DepKind::Build => "build",
        DepKind::Dev => "dev",
    };
    VixGate {
        kind: kind.to_owned(),
        target: dep.target.clone(),
    }
}

fn vix_clause(
    _id: i32,
    antecedents: &[VixGuard],
    consequent: VixConsequent,
    gate: Option<VixGate>,
) -> VixClause {
    VixClause {
        antecedents: antecedents.to_vec(),
        consequent,
        gate,
    }
}

fn push_generated_clause(clauses: &mut VixClauses, next_id: &mut i32, clause: VixClause) {
    clauses.push(clause);
    *next_id += 1;
}

// ===========================================================================
// The live-Cargo oracle harness (typed, serde-free).
//
// cargo is THE and ONLY oracle (rodin/PLAN.md, rodin/docs/00-oracle.md). Both
// oracles are read from `cargo metadata` — Cargo's own machine surface — parsed
// with facet-json (no serde). metadata is chosen over `cargo tree` / `Cargo.lock`
// display text because it alone gives the exact package id, the typed source
// (with a path coordinate), the per-edge dependency kind, and `--filter-platform`
// cfg projection. Mapping to doc-00's two oracles is in the module docs.
// ===========================================================================

// ---- facet-json view of `cargo metadata` (a subset; unknown JSON fields are ----
// ---- ignored — facet-json only rejects them under deny_unknown_fields).    ----

#[derive(Facet, Debug)]
struct CargoMetadata {
    packages: Vec<MetaPackage>,
    resolve: MetaResolve,
}

#[derive(Facet, Debug)]
struct MetaPackage {
    /// Cargo's opaque-but-unique package id.
    id: String,
    name: String,
    version: String,
    /// `null` for a path/workspace member; `registry+`/`sparse+`/`git+…` otherwise.
    source: Option<String>,
    manifest_path: String,
    dependencies: Vec<MetaDependency>,
    features: BTreeMap<String, Vec<String>>,
}

#[derive(Facet, Debug)]
struct MetaDependency {
    name: String,
    rename: Option<String>,
    kind: Option<String>,
    optional: bool,
}

#[derive(Facet, Debug)]
struct MetaResolve {
    nodes: Vec<MetaNode>,
}

#[derive(Facet, Debug)]
struct MetaNode {
    id: String,
    features: Vec<String>,
    deps: Vec<MetaDep>,
}

#[derive(Facet, Debug)]
struct MetaDep {
    /// The dependency name in the parent package (the alias when renamed).
    name: String,
    /// The resolved dependency's package id.
    pkg: String,
    dep_kinds: Vec<MetaDepKind>,
}

#[derive(Facet, Debug)]
struct MetaDepKind {
    /// `null` = normal, `"build"`, or `"dev"`. (`--filter-platform` has already
    /// applied the cfg gate, so the `target` field is not modelled.)
    kind: Option<String>,
}

// ---- exact Cargo identity, kept separate from Rodin's resolution domain ----

/// Provenance of a Cargo package: a typed classification of cargo's `source`
/// field. A path dependency carries its manifest directory, so two path crates of
/// the same name never collapse; an unrecognized scheme is a parse error, never a
/// silent registry (see [`classify_source`]).
#[derive(Facet, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
enum Source {
    /// A path dependency (cargo `source: null`), keyed by its manifest directory.
    Path { dir: String },
    /// crates.io or an alternate registry, by the full `registry+`/`sparse+` spec.
    Registry { spec: String },
    /// A git dependency, by the full `git+URL#REV` spec (rev embedded by cargo).
    Git { spec: String },
}

/// The exact, unambiguous Cargo package identity — `(source, name, version)`,
/// which is Cargo.lock's own key and is unique per resolved package. This is what
/// Cargo actually selected; the harness never collapses two of these.
#[derive(Facet, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct CargoPackageId {
    source: Source,
    name: String,
    version: String,
}

impl CargoPackageId {
    /// Project onto Rodin's coexistence domain. Fails only if the version is not
    /// semver — cargo guarantees it is, so this is a defensive typed error.
    fn domain(&self) -> Result<ResolutionDomain, String> {
        let parsed = SemVer::parse(&self.version)
            .map_err(|err| format!("parse version `{}` of `{}`: {err}", self.version, self.name))?;
        Ok(ResolutionDomain {
            source: self.source.clone(),
            name: self.name.clone(),
            compat: CompatClass::of(&parsed),
        })
    }
}

/// Rodin's resolution domain (rodin/docs/10-identity.md): `(source, name,
/// compat-class)`, the coexistence bucket a version competes in. This is a
/// *projection* of the exact Cargo identity, NOT the identity itself; a
/// non-injective projection is a [`Discrepancy::DomainMultiplicity`].
#[derive(Facet, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ResolutionDomain {
    source: Source,
    name: String,
    compat: CompatClass,
}

/// The semver coexistence bucket (rodin/docs/10-identity.md § compat-class rule),
/// keyed on the first non-zero version component.
#[derive(Facet, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
enum CompatClass {
    /// `major >= 1` → class keyed on major (`1.4.2` and `1.9.0` share class `1`).
    Major { major: u64 },
    /// `major == 0, minor != 0` → the `0.y` footgun; each `0.minor` is its own class.
    ZeroMinor { minor: u64 },
    /// `major == 0, minor == 0` → each `0.0.patch` is its own class.
    ZeroZeroPatch { patch: u64 },
}

impl CompatClass {
    fn of(version: &SemVer) -> Self {
        if version.major >= 1 {
            Self::Major {
                major: version.major,
            }
        } else if version.minor != 0 {
            Self::ZeroMinor {
                minor: version.minor,
            }
        } else {
            Self::ZeroZeroPatch {
                patch: version.patch,
            }
        }
    }
}

/// Classify cargo's `source` field into a typed [`Source`]. `None` is a path
/// dependency (coordinate taken from `manifest_path`); recognized registry/git
/// schemes keep their full spec; anything else is a typed parse error.
fn classify_source(source: Option<&str>, manifest_path: &str) -> Result<Source, String> {
    match source {
        None => {
            let dir = Path::new(manifest_path)
                .parent()
                .ok_or_else(|| {
                    format!("path package manifest has no directory: {manifest_path:?}")
                })?
                .to_string_lossy()
                .into_owned();
            Ok(Source::Path { dir })
        }
        Some(spec) if spec.starts_with("registry+") || spec.starts_with("sparse+") => {
            Ok(Source::Registry {
                spec: spec.to_owned(),
            })
        }
        Some(spec) if spec.starts_with("git+") => Ok(Source::Git {
            spec: spec.to_owned(),
        }),
        Some(other) => Err(format!("unrecognized Cargo source scheme: {other:?}")),
    }
}

fn package_id(package: &MetaPackage) -> Result<CargoPackageId, String> {
    Ok(CargoPackageId {
        source: classify_source(package.source.as_deref(), &package.manifest_path)?,
        name: package.name.clone(),
        version: package.version.clone(),
    })
}

fn activated_optional_dependencies(package: &MetaPackage, node: &MetaNode) -> BTreeSet<String> {
    let mut active = BTreeSet::new();
    let mut expanded = BTreeSet::new();
    let mut pending = node.features.clone();
    while let Some(feature) = pending.pop() {
        if !expanded.insert(feature.clone()) {
            continue;
        }
        let Some(effects) = package.features.get(&feature) else {
            continue;
        };
        for effect in effects {
            if let Some(dependency) = effect.strip_prefix("dep:") {
                active.insert(dependency.to_owned());
            } else if effect.contains("?/") {
                continue;
            } else if let Some((dependency, _)) = effect.split_once('/') {
                active.insert(dependency.to_owned());
            } else {
                pending.push(effect.clone());
            }
        }
    }
    active
}

fn dependency_alias(dependency: &MetaDependency) -> &str {
    dependency.rename.as_deref().unwrap_or(&dependency.name)
}

// ---- Oracle 2's edge kind ----

/// The dependency-edge kind retained by Oracle 2. The graph oracle compares the
/// enabled `normal` + `build` edges (dev is excluded by the walk), and the kind
/// is part of edge identity: a `normal` edge and a `build` edge to the same
/// package are distinct edges (target graph vs host graph). `dev` never appears.
#[derive(Facet, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
enum EdgeKind {
    Normal,
    Build,
}

impl EdgeKind {
    /// Map cargo's `dep_kinds[].kind`: `null` = normal, `"build"` = build, `"dev"`
    /// = excluded (`Ok(None)`); anything else is a typed parse error.
    fn classify(kind: Option<&str>) -> Result<Option<Self>, String> {
        match kind {
            None => Ok(Some(Self::Normal)),
            Some("build") => Ok(Some(Self::Build)),
            Some("dev") => Ok(None),
            Some(other) => Err(format!("unrecognized Cargo dependency kind: {other:?}")),
        }
    }
}

/// One edge of Oracle 2: a consumed dependency edge under a specific target and
/// the `normal`/`build` edge-kind filter, with both endpoints as exact ids.
#[derive(Facet, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct GraphEdge {
    from: CargoPackageId,
    to: CargoPackageId,
    kind: EdgeKind,
}

// ---- the two oracles + the future SUT's typed output ----

/// Oracle 1 — the lockfile's selection: every exact Cargo package cargo resolved.
struct SelectionOracle {
    packages: BTreeSet<CargoPackageId>,
}

/// Oracle 2 — the target-projected enabled graph for one triple.
struct GraphOracle {
    triple: String,
    nodes: BTreeSet<CargoPackageId>,
    edges: BTreeSet<GraphEdge>,
}

/// The Cargo-facing projection of a future Vix kernel result: a per-package
/// selection plus the per-target enabled graph. The pure kernel keeps `Version`
/// typed; the external harness adapter renders its exact Cargo spelling while
/// constructing this value. Neither the kernel result nor that adapter exists
/// in-tree yet.
#[derive(Facet, Clone, Debug, Default)]
struct SolveResult {
    selected: BTreeSet<CargoPackageId>,
    /// Enabled graph edges, keyed by target triple.
    graphs: BTreeMap<String, BTreeSet<GraphEdge>>,
}

impl SolveResult {
    /// Compare against Oracle 1 and, for each provided [`GraphOracle`], Oracle 2.
    /// Deterministic (BTree order) and structural — ready for a minimizer.
    fn compare(&self, selection: &SelectionOracle, graphs: &[GraphOracle]) -> Vec<Discrepancy> {
        let mut out = compare_selection(&self.selected, selection);
        for graph in graphs {
            let candidate = self.graphs.get(&graph.triple).cloned().unwrap_or_default();
            out.extend(compare_graph(&graph.triple, &candidate, graph));
        }
        out
    }
}

/// Which side of a comparison an observation belongs to.
#[derive(Facet, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum Side {
    Cargo,
    Candidate,
}

/// A single typed divergence between a candidate `SolveResult` and a cargo oracle.
/// The Oracle-1 vocabulary mirrors the historical differential's classification
/// (rodin/docs/00-oracle.md § provenance): MissingInRodin / ExtraInRodin /
/// VersionMismatch, now over exact Cargo identity with domain multiplicity and
/// malformed versions surfaced rather than collapsed.
#[derive(Facet, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
enum Discrepancy {
    /// Oracle 1: cargo locked this domain's exact package; the candidate did not.
    MissingSelection { expected: CargoPackageId },
    /// Oracle 1: the candidate selected a domain cargo did not lock.
    ExtraSelection { unexpected: CargoPackageId },
    /// Oracle 1: same Rodin domain (same coexistence bucket), different exact version.
    VersionMismatch {
        domain: ResolutionDomain,
        cargo: String,
        candidate: String,
    },
    /// Oracle 1: one Rodin domain is covered by >1 exact Cargo package on `side`.
    /// Rodin's `(source, name, compat-class)` model represents at most one version
    /// per domain, so it cannot represent this — an explicit *unsupported* Cargo
    /// case, not an impossibility. The harness does not assume Rodin's compat-class
    /// matches cargo's own version bucketing for every input (prerelease/build-
    /// metadata semantics, alternate registries, or a compat-class bug could
    /// diverge), so any such domain is surfaced here rather than last-wins
    /// collapsed or assumed away.
    DomainMultiplicity {
        side: Side,
        domain: ResolutionDomain,
        packages: Vec<CargoPackageId>,
    },
    /// Oracle 1: a package version on `side` is not semver — cannot be assigned a
    /// coexistence domain.
    MalformedVersion { side: Side, package: CargoPackageId },
    /// Oracle 2: cargo's target-projected graph has an edge the candidate lacks.
    MissingEdge { target: String, edge: GraphEdge },
    /// Oracle 2: the candidate has a target-projected edge cargo does not.
    ExtraEdge { target: String, edge: GraphEdge },
}

/// A minimizable, machine-readable report over one fixture, serialized via
/// `facet-json` — never hand-written, never serde.
#[derive(Facet, Clone, Debug)]
struct DiscrepancyReport {
    fixture: String,
    discrepancies: Vec<Discrepancy>,
}

impl DiscrepancyReport {
    fn to_json(&self) -> Result<String, String> {
        facet_json::to_string_pretty(self)
            .map_err(|err| format!("serialize discrepancy report: {err}"))
    }
}

/// Group exact packages by Rodin domain, surfacing every non-injective projection
/// (domain multiplicity) and every malformed version as a typed discrepancy on
/// `side`, and returning the well-defined singleton domains for comparison.
fn domain_index(
    packages: &BTreeSet<CargoPackageId>,
    side: Side,
    out: &mut Vec<Discrepancy>,
) -> BTreeMap<ResolutionDomain, CargoPackageId> {
    let mut grouped: BTreeMap<ResolutionDomain, Vec<CargoPackageId>> = BTreeMap::new();
    for package in packages {
        match package.domain() {
            Ok(domain) => grouped.entry(domain).or_default().push(package.clone()),
            Err(_) => out.push(Discrepancy::MalformedVersion {
                side,
                package: package.clone(),
            }),
        }
    }
    let mut singletons = BTreeMap::new();
    for (domain, mut packages) in grouped {
        if packages.len() == 1 {
            singletons.insert(domain, packages.pop().expect("length checked to be one"));
        } else {
            // packages came from a BTreeSet, so they are already sorted.
            out.push(Discrepancy::DomainMultiplicity {
                side,
                domain,
                packages,
            });
        }
    }
    singletons
}

/// Oracle 1 comparison: exact-identity missing/extra plus same-domain version
/// mismatch, with multiplicity/malformed observations preserved (never collapsed).
fn compare_selection(
    candidate: &BTreeSet<CargoPackageId>,
    oracle: &SelectionOracle,
) -> Vec<Discrepancy> {
    let mut out = Vec::new();
    let oracle_by_domain = domain_index(&oracle.packages, Side::Cargo, &mut out);
    let candidate_by_domain = domain_index(candidate, Side::Candidate, &mut out);
    for (domain, cargo_pkg) in &oracle_by_domain {
        match candidate_by_domain.get(domain) {
            None => out.push(Discrepancy::MissingSelection {
                expected: cargo_pkg.clone(),
            }),
            Some(candidate_pkg) if candidate_pkg.version != cargo_pkg.version => {
                out.push(Discrepancy::VersionMismatch {
                    domain: domain.clone(),
                    cargo: cargo_pkg.version.clone(),
                    candidate: candidate_pkg.version.clone(),
                });
            }
            Some(_) => {}
        }
    }
    for (domain, candidate_pkg) in &candidate_by_domain {
        if !oracle_by_domain.contains_key(domain) {
            out.push(Discrepancy::ExtraSelection {
                unexpected: candidate_pkg.clone(),
            });
        }
    }
    out
}

/// Oracle 2 comparison: the symmetric difference of the edge sets for `target`.
fn compare_graph(
    target: &str,
    candidate: &BTreeSet<GraphEdge>,
    oracle: &GraphOracle,
) -> Vec<Discrepancy> {
    let mut out = Vec::new();
    for edge in oracle.edges.difference(candidate) {
        out.push(Discrepancy::MissingEdge {
            target: target.to_owned(),
            edge: edge.clone(),
        });
    }
    for edge in candidate.difference(&oracle.edges) {
        out.push(Discrepancy::ExtraEdge {
            target: target.to_owned(),
            edge: edge.clone(),
        });
    }
    out
}

impl Fixture {
    /// Run `cargo metadata` (optionally `--filter-platform <triple>`) over a
    /// *single* materialized workspace and parse it via facet-json. Both oracles
    /// take the same `workspace` so the path-source coordinate — part of exact
    /// identity — is shared and their exact ids are directly comparable.
    fn cargo_metadata(
        &self,
        workspace: &Path,
        filter_platform: Option<&str>,
    ) -> Result<CargoMetadata, String> {
        let mut args = vec![
            "metadata".to_owned(),
            "--format-version".to_owned(),
            "1".to_owned(),
            "--offline".to_owned(),
        ];
        if let Some(triple) = filter_platform {
            args.push("--filter-platform".to_owned());
            args.push(triple.to_owned());
        }
        let output = Command::new("cargo")
            .args(&args)
            .current_dir(workspace)
            .output()
            .map_err(|err| format!("spawning cargo metadata: {err}"))?;
        if !output.status.success() {
            return Err(format!(
                "cargo metadata failed for `{}`: {}",
                self.name,
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        let text = String::from_utf8(output.stdout).map_err(|err| err.to_string())?;
        facet_json::from_str(&text).map_err(|err| format!("parse cargo metadata: {err}"))
    }

    /// Oracle 1: the lockfile's selection — every exact Cargo package cargo
    /// resolved (the same set `cargo generate-lockfile` writes to `Cargo.lock`).
    fn selection_oracle(&self, workspace: &Path) -> Result<SelectionOracle, String> {
        let metadata = self.cargo_metadata(workspace, None)?;
        let mut packages = BTreeSet::new();
        for package in &metadata.packages {
            packages.insert(package_id(package)?);
        }
        Ok(SelectionOracle { packages })
    }

    /// Oracle 2: the target-projected enabled graph. `cargo metadata
    /// --filter-platform <triple>` applies the cfg gate; the walk from the root
    /// over `normal`/`build` edges (dev excluded) is the same projection `cargo
    /// tree -e normal,build --target <triple>` shows.
    fn graph_oracle(&self, workspace: &Path, triple: &str) -> Result<GraphOracle, String> {
        let metadata = self.cargo_metadata(workspace, Some(triple))?;
        let mut by_id: BTreeMap<String, CargoPackageId> = BTreeMap::new();
        let mut package_by_id: BTreeMap<&str, &MetaPackage> = BTreeMap::new();
        for package in &metadata.packages {
            by_id.insert(package.id.clone(), package_id(package)?);
            package_by_id.insert(package.id.as_str(), package);
        }
        let exact = |id: &str| -> Result<CargoPackageId, String> {
            by_id
                .get(id)
                .cloned()
                .ok_or_else(|| format!("metadata id `{id}` absent from packages"))
        };
        let node_by_id: BTreeMap<&str, &MetaNode> = metadata
            .resolve
            .nodes
            .iter()
            .map(|node| (node.id.as_str(), node))
            .collect();
        let root_id = metadata
            .packages
            .iter()
            .find(|package| package.name == self.root)
            .map(|package| package.id.clone())
            .ok_or_else(|| format!("root package `{}` absent from cargo metadata", self.root))?;

        let mut nodes = BTreeSet::new();
        let mut edges = BTreeSet::new();
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut stack = vec![root_id.clone()];
        nodes.insert(exact(&root_id)?);
        seen.insert(root_id);
        while let Some(id) = stack.pop() {
            let Some(node) = node_by_id.get(id.as_str()) else {
                continue;
            };
            let package = package_by_id
                .get(id.as_str())
                .copied()
                .ok_or_else(|| format!("metadata id `{id}` has no package declaration"))?;
            let activated_optional = activated_optional_dependencies(package, node);
            let from = exact(&id)?;
            for dep in &node.deps {
                let mut kinds = BTreeSet::new();
                for dep_kind in &dep.dep_kinds {
                    if let Some(kind) = EdgeKind::classify(dep_kind.kind.as_deref())? {
                        let mut declarations = Vec::new();
                        for declaration in package
                            .dependencies
                            .iter()
                            .filter(|declaration| dependency_alias(declaration) == dep.name)
                        {
                            if EdgeKind::classify(declaration.kind.as_deref())? == Some(kind) {
                                declarations.push(declaration);
                            }
                        }
                        if declarations.is_empty() {
                            return Err(format!(
                                "resolved dependency {:?} kind {kind:?} has no package declaration",
                                dep.name
                            ));
                        }
                        if declarations.iter().all(|declaration| declaration.optional)
                            && !activated_optional.contains(&dep.name)
                        {
                            continue;
                        }
                        kinds.insert(kind);
                    }
                }
                if kinds.is_empty() {
                    // A dev-only edge under this platform — excluded by the oracle.
                    continue;
                }
                let to = exact(&dep.pkg)?;
                nodes.insert(to.clone());
                for kind in kinds {
                    edges.insert(GraphEdge {
                        from: from.clone(),
                        to: to.clone(),
                        kind,
                    });
                }
                if seen.insert(dep.pkg.clone()) {
                    stack.push(dep.pkg.clone());
                }
            }
        }
        Ok(GraphOracle {
            triple: triple.to_owned(),
            nodes,
            edges,
        })
    }
}

// ---------------------------------------------------------------------------
// The harness smoke test: prove the ported DSL -> workspace -> cargo -> parse
// path works end to end against the real cargo oracle (independent of rodin.vix).
// ---------------------------------------------------------------------------

#[test]
fn harness_materializes_workspace_and_cargo_resolves() {
    // A build-dependency is part of the host graph, consumed on every target.
    let fixture = Fixture::new("smoke-build-dep", "app")
        .krate(FixtureCrate::new("gen"))
        .krate(FixtureCrate::new("app").dep(FixtureDep::new("gen").kind(DepKind::Build)));
    let workspace = fixture.materialize().expect("materialize workspace");
    let selected = cargo_selected(&workspace, "app", LINUX).expect("cargo tree resolves");
    assert!(selected.contains("app"), "root present: {selected:?}");
    assert!(selected.contains("gen"), "build-dep consumed: {selected:?}");
}

// ---------------------------------------------------------------------------
// The differential corpus (rodin/docs/05-fixture-corpus.md). #[ignore]'d until
// rodin.vix implements the native resolver (rodin/docs/40-search.md); each
// asserts rodin.vix's per-target selection equals cargo's.
// ---------------------------------------------------------------------------

/// 1. Optional dep must not over-activate (the jiff-static shape): referenced
///    only by a non-default feature (`dep:helper`), a weak feature
///    (`helper?/tz-fat`), and an always-false `cfg(any())` edge. `helper` must be
///    selected on no target.
#[test]
fn cfg_any_and_weak_feature_never_pull_optional_dep() {
    let fixture = Fixture::new("cfg-any-never-dep", "app")
        .krate(FixtureCrate::new("helper").feature("tz-fat", &[]))
        .krate(
            FixtureCrate::new("lib")
                .feature("default", &["tz-fat"])
                .feature("tz-fat", &["helper?/tz-fat"])
                .feature("static-tz", &["dep:helper"])
                .dep(FixtureDep::new("helper").optional())
                .dep(FixtureDep::new("helper").target("cfg(any())")),
        )
        .krate(FixtureCrate::new("app").dep(FixtureDep::new("lib")));

    for target in [LINUX, WINDOWS] {
        let selected = fixture
            .assert_selection_matches(target)
            .expect("selection matches");
        assert!(
            !selected.contains("helper"),
            "no helper on {target}: {selected:?}"
        );
    }
}

#[test]
fn weak_feature_never_pulls_optional_dep_without_activation() {
    let fixture = Fixture::new("weak-feature-suppression", "app")
        .krate(FixtureCrate::new("helper").feature("extra", &[]))
        .krate(
            FixtureCrate::new("lib")
                .feature("default", &["weak"])
                .feature("weak", &["helper?/extra"])
                .dep(FixtureDep::new("helper").optional()),
        )
        .krate(FixtureCrate::new("app").dep(FixtureDep::new("lib")));

    let selected = fixture.assert_selection_matches(LINUX).unwrap();
    assert!(
        !selected.contains("helper"),
        "weak feature alone must not activate helper: {selected:?}"
    );
}

#[test]
fn feature_unification_pulls_superset_optional_deps() {
    let fixture = Fixture::new("feature-unification-superset", "app")
        .krate(FixtureCrate::new("left_dep"))
        .krate(FixtureCrate::new("right_dep"))
        .krate(
            FixtureCrate::new("shared")
                .feature("left", &["dep:left_dep"])
                .feature("right", &["dep:right_dep"])
                .dep(FixtureDep::new("left_dep").optional())
                .dep(FixtureDep::new("right_dep").optional()),
        )
        .krate(
            FixtureCrate::new("a").dep(
                FixtureDep::new("shared")
                    .default_features(false)
                    .feature("left"),
            ),
        )
        .krate(
            FixtureCrate::new("b").dep(
                FixtureDep::new("shared")
                    .default_features(false)
                    .feature("right"),
            ),
        )
        .krate(
            FixtureCrate::new("app")
                .dep(FixtureDep::new("a"))
                .dep(FixtureDep::new("b")),
        );

    let selected = fixture.assert_selection_matches(LINUX).unwrap();
    assert!(
        selected.contains("left_dep"),
        "left feature activation selected: {selected:?}"
    );
    assert!(
        selected.contains("right_dep"),
        "right feature activation selected: {selected:?}"
    );
}

/// 2. A default-on feature (`bundle-platform`) references an optional dep
///    declared only for `cfg(windows)`: active on windows, not linux.
#[test]
fn feature_activated_target_conditional_optional_dep() {
    let fixture = Fixture::new("feature-target-optional", "app")
        .krate(FixtureCrate::new("platform"))
        .krate(
            FixtureCrate::new("lib")
                .feature("default", &["bundle-platform"])
                .feature("bundle-platform", &["dep:platform"])
                .dep(
                    FixtureDep::new("platform")
                        .optional()
                        .target("cfg(windows)"),
                ),
        )
        .krate(FixtureCrate::new("app").dep(FixtureDep::new("lib")));

    assert!(
        fixture
            .assert_selection_matches(WINDOWS)
            .unwrap()
            .contains("platform")
    );
    assert!(
        !fixture
            .assert_selection_matches(LINUX)
            .unwrap()
            .contains("platform")
    );
}

/// 3. A plain `cfg(windows)` dependency edge (winapi shape): present on windows,
///    absent on linux.
#[test]
fn direct_target_conditional_edge() {
    let fixture = Fixture::new("direct-cfg-windows", "app")
        .krate(FixtureCrate::new("winthing"))
        .krate(FixtureCrate::new("app").dep(FixtureDep::new("winthing").target("cfg(windows)")));

    assert!(
        fixture
            .assert_selection_matches(WINDOWS)
            .unwrap()
            .contains("winthing")
    );
    assert!(
        !fixture
            .assert_selection_matches(LINUX)
            .unwrap()
            .contains("winthing")
    );
}

#[test]
fn cfg_combinators_and_key_values_follow_target_cfg_facts() {
    let fixture = Fixture::new("cfg-combinators-key-values", "app")
        .krate(FixtureCrate::new("unix_x64"))
        .krate(FixtureCrate::new("win_os"))
        .krate(
            FixtureCrate::new("app")
                .dep(
                    FixtureDep::new("unix_x64")
                        .target(r#"cfg(all(unix, target_arch = "x86_64", not(windows)))"#),
                )
                .dep(FixtureDep::new("win_os").target(r#"cfg(target_os = "windows")"#)),
        );

    let linux = fixture.assert_selection_matches(LINUX).unwrap();
    assert!(linux.contains("unix_x64"), "unix+x64 selected: {linux:?}");
    assert!(!linux.contains("win_os"), "windows dep absent: {linux:?}");

    let windows = fixture.assert_selection_matches(WINDOWS).unwrap();
    assert!(
        !windows.contains("unix_x64"),
        "unix dep absent: {windows:?}"
    );
    assert!(
        windows.contains("win_os"),
        "target_os windows selected: {windows:?}"
    );
}

/// 4. A build-dependency is part of the host graph and consumed on every target.
#[test]
fn build_dependency_is_consumed() {
    let fixture = Fixture::new("build-dep", "app")
        .krate(FixtureCrate::new("gen"))
        .krate(FixtureCrate::new("app").dep(FixtureDep::new("gen").kind(DepKind::Build)));

    assert!(
        fixture
            .assert_selection_matches(LINUX)
            .unwrap()
            .contains("gen")
    );
}

/// 5. A dev-dependency of a non-root crate is not consumed by a normal build of
///    the root.
#[test]
fn transitive_dev_dependency_is_not_consumed() {
    let fixture = Fixture::new("dev-dep", "app")
        .krate(FixtureCrate::new("testonly"))
        .krate(FixtureCrate::new("lib").dep(FixtureDep::new("testonly").kind(DepKind::Dev)))
        .krate(FixtureCrate::new("app").dep(FixtureDep::new("lib")));

    let selected = fixture.assert_selection_matches(LINUX).unwrap();
    assert!(
        !selected.contains("testonly"),
        "dev dep of lib not built: {selected:?}"
    );
}

#[test]
fn version_conflict_backtracks_and_installs_learned_no_good() {
    let fixture = Fixture::new("version-conflict-learn", "app")
        .krate(FixtureCrate::new("shared").version("1.0.0"))
        .krate(
            FixtureCrate::new("app")
                .version("0.1.0")
                .vix_version("0.2.0")
                .dep(FixtureDep::new("shared").req("=1.0.0"))
                .vix_version_dep("0.2.0", FixtureDep::new("shared").req("=2.0.0")),
        );

    let selected = fixture.assert_selection_matches(LINUX).unwrap();
    assert!(selected.contains("app"), "app selected: {selected:?}");
    assert!(selected.contains("shared"), "shared selected: {selected:?}");
    assert_eq!(
        fixture.vix_learned_count(LINUX).unwrap(),
        1,
        "the failed high lib candidate should install one active no-good"
    );
}

#[test]
fn rodin_linear_interiors_use_tail_loops() {
    let fixture = Fixture::new("tail-loop-trace", "app")
        .krate(FixtureCrate::new("leaf"))
        .krate(FixtureCrate::new("mid").dep(FixtureDep::new("leaf")))
        .krate(FixtureCrate::new("app").dep(FixtureDep::new("mid")));

    let enabled = fixture.rodin_trace_counts(LINUX, false).unwrap();
    let forced = fixture.rodin_trace_counts(LINUX, true).unwrap();
    assert!(
        enabled.total() < forced.total(),
        "tail-loop trace should shrink linear recursion events: enabled={enabled:?} forced={forced:?}"
    );
}

// ===========================================================================
// The live-Cargo oracle harness: production-shaped coverage.
//
// Every test drives a real, offline `cargo metadata` over a materialized path
// workspace and validates the typed extraction and the comparator. cargo is the
// only oracle; no recorded reference selection is consulted. Both oracles run on
// one shared materialized workspace so the path-source coordinate is comparable.
// ===========================================================================

const NATIVE_KERNEL_RED_FIXTURE: &str = r#"
fn native_line_input() -> SolveInput {
    let app = PackageId {
        source: PackageSource::Path("fixture/app"),
        name: "app",
        compat: CompatClass::ZeroMinor(1),
    };
    let row = PackageRow {
        package: app,
        version: parse_version("0.1.0"),
        dependencies: [],
        features: [],
        yanked: false,
        links: None,
        provenance: "fixture/app/Cargo.toml",
    };
    SolveInput {
        universe: PackageUniverse { rows: %{app => [row]} },
        roots: [RootRequest {
            package: app,
            requirement: parse_req("=0.1.0"),
            features: %[],
            default_features: true,
            graph: true,
        }],
        target: TargetFacts {
            triple: "x86_64-unknown-linux-gnu",
            atoms: %["unix"],
            values: %{"target_arch" => "x86_64", "target_os" => "linux"},
        },
        policy: SolvePolicy {
            consume_build: true,
            consume_dev: false,
            mutually_exclusive_features: %{},
        },
    }
}

#[test]
fn native_rodin_line() -> Stream<Check> {
    yield match rodin_solve(native_line_input()) {
        RodinOutcome::Solved(result) => expect_eq(result.selected.len(), 1),
        RodinOutcome::Failed(_) => expect(false),
        RodinOutcome::Unsupported(_) => expect(false),
    };
}
"#;

const NATIVE_KERNEL_CONFLICT_FIXTURE: &str = r#"
fn app_package() -> PackageId {
    PackageId {
        source: PackageSource::Path("fixture/app"),
        name: "app",
        compat: CompatClass::Major(1),
    }
}

fn shared_package() -> PackageId {
    PackageId {
        source: PackageSource::Path("fixture/shared"),
        name: "shared",
        compat: CompatClass::Major(1),
    }
}

fn native_target() -> TargetFacts {
    TargetFacts {
        triple: "x86_64-unknown-linux-gnu",
        atoms: %["unix"],
        values: %{"target_arch" => "x86_64", "target_os" => "linux"},
    }
}

fn empty_policy() -> SolvePolicy {
    SolvePolicy {
        consume_build: true,
        consume_dev: false,
        mutually_exclusive_features: %{},
    }
}

fn app_row(version: String) where { requirement: String } -> PackageRow {
    PackageRow {
        package: app_package(),
        version: parse_version(version),
        dependencies: [Dependency {
            package: shared_package(),
            requirement: parse_req(requirement),
            kind: DependencyKind::Normal,
            target: None,
            optional: false,
            default_features: true,
            features: %[],
        }],
        features: [],
        yanked: false,
        links: None,
        provenance: "fixture/app/Cargo.toml",
    }
}

fn shared_row() -> PackageRow {
    PackageRow {
        package: shared_package(),
        version: parse_version("1.0.0"),
        dependencies: [],
        features: [],
        yanked: false,
        links: None,
        provenance: "fixture/shared/Cargo.toml",
    }
}

fn search_input(include_low: Bool) -> SolveInput {
    let app_rows = if include_low {
        [
            app_row("1.0.0") where { requirement: "=1.0.0" },
            app_row("1.1.0") where { requirement: "=2.0.0" },
        ]
    } else {
        [app_row("1.1.0") where { requirement: "=2.0.0" }]
    };
    SolveInput {
        universe: PackageUniverse {
            rows: %{app_package() => app_rows, shared_package() => [shared_row()]},
        },
        roots: [RootRequest {
            package: app_package(),
            requirement: universe(),
            features: %[],
            default_features: true,
            graph: true,
        }],
        target: native_target(),
        policy: empty_policy(),
    }
}

#[test]
fn native_backtracking() -> Stream<Check> {
    yield match rodin_solve(search_input(true)) {
        RodinOutcome::Solved(result) => {
            yield expect_eq(result.selected.get(app_package()).version, parse_version("1.0.0"));
            yield expect_eq(result.selected.get(shared_package()).version, parse_version("1.0.0"));
            yield expect_eq(result.edges.len(), 1);
        },
        RodinOutcome::Failed(_) => expect(false),
        RodinOutcome::Unsupported(_) => expect(false),
    };
}

fn is_shared_exhaustion(conflict: Conflict) -> Bool {
    match conflict.cause {
        ConflictCause::EmptyDomain(_) => false,
        ConflictCause::NoCandidates(package) => package == shared_package(),
        ConflictCause::FeatureExclusion { package: _, left: _, right: _ } => false,
        ConflictCause::LinksCollision { links: _, left: _, right: _ } => false,
    }
}

#[test]
fn native_exhaustion_conflict() -> Stream<Check> {
    yield match rodin_solve(search_input(false)) {
        RodinOutcome::Solved(_) => expect(false),
        RodinOutcome::Unsupported(_) => expect(false),
        RodinOutcome::Failed(conflict) => expect(is_shared_exhaustion(conflict)),
    };
}

fn feature_conflict_package() -> PackageId {
    PackageId {
        source: PackageSource::Path("fixture/feature-conflict"),
        name: "feature-conflict",
        compat: CompatClass::ZeroMinor(1),
    }
}

fn feature_conflict_input() -> SolveInput {
    let package = feature_conflict_package();
    let row = PackageRow {
        package,
        version: parse_version("0.1.0"),
        dependencies: [],
        features: [],
        yanked: false,
        links: None,
        provenance: "fixture/feature-conflict/Cargo.toml",
    };
    SolveInput {
        universe: PackageUniverse { rows: %{package => [row]} },
        roots: [RootRequest {
            package,
            requirement: universe(),
            features: %["left", "right"],
            default_features: false,
            graph: true,
        }],
        target: native_target(),
        policy: SolvePolicy {
            consume_build: true,
            consume_dev: false,
            mutually_exclusive_features: %{package => [%["left", "right"]]},
        },
    }
}

fn is_feature_conflict(conflict: Conflict) -> Bool {
    match conflict.cause {
        ConflictCause::EmptyDomain(_) => false,
        ConflictCause::NoCandidates(_) => false,
        ConflictCause::FeatureExclusion { package, left: _, right: _ } => {
            package == feature_conflict_package()
        },
        ConflictCause::LinksCollision { links: _, left: _, right: _ } => false,
    }
}

#[test]
fn native_feature_conflict() -> Stream<Check> {
    yield match rodin_solve(feature_conflict_input()) {
        RodinOutcome::Solved(_) => expect(false),
        RodinOutcome::Unsupported(_) => expect(false),
        RodinOutcome::Failed(conflict) => expect(is_feature_conflict(conflict)),
    };
}

fn links_package(name: String) -> PackageId {
    PackageId {
        source: PackageSource::Path("fixture/links"),
        name,
        compat: CompatClass::ZeroMinor(1),
    }
}

fn links_row(name: String) -> PackageRow {
    PackageRow {
        package: links_package(name),
        version: parse_version("0.1.0"),
        dependencies: [],
        features: [],
        yanked: false,
        links: Some("native"),
        provenance: "fixture/links/Cargo.toml",
    }
}

fn links_conflict_input() -> SolveInput {
    let left = links_package("left");
    let right = links_package("right");
    SolveInput {
        universe: PackageUniverse {
            rows: %{left => [links_row("left")], right => [links_row("right")]},
        },
        roots: [
            RootRequest {
                package: left,
                requirement: universe(),
                features: %[],
                default_features: false,
                graph: true,
            },
            RootRequest {
                package: right,
                requirement: universe(),
                features: %[],
                default_features: false,
                graph: false,
            },
        ],
        target: native_target(),
        policy: empty_policy(),
    }
}

fn is_links_conflict(conflict: Conflict) -> Bool {
    match conflict.cause {
        ConflictCause::EmptyDomain(_) => false,
        ConflictCause::NoCandidates(_) => false,
        ConflictCause::FeatureExclusion { package: _, left: _, right: _ } => false,
        ConflictCause::LinksCollision { links, left: _, right: _ } => links == "native",
    }
}

#[test]
fn native_links_conflict() -> Stream<Check> {
    yield match rodin_solve(links_conflict_input()) {
        RodinOutcome::Solved(_) => expect(false),
        RodinOutcome::Unsupported(_) => expect(false),
        RodinOutcome::Failed(conflict) => expect(is_links_conflict(conflict)),
    };
}
"#;

#[test]
fn native_rodin_kernel_executes_typed_line_input() {
    let source = format!("{STD_VERSION}\n{NATIVE_RODIN_KERNEL}\n{NATIVE_KERNEL_RED_FIXTURE}");
    let report = run_source(&source).expect("native Rodin kernel runs through production Vix");
    assert!(report.passed(), "typed line solve passes: {report:?}");
    assert!(report.agrees(), "plain and chaos agree");
    assert_eq!(report.plain.checks.len(), 1);
    assert_eq!(report.plain.counters.pure_host_calls, 0);
    assert_eq!(report.plain.receipt_count, 0);
    assert_eq!(report.chaos.counters.pure_host_calls, 0);
    assert_eq!(report.chaos.receipt_count, 0);
}

#[test]
fn native_rodin_kernel_backtracks_and_returns_typed_conflicts() {
    let source = format!("{STD_VERSION}\n{NATIVE_RODIN_KERNEL}\n{NATIVE_KERNEL_CONFLICT_FIXTURE}");
    let report = run_source(&source).expect("native Rodin search and conflicts execute");
    assert!(
        report.passed(),
        "search/conflict certificates pass: {report:?}"
    );
    assert!(report.agrees(), "plain and chaos agree");
    assert_eq!(report.plain.checks.len(), 6);
    for lane in [&report.plain, &report.chaos] {
        assert_eq!(lane.counters.pure_host_calls, 0);
        assert_eq!(lane.receipt_count, 0);
    }
}

#[derive(Facet, Clone, Debug)]
struct FunctionFrameCount {
    function: String,
    entries: u64,
}

#[derive(Facet, Clone, Debug)]
struct NativeKernelScaleProfile {
    packages: u64,
    prepare_micros: u64,
    execute_micros: u64,
    scheduler_requests: u64,
    task_spawns: u64,
    task_discards: u64,
    native_task_spawns: u64,
    interpreter_task_spawns: u64,
    memo_hits_exact: u64,
    memo_misses: u64,
    store_interns: u64,
    store_dedups: u64,
    bytes_hashed: u64,
    framed_bytes: u64,
    value_island_spawns: u64,
    successful_aggregate_freezes: u64,
    active_molten_selections: u64,
    forced_copy_selections: u64,
    peak_molten_bytes: u64,
    peak_molten_nodes: u64,
    frame_entries: u64,
    event_count: u64,
    lowering_hits: u64,
    lowering_misses: u64,
    function_frames: Vec<FunctionFrameCount>,
}

fn native_scale_source(packages: usize) -> String {
    assert!(packages > 0, "scale fixture needs at least one package");
    let mut fixture = String::new();
    fixture.push_str("fn native_scale_input() -> SolveInput {\n");
    for index in 0..packages {
        writeln!(&mut fixture, "    let p{index} = PackageId {{")
            .expect("write package declaration");
        writeln!(
            &mut fixture,
            "        source: PackageSource::Path(\"fixture/scale/p{index}\"),"
        )
        .expect("write package source");
        writeln!(&mut fixture, "        name: \"p{index}\",").expect("write package name");
        fixture.push_str("        compat: CompatClass::ZeroMinor(1),\n    };\n");
    }
    for index in 0..packages {
        writeln!(&mut fixture, "    let r{index} = PackageRow {{").expect("write row declaration");
        writeln!(&mut fixture, "        package: p{index},").expect("write row package");
        fixture.push_str("        version: parse_version(\"0.1.0\"),\n");
        if index + 1 == packages {
            fixture.push_str("        dependencies: [],\n");
        } else {
            fixture.push_str("        dependencies: [Dependency {\n");
            writeln!(&mut fixture, "            package: p{},", index + 1)
                .expect("write dependency package");
            fixture.push_str(concat!(
                "            requirement: parse_req(\"=0.1.0\"),\n",
                "            kind: DependencyKind::Normal,\n",
                "            target: None,\n",
                "            optional: false,\n",
                "            default_features: false,\n",
                "            features: %[],\n",
                "        }],\n",
            ));
        }
        fixture.push_str(concat!(
            "        features: [],\n",
            "        yanked: false,\n",
            "        links: None,\n",
        ));
        writeln!(
            &mut fixture,
            "        provenance: \"fixture/scale/p{index}/Cargo.toml\","
        )
        .expect("write provenance");
        fixture.push_str("    };\n");
    }
    fixture.push_str("    SolveInput {\n        universe: PackageUniverse { rows: %{");
    for index in 0..packages {
        if index > 0 {
            fixture.push_str(", ");
        }
        write!(&mut fixture, "p{index} => [r{index}]").expect("write universe row");
    }
    fixture.push_str(concat!(
        "} },\n",
        "        roots: [RootRequest {\n",
        "            package: p0,\n",
        "            requirement: parse_req(\"=0.1.0\"),\n",
        "            features: %[],\n",
        "            default_features: false,\n",
        "            graph: true,\n",
        "        }],\n",
        "        target: TargetFacts {\n",
        "            triple: \"x86_64-unknown-linux-gnu\",\n",
        "            atoms: %[\"unix\"],\n",
        "            values: %{\"target_arch\" => \"x86_64\", \"target_os\" => \"linux\"},\n",
        "        },\n",
        "        policy: SolvePolicy {\n",
        "            consume_build: true,\n",
        "            consume_dev: false,\n",
        "            mutually_exclusive_features: %{},\n",
        "        },\n",
        "    }\n",
        "}\n\n",
        "#[test]\n",
        "fn native_scale() -> Stream<Check> {\n",
        "    yield match rodin_solve(native_scale_input()) {\n",
    ));
    writeln!(
        &mut fixture,
        "        RodinOutcome::Solved(result) => expect(result.selected.len() == {packages} && result.edges.len() == {}),",
        packages - 1
    )
    .expect("write scale expectation");
    fixture.push_str(concat!(
        "        RodinOutcome::Failed(_) => expect(false),\n",
        "        RodinOutcome::Unsupported(_) => expect(false),\n",
        "    };\n",
        "}\n",
    ));
    format!("{STD_VERSION}\n{NATIVE_RODIN_KERNEL}\n{fixture}")
}

fn micros(elapsed: std::time::Duration) -> u64 {
    u64::try_from(elapsed.as_micros()).unwrap_or(u64::MAX)
}

fn native_scale_profile(
    packages: usize,
    lane: LaneRequest,
) -> (NativeKernelScaleProfile, vix::ratchet::RatchetReport) {
    let source = native_scale_source(packages);
    let prepare_started = Instant::now();
    let prepared = match lane {
        LaneRequest::Auto => prepare_source(&source),
        LaneRequest::Interpreter | LaneRequest::Native => prepare_source_with_lane(&source, lane),
    }
    .unwrap_or_else(|error| panic!("prepare {packages}-package native kernel: {error:?}"));
    let prepare_micros = micros(prepare_started.elapsed());
    let execute_started = Instant::now();
    let report = prepared
        .execute()
        .unwrap_or_else(|error| panic!("execute {packages}-package native kernel: {error:?}"));
    let execute_micros = micros(execute_started.elapsed());
    assert!(report.passed(), "{packages}-package scale solve passes");
    assert!(report.agrees(), "{packages}-package plain and chaos agree");
    assert_eq!(report.plain.checks.len(), 1);
    assert_eq!(report.plain.counters.pure_host_calls, 0);
    assert_eq!(report.plain.receipt_count, 0);
    let counters = report.plain.counters;
    let function_frames = if std::env::var_os("VIX_RODIN_SCALE_PROFILE").is_some() {
        let compilation = Compiler::new()
            .compile(&source)
            .expect("compile scale source for function attribution");
        let names = compilation
            .module
            .functions
            .iter()
            .map(|function| (function.id, function.name.as_str()))
            .collect::<BTreeMap<_, _>>();
        let mut counts = BTreeMap::new();
        for event in &report.plain.events {
            if let EventKind::WeavyFrameEntered { function, .. } = &event.kind {
                *counts.entry(*function).or_insert(0u64) += 1;
            }
        }
        let mut counts = counts
            .into_iter()
            .map(|(function, entries)| FunctionFrameCount {
                function: names
                    .get(&function)
                    .copied()
                    .unwrap_or("<lowered synthetic>")
                    .to_owned(),
                entries,
            })
            .collect::<Vec<_>>();
        counts.sort_by(|left, right| {
            right
                .entries
                .cmp(&left.entries)
                .then_with(|| left.function.cmp(&right.function))
        });
        counts
    } else {
        Vec::new()
    };
    let profile = NativeKernelScaleProfile {
        packages: packages.try_into().expect("package count fits u64"),
        prepare_micros,
        execute_micros,
        scheduler_requests: counters.scheduler_requests,
        task_spawns: counters.task_spawns,
        task_discards: counters.task_discards,
        native_task_spawns: counters.native_task_spawns,
        interpreter_task_spawns: counters.interpreter_task_spawns,
        memo_hits_exact: counters.memo_hits_exact,
        memo_misses: counters.memo_misses,
        store_interns: counters.store_interns,
        store_dedups: counters.store_dedups,
        bytes_hashed: counters.bytes_hashed,
        framed_bytes: counters.framed_bytes,
        value_island_spawns: counters.value_island_spawns,
        successful_aggregate_freezes: counters.successful_aggregate_freezes,
        active_molten_selections: counters.active_molten_selections,
        forced_copy_selections: counters.forced_copy_selections,
        peak_molten_bytes: counters.peak_molten_bytes,
        peak_molten_nodes: counters.peak_molten_nodes,
        frame_entries: report
            .plain
            .events
            .iter()
            .filter(|event| matches!(event.kind, EventKind::WeavyFrameEntered { .. }))
            .count()
            .try_into()
            .expect("frame count fits u64"),
        event_count: report
            .plain
            .events
            .len()
            .try_into()
            .expect("event count fits u64"),
        lowering_hits: report.lowering_cache.hits,
        lowering_misses: report.lowering_cache.misses,
        function_frames,
    };
    (profile, report)
}

fn maybe_write_scale_profiles(profiles: &[NativeKernelScaleProfile]) {
    let Some(path) = std::env::var_os("VIX_RODIN_SCALE_PROFILE") else {
        return;
    };
    let encoded = facet_json::to_string_pretty(profiles).expect("encode Rodin scale profile");
    std::fs::write(path, encoded).expect("write Rodin scale profile");
}

#[test]
fn native_rodin_kernel_scale_keeps_one_demand_shape() {
    let mut profiles = Vec::new();
    let packages = std::env::var("VIX_RODIN_SCALE_PACKAGES")
        .map(|value| {
            vec![
                value
                    .parse::<usize>()
                    .expect("VIX_RODIN_SCALE_PACKAGES is a positive integer"),
            ]
        })
        .unwrap_or_else(|_| vec![4, 16, 64]);
    for packages in packages {
        profiles.push(native_scale_profile(packages, LaneRequest::Auto).0);
    }
    maybe_write_scale_profiles(&profiles);

    let demand_shape = (
        profiles[0].scheduler_requests,
        profiles[0].task_spawns,
        profiles[0].memo_misses,
        profiles[0].lowering_misses,
    );
    assert_eq!(demand_shape, (3, 3, 2, 5));
    for profile in &profiles {
        assert_eq!(
            (
                profile.scheduler_requests,
                profile.task_spawns,
                profile.memo_misses,
                profile.lowering_misses,
            ),
            demand_shape,
            "package count must not create scheduler tasks or lowering units"
        );
        assert_eq!(profile.memo_hits_exact, 0);
        assert_eq!(profile.task_discards, 0);
        assert_eq!(profile.value_island_spawns, 1);
        assert_eq!(profile.successful_aggregate_freezes, 1);
        assert_eq!(profile.active_molten_selections, 1);
        assert_eq!(profile.forced_copy_selections, 0);
        assert!(
            profile.store_interns <= profile.packages * 4 + 32,
            "Store interns must remain linear: {profile:?}"
        );
        assert!(
            profile.store_dedups <= profile.packages * 3 + 32,
            "Store deduplications must remain linear: {profile:?}"
        );
        assert!(
            profile.bytes_hashed <= profile.packages * 64 + 256,
            "identity hashing must remain linear: {profile:?}"
        );
        assert!(
            profile.framed_bytes <= profile.packages * 56,
            "published semantic bytes must remain linear: {profile:?}"
        );
        assert!(
            profile.peak_molten_bytes <= profile.packages * 8_000,
            "peak molten bytes must remain linear: {profile:?}"
        );
        assert!(
            profile.peak_molten_nodes <= profile.packages * 130,
            "peak molten nodes must remain linear: {profile:?}"
        );
        assert!(
            profile.frame_entries <= profile.packages * 160,
            "verified function entries must remain linear: {profile:?}"
        );
        assert!(
            profile.event_count <= profile.packages * 340,
            "bounded Production events must remain linear: {profile:?}"
        );
    }
    for pair in profiles.windows(2) {
        let [smaller, larger] = pair else {
            unreachable!("windows(2) has two profiles")
        };
        assert!(smaller.store_interns <= larger.store_interns);
        assert!(smaller.store_dedups <= larger.store_dedups);
        assert!(smaller.framed_bytes <= larger.framed_bytes);
        assert!(smaller.peak_molten_bytes <= larger.peak_molten_bytes);
        assert!(smaller.peak_molten_nodes <= larger.peak_molten_nodes);
        assert!(smaller.frame_entries <= larger.frame_entries);
    }
}

#[test]
fn native_rodin_kernel_scale_agrees_across_execution_lanes() {
    if !weavy::jit::task_lane::available() {
        return;
    }
    let (native_profile, native) = native_scale_profile(16, LaneRequest::Native);
    let (interpreter_profile, interpreter) = native_scale_profile(16, LaneRequest::Interpreter);
    assert_eq!(native.plain.checks, interpreter.plain.checks);
    assert_eq!(native.plain.values, interpreter.plain.values);
    assert_eq!(
        native_profile.native_task_spawns,
        native_profile.task_spawns
    );
    assert_eq!(native_profile.interpreter_task_spawns, 0);
    assert_eq!(interpreter_profile.native_task_spawns, 0);
    assert_eq!(
        interpreter_profile.interpreter_task_spawns,
        interpreter_profile.task_spawns
    );
}

fn vix_compat_class(version: &str) -> Result<String, String> {
    let version =
        SemVer::parse(version).map_err(|err| format!("parse version `{version}`: {err}"))?;
    let component = |value: u64| {
        i64::try_from(value).map_err(|_| format!("version component {value} exceeds Vix Int"))
    };
    if version.major != 0 {
        Ok(format!("CompatClass::Major({})", component(version.major)?))
    } else if version.minor != 0 {
        Ok(format!(
            "CompatClass::ZeroMinor({})",
            component(version.minor)?
        ))
    } else {
        Ok(format!(
            "CompatClass::ZeroZeroPatch({})",
            component(version.patch)?
        ))
    }
}

fn vix_package_source(source: &Source) -> Result<String, String> {
    match source {
        Source::Path { dir } => Ok(format!("PackageSource::Path({})", vix_string(dir))),
        Source::Registry { spec } => Ok(format!("PackageSource::Registry({})", vix_string(spec))),
        Source::Git { spec } => {
            let body = spec
                .strip_prefix("git+")
                .ok_or_else(|| format!("git source lacks git+ prefix: {spec:?}"))?;
            let (url, rev) = body
                .rsplit_once('#')
                .ok_or_else(|| format!("git source lacks resolved revision: {spec:?}"))?;
            Ok(format!(
                "PackageSource::Git {{ url: {}, rev: {} }}",
                vix_string(url),
                vix_string(rev)
            ))
        }
    }
}

fn vix_package_id(package: &CargoPackageId) -> Result<String, String> {
    Ok(format!(
        "PackageId {{ source: {}, name: {}, compat: {} }}",
        vix_package_source(&package.source)?,
        vix_string(&package.name),
        vix_compat_class(&package.version)?
    ))
}

#[derive(Clone, Debug)]
enum NativeCfgExpr {
    Atom(String),
    Value { key: String, value: String },
    Not(Box<NativeCfgExpr>),
    All(Vec<NativeCfgExpr>),
    Any(Vec<NativeCfgExpr>),
}

struct NativeCfgParser<'a> {
    input: &'a str,
    offset: usize,
}

impl<'a> NativeCfgParser<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, offset: 0 }
    }

    fn parse(mut self) -> Result<NativeCfgExpr, String> {
        let expression = self.expression()?;
        self.whitespace();
        if self.offset != self.input.len() {
            return Err(format!(
                "unexpected cfg suffix {:?}",
                &self.input[self.offset..]
            ));
        }
        Ok(expression)
    }

    fn expression(&mut self) -> Result<NativeCfgExpr, String> {
        self.whitespace();
        let identifier = self.identifier()?;
        self.whitespace();
        if self.consume('(') {
            let mut arguments = Vec::new();
            self.whitespace();
            if !self.consume(')') {
                loop {
                    arguments.push(self.expression()?);
                    self.whitespace();
                    if self.consume(')') {
                        break;
                    }
                    self.expect(',')?;
                }
            }
            return match identifier.as_str() {
                "all" => Ok(NativeCfgExpr::All(arguments)),
                "any" => Ok(NativeCfgExpr::Any(arguments)),
                "not" if arguments.len() == 1 => Ok(NativeCfgExpr::Not(Box::new(
                    arguments.pop().expect("length checked"),
                ))),
                "not" => Err("cfg not() requires exactly one operand".to_owned()),
                other => Err(format!("unsupported cfg operator {other:?}")),
            };
        }
        if self.consume('=') {
            self.whitespace();
            return Ok(NativeCfgExpr::Value {
                key: identifier,
                value: self.quoted_string()?,
            });
        }
        Ok(NativeCfgExpr::Atom(identifier))
    }

    fn identifier(&mut self) -> Result<String, String> {
        let start = self.offset;
        while self
            .input
            .as_bytes()
            .get(self.offset)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
        {
            self.offset += 1;
        }
        if start == self.offset {
            return Err(format!(
                "expected cfg identifier at {:?}",
                &self.input[self.offset..]
            ));
        }
        Ok(self.input[start..self.offset].to_owned())
    }

    fn quoted_string(&mut self) -> Result<String, String> {
        self.expect('"')?;
        let start = self.offset;
        while self.input.as_bytes().get(self.offset).copied() != Some(b'"') {
            if self.offset == self.input.len() {
                return Err("unterminated cfg string".to_owned());
            }
            self.offset += 1;
        }
        let value = self.input[start..self.offset].to_owned();
        self.offset += 1;
        Ok(value)
    }

    fn whitespace(&mut self) {
        while self
            .input
            .as_bytes()
            .get(self.offset)
            .is_some_and(u8::is_ascii_whitespace)
        {
            self.offset += 1;
        }
    }

    fn consume(&mut self, expected: char) -> bool {
        if self.input[self.offset..].starts_with(expected) {
            self.offset += expected.len_utf8();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, expected: char) -> Result<(), String> {
        if self.consume(expected) {
            Ok(())
        } else {
            Err(format!(
                "expected {expected:?} at {:?}",
                &self.input[self.offset..]
            ))
        }
    }
}

#[derive(Clone, Debug, Default)]
struct NativeCfgConjunction {
    required_atoms: BTreeSet<String>,
    forbidden_atoms: BTreeSet<String>,
    required_values: BTreeMap<String, String>,
}

fn cfg_dnf(expression: &NativeCfgExpr) -> Result<Vec<NativeCfgConjunction>, String> {
    match expression {
        NativeCfgExpr::Atom(atom) => Ok(vec![NativeCfgConjunction {
            required_atoms: [atom.clone()].into_iter().collect(),
            ..NativeCfgConjunction::default()
        }]),
        NativeCfgExpr::Value { key, value } => Ok(vec![NativeCfgConjunction {
            required_values: [(key.clone(), value.clone())].into_iter().collect(),
            ..NativeCfgConjunction::default()
        }]),
        NativeCfgExpr::Not(inner) => match inner.as_ref() {
            NativeCfgExpr::Atom(atom) => Ok(vec![NativeCfgConjunction {
                forbidden_atoms: [atom.clone()].into_iter().collect(),
                ..NativeCfgConjunction::default()
            }]),
            _ => Err("normalized target facts cannot represent not(key = value)".to_owned()),
        },
        NativeCfgExpr::Any(arguments) => {
            let mut out = Vec::new();
            for argument in arguments {
                out.extend(cfg_dnf(argument)?);
            }
            Ok(out)
        }
        NativeCfgExpr::All(arguments) => {
            let mut out = vec![NativeCfgConjunction::default()];
            for argument in arguments {
                let terms = cfg_dnf(argument)?;
                let mut product = Vec::new();
                for left in &out {
                    for right in &terms {
                        let mut merged = left.clone();
                        merged.required_atoms.extend(right.required_atoms.clone());
                        merged.forbidden_atoms.extend(right.forbidden_atoms.clone());
                        for (key, value) in &right.required_values {
                            if let Some(previous) =
                                merged.required_values.insert(key.clone(), value.clone())
                                && previous != *value
                            {
                                return Err(format!(
                                    "cfg conjunction requires two values for {key:?}"
                                ));
                            }
                        }
                        product.push(merged);
                    }
                }
                out = product;
            }
            Ok(out)
        }
    }
}

fn vix_target_predicate(target: &str) -> Result<String, String> {
    if !target.starts_with("cfg(") {
        let target = vix_string(target);
        return Ok(format!(
            "TargetPredicate {{ triples: %[{target}], any: [] }}"
        ));
    }
    let inner = target
        .strip_prefix("cfg(")
        .and_then(|value| value.strip_suffix(')'))
        .ok_or_else(|| format!("malformed cfg predicate {target:?}"))?;
    let mut terms = cfg_dnf(&NativeCfgParser::new(inner).parse()?)?;
    if terms.is_empty() {
        terms.push(NativeCfgConjunction {
            required_atoms: ["__rodin_never__".to_owned()].into_iter().collect(),
            forbidden_atoms: ["__rodin_never__".to_owned()].into_iter().collect(),
            ..NativeCfgConjunction::default()
        });
    }
    let terms = terms
        .into_iter()
        .map(|term| {
            let values = term
                .required_values
                .iter()
                .map(|(key, value)| format!("{} => {}", vix_string(key), vix_string(value)))
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "CfgConjunction {{ required_atoms: {}, forbidden_atoms: {}, required_values: %{{{values}}} }}",
                vix_string_set(term.required_atoms),
                vix_string_set(term.forbidden_atoms),
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    Ok(format!(
        "TargetPredicate {{ triples: %[], any: [{terms}] }}"
    ))
}

fn vix_dependency_kind(kind: DepKind) -> &'static str {
    match kind {
        DepKind::Normal => "DependencyKind::Normal",
        DepKind::Build => "DependencyKind::Build",
        DepKind::Dev => "DependencyKind::Dev",
    }
}

fn vix_edge_kind(kind: EdgeKind) -> &'static str {
    match kind {
        EdgeKind::Normal => "DependencyKind::Normal",
        EdgeKind::Build => "DependencyKind::Build",
    }
}

fn vix_string_set(values: impl IntoIterator<Item = impl AsRef<str>>) -> String {
    let values = values
        .into_iter()
        .map(|value| vix_string(value.as_ref()))
        .collect::<Vec<_>>()
        .join(", ");
    format!("%[{values}]")
}

fn vix_feature_rules(
    krate: &FixtureCrate,
    by_name: &BTreeMap<String, CargoPackageId>,
    package_fn: &BTreeMap<CargoPackageId, String>,
) -> Result<String, String> {
    let dependency = |name: &str| -> Result<(&FixtureDep, &str), String> {
        krate
            .deps
            .iter()
            .find(|dependency| dependency.name == name)
            .map(|dependency| (dependency, dependency.name.as_str()))
            .ok_or_else(|| {
                format!(
                    "feature of {:?} names absent dependency {name:?}",
                    krate.name
                )
            })
    };
    let effect_for = |enable: &str| -> Result<String, String> {
        if let Some(name) = enable.strip_prefix("dep:") {
            let (dependency, name) = dependency(name)?;
            let package = &package_fn[&by_name[name]];
            return Ok(format!(
                "FeatureEffect::Activate {{ package: {package}, kind: {} }}",
                vix_dependency_kind(dependency.kind)
            ));
        }
        if let Some((name, feature)) = enable.split_once("?/") {
            let (dependency, name) = dependency(name)?;
            let package = &package_fn[&by_name[name]];
            return Ok(format!(
                "FeatureEffect::Weak {{ package: {package}, kind: {}, feature: {} }}",
                vix_dependency_kind(dependency.kind),
                vix_string(feature)
            ));
        }
        if let Some((name, feature)) = enable.split_once('/') {
            let (dependency, name) = dependency(name)?;
            let package = &package_fn[&by_name[name]];
            return Ok(format!(
                "FeatureEffect::Strong {{ package: {package}, kind: {}, feature: {} }}",
                vix_dependency_kind(dependency.kind),
                vix_string(feature)
            ));
        }
        Ok(format!("FeatureEffect::Local({})", vix_string(enable)))
    };

    let mut rules = Vec::new();
    for (name, enables) in &krate.features {
        let effects = enables
            .iter()
            .map(|enable| effect_for(enable))
            .collect::<Result<Vec<_>, _>>()?
            .join(", ");
        rules.push(format!(
            "FeatureRule {{ name: {}, effects: [{effects}] }}",
            vix_string(name)
        ));
    }
    let explicit = explicit_optional_dep_features(krate);
    for dependency in &krate.deps {
        if dependency.optional && !explicit.contains(&dependency.name) {
            let package = &package_fn[&by_name[&dependency.name]];
            rules.push(format!(
                "FeatureRule {{ name: {}, effects: [FeatureEffect::Activate {{ package: {package}, kind: {} }}] }}",
                vix_string(&dependency.name),
                vix_dependency_kind(dependency.kind)
            ));
        }
    }
    Ok(format!("[{}]", rules.join(", ")))
}

fn native_target_facts(triple: &str) -> Result<String, String> {
    let (atom, os) = match triple {
        LINUX => ("unix", "linux"),
        WINDOWS => ("windows", "windows"),
        other => return Err(format!("fixture adapter has no target facts for {other:?}")),
    };
    let triple = vix_string(triple);
    let atom = vix_string(atom);
    let arch_key = vix_string("target_arch");
    let arch = vix_string("x86_64");
    let os_key = vix_string("target_os");
    let os = vix_string(os);
    Ok(format!(
        "TargetFacts {{ triple: {triple}, atoms: %[{atom}], values: %{{{arch_key} => {arch}, {os_key} => {os}}} }}"
    ))
}

impl Fixture {
    fn native_kernel_oracle_source(
        &self,
        workspace: &Path,
        triple: &str,
    ) -> Result<String, String> {
        let selection = self.selection_oracle(workspace)?;
        let graph = self.graph_oracle(workspace, triple)?;
        let mut by_name = BTreeMap::new();
        for package in &selection.packages {
            if let Some(previous) = by_name.insert(package.name.clone(), package.clone()) {
                return Err(format!(
                    "fixture adapter cannot collapse same-name packages: {previous:?} and {package:?}"
                ));
            }
        }
        for krate in &self.crates {
            if !by_name.contains_key(&krate.name) {
                return Err(format!(
                    "Cargo selection omitted fixture crate {:?}",
                    krate.name
                ));
            }
        }

        let mut source = String::new();
        for (index, package) in selection.packages.iter().enumerate() {
            writeln!(
                source,
                "fn cargo_package_{index}() -> PackageId {{ {} }}",
                vix_package_id(package)?
            )
            .ok();
        }
        let package_fn = selection
            .packages
            .iter()
            .enumerate()
            .map(|(index, package)| (package.clone(), format!("cargo_package_{index}()")))
            .collect::<BTreeMap<_, _>>();

        writeln!(source, "\nfn cargo_fixture_input() -> SolveInput {{").ok();
        writeln!(source, "    SolveInput {{").ok();
        writeln!(source, "        universe: PackageUniverse {{ rows: %{{").ok();
        for krate in &self.crates {
            let package = by_name
                .get(&krate.name)
                .ok_or_else(|| format!("fixture package {:?} is absent", krate.name))?;
            let package_expr = &package_fn[package];
            let dependencies = krate
                .deps
                .iter()
                .map(|dependency| {
                    let target = by_name.get(&dependency.name).ok_or_else(|| {
                        format!("dependency package {:?} is absent", dependency.name)
                    })?;
                    let target_predicate = match &dependency.target {
                        None => "None".to_owned(),
                        Some(target) => format!("Some({})", vix_target_predicate(target)?),
                    };
                    Ok(format!(
                        "Dependency {{ package: {}, requirement: parse_req({}), kind: {}, target: {target_predicate}, optional: {}, default_features: {}, features: {} }}",
                        package_fn[target],
                        vix_string(&dependency.req),
                        vix_dependency_kind(dependency.kind),
                        dependency.optional,
                        dependency.default_features,
                        vix_string_set(&dependency.features),
                    ))
                })
                .collect::<Result<Vec<_>, String>>()?
                .join(", ");
            let features = vix_feature_rules(krate, &by_name, &package_fn)?;
            writeln!(
                source,
                "            {package_expr} => [PackageRow {{ package: {package_expr}, version: parse_version({}), dependencies: [{dependencies}], features: {features}, yanked: false, links: None, provenance: {} }}],",
                vix_string(&package.version),
                vix_string(&format!("{}/Cargo.toml", krate.name)),
            )
            .ok();
        }
        writeln!(source, "        }} }},").ok();
        writeln!(source, "        roots: [").ok();
        for krate in &self.crates {
            let package = by_name
                .get(&krate.name)
                .ok_or_else(|| format!("fixture package {:?} is absent", krate.name))?;
            let graph = krate.name == self.root;
            writeln!(
                source,
                "            RootRequest {{ package: {}, requirement: parse_req({}), features: %[], default_features: {graph}, graph: {graph} }},",
                package_fn[package],
                vix_string(&format!("={}", package.version)),
            )
            .ok();
        }
        writeln!(source, "        ],").ok();
        writeln!(source, "        target: {},", native_target_facts(triple)?).ok();
        writeln!(
            source,
            "        policy: SolvePolicy {{ consume_build: true, consume_dev: false, mutually_exclusive_features: %{{}} }},"
        )
        .ok();
        writeln!(source, "    }}").ok();
        writeln!(source, "}}").ok();

        writeln!(
            source,
            "\nfn cargo_expected_versions() -> Map<PackageId, Version> {{ %{{"
        )
        .ok();
        for package in &selection.packages {
            writeln!(
                source,
                "    {} => parse_version({}),",
                package_fn[package],
                vix_string(&package.version)
            )
            .ok();
        }
        writeln!(source, "}} }}").ok();

        writeln!(
            source,
            "\nfn cargo_expected_edges() -> Set<ResolvedEdge> {{ %["
        )
        .ok();
        for edge in &graph.edges {
            writeln!(
                source,
                "    ResolvedEdge {{ from: {}, to: {}, kind: {} }},",
                package_fn[&edge.from],
                package_fn[&edge.to],
                vix_edge_kind(edge.kind),
            )
            .ok();
        }
        writeln!(source, "] }}").ok();

        source.push_str(
            r#"
#[test]
fn native_cargo_oracles() -> Stream<Check> {
    yield match rodin_solve(cargo_fixture_input()) {
        RodinOutcome::Solved(result) => {
            let expected = cargo_expected_versions();
            let expected_edges = cargo_expected_edges();
            yield expect_eq(result.selected.keys(), expected.keys());
            yield expect(expected.keys().all(|package| result.selected.get(package).version == expected.get(package)));
            yield expect_eq(result.edges.len(), expected_edges.len());
            yield expect(expected_edges.values().all(|edge| result.edges.has(edge)));
        },
        RodinOutcome::Failed(_) => expect(false),
        RodinOutcome::Unsupported(_) => expect(false),
    };
}
"#,
        );
        Ok(source)
    }
}

fn assert_native_kernel_matches_cargo(fixture: &Fixture, workspace: &Path, triple: &str) {
    let adapter = fixture
        .native_kernel_oracle_source(workspace, triple)
        .unwrap_or_else(|error| panic!("project Cargo oracles for {}: {error}", fixture.name));
    let source = format!("{STD_VERSION}\n{NATIVE_RODIN_KERNEL}\n{adapter}");
    let report = run_source(&source)
        .unwrap_or_else(|error| panic!("native Rodin kernel runs {}: {error:?}", fixture.name));
    let verdicts = report
        .plain
        .checks
        .iter()
        .map(|check| check.passed)
        .collect::<Vec<_>>();
    assert_eq!(
        verdicts,
        [true, true, true, true],
        "Cargo oracle verdicts [selection keys, versions, graph edge count, expected edges] for {} on {triple}",
        fixture.name
    );
    assert!(
        report.passed(),
        "Cargo differential passes for {} on {triple}",
        fixture.name
    );
    assert!(
        report.agrees(),
        "plain and chaos agree for {} on {triple}",
        fixture.name
    );
    assert_eq!(report.plain.checks.len(), 4);
    for lane in [&report.plain, &report.chaos] {
        assert_eq!(lane.counters.pure_host_calls, 0);
        assert_eq!(lane.receipt_count, 0);
    }
}

#[test]
fn native_rodin_kernel_matches_live_cargo_line_oracles() {
    let fixture = line_fixture("native-live-cargo-line");
    let workspace = fixture.materialize().expect("materialize line fixture");
    assert_native_kernel_matches_cargo(&fixture, &workspace, LINUX);
}

#[test]
fn native_rodin_kernel_matches_live_cargo_target_oracles() {
    let fixture = direct_target_fixture("native-live-cargo-target");
    let workspace = fixture.materialize().expect("materialize target fixture");
    for triple in [LINUX, WINDOWS] {
        assert_native_kernel_matches_cargo(&fixture, &workspace, triple);
    }
}

#[test]
fn native_rodin_kernel_matches_live_cargo_feature_oracles() {
    let fixture = feature_unification_fixture("native-live-cargo-features");
    let workspace = fixture
        .materialize()
        .expect("materialize feature-unification fixture");
    assert_native_kernel_matches_cargo(&fixture, &workspace, LINUX);
}

#[test]
fn native_rodin_kernel_matches_live_cargo_policy_oracles() {
    let cases = [
        (
            weak_feature_fixture("native-live-cargo-weak-feature"),
            vec![LINUX],
        ),
        (
            feature_target_fixture("native-live-cargo-feature-target"),
            vec![LINUX, WINDOWS],
        ),
        (
            cfg_combinator_fixture("native-live-cargo-cfg-combinators"),
            vec![LINUX, WINDOWS],
        ),
        (
            edge_kind_fixture("native-live-cargo-edge-kinds"),
            vec![LINUX],
        ),
    ];
    for (fixture, targets) in cases {
        let workspace = fixture
            .materialize()
            .unwrap_or_else(|error| panic!("materialize {}: {error}", fixture.name));
        for triple in targets {
            assert_native_kernel_matches_cargo(&fixture, &workspace, triple);
        }
    }
}

/// A trivial three-crate line workspace: `app -> mid -> leaf`, all `0.1.0`, all
/// path dependencies.
fn line_fixture(name: &str) -> Fixture {
    Fixture::new(name, "app")
        .krate(FixtureCrate::new("leaf"))
        .krate(FixtureCrate::new("mid").dep(FixtureDep::new("leaf")))
        .krate(FixtureCrate::new("app").dep(FixtureDep::new("mid")))
}

fn direct_target_fixture(name: &str) -> Fixture {
    Fixture::new(name, "app")
        .krate(FixtureCrate::new("winthing"))
        .krate(FixtureCrate::new("app").dep(FixtureDep::new("winthing").target("cfg(windows)")))
}

fn feature_unification_fixture(name: &str) -> Fixture {
    Fixture::new(name, "app")
        .krate(FixtureCrate::new("left_dep"))
        .krate(FixtureCrate::new("right_dep"))
        .krate(
            FixtureCrate::new("shared")
                .feature("left", &["dep:left_dep"])
                .feature("right", &["dep:right_dep"])
                .dep(FixtureDep::new("left_dep").optional())
                .dep(FixtureDep::new("right_dep").optional()),
        )
        .krate(
            FixtureCrate::new("a").dep(
                FixtureDep::new("shared")
                    .default_features(false)
                    .feature("left"),
            ),
        )
        .krate(
            FixtureCrate::new("b").dep(
                FixtureDep::new("shared")
                    .default_features(false)
                    .feature("right"),
            ),
        )
        .krate(
            FixtureCrate::new("app")
                .dep(FixtureDep::new("a"))
                .dep(FixtureDep::new("b")),
        )
}

fn weak_feature_fixture(name: &str) -> Fixture {
    Fixture::new(name, "app")
        .krate(FixtureCrate::new("helper").feature("extra", &[]))
        .krate(
            FixtureCrate::new("lib")
                .feature("default", &["weak"])
                .feature("weak", &["helper?/extra"])
                .dep(FixtureDep::new("helper").optional()),
        )
        .krate(FixtureCrate::new("app").dep(FixtureDep::new("lib")))
}

fn feature_target_fixture(name: &str) -> Fixture {
    Fixture::new(name, "app")
        .krate(FixtureCrate::new("platform"))
        .krate(
            FixtureCrate::new("lib")
                .feature("default", &["bundle-platform"])
                .feature("bundle-platform", &["dep:platform"])
                .dep(
                    FixtureDep::new("platform")
                        .optional()
                        .target("cfg(windows)"),
                ),
        )
        .krate(FixtureCrate::new("app").dep(FixtureDep::new("lib")))
}

fn cfg_combinator_fixture(name: &str) -> Fixture {
    Fixture::new(name, "app")
        .krate(FixtureCrate::new("unix_x64"))
        .krate(FixtureCrate::new("win_os"))
        .krate(
            FixtureCrate::new("app")
                .dep(
                    FixtureDep::new("unix_x64")
                        .target(r#"cfg(all(unix, target_arch = "x86_64", not(windows)))"#),
                )
                .dep(FixtureDep::new("win_os").target(r#"cfg(target_os = "windows")"#)),
        )
}

fn edge_kind_fixture(name: &str) -> Fixture {
    Fixture::new(name, "app")
        .krate(FixtureCrate::new("testonly"))
        .krate(FixtureCrate::new("gen"))
        .krate(FixtureCrate::new("lib").dep(FixtureDep::new("testonly").kind(DepKind::Dev)))
        .krate(
            FixtureCrate::new("app")
                .dep(FixtureDep::new("lib"))
                .dep(FixtureDep::new("gen").kind(DepKind::Build)),
        )
}

fn selected_named(oracle: &SelectionOracle, name: &str) -> CargoPackageId {
    oracle
        .packages
        .iter()
        .find(|package| package.name == name)
        .unwrap_or_else(|| panic!("`{name}` locked by cargo: {:?}", oracle.packages))
        .clone()
}

fn edge_kind_to(graph: &GraphOracle, from: &str, to: &str) -> Option<EdgeKind> {
    graph
        .edges
        .iter()
        .find(|edge| edge.from.name == from && edge.to.name == to)
        .map(|edge| edge.kind)
}

/// The compat-class rule (rodin/docs/10-identity.md) — cargo's coexistence
/// semantics, keyed on the first non-zero version component.
#[test]
fn compat_class_follows_cargo_coexistence_rule() {
    let class = |v: &str| CompatClass::of(&SemVer::parse(v).unwrap());
    assert_eq!(class("1.4.2"), CompatClass::Major { major: 1 });
    assert_eq!(class("1.9.0"), CompatClass::Major { major: 1 });
    assert_eq!(class("2.0.0"), CompatClass::Major { major: 2 });
    assert_eq!(class("0.4.9"), CompatClass::ZeroMinor { minor: 4 });
    assert_eq!(class("0.5.0"), CompatClass::ZeroMinor { minor: 5 });
    assert_eq!(class("0.0.7"), CompatClass::ZeroZeroPatch { patch: 7 });
    assert_eq!(class("1.4.2"), class("1.9.0"));
    assert_ne!(class("1.9.0"), class("2.0.0"));
    assert_ne!(class("0.4.9"), class("0.5.0"));
}

/// Cargo's `source` field is classified into a typed [`Source`]; a path keeps its
/// directory, registry/git keep their full spec, and an unknown scheme is a typed
/// parse error — never a silent registry.
#[test]
fn classify_source_types_path_registry_git_and_rejects_unknown() {
    let path = classify_source(None, "/tmp/ws/leaf/Cargo.toml").unwrap();
    assert_eq!(
        path,
        Source::Path {
            dir: "/tmp/ws/leaf".to_owned()
        }
    );
    let registry = classify_source(
        Some("registry+https://github.com/rust-lang/crates.io-index"),
        "",
    )
    .unwrap();
    assert_eq!(
        registry,
        Source::Registry {
            spec: "registry+https://github.com/rust-lang/crates.io-index".to_owned()
        }
    );
    let git = classify_source(Some("git+https://example.com/x?rev=abc#abc123"), "").unwrap();
    assert_eq!(
        git,
        Source::Git {
            spec: "git+https://example.com/x?rev=abc#abc123".to_owned()
        }
    );
    let err = classify_source(Some("weird+file:///nope"), "").unwrap_err();
    assert!(err.contains("unrecognized Cargo source scheme"), "{err}");
}

/// Cargo's per-edge `kind` maps to a typed [`EdgeKind`]; `dev` is excluded and an
/// unknown kind is a typed parse error.
#[test]
fn edge_kind_classify_maps_cargo_dep_kinds() {
    assert_eq!(EdgeKind::classify(None).unwrap(), Some(EdgeKind::Normal));
    assert_eq!(
        EdgeKind::classify(Some("build")).unwrap(),
        Some(EdgeKind::Build)
    );
    assert_eq!(EdgeKind::classify(Some("dev")).unwrap(), None);
    let err = EdgeKind::classify(Some("bench")).unwrap_err();
    assert!(err.contains("unrecognized Cargo dependency kind"), "{err}");
}

/// Oracle 1: `cargo metadata` locks every workspace member as an exact
/// path-source identity at its declared version and domain.
#[test]
fn selection_oracle_locks_exact_path_identities() {
    let fixture = line_fixture("selection-identities");
    let workspace = fixture.materialize().expect("materialize workspace");
    let oracle = fixture
        .selection_oracle(&workspace)
        .expect("cargo metadata resolves");
    for name in ["app", "mid", "leaf"] {
        let package = selected_named(&oracle, name);
        match &package.source {
            Source::Path { dir } => {
                assert!(dir.ends_with(name), "{name} path dir is {dir:?}");
            }
            other => panic!("{name} should be a path source, got {other:?}"),
        }
        assert_eq!(package.version, "0.1.0");
        assert_eq!(
            package.domain().unwrap().compat,
            CompatClass::ZeroMinor { minor: 1 },
            "{name}@0.1.0 is compat class 0.1"
        );
    }
}

/// Oracle 2: the graph is target-projected AND retains edge kind. A `cfg(windows)`
/// edge appears only on windows; a build-dep edge is tagged `Build`, a normal-dep
/// edge `Normal`.
#[test]
fn graph_oracle_projects_target_and_retains_edge_kind() {
    let fixture = Fixture::new("graph-kinds", "app")
        .krate(FixtureCrate::new("leaf"))
        .krate(FixtureCrate::new("gen"))
        .krate(FixtureCrate::new("winonly"))
        .krate(
            FixtureCrate::new("app")
                .dep(FixtureDep::new("leaf"))
                .dep(FixtureDep::new("gen").kind(DepKind::Build))
                .dep(FixtureDep::new("winonly").target("cfg(windows)")),
        );
    let workspace = fixture.materialize().expect("materialize workspace");

    let linux = fixture
        .graph_oracle(&workspace, LINUX)
        .expect("linux graph");
    assert_eq!(edge_kind_to(&linux, "app", "leaf"), Some(EdgeKind::Normal));
    assert_eq!(
        edge_kind_to(&linux, "app", "gen"),
        Some(EdgeKind::Build),
        "build-dep edge kind is retained: {:?}",
        linux.edges
    );
    assert_eq!(
        edge_kind_to(&linux, "app", "winonly"),
        None,
        "cfg(windows) edge is projected out on linux: {:?}",
        linux.edges
    );
    assert!(!linux.nodes.iter().any(|node| node.name == "winonly"));

    let windows = fixture
        .graph_oracle(&workspace, WINDOWS)
        .expect("windows graph");
    assert_eq!(
        edge_kind_to(&windows, "app", "winonly"),
        Some(EdgeKind::Normal),
        "cfg(windows) edge is enabled on windows: {:?}",
        windows.edges
    );
    assert_eq!(edge_kind_to(&windows, "app", "gen"), Some(EdgeKind::Build));
}

/// The two oracles cohere on one workspace: every node in the target-projected
/// graph is an *exact* package the lockfile selected (equal by source+name+version,
/// not just name+version).
#[test]
fn graph_nodes_are_exact_subset_of_selection() {
    let fixture = line_fixture("oracles-cohere");
    let workspace = fixture.materialize().expect("materialize workspace");
    let selection = fixture.selection_oracle(&workspace).expect("selection");
    let graph = fixture.graph_oracle(&workspace, LINUX).expect("graph");
    assert!(!graph.nodes.is_empty(), "graph has nodes");
    for node in &graph.nodes {
        assert!(
            selection.packages.contains(node),
            "graph node {node:?} must be an exact locked package: {:?}",
            selection.packages
        );
    }
}

/// A candidate `SolveResult` built from cargo's own oracle output matches both
/// oracles with zero discrepancies — the comparator's reflexivity plus the whole
/// materialize -> cargo metadata -> parse -> compare pipeline end to end.
#[test]
fn cargo_derived_candidate_matches_both_oracles() {
    let fixture = line_fixture("candidate-matches");
    let workspace = fixture.materialize().expect("materialize workspace");
    let selection = fixture.selection_oracle(&workspace).expect("selection");
    let graph = fixture.graph_oracle(&workspace, LINUX).expect("graph");

    let mut graphs = BTreeMap::new();
    graphs.insert(graph.triple.clone(), graph.edges.clone());
    let candidate = SolveResult {
        selected: selection.packages.clone(),
        graphs,
    };

    let discrepancies = candidate.compare(&selection, std::slice::from_ref(&graph));
    assert!(
        discrepancies.is_empty(),
        "cargo-derived candidate matches both oracles: {discrepancies:?}"
    );
}

/// Oracle 1 comparator: a missing selection, a same-domain version bump, and an
/// extra selection each become their own typed `Discrepancy` — keyed on exact
/// identity with a domain projection, no last-wins collapse.
#[test]
fn selection_comparator_types_missing_extra_and_version_mismatch() {
    let fixture = line_fixture("selection-discrepancies");
    let workspace = fixture.materialize().expect("materialize workspace");
    let oracle = fixture.selection_oracle(&workspace).expect("selection");
    let leaf = selected_named(&oracle, "leaf");
    let mid = selected_named(&oracle, "mid");

    let mut candidate = oracle.packages.clone();
    candidate.remove(&leaf);
    candidate.remove(&mid);
    // Bump `mid` within its coexistence domain (0.1.0 -> 0.1.9 stays class 0.1).
    candidate.insert(CargoPackageId {
        source: mid.source.clone(),
        name: mid.name.clone(),
        version: "0.1.9".to_owned(),
    });
    // Invent an exact package cargo never locked.
    let ghost = CargoPackageId {
        source: Source::Path {
            dir: "/tmp/ghost".to_owned(),
        },
        name: "ghost".to_owned(),
        version: "0.1.0".to_owned(),
    };
    candidate.insert(ghost.clone());

    let discrepancies = compare_selection(&candidate, &oracle);
    assert_eq!(
        discrepancies.len(),
        3,
        "exactly three typed divergences: {discrepancies:?}"
    );
    assert!(discrepancies.contains(&Discrepancy::MissingSelection {
        expected: leaf.clone()
    }));
    assert!(discrepancies.contains(&Discrepancy::VersionMismatch {
        domain: mid.domain().unwrap(),
        cargo: "0.1.0".to_owned(),
        candidate: "0.1.9".to_owned(),
    }));
    assert!(discrepancies.contains(&Discrepancy::ExtraSelection { unexpected: ghost }));
}

/// Two exact packages that project to one Rodin domain are surfaced as a typed
/// `DomainMultiplicity` (never a silent last-wins collapse), and the ambiguous
/// domain is withheld from the missing/extra comparison.
#[test]
fn domain_multiplicity_is_surfaced_not_collapsed() {
    let source = Source::Registry {
        spec: "registry+https://github.com/rust-lang/crates.io-index".to_owned(),
    };
    let serde_low = CargoPackageId {
        source: source.clone(),
        name: "serde".to_owned(),
        version: "1.0.0".to_owned(),
    };
    let serde_high = CargoPackageId {
        source,
        name: "serde".to_owned(),
        version: "1.5.0".to_owned(),
    };
    let mut candidate = BTreeSet::new();
    candidate.insert(serde_low.clone());
    candidate.insert(serde_high.clone());
    let oracle = SelectionOracle {
        packages: BTreeSet::new(),
    };

    let discrepancies = compare_selection(&candidate, &oracle);
    assert!(
        discrepancies.iter().any(|d| matches!(
            d,
            Discrepancy::DomainMultiplicity { side: Side::Candidate, packages, .. }
                if packages.len() == 2
                    && packages.contains(&serde_low)
                    && packages.contains(&serde_high)
        )),
        "two same-class exact versions surface as domain multiplicity: {discrepancies:?}"
    );
    assert!(
        !discrepancies
            .iter()
            .any(|d| matches!(d, Discrepancy::ExtraSelection { .. })),
        "the ambiguous domain is withheld from the missing/extra comparison: {discrepancies:?}"
    );
}

/// A non-semver version cannot be assigned a coexistence domain; it is surfaced as
/// a typed `MalformedVersion`, not silently dropped.
#[test]
fn malformed_version_is_surfaced() {
    let bad = CargoPackageId {
        source: Source::Path {
            dir: "/tmp/bad".to_owned(),
        },
        name: "bad".to_owned(),
        version: "not-semver".to_owned(),
    };
    let mut candidate = BTreeSet::new();
    candidate.insert(bad.clone());
    let oracle = SelectionOracle {
        packages: BTreeSet::new(),
    };

    let discrepancies = compare_selection(&candidate, &oracle);
    assert!(
        discrepancies.iter().any(|d| matches!(
            d,
            Discrepancy::MalformedVersion { side: Side::Candidate, package } if *package == bad
        )),
        "a non-semver version is surfaced, not dropped: {discrepancies:?}"
    );
}

/// Executable cargo certificate: two overlapping *compatible* requirements
/// (`^1.0.0` and `=1.0.0`) on one path crate unify to a single locked package.
/// This demonstrates local unification only — it does NOT establish that a real
/// lockfile can never place two exact versions in one Rodin domain. Rodin's
/// `(source, name, compat-class)` domain is a *model* of cargo's version
/// bucketing, not a proven identity with it. Disjoint *exact* requirements across
/// dependency paths (`=1.0.0` and `=1.2.0`, both compat class 1) need a registry
/// publishing two versions; measured against live cargo, that specific pair is a
/// resolution *conflict* (cargo activates one version per compat bucket) rather
/// than coexistence — but a Rodin/cargo bucketing divergence (prerelease/build-
/// metadata, alternate registries, or a compat-class bug) is a valid Cargo case
/// Rodin cannot represent, surfaced as [`Discrepancy::DomainMultiplicity`], never
/// collapsed and never assumed away. Certifying that boundary against live cargo
/// needs a two-version registry; no cargo-usable local registry exists in-repo
/// (the sparse-index snapshot feeds vix's own resolver, not cargo), so the live
/// differential is a later oracle extension — the constructed
/// `domain_multiplicity_is_surfaced_not_collapsed` certificate stands in for now.
#[test]
fn cargo_unifies_overlapping_compatible_requirements() {
    let fixture = Fixture::new("same-class-unify", "app")
        .krate(FixtureCrate::new("dep").version("1.0.0"))
        .krate(FixtureCrate::new("a").dep(FixtureDep::new("dep").req("^1.0.0")))
        .krate(FixtureCrate::new("b").dep(FixtureDep::new("dep").req("=1.0.0")))
        .krate(
            FixtureCrate::new("app")
                .dep(FixtureDep::new("a"))
                .dep(FixtureDep::new("b")),
        );
    let workspace = fixture.materialize().expect("materialize workspace");
    let oracle = fixture.selection_oracle(&workspace).expect("selection");

    let deps: Vec<_> = oracle
        .packages
        .iter()
        .filter(|package| package.name == "dep")
        .collect();
    assert_eq!(
        deps.len(),
        1,
        "cargo unifies same-compat-class requirements to one package: {deps:?}"
    );
    let self_check = compare_selection(&oracle.packages, &oracle);
    assert!(
        self_check.is_empty(),
        "this fixture's cargo selection has one package per domain, so the self-comparison is clean: {self_check:?}"
    );
}

/// Oracle 2 comparator: a dropped edge and an invented edge become MissingEdge and
/// ExtraEdge for the target.
#[test]
fn graph_comparator_types_missing_and_extra_edges() {
    let fixture = line_fixture("graph-discrepancies");
    let workspace = fixture.materialize().expect("materialize workspace");
    let graph = fixture.graph_oracle(&workspace, LINUX).expect("graph");
    assert!(!graph.edges.is_empty(), "line workspace has edges");

    let mut candidate = graph.edges.clone();
    let dropped = candidate.iter().next().cloned().unwrap();
    candidate.remove(&dropped);
    let invented = GraphEdge {
        from: CargoPackageId {
            source: Source::Path {
                dir: "/tmp/app".to_owned(),
            },
            name: "app".to_owned(),
            version: "0.1.0".to_owned(),
        },
        to: CargoPackageId {
            source: Source::Path {
                dir: "/tmp/ghost".to_owned(),
            },
            name: "ghost".to_owned(),
            version: "9.9.9".to_owned(),
        },
        kind: EdgeKind::Normal,
    };
    candidate.insert(invented.clone());

    let discrepancies = compare_graph(LINUX, &candidate, &graph);
    assert_eq!(
        discrepancies.len(),
        2,
        "one missing, one extra: {discrepancies:?}"
    );
    assert!(discrepancies.contains(&Discrepancy::MissingEdge {
        target: LINUX.to_owned(),
        edge: dropped,
    }));
    assert!(discrepancies.contains(&Discrepancy::ExtraEdge {
        target: LINUX.to_owned(),
        edge: invented,
    }));
}

/// The discrepancy report is a machine-readable artifact: it serializes to JSON
/// via facet (no serde, no hand-written JSON) and round-trips back to the same
/// typed values — ready to feed a minimizer.
#[test]
fn discrepancy_report_serializes_and_round_trips_via_facet_json() {
    let registry = Source::Registry {
        spec: "registry+https://github.com/rust-lang/crates.io-index".to_owned(),
    };
    let report = DiscrepancyReport {
        fixture: "demo".to_owned(),
        discrepancies: vec![
            Discrepancy::MissingSelection {
                expected: CargoPackageId {
                    source: registry.clone(),
                    name: "serde".to_owned(),
                    version: "1.0.1".to_owned(),
                },
            },
            Discrepancy::VersionMismatch {
                domain: ResolutionDomain {
                    source: Source::Path {
                        dir: "/tmp/ws/mid".to_owned(),
                    },
                    name: "mid".to_owned(),
                    compat: CompatClass::ZeroMinor { minor: 1 },
                },
                cargo: "0.1.0".to_owned(),
                candidate: "0.1.9".to_owned(),
            },
            Discrepancy::DomainMultiplicity {
                side: Side::Candidate,
                domain: ResolutionDomain {
                    source: registry.clone(),
                    name: "serde".to_owned(),
                    compat: CompatClass::Major { major: 1 },
                },
                packages: vec![CargoPackageId {
                    source: registry,
                    name: "serde".to_owned(),
                    version: "1.0.0".to_owned(),
                }],
            },
        ],
    };

    let json = report.to_json().expect("serialize via facet-json");
    assert!(
        json.contains("MissingSelection"),
        "variant tag present: {json}"
    );
    assert!(json.contains("serde"), "identity payload present: {json}");

    let parsed: DiscrepancyReport =
        facet_json::from_str(&json).expect("round-trip back through facet-json");
    assert_eq!(parsed.fixture, report.fixture);
    assert_eq!(parsed.discrepancies, report.discrepancies);
}
