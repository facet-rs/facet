#![cfg(all(feature = "real-process", not(target_arch = "wasm32")))]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Instant;

use facet::Facet;
use vix::exec::Tree;
use vix::fetch::FakeFetchBackend;
use vix::machine::{DriveEvent, Machine, MachineArg, RenderedValue};
use vix::real_process::RealProcessBackend;

const SOURCE: &str = include_str!("../../playgrounds/snark/src/bundled/vix/samples/crate.vix");
const RODIN_SOURCE: &str = include_str!("../../rodin/rodin.vix");
const CARGO_NEXT_SOURCE: &str = include_str!("../corpus-next/cargo.vix");
const CRATE_NEXT_SOURCE: &str =
    include_str!("../../playgrounds/snark/src/bundled/vix/samples/crate.vix");
const CARGO_MANIFEST_NEXT_SOURCE: &str =
    include_str!("../../playgrounds/snark/src/bundled/vix/samples/cargo_manifest.vix");
const BLAKE3_SPARSE_INDEX: &str =
    include_str!("fixtures/sparse-index/snapshot-2025-03-04/bl/ak/blake3");
const CC_SPARSE_INDEX: &str = include_str!("fixtures/sparse-index/snapshot-2025-03-04/2/cc");
const ARRAYREF_SPARSE_INDEX: &str =
    include_str!("fixtures/sparse-index/snapshot-2025-03-04/ar/ra/arrayref");
const ARRAYVEC_SPARSE_INDEX: &str =
    include_str!("fixtures/sparse-index/snapshot-2025-03-04/ar/ra/arrayvec");
const CFG_IF_SPARSE_INDEX: &str =
    include_str!("fixtures/sparse-index/snapshot-2025-03-04/cf/g-/cfg-if");
const CONSTANT_TIME_EQ_SPARSE_INDEX: &str =
    include_str!("fixtures/sparse-index/snapshot-2025-03-04/co/ns/constant_time_eq");
const SHLEX_SPARSE_INDEX: &str =
    include_str!("fixtures/sparse-index/snapshot-2025-03-04/sh/le/shlex");
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

fn proc_macro_solution_source() -> String {
    format!("{RODIN_SOURCE}\n\n{SOURCE}\n\n{PROC_MACRO_SOLUTION_BRIDGE}")
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
    let targets = targets.insert(0, UnitTarget { kind: "lib", manifest: p"crates/alpha_lib", source: p"src/lib.rs", cfgs: [], metadata: p"libalpha_lib.rmeta", link: p"libalpha_lib.rlib", metadata_file: "libalpha_lib.rmeta", link_file: "libalpha_lib.rlib" });
    let targets = targets.insert(1, UnitTarget { kind: "lib", manifest: p"crates/core_lib", source: p"src/lib.rs", cfgs: [], metadata: p"libcore_lib.rmeta", link: p"libcore_lib.rlib", metadata_file: "libcore_lib.rmeta", link_file: "libcore_lib.rlib" });
    let targets = targets.insert(2, UnitTarget { kind: "lib", manifest: p"crates/formatting_lib", source: p"src/lib.rs", cfgs: [], metadata: p"libformatting_lib.rmeta", link: p"libformatting_lib.rlib", metadata_file: "libformatting_lib.rmeta", link_file: "libformatting_lib.rlib" });
    let targets = targets.insert(3, UnitTarget { kind: "bin", manifest: p"app", source: p"src/main.rs", cfgs: [], metadata: p"mini_app.rmeta", link: p"mini_app", metadata_file: "mini_app.rmeta", link_file: "mini_app" });
    UnitTargetTable { root: 3, targets: targets }
}

fn fixture_resolved_graph(target: String) -> ResolvedGraph {
    let result = solve(fixture_index(), fixture_problem(), target);
    let units: Map<Int, ResolvedUnit> = {};
    let units = selected_insert_unit(units, result.selected, 0, ResolvedUnit { name: "alpha_lib", kind: "lib", manifest: p"crates/alpha_lib", source: p"src/lib.rs", deps: [1], cfgs: [], metadata: p"libalpha_lib.rmeta", link: p"libalpha_lib.rlib", metadata_file: "libalpha_lib.rmeta", link_file: "libalpha_lib.rlib", profile: default_resolved_profile("lib", "alpha_lib") });
    let units = selected_insert_unit(units, result.selected, 1, ResolvedUnit { name: "core_lib", kind: "lib", manifest: p"crates/core_lib", source: p"src/lib.rs", deps: [], cfgs: [], metadata: p"libcore_lib.rmeta", link: p"libcore_lib.rlib", metadata_file: "libcore_lib.rmeta", link_file: "libcore_lib.rlib", profile: default_resolved_profile("lib", "core_lib") });
    let units = selected_insert_unit(units, result.selected, 2, ResolvedUnit { name: "formatting_lib", kind: "lib", manifest: p"crates/formatting_lib", source: p"src/lib.rs", deps: [], cfgs: [], metadata: p"libformatting_lib.rmeta", link: p"libformatting_lib.rlib", metadata_file: "libformatting_lib.rmeta", link_file: "libformatting_lib.rlib", profile: default_resolved_profile("lib", "formatting_lib") });
    let units = selected_insert_unit(units, result.selected, 3, ResolvedUnit { name: "mini_app", kind: "bin", manifest: p"app", source: p"src/main.rs", deps: [0, 2], cfgs: [], metadata: p"mini_app.rmeta", link: p"mini_app", metadata_file: "mini_app.rmeta", link_file: "mini_app", profile: default_resolved_profile("bin", "mini_app") });
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

const PROC_MACRO_SOLUTION_BRIDGE: &str = r#"
fn proc_macro_solution_index() -> Index {
    let names: Map<Int, String> = {};
    let names = names.insert(0, "emit_answer_macro");
    let names = names.insert(1, "macro_app");

    let version_pkgs: Map<Int, Int> = {};
    let version_pkgs = version_pkgs.insert(0, 0);
    let version_pkgs = version_pkgs.insert(1, 1);

    let version_values: Map<Int, String> = {};
    let version_values = version_values.insert(0, "0.1.0");
    let version_values = version_values.insert(1, "0.1.0");

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
    let guard_pkgs = guard_pkgs.insert(0, 1);
    let guard_features = guard_features.insert(0, 0);
    let consequent_tags = consequent_tags.insert(0, "in_graph");
    let consequent_pkgs = consequent_pkgs.insert(0, 0);
    let consequent_version_sets = consequent_version_sets.insert(0, VersionSet::from_req("*"));
    let consequent_features = consequent_features.insert(0, 0);
    let gate_kinds = gate_kinds.insert(0, "normal");

    let guard_clause_ids = guard_clause_ids.insert(1, 1);
    let guard_tags = guard_tags.insert(1, "in_graph");
    let guard_kinds = guard_kinds.insert(1, 0);
    let guard_pkgs = guard_pkgs.insert(1, 1);
    let guard_features = guard_features.insert(1, 0);
    let consequent_tags = consequent_tags.insert(1, "version_set");
    let consequent_pkgs = consequent_pkgs.insert(1, 0);
    let consequent_version_sets = consequent_version_sets.insert(1, VersionSet::from_req("0.1.0"));
    let consequent_features = consequent_features.insert(1, 0);
    let gate_kinds = gate_kinds.insert(1, "normal");

    Index {
        packages: [0, 1],
        names: names,
        version_ids: [0, 1],
        version_pkgs: version_pkgs,
        version_values: version_values,
        clause_ids: [0, 1],
        guard_ids: [0, 1],
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

fn proc_macro_solution_problem() -> Problem {
    Problem {
        root_pkg: 1,
        root_req: VersionSet::from_req("*"),
        root_features: [],
        root_default_feature: 0,
        root_default_features: false,
    }
}

fn proc_macro_solution_targets() -> UnitTargetTable {
    let host = Target::host();
    let dylib = proc_macro_dylib_path(host);
    let dylib_file = proc_macro_dylib_file(host);
    let targets: Map<Int, UnitTarget> = {};
    let targets = targets.insert(0, UnitTarget { kind: "proc-macro", manifest: p"crates/emit_answer_macro", source: p"src/lib.rs", cfgs: [], metadata: dylib, link: dylib, metadata_file: dylib_file, link_file: dylib_file });
    let targets = targets.insert(1, UnitTarget { kind: "bin", manifest: p"app", source: p"src/main.rs", cfgs: [], metadata: p"macro_app.rmeta", link: p"macro_app", metadata_file: "macro_app.rmeta", link_file: "macro_app" });
    UnitTargetTable { root: 1, targets: targets }
}

pub fn derived_proc_macro_cross_bin(graph: Tree) -> Tree {
    crate_solution_bin(proc_macro_cross_target(), graph, proc_macro_solution_index(), proc_macro_solution_problem(), "x86_64-unknown-linux-gnu", proc_macro_solution_targets())
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

struct ArtifactExpectation<'a> {
    path: &'a str,
    archive: bool,
}

fn artifact_receipt_table(
    machine: &mut Machine,
    handle: i64,
    artifacts: &[ArtifactExpectation<'_>],
) -> Result<String, String> {
    let mut rows = Vec::with_capacity(artifacts.len() + 1);
    rows.push("path\tbytes\tblake3\tarchive".to_string());
    for artifact in artifacts {
        let bytes = tree_file_bytes(machine, handle, artifact.path)?;
        if bytes.is_empty() {
            return Err(format!("artifact `{}` was empty", artifact.path));
        }
        let archive = bytes.starts_with(b"!<arch>\n");
        if artifact.archive && !archive {
            return Err(format!(
                "artifact `{}` was not an ar archive; first bytes: {:02x?}",
                artifact.path,
                &bytes[..bytes.len().min(16)]
            ));
        }
        rows.push(format!(
            "{}\t{}\t{}\t{}",
            artifact.path,
            bytes.len(),
            blake3::hash(&bytes).to_hex(),
            archive
        ));
    }
    rows.push(String::new());
    Ok(rows.join("\n"))
}

fn assert_rustc_requests_include_crates(
    machine: &Machine,
    expected: &[&str],
) -> Result<(), String> {
    let mut missing = Vec::new();
    for crate_name in expected {
        let seen = machine.trace().iter().any(|event| match event {
            DriveEvent::RunRequested {
                command_name, argv, ..
            } if command_name == "rustc" => argv
                .windows(2)
                .any(|pair| pair[0] == "--crate-name" && pair[1] == *crate_name),
            _ => false,
        });
        if !seen {
            missing.push(*crate_name);
        }
    }
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "missing rustc requests for crates {missing:?}; trace:\n{}",
            rustc_argv_trace(machine)
        ))
    }
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
    roots: Vec<usize>,
}

#[derive(Debug, Facet)]
struct CargoUnit {
    pkg_id: String,
    target: CargoTarget,
    mode: String,
    platform: Option<String>,
    profile: CargoProfile,
    features: Vec<String>,
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

fn rendered_result_string(machine: &Machine, name: &str, handle: i64) -> Result<String, String> {
    match machine.render_result(name, handle)? {
        RenderedValue::String { value } => Ok(value),
        other => Err(format!("result {name} rendered as {other:?}, not String")),
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

#[derive(Facet)]
struct RawSparseIndexRow {
    name: String,
    vers: String,
    deps: Vec<RawSparseIndexDep>,
    cksum: String,
    features: BTreeMap<String, Vec<String>>,
    features2: Option<BTreeMap<String, Vec<String>>>,
    yanked: bool,
    rust_version: Option<String>,
    pubtime: String,
    v: Option<i64>,
}

#[derive(Facet)]
struct RawSparseIndexDep {
    name: String,
    package: Option<String>,
    req: String,
    kind: String,
    target: Option<String>,
    optional: bool,
    default_features: bool,
    features: Vec<String>,
}

#[derive(Facet)]
struct NormalizedSparseIndexRow {
    name: String,
    vers: String,
    deps: Vec<NormalizedSparseIndexDep>,
    cksum: String,
    features: BTreeMap<String, Vec<String>>,
    yanked: bool,
    pubtime: String,
}

#[derive(Facet)]
struct NormalizedSparseIndexDep {
    name: String,
    package: String,
    req: String,
    kind: String,
    target: String,
    optional: bool,
    default_features: bool,
    features: Vec<String>,
}

fn taxon_native_sparse_jsonl() -> Result<String, String> {
    let rows = [
        selected_sparse_row(BLAKE3_SPARSE_INDEX, "blake3", "1.6.1")?,
        selected_sparse_row(CC_SPARSE_INDEX, "cc", "1.2.16")?,
        selected_sparse_row(ARRAYREF_SPARSE_INDEX, "arrayref", "0.3.9")?,
        selected_sparse_row(ARRAYVEC_SPARSE_INDEX, "arrayvec", "0.7.6")?,
        selected_sparse_row(CFG_IF_SPARSE_INDEX, "cfg-if", "1.0.0")?,
        selected_sparse_row(
            CONSTANT_TIME_EQ_SPARSE_INDEX,
            "constant_time_eq",
            "0.3.1",
        )?,
        selected_sparse_row(SHLEX_SPARSE_INDEX, "shlex", "1.3.0")?,
    ];
    Ok(rows.join("\n"))
}

fn selected_sparse_row(index: &str, name: &str, version: &str) -> Result<String, String> {
    for line in index.lines() {
        let raw: RawSparseIndexRow = facet_json::from_str(line).map_err(|err| err.to_string())?;
        let RawSparseIndexRow {
            name: row_name,
            vers,
            deps,
            cksum,
            features,
            features2: _,
            yanked,
            rust_version: _,
            pubtime,
            v: _,
        } = raw;
        if row_name != name || vers != version {
            continue;
        }
        let normalized = NormalizedSparseIndexRow {
            name: row_name,
            vers,
            deps: deps
                .into_iter()
                .map(|dep| {
                    let RawSparseIndexDep {
                        name,
                        package,
                        req,
                        kind,
                        target,
                        optional,
                        default_features,
                        features,
                    } = dep;
                    NormalizedSparseIndexDep {
                        package: package.unwrap_or_else(|| name.clone()),
                        target: target.unwrap_or_default(),
                        name,
                        req,
                        kind,
                        optional,
                        default_features,
                        features,
                    }
                })
                .collect(),
            cksum,
            features,
            yanked,
            pubtime,
        };
        return facet_json::to_string(&normalized).map_err(|err| err.to_string());
    }
    Err(format!("missing sparse row {name} {version} in pinned snapshot"))
}

fn taxon_native_workspace_tree() -> Result<Tree, String> {
    let root = workspace_root();
    let mut entries = BTreeMap::new();
    let mut blobs = BTreeMap::new();
    entries.insert(
        "Cargo.toml".to_owned(),
        fs::read_to_string(root.join("Cargo.toml")).map_err(|err| err.to_string())?,
    );
    copy_tree_into_vix_tree(&root.join("phon/rust/taxon"), "phon/rust/taxon", &mut entries, &mut blobs)?;
    Ok(Tree { entries, blobs })
}

fn taxon_native_archive_path(name: &str, version: &str) -> PathBuf {
    let crate_file = format!("{name}-{version}.crate");
    PathBuf::from(std::env::var_os("HOME").expect("HOME is set"))
        .join(".cargo/registry/cache/index.crates.io-1949cf8c6b5b557f")
        .join(crate_file)
}

fn taxon_native_archive_bytes(name: &str, version: &str) -> Result<Vec<u8>, String> {
    let path = taxon_native_archive_path(name, version);
    fs::read(&path).map_err(|err| {
        format!(
            "read pinned archive {}: {err}; run `cargo info {name}@{version}` to populate the cache",
            path.display()
        )
    })
}

fn taxon_native_crate_url(name: &str, version: &str) -> String {
    format!("https://static.crates.io/crates/{name}/{name}-{version}.crate")
}

fn taxon_native_fetch_backend() -> Result<FakeFetchBackend, String> {
    let mut backend = FakeFetchBackend::new();
    for (name, version) in [
        ("arrayref", "0.3.9"),
        ("arrayvec", "0.7.6"),
        ("cfg-if", "1.0.0"),
        ("constant_time_eq", "0.3.1"),
        ("shlex", "1.3.0"),
        ("cc", "1.2.16"),
        ("blake3", "1.6.1"),
    ] {
        let bytes = taxon_native_archive_bytes(name, version)?;
        let file = format!("{name}-{version}.crate");
        backend.insert_archive(
            taxon_native_crate_url(name, version),
            &bytes,
            Tree::of_blobs(&[(file.as_str(), &bytes)]),
        );
    }
    Ok(backend)
}

fn taxon_native_fetch_backend_with_changed_blake3() -> Result<FakeFetchBackend, String> {
    let mut backend = FakeFetchBackend::new();
    for (name, version) in [
        ("arrayref", "0.3.9"),
        ("arrayvec", "0.7.6"),
        ("cfg-if", "1.0.0"),
        ("constant_time_eq", "0.3.1"),
        ("shlex", "1.3.0"),
        ("cc", "1.2.16"),
        ("blake3", "1.6.1"),
    ] {
        let mut bytes = taxon_native_archive_bytes(name, version)?;
        if name == "blake3" {
            let first = bytes
                .first_mut()
                .ok_or_else(|| "empty blake3 archive fixture".to_string())?;
            *first ^= 0x01;
        }
        let file = format!("{name}-{version}.crate");
        backend.insert_archive(
            taxon_native_crate_url(name, version),
            &bytes,
            Tree::of_blobs(&[(file.as_str(), &bytes)]),
        );
    }
    Ok(backend)
}

fn taxon_native_machine(fetch_backend: FakeFetchBackend) -> Result<Machine, String> {
    let sources = BTreeMap::from([
        ("cargo_demo".to_owned(), CARGO_NEXT_SOURCE.to_owned()),
        ("crate_build".to_owned(), CRATE_NEXT_SOURCE.to_owned()),
        (
            "cargo_manifest".to_owned(),
            CARGO_MANIFEST_NEXT_SOURCE.to_owned(),
        ),
        ("rodin".to_owned(), RODIN_SOURCE.to_owned()),
    ]);
    let backend = Arc::new(RealProcessBackend::new());
    let mut machine = Machine::load_modules("cargo_demo", sources)
        .map_err(|err| err.to_string())?
        .with_fetch_backend(fetch_backend)
        .with_exec_backend(backend);
    machine.set_force_molten_copy(true);
    Ok(machine)
}

fn taxon_native_args(machine: &mut Machine) -> Result<Vec<i64>, String> {
    let workspace = machine
        .intern_arg("Tree", MachineArg::Tree(taxon_native_workspace_tree()?))?
        .0;
    let sparse = machine
        .intern_arg("String", MachineArg::String(taxon_native_sparse_jsonl()?))?
        .0;
    let target_name = machine
        .intern_arg("String", MachineArg::String(host_triple()?))?
        .0;
    Ok(vec![workspace, sparse, target_name])
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

fn cargo_taxon_unit_graph_oracle() -> Result<CargoUnitGraph, String> {
    if !Command::new("cargo")
        .arg("+nightly")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
    {
        return Ok(CargoUnitGraph {
            units: Vec::new(),
            roots: Vec::new(),
        });
    }

    let output = Command::new("cargo")
        .arg("+nightly")
        .arg("build")
        .arg("--unit-graph")
        .arg("-Z")
        .arg("unstable-options")
        .arg("--locked")
        .arg("-p")
        .arg("taxon")
        .output()
        .map_err(|err| err.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "cargo taxon unit graph oracle exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let stdout = String::from_utf8(output.stdout).map_err(|err| err.to_string())?;
    facet_json::from_str(&stdout).map_err(|err| err.to_string())
}

fn cargo_facet_core_no_default_unit_graph_oracle() -> Result<CargoUnitGraph, String> {
    if !Command::new("cargo")
        .arg("+nightly")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
    {
        return Ok(CargoUnitGraph {
            units: Vec::new(),
            roots: Vec::new(),
        });
    }

    let output = Command::new("cargo")
        .arg("+nightly")
        .arg("build")
        .arg("--unit-graph")
        .arg("-Z")
        .arg("unstable-options")
        .arg("--locked")
        .arg("-p")
        .arg("facet-core")
        .arg("--no-default-features")
        .output()
        .map_err(|err| err.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "cargo facet-core unit graph oracle exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let stdout = String::from_utf8(output.stdout).map_err(|err| err.to_string())?;
    facet_json::from_str(&stdout).map_err(|err| err.to_string())
}

fn cargo_facet_default_unit_graph_oracle() -> Result<CargoUnitGraph, String> {
    if !Command::new("cargo")
        .arg("+nightly")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
    {
        return Ok(CargoUnitGraph {
            units: Vec::new(),
            roots: Vec::new(),
        });
    }

    let output = Command::new("cargo")
        .arg("+nightly")
        .arg("build")
        .arg("--unit-graph")
        .arg("-Z")
        .arg("unstable-options")
        .arg("--locked")
        .arg("-p")
        .arg("facet")
        .output()
        .map_err(|err| err.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "cargo facet unit graph oracle exited with {}: {}",
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
fn real_process_rustc_builds_derived_proc_macro_fixture_and_matches_cargo_unit_graph_oracle()
-> Result<(), String> {
    if !host_rustc_available() {
        return Ok(());
    }

    let source = proc_macro_solution_source();
    let backend = Arc::new(RealProcessBackend::new());
    let mut machine = Machine::load(&source)?.with_exec_backend(backend);
    let graph = machine
        .intern_arg("Tree", MachineArg::Tree(proc_macro_graph_tree()))?
        .0;

    let built = demand_with_rustc_trace(&mut machine, "derived_proc_macro_cross_bin", vec![graph])?;
    let bin = tree_file_bytes(&mut machine, built, "macro_app")?;
    let stdout = run_named_binary_bytes(&bin, "macro_app")?;
    if stdout != PROC_MACRO_EXPECTED_STDOUT {
        return Err(format!(
            "unexpected derived proc-macro fixture stdout: {:?}",
            String::from_utf8_lossy(&stdout)
        ));
    }

    let (machine_graph, host_target_capabilities) = machine_proc_macro_unit_graph(&machine)?;
    let cargo_graph = cargo_proc_macro_unit_graph_oracle()?;
    if machine_graph != cargo_graph {
        return Err(format!(
            "derived proc-macro machine unit graph did not match cargo oracle\nmachine: {machine_graph:#?}\ncargo: {cargo_graph:#?}"
        ));
    }
    let (host_capability, target_capability) = host_target_capabilities
        .first()
        .ok_or_else(|| "missing proc-macro host/target capability pair".to_string())?;
    if host_capability == target_capability {
        return Err(format!(
            "derived proc-macro producer and consumer used the same rustc capability: {host_capability}"
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

#[test]
#[ignore = "demo boundary: blake3 1.8.5 default-features=false still has a custom-build unit via cc"]
fn taxon_ladder_oracle_reveals_blake3_build_script_boundary() -> Result<(), String> {
    let graph = cargo_taxon_unit_graph_oracle()?;
    if graph.units.is_empty() {
        return Ok(());
    }

    let has_blake3_build_script = graph.units.iter().any(|unit| {
        unit.pkg_id.contains("#blake3@1.8.5")
            && unit.target.kind.iter().any(|kind| kind == "custom-build")
    });
    let has_cc_build_dependency = graph
        .units
        .iter()
        .any(|unit| unit.pkg_id.contains("#cc@") && unit.profile.debuginfo == 0);

    assert!(
        has_blake3_build_script,
        "cargo unit graph for taxon did not expose blake3's custom-build unit: {graph:#?}"
    );
    assert!(
        has_cc_build_dependency,
        "cargo unit graph for taxon did not expose blake3's cc build dependency: {graph:#?}"
    );
    let host = host_triple()?;
    let bridge = taxon_demo_bridge_source(&graph, &host)?;
    write_tier_a_artifact("taxon-demo-bridge-oracle.vix", &bridge)?;
    let source_tree = taxon_demo_source_tree(&graph)?;
    assert_taxon_source_tree_has_blake3_build_main(&source_tree)?;
    Ok(())
}

#[test]
fn taxon_ladder_runs_blake3_build_script_with_declared_cc() -> Result<(), String> {
    if !host_rustc_available() {
        return Ok(());
    }

    let mut machine = taxon_native_machine(taxon_native_fetch_backend()?)?;
    let args = taxon_native_args(&mut machine)?;
    let projected_build_rs = machine
        .demand_i64("taxon_blake3_build_rs", args.clone())
        .map_err(|err| format!("taxon_blake3_build_rs failed: {err}"))?;
    assert_projected_blake3_build_main(&mut machine, projected_build_rs)?;

    let demanded = machine.demand_i64("taxon_blake3_build_script_run", args);
    let run = match demanded {
        Ok(run) => run,
        Err(err) => {
            let err = format!("taxon_blake3_build_script_run failed: {err}");
            write_tier_a_artifact(
                "taxon-blake3-build-script-error.txt",
                &format!("{err}\nrustc argv trace:\n{}", rustc_argv_trace(&machine)),
            )?;
            return Err(err);
        }
    };

    let stdout = match tree_file_bytes(&mut machine, run, "build.stdout") {
        Ok(stdout) => stdout,
        Err(err) => {
            write_tier_a_artifact(
                "taxon-blake3-build-script-error.txt",
                &format!("{err}\nrustc argv trace:\n{}", rustc_argv_trace(&machine)),
            )?;
            return Err(err);
        }
    };
    let stdout = String::from_utf8_lossy(&stdout).into_owned();
    write_tier_a_artifact("taxon-blake3-build-stdout.txt", &stdout)?;

    if stdout.contains("undeclared C-toolchain capability") {
        return Err(format!(
            "blake3 build.rs hit undeclared C-toolchain trap:\n{stdout}"
        ));
    }

    Ok(())
}

#[test]
fn taxon_ladder_builds_cc_host_unit_and_hashes_artifacts() -> Result<(), String> {
    if !host_rustc_available() {
        return Ok(());
    }

    let mut machine = taxon_native_machine(taxon_native_fetch_backend()?)?;
    let args = taxon_native_args(&mut machine)?;

    let built = demand_with_rustc_trace(&mut machine, "taxon_cc_host_unit", args)?;
    assert_rustc_requests_include_crates(&machine, &["shlex", "cc"])?;

    let receipts = artifact_receipt_table(
        &mut machine,
        built,
        &[
            ArtifactExpectation {
                path: "libcc.rlib",
                archive: true,
            },
            ArtifactExpectation {
                path: "libcc.rmeta",
                archive: false,
            },
        ],
    )?;
    write_tier_a_artifact("taxon-cc-host-artifact-receipts.tsv", &receipts)?;
    write_tier_a_artifact("taxon-cc-host-rustc-trace.txt", &rustc_argv_trace(&machine))?;

    Ok(())
}

#[test]
fn taxon_ladder_builds_taxon_with_real_process_and_hashes_artifacts() -> Result<(), String> {
    if !host_rustc_available() {
        return Ok(());
    }

    let mut machine = taxon_native_machine(taxon_native_fetch_backend()?)?;
    let args = taxon_native_args(&mut machine)?;
    let literal_member_name = machine
        .demand_i64("taxon_literal_member_package_name", args.clone())
        .map_err(|err| format!("taxon_literal_member_package_name failed: {err}"))?;
    write_tier_a_artifact(
        "taxon-literal-member-package-name.txt",
        &rendered_result_string(
            &machine,
            "taxon_literal_member_package_name",
            literal_member_name,
        )?,
    )?;
    let member_name = machine
        .demand_i64("taxon_member_package_name", args.clone())
        .map_err(|err| format!("taxon_member_package_name failed: {err}"))?;
    write_tier_a_artifact(
        "taxon-member-package-name.txt",
        &rendered_result_string(&machine, "taxon_member_package_name", member_name)?,
    )?;
    let member_version = machine
        .demand_i64("taxon_member_package_version", args.clone())
        .map_err(|err| format!("taxon_member_package_version failed: {err}"))?;
    write_tier_a_artifact(
        "taxon-member-package-version.txt",
        &rendered_result_string(&machine, "taxon_member_package_version", member_version)?,
    )?;
    let member_package_count = machine
        .demand_i64("taxon_member_only_package_count", args.clone())
        .map_err(|err| format!("taxon_member_only_package_count failed: {err}"))?;
    write_tier_a_artifact(
        "taxon-member-only-package-count.txt",
        &format!(
            "{:?}",
            machine.render_result("taxon_member_only_package_count", member_package_count)?
        ),
    )?;
    let member_packages = machine
        .demand_i64("taxon_member_only_packages_text", args.clone())
        .map_err(|err| format!("taxon_member_only_packages_text failed: {err}"))?;
    write_tier_a_artifact(
        "taxon-member-only-packages.txt",
        &rendered_result_string(&machine, "taxon_member_only_packages_text", member_packages)?,
    )?;
    let package_count = machine
        .demand_i64("taxon_index_package_count", args.clone())
        .map_err(|err| format!("taxon_index_package_count failed: {err}"))?;
    write_tier_a_artifact(
        "taxon-index-package-count.txt",
        &format!("{:?}", machine.render_result("taxon_index_package_count", package_count)?),
    )?;
    let packages = machine
        .demand_i64("taxon_index_packages_text", args.clone())
        .map_err(|err| format!("taxon_index_packages_text failed: {err}"))?;
    write_tier_a_artifact(
        "taxon-index-packages.txt",
        &rendered_result_string(&machine, "taxon_index_packages_text", packages)?,
    )?;
    for name in [
        "taxon_root_pkg_id",
        "taxon_root_candidate_count",
        "taxon_root_version_count",
    ] {
        let value = machine
            .demand_i64(name, args.clone())
            .map_err(|err| format!("{name} failed: {err}"))?;
        write_tier_a_artifact(
            &format!("{name}.txt"),
            &format!("{:?}", machine.render_result(name, value)?),
        )?;
    }
    let selected = machine
        .demand_i64("taxon_selected_versions", args.clone())
        .map_err(|err| format!("taxon_selected_versions failed: {err}"))?;
    write_tier_a_artifact(
        "taxon-root-selected.txt",
        &rendered_result_string(&machine, "taxon_selected_versions", selected)?,
    )?;
    let source_names = machine
        .demand_i64("taxon_source_package_names_text", args.clone())
        .map_err(|err| format!("taxon_source_package_names_text failed: {err}"))?;
    write_tier_a_artifact(
        "taxon-source-package-names.txt",
        &rendered_result_string(&machine, "taxon_source_package_names_text", source_names)?,
    )?;
    let build_script_run = demand_with_rustc_trace(
        &mut machine,
        "taxon_blake3_build_script_run",
        args.clone(),
    )?;
    let build_script_stdout = tree_file_bytes(&mut machine, build_script_run, "build.stdout")?;
    write_tier_a_artifact(
        "taxon-blake3-build-stdout-final.txt",
        &String::from_utf8_lossy(&build_script_stdout),
    )?;
    let cfgs = machine.demand_i64("taxon_blake3_cfgs", args.clone())?;
    write_tier_a_artifact(
        "taxon-blake3-cfgs.txt",
        &rendered_result_string(&machine, "taxon_blake3_cfgs", cfgs)?,
    )?;
    let root_deps = machine.demand_i64("taxon_root_deps_text", args.clone())?;
    write_tier_a_artifact(
        "taxon-root-deps.txt",
        &rendered_result_string(&machine, "taxon_root_deps_text", root_deps)?,
    )?;
    let blake3 = demand_with_rustc_trace(&mut machine, "taxon_blake3_link", args.clone())?;
    let blake3_receipts = artifact_receipt_table(
        &mut machine,
        blake3,
        &[
            ArtifactExpectation {
                path: "libblake3.rlib",
                archive: true,
            },
            ArtifactExpectation {
                path: "libblake3.rmeta",
                archive: false,
            },
        ],
    )?;
    write_tier_a_artifact("taxon-blake3-artifact-receipts.tsv", &blake3_receipts)?;

    let built = match demand_with_rustc_trace(&mut machine, "taxon_root_link", args) {
        Ok(built) => built,
        Err(err) => {
            write_tier_a_artifact("taxon-final-rustc-trace.txt", &rustc_argv_trace(&machine))?;
            return Err(err);
        }
    };
    assert_rustc_requests_include_crates(
        &machine,
        &[
            "shlex",
            "cc",
            "arrayref",
            "arrayvec",
            "cfg_if",
            "constant_time_eq",
            "blake3",
            "taxon",
        ],
    )?;

    let receipts = artifact_receipt_table(
        &mut machine,
        built,
        &[
            ArtifactExpectation {
                path: "libtaxon.rlib",
                archive: true,
            },
            ArtifactExpectation {
                path: "libtaxon.rmeta",
                archive: false,
            },
        ],
    )?;
    write_tier_a_artifact("taxon-final-artifact-receipts.tsv", &receipts)?;
    write_tier_a_artifact("taxon-final-rustc-trace.txt", &rustc_argv_trace(&machine))?;

    Ok(())
}

#[test]
fn taxon_ladder_missing_cc_capability_traps_blake3_build_script() -> Result<(), String> {
    if !host_rustc_available() {
        return Ok(());
    }

    let mut machine = taxon_native_machine(taxon_native_fetch_backend()?)?;
    let args = taxon_native_args(&mut machine)?;
    let err = machine
        .demand_i64("taxon_blake3_build_script_run_without_declared_cc", args)
        .expect_err("blake3 build.rs must not access cc without declared capability");
    assert!(
        err.contains("undeclared C-toolchain capability"),
        "unexpected missing-capability error: {err}\nrustc argv trace:\n{}",
        rustc_argv_trace(&machine)
    );
    Ok(())
}

#[test]
fn taxon_ladder_wrong_cc_capability_traps_blake3_build_script() -> Result<(), String> {
    if !host_rustc_available() {
        return Ok(());
    }

    let mut machine = taxon_native_machine(taxon_native_fetch_backend()?)?;
    let args = taxon_native_args(&mut machine)?;
    let err = machine
        .demand_i64("taxon_blake3_build_script_run_with_wrong_toolchain", args)
        .expect_err("blake3 build.rs must not accept a non-cc toolchain declaration");
    assert!(
        err.contains("undeclared C-toolchain capability"),
        "unexpected wrong-capability error: {err}\nrustc argv trace:\n{}",
        rustc_argv_trace(&machine)
    );
    Ok(())
}

#[test]
fn taxon_ladder_changed_blake3_archive_fails_checksum_pin() -> Result<(), String> {
    let mut machine = taxon_native_machine(taxon_native_fetch_backend_with_changed_blake3()?)?;
    let args = taxon_native_args(&mut machine)?;
    let err = machine
        .demand_i64("taxon_blake3_build_rs", args)
        .expect_err("changed blake3 archive bytes must fail the sparse-row checksum pin");
    assert!(
        err.contains("checksum mismatch") || err.contains("sha256"),
        "unexpected checksum error: {err}"
    );
    Ok(())
}

#[test]
#[ignore = "demo: builds facet-core --no-default-features through real rustc and build.rs"]
fn facet_core_ladder_builds_facet_core_with_real_process_and_hashes_artifacts() -> Result<(), String>
{
    if !host_rustc_available() {
        return Ok(());
    }

    let graph = cargo_facet_core_no_default_unit_graph_oracle()?;
    if graph.units.is_empty() {
        return Ok(());
    }
    assert_facet_core_unit_graph_shape(&graph)?;

    let host = host_triple()?;
    let bridge = facet_core_demo_bridge_source(&graph, &host)?;
    write_tier_a_artifact("facet-core-demo-bridge.vix", &bridge)?;
    let source_tree = facet_core_demo_source_tree(&graph)?;
    assert_source_tree_has_build_main(
        &source_tree,
        "facet-core/build.rs",
        "facet-core-build-source-head.txt",
    )?;
    let source = format!("{RODIN_SOURCE}\n\n{SOURCE}\n\n{bridge}");
    let backend = Arc::new(RealProcessBackend::new());
    let mut machine = Machine::load(&source)?.with_exec_backend(backend);
    let source_arg = machine.intern_arg("Tree", MachineArg::Tree(source_tree))?.0;

    let selected = machine.demand_i64("facet_core_root_selected_names", Vec::new())?;
    write_tier_a_artifact(
        "facet-core-root-selected.txt",
        &rendered_result_string(&machine, "facet_core_root_selected_names", selected)?,
    )?;
    let build_script_run = demand_with_rustc_trace(
        &mut machine,
        "facet_core_build_script_run",
        vec![source_arg],
    )?;
    let build_script_stdout = tree_file_bytes(&mut machine, build_script_run, "build.stdout")?;
    write_tier_a_artifact(
        "facet-core-build-stdout-final.txt",
        &String::from_utf8_lossy(&build_script_stdout),
    )?;
    let cfgs = machine.demand_i64("facet_core_build_script_cfgs", vec![source_arg])?;
    write_tier_a_artifact(
        "facet-core-build-cfgs.txt",
        &rendered_result_string(&machine, "facet_core_build_script_cfgs", cfgs)?,
    )?;
    let root_deps = machine.demand_i64("facet_core_root_deps_text", vec![source_arg])?;
    write_tier_a_artifact(
        "facet-core-root-deps.txt",
        &rendered_result_string(&machine, "facet_core_root_deps_text", root_deps)?,
    )?;

    let built =
        match demand_with_rustc_trace(&mut machine, "facet_core_root_link", vec![source_arg]) {
            Ok(built) => built,
            Err(err) => {
                write_tier_a_artifact(
                    "facet-core-final-rustc-trace.txt",
                    &rustc_argv_trace(&machine),
                )?;
                return Err(err);
            }
        };
    assert_rustc_requests_include_crates(
        &machine,
        &[
            "autocfg",
            "build_script_build",
            "const_fnv1a_hash",
            "impls",
            "facet_core",
        ],
    )?;

    let receipts = artifact_receipt_table(
        &mut machine,
        built,
        &[
            ArtifactExpectation {
                path: "libfacet_core.rlib",
                archive: true,
            },
            ArtifactExpectation {
                path: "libfacet_core.rmeta",
                archive: false,
            },
        ],
    )?;
    write_tier_a_artifact("facet-core-final-artifact-receipts.tsv", &receipts)?;
    write_tier_a_artifact(
        "facet-core-final-rustc-trace.txt",
        &rustc_argv_trace(&machine),
    )?;

    Ok(())
}

#[test]
#[ignore = "demo: builds facet default features through real rustc and proc-macro dylibs"]
fn facet_ladder_builds_facet_with_real_process_proc_macros_and_hashes_artifacts()
-> Result<(), String> {
    if !host_rustc_available() {
        return Ok(());
    }

    let graph = cargo_facet_default_unit_graph_oracle()?;
    if graph.units.is_empty() {
        return Ok(());
    }
    assert_facet_unit_graph_shape(&graph)?;

    let host = host_triple()?;
    let bridge = facet_demo_bridge_source(&graph, &host)?;
    write_tier_a_artifact("facet-demo-bridge.vix", &bridge)?;
    let source_tree = facet_demo_source_tree(&graph)?;
    assert_source_tree_has_build_main(
        &source_tree,
        "facet/build.rs",
        "facet-build-source-head.txt",
    )?;
    let source = format!("{RODIN_SOURCE}\n\n{SOURCE}\n\n{bridge}");
    let backend = Arc::new(RealProcessBackend::new());
    let mut machine = Machine::load(&source)?.with_exec_backend(backend);
    let source_arg = machine.intern_arg("Tree", MachineArg::Tree(source_tree))?.0;

    let selected = machine.demand_i64("facet_root_selected_names", Vec::new())?;
    write_tier_a_artifact(
        "facet-root-selected.txt",
        &rendered_result_string(&machine, "facet_root_selected_names", selected)?,
    )?;
    let root_deps = machine.demand_i64("facet_root_deps_text", vec![source_arg])?;
    write_tier_a_artifact(
        "facet-root-deps.txt",
        &rendered_result_string(&machine, "facet_root_deps_text", root_deps)?,
    )?;
    let build_script_run =
        demand_with_rustc_trace(&mut machine, "facet_build_script_run", vec![source_arg])?;
    let build_script_stdout = tree_file_bytes(&mut machine, build_script_run, "build.stdout")?;
    write_tier_a_artifact(
        "facet-build-stdout-final.txt",
        &String::from_utf8_lossy(&build_script_stdout),
    )?;
    let cfgs = machine.demand_i64("facet_build_script_cfgs", vec![source_arg])?;
    write_tier_a_artifact(
        "facet-build-cfgs.txt",
        &rendered_result_string(&machine, "facet_build_script_cfgs", cfgs)?,
    )?;
    let proc_macro_deps = machine.demand_i64("facet_macros_deps_text", vec![source_arg])?;
    let proc_macro_deps_text =
        rendered_result_string(&machine, "facet_macros_deps_text", proc_macro_deps)?;
    write_tier_a_artifact("facet-macros-deps.txt", &proc_macro_deps_text)?;
    if !proc_macro_deps_text
        .lines()
        .any(|line| line == "facet_macros_impl")
    {
        return Err(format!(
            "facet_macros derived deps did not include facet_macros_impl:\n{proc_macro_deps_text}"
        ));
    }
    let proc_macro_cfgs = machine.demand_i64("facet_macros_cfgs_text", vec![source_arg])?;
    let proc_macro_cfgs_text =
        rendered_result_string(&machine, "facet_macros_cfgs_text", proc_macro_cfgs)?;
    write_tier_a_artifact("facet-macros-cfgs.txt", &proc_macro_cfgs_text)?;
    if !proc_macro_cfgs_text
        .lines()
        .any(|line| line == "feature=\"helpful-derive\"")
    {
        return Err(format!(
            "facet_macros derived cfgs did not include helpful-derive:\n{proc_macro_cfgs_text}"
        ));
    }

    let proc_macro =
        demand_with_rustc_trace(&mut machine, "facet_macros_proc_macro", vec![source_arg])?;
    let proc_macro_file = facet_proc_macro_artifact_file(&host, "facet_macros");
    let proc_macro_receipts = artifact_receipt_table(
        &mut machine,
        proc_macro,
        &[ArtifactExpectation {
            path: &proc_macro_file,
            archive: false,
        }],
    )?;
    write_tier_a_artifact(
        "facet-macros-proc-macro-artifact-receipts.tsv",
        &proc_macro_receipts,
    )?;

    let built = match demand_with_rustc_trace(&mut machine, "facet_root_link", vec![source_arg]) {
        Ok(built) => built,
        Err(err) => {
            write_tier_a_artifact("facet-final-rustc-trace.txt", &rustc_argv_trace(&machine))?;
            return Err(err);
        }
    };
    assert_rustc_requests_include_crates(
        &machine,
        &[
            "autocfg",
            "build_script_build",
            "facet_core",
            "facet_macro_parse",
            "facet_macro_types",
            "facet_macros_impl",
            "facet_macros",
            "proc_macro2",
            "quote",
            "strsim",
            "unsynn",
            "facet",
        ],
    )?;

    let receipts = artifact_receipt_table(
        &mut machine,
        built,
        &[
            ArtifactExpectation {
                path: "libfacet.rlib",
                archive: true,
            },
            ArtifactExpectation {
                path: "libfacet.rmeta",
                archive: false,
            },
        ],
    )?;
    write_tier_a_artifact("facet-final-artifact-receipts.tsv", &receipts)?;
    write_tier_a_artifact("facet-final-rustc-trace.txt", &rustc_argv_trace(&machine))?;

    Ok(())
}

fn assert_facet_core_unit_graph_shape(graph: &CargoUnitGraph) -> Result<(), String> {
    let root = graph
        .roots
        .first()
        .copied()
        .ok_or_else(|| "facet-core cargo unit graph had no root".to_string())?;
    let root_unit = graph
        .units
        .get(root)
        .ok_or_else(|| format!("facet-core root index {root} was absent"))?;
    if !root_unit.pkg_id.contains("/facet-core#")
        || !root_unit.target.kind.iter().any(|kind| kind == "lib")
        || !root_unit.features.is_empty()
    {
        return Err(format!(
            "facet-core root was not the no-default-features lib unit: {root_unit:#?}"
        ));
    }
    if graph
        .units
        .iter()
        .any(|unit| unit.target.kind.iter().any(|kind| kind == "proc-macro"))
    {
        return Err(format!(
            "facet-core rung unexpectedly pulled a proc-macro unit: {graph:#?}"
        ));
    }
    let has_build_script = graph.units.iter().any(|unit| {
        unit.pkg_id.contains("/facet-core#")
            && unit.mode == "build"
            && unit.target.kind.iter().any(|kind| kind == "custom-build")
    });
    let has_autocfg = graph
        .units
        .iter()
        .any(|unit| unit.pkg_id.contains("#autocfg@") && unit.profile.debuginfo == 0);
    if !has_build_script || !has_autocfg {
        return Err(format!(
            "facet-core rung missed build.rs/autocfg units: build_script={has_build_script} autocfg={has_autocfg}\n{graph:#?}"
        ));
    }
    Ok(())
}

fn assert_facet_unit_graph_shape(graph: &CargoUnitGraph) -> Result<(), String> {
    let root = graph
        .roots
        .first()
        .copied()
        .ok_or_else(|| "facet cargo unit graph had no root".to_string())?;
    let root_unit = graph
        .units
        .get(root)
        .ok_or_else(|| format!("facet root index {root} was absent"))?;
    let expected_features = BTreeSet::from([
        "alloc".to_string(),
        "default".to_string(),
        "doc".to_string(),
        "helpful-derive".to_string(),
        "std".to_string(),
    ]);
    let actual_features = root_unit.features.iter().cloned().collect::<BTreeSet<_>>();
    if !root_unit.pkg_id.contains("/facet#")
        || !root_unit.target.kind.iter().any(|kind| kind == "lib")
        || actual_features != expected_features
    {
        return Err(format!(
            "facet root was not the default-feature lib unit: {root_unit:#?}"
        ));
    }
    let has_facet_proc_macro = graph.units.iter().any(|unit| {
        unit.pkg_id.contains("/facet-macros#")
            && unit.mode == "build"
            && unit.target.kind.iter().any(|kind| kind == "proc-macro")
    });
    let has_proc_macro_with_deps = graph.units.iter().any(|unit| {
        unit.pkg_id.contains("/facet-macros#")
            && unit.target.kind.iter().any(|kind| kind == "proc-macro")
            && !unit.dependencies.is_empty()
    });
    let has_root_build_script = graph.units.iter().any(|unit| {
        unit.pkg_id.contains("/facet#")
            && unit.mode == "build"
            && unit.target.kind.iter().any(|kind| kind == "custom-build")
    });
    if !has_facet_proc_macro || !has_proc_macro_with_deps || !has_root_build_script {
        return Err(format!(
            "facet rung missed expected build/proc-macro units: proc_macro={has_facet_proc_macro} proc_macro_deps={has_proc_macro_with_deps} build_script={has_root_build_script}\n{graph:#?}"
        ));
    }
    Ok(())
}

fn facet_demo_bridge_source(graph: &CargoUnitGraph, host: &str) -> Result<String, String> {
    let ids = taxon_included_unit_ids(graph);
    let run_to_build = taxon_run_custom_build_map(graph);
    let root_cargo_index = graph
        .roots
        .first()
        .copied()
        .ok_or_else(|| "facet cargo unit graph had no root".to_string())?;
    let root = *ids
        .get(&root_cargo_index)
        .ok_or_else(|| format!("facet root unit {root_cargo_index} was not included"))?;
    let root_version = taxon_pkg_version(&graph.units[root_cargo_index].pkg_id)?;
    let build_script = graph
        .units
        .iter()
        .enumerate()
        .find(|(_, unit)| {
            unit.pkg_id.contains("/facet#")
                && unit.mode == "build"
                && unit.target.kind.iter().any(|kind| kind == "custom-build")
        })
        .and_then(|(index, _)| ids.get(&index).copied())
        .ok_or_else(|| "missing facet custom-build unit".to_string())?;
    let facet_macros = graph
        .units
        .iter()
        .enumerate()
        .find(|(_, unit)| {
            unit.pkg_id.contains("/facet-macros#")
                && unit.mode == "build"
                && unit.target.kind.iter().any(|kind| kind == "proc-macro")
        })
        .and_then(|(index, _)| ids.get(&index).copied())
        .ok_or_else(|| "missing facet-macros proc-macro unit".to_string())?;
    let mut root_link_deps = Vec::new();
    for dep in &graph.units[root_cargo_index].dependencies {
        let mapped_dep = run_to_build.get(&dep.index).copied().unwrap_or(dep.index);
        let Some(dep_id) = ids.get(&mapped_dep).copied() else {
            continue;
        };
        let unit = &graph.units[mapped_dep];
        if taxon_unit_kind(unit) == "build-script" {
            continue;
        }
        root_link_deps.push((
            dep_id,
            unit.target.name.replace('-', "_"),
            taxon_unit_kind(unit),
        ));
    }

    let mut out = String::new();
    out.push_str("fn facet_index() -> Index {\n");
    out.push_str("    let names: Map<Int, String> = {};\n");
    out.push_str("    let version_pkgs: Map<Int, Int> = {};\n");
    out.push_str("    let version_values: Map<Int, String> = {};\n");
    out.push_str("    let guard_clause_ids: Map<Int, Int> = {};\n");
    out.push_str("    let guard_tags: Map<Int, String> = {};\n");
    out.push_str("    let guard_kinds: Map<Int, Int> = {};\n");
    out.push_str("    let guard_pkgs: Map<Int, Int> = {};\n");
    out.push_str("    let guard_version_values: Map<Int, String> = {};\n");
    out.push_str("    let guard_features: Map<Int, Int> = {};\n");
    out.push_str("    let consequent_tags: Map<Int, String> = {};\n");
    out.push_str("    let consequent_pkgs: Map<Int, Int> = {};\n");
    out.push_str("    let consequent_version_sets: Map<Int, VersionSet> = {};\n");
    out.push_str("    let consequent_features: Map<Int, Int> = {};\n");
    out.push_str("    let gate_kinds: Map<Int, String> = {};\n");
    out.push_str("    let gate_targets: Map<Int, String> = {};\n");

    let mut packages = Vec::new();
    let mut version_ids = Vec::new();
    for (cargo_index, id) in &ids {
        let unit = &graph.units[*cargo_index];
        packages.push(id.to_string());
        version_ids.push(id.to_string());
        out.push_str(&format!(
            "    let names = names.insert({id}, {});\n",
            vix_string(&taxon_unit_name(unit, *cargo_index))
        ));
        out.push_str(&format!(
            "    let version_pkgs = version_pkgs.insert({id}, {id});\n"
        ));
        out.push_str(&format!(
            "    let version_values = version_values.insert({id}, {});\n",
            vix_string(&taxon_pkg_version(&unit.pkg_id)?)
        ));
    }

    let mut clause = 0usize;
    let mut guard = 0usize;
    for (cargo_index, id) in &ids {
        let unit = &graph.units[*cargo_index];
        for dep in &unit.dependencies {
            let mapped_dep = run_to_build.get(&dep.index).copied().unwrap_or(dep.index);
            let Some(dep_id) = ids.get(&mapped_dep).copied() else {
                continue;
            };
            let dep_version = taxon_pkg_version(&graph.units[mapped_dep].pkg_id)?;
            push_taxon_clause(
                &mut out,
                &mut clause,
                &mut guard,
                *id,
                dep_id,
                "*",
                "in_graph",
            );
            push_taxon_clause(
                &mut out,
                &mut clause,
                &mut guard,
                *id,
                dep_id,
                &format!("={dep_version}"),
                "version_set",
            );
        }
    }

    out.push_str("    Index {\n");
    out.push_str(&format!("        packages: [{}],\n", packages.join(", ")));
    out.push_str("        names: names,\n");
    out.push_str(&format!(
        "        version_ids: [{}],\n",
        version_ids.join(", ")
    ));
    out.push_str("        version_pkgs: version_pkgs,\n");
    out.push_str("        version_values: version_values,\n");
    out.push_str(&format!(
        "        clause_ids: [{}],\n",
        (0..clause)
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    ));
    out.push_str(&format!(
        "        guard_ids: [{}],\n",
        (0..guard)
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    ));
    out.push_str("        guard_clause_ids: guard_clause_ids,\n");
    out.push_str("        guard_tags: guard_tags,\n");
    out.push_str("        guard_kinds: guard_kinds,\n");
    out.push_str("        guard_pkgs: guard_pkgs,\n");
    out.push_str("        guard_version_values: guard_version_values,\n");
    out.push_str("        guard_features: guard_features,\n");
    out.push_str("        consequent_tags: consequent_tags,\n");
    out.push_str("        consequent_pkgs: consequent_pkgs,\n");
    out.push_str("        consequent_version_sets: consequent_version_sets,\n");
    out.push_str("        consequent_features: consequent_features,\n");
    out.push_str("        gate_kinds: gate_kinds,\n");
    out.push_str("        gate_targets: gate_targets,\n");
    out.push_str("    }\n");
    out.push_str("}\n\n");

    out.push_str("fn facet_problem() -> Problem {\n");
    out.push_str(&format!(
        "    Problem {{ root_pkg: {root}, root_req: VersionSet::from_req({}), root_features: [], root_default_feature: 0, root_default_features: true }}\n",
        vix_string(&format!("={root_version}"))
    ));
    out.push_str("}\n\n");

    out.push_str("fn facet_targets() -> UnitTargetTable {\n");
    out.push_str("    let targets: Map<Int, UnitTarget> = {};\n");
    for (cargo_index, id) in &ids {
        let unit = &graph.units[*cargo_index];
        let logical = taxon_logical_unit_root(unit)?;
        let source = taxon_unit_source_suffix(unit)?;
        let kind = taxon_unit_kind(unit);
        let cfgs = unit
            .features
            .iter()
            .map(|feature| vix_string(&format!("feature=\"{feature}\"")))
            .collect::<Vec<_>>()
            .join(", ");
        let crate_name = unit.target.name.replace('-', "_");
        let (metadata, link, metadata_file, link_file) = if kind == "build-script" {
            (
                "build_script_build.rmeta".to_string(),
                "build_script".to_string(),
                "build_script_build.rmeta".to_string(),
                "build_script".to_string(),
            )
        } else if kind == "proc-macro" {
            let dylib = facet_proc_macro_artifact_file(host, &crate_name);
            (dylib.clone(), dylib.clone(), dylib.clone(), dylib)
        } else {
            (
                format!("lib{crate_name}.rmeta"),
                format!("lib{crate_name}.rlib"),
                format!("lib{crate_name}.rmeta"),
                format!("lib{crate_name}.rlib"),
            )
        };
        out.push_str(&format!(
            "    let targets = targets.insert({id}, UnitTarget {{ kind: {}, manifest: p{}, source: p{}, cfgs: [{}], metadata: p{}, link: p{}, metadata_file: {}, link_file: {} }});\n",
            vix_string(&kind),
            vix_string(&logical),
            vix_string(&source),
            cfgs,
            vix_string(&metadata),
            vix_string(&link),
            vix_string(&metadata_file),
            vix_string(&link_file),
        ));
    }
    out.push_str(&format!(
        "    UnitTargetTable {{ root: {root}, targets: targets }}\n"
    ));
    out.push_str("}\n\n");
    out.push_str("fn facet_dep_names(index: Index, deps: [Int], out: [String]) -> [String] {\n");
    out.push_str("    match deps.len() == 0 {\n");
    out.push_str("        true => out,\n");
    out.push_str("        false => match deps.pop() {\n");
    out.push_str("            popped => facet_dep_names(index, popped.1, out.push(index.names.get(popped.0).unwrap())),\n");
    out.push_str("        },\n");
    out.push_str("    }\n");
    out.push_str("}\n\n");
    out.push_str("pub fn facet_root_selected_names() -> String {\n");
    out.push_str("    solve_selected_names_text(facet_index(), facet_problem(), ");
    out.push_str(&vix_string(host));
    out.push_str(")\n");
    out.push_str("}\n\n");
    out.push_str("pub fn facet_root_deps_text(source: Tree) -> String {\n");
    out.push_str("    let index = facet_index();\n");
    out.push_str("    let result = solve(index, facet_problem(), ");
    out.push_str(&vix_string(host));
    out.push_str(");\n");
    out.push_str("    let unit = solution_unit(index, result, facet_targets(), source, ");
    out.push_str(&vix_string(host));
    out.push_str(", ");
    out.push_str(&root.to_string());
    out.push_str(");\n");
    out.push_str("    facet_dep_names(index, unit.deps, []).join(\"\\n\")\n");
    out.push_str("}\n\n");
    out.push_str("pub fn facet_build_script_run(source: Tree) -> Tree {\n");
    out.push_str("    let index = facet_index();\n");
    out.push_str("    let result = solve(index, facet_problem(), ");
    out.push_str(&vix_string(host));
    out.push_str(");\n");
    out.push_str(
        "    solution_unit_built(Target::host(), source, index, result, facet_targets(), ",
    );
    out.push_str(&vix_string(host));
    out.push_str(", ");
    out.push_str(&build_script.to_string());
    out.push_str(", \"link\")\n");
    out.push_str("}\n\n");
    out.push_str("pub fn facet_build_script_cfgs(source: Tree) -> String {\n");
    out.push_str("    let run = facet_build_script_run(source);\n");
    out.push_str("    build_script_rustc_cfgs(run).join(\" \")\n");
    out.push_str("}\n\n");
    out.push_str("pub fn facet_macros_deps_text(source: Tree) -> String {\n");
    out.push_str("    let index = facet_index();\n");
    out.push_str("    let result = solve(index, facet_problem(), ");
    out.push_str(&vix_string(host));
    out.push_str(");\n");
    out.push_str("    let unit = solution_unit(index, result, facet_targets(), source, ");
    out.push_str(&vix_string(host));
    out.push_str(", ");
    out.push_str(&facet_macros.to_string());
    out.push_str(");\n");
    out.push_str("    facet_dep_names(index, unit.deps, []).join(\"\\n\")\n");
    out.push_str("}\n\n");
    out.push_str("pub fn facet_macros_cfgs_text(source: Tree) -> String {\n");
    out.push_str("    let index = facet_index();\n");
    out.push_str("    let result = solve(index, facet_problem(), ");
    out.push_str(&vix_string(host));
    out.push_str(");\n");
    out.push_str("    let unit = solution_unit(index, result, facet_targets(), source, ");
    out.push_str(&vix_string(host));
    out.push_str(", ");
    out.push_str(&facet_macros.to_string());
    out.push_str(");\n");
    out.push_str("    unit.cfgs.join(\"\\n\")\n");
    out.push_str("}\n\n");
    out.push_str("pub fn facet_macros_proc_macro(source: Tree) -> Tree {\n");
    out.push_str("    let index = facet_index();\n");
    out.push_str("    let result = solve(index, facet_problem(), ");
    out.push_str(&vix_string(host));
    out.push_str(");\n");
    out.push_str(
        "    solution_unit_built(Target::host(), source, index, result, facet_targets(), ",
    );
    out.push_str(&vix_string(host));
    out.push_str(", ");
    out.push_str(&facet_macros.to_string());
    out.push_str(", \"link\")\n");
    out.push_str("}\n\n");
    out.push_str("pub fn facet_root_link(source: Tree) -> Tree {\n");
    out.push_str("    let index = facet_index();\n");
    out.push_str("    let result = solve(index, facet_problem(), ");
    out.push_str(&vix_string(host));
    out.push_str(");\n");
    out.push_str("    let targets = facet_targets();\n");
    out.push_str("    let unit = solution_unit(index, result, targets, source, ");
    out.push_str(&vix_string(host));
    out.push_str(", ");
    out.push_str(&root.to_string());
    out.push_str(");\n");
    out.push_str("    let rustc = Rustc::acquire(Target::host());\n");
    out.push_str("    let manifest = source / unit.manifest;\n");
    out.push_str("    let edition = package_edition_from_source(source, manifest);\n");
    out.push_str("    let profile_args = rustc_profile_args(unit.profile);\n");
    out.push_str("    let build_run = facet_build_script_run(source);\n");
    out.push_str(
        "    let cfg_args = push_cfg_args(build_script_rustc_cfgs(build_run), push_cfg_args(unit.cfgs, []));\n",
    );
    out.push_str("    let source_arg = argv_source_interpolation(manifest, unit.source);\n");
    out.push_str(
        "    let deps = solution_dependency_tree(Target::host(), source, index, result, targets, ",
    );
    out.push_str(&vix_string(host));
    out.push_str(", unit, \"link\");\n");
    for (id, crate_name, kind) in &root_link_deps {
        out.push_str(&format!(
            "    let dep_{crate_name} = solution_unit_built(Target::host(), source, index, result, targets, {}, {id}, \"link\");\n",
            vix_string(host)
        ));
        let subpath = if kind == "proc-macro" {
            facet_proc_macro_artifact_file(host, crate_name)
        } else {
            format!("lib{crate_name}.rlib")
        };
        out.push_str(&format!(
            "    let dep_{crate_name}_arg = Arg::Interpolation {{ tree: dep_{crate_name}, subpath: p{} }};\n",
            vix_string(&subpath)
        ));
    }
    out.push_str("    rustc! {\n");
    out.push_str("        --crate-name {unit.name}\n");
    out.push_str("        --edition {edition}\n");
    out.push_str("        --crate-type lib\n");
    out.push_str("        {profile_args}\n");
    out.push_str("        --emit=metadata={unit.metadata},link={unit.link}\n");
    out.push_str("        -L dependency={deps}\n");
    for (_, crate_name, _) in &root_link_deps {
        out.push_str(&format!("        -L dependency={{dep_{crate_name}}}\n"));
    }
    for (_, crate_name, _) in &root_link_deps {
        out.push_str(&format!(
            "        --extern {crate_name}={{dep_{crate_name}_arg}}\n"
        ));
    }
    out.push_str("        {cfg_args}\n");
    out.push_str("        --env CARGO_MANIFEST_DIR={manifest}\n");
    out.push_str("        {source_arg}\n");
    out.push_str("    }\n");
    out.push_str("}\n");
    Ok(out)
}

fn facet_core_demo_bridge_source(graph: &CargoUnitGraph, host: &str) -> Result<String, String> {
    let ids = taxon_included_unit_ids(graph);
    let run_to_build = taxon_run_custom_build_map(graph);
    let root_cargo_index = graph
        .roots
        .first()
        .copied()
        .ok_or_else(|| "facet-core cargo unit graph had no root".to_string())?;
    let root = *ids
        .get(&root_cargo_index)
        .ok_or_else(|| format!("facet-core root unit {root_cargo_index} was not included"))?;
    let root_version = taxon_pkg_version(&graph.units[root_cargo_index].pkg_id)?;
    let build_script = graph
        .units
        .iter()
        .enumerate()
        .find(|(_, unit)| {
            unit.pkg_id.contains("/facet-core#")
                && unit.mode == "build"
                && unit.target.kind.iter().any(|kind| kind == "custom-build")
        })
        .and_then(|(index, _)| ids.get(&index).copied())
        .ok_or_else(|| "missing facet-core custom-build unit".to_string())?;
    let mut root_link_deps = Vec::new();
    for dep in &graph.units[root_cargo_index].dependencies {
        let mapped_dep = run_to_build.get(&dep.index).copied().unwrap_or(dep.index);
        let Some(dep_id) = ids.get(&mapped_dep).copied() else {
            continue;
        };
        let unit = &graph.units[mapped_dep];
        if taxon_unit_kind(unit) == "build-script" {
            continue;
        }
        root_link_deps.push((dep_id, unit.target.name.replace('-', "_")));
    }

    let mut out = String::new();
    out.push_str("fn facet_core_index() -> Index {\n");
    out.push_str("    let names: Map<Int, String> = {};\n");
    out.push_str("    let version_pkgs: Map<Int, Int> = {};\n");
    out.push_str("    let version_values: Map<Int, String> = {};\n");
    out.push_str("    let guard_clause_ids: Map<Int, Int> = {};\n");
    out.push_str("    let guard_tags: Map<Int, String> = {};\n");
    out.push_str("    let guard_kinds: Map<Int, Int> = {};\n");
    out.push_str("    let guard_pkgs: Map<Int, Int> = {};\n");
    out.push_str("    let guard_version_values: Map<Int, String> = {};\n");
    out.push_str("    let guard_features: Map<Int, Int> = {};\n");
    out.push_str("    let consequent_tags: Map<Int, String> = {};\n");
    out.push_str("    let consequent_pkgs: Map<Int, Int> = {};\n");
    out.push_str("    let consequent_version_sets: Map<Int, VersionSet> = {};\n");
    out.push_str("    let consequent_features: Map<Int, Int> = {};\n");
    out.push_str("    let gate_kinds: Map<Int, String> = {};\n");
    out.push_str("    let gate_targets: Map<Int, String> = {};\n");

    let mut packages = Vec::new();
    let mut version_ids = Vec::new();
    for (cargo_index, id) in &ids {
        let unit = &graph.units[*cargo_index];
        packages.push(id.to_string());
        version_ids.push(id.to_string());
        out.push_str(&format!(
            "    let names = names.insert({id}, {});\n",
            vix_string(&taxon_unit_name(unit, *cargo_index))
        ));
        out.push_str(&format!(
            "    let version_pkgs = version_pkgs.insert({id}, {id});\n"
        ));
        out.push_str(&format!(
            "    let version_values = version_values.insert({id}, {});\n",
            vix_string(&taxon_pkg_version(&unit.pkg_id)?)
        ));
    }

    let mut clause = 0usize;
    let mut guard = 0usize;
    for (cargo_index, id) in &ids {
        let unit = &graph.units[*cargo_index];
        for dep in &unit.dependencies {
            let mapped_dep = run_to_build.get(&dep.index).copied().unwrap_or(dep.index);
            let Some(dep_id) = ids.get(&mapped_dep).copied() else {
                continue;
            };
            let dep_version = taxon_pkg_version(&graph.units[mapped_dep].pkg_id)?;
            push_taxon_clause(
                &mut out,
                &mut clause,
                &mut guard,
                *id,
                dep_id,
                "*",
                "in_graph",
            );
            push_taxon_clause(
                &mut out,
                &mut clause,
                &mut guard,
                *id,
                dep_id,
                &format!("={dep_version}"),
                "version_set",
            );
        }
    }

    out.push_str("    Index {\n");
    out.push_str(&format!("        packages: [{}],\n", packages.join(", ")));
    out.push_str("        names: names,\n");
    out.push_str(&format!(
        "        version_ids: [{}],\n",
        version_ids.join(", ")
    ));
    out.push_str("        version_pkgs: version_pkgs,\n");
    out.push_str("        version_values: version_values,\n");
    out.push_str(&format!(
        "        clause_ids: [{}],\n",
        (0..clause)
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    ));
    out.push_str(&format!(
        "        guard_ids: [{}],\n",
        (0..guard)
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    ));
    out.push_str("        guard_clause_ids: guard_clause_ids,\n");
    out.push_str("        guard_tags: guard_tags,\n");
    out.push_str("        guard_kinds: guard_kinds,\n");
    out.push_str("        guard_pkgs: guard_pkgs,\n");
    out.push_str("        guard_version_values: guard_version_values,\n");
    out.push_str("        guard_features: guard_features,\n");
    out.push_str("        consequent_tags: consequent_tags,\n");
    out.push_str("        consequent_pkgs: consequent_pkgs,\n");
    out.push_str("        consequent_version_sets: consequent_version_sets,\n");
    out.push_str("        consequent_features: consequent_features,\n");
    out.push_str("        gate_kinds: gate_kinds,\n");
    out.push_str("        gate_targets: gate_targets,\n");
    out.push_str("    }\n");
    out.push_str("}\n\n");

    out.push_str("fn facet_core_problem() -> Problem {\n");
    out.push_str(&format!(
        "    Problem {{ root_pkg: {root}, root_req: VersionSet::from_req({}), root_features: [], root_default_feature: 0, root_default_features: false }}\n",
        vix_string(&format!("={root_version}"))
    ));
    out.push_str("}\n\n");

    out.push_str("fn facet_core_targets() -> UnitTargetTable {\n");
    out.push_str("    let targets: Map<Int, UnitTarget> = {};\n");
    for (cargo_index, id) in &ids {
        let unit = &graph.units[*cargo_index];
        let logical = taxon_logical_unit_root(unit)?;
        let source = taxon_unit_source_suffix(unit)?;
        let kind = taxon_unit_kind(unit);
        let cfgs = unit
            .features
            .iter()
            .map(|feature| vix_string(&format!("feature=\"{feature}\"")))
            .collect::<Vec<_>>()
            .join(", ");
        let crate_name = unit.target.name.replace('-', "_");
        let (metadata, link, metadata_file, link_file) = if kind == "build-script" {
            (
                "build_script_build.rmeta".to_string(),
                "build_script".to_string(),
                "build_script_build.rmeta".to_string(),
                "build_script".to_string(),
            )
        } else {
            (
                format!("lib{crate_name}.rmeta"),
                format!("lib{crate_name}.rlib"),
                format!("lib{crate_name}.rmeta"),
                format!("lib{crate_name}.rlib"),
            )
        };
        out.push_str(&format!(
            "    let targets = targets.insert({id}, UnitTarget {{ kind: {}, manifest: p{}, source: p{}, cfgs: [{}], metadata: p{}, link: p{}, metadata_file: {}, link_file: {} }});\n",
            vix_string(&kind),
            vix_string(&logical),
            vix_string(&source),
            cfgs,
            vix_string(&metadata),
            vix_string(&link),
            vix_string(&metadata_file),
            vix_string(&link_file),
        ));
    }
    out.push_str(&format!(
        "    UnitTargetTable {{ root: {root}, targets: targets }}\n"
    ));
    out.push_str("}\n\n");
    out.push_str(
        "fn facet_core_dep_names(index: Index, deps: [Int], out: [String]) -> [String] {\n",
    );
    out.push_str("    match deps.len() == 0 {\n");
    out.push_str("        true => out,\n");
    out.push_str("        false => match deps.pop() {\n");
    out.push_str("            popped => facet_core_dep_names(index, popped.1, out.push(index.names.get(popped.0).unwrap())),\n");
    out.push_str("        },\n");
    out.push_str("    }\n");
    out.push_str("}\n\n");
    out.push_str("pub fn facet_core_root_selected_names() -> String {\n");
    out.push_str("    solve_selected_names_text(facet_core_index(), facet_core_problem(), ");
    out.push_str(&vix_string(host));
    out.push_str(")\n");
    out.push_str("}\n\n");
    out.push_str("pub fn facet_core_root_deps_text(source: Tree) -> String {\n");
    out.push_str("    let index = facet_core_index();\n");
    out.push_str("    let result = solve(index, facet_core_problem(), ");
    out.push_str(&vix_string(host));
    out.push_str(");\n");
    out.push_str("    let unit = solution_unit(index, result, facet_core_targets(), source, ");
    out.push_str(&vix_string(host));
    out.push_str(", ");
    out.push_str(&root.to_string());
    out.push_str(");\n");
    out.push_str("    facet_core_dep_names(index, unit.deps, []).join(\"\\n\")\n");
    out.push_str("}\n\n");
    out.push_str("pub fn facet_core_build_script_run(source: Tree) -> Tree {\n");
    out.push_str("    let index = facet_core_index();\n");
    out.push_str("    let result = solve(index, facet_core_problem(), ");
    out.push_str(&vix_string(host));
    out.push_str(");\n");
    out.push_str(
        "    solution_unit_built(Target::host(), source, index, result, facet_core_targets(), ",
    );
    out.push_str(&vix_string(host));
    out.push_str(", ");
    out.push_str(&build_script.to_string());
    out.push_str(", \"link\")\n");
    out.push_str("}\n\n");
    out.push_str("pub fn facet_core_build_script_cfgs(source: Tree) -> String {\n");
    out.push_str("    let run = facet_core_build_script_run(source);\n");
    out.push_str("    build_script_rustc_cfgs(run).join(\" \")\n");
    out.push_str("}\n\n");
    out.push_str("pub fn facet_core_root_link(source: Tree) -> Tree {\n");
    out.push_str("    let index = facet_core_index();\n");
    out.push_str("    let result = solve(index, facet_core_problem(), ");
    out.push_str(&vix_string(host));
    out.push_str(");\n");
    out.push_str("    let targets = facet_core_targets();\n");
    out.push_str("    let unit = solution_unit(index, result, targets, source, ");
    out.push_str(&vix_string(host));
    out.push_str(", ");
    out.push_str(&root.to_string());
    out.push_str(");\n");
    out.push_str("    let rustc = Rustc::acquire(Target::host());\n");
    out.push_str("    let manifest = source / unit.manifest;\n");
    out.push_str("    let edition = package_edition_from_source(source, manifest);\n");
    out.push_str("    let profile_args = rustc_profile_args(unit.profile);\n");
    out.push_str("    let build_run = facet_core_build_script_run(source);\n");
    out.push_str(
        "    let cfg_args = push_cfg_args(build_script_rustc_cfgs(build_run), push_cfg_args(unit.cfgs, []));\n",
    );
    out.push_str("    let source_arg = argv_source_interpolation(manifest, unit.source);\n");
    out.push_str(
        "    let deps = solution_dependency_tree(Target::host(), source, index, result, targets, ",
    );
    out.push_str(&vix_string(host));
    out.push_str(", unit, \"link\");\n");
    for (id, crate_name) in &root_link_deps {
        out.push_str(&format!(
            "    let dep_{crate_name} = solution_unit_built(Target::host(), source, index, result, targets, {}, {id}, \"link\");\n",
            vix_string(host)
        ));
        out.push_str(&format!(
            "    let dep_{crate_name}_arg = Arg::Interpolation {{ tree: dep_{crate_name}, subpath: p\"lib{crate_name}.rlib\" }};\n"
        ));
    }
    out.push_str("    rustc! {\n");
    out.push_str("        --crate-name {unit.name}\n");
    out.push_str("        --edition {edition}\n");
    out.push_str("        --crate-type lib\n");
    out.push_str("        {profile_args}\n");
    out.push_str("        --emit=metadata={unit.metadata},link={unit.link}\n");
    out.push_str("        -L dependency={deps}\n");
    for (_, crate_name) in &root_link_deps {
        out.push_str(&format!("        -L dependency={{dep_{crate_name}}}\n"));
    }
    for (_, crate_name) in &root_link_deps {
        out.push_str(&format!(
            "        --extern {crate_name}={{dep_{crate_name}_arg}}\n"
        ));
    }
    out.push_str("        {cfg_args}\n");
    out.push_str("        --env CARGO_MANIFEST_DIR={manifest}\n");
    out.push_str("        {source_arg}\n");
    out.push_str("    }\n");
    out.push_str("}\n");
    Ok(out)
}

fn taxon_demo_bridge_source(graph: &CargoUnitGraph, host: &str) -> Result<String, String> {
    let ids = taxon_included_unit_ids(graph);
    let run_to_build = taxon_run_custom_build_map(graph);
    let root = graph
        .roots
        .first()
        .copied()
        .ok_or_else(|| "taxon cargo unit graph had no root".to_string())?;
    let root = *ids
        .get(&root)
        .ok_or_else(|| format!("taxon root unit {root} was not included"))?;
    let root_version = taxon_pkg_version(
        &graph.units[*graph
            .roots
            .first()
            .ok_or_else(|| "taxon cargo unit graph had no root".to_string())?]
        .pkg_id,
    )?;
    let blake3_build = graph
        .units
        .iter()
        .enumerate()
        .find(|(_, unit)| {
            unit.pkg_id.contains("#blake3@")
                && unit.mode == "build"
                && unit.target.kind.iter().any(|kind| kind == "custom-build")
        })
        .and_then(|(index, _)| ids.get(&index).copied())
        .ok_or_else(|| "missing blake3 custom-build unit".to_string())?;
    let blake3_lib_cargo_index = graph
        .units
        .iter()
        .enumerate()
        .find(|(_, unit)| {
            unit.pkg_id.contains("#blake3@")
                && unit.target.kind.iter().any(|kind| kind == "lib")
                && !unit.target.kind.iter().any(|kind| kind == "custom-build")
        })
        .map(|(index, _)| index)
        .ok_or_else(|| "missing blake3 lib unit".to_string())?;
    let blake3_lib = *ids
        .get(&blake3_lib_cargo_index)
        .ok_or_else(|| format!("missing vix id for blake3 lib unit {blake3_lib_cargo_index}"))?;
    let blake3_transitive_libs =
        taxon_transitive_lib_dependency_ids(graph, &ids, blake3_lib_cargo_index)?;
    let cc_unit = graph
        .units
        .iter()
        .enumerate()
        .find(|(_, unit)| unit.pkg_id.contains("#cc@") && unit.mode == "build")
        .and_then(|(index, _)| ids.get(&index).copied())
        .ok_or_else(|| "missing cc host unit".to_string())?;

    let mut out = String::new();
    out.push_str("fn taxon_index() -> Index {\n");
    out.push_str("    let names: Map<Int, String> = {};\n");
    out.push_str("    let version_pkgs: Map<Int, Int> = {};\n");
    out.push_str("    let version_values: Map<Int, String> = {};\n");
    out.push_str("    let guard_clause_ids: Map<Int, Int> = {};\n");
    out.push_str("    let guard_tags: Map<Int, String> = {};\n");
    out.push_str("    let guard_kinds: Map<Int, Int> = {};\n");
    out.push_str("    let guard_pkgs: Map<Int, Int> = {};\n");
    out.push_str("    let guard_version_values: Map<Int, String> = {};\n");
    out.push_str("    let guard_features: Map<Int, Int> = {};\n");
    out.push_str("    let consequent_tags: Map<Int, String> = {};\n");
    out.push_str("    let consequent_pkgs: Map<Int, Int> = {};\n");
    out.push_str("    let consequent_version_sets: Map<Int, VersionSet> = {};\n");
    out.push_str("    let consequent_features: Map<Int, Int> = {};\n");
    out.push_str("    let gate_kinds: Map<Int, String> = {};\n");
    out.push_str("    let gate_targets: Map<Int, String> = {};\n");

    let mut packages = Vec::new();
    let mut version_ids = Vec::new();
    for (cargo_index, id) in &ids {
        let unit = &graph.units[*cargo_index];
        packages.push(id.to_string());
        version_ids.push(id.to_string());
        out.push_str(&format!(
            "    let names = names.insert({id}, {});\n",
            vix_string(&taxon_unit_name(unit, *cargo_index))
        ));
        out.push_str(&format!(
            "    let version_pkgs = version_pkgs.insert({id}, {id});\n"
        ));
        out.push_str(&format!(
            "    let version_values = version_values.insert({id}, {});\n",
            vix_string(&taxon_pkg_version(&unit.pkg_id)?)
        ));
    }

    let mut clause = 0usize;
    let mut guard = 0usize;
    for (cargo_index, id) in &ids {
        let unit = &graph.units[*cargo_index];
        for dep in &unit.dependencies {
            let mapped_dep = run_to_build.get(&dep.index).copied().unwrap_or(dep.index);
            let Some(dep_id) = ids.get(&mapped_dep).copied() else {
                continue;
            };
            let dep_version = taxon_pkg_version(&graph.units[mapped_dep].pkg_id)?;
            push_taxon_clause(
                &mut out,
                &mut clause,
                &mut guard,
                *id,
                dep_id,
                "*",
                "in_graph",
            );
            push_taxon_clause(
                &mut out,
                &mut clause,
                &mut guard,
                *id,
                dep_id,
                &format!("={dep_version}"),
                "version_set",
            );
        }
    }

    out.push_str("    Index {\n");
    out.push_str(&format!("        packages: [{}],\n", packages.join(", ")));
    out.push_str("        names: names,\n");
    out.push_str(&format!(
        "        version_ids: [{}],\n",
        version_ids.join(", ")
    ));
    out.push_str("        version_pkgs: version_pkgs,\n");
    out.push_str("        version_values: version_values,\n");
    out.push_str(&format!(
        "        clause_ids: [{}],\n",
        (0..clause)
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    ));
    out.push_str(&format!(
        "        guard_ids: [{}],\n",
        (0..guard)
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    ));
    out.push_str("        guard_clause_ids: guard_clause_ids,\n");
    out.push_str("        guard_tags: guard_tags,\n");
    out.push_str("        guard_kinds: guard_kinds,\n");
    out.push_str("        guard_pkgs: guard_pkgs,\n");
    out.push_str("        guard_version_values: guard_version_values,\n");
    out.push_str("        guard_features: guard_features,\n");
    out.push_str("        consequent_tags: consequent_tags,\n");
    out.push_str("        consequent_pkgs: consequent_pkgs,\n");
    out.push_str("        consequent_version_sets: consequent_version_sets,\n");
    out.push_str("        consequent_features: consequent_features,\n");
    out.push_str("        gate_kinds: gate_kinds,\n");
    out.push_str("        gate_targets: gate_targets,\n");
    out.push_str("    }\n");
    out.push_str("}\n\n");

    out.push_str("fn taxon_problem() -> Problem {\n");
    out.push_str(&format!(
        "    Problem {{ root_pkg: {root}, root_req: VersionSet::from_req({}), root_features: [], root_default_feature: 0, root_default_features: true }}\n",
        vix_string(&format!("={root_version}"))
    ));
    out.push_str("}\n\n");
    out.push_str("fn taxon_blake3_build_script_problem() -> Problem {\n");
    out.push_str(&format!(
        "    Problem {{ root_pkg: {blake3_build}, root_req: VersionSet::from_req(\"*\"), root_features: [], root_default_feature: 0, root_default_features: false }}\n"
    ));
    out.push_str("}\n\n");

    out.push_str("fn taxon_targets() -> UnitTargetTable {\n");
    out.push_str("    let targets: Map<Int, UnitTarget> = {};\n");
    for (cargo_index, id) in &ids {
        let unit = &graph.units[*cargo_index];
        let logical = taxon_logical_unit_root(unit)?;
        let source = taxon_unit_source_suffix(unit)?;
        let kind = taxon_unit_kind(unit);
        let cfgs = unit
            .features
            .iter()
            .map(|feature| vix_string(&format!("feature=\"{feature}\"")))
            .collect::<Vec<_>>()
            .join(", ");
        let crate_name = unit.target.name.replace('-', "_");
        let (metadata, link, metadata_file, link_file) = if kind == "build-script" {
            (
                "build_script_build.rmeta".to_string(),
                "build_script".to_string(),
                "build_script_build.rmeta".to_string(),
                "build_script".to_string(),
            )
        } else {
            (
                format!("lib{crate_name}.rmeta"),
                format!("lib{crate_name}.rlib"),
                format!("lib{crate_name}.rmeta"),
                format!("lib{crate_name}.rlib"),
            )
        };
        out.push_str(&format!(
            "    let targets = targets.insert({id}, UnitTarget {{ kind: {}, manifest: p{}, source: p{}, cfgs: [{}], metadata: p{}, link: p{}, metadata_file: {}, link_file: {} }});\n",
            vix_string(&kind),
            vix_string(&logical),
            vix_string(&source),
            cfgs,
            vix_string(&metadata),
            vix_string(&link),
            vix_string(&metadata_file),
            vix_string(&link_file),
        ));
    }
    out.push_str(&format!(
        "    UnitTargetTable {{ root: {root}, targets: targets }}\n"
    ));
    out.push_str("}\n\n");
    out.push_str("pub fn taxon_blake3_build_rs(source: Tree) -> Tree {\n");
    out.push_str("    source / p\"registry/blake3-1.8.5\" / p\"build.rs\"\n");
    out.push_str("}\n\n");
    out.push_str("pub fn taxon_cc_host_unit(source: Tree) -> Tree {\n");
    out.push_str("    let index = taxon_index();\n");
    out.push_str("    let result = solve(index, taxon_blake3_build_script_problem(), \"host\");\n");
    out.push_str(&format!(
        "    solution_unit_built(Target::host(), source, index, result, taxon_targets(), {}, {cc_unit}, \"link\")\n",
        vix_string(host)
    ));
    out.push_str("}\n\n");
    out.push_str("pub fn taxon_root_link(source: Tree) -> Tree {\n");
    out.push_str("    let index = taxon_index();\n");
    out.push_str("    let result = solve(index, taxon_problem(), ");
    out.push_str(&vix_string(host));
    out.push_str(");\n");
    out.push_str("    let targets = taxon_targets();\n");
    out.push_str("    let unit = solution_unit(index, result, targets, source, ");
    out.push_str(&vix_string(host));
    out.push_str(", ");
    out.push_str(&root.to_string());
    out.push_str(");\n");
    out.push_str("    let blake3_unit = solution_unit(index, result, targets, source, ");
    out.push_str(&vix_string(host));
    out.push_str(", ");
    out.push_str(&blake3_lib.to_string());
    out.push_str(");\n");
    out.push_str(
        "    let blake3 = solution_unit_built(Target::host(), source, index, result, targets, ",
    );
    out.push_str(&vix_string(host));
    out.push_str(", ");
    out.push_str(&blake3_lib.to_string());
    out.push_str(", \"link\");\n");
    out.push_str(
        "    let deps = solution_dependency_tree(Target::host(), source, index, result, targets, ",
    );
    out.push_str(&vix_string(host));
    out.push_str(", blake3_unit, \"link\");\n");
    out.push_str("    let rustc = Rustc::acquire(Target::host());\n");
    out.push_str("    let manifest = source / unit.manifest;\n");
    out.push_str("    let edition = package_edition_from_source(source, manifest);\n");
    out.push_str("    let profile_args = rustc_profile_args(unit.profile);\n");
    out.push_str("    let source_arg = argv_source_interpolation(manifest, unit.source);\n");
    out.push_str(
        "    let blake3_arg = Arg::Interpolation { tree: blake3, subpath: p\"libblake3.rlib\" };\n",
    );
    for id in &blake3_transitive_libs {
        let unit = graph
            .units
            .iter()
            .enumerate()
            .find_map(|(cargo_index, unit)| {
                ids.get(&cargo_index)
                    .copied()
                    .filter(|candidate| candidate == id)
                    .map(|_| unit)
            })
            .ok_or_else(|| format!("missing cargo unit for vix id {id}"))?;
        let crate_name = unit.target.name.replace('-', "_");
        out.push_str(&format!(
            "    let dep_{crate_name} = solution_unit_built(Target::host(), source, index, result, targets, {}, {id}, \"link\");\n",
            vix_string(host)
        ));
        out.push_str(&format!(
            "    let dep_{crate_name}_arg = Arg::Interpolation {{ tree: dep_{crate_name}, subpath: p\"lib{crate_name}.rlib\" }};\n"
        ));
    }
    out.push_str("    rustc! {\n");
    out.push_str("        --crate-name {unit.name}\n");
    out.push_str("        --edition {edition}\n");
    out.push_str("        --crate-type lib\n");
    out.push_str("        {profile_args}\n");
    out.push_str("        --emit=metadata={unit.metadata},link={unit.link}\n");
    out.push_str("        -L dependency={deps}\n");
    out.push_str("        -L dependency={blake3}\n");
    for id in &blake3_transitive_libs {
        let unit = graph
            .units
            .iter()
            .enumerate()
            .find_map(|(cargo_index, unit)| {
                ids.get(&cargo_index)
                    .copied()
                    .filter(|candidate| candidate == id)
                    .map(|_| unit)
            })
            .ok_or_else(|| format!("missing cargo unit for vix id {id}"))?;
        let crate_name = unit.target.name.replace('-', "_");
        out.push_str(&format!("        -L dependency={{dep_{crate_name}}}\n"));
    }
    out.push_str("        --extern blake3={blake3_arg}\n");
    for id in &blake3_transitive_libs {
        let unit = graph
            .units
            .iter()
            .enumerate()
            .find_map(|(cargo_index, unit)| {
                ids.get(&cargo_index)
                    .copied()
                    .filter(|candidate| candidate == id)
                    .map(|_| unit)
            })
            .ok_or_else(|| format!("missing cargo unit for vix id {id}"))?;
        let crate_name = unit.target.name.replace('-', "_");
        out.push_str(&format!(
            "        --extern {crate_name}={{dep_{crate_name}_arg}}\n"
        ));
    }
    out.push_str("        --env CARGO_MANIFEST_DIR={manifest}\n");
    out.push_str("        {source_arg}\n");
    out.push_str("    }\n");
    out.push_str("}\n\n");
    out.push_str("pub fn taxon_blake3_link(source: Tree) -> Tree {\n");
    out.push_str("    let index = taxon_index();\n");
    out.push_str("    let result = solve(index, taxon_problem(), ");
    out.push_str(&vix_string(host));
    out.push_str(");\n");
    out.push_str(
        "    solution_unit_built(Target::host(), source, index, result, taxon_targets(), ",
    );
    out.push_str(&vix_string(host));
    out.push_str(", ");
    out.push_str(&blake3_lib.to_string());
    out.push_str(", \"link\")\n");
    out.push_str("}\n\n");
    out.push_str("pub fn taxon_root_selected_names() -> String {\n");
    out.push_str("    solve_selected_names_text(taxon_index(), taxon_problem(), ");
    out.push_str(&vix_string(host));
    out.push_str(")\n");
    out.push_str("}\n\n");
    out.push_str("pub fn taxon_blake3_cfgs(source: Tree) -> String {\n");
    out.push_str("    let run = taxon_blake3_build_script_run(source);\n");
    out.push_str("    build_script_rustc_cfgs(run).join(\" \")\n");
    out.push_str("}\n\n");
    out.push_str("fn taxon_dep_names(index: Index, deps: [Int], out: [String]) -> [String] {\n");
    out.push_str("    match deps.len() == 0 {\n");
    out.push_str("        true => out,\n");
    out.push_str("        false => match deps.pop() {\n");
    out.push_str("            popped => taxon_dep_names(index, popped.1, out.push(index.names.get(popped.0).unwrap())),\n");
    out.push_str("        },\n");
    out.push_str("    }\n");
    out.push_str("}\n\n");
    out.push_str("pub fn taxon_root_deps_text(source: Tree) -> String {\n");
    out.push_str("    let index = taxon_index();\n");
    out.push_str("    let result = solve(index, taxon_problem(), ");
    out.push_str(&vix_string(host));
    out.push_str(");\n");
    out.push_str("    let unit = solution_unit(index, result, taxon_targets(), source, ");
    out.push_str(&vix_string(host));
    out.push_str(", ");
    out.push_str(&root.to_string());
    out.push_str(");\n");
    out.push_str("    taxon_dep_names(index, unit.deps, []).join(\"\\n\")\n");
    out.push_str("}\n\n");
    out.push_str("pub fn taxon_blake3_build_script_run(source: Tree) -> Tree {\n");
    out.push_str("    let index = taxon_index();\n");
    out.push_str("    let result = solve(index, taxon_blake3_build_script_problem(), \"host\");\n");
    out.push_str(&format!(
        "    let cc = solution_unit_built(Target::host(), source, index, result, taxon_targets(), {}, {cc_unit}, \"link\");\n",
        vix_string(host)
    ));
    out.push_str(&format!(
        "    let cc_unit = solution_unit(index, result, taxon_targets(), source, {}, {cc_unit});\n",
        vix_string(host)
    ));
    out.push_str(&format!(
        "    let cc_deps = solution_dependency_tree(Target::host(), source, index, result, taxon_targets(), {}, cc_unit, \"link\");\n",
        vix_string(host)
    ));
    out.push_str("    let rustc = Rustc::acquire(Target::host());\n");
    out.push_str("    let manifest = source / p\"registry/blake3-1.8.5\";\n");
    out.push_str("    let edition = \"2024\";\n");
    out.push_str("    let cc_arg = Arg::Interpolation { tree: cc, subpath: p\"libcc.rlib\" };\n");
    out.push_str("    let source_arg = Arg::Interpolation { tree: source, subpath: p\"registry/blake3-1.8.5/build.rs\" };\n");
    out.push_str("    let binary = rustc! {\n");
    out.push_str("        --crate-name build_script_build\n");
    out.push_str("        --edition {edition}\n");
    out.push_str("        --crate-type bin\n");
    out.push_str("        --emit=link=build_script\n");
    out.push_str("        -L dependency={cc_deps}\n");
    out.push_str("        --extern cc={cc_arg}\n");
    out.push_str("        --env CARGO_MANIFEST_DIR={manifest}\n");
    out.push_str("        {source_arg}\n");
    out.push_str("    };\n");
    out.push_str("    let unit = solution_unit(index, result, taxon_targets(), source, ");
    out.push_str(&vix_string(host));
    out.push_str(", ");
    out.push_str(&blake3_build.to_string());
    out.push_str(");\n");
    out.push_str("    solution_build_script_run(source, ");
    out.push_str(&vix_string(host));
    out.push_str(", unit, binary)\n");
    out.push_str("}\n");
    Ok(out)
}

fn push_taxon_clause(
    out: &mut String,
    clause: &mut usize,
    guard: &mut usize,
    parent: usize,
    dep: usize,
    req: &str,
    tag: &str,
) {
    out.push_str(&format!(
        "    let guard_clause_ids = guard_clause_ids.insert({guard}, {clause});\n"
    ));
    out.push_str(&format!(
        "    let guard_tags = guard_tags.insert({guard}, \"in_graph\");\n"
    ));
    out.push_str(&format!(
        "    let guard_kinds = guard_kinds.insert({guard}, 0);\n"
    ));
    out.push_str(&format!(
        "    let guard_pkgs = guard_pkgs.insert({guard}, {parent});\n"
    ));
    out.push_str(&format!(
        "    let guard_features = guard_features.insert({guard}, 0);\n"
    ));
    out.push_str(&format!(
        "    let consequent_tags = consequent_tags.insert({clause}, {tag:?});\n"
    ));
    out.push_str(&format!(
        "    let consequent_pkgs = consequent_pkgs.insert({clause}, {dep});\n"
    ));
    out.push_str(&format!(
        "    let consequent_version_sets = consequent_version_sets.insert({clause}, VersionSet::from_req({}));\n",
        vix_string(req)
    ));
    out.push_str(&format!(
        "    let consequent_features = consequent_features.insert({clause}, 0);\n"
    ));
    out.push_str(&format!(
        "    let gate_kinds = gate_kinds.insert({clause}, \"normal\");\n"
    ));
    *clause += 1;
    *guard += 1;
}

fn taxon_included_unit_ids(graph: &CargoUnitGraph) -> BTreeMap<usize, usize> {
    graph
        .units
        .iter()
        .enumerate()
        .filter(|(_, unit)| unit.mode != "run-custom-build")
        .enumerate()
        .map(|(id, (cargo_index, _))| (cargo_index, id))
        .collect()
}

fn taxon_transitive_lib_dependency_ids(
    graph: &CargoUnitGraph,
    ids: &BTreeMap<usize, usize>,
    root: usize,
) -> Result<Vec<usize>, String> {
    fn walk(graph: &CargoUnitGraph, index: usize, seen: &mut BTreeSet<usize>) {
        for dep in &graph.units[index].dependencies {
            let unit = &graph.units[dep.index];
            if unit.mode == "run-custom-build"
                || unit.target.kind.iter().any(|kind| kind == "custom-build")
            {
                continue;
            }
            if seen.insert(dep.index) {
                walk(graph, dep.index, seen);
            }
        }
    }

    let mut cargo_ids = BTreeSet::new();
    walk(graph, root, &mut cargo_ids);
    let mut vix_ids = Vec::new();
    for cargo_id in cargo_ids {
        let unit = &graph.units[cargo_id];
        if !unit.target.kind.iter().any(|kind| kind == "lib") {
            continue;
        }
        let vix_id = ids
            .get(&cargo_id)
            .copied()
            .ok_or_else(|| format!("missing vix id for cargo unit {cargo_id}"))?;
        vix_ids.push(vix_id);
    }
    vix_ids.sort_unstable();
    Ok(vix_ids)
}

fn taxon_run_custom_build_map(graph: &CargoUnitGraph) -> BTreeMap<usize, usize> {
    let mut out = BTreeMap::new();
    for (run_index, run_unit) in graph.units.iter().enumerate() {
        if run_unit.mode != "run-custom-build" {
            continue;
        }
        if let Some((build_index, _)) = graph.units.iter().enumerate().find(|(_, unit)| {
            unit.pkg_id == run_unit.pkg_id
                && unit.mode == "build"
                && unit.target.kind.iter().any(|kind| kind == "custom-build")
        }) {
            out.insert(run_index, build_index);
        }
    }
    out
}

fn taxon_demo_source_tree(graph: &CargoUnitGraph) -> Result<Tree, String> {
    let mut entries = BTreeMap::new();
    let mut blobs = BTreeMap::new();
    entries.insert(
        "Cargo.toml".to_owned(),
        fs::read_to_string(workspace_root().join("Cargo.toml")).map_err(|err| err.to_string())?,
    );

    let mut copied = BTreeSet::new();
    for unit in graph
        .units
        .iter()
        .filter(|unit| unit.mode != "run-custom-build")
    {
        let physical = taxon_physical_unit_root(unit)?;
        let logical = taxon_logical_unit_root(unit)?;
        if !copied.insert(logical.clone()) {
            continue;
        }
        copy_tree_into_vix_tree(&physical, &logical, &mut entries, &mut blobs)?;
    }
    Ok(Tree { entries, blobs })
}

fn facet_core_demo_source_tree(graph: &CargoUnitGraph) -> Result<Tree, String> {
    let mut entries = BTreeMap::new();
    let mut blobs = BTreeMap::new();
    entries.insert(
        "Cargo.toml".to_owned(),
        fs::read_to_string(workspace_root().join("Cargo.toml")).map_err(|err| err.to_string())?,
    );

    let mut copied = BTreeSet::new();
    for unit in graph
        .units
        .iter()
        .filter(|unit| unit.mode != "run-custom-build")
    {
        let physical = taxon_physical_unit_root(unit)?;
        let logical = taxon_logical_unit_root(unit)?;
        if !copied.insert(logical.clone()) {
            continue;
        }
        copy_tree_into_vix_tree(&physical, &logical, &mut entries, &mut blobs)?;
    }
    Ok(Tree { entries, blobs })
}

fn facet_demo_source_tree(graph: &CargoUnitGraph) -> Result<Tree, String> {
    facet_core_demo_source_tree(graph)
}

fn assert_taxon_source_tree_has_blake3_build_main(source: &Tree) -> Result<(), String> {
    let path = "registry/blake3-1.8.5/build.rs";
    let text = source
        .entries
        .get(path)
        .ok_or_else(|| format!("taxon source tree missing {path}"))?;
    assert_build_main_text(path, text, "taxon-blake3-build-source-head.txt")
}

fn assert_source_tree_has_build_main(
    source: &Tree,
    path: &str,
    artifact: &str,
) -> Result<(), String> {
    let text = source
        .entries
        .get(path)
        .ok_or_else(|| format!("source tree missing {path}"))?;
    assert_build_main_text(path, text, artifact)
}

fn assert_build_main_text(path: &str, text: &str, artifact: &str) -> Result<(), String> {
    write_tier_a_artifact(artifact, &text[..text.len().min(512)])?;
    if text.contains("fn main()") {
        Ok(())
    } else {
        Err(format!(
            "source tree {path} did not contain fn main(); first bytes:\n{}",
            &text[..text.len().min(512)]
        ))
    }
}

fn assert_projected_blake3_build_main(machine: &mut Machine, handle: i64) -> Result<(), String> {
    let bytes = tree_file_bytes(machine, handle, "build.rs")?;
    let text = String::from_utf8(bytes).map_err(|err| err.to_string())?;
    write_tier_a_artifact(
        "taxon-blake3-build-projected-head.txt",
        &text[..text.len().min(512)],
    )?;
    if text.contains("fn main()") {
        Ok(())
    } else {
        Err(format!(
            "projected blake3 build.rs did not contain fn main(); first bytes:\n{}",
            &text[..text.len().min(512)]
        ))
    }
}

fn copy_tree_into_vix_tree(
    physical: &Path,
    logical: &str,
    entries: &mut BTreeMap<String, String>,
    blobs: &mut BTreeMap<String, Vec<u8>>,
) -> Result<(), String> {
    for entry in
        fs::read_dir(physical).map_err(|err| format!("read {}: {err}", physical.display()))?
    {
        let entry = entry.map_err(|err| err.to_string())?;
        let path = entry.path();
        let name = entry
            .file_name()
            .into_string()
            .map_err(|name| format!("non-utf8 path component {name:?}"))?;
        let logical = format!("{}/{}", logical.trim_end_matches('/'), name);
        if path.is_dir() {
            copy_tree_into_vix_tree(&path, &logical, entries, blobs)?;
        } else if path.is_file() {
            let bytes = fs::read(&path).map_err(|err| format!("read {}: {err}", path.display()))?;
            match String::from_utf8(bytes) {
                Ok(text) => {
                    entries.insert(logical, text);
                }
                Err(err) => {
                    blobs.insert(logical, err.into_bytes());
                }
            }
        }
    }
    Ok(())
}

fn taxon_physical_unit_root(unit: &CargoUnit) -> Result<PathBuf, String> {
    let src = Path::new(&unit.target.src_path);
    if src.ends_with("build.rs") {
        return src
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| format!("source had no parent: {}", unit.target.src_path));
    }
    if src.ends_with("src/lib.rs") {
        return src
            .parent()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .ok_or_else(|| format!("source had no crate root: {}", unit.target.src_path));
    }
    Err(format!(
        "unsupported taxon unit source {}",
        unit.target.src_path
    ))
}

fn taxon_logical_unit_root(unit: &CargoUnit) -> Result<String, String> {
    let physical = taxon_physical_unit_root(unit)?;
    let root = workspace_root();
    if let Ok(relative) = physical.strip_prefix(&root) {
        return relative
            .to_str()
            .map(str::to_owned)
            .ok_or_else(|| format!("non-utf8 workspace path {}", physical.display()));
    }
    let package = taxon_pkg_name(&unit.pkg_id)?;
    let version = taxon_pkg_version(&unit.pkg_id)?;
    Ok(format!("registry/{package}-{version}"))
}

fn taxon_unit_source_suffix(unit: &CargoUnit) -> Result<String, String> {
    let root = taxon_physical_unit_root(unit)?;
    let source = Path::new(&unit.target.src_path)
        .strip_prefix(&root)
        .map_err(|err| err.to_string())?;
    source
        .to_str()
        .map(|value| value.replace('\\', "/"))
        .ok_or_else(|| format!("non-utf8 source path {}", unit.target.src_path))
}

fn taxon_unit_kind(unit: &CargoUnit) -> String {
    if unit.target.kind.iter().any(|kind| kind == "custom-build") {
        "build-script".to_owned()
    } else if unit.target.kind.iter().any(|kind| kind == "proc-macro") {
        "proc-macro".to_owned()
    } else if unit.target.kind.iter().any(|kind| kind == "lib") && unit.profile.debuginfo == 0 {
        "build-dependency".to_owned()
    } else {
        "lib".to_owned()
    }
}

fn taxon_unit_name(unit: &CargoUnit, index: usize) -> String {
    if unit.target.kind.iter().any(|kind| kind == "custom-build") {
        format!(
            "{}_build_script_{index}",
            taxon_pkg_name(&unit.pkg_id).unwrap_or_default()
        )
    } else {
        unit.target.name.clone()
    }
}

fn taxon_pkg_name(pkg_id: &str) -> Result<String, String> {
    let tail = pkg_id
        .rsplit('#')
        .next()
        .ok_or_else(|| format!("pkg id had no #: {pkg_id}"))?;
    Ok(tail
        .split('@')
        .next()
        .ok_or_else(|| format!("pkg id had no package name: {pkg_id}"))?
        .to_owned())
}

fn taxon_pkg_version(pkg_id: &str) -> Result<String, String> {
    let tail = pkg_id
        .rsplit('#')
        .next()
        .ok_or_else(|| format!("pkg id had no #: {pkg_id}"))?;
    match tail.split_once('@') {
        Some((_, version)) => Ok(version.to_owned()),
        None => Ok(tail.to_owned()),
    }
}

fn vix_string(value: &str) -> String {
    format!("{value:?}")
}

fn facet_proc_macro_artifact_file(host: &str, crate_name: &str) -> String {
    if host.contains("windows") {
        format!("{crate_name}.dll")
    } else if host.contains("apple-darwin") {
        format!("lib{crate_name}.dylib")
    } else {
        format!("lib{crate_name}.so")
    }
}

fn host_triple() -> Result<String, String> {
    let output = Command::new("rustc")
        .arg("-vV")
        .output()
        .map_err(|err| err.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "rustc -vV exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let stdout = String::from_utf8(output.stdout).map_err(|err| err.to_string())?;
    stdout
        .lines()
        .find_map(|line| line.strip_prefix("host: ").map(str::to_owned))
        .ok_or_else(|| format!("rustc -vV did not print host triple:\n{stdout}"))
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
