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
//! rodin/PLAN.md). It reuses the fixture DSL (`Fixture`/`materialize`) and adds
//! the two cargo oracles as typed, minimizable data:
//!
//!   * Oracle 1 — version selection: `cargo generate-lockfile --offline` parsed
//!     (via `facet-cargo-toml`, no serde) into `(source, name, compat-class,
//!     version)` identities (rodin/docs/10-identity).
//!   * Oracle 2 — the target-projected enabled graph: `cargo tree -e
//!     normal,build --target <triple> --offline` parsed into typed graph edges.
//!
//! A future Vix resolver kernel emits a typed [`SolveResult`]; the harness
//! compares it against both oracles and turns every divergence into a structured
//! [`Discrepancy`] value suitable for minimization (serialized to JSON via
//! `facet-json`, never hand-written). The kernel does not exist yet, so nothing
//! in-tree produces a real `SolveResult`; the comparator is exercised with
//! candidates constructed from cargo's own oracle output (a trivially-matching
//! twin) and deliberately-perturbed variants — the Cargo side itself carries
//! production-shaped coverage against real offline workspaces.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use facet::Facet;
use facet_cargo_toml::{CargoLock, LockPackage};
use semver::Version as SemVer;
use vix::machine::{DriveEvent, Machine, MachineArg, NamedArg, RenderedValue};

const LINUX: &str = "x86_64-unknown-linux-gnu";
const WINDOWS: &str = "x86_64-pc-windows-msvc";

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
// cargo is THE and ONLY oracle (rodin/PLAN.md, rodin/docs/00-oracle.md). This
// layer runs real, offline cargo over a materialized fixture workspace and turns
// both observable outputs into typed, minimizable data:
//
//   * Oracle 1 — `cargo generate-lockfile` → per-identity version selection.
//   * Oracle 2 — `cargo tree --target`     → the target-projected enabled graph.
//
// A future Vix resolver kernel will emit a `SolveResult`; `SolveResult::compare`
// diffs it against both oracles and yields `Discrepancy` values. The kernel is
// not built yet — see the residual seam note in rodin/PLAN.md — so no in-tree
// code produces a real `SolveResult`; the tests construct candidates instead.
// ===========================================================================

/// Provenance of a package identity (rodin/docs/10-identity.md). The same `name`
/// from two provenances is two identities; cargo resolves and locks them apart,
/// so the harness must never fold a registry crate into a path crate of the same
/// name. Serialized into discrepancy artifacts, hence `Facet`.
#[derive(Facet, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
enum Source {
    /// A path dependency — no `source` key in Cargo.lock.
    Path,
    /// crates.io or an alternate registry, keyed by index URL.
    Registry { url: String },
    /// A git dependency, by url and the locked rev.
    Git { url: String, rev: String },
}

impl Source {
    /// Classify a Cargo.lock `source` string. `None` is a path dependency;
    /// `registry+`/`sparse+` are registries; `git+URL#REV` is a git source.
    fn classify(source: Option<&str>) -> Self {
        match source {
            None => Self::Path,
            Some(raw) => {
                if let Some(url) = raw
                    .strip_prefix("registry+")
                    .or_else(|| raw.strip_prefix("sparse+"))
                {
                    Self::Registry {
                        url: url.to_owned(),
                    }
                } else if let Some(body) = raw.strip_prefix("git+") {
                    match body.split_once('#') {
                        Some((url, rev)) => Self::Git {
                            url: url.to_owned(),
                            rev: rev.to_owned(),
                        },
                        None => Self::Git {
                            url: body.to_owned(),
                            rev: String::new(),
                        },
                    }
                } else {
                    Self::Registry {
                        url: raw.to_owned(),
                    }
                }
            }
        }
    }
}

/// The semver coexistence bucket (rodin/docs/10-identity.md § compat-class rule).
/// Same class ⇒ two versions compete for one slot; different class ⇒ they coexist
/// (this is why `serde 1.x` and `serde 2.x` can both appear in one lockfile).
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

/// A package identity: the `(source, name, compat-class)` triple everything else
/// quantifies over (rodin/docs/10-identity.md). Two nodes with equal triples are
/// the same resolution domain and must resolve to one version.
#[derive(Facet, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct PackageIdentity {
    source: Source,
    name: String,
    compat: CompatClass,
}

/// One row of Oracle 1: an identity locked to an exact version. `version` stays a
/// string (the exact Cargo.lock spelling); `compat` is derived from it.
#[derive(Facet, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct SelectedPackage {
    identity: PackageIdentity,
    version: String,
}

impl SelectedPackage {
    fn from_lock(package: &LockPackage) -> Result<Self, String> {
        let parsed = SemVer::parse(&package.version)
            .map_err(|err| format!("parse version `{}`: {err}", package.version))?;
        Ok(Self {
            identity: PackageIdentity {
                source: Source::classify(package.source.as_deref()),
                name: package.name.clone(),
                compat: CompatClass::of(&parsed),
            },
            version: package.version.clone(),
        })
    }
}

/// A node in the target-projected graph: a package name at a resolved version, as
/// cargo tree prints it.
#[derive(Facet, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct GraphNode {
    name: String,
    version: String,
}

/// One edge of Oracle 2: a consumed dependency edge under a specific target and
/// the `normal,build` edge-kind filter.
#[derive(Facet, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct GraphEdge {
    from: GraphNode,
    to: GraphNode,
}

/// Oracle 1 — the lockfile's per-identity version selection.
struct LockOracle {
    selected: BTreeSet<SelectedPackage>,
}

/// Oracle 2 — the target-projected enabled graph for one triple.
struct TreeOracle {
    triple: String,
    nodes: BTreeSet<GraphNode>,
    edges: BTreeSet<GraphEdge>,
}

/// What a future Vix resolver kernel emits: a per-identity selection plus the
/// per-target enabled graph. The harness accepts this typed value and compares it
/// against the two cargo oracles. Nothing in-tree builds a real one yet.
#[derive(Facet, Clone, Debug, Default)]
struct SolveResult {
    selected: BTreeSet<SelectedPackage>,
    /// Enabled graph edges, keyed by target triple.
    graphs: BTreeMap<String, BTreeSet<GraphEdge>>,
}

impl SolveResult {
    /// Compare against Oracle 1 and, for every provided `TreeOracle`, Oracle 2.
    /// The result is deterministic (BTree iteration order) and structural — ready
    /// to hand to a minimizer, never prose.
    fn compare(&self, lock: &LockOracle, trees: &[TreeOracle]) -> Vec<Discrepancy> {
        let mut out = compare_lockfile(&self.selected, lock);
        for tree in trees {
            let candidate = self.graphs.get(&tree.triple).cloned().unwrap_or_default();
            out.extend(compare_tree(&tree.triple, &candidate, tree));
        }
        out
    }
}

/// A single typed divergence between a candidate `SolveResult` and a cargo oracle.
/// The vocabulary mirrors the historical differential's classification
/// (rodin/docs/00-oracle.md § provenance): MissingInRodin / ExtraInRodin /
/// VersionMismatch, now split across the two oracles.
#[derive(Facet, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
enum Discrepancy {
    /// Oracle 1: cargo locked this identity+version; the candidate did not select it.
    MissingSelection { expected: SelectedPackage },
    /// Oracle 1: the candidate selected an identity+version cargo did not lock.
    ExtraSelection { unexpected: SelectedPackage },
    /// Oracle 1: same identity (same coexistence domain), different locked version.
    VersionMismatch {
        identity: PackageIdentity,
        cargo: String,
        candidate: String,
    },
    /// Oracle 2: cargo's target-projected graph has an edge the candidate lacks.
    MissingEdge { target: String, edge: GraphEdge },
    /// Oracle 2: the candidate has a target-projected edge cargo does not.
    ExtraEdge { target: String, edge: GraphEdge },
}

/// A minimizable, machine-readable report over one fixture: the discrepancies
/// found against the cargo oracles, serialized to JSON via `facet-json`.
#[derive(Facet, Clone, Debug)]
struct DiscrepancyReport {
    fixture: String,
    discrepancies: Vec<Discrepancy>,
}

impl DiscrepancyReport {
    /// Serialize to pretty JSON via facet — never hand-written, never serde.
    fn to_json(&self) -> Result<String, String> {
        facet_json::to_string_pretty(self)
            .map_err(|err| format!("serialize discrepancy report: {err}"))
    }
}

/// Oracle 1 comparison: per-identity, missing/extra/version-mismatch.
fn compare_lockfile(
    candidate: &BTreeSet<SelectedPackage>,
    oracle: &LockOracle,
) -> Vec<Discrepancy> {
    let index = |set: &BTreeSet<SelectedPackage>| -> BTreeMap<PackageIdentity, String> {
        set.iter()
            .map(|pkg| (pkg.identity.clone(), pkg.version.clone()))
            .collect()
    };
    let candidate_by_id = index(candidate);
    let oracle_by_id = index(&oracle.selected);
    let mut out = Vec::new();
    for (identity, cargo_version) in &oracle_by_id {
        match candidate_by_id.get(identity) {
            None => out.push(Discrepancy::MissingSelection {
                expected: SelectedPackage {
                    identity: identity.clone(),
                    version: cargo_version.clone(),
                },
            }),
            Some(candidate_version) if candidate_version != cargo_version => {
                out.push(Discrepancy::VersionMismatch {
                    identity: identity.clone(),
                    cargo: cargo_version.clone(),
                    candidate: candidate_version.clone(),
                });
            }
            Some(_) => {}
        }
    }
    for (identity, candidate_version) in &candidate_by_id {
        if !oracle_by_id.contains_key(identity) {
            out.push(Discrepancy::ExtraSelection {
                unexpected: SelectedPackage {
                    identity: identity.clone(),
                    version: candidate_version.clone(),
                },
            });
        }
    }
    out
}

/// Oracle 2 comparison: the symmetric difference of the edge sets for `target`.
fn compare_tree(
    target: &str,
    candidate: &BTreeSet<GraphEdge>,
    oracle: &TreeOracle,
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
    /// Oracle 1: `cargo generate-lockfile --offline` in the materialized
    /// workspace, then parse `Cargo.lock` (via facet-cargo-toml, no serde) into
    /// typed `(source, name, compat-class, version)` identities.
    fn lock_oracle(&self) -> Result<LockOracle, String> {
        let workspace = self.materialize()?;
        let output = Command::new("cargo")
            .args(["generate-lockfile", "--offline"])
            .current_dir(&workspace)
            .output()
            .map_err(|err| format!("spawning cargo generate-lockfile: {err}"))?;
        if !output.status.success() {
            return Err(format!(
                "cargo generate-lockfile failed for `{}`: {}",
                self.name,
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        let lock_path = workspace.join("Cargo.lock");
        let contents = std::fs::read_to_string(&lock_path)
            .map_err(|err| format!("read {}: {err}", lock_path.display()))?;
        let lock = CargoLock::parse(&contents).map_err(|err| format!("parse Cargo.lock: {err}"))?;
        let mut selected = BTreeSet::new();
        for package in &lock.packages {
            selected.insert(SelectedPackage::from_lock(package)?);
        }
        Ok(LockOracle { selected })
    }

    /// Oracle 2: the target-projected enabled graph. `cargo tree -e normal,build
    /// --target <triple>` with `--no-dedupe` (so repeated subtrees still yield
    /// their edges) and `--prefix depth` (so parent/child is reconstructable).
    fn tree_oracle(&self, triple: &str) -> Result<TreeOracle, String> {
        let workspace = self.materialize()?;
        let output = Command::new("cargo")
            .args([
                "tree",
                "-e",
                "normal,build",
                "--target",
                triple,
                "--prefix",
                "depth",
                "--no-dedupe",
                "--offline",
                "-p",
                &self.root,
            ])
            .current_dir(&workspace)
            .output()
            .map_err(|err| format!("spawning cargo tree: {err}"))?;
        if !output.status.success() {
            return Err(format!(
                "cargo tree failed for {} on {triple}: {}",
                self.root,
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        let text = String::from_utf8(output.stdout).map_err(|err| err.to_string())?;
        parse_depth_tree(triple, &text)
    }
}

/// Parse `cargo tree --prefix depth --no-dedupe` output into a typed graph. Each
/// line is `<depth><name> v<version> [(source)]`; parent/child edges are rebuilt
/// from the depth stack.
fn parse_depth_tree(triple: &str, text: &str) -> Result<TreeOracle, String> {
    let mut nodes = BTreeSet::new();
    let mut edges = BTreeSet::new();
    // stack[d] = the node most recently seen at depth d.
    let mut stack: Vec<GraphNode> = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let split = line
            .find(|c: char| !c.is_ascii_digit())
            .ok_or_else(|| format!("cargo tree line has no package after depth: {line:?}"))?;
        let depth: usize = line[..split]
            .parse()
            .map_err(|err| format!("parse tree depth in {line:?}: {err}"))?;
        let mut tokens = line[split..].split_whitespace();
        let name = tokens
            .next()
            .ok_or_else(|| format!("cargo tree line missing package name: {line:?}"))?;
        let version = tokens
            .next()
            .ok_or_else(|| format!("cargo tree line missing version: {line:?}"))?
            .trim_start_matches('v');
        let node = GraphNode {
            name: name.to_owned(),
            version: version.to_owned(),
        };
        nodes.insert(node.clone());
        stack.truncate(depth);
        if let Some(parent) = stack.last() {
            edges.insert(GraphEdge {
                from: parent.clone(),
                to: node.clone(),
            });
        }
        stack.push(node);
    }
    Ok(TreeOracle {
        triple: triple.to_owned(),
        nodes,
        edges,
    })
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
// Every test below drives a real, offline cargo over a materialized path
// workspace and validates the typed extraction and the comparator. cargo is the
// only oracle; no recorded reference selection is consulted.
// ===========================================================================

/// A trivial three-crate line workspace: `app -> mid -> leaf`, all `0.1.0`,
/// all path dependencies.
fn line_fixture(name: &str) -> Fixture {
    Fixture::new(name, "app")
        .krate(FixtureCrate::new("leaf"))
        .krate(FixtureCrate::new("mid").dep(FixtureDep::new("leaf")))
        .krate(FixtureCrate::new("app").dep(FixtureDep::new("mid")))
}

fn selected_named(oracle: &LockOracle, name: &str) -> SelectedPackage {
    oracle
        .selected
        .iter()
        .find(|pkg| pkg.identity.name == name)
        .unwrap_or_else(|| panic!("`{name}` locked by cargo: {:?}", oracle.selected))
        .clone()
}

/// The compat-class rule (rodin/docs/10-identity.md) — recovered as an executable
/// oracle over cargo's coexistence semantics, keyed on the first non-zero
/// version component.
#[test]
fn compat_class_follows_cargo_coexistence_rule() {
    let class = |v: &str| CompatClass::of(&SemVer::parse(v).unwrap());
    // major >= 1 → the major band; 1.4.2 and 1.9.0 share class `1`, 2.0.0 is `2`.
    assert_eq!(class("1.4.2"), CompatClass::Major { major: 1 });
    assert_eq!(class("1.9.0"), CompatClass::Major { major: 1 });
    assert_eq!(class("2.0.0"), CompatClass::Major { major: 2 });
    // major == 0, minor != 0 → the 0.y footgun; 0.4.x and 0.5.x are distinct.
    assert_eq!(class("0.4.9"), CompatClass::ZeroMinor { minor: 4 });
    assert_eq!(class("0.5.0"), CompatClass::ZeroMinor { minor: 5 });
    // major == 0, minor == 0 → each 0.0.patch is its own class.
    assert_eq!(class("0.0.7"), CompatClass::ZeroZeroPatch { patch: 7 });
    // Two same-class versions are one coexistence domain; different class coexist.
    assert_eq!(class("1.4.2"), class("1.9.0"));
    assert_ne!(class("1.9.0"), class("2.0.0"));
    assert_ne!(class("0.4.9"), class("0.5.0"));
}

/// Oracle 1: `cargo generate-lockfile` locks every workspace member as a
/// path-source identity at its declared version.
#[test]
fn lock_oracle_locks_path_workspace_identities() {
    let oracle = line_fixture("lock-identities")
        .lock_oracle()
        .expect("cargo generate-lockfile resolves");
    for name in ["app", "mid", "leaf"] {
        let pkg = selected_named(&oracle, name);
        assert_eq!(pkg.identity.source, Source::Path, "{name} is a path source");
        assert_eq!(
            pkg.identity.compat,
            CompatClass::ZeroMinor { minor: 1 },
            "{name}@0.1.0 is compat class 0.1"
        );
        assert_eq!(pkg.version, "0.1.0");
    }
}

/// Oracle 2: the tree is a *projection* — a `cfg(windows)` edge is in the graph on
/// windows and gone on linux, both as node presence and as a typed edge.
#[test]
fn tree_oracle_projects_target_conditional_edges() {
    let fixture = Fixture::new("tree-target-edges", "app")
        .krate(FixtureCrate::new("winthing"))
        .krate(FixtureCrate::new("app").dep(FixtureDep::new("winthing").target("cfg(windows)")));

    let windows = fixture.tree_oracle(WINDOWS).expect("cargo tree on windows");
    let linux = fixture.tree_oracle(LINUX).expect("cargo tree on linux");

    assert!(
        windows.nodes.iter().any(|node| node.name == "winthing"),
        "winthing is a node on windows: {:?}",
        windows.nodes
    );
    assert!(
        !linux.nodes.iter().any(|node| node.name == "winthing"),
        "winthing is absent on linux: {:?}",
        linux.nodes
    );

    let edge = GraphEdge {
        from: GraphNode {
            name: "app".to_owned(),
            version: "0.1.0".to_owned(),
        },
        to: GraphNode {
            name: "winthing".to_owned(),
            version: "0.1.0".to_owned(),
        },
    };
    assert!(
        windows.edges.contains(&edge),
        "app->winthing edge present on windows: {:?}",
        windows.edges
    );
    assert!(
        !linux.edges.iter().any(|e| e.to.name == "winthing"),
        "no winthing edge on linux: {:?}",
        linux.edges
    );
}

/// The two oracles cohere: every node in the target-projected tree is an identity
/// the lockfile selected (the tree is a subset of the lock — doc 00).
#[test]
fn tree_nodes_are_a_subset_of_lock_selection() {
    let fixture = line_fixture("oracles-cohere");
    let lock = fixture.lock_oracle().expect("generate-lockfile");
    let tree = fixture.tree_oracle(LINUX).expect("cargo tree");
    assert!(!tree.nodes.is_empty(), "tree has nodes");
    for node in &tree.nodes {
        assert!(
            lock.selected
                .iter()
                .any(|pkg| pkg.identity.name == node.name && pkg.version == node.version),
            "tree node {node:?} must be locked by Oracle 1: {:?}",
            lock.selected
        );
    }
}

/// A candidate `SolveResult` built from cargo's own oracle output must match both
/// oracles with zero discrepancies — the comparator's reflexivity plus the whole
/// materialize -> cargo -> parse -> compare pipeline, end to end.
#[test]
fn cargo_derived_candidate_matches_both_oracles() {
    let fixture = line_fixture("candidate-matches");
    let lock = fixture.lock_oracle().expect("generate-lockfile");
    let tree = fixture.tree_oracle(LINUX).expect("cargo tree");

    let mut graphs = BTreeMap::new();
    graphs.insert(tree.triple.clone(), tree.edges.clone());
    let candidate = SolveResult {
        selected: lock.selected.clone(),
        graphs,
    };

    let discrepancies = candidate.compare(&lock, std::slice::from_ref(&tree));
    assert!(
        discrepancies.is_empty(),
        "cargo-derived candidate matches both oracles: {discrepancies:?}"
    );
}

/// Oracle 1 comparator: a missing selection, a same-domain version bump, and an
/// extra selection each become their own typed `Discrepancy`.
#[test]
fn lockfile_comparator_types_missing_extra_and_version_mismatch() {
    let fixture = line_fixture("lock-discrepancies");
    let oracle = fixture.lock_oracle().expect("generate-lockfile");
    let leaf = selected_named(&oracle, "leaf");
    let mid = selected_named(&oracle, "mid");

    let mut candidate = oracle.selected.clone();
    // Drop `leaf` entirely → MissingSelection.
    candidate.remove(&leaf);
    // Bump `mid` within its coexistence domain (0.1.0 -> 0.1.9 stays class 0.1) →
    // VersionMismatch, not a re-identification.
    candidate.remove(&mid);
    candidate.insert(SelectedPackage {
        identity: mid.identity.clone(),
        version: "0.1.9".to_owned(),
    });
    // Invent an identity cargo never locked → ExtraSelection.
    let ghost = SelectedPackage {
        identity: PackageIdentity {
            source: Source::Path,
            name: "ghost".to_owned(),
            compat: CompatClass::ZeroMinor { minor: 1 },
        },
        version: "0.1.0".to_owned(),
    };
    candidate.insert(ghost.clone());

    let discrepancies = compare_lockfile(&candidate, &oracle);
    assert_eq!(
        discrepancies.len(),
        3,
        "exactly three typed divergences: {discrepancies:?}"
    );
    assert!(discrepancies.contains(&Discrepancy::MissingSelection {
        expected: leaf.clone()
    }));
    assert!(discrepancies.contains(&Discrepancy::VersionMismatch {
        identity: mid.identity.clone(),
        cargo: "0.1.0".to_owned(),
        candidate: "0.1.9".to_owned(),
    }));
    assert!(discrepancies.contains(&Discrepancy::ExtraSelection { unexpected: ghost }));
}

/// Oracle 2 comparator: a dropped edge and an invented edge become MissingEdge and
/// ExtraEdge for the target.
#[test]
fn tree_comparator_types_missing_and_extra_edges() {
    let fixture = line_fixture("tree-discrepancies");
    let tree = fixture.tree_oracle(LINUX).expect("cargo tree");
    assert!(!tree.edges.is_empty(), "line workspace has edges");

    let mut candidate = tree.edges.clone();
    let dropped = candidate.iter().next().cloned().unwrap();
    candidate.remove(&dropped);
    let invented = GraphEdge {
        from: GraphNode {
            name: "app".to_owned(),
            version: "0.1.0".to_owned(),
        },
        to: GraphNode {
            name: "ghost".to_owned(),
            version: "9.9.9".to_owned(),
        },
    };
    candidate.insert(invented.clone());

    let discrepancies = compare_tree(LINUX, &candidate, &tree);
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
    let report = DiscrepancyReport {
        fixture: "demo".to_owned(),
        discrepancies: vec![
            Discrepancy::MissingSelection {
                expected: SelectedPackage {
                    identity: PackageIdentity {
                        source: Source::Registry {
                            url: "https://github.com/rust-lang/crates.io-index".to_owned(),
                        },
                        name: "serde".to_owned(),
                        compat: CompatClass::Major { major: 1 },
                    },
                    version: "1.0.1".to_owned(),
                },
            },
            Discrepancy::VersionMismatch {
                identity: PackageIdentity {
                    source: Source::Path,
                    name: "mid".to_owned(),
                    compat: CompatClass::ZeroMinor { minor: 1 },
                },
                cargo: "0.1.0".to_owned(),
                candidate: "0.1.9".to_owned(),
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
