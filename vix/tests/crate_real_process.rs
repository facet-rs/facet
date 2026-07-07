#![cfg(all(feature = "real-process", not(target_arch = "wasm32")))]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Instant;

use facet::Facet;
use vix::exec::Tree;
use vix::machine::{DriveEvent, Machine, MachineArg, RenderedValue};
use vix::real_process::RealProcessBackend;

const SOURCE: &str = include_str!("../../playgrounds/snark/src/bundled/vix/samples/crate.vix");
const RODIN_SOURCE: &str = include_str!("../../rodin/rodin.vix");
const APP_MANIFEST: &str = include_str!(
    "../../playgrounds/snark/src/bundled/vix/samples/fixtures/two_crate_graph/app/Cargo.toml"
);
const APP_MAIN: &str = include_str!(
    "../../playgrounds/snark/src/bundled/vix/samples/fixtures/two_crate_graph/app/src/main.rs"
);
const HELPER_MANIFEST: &str = include_str!(
    "../../playgrounds/snark/src/bundled/vix/samples/fixtures/two_crate_graph/crates/helper/Cargo.toml"
);
const HELPER_LIB: &str = include_str!(
    "../../playgrounds/snark/src/bundled/vix/samples/fixtures/two_crate_graph/crates/helper/src/lib.rs"
);
const EXPECTED_STDOUT: &[u8] = b"vix dependency fixture\n";
const LOCK_GRAPH_LOCK: &str =
    include_str!("../../playgrounds/snark/src/bundled/vix/samples/fixtures/lock_graph/Cargo.lock");
const LOCK_GRAPH_APP_LOCK: &str = include_str!(
    "../../playgrounds/snark/src/bundled/vix/samples/fixtures/lock_graph/app/Cargo.lock"
);
const LOCK_GRAPH_APP_MANIFEST: &str = include_str!(
    "../../playgrounds/snark/src/bundled/vix/samples/fixtures/lock_graph/app/Cargo.toml"
);
const LOCK_GRAPH_APP_MAIN: &str = include_str!(
    "../../playgrounds/snark/src/bundled/vix/samples/fixtures/lock_graph/app/src/main.rs"
);
const LOCK_GRAPH_ALPHA_MANIFEST: &str = include_str!(
    "../../playgrounds/snark/src/bundled/vix/samples/fixtures/lock_graph/crates/alpha_lib/Cargo.toml"
);
const LOCK_GRAPH_ALPHA_LIB: &str = include_str!(
    "../../playgrounds/snark/src/bundled/vix/samples/fixtures/lock_graph/crates/alpha_lib/src/lib.rs"
);
const LOCK_GRAPH_CORE_MANIFEST: &str = include_str!(
    "../../playgrounds/snark/src/bundled/vix/samples/fixtures/lock_graph/crates/core_lib/Cargo.toml"
);
const LOCK_GRAPH_CORE_LIB: &str = include_str!(
    "../../playgrounds/snark/src/bundled/vix/samples/fixtures/lock_graph/crates/core_lib/src/lib.rs"
);
const LOCK_GRAPH_FORMATTING_MANIFEST: &str = include_str!(
    "../../playgrounds/snark/src/bundled/vix/samples/fixtures/lock_graph/crates/formatting_lib/Cargo.toml"
);
const LOCK_GRAPH_FORMATTING_LIB: &str = include_str!(
    "../../playgrounds/snark/src/bundled/vix/samples/fixtures/lock_graph/crates/formatting_lib/src/lib.rs"
);
const LOCK_GRAPH_EXPECTED_STDOUT: &[u8] = b"core via alpha + formatted\n";
const BUILD_SCRIPT_LOCK: &str = include_str!(
    "../../playgrounds/snark/src/bundled/vix/samples/fixtures/build_script/Cargo.lock"
);
const BUILD_SCRIPT_APP_LOCK: &str = include_str!(
    "../../playgrounds/snark/src/bundled/vix/samples/fixtures/build_script/app/Cargo.lock"
);
const BUILD_SCRIPT_MANIFEST: &str = include_str!(
    "../../playgrounds/snark/src/bundled/vix/samples/fixtures/build_script/app/Cargo.toml"
);
const BUILD_SCRIPT_RS: &str = include_str!(
    "../../playgrounds/snark/src/bundled/vix/samples/fixtures/build_script/app/build.rs"
);
const BUILD_SCRIPT_LIB: &str = include_str!(
    "../../playgrounds/snark/src/bundled/vix/samples/fixtures/build_script/app/src/lib.rs"
);
const BUILD_SCRIPT_MAIN: &str = include_str!(
    "../../playgrounds/snark/src/bundled/vix/samples/fixtures/build_script/app/src/main.rs"
);
const BUILD_SCRIPT_EXPECTED_STDOUT: &[u8] = b"vix-build-script-generated\n";

#[test]
fn real_process_rustc_builds_two_crate_fixture_and_matches_cargo_unit_graph_oracle()
-> Result<(), String> {
    if !host_rustc_available() {
        return Ok(());
    }

    let source = crate_source();
    let backend = Arc::new(RealProcessBackend::new());
    let mut machine = Machine::load(&source)?.with_exec_backend(backend);
    let target = machine.linux_target_handle();
    let graph = machine
        .intern_arg("Tree", MachineArg::Tree(two_crate_graph_tree()))?
        .0;

    let checked = machine.demand_i64("crate_bin_check", vec![target, graph])?;
    let _rmeta = tree_file_bytes(&mut machine, checked, "mini_app.rmeta")?;

    let built = machine.demand_i64("crate_bin", vec![target, graph])?;
    let bin = tree_file_bytes(&mut machine, built, "mini_app")?;
    let stdout = run_binary_bytes(&bin)?;
    if stdout != EXPECTED_STDOUT {
        return Err(format!(
            "unexpected binary stdout: {:?}",
            String::from_utf8_lossy(&stdout)
        ));
    }

    let machine_graph = machine_rustc_unit_graph(&machine)?;
    let cargo_graph = cargo_unit_graph_oracle()?;
    if machine_graph != cargo_graph {
        return Err(format!(
            "machine unit graph did not match cargo oracle\nmachine: {machine_graph:#?}\ncargo: {cargo_graph:#?}"
        ));
    }

    Ok(())
}

#[test]
fn real_process_rustc_builds_lockfile_graph_and_matches_cargo_unit_graph_oracle()
-> Result<(), String> {
    if !host_rustc_available() {
        return Ok(());
    }

    let source = crate_source();
    let backend = Arc::new(RealProcessBackend::new());
    let mut machine = Machine::load(&source)?.with_exec_backend(backend);
    let target = machine.linux_target_handle();
    let graph = machine
        .intern_arg("Tree", MachineArg::Tree(lock_graph_tree()))?
        .0;

    let cold_start = Instant::now();
    let checked = machine.demand_i64("crate_lock_bin_check", vec![target, graph])?;
    let _rmeta = tree_file_bytes(&mut machine, checked, "mini_app.rmeta")?;

    let built = machine.demand_i64("crate_lock_bin", vec![target, graph])?;
    let bin = tree_file_bytes(&mut machine, built, "mini_app")?;
    let stdout = run_binary_bytes(&bin)?;
    if stdout != LOCK_GRAPH_EXPECTED_STDOUT {
        return Err(format!(
            "unexpected lock graph binary stdout: {:?}",
            String::from_utf8_lossy(&stdout)
        ));
    }
    let cold_elapsed = cold_start.elapsed();
    let built_tree = tree_snapshot(&mut machine, built)?;

    let machine_graph = machine_rustc_unit_graph(&machine)?;
    let cargo_graph = cargo_lock_graph_unit_graph_oracle()?;
    if machine_graph != cargo_graph {
        return Err(format!(
            "lock graph machine unit graph did not match cargo oracle\nmachine: {machine_graph:#?}\ncargo: {cargo_graph:#?}"
        ));
    }

    machine.clear_trace();
    let warm_start = Instant::now();
    let warm = machine.demand_i64("crate_lock_bin", vec![target, graph])?;
    let warm_tree = tree_snapshot(&mut machine, warm)?;
    let warm_elapsed = warm_start.elapsed();
    if warm_tree != built_tree {
        return Err(format!(
            "warm crate_lock_bin tree differed from cold tree\ncold: {built_tree:?}\nwarm: {warm_tree:?}"
        ));
    }
    let warm_requested = machine
        .trace()
        .iter()
        .filter(|event| matches!(event, DriveEvent::RunRequested { .. }))
        .count();
    if warm_requested != 0 {
        return Err(format!(
            "warm crate_lock_bin emitted {warm_requested} RunRequested events: {:?}",
            machine.trace()
        ));
    }
    eprintln!("crate_lock_bin machine wall: cold={cold_elapsed:?} warm={warm_elapsed:?}");

    Ok(())
}

#[test]
fn generic_walk_builds_resolved_graph_and_matches_cargo_oracle() -> Result<(), String> {
    if !host_rustc_available() {
        return Ok(());
    }

    let source = generic_lock_graph_source();
    let backend = Arc::new(RealProcessBackend::new());
    let mut machine = Machine::load(&source)?.with_exec_backend(backend);
    let target = machine.linux_target_handle();
    let graph = machine
        .intern_arg("Tree", MachineArg::Tree(lock_graph_tree()))?
        .0;

    let checked =
        demand_with_rustc_trace(&mut machine, "generic_lock_bin_check", vec![target, graph])?;
    let _rmeta = tree_file_bytes(&mut machine, checked, "mini_app.rmeta")?;

    let built = demand_with_rustc_trace(&mut machine, "generic_lock_bin", vec![target, graph])?;
    let bin = tree_file_bytes(&mut machine, built, "mini_app")?;
    let stdout = run_binary_bytes(&bin)?;
    if stdout != LOCK_GRAPH_EXPECTED_STDOUT {
        return Err(format!(
            "unexpected generic lock graph stdout: {:?}",
            String::from_utf8_lossy(&stdout)
        ));
    }

    let machine_graph = machine_rustc_unit_graph(&machine)?;
    let cargo_graph = cargo_lock_graph_unit_graph_oracle()?;
    if machine_graph != cargo_graph {
        return Err(format!(
            "generic walk unit graph did not match cargo oracle\nmachine: {machine_graph:#?}\ncargo: {cargo_graph:#?}"
        ));
    }

    Ok(())
}

#[test]
fn solution_walk_derives_units_from_rodin_and_matches_cargo_oracle() -> Result<(), String> {
    if !host_rustc_available() {
        return Ok(());
    }

    let source = generic_lock_graph_source();
    let backend = Arc::new(RealProcessBackend::new());
    let mut machine = Machine::load(&source)?.with_exec_backend(backend);
    let target = machine.linux_target_handle();
    let graph = machine
        .intern_arg("Tree", MachineArg::Tree(lock_graph_tree()))?
        .0;

    let checked =
        demand_with_rustc_trace(&mut machine, "derived_lock_bin_check", vec![target, graph])?;
    let _rmeta = tree_file_bytes(&mut machine, checked, "mini_app.rmeta")?;

    let built = demand_with_rustc_trace(&mut machine, "derived_lock_bin", vec![target, graph])?;
    let bin = tree_file_bytes(&mut machine, built, "mini_app")?;
    let stdout = run_binary_bytes(&bin)?;
    if stdout != LOCK_GRAPH_EXPECTED_STDOUT {
        return Err(format!(
            "unexpected derived lock graph stdout: {:?}",
            String::from_utf8_lossy(&stdout)
        ));
    }

    let machine_graph = machine_rustc_unit_graph(&machine)?;
    let cargo_graph = cargo_lock_graph_unit_graph_oracle()?;
    let diff = diff_unit_graphs(&machine_graph, &cargo_graph);
    if !diff.is_exact_match() {
        return Err(format!(
            "derived unit graph did not match cargo oracle\nsummary: {diff:#?}\nmachine: {machine_graph:#?}\ncargo: {cargo_graph:#?}"
        ));
    }
    assert_eq!(diff.machine_units, 4, "{diff:#?}");
    assert_eq!(diff.cargo_units, 4, "{diff:#?}");
    assert_eq!(diff.unit_matches, 4, "{diff:#?}");
    assert!(diff.edge_matches > 0, "{diff:#?}");
    write_tier_a_artifact(
        "lock-fixture-unit-diff-summary.tsv",
        &unit_graph_diff_summary_table(&diff),
    )?;

    Ok(())
}

#[test]
fn real_process_runs_build_script_threads_directives_and_out_dir_into_parent_rustc()
-> Result<(), String> {
    if !host_rustc_available() {
        return Ok(());
    }

    let source = crate_source();
    let backend = Arc::new(RealProcessBackend::new());
    let mut machine = Machine::load(&source)?.with_exec_backend(backend);
    let target = machine.linux_target_handle();
    let graph = machine
        .intern_arg("Tree", MachineArg::Tree(build_script_tree()))?
        .0;

    let built = machine.demand_i64("crate_build_script_bin", vec![target, graph])?;
    let bin = tree_file_bytes(&mut machine, built, "build_script_runner")?;
    let stdout = run_named_binary_bytes(&bin, "build_script_runner")?;
    if stdout != BUILD_SCRIPT_EXPECTED_STDOUT {
        return Err(format!(
            "unexpected build-script fixture stdout: {:?}",
            String::from_utf8_lossy(&stdout)
        ));
    }

    let machine_graph = machine_rustc_unit_graph(&machine)?;
    let cargo_graph = cargo_build_script_unit_graph_oracle()?;
    if machine_graph != cargo_graph {
        return Err(format!(
            "build-script machine unit graph did not match cargo oracle\nmachine: {machine_graph:#?}\ncargo: {cargo_graph:#?}"
        ));
    }

    let completed = machine
        .trace()
        .iter()
        .filter_map(|event| match event {
            DriveEvent::RunCompleted {
                command_name,
                outputs,
                ..
            } if command_name == "build_script" => Some(outputs),
            _ => None,
        })
        .next()
        .ok_or_else(|| "missing build_script completion event".to_string())?;
    if !completed.iter().any(|(path, _)| path == "build.stdout")
        || !completed.iter().any(|(path, _)| path == "out/generated.rs")
    {
        return Err(format!(
            "build_script completion did not expose stdout and OUT_DIR: {completed:?}"
        ));
    }

    Ok(())
}

fn host_rustc_available() -> bool {
    Command::new("rustc")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

fn two_crate_graph_tree() -> Tree {
    Tree::of(&[
        ("app/Cargo.toml", APP_MANIFEST),
        ("app/src/main.rs", APP_MAIN),
        ("crates/helper/Cargo.toml", HELPER_MANIFEST),
        ("crates/helper/src/lib.rs", HELPER_LIB),
    ])
}

fn lock_graph_tree() -> Tree {
    Tree::of(&[
        ("Cargo.lock", LOCK_GRAPH_LOCK),
        ("app/Cargo.lock", LOCK_GRAPH_APP_LOCK),
        ("app/Cargo.toml", LOCK_GRAPH_APP_MANIFEST),
        ("app/src/main.rs", LOCK_GRAPH_APP_MAIN),
        ("crates/alpha_lib/Cargo.toml", LOCK_GRAPH_ALPHA_MANIFEST),
        ("crates/alpha_lib/src/lib.rs", LOCK_GRAPH_ALPHA_LIB),
        ("crates/core_lib/Cargo.toml", LOCK_GRAPH_CORE_MANIFEST),
        ("crates/core_lib/src/lib.rs", LOCK_GRAPH_CORE_LIB),
        (
            "crates/formatting_lib/Cargo.toml",
            LOCK_GRAPH_FORMATTING_MANIFEST,
        ),
        (
            "crates/formatting_lib/src/lib.rs",
            LOCK_GRAPH_FORMATTING_LIB,
        ),
    ])
}

fn build_script_tree() -> Tree {
    Tree::of(&[
        ("Cargo.lock", BUILD_SCRIPT_LOCK),
        ("app/Cargo.lock", BUILD_SCRIPT_APP_LOCK),
        ("app/Cargo.toml", BUILD_SCRIPT_MANIFEST),
        ("app/build.rs", BUILD_SCRIPT_RS),
        ("app/src/lib.rs", BUILD_SCRIPT_LIB),
        ("app/src/main.rs", BUILD_SCRIPT_MAIN),
    ])
}

fn generic_lock_graph_source() -> String {
    format!("{RODIN_SOURCE}\n\n{SOURCE}\n\n{GENERIC_LOCK_GRAPH_BRIDGE}")
}

fn crate_source() -> String {
    format!("{RODIN_SOURCE}\n\n{SOURCE}")
}

const GENERIC_LOCK_GRAPH_BRIDGE: &str = r#"
fn fixture_index() -> Index {
    let names: Map<Int, String> = {};
    let names = names.insert(0, "alpha_lib");
    let names = names.insert(1, "core_lib");
    let names = names.insert(2, "formatting_lib");
    let names = names.insert(3, "mini_app");

    let version_pkgs: Map<Int, Int> = {};
    let version_pkgs = version_pkgs.insert(0, 0);
    let version_pkgs = version_pkgs.insert(1, 1);
    let version_pkgs = version_pkgs.insert(2, 2);
    let version_pkgs = version_pkgs.insert(3, 3);

    let version_values: Map<Int, String> = {};
    let version_values = version_values.insert(0, "0.1.0");
    let version_values = version_values.insert(1, "0.1.0");
    let version_values = version_values.insert(2, "0.1.0");
    let version_values = version_values.insert(3, "0.1.0");

    let guard_clause_ids: Map<Int, Int> = {};
    let guard_tags: Map<Int, String> = {};
    let guard_kinds: Map<Int, Int> = {};
    let guard_pkgs: Map<Int, Int> = {};
    let guard_version_values: Map<Int, String> = {};
    let guard_features: Map<Int, Int> = {};
    let consequent_tags: Map<Int, String> = {};
    let consequent_pkgs: Map<Int, Int> = {};
    let consequent_version_sets: Map<Int, VersionSet> = {};
    let consequent_features: Map<Int, Int> = {};
    let gate_kinds: Map<Int, String> = {};
    let gate_targets: Map<Int, String> = {};

    let guard_clause_ids = guard_clause_ids.insert(0, 0);
    let guard_tags = guard_tags.insert(0, "in_graph");
    let guard_kinds = guard_kinds.insert(0, 0);
    let guard_pkgs = guard_pkgs.insert(0, 3);
    let guard_features = guard_features.insert(0, 0);
    let consequent_tags = consequent_tags.insert(0, "in_graph");
    let consequent_pkgs = consequent_pkgs.insert(0, 0);
    let consequent_version_sets = consequent_version_sets.insert(0, VersionSet::from_req("*"));
    let consequent_features = consequent_features.insert(0, 0);
    let gate_kinds = gate_kinds.insert(0, "normal");

    let guard_clause_ids = guard_clause_ids.insert(1, 1);
    let guard_tags = guard_tags.insert(1, "in_graph");
    let guard_kinds = guard_kinds.insert(1, 0);
    let guard_pkgs = guard_pkgs.insert(1, 3);
    let guard_features = guard_features.insert(1, 0);
    let consequent_tags = consequent_tags.insert(1, "version_set");
    let consequent_pkgs = consequent_pkgs.insert(1, 0);
    let consequent_version_sets = consequent_version_sets.insert(1, VersionSet::from_req("0.1.0"));
    let consequent_features = consequent_features.insert(1, 0);
    let gate_kinds = gate_kinds.insert(1, "normal");

    let guard_clause_ids = guard_clause_ids.insert(2, 2);
    let guard_tags = guard_tags.insert(2, "in_graph");
    let guard_kinds = guard_kinds.insert(2, 0);
    let guard_pkgs = guard_pkgs.insert(2, 3);
    let guard_features = guard_features.insert(2, 0);
    let consequent_tags = consequent_tags.insert(2, "in_graph");
    let consequent_pkgs = consequent_pkgs.insert(2, 2);
    let consequent_version_sets = consequent_version_sets.insert(2, VersionSet::from_req("*"));
    let consequent_features = consequent_features.insert(2, 0);
    let gate_kinds = gate_kinds.insert(2, "normal");

    let guard_clause_ids = guard_clause_ids.insert(3, 3);
    let guard_tags = guard_tags.insert(3, "in_graph");
    let guard_kinds = guard_kinds.insert(3, 0);
    let guard_pkgs = guard_pkgs.insert(3, 3);
    let guard_features = guard_features.insert(3, 0);
    let consequent_tags = consequent_tags.insert(3, "version_set");
    let consequent_pkgs = consequent_pkgs.insert(3, 2);
    let consequent_version_sets = consequent_version_sets.insert(3, VersionSet::from_req("0.1.0"));
    let consequent_features = consequent_features.insert(3, 0);
    let gate_kinds = gate_kinds.insert(3, "normal");

    let guard_clause_ids = guard_clause_ids.insert(4, 4);
    let guard_tags = guard_tags.insert(4, "in_graph");
    let guard_kinds = guard_kinds.insert(4, 0);
    let guard_pkgs = guard_pkgs.insert(4, 0);
    let guard_features = guard_features.insert(4, 0);
    let consequent_tags = consequent_tags.insert(4, "in_graph");
    let consequent_pkgs = consequent_pkgs.insert(4, 1);
    let consequent_version_sets = consequent_version_sets.insert(4, VersionSet::from_req("*"));
    let consequent_features = consequent_features.insert(4, 0);
    let gate_kinds = gate_kinds.insert(4, "normal");

    let guard_clause_ids = guard_clause_ids.insert(5, 5);
    let guard_tags = guard_tags.insert(5, "in_graph");
    let guard_kinds = guard_kinds.insert(5, 0);
    let guard_pkgs = guard_pkgs.insert(5, 0);
    let guard_features = guard_features.insert(5, 0);
    let consequent_tags = consequent_tags.insert(5, "version_set");
    let consequent_pkgs = consequent_pkgs.insert(5, 1);
    let consequent_version_sets = consequent_version_sets.insert(5, VersionSet::from_req("0.1.0"));
    let consequent_features = consequent_features.insert(5, 0);
    let gate_kinds = gate_kinds.insert(5, "normal");

    Index {
        packages: [0, 1, 2, 3],
        names: names,
        version_ids: [0, 1, 2, 3],
        version_pkgs: version_pkgs,
        version_values: version_values,
        clause_ids: [0, 1, 2, 3, 4, 5],
        guard_ids: [0, 1, 2, 3, 4, 5],
        guard_clause_ids: guard_clause_ids,
        guard_tags: guard_tags,
        guard_kinds: guard_kinds,
        guard_pkgs: guard_pkgs,
        guard_version_values: guard_version_values,
        guard_features: guard_features,
        consequent_tags: consequent_tags,
        consequent_pkgs: consequent_pkgs,
        consequent_version_sets: consequent_version_sets,
        consequent_features: consequent_features,
        gate_kinds: gate_kinds,
        gate_targets: gate_targets,
    }
}

fn fixture_problem() -> Problem {
    Problem {
        root_pkg: 3,
        root_req: VersionSet::from_req("*"),
        root_features: [],
        root_default_feature: 0,
        root_default_features: false,
    }
}

fn selected_insert_unit(units: Map<Int, ResolvedUnit>, selected: Map<Int, Version>, pkg: Int, unit: ResolvedUnit) -> Map<Int, ResolvedUnit> {
    match selected.get(pkg) {
        Some(_) => units.insert(pkg, unit),
        None => units,
    }
}

fn fixture_unit_targets() -> UnitTargetTable {
    let targets: Map<Int, UnitTarget> = {};
    let targets = targets.insert(0, UnitTarget { kind: "lib", manifest: p"crates/alpha_lib", source: p"src/lib.rs", metadata: p"libalpha_lib.rmeta", link: p"libalpha_lib.rlib", metadata_file: "libalpha_lib.rmeta", link_file: "libalpha_lib.rlib" });
    let targets = targets.insert(1, UnitTarget { kind: "lib", manifest: p"crates/core_lib", source: p"src/lib.rs", metadata: p"libcore_lib.rmeta", link: p"libcore_lib.rlib", metadata_file: "libcore_lib.rmeta", link_file: "libcore_lib.rlib" });
    let targets = targets.insert(2, UnitTarget { kind: "lib", manifest: p"crates/formatting_lib", source: p"src/lib.rs", metadata: p"libformatting_lib.rmeta", link: p"libformatting_lib.rlib", metadata_file: "libformatting_lib.rmeta", link_file: "libformatting_lib.rlib" });
    let targets = targets.insert(3, UnitTarget { kind: "bin", manifest: p"app", source: p"src/main.rs", metadata: p"mini_app.rmeta", link: p"mini_app", metadata_file: "mini_app.rmeta", link_file: "mini_app" });
    UnitTargetTable { root: 3, targets: targets }
}

fn fixture_resolved_graph(target: String) -> ResolvedGraph {
    let result = solve(fixture_index(), fixture_problem(), target);
    let units: Map<Int, ResolvedUnit> = {};
    let units = selected_insert_unit(units, result.selected, 0, ResolvedUnit { name: "alpha_lib", kind: "lib", manifest: p"crates/alpha_lib", source: p"src/lib.rs", deps: [1], metadata: p"libalpha_lib.rmeta", link: p"libalpha_lib.rlib", metadata_file: "libalpha_lib.rmeta", link_file: "libalpha_lib.rlib", profile: default_resolved_profile("lib", "alpha_lib") });
    let units = selected_insert_unit(units, result.selected, 1, ResolvedUnit { name: "core_lib", kind: "lib", manifest: p"crates/core_lib", source: p"src/lib.rs", deps: [], metadata: p"libcore_lib.rmeta", link: p"libcore_lib.rlib", metadata_file: "libcore_lib.rmeta", link_file: "libcore_lib.rlib", profile: default_resolved_profile("lib", "core_lib") });
    let units = selected_insert_unit(units, result.selected, 2, ResolvedUnit { name: "formatting_lib", kind: "lib", manifest: p"crates/formatting_lib", source: p"src/lib.rs", deps: [], metadata: p"libformatting_lib.rmeta", link: p"libformatting_lib.rlib", metadata_file: "libformatting_lib.rmeta", link_file: "libformatting_lib.rlib", profile: default_resolved_profile("lib", "formatting_lib") });
    let units = selected_insert_unit(units, result.selected, 3, ResolvedUnit { name: "mini_app", kind: "bin", manifest: p"app", source: p"src/main.rs", deps: [0, 2], metadata: p"mini_app.rmeta", link: p"mini_app", metadata_file: "mini_app.rmeta", link_file: "mini_app", profile: default_resolved_profile("bin", "mini_app") });
    ResolvedGraph { root: 3, units: units }
}

pub fn generic_lock_bin_check(target: Target, graph: Tree) -> Tree {
    crate_resolved_bin_check(target, graph, fixture_resolved_graph("x86_64-unknown-linux-gnu"))
}

pub fn generic_lock_bin(target: Target, graph: Tree) -> Tree {
    crate_resolved_bin(target, graph, fixture_resolved_graph("x86_64-unknown-linux-gnu"))
}

pub fn derived_lock_bin_check(target: Target, graph: Tree) -> Tree {
    crate_solution_bin_check(target, graph, fixture_index(), fixture_problem(), "x86_64-unknown-linux-gnu", fixture_unit_targets())
}

pub fn derived_lock_bin(target: Target, graph: Tree) -> Tree {
    crate_solution_bin(target, graph, fixture_index(), fixture_problem(), "x86_64-unknown-linux-gnu", fixture_unit_targets())
}

"#;

fn run_binary_bytes(bytes: &[u8]) -> Result<Vec<u8>, String> {
    run_named_binary_bytes(bytes, "mini_app")
}

fn run_named_binary_bytes(bytes: &[u8], name: &str) -> Result<Vec<u8>, String> {
    let temp = tempfile::Builder::new()
        .prefix("vix-real-rustc-bin-")
        .tempdir()
        .map_err(|err| err.to_string())?;
    let bin = temp.path().join(name);
    fs::write(&bin, bytes).map_err(|err| err.to_string())?;
    make_executable(&bin)?;
    let output = Command::new(&bin).output().map_err(|err| err.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "built binary exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(output.stdout)
}

fn tree_file_bytes(machine: &mut Machine, handle: i64, path: &str) -> Result<Vec<u8>, String> {
    let blobs = machine.tree_blob_entries(handle)?;
    if let Some(bytes) = blobs.get(path) {
        return Ok(bytes.clone());
    }
    let entries = machine.tree_entries(handle)?;
    if let Some(contents) = entries.get(path) {
        return Ok(contents.as_bytes().to_vec());
    }
    Err(format!(
        "missing `{path}` in text entries {entries:?} and blob entries {blobs:?}"
    ))
}

fn tree_snapshot(machine: &mut Machine, handle: i64) -> Result<Tree, String> {
    Ok(Tree {
        entries: machine.tree_entries(handle)?,
        blobs: machine.tree_blob_entries(handle)?,
    })
}

fn demand_with_rustc_trace(
    machine: &mut Machine,
    name: &str,
    args: Vec<i64>,
) -> Result<i64, String> {
    machine
        .demand_i64(name, args)
        .map_err(|err| format!("{err}\nrustc argv trace:\n{}", rustc_argv_trace(machine)))
}

fn rustc_argv_trace(machine: &Machine) -> String {
    let mut out = String::new();
    for event in machine.trace() {
        match event {
            DriveEvent::RunRequested {
                command_name, argv, ..
            } if command_name == "rustc" => {
                out.push_str(&format!("requested {argv:?}\n"));
            }
            DriveEvent::RunCompleted {
                command_name,
                outputs,
                ..
            } if command_name == "rustc" => {
                out.push_str(&format!("completed {outputs:?}\n"));
            }
            _ => {}
        }
    }
    out
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)
        .map_err(|err| err.to_string())?
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).map_err(|err| err.to_string())
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct UnitShape {
    name: String,
    crate_type: String,
    edition: String,
    source_suffix: String,
    dependencies: Vec<String>,
    profile: ProfileShape,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ProfileShape {
    opt_level: String,
    debuginfo: String,
    debug_assertions: bool,
    overflow_checks: bool,
    panic: String,
    lto: String,
    codegen_units: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct UnitKey {
    name: String,
    crate_type: String,
    edition: String,
    source_suffix: String,
}

impl From<&UnitShape> for UnitKey {
    fn from(shape: &UnitShape) -> Self {
        Self {
            name: shape.name.clone(),
            crate_type: shape.crate_type.clone(),
            edition: shape.edition.clone(),
            source_suffix: shape.source_suffix.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct UnitEdge {
    from: UnitKey,
    extern_crate_name: String,
}

#[derive(Debug)]
struct UnitGraphDiffSummary {
    machine_units: usize,
    cargo_units: usize,
    unit_matches: usize,
    machine_only_units: usize,
    cargo_only_units: usize,
    machine_edges: usize,
    cargo_edges: usize,
    edge_matches: usize,
    machine_only_edges: usize,
    cargo_only_edges: usize,
}

impl UnitGraphDiffSummary {
    fn is_exact_match(&self) -> bool {
        self.machine_only_units == 0
            && self.cargo_only_units == 0
            && self.machine_only_edges == 0
            && self.cargo_only_edges == 0
    }
}

fn diff_unit_graphs(machine: &[UnitShape], cargo: &[UnitShape]) -> UnitGraphDiffSummary {
    let machine_units = unit_keys(machine);
    let cargo_units = unit_keys(cargo);
    let machine_edges = unit_edges(machine);
    let cargo_edges = unit_edges(cargo);

    UnitGraphDiffSummary {
        machine_units: machine_units.len(),
        cargo_units: cargo_units.len(),
        unit_matches: machine_units.intersection(&cargo_units).count(),
        machine_only_units: machine_units.difference(&cargo_units).count(),
        cargo_only_units: cargo_units.difference(&machine_units).count(),
        machine_edges: machine_edges.len(),
        cargo_edges: cargo_edges.len(),
        edge_matches: machine_edges.intersection(&cargo_edges).count(),
        machine_only_edges: machine_edges.difference(&cargo_edges).count(),
        cargo_only_edges: cargo_edges.difference(&machine_edges).count(),
    }
}

fn unit_keys(shapes: &[UnitShape]) -> BTreeSet<UnitKey> {
    shapes.iter().map(UnitKey::from).collect()
}

fn unit_edges(shapes: &[UnitShape]) -> BTreeSet<UnitEdge> {
    shapes
        .iter()
        .flat_map(|shape| {
            let from = UnitKey::from(shape);
            shape.dependencies.iter().map(move |dependency| UnitEdge {
                from: from.clone(),
                extern_crate_name: dependency.clone(),
            })
        })
        .collect()
}

fn unit_graph_diff_summary_table(diff: &UnitGraphDiffSummary) -> String {
    [
        "metric\tcount".to_owned(),
        format!("machine_units\t{}", diff.machine_units),
        format!("cargo_units\t{}", diff.cargo_units),
        format!("unit_matches\t{}", diff.unit_matches),
        format!("machine_only_units\t{}", diff.machine_only_units),
        format!("cargo_only_units\t{}", diff.cargo_only_units),
        format!("machine_edges\t{}", diff.machine_edges),
        format!("cargo_edges\t{}", diff.cargo_edges),
        format!("edge_matches\t{}", diff.edge_matches),
        format!("machine_only_edges\t{}", diff.machine_only_edges),
        format!("cargo_only_edges\t{}", diff.cargo_only_edges),
        String::new(),
    ]
    .join("\n")
}

fn write_tier_a_artifact(relative: &str, contents: &str) -> Result<(), String> {
    let Ok(root) = std::env::var("TIER_A_OUT") else {
        return Ok(());
    };
    let path = Path::new(&root).join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    fs::write(path, contents).map_err(|err| err.to_string())
}

fn machine_rustc_unit_graph(machine: &Machine) -> Result<Vec<UnitShape>, String> {
    let mut shapes = machine
        .trace()
        .iter()
        .filter_map(|event| match event {
            DriveEvent::RunRequested {
                command_name, argv, ..
            } if command_name == "rustc" => Some(argv.as_slice()),
            _ => None,
        })
        .map(machine_unit_shape)
        .collect::<Result<Vec<_>, _>>()?;
    shapes.sort();
    shapes.dedup();
    Ok(shapes)
}

fn machine_unit_shape(argv: &[String]) -> Result<UnitShape, String> {
    let arg_after = |flag: &str| {
        argv.windows(2)
            .find_map(|pair| (pair[0] == flag).then(|| pair[1].clone()))
            .ok_or_else(|| format!("missing {flag} in {argv:?}"))
    };
    let source = argv
        .iter()
        .find(|arg| arg.ends_with(".rs"))
        .ok_or_else(|| format!("missing source in {argv:?}"))?;
    let dependencies = argv
        .windows(2)
        .filter_map(|pair| {
            if pair[0] == "--extern" {
                pair[1].split_once('=').map(|(name, _)| name.to_string())
            } else {
                None
            }
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let name = arg_after("--crate-name")?;
    let crate_type = arg_after("--crate-type")?;
    let source_suffix = source_suffix(source)?;
    Ok(UnitShape {
        name: name.clone(),
        crate_type: crate_type.clone(),
        edition: arg_after("--edition")?,
        source_suffix: source_suffix.clone(),
        dependencies,
        profile: machine_profile_shape(argv, &crate_type, &source_suffix),
    })
}

fn machine_profile_shape(argv: &[String], crate_type: &str, source_suffix: &str) -> ProfileShape {
    let fallback = default_profile_shape(crate_type, source_suffix);
    ProfileShape {
        opt_level: codegen_option(argv, "opt-level=").unwrap_or(fallback.opt_level),
        debuginfo: codegen_option(argv, "debuginfo=").unwrap_or(fallback.debuginfo),
        debug_assertions: codegen_bool(argv, "debug-assertions=")
            .unwrap_or(fallback.debug_assertions),
        overflow_checks: codegen_bool(argv, "overflow-checks=").unwrap_or(fallback.overflow_checks),
        panic: codegen_option(argv, "panic=").unwrap_or(fallback.panic),
        lto: codegen_option(argv, "lto=").unwrap_or(fallback.lto),
        codegen_units: codegen_option(argv, "codegen-units=")
            .and_then(|value| value.parse::<i64>().ok())
            .or(fallback.codegen_units),
    }
}

fn codegen_option(argv: &[String], prefix: &str) -> Option<String> {
    argv.windows(2).find_map(|pair| {
        (pair[0] == "-C")
            .then(|| pair[1].strip_prefix(prefix).map(str::to_owned))
            .flatten()
    })
}

fn codegen_bool(argv: &[String], prefix: &str) -> Option<bool> {
    codegen_option(argv, prefix).and_then(|value| match value.as_str() {
        "true" | "yes" | "on" => Some(true),
        "false" | "no" | "off" => Some(false),
        _ => None,
    })
}

fn default_profile_shape(_crate_type: &str, source_suffix: &str) -> ProfileShape {
    let host_debuginfo = source_suffix == "build.rs";
    ProfileShape {
        opt_level: "0".to_owned(),
        debuginfo: if host_debuginfo { "0" } else { "2" }.to_owned(),
        debug_assertions: true,
        overflow_checks: true,
        panic: "unwind".to_owned(),
        lto: "false".to_owned(),
        codegen_units: None,
    }
}

fn source_suffix(source: &str) -> Result<String, String> {
    if source.ends_with("/src/lib.rs") || source.ends_with("/lib.rs") {
        Ok("src/lib.rs".to_string())
    } else if source.ends_with("/src/main.rs") || source.ends_with("/main.rs") {
        Ok("src/main.rs".to_string())
    } else if source.ends_with("/build.rs") {
        Ok("build.rs".to_string())
    } else {
        Err(format!("unexpected source path `{source}`"))
    }
}

fn cargo_unit_graph_oracle() -> Result<Vec<UnitShape>, String> {
    if !Command::new("cargo")
        .arg("+nightly")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
    {
        return Ok(Vec::new());
    }

    let temp = tempfile::Builder::new()
        .prefix("vix-cargo-unit-graph-oracle-")
        .tempdir()
        .map_err(|err| err.to_string())?;
    write_fixture(temp.path())?;
    let manifest = temp.path().join("app/Cargo.toml");
    let output = Command::new("cargo")
        .arg("+nightly")
        .arg("build")
        .arg("--unit-graph")
        .arg("-Z")
        .arg("unstable-options")
        .arg("--manifest-path")
        .arg(&manifest)
        .env("CARGO_NET_OFFLINE", "true")
        .output()
        .map_err(|err| err.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "cargo unit graph oracle exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let stdout = String::from_utf8(output.stdout).map_err(|err| err.to_string())?;
    let graph: CargoUnitGraph = facet_json::from_str(&stdout).map_err(|err| err.to_string())?;
    unit_shapes_from_graph(&graph, |_| true)
}

fn cargo_lock_graph_unit_graph_oracle() -> Result<Vec<UnitShape>, String> {
    if !Command::new("cargo")
        .arg("+nightly")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
    {
        return Ok(Vec::new());
    }

    let temp = tempfile::Builder::new()
        .prefix("vix-cargo-lock-graph-unit-graph-oracle-")
        .tempdir()
        .map_err(|err| err.to_string())?;
    write_lock_graph_fixture(temp.path())?;
    let manifest = temp.path().join("app/Cargo.toml");
    let output = Command::new("cargo")
        .arg("+nightly")
        .arg("build")
        .arg("--unit-graph")
        .arg("-Z")
        .arg("unstable-options")
        .arg("--locked")
        .arg("--manifest-path")
        .arg(&manifest)
        .env("CARGO_NET_OFFLINE", "true")
        .output()
        .map_err(|err| err.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "cargo lock graph unit graph oracle exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let stdout = String::from_utf8(output.stdout).map_err(|err| err.to_string())?;
    cargo_unit_shapes_from_json(&stdout)
}

fn cargo_build_script_unit_graph_oracle() -> Result<Vec<UnitShape>, String> {
    if !Command::new("cargo")
        .arg("+nightly")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
    {
        return Ok(Vec::new());
    }

    let temp = tempfile::Builder::new()
        .prefix("vix-cargo-build-script-unit-graph-oracle-")
        .tempdir()
        .map_err(|err| err.to_string())?;
    write_build_script_fixture(temp.path())?;
    let manifest = temp.path().join("app/Cargo.toml");
    let output = Command::new("cargo")
        .arg("+nightly")
        .arg("build")
        .arg("--unit-graph")
        .arg("-Z")
        .arg("unstable-options")
        .arg("--locked")
        .arg("--manifest-path")
        .arg(&manifest)
        .env("CARGO_NET_OFFLINE", "true")
        .output()
        .map_err(|err| err.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "cargo build-script unit graph oracle exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let stdout = String::from_utf8(output.stdout).map_err(|err| err.to_string())?;
    let graph: CargoUnitGraph = facet_json::from_str(&stdout).map_err(|err| err.to_string())?;
    if !graph.units.iter().any(|unit| {
        unit.mode == "build" && unit.target.kind.iter().any(|kind| kind == "custom-build")
    }) || !graph
        .units
        .iter()
        .any(|unit| unit.mode == "run-custom-build")
    {
        return Err(format!(
            "cargo oracle did not expose build and run-custom-build units: {graph:#?}"
        ));
    }
    unit_shapes_from_graph(&graph, |unit| unit.mode != "run-custom-build")
}

fn write_fixture(root: &Path) -> Result<(), String> {
    let files: [(PathBuf, &str); 4] = [
        (PathBuf::from("app/Cargo.toml"), APP_MANIFEST),
        (PathBuf::from("app/src/main.rs"), APP_MAIN),
        (PathBuf::from("crates/helper/Cargo.toml"), HELPER_MANIFEST),
        (PathBuf::from("crates/helper/src/lib.rs"), HELPER_LIB),
    ];
    for (relative, contents) in files {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        fs::write(path, contents).map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn write_lock_graph_fixture(root: &Path) -> Result<(), String> {
    let files: [(PathBuf, &str); 10] = [
        (PathBuf::from("Cargo.lock"), LOCK_GRAPH_LOCK),
        (PathBuf::from("app/Cargo.lock"), LOCK_GRAPH_APP_LOCK),
        (PathBuf::from("app/Cargo.toml"), LOCK_GRAPH_APP_MANIFEST),
        (PathBuf::from("app/src/main.rs"), LOCK_GRAPH_APP_MAIN),
        (
            PathBuf::from("crates/alpha_lib/Cargo.toml"),
            LOCK_GRAPH_ALPHA_MANIFEST,
        ),
        (
            PathBuf::from("crates/alpha_lib/src/lib.rs"),
            LOCK_GRAPH_ALPHA_LIB,
        ),
        (
            PathBuf::from("crates/core_lib/Cargo.toml"),
            LOCK_GRAPH_CORE_MANIFEST,
        ),
        (
            PathBuf::from("crates/core_lib/src/lib.rs"),
            LOCK_GRAPH_CORE_LIB,
        ),
        (
            PathBuf::from("crates/formatting_lib/Cargo.toml"),
            LOCK_GRAPH_FORMATTING_MANIFEST,
        ),
        (
            PathBuf::from("crates/formatting_lib/src/lib.rs"),
            LOCK_GRAPH_FORMATTING_LIB,
        ),
    ];
    for (relative, contents) in files {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        fs::write(path, contents).map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn write_build_script_fixture(root: &Path) -> Result<(), String> {
    let files: [(PathBuf, &str); 6] = [
        (PathBuf::from("Cargo.lock"), BUILD_SCRIPT_LOCK),
        (PathBuf::from("app/Cargo.lock"), BUILD_SCRIPT_APP_LOCK),
        (PathBuf::from("app/Cargo.toml"), BUILD_SCRIPT_MANIFEST),
        (PathBuf::from("app/build.rs"), BUILD_SCRIPT_RS),
        (PathBuf::from("app/src/lib.rs"), BUILD_SCRIPT_LIB),
        (PathBuf::from("app/src/main.rs"), BUILD_SCRIPT_MAIN),
    ];
    for (relative, contents) in files {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        fs::write(path, contents).map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn cargo_unit_shapes_from_json(stdout: &str) -> Result<Vec<UnitShape>, String> {
    let graph: CargoUnitGraph = facet_json::from_str(stdout).map_err(|err| err.to_string())?;
    unit_shapes_from_graph(&graph, |_| true)
}

fn unit_shapes_from_graph(
    graph: &CargoUnitGraph,
    include: impl Fn(&CargoUnit) -> bool,
) -> Result<Vec<UnitShape>, String> {
    let mut shapes = graph
        .units
        .iter()
        .filter(|unit| include(unit))
        .map(|unit| {
            let dependencies = unit
                .dependencies
                .iter()
                .filter_map(|dep| {
                    let dep_unit = graph.units.get(dep.index)?;
                    if dep_unit
                        .target
                        .kind
                        .iter()
                        .any(|kind| kind == "custom-build")
                    {
                        return None;
                    }
                    dep.extern_crate_name.clone()
                })
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            Ok(UnitShape {
                name: unit.target.name.replace('-', "_"),
                crate_type: unit
                    .target
                    .crate_types
                    .first()
                    .cloned()
                    .ok_or_else(|| format!("missing crate type for {:?}", unit.target))?,
                edition: unit.target.edition.clone(),
                source_suffix: source_suffix(&unit.target.src_path)?,
                dependencies,
                profile: unit.profile.shape(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    shapes.sort();
    shapes.dedup();
    Ok(shapes)
}

#[derive(Debug, Facet)]
struct CargoUnitGraph {
    units: Vec<CargoUnit>,
}

#[derive(Debug, Facet)]
struct CargoUnit {
    pkg_id: String,
    target: CargoTarget,
    mode: String,
    platform: Option<String>,
    profile: CargoProfile,
    dependencies: Vec<CargoDependency>,
}

#[derive(Debug, Facet)]
struct CargoProfile {
    name: String,
    opt_level: String,
    lto: String,
    codegen_units: Option<i64>,
    debuginfo: i64,
    debug_assertions: bool,
    overflow_checks: bool,
    panic: String,
}

impl CargoProfile {
    fn shape(&self) -> ProfileShape {
        ProfileShape {
            opt_level: self.opt_level.clone(),
            debuginfo: self.debuginfo.to_string(),
            debug_assertions: self.debug_assertions,
            overflow_checks: self.overflow_checks,
            panic: self.panic.clone(),
            lto: self.lto.clone(),
            codegen_units: self.codegen_units,
        }
    }
}

#[derive(Debug, Default)]
struct ProfileDivergenceCounts {
    package: usize,
    opt_level: usize,
    debuginfo: usize,
    debug_assertions: usize,
    overflow_checks: usize,
    panic: usize,
    lto: usize,
    codegen_units: usize,
}

impl ProfileDivergenceCounts {
    fn total(&self) -> usize {
        self.package
            + self.opt_level
            + self.debuginfo
            + self.debug_assertions
            + self.overflow_checks
            + self.panic
            + self.lto
            + self.codegen_units
    }

    fn record(
        &mut self,
        package: &str,
        unit_kind: &str,
        vix: &ProfileShape,
        cargo: &ProfileShape,
        examples: &mut Vec<String>,
    ) {
        macro_rules! check_field {
            ($field:ident) => {
                if vix.$field != cargo.$field {
                    self.$field += 1;
                    push_profile_example(
                        examples,
                        format!(
                            "{package}:{unit_kind}:{} vix={:?} cargo={:?}",
                            stringify!($field),
                            vix.$field,
                            cargo.$field
                        ),
                    );
                }
            };
        }

        check_field!(opt_level);
        check_field!(debuginfo);
        check_field!(debug_assertions);
        check_field!(overflow_checks);
        check_field!(panic);
        check_field!(lto);
        check_field!(codegen_units);
    }
}

fn profile_shape_from_rendered(
    fields: &BTreeMap<String, RenderedValue>,
) -> Result<ProfileShape, String> {
    Ok(ProfileShape {
        opt_level: rendered_field_string(fields, "opt_level")?,
        debuginfo: rendered_field_string(fields, "debuginfo")?,
        debug_assertions: rendered_field_bool_string(fields, "debug_assertions")?,
        overflow_checks: rendered_field_bool_string(fields, "overflow_checks")?,
        panic: rendered_field_string(fields, "panic")?,
        lto: rendered_field_string(fields, "lto")?,
        codegen_units: match rendered_field_string(fields, "codegen_units")?.as_str() {
            "" => None,
            value => Some(value.parse::<i64>().map_err(|err| err.to_string())?),
        },
    })
}

fn rendered_field_string(
    fields: &BTreeMap<String, RenderedValue>,
    name: &str,
) -> Result<String, String> {
    match fields.get(name) {
        Some(RenderedValue::String { value }) => Ok(value.clone()),
        other => Err(format!("field {name} was {other:?}, not String")),
    }
}

fn rendered_field_bool_string(
    fields: &BTreeMap<String, RenderedValue>,
    name: &str,
) -> Result<bool, String> {
    match rendered_field_string(fields, name)?.as_str() {
        "true" => Ok(true),
        "false" => Ok(false),
        other => Err(format!("field {name} was {other:?}, not bool string")),
    }
}

fn record(value: RenderedValue) -> Result<BTreeMap<String, RenderedValue>, String> {
    let RenderedValue::Record { fields, .. } = value else {
        return Err(format!("value rendered as {value:?}, not Record"));
    };
    Ok(fields
        .into_iter()
        .map(|field| (field.name, field.value))
        .collect())
}

fn intern_string(machine: &mut Machine, value: &str) -> Result<i64, String> {
    Ok(machine
        .intern_arg("String", MachineArg::String(value.to_owned()))?
        .0)
}

fn package_name_from_pkg_id(pkg_id: &str) -> Option<String> {
    let after_hash = pkg_id.rsplit_once('#')?.1;
    let (name, _) = after_hash.rsplit_once('@')?;
    Some(name.to_owned())
}

fn cargo_unit_kind(unit: &CargoUnit, host_dependency: bool) -> Result<String, String> {
    let kind = unit
        .target
        .kind
        .first()
        .ok_or_else(|| format!("target {:?} had no kind", unit.target.name))?;
    Ok(match kind.as_str() {
        "custom-build" => "build-script".to_owned(),
        "proc-macro" => "proc-macro".to_owned(),
        _ if host_dependency => "build-dependency".to_owned(),
        other => other.to_owned(),
    })
}

fn host_dependency_indices(graph: &CargoUnitGraph) -> BTreeSet<usize> {
    let mut roots = Vec::new();
    for (index, unit) in graph.units.iter().enumerate() {
        if unit.mode == "run-custom-build" {
            continue;
        }
        if unit.target.kind.iter().any(|kind| kind == "custom-build") {
            roots.push(index);
        }
    }

    let mut out = BTreeSet::new();
    let mut stack = roots;
    while let Some(index) = stack.pop() {
        let Some(unit) = graph.units.get(index) else {
            continue;
        };
        for dep in &unit.dependencies {
            if out.insert(dep.index) {
                stack.push(dep.index);
            }
        }
    }
    out
}

fn push_profile_example(examples: &mut Vec<String>, example: String) {
    if examples.len() < 12 {
        examples.push(example);
    }
}

fn profile_override_packages() -> BTreeSet<&'static str> {
    BTreeSet::from([
        "backtrace",
        "aho-corasick",
        "blake3",
        "block-buffer",
        "cpufeatures",
        "crypto-common",
        "digest",
        "generic-array",
        "hashbrown",
        "memchr",
        "miniz_oxide",
        "regex",
        "regex-automata",
        "regex-syntax",
        "sha2",
    ])
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("vix crate has workspace parent")
        .to_path_buf()
}

fn cargo_real_workspace_unit_graph_oracle() -> Result<CargoUnitGraph, String> {
    let output = Command::new("cargo")
        .arg("+nightly")
        .arg("build")
        .arg("--unit-graph")
        .arg("-Z")
        .arg("unstable-options")
        .arg("--locked")
        .arg("--manifest-path")
        .arg(workspace_root().join("Cargo.toml"))
        .output()
        .map_err(|err| err.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "cargo real-workspace unit graph oracle exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let stdout = String::from_utf8(output.stdout).map_err(|err| err.to_string())?;
    facet_json::from_str(&stdout).map_err(|err| err.to_string())
}

#[derive(Debug, Facet)]
struct CargoTarget {
    name: String,
    src_path: String,
    edition: String,
    crate_types: Vec<String>,
    kind: Vec<String>,
}

#[derive(Debug, Facet)]
struct CargoDependency {
    index: usize,
    extern_crate_name: Option<String>,
}

const PROC_MACRO_APP_MANIFEST: &str = include_str!(
    "../../playgrounds/snark/src/bundled/vix/samples/fixtures/proc_macro_graph/app/Cargo.toml"
);

const PROC_MACRO_APP_MAIN: &str = include_str!(
    "../../playgrounds/snark/src/bundled/vix/samples/fixtures/proc_macro_graph/app/src/main.rs"
);

const PROC_MACRO_MANIFEST: &str = include_str!(
    "../../playgrounds/snark/src/bundled/vix/samples/fixtures/proc_macro_graph/crates/emit_answer_macro/Cargo.toml"
);

const PROC_MACRO_LIB: &str = include_str!(
    "../../playgrounds/snark/src/bundled/vix/samples/fixtures/proc_macro_graph/crates/emit_answer_macro/src/lib.rs"
);

const PROC_MACRO_EXPECTED_STDOUT: &[u8] = b"proc macro says hello\n";

#[test]
fn real_process_rustc_builds_proc_macro_fixture_and_matches_cargo_unit_graph_oracle()
-> Result<(), String> {
    if !host_rustc_available() {
        return Ok(());
    }

    let source = crate_source();
    let backend = Arc::new(RealProcessBackend::new());
    let mut machine = Machine::load(&source)?.with_exec_backend(backend);
    let graph = machine
        .intern_arg("Tree", MachineArg::Tree(proc_macro_graph_tree()))?
        .0;

    let built = machine.demand_i64("crate_proc_macro_cross_bin", vec![graph])?;
    let bin = tree_file_bytes(&mut machine, built, "macro_app")?;
    let stdout = run_named_binary_bytes(&bin, "macro_app")?;
    if stdout != PROC_MACRO_EXPECTED_STDOUT {
        return Err(format!(
            "unexpected proc-macro fixture stdout: {:?}",
            String::from_utf8_lossy(&stdout)
        ));
    }

    let (machine_graph, host_target_capabilities) = machine_proc_macro_unit_graph(&machine)?;
    let cargo_graph = cargo_proc_macro_unit_graph_oracle()?;
    if machine_graph != cargo_graph {
        return Err(format!(
            "proc-macro machine unit graph did not match cargo oracle\nmachine: {machine_graph:#?}\ncargo: {cargo_graph:#?}"
        ));
    }
    let (host_capability, target_capability) = host_target_capabilities
        .first()
        .ok_or_else(|| "missing proc-macro host/target capability pair".to_string())?;
    if host_capability == target_capability {
        return Err(format!(
            "proc-macro producer and consumer used the same rustc capability: {host_capability}"
        ));
    }

    Ok(())
}

#[test]
fn real_workspace_resolved_profiles_match_cargo_unit_graph_oracle() -> Result<(), String> {
    if !Command::new("cargo")
        .arg("+nightly")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
    {
        return Ok(());
    }

    let graph = cargo_real_workspace_unit_graph_oracle()?;
    let workspace_manifest =
        fs::read_to_string(workspace_root().join("Cargo.toml")).map_err(|err| err.to_string())?;
    let mut machine = Machine::load(&crate_source())?;
    let workspace = machine
        .intern_arg(
            "Tree",
            MachineArg::Tree(Tree::of(&[("Cargo.toml", workspace_manifest.as_str())])),
        )?
        .0;

    let mut checked = 0usize;
    let mut divergences = ProfileDivergenceCounts::default();
    let mut examples = Vec::new();
    let override_packages = profile_override_packages();
    let host_dependencies = host_dependency_indices(&graph);

    for (index, unit) in graph.units.iter().enumerate().filter(|(_, unit)| {
        unit.mode != "run-custom-build"
            && package_name_from_pkg_id(&unit.pkg_id)
                .is_some_and(|package| override_packages.contains(package.as_str()))
    }) {
        let Some(package) = package_name_from_pkg_id(&unit.pkg_id) else {
            divergences.package += 1;
            push_profile_example(
                &mut examples,
                format!("could not parse package name from {}", unit.pkg_id),
            );
            continue;
        };
        let unit_kind = cargo_unit_kind(
            unit,
            host_dependencies.contains(&index) && unit.profile.debuginfo == 0,
        )?;
        let package_arg = intern_string(&mut machine, &package)?;
        let unit_kind_arg = intern_string(&mut machine, &unit_kind)?;
        let profile_name = intern_string(&mut machine, &unit.profile.name)?;
        let resolved = machine.demand_i64(
            "resolved_profile_for",
            vec![workspace, package_arg, unit_kind_arg, profile_name],
        )?;
        let rendered = record(machine.render_result("resolved_profile_for", resolved)?)?;
        let vix = profile_shape_from_rendered(&rendered)?;
        let cargo = unit.profile.shape();
        checked += 1;
        divergences.record(&package, &unit_kind, &vix, &cargo, &mut examples);
    }

    assert!(checked > 0);
    assert_eq!(
        divergences.total(),
        0,
        "profile divergences over {checked} real-root override units: {divergences:#?}\nexamples: {examples:#?}"
    );

    Ok(())
}

fn proc_macro_graph_tree() -> Tree {
    Tree::of(&[
        ("app/Cargo.toml", PROC_MACRO_APP_MANIFEST),
        ("app/src/main.rs", PROC_MACRO_APP_MAIN),
        ("crates/emit_answer_macro/Cargo.toml", PROC_MACRO_MANIFEST),
        ("crates/emit_answer_macro/src/lib.rs", PROC_MACRO_LIB),
    ])
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ProcMacroUnitShape {
    name: String,
    crate_type: String,
    edition: String,
    source_suffix: String,
    dependencies: Vec<String>,
    platform: String,
    profile: ProfileShape,
}

type ProcMacroUnitGraph = (Vec<ProcMacroUnitShape>, Vec<(String, String)>);

fn machine_proc_macro_unit_graph(machine: &Machine) -> Result<ProcMacroUnitGraph, String> {
    let mut shapes = machine
        .trace()
        .iter()
        .filter_map(|event| match event {
            DriveEvent::RunRequested {
                command_name,
                capability_key,
                argv,
                ..
            } if command_name == "rustc" => Some((capability_key.as_str(), argv.as_slice())),
            _ => None,
        })
        .map(machine_proc_macro_unit_shape)
        .collect::<Result<Vec<_>, _>>()?;
    shapes.sort();
    shapes.dedup();

    let capabilities = machine
        .trace()
        .iter()
        .filter_map(|event| match event {
            DriveEvent::RunRequested {
                command_name,
                capability_key,
                argv,
                ..
            } if command_name == "rustc" => Some((
                arg_after(argv, "--crate-name").ok()?,
                capability_key.clone(),
            )),
            _ => None,
        })
        .collect::<Vec<_>>();
    let host = capabilities
        .iter()
        .find_map(|(name, capability)| (name == "emit_answer_macro").then(|| capability.clone()))
        .ok_or_else(|| "missing emit_answer_macro capability".to_string())?;
    let target = capabilities
        .iter()
        .find_map(|(name, capability)| (name == "macro_app").then(|| capability.clone()))
        .ok_or_else(|| "missing macro_app capability".to_string())?;
    Ok((shapes, vec![(host, target)]))
}

fn machine_proc_macro_unit_shape(
    (capability_key, argv): (&str, &[String]),
) -> Result<ProcMacroUnitShape, String> {
    if !capability_key.starts_with("acquire:Rustc:") {
        return Err(format!(
            "rustc run had non-rustc capability `{capability_key}`"
        ));
    }
    let unit = machine_unit_shape(argv)?;
    Ok(ProcMacroUnitShape {
        platform: if unit.crate_type == "proc-macro" {
            "host".to_string()
        } else {
            "target".to_string()
        },
        name: unit.name,
        crate_type: unit.crate_type,
        edition: unit.edition,
        source_suffix: unit.source_suffix,
        dependencies: unit.dependencies,
        profile: unit.profile,
    })
}

fn arg_after(argv: &[String], flag: &str) -> Result<String, String> {
    argv.windows(2)
        .find_map(|pair| (pair[0] == flag).then(|| pair[1].clone()))
        .ok_or_else(|| format!("missing {flag} in {argv:?}"))
}

fn cargo_proc_macro_unit_graph_oracle() -> Result<Vec<ProcMacroUnitShape>, String> {
    if !Command::new("cargo")
        .arg("+nightly")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
    {
        return Ok(Vec::new());
    }

    let temp = tempfile::Builder::new()
        .prefix("vix-cargo-proc-macro-unit-graph-oracle-")
        .tempdir()
        .map_err(|err| err.to_string())?;
    write_proc_macro_fixture(temp.path())?;
    let manifest = temp.path().join("app/Cargo.toml");
    let output = Command::new("cargo")
        .arg("+nightly")
        .arg("build")
        .arg("--unit-graph")
        .arg("-Z")
        .arg("unstable-options")
        .arg("--target")
        .arg("aarch64-unknown-linux-gnu")
        .arg("--manifest-path")
        .arg(&manifest)
        .env("CARGO_NET_OFFLINE", "true")
        .output()
        .map_err(|err| err.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "cargo proc-macro unit graph oracle exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let stdout = String::from_utf8(output.stdout).map_err(|err| err.to_string())?;
    cargo_proc_macro_unit_shapes_from_json(&stdout)
}

fn write_proc_macro_fixture(root: &Path) -> Result<(), String> {
    let files: [(PathBuf, &str); 4] = [
        (PathBuf::from("app/Cargo.toml"), PROC_MACRO_APP_MANIFEST),
        (PathBuf::from("app/src/main.rs"), PROC_MACRO_APP_MAIN),
        (
            PathBuf::from("crates/emit_answer_macro/Cargo.toml"),
            PROC_MACRO_MANIFEST,
        ),
        (
            PathBuf::from("crates/emit_answer_macro/src/lib.rs"),
            PROC_MACRO_LIB,
        ),
    ];
    for (relative, contents) in files {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        fs::write(path, contents).map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn cargo_proc_macro_unit_shapes_from_json(stdout: &str) -> Result<Vec<ProcMacroUnitShape>, String> {
    let graph: CargoUnitGraph = facet_json::from_str(stdout).map_err(|err| err.to_string())?;
    let mut shapes = graph
        .units
        .iter()
        .map(|unit| {
            let dependencies = unit
                .dependencies
                .iter()
                .filter_map(|dep| dep.extern_crate_name.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            Ok(ProcMacroUnitShape {
                name: unit.target.name.clone(),
                crate_type: unit
                    .target
                    .crate_types
                    .first()
                    .cloned()
                    .ok_or_else(|| format!("missing crate type for {:?}", unit.target))?,
                edition: unit.target.edition.clone(),
                source_suffix: source_suffix(&unit.target.src_path)?,
                dependencies,
                profile: unit.profile.shape(),
                platform: if unit.platform.is_some() {
                    "target".to_string()
                } else {
                    "host".to_string()
                },
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    shapes.sort();
    Ok(shapes)
}
