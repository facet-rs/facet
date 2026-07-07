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

use vix::machine::{Machine, MachineArg, NamedArg, RenderedValue};

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

    fn optional(mut self) -> Self {
        self.optional = true;
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
    features: BTreeMap<String, Vec<String>>,
    deps: Vec<FixtureDep>,
}

impl FixtureCrate {
    fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            features: BTreeMap::new(),
            deps: Vec::new(),
        }
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
            .map_err(|err| format!("call fixture_selected: {err}"))?;
        let rendered = machine
            .render_result("fixture_selected", selected.0)
            .map_err(|err| format!("render fixture_selected: {err}"))?;
        let selected = rendered_name_set(rendered)?;
        if selected.is_empty() {
            let root_candidates = machine
                .call("fixture_root_candidate_count", &[])
                .map_err(|err| format!("call fixture_root_candidate_count: {err}"))?;
            let root_versions = machine
                .call("fixture_root_version_count", &[])
                .map_err(|err| format!("call fixture_root_version_count: {err}"))?;
            let RenderedValue::Int { value } = machine
                .render_result("fixture_root_candidate_count", root_candidates.0)
                .map_err(|err| format!("render fixture_root_candidate_count: {err}"))?
            else {
                return Err("fixture_root_candidate_count did not render as Int".to_owned());
            };
            let RenderedValue::Int {
                value: version_count,
            } = machine
                .render_result("fixture_root_version_count", root_versions.0)
                .map_err(|err| format!("render fixture_root_version_count: {err}"))?
            else {
                return Err("fixture_root_version_count did not render as Int".to_owned());
            };
            return Err(format!(
                "rodin.vix selected no packages; root version count = {version_count}; root candidate count = {value}"
            ));
        }
        Ok(selected)
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
        writeln!(toml, "version = \"0.1.0\"").ok();
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
        for (version_id, krate) in self.crates.iter().enumerate() {
            let pkg_id = package_indices[&krate.name];
            writeln!(
                source,
                "    let version_pkgs = version_pkgs.insert({version_id}, {pkg_id});"
            )
            .ok();
            writeln!(
                source,
                "    let version_values = version_values.insert({version_id}, \"0.1.0\");"
            )
            .ok();
        }
        let version_ids = (0..self.crates.len())
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
            "guard_pkgs: Map<Int, Int>",
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
            "    Index {{ packages: [{packages}], names: names, version_ids: [{version_ids}], version_pkgs: version_pkgs, version_values: version_values, clause_ids: [{}], guard_ids: [{}], guard_clause_ids: guard_clause_ids, guard_tags: guard_tags, guard_pkgs: guard_pkgs, guard_features: guard_features, consequent_tags: consequent_tags, consequent_pkgs: consequent_pkgs, consequent_version_sets: consequent_version_sets, consequent_features: consequent_features, gate_kinds: gate_kinds, gate_targets: gate_targets }}",
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
            "\npub fn fixture_root_candidate_count() -> Int {{\n    root_candidate_count(fixture_index(), fixture_problem())\n}}"
        )
        .ok();
        writeln!(
            source,
            "\npub fn fixture_root_version_count() -> Int {{\n    root_version_count(fixture_index(), fixture_problem())\n}}"
        )
        .ok();

        source
    }

    fn vix_clauses(&self) -> VixClauses {
        let mut clauses = VixClauses::new(self.package_indices(), self.feature_indices());
        let mut next_id = 0;

        for krate in &self.crates {
            for dep in &krate.deps {
                if !dep.optional {
                    clauses.push(vix_clause(
                        next_id,
                        &[guard_in_graph(&krate.name)],
                        consequent_in_graph(&dep.name),
                        Some(dep_gate(krate, dep)),
                    ));
                    next_id += 1;
                }

                let version_guards = if dep.optional {
                    vec![guard_in_graph(&krate.name), guard_in_graph(&dep.name)]
                } else {
                    vec![guard_in_graph(&krate.name)]
                };
                clauses.push(vix_clause(
                    next_id,
                    &version_guards,
                    consequent_version(&dep.name, "0.1.0"),
                    Some(dep_gate(krate, dep)),
                ));
                next_id += 1;

                if dep.default_features {
                    clauses.push(vix_clause(
                        next_id,
                        &[guard_in_graph(&krate.name), guard_in_graph(&dep.name)],
                        consequent_feature(&dep.name, "default"),
                        Some(dep_gate(krate, dep)),
                    ));
                    next_id += 1;
                }

                for feature in &dep.features {
                    clauses.push(vix_clause(
                        next_id,
                        &[guard_in_graph(&krate.name), guard_in_graph(&dep.name)],
                        consequent_feature(&dep.name, feature),
                        Some(dep_gate(krate, dep)),
                    ));
                    next_id += 1;
                }
            }

            for (feature, enables) in &krate.features {
                for enable in enables {
                    for clause in self.feature_enable_clauses(next_id, krate, feature, enable) {
                        clauses.push(clause);
                        next_id += 1;
                    }
                }
            }
        }

        clauses
    }

    fn feature_enable_clauses(
        &self,
        id: i32,
        krate: &FixtureCrate,
        feature: &str,
        enable: &str,
    ) -> Vec<VixClause> {
        let base_guards = vec![
            guard_in_graph(&krate.name),
            guard_feature(&krate.name, feature),
        ];
        if let Some(dep_name) = enable.strip_prefix("dep:") {
            let gate = self
                .feature_dep(krate, dep_name)
                .map(|dep| dep_gate(krate, dep));
            return vec![vix_clause(
                id,
                &base_guards,
                consequent_in_graph(dep_name),
                gate,
            )];
        }

        if let Some((dep_name, dep_feature)) = enable.split_once("?/") {
            let mut guards = base_guards;
            guards.push(guard_in_graph(dep_name));
            let gate = self
                .feature_dep(krate, dep_name)
                .map(|dep| dep_gate(krate, dep));
            return vec![vix_clause(
                id,
                &guards,
                consequent_feature(dep_name, dep_feature),
                gate,
            )];
        }

        if let Some((dep_name, dep_feature)) = enable.split_once('/') {
            let gate = self
                .feature_dep(krate, dep_name)
                .map(|dep| dep_gate(krate, dep));
            let mut feature_guards = base_guards.clone();
            feature_guards.push(guard_in_graph(dep_name));
            return vec![
                vix_clause(
                    id,
                    &base_guards,
                    consequent_in_graph(dep_name),
                    gate.clone(),
                ),
                vix_clause(
                    id + 1,
                    &feature_guards,
                    consequent_feature(dep_name, dep_feature),
                    gate,
                ),
            ];
        }

        vec![vix_clause(
            id,
            &base_guards,
            consequent_feature(&krate.name, enable),
            None,
        )]
    }

    fn feature_dep<'a>(&'a self, krate: &'a FixtureCrate, name: &str) -> Option<&'a FixtureDep> {
        krate
            .deps
            .iter()
            .find(|dep| dep.name == name && dep.optional)
            .or_else(|| krate.deps.iter().find(|dep| dep.name == name))
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
            register_feature(&mut features, &krate.name, "default");
            for feature in krate.features.keys() {
                register_feature(&mut features, &krate.name, feature);
            }
            for dep in &krate.deps {
                if dep.default_features {
                    register_feature(&mut features, &dep.name, "default");
                }
                for feature in &dep.features {
                    register_feature(&mut features, &dep.name, feature);
                }
            }
            for enables in krate.features.values() {
                for enable in enables {
                    if let Some(dep_name) = enable.strip_prefix("dep:") {
                        register_feature(&mut features, dep_name, "default");
                    } else if let Some((dep_name, dep_feature)) = enable.split_once("?/") {
                        register_feature(&mut features, dep_name, dep_feature);
                    } else if let Some((dep_name, dep_feature)) = enable.split_once('/') {
                        register_feature(&mut features, dep_name, dep_feature);
                    } else {
                        register_feature(&mut features, &krate.name, enable);
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
                "let guard_pkgs = guard_pkgs.insert({guard_id}, {});",
                self.pkg_id(guard.pkg())
            ));
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
    Feature { name: String, feature: String },
}

impl VixGuard {
    fn tag(&self) -> &'static str {
        match self {
            Self::InGraph { .. } => "in_graph",
            Self::Feature { .. } => "feature",
        }
    }

    fn pkg(&self) -> &str {
        match self {
            Self::InGraph { name } | Self::Feature { name, .. } => name,
        }
    }

    fn feature(&self) -> &str {
        match self {
            Self::Feature { feature, .. } => feature,
            Self::InGraph { .. } => "",
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
        "version = \"0.1.0\"".to_owned(),
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
#[ignore = "pending optional weak feature activation fix in rodin.vix"]
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
