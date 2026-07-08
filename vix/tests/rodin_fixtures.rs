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
//! `vix_selected` SUT seam is filled in as rodin.vix grows (rodin/docs/40-search).
//! The five differential tests are #[ignore]'d until the native resolver lands;
//! the behaviors they pin are catalogued in rodin/docs/05-fixture-corpus.md.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use vix::machine::driver::StepCommand;
use vix::machine::{
    DriveEvent, Machine, MachineArg, MachineDiagSnapshot, NamedArg, RenderedValue,
    machine_diag_snapshot, reset_machine_diag, set_machine_diag_enabled,
};

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

fn trace_summary(machine: &Machine) -> BTreeMap<&'static str, usize> {
    let mut counts = BTreeMap::new();
    for event in machine.trace() {
        let key = match event {
            DriveEvent::Demanded { .. } => "demanded",
            DriveEvent::MemoHit { .. } => "memo_hit",
            DriveEvent::MemoProjectionHit { .. } => "memo_projection_hit",
            DriveEvent::MemoSemanticHit { .. } => "memo_semantic_hit",
            DriveEvent::Spawned { .. } => "spawned",
            DriveEvent::ParkedOn { .. } => "parked_on",
            DriveEvent::Completed { .. } => "completed",
            DriveEvent::SpawnedInvocation { .. } => "spawned_invocation",
            DriveEvent::StoreAlloc { .. } => "store_alloc",
            DriveEvent::RunRequested { .. } => "run_requested",
            DriveEvent::RunStarted { .. } => "run_started",
            DriveEvent::RunCompleted { .. } => "run_completed",
            DriveEvent::Observation { .. } => "observation",
            DriveEvent::ArtifactProbe { .. } => "artifact_probe",
        };
        *counts.entry(key).or_default() += 1;
    }
    counts
}

fn fn_event_summary(machine: &Machine, names: &[&str]) -> Vec<(String, Option<TraceCounts>)> {
    names
        .iter()
        .map(|name| ((*name).to_owned(), trace_counts(machine, &[*name]).ok()))
        .collect()
}

fn feature_unification_superset_fixture() -> Fixture {
    Fixture::new("feature-unification-superset", "app")
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

fn append_diag_snapshot(out: &mut String, diag: &MachineDiagSnapshot) {
    writeln!(
        out,
        "diag\twhole_projection_reads\t{}",
        diag.whole_projection_reads
    )
    .ok();
    writeln!(
        out,
        "diag\twhole_array_projection_reads\t{}",
        diag.whole_array_projection_reads
    )
    .ok();
    writeln!(
        out,
        "diag\tcanonical_word_hash_calls\t{}",
        diag.canonical_word_hash_calls
    )
    .ok();
    writeln!(
        out,
        "diag\tcanonical_word_hash_store_hits\t{}",
        diag.canonical_word_hash_store_hits
    )
    .ok();
    writeln!(
        out,
        "diag\traw_value_hash_calls\t{}",
        diag.raw_value_hash_calls
    )
    .ok();
    writeln!(
        out,
        "diag\traw_value_hash_bytes\t{}",
        diag.raw_value_hash_bytes
    )
    .ok();
    writeln!(
        out,
        "diag\tstructured_value_hash_calls\t{}",
        diag.structured_value_hash_calls
    )
    .ok();
    writeln!(
        out,
        "diag\tstructured_value_hash_bytes\t{}",
        diag.structured_value_hash_bytes
    )
    .ok();
    writeln!(
        out,
        "diag\tstructured_array_elements\t{}",
        diag.structured_array_elements
    )
    .ok();
    writeln!(
        out,
        "diag\tstructured_sequence_elements\t{}",
        diag.structured_sequence_elements
    )
    .ok();
    writeln!(
        out,
        "diag\tstructured_map_pairs\t{}",
        diag.structured_map_pairs
    )
    .ok();
    for (schema, count) in &diag.projection_schemas {
        writeln!(out, "projection_schema\t{schema}\t{count}").ok();
    }
}

#[derive(Debug)]
struct DenseDiagEventLimit;

const DENSE_DIAG_EVENT_LIMIT: u64 = 200_000;

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
    let fixture = feature_unification_superset_fixture();

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

#[test]
#[ignore = "tier-A dense-state diagnostic: feature-heavy fixture trace summary"]
fn dense_state_feature_unification_diagnostic_trace() -> Result<(), String> {
    let fixture = feature_unification_superset_fixture();
    let source = format!("{}\n\n{}", rodin_source()?, fixture.vix_fixture_source());
    reset_machine_diag();
    set_machine_diag_enabled(true);
    let started = Instant::now();
    type DiagnosticRun = (
        &'static str,
        usize,
        BTreeSet<String>,
        MachineDiagSnapshot,
        BTreeMap<&'static str, usize>,
        Vec<(String, Option<TraceCounts>)>,
    );
    let result: Result<DiagnosticRun, String> = (|| {
        let mut machine = Machine::load(&source)?;
        machine.set_force_molten_copy(true);
        let mut event_count = 0_u64;
        machine.set_event_sink(Some(Box::new(move |_| {
            event_count = event_count.saturating_add(1);
            if event_count >= DENSE_DIAG_EVENT_LIMIT {
                std::panic::panic_any(DenseDiagEventLimit);
            }
            StepCommand::Step
        })));
        let selected = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            machine.call(
                "fixture_selected",
                &[NamedArg {
                    name: "target".to_owned(),
                    value: MachineArg::String(LINUX.to_owned()),
                }],
            )
        })) {
            Ok(Ok(selected)) => {
                let rendered = machine
                    .render_result("fixture_selected", selected.0)
                    .map_err(|err| format!("render fixture_selected: {err}"))?;
                rendered_name_set(rendered)?
            }
            Ok(Err(err)) => {
                return Err(format!(
                    "call fixture_selected: {err}\n{}",
                    trace_tail(&machine)
                ));
            }
            Err(payload) if payload.is::<DenseDiagEventLimit>() => BTreeSet::new(),
            Err(payload) => std::panic::resume_unwind(payload),
        };
        let status = if selected.is_empty() {
            "event_limit"
        } else {
            "completed"
        };
        let diag = machine_diag_snapshot();
        let trace = trace_summary(&machine);
        let fn_counts = fn_event_summary(
            &machine,
            &[
                "propagate",
                "propagate_loop",
                "apply_clauses",
                "apply_one_clause",
                "enable_feature",
                "feature_enabled",
                "domain_for",
                "force_singletons_over",
                "selected_from_state",
            ],
        );
        Ok((
            status,
            machine.trace().len(),
            selected,
            diag,
            trace,
            fn_counts,
        ))
    })();
    set_machine_diag_enabled(false);
    let (status, trace_len, selected, diag, trace, fn_counts) = result?;
    let wall = started.elapsed();
    let mut out = String::new();
    writeln!(out, "metric\tname\tvalue").ok();
    writeln!(out, "run\tfixture\tfeature_unification_superset").ok();
    writeln!(out, "run\tforce_molten_copy\ttrue").ok();
    writeln!(out, "run\tstatus\t{status}").ok();
    writeln!(out, "run\tevent_limit\t{DENSE_DIAG_EVENT_LIMIT}").ok();
    writeln!(out, "run\twall_ms\t{}", wall.as_millis()).ok();
    writeln!(out, "run\ttrace_len\t{trace_len}").ok();
    writeln!(out, "run\tselected_count\t{}", selected.len()).ok();
    for name in &selected {
        writeln!(out, "selected\t{name}\t1").ok();
    }
    for (event, count) in trace {
        writeln!(out, "trace_event\t{event}\t{count}").ok();
    }
    for (name, counts) in fn_counts {
        if let Some(counts) = counts {
            writeln!(out, "fn_demanded\t{name}\t{}", counts.demanded).ok();
            writeln!(
                out,
                "fn_spawned_invocations\t{name}\t{}",
                counts.spawned_invocations
            )
            .ok();
        } else {
            writeln!(out, "fn_missing\t{name}\t1").ok();
        }
    }
    append_diag_snapshot(&mut out, &diag);

    let out_dir = std::env::var_os("TIER_A_OUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/tier-a-scale-measurement"));
    std::fs::create_dir_all(&out_dir).map_err(|err| err.to_string())?;
    std::fs::write(
        out_dir.join("dense-state-feature-fixture-diagnostics.tsv"),
        out,
    )
    .map_err(|err| err.to_string())
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
