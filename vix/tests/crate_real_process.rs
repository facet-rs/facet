#![cfg(all(feature = "real-process", not(target_arch = "wasm32")))]

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Instant;

use facet::Facet;
use vix::exec::Tree;
use vix::machine::{DriveEvent, Machine, MachineArg};
use vix::real_process::RealProcessBackend;

const SOURCE: &str = include_str!("../../playgrounds/snark/src/bundled/vix/samples/crate.vix");
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

#[test]
fn real_process_rustc_builds_two_crate_fixture_and_matches_cargo_unit_graph_oracle()
-> Result<(), String> {
    if !host_rustc_available() {
        return Ok(());
    }

    let backend = Arc::new(RealProcessBackend::new());
    let mut machine = Machine::load(SOURCE)?.with_exec_backend(backend);
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

    let backend = Arc::new(RealProcessBackend::new());
    let mut machine = Machine::load(SOURCE)?.with_exec_backend(backend);
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

fn run_binary_bytes(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let temp = tempfile::Builder::new()
        .prefix("vix-real-rustc-bin-")
        .tempdir()
        .map_err(|err| err.to_string())?;
    let bin = temp.path().join("mini_app");
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
    Ok(UnitShape {
        name: arg_after("--crate-name")?,
        crate_type: arg_after("--crate-type")?,
        edition: arg_after("--edition")?,
        source_suffix: source_suffix(source)?,
        dependencies,
    })
}

fn source_suffix(source: &str) -> Result<String, String> {
    if source.ends_with("/src/lib.rs") || source.ends_with("/lib.rs") {
        Ok("src/lib.rs".to_string())
    } else if source.ends_with("/src/main.rs") || source.ends_with("/main.rs") {
        Ok("src/main.rs".to_string())
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
    let mut shapes = graph
        .units
        .iter()
        .map(|unit| {
            let dependencies = unit
                .dependencies
                .iter()
                .map(|dep| dep.extern_crate_name.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            Ok(UnitShape {
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
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    shapes.sort();
    Ok(shapes)
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

fn cargo_unit_shapes_from_json(stdout: &str) -> Result<Vec<UnitShape>, String> {
    let graph: CargoUnitGraph = facet_json::from_str(stdout).map_err(|err| err.to_string())?;
    let mut shapes = graph
        .units
        .iter()
        .map(|unit| {
            let dependencies = unit
                .dependencies
                .iter()
                .map(|dep| dep.extern_crate_name.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            Ok(UnitShape {
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
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    shapes.sort();
    Ok(shapes)
}

#[derive(Debug, Facet)]
struct CargoUnitGraph {
    units: Vec<CargoUnit>,
}

#[derive(Debug, Facet)]
struct CargoUnit {
    target: CargoTarget,
    dependencies: Vec<CargoDependency>,
}

#[derive(Debug, Facet)]
struct CargoTarget {
    name: String,
    src_path: String,
    edition: String,
    crate_types: Vec<String>,
}

#[derive(Debug, Facet)]
struct CargoDependency {
    extern_crate_name: String,
}
