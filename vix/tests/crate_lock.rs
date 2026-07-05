use std::collections::BTreeSet;

use vix::exec::Tree;
use vix::machine::{DriveEvent, Machine, MachineArg, RenderedValue};

const SOURCE: &str = include_str!("../../playgrounds/snark/src/bundled/vix/samples/crate.vix");
const LOCK: &str =
    include_str!("../../playgrounds/snark/src/bundled/vix/samples/fixtures/lock_graph/Cargo.lock");
const APP_LOCK: &str = include_str!(
    "../../playgrounds/snark/src/bundled/vix/samples/fixtures/lock_graph/app/Cargo.lock"
);
const APP_MANIFEST: &str = include_str!(
    "../../playgrounds/snark/src/bundled/vix/samples/fixtures/lock_graph/app/Cargo.toml"
);
const APP_MAIN: &str = include_str!(
    "../../playgrounds/snark/src/bundled/vix/samples/fixtures/lock_graph/app/src/main.rs"
);
const ALPHA_MANIFEST: &str = include_str!(
    "../../playgrounds/snark/src/bundled/vix/samples/fixtures/lock_graph/crates/alpha_lib/Cargo.toml"
);
const ALPHA_LIB: &str = include_str!(
    "../../playgrounds/snark/src/bundled/vix/samples/fixtures/lock_graph/crates/alpha_lib/src/lib.rs"
);
const CORE_MANIFEST: &str = include_str!(
    "../../playgrounds/snark/src/bundled/vix/samples/fixtures/lock_graph/crates/core_lib/Cargo.toml"
);
const CORE_LIB: &str = include_str!(
    "../../playgrounds/snark/src/bundled/vix/samples/fixtures/lock_graph/crates/core_lib/src/lib.rs"
);
const FORMATTING_MANIFEST: &str = include_str!(
    "../../playgrounds/snark/src/bundled/vix/samples/fixtures/lock_graph/crates/formatting_lib/Cargo.toml"
);
const FORMATTING_LIB: &str = include_str!(
    "../../playgrounds/snark/src/bundled/vix/samples/fixtures/lock_graph/crates/formatting_lib/src/lib.rs"
);

#[test]
fn crate_vix_consumes_lockfile_and_builds_transitive_graph_with_fake_rustc() -> Result<(), String> {
    let mut machine = Machine::load(SOURCE)?;
    let target = machine.linux_target_handle();
    let graph = machine
        .intern_arg("Tree", MachineArg::Tree(lock_graph_tree()))?
        .0;

    let root_version = machine.demand_i64("crate_lock_root_version", vec![graph])?;
    if rendered_string(&machine, "crate_lock_root_version", root_version)? != "0.1.0" {
        return Err("lock root version did not come from Cargo.lock".into());
    }

    let alpha = machine
        .intern_arg("String", MachineArg::String("alpha_lib".to_string()))?
        .0;
    let alpha_deps = machine.demand_i64("lock_fixture_deps", vec![graph, alpha])?;
    if rendered_string(&machine, "lock_fixture_deps", alpha_deps)? != "core_lib" {
        return Err("alpha_lib dependencies did not come from Cargo.lock".into());
    }

    let checked = machine.demand_i64("crate_lock_bin_check", vec![target, graph])?;
    if !machine
        .tree_entries(checked)?
        .contains_key("mini_app.rmeta")
    {
        return Err("metadata build did not produce mini_app.rmeta".into());
    }

    let built = machine.demand_i64("crate_lock_bin", vec![target, graph])?;
    if !machine.tree_entries(built)?.contains_key("mini_app") {
        return Err("link build did not produce mini_app".into());
    }

    let extern_edges = rustc_extern_edges(&machine);
    for edge in [
        ("alpha_lib", "core_lib"),
        ("mini_app", "alpha_lib"),
        ("mini_app", "formatting_lib"),
    ] {
        if !extern_edges.contains(&edge) {
            return Err(format!(
                "missing rustc extern edge {edge:?}: {extern_edges:?}"
            ));
        }
    }
    Ok(())
}

fn rendered_string(machine: &Machine, name: &str, word: i64) -> Result<String, String> {
    match machine.render_result(name, word)? {
        RenderedValue::String { value } => Ok(value),
        other => Err(format!("{name} rendered as {other:?}, not String")),
    }
}

fn rustc_extern_edges(machine: &Machine) -> BTreeSet<(&str, &str)> {
    machine
        .trace()
        .iter()
        .filter_map(|event| match event {
            DriveEvent::RunRequested {
                command_name, argv, ..
            } if command_name == "rustc" => Some(argv.as_slice()),
            _ => None,
        })
        .filter_map(|argv| {
            let from = arg_after(argv, "--crate-name")?;
            Some(
                argv.windows(2)
                    .filter_map(move |pair| {
                        (pair[0] == "--extern")
                            .then(|| pair[1].split_once('=').map(|(name, _)| (from, name)))
                            .flatten()
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .flatten()
        .collect()
}

fn arg_after<'a>(argv: &'a [String], flag: &str) -> Option<&'a str> {
    argv.windows(2)
        .find_map(|pair| (pair[0] == flag).then_some(pair[1].as_str()))
}

fn lock_graph_tree() -> Tree {
    Tree::of(&[
        ("Cargo.lock", LOCK),
        ("app/Cargo.lock", APP_LOCK),
        ("app/Cargo.toml", APP_MANIFEST),
        ("app/src/main.rs", APP_MAIN),
        ("crates/alpha_lib/Cargo.toml", ALPHA_MANIFEST),
        ("crates/alpha_lib/src/lib.rs", ALPHA_LIB),
        ("crates/core_lib/Cargo.toml", CORE_MANIFEST),
        ("crates/core_lib/src/lib.rs", CORE_LIB),
        ("crates/formatting_lib/Cargo.toml", FORMATTING_MANIFEST),
        ("crates/formatting_lib/src/lib.rs", FORMATTING_LIB),
    ])
}
