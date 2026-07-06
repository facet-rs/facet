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
        self.features
            .insert(name.into(), enables.iter().map(|s| (*s).to_owned()).collect());
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
    /// Seam for the native resolver (rodin/docs/40-search.md). Unimplemented until
    /// rodin.vix can resolve a workspace; the differential corpus is #[ignore]'d
    /// until then.
    fn vix_selected(&self, _triple: &str) -> Result<BTreeSet<String>, String> {
        Err("rodin.vix native resolver not implemented yet (rodin/docs/40-search.md)".to_owned())
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
        let mut tables: BTreeMap<(Option<String>, &'static str), Vec<&FixtureDep>> = BTreeMap::new();
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
            "tree", "-e", "normal,build", "--target", triple, "--prefix", "none", "--offline", "-p",
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
    std::env::temp_dir().join(format!("rodin-fixture-{name}-{}-{nonce}", std::process::id()))
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
#[ignore = "pending rodin.vix native resolver (rodin/docs/40-search.md)"]
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
        let selected = fixture.assert_selection_matches(target).expect("selection matches");
        assert!(!selected.contains("helper"), "no helper on {target}: {selected:?}");
    }
}

/// 2. A default-on feature (`bundle-platform`) references an optional dep
///    declared only for `cfg(windows)`: active on windows, not linux.
#[ignore = "pending rodin.vix native resolver (rodin/docs/40-search.md)"]
#[test]
fn feature_activated_target_conditional_optional_dep() {
    let fixture = Fixture::new("feature-target-optional", "app")
        .krate(FixtureCrate::new("platform"))
        .krate(
            FixtureCrate::new("lib")
                .feature("default", &["bundle-platform"])
                .feature("bundle-platform", &["dep:platform"])
                .dep(FixtureDep::new("platform").optional().target("cfg(windows)")),
        )
        .krate(FixtureCrate::new("app").dep(FixtureDep::new("lib")));

    assert!(fixture.assert_selection_matches(WINDOWS).unwrap().contains("platform"));
    assert!(!fixture.assert_selection_matches(LINUX).unwrap().contains("platform"));
}

/// 3. A plain `cfg(windows)` dependency edge (winapi shape): present on windows,
///    absent on linux.
#[ignore = "pending rodin.vix native resolver (rodin/docs/40-search.md)"]
#[test]
fn direct_target_conditional_edge() {
    let fixture = Fixture::new("direct-cfg-windows", "app")
        .krate(FixtureCrate::new("winthing"))
        .krate(FixtureCrate::new("app").dep(FixtureDep::new("winthing").target("cfg(windows)")));

    assert!(fixture.assert_selection_matches(WINDOWS).unwrap().contains("winthing"));
    assert!(!fixture.assert_selection_matches(LINUX).unwrap().contains("winthing"));
}

/// 4. A build-dependency is part of the host graph and consumed on every target.
#[ignore = "pending rodin.vix native resolver (rodin/docs/40-search.md)"]
#[test]
fn build_dependency_is_consumed() {
    let fixture = Fixture::new("build-dep", "app")
        .krate(FixtureCrate::new("gen"))
        .krate(FixtureCrate::new("app").dep(FixtureDep::new("gen").kind(DepKind::Build)));

    assert!(fixture.assert_selection_matches(LINUX).unwrap().contains("gen"));
}

/// 5. A dev-dependency of a non-root crate is not consumed by a normal build of
///    the root.
#[ignore = "pending rodin.vix native resolver (rodin/docs/40-search.md)"]
#[test]
fn transitive_dev_dependency_is_not_consumed() {
    let fixture = Fixture::new("dev-dep", "app")
        .krate(FixtureCrate::new("testonly"))
        .krate(FixtureCrate::new("lib").dep(FixtureDep::new("testonly").kind(DepKind::Dev)))
        .krate(FixtureCrate::new("app").dep(FixtureDep::new("lib")));

    let selected = fixture.assert_selection_matches(LINUX).unwrap();
    assert!(!selected.contains("testonly"), "dev dep of lib not built: {selected:?}");
}
