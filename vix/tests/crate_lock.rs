use std::collections::{BTreeMap, BTreeSet};

use vix::exec::Tree;
use vix::machine::{DriveEvent, Machine, MachineArg, RenderedValue};

const SOURCE: &str = include_str!("../../playgrounds/snark/src/bundled/vix/samples/crate.vix");
const RODIN_SOURCE: &str = include_str!("../../rodin/rodin.vix");
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

#[test]
fn crate_vix_consumes_lockfile_and_builds_transitive_graph_with_fake_rustc() -> Result<(), String> {
    let mut machine = crate_machine()?;
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

#[test]
fn crate_vix_models_build_script_directives_and_out_dir_with_fake_runner() -> Result<(), String> {
    let mut machine = crate_machine()?;
    let target = machine.linux_target_handle();
    let graph = machine
        .intern_arg("Tree", MachineArg::Tree(build_script_tree()))?
        .0;

    let cfg = machine.demand_i64("crate_build_script_directive_cfg", vec![graph])?;
    if rendered_string(&machine, "crate_build_script_directive_cfg", cfg)?
        != "vix_slice3_build_script"
    {
        return Err("build script cfg directive was not parsed".into());
    }

    let out_dir = machine.demand_i64("crate_build_script_out_dir", vec![graph])?;
    if machine
        .tree_entries(out_dir)?
        .get("generated.rs")
        .is_none_or(|contents| !contents.contains("vix-build-script-generated"))
    {
        return Err("build script OUT_DIR generated.rs was not harvested".into());
    }

    let built = machine.demand_i64("crate_build_script_bin", vec![target, graph])?;
    if !machine
        .tree_entries(built)?
        .contains_key("build_script_runner")
    {
        return Err("build script fake link did not produce build_script_runner".into());
    }

    let rustc_argv = machine
        .trace()
        .iter()
        .filter_map(|event| match event {
            DriveEvent::RunRequested {
                command_name, argv, ..
            } if command_name == "rustc" => Some(argv.as_slice()),
            _ => None,
        })
        .collect::<Vec<_>>();
    if !rustc_argv.iter().any(|argv| {
        argv.windows(2)
            .any(|pair| pair[0] == "--cfg" && pair[1] == "vix_slice3_build_script")
    }) {
        return Err("parent rustc did not receive the build-script cfg".into());
    }
    if !rustc_argv.iter().any(|argv| {
        argv.windows(2)
            .any(|pair| pair[0] == "--env" && pair[1].starts_with("OUT_DIR="))
    }) {
        return Err("parent rustc did not receive OUT_DIR env".into());
    }
    Ok(())
}

#[test]
fn crate_vix_parses_build_script_stdout_directives_as_pure_vix() -> Result<(), String> {
    let mut machine = crate_machine()?;
    let stdout = machine
        .intern_arg(
            "String",
            MachineArg::String(
                "cargo:rustc-cfg=has_native\n\
                 cargo::rustc-env=LIB_FOO=bar\n\
                 cargo:rustc-link-lib=static=foo\n\
                 cargo:rustc-link-search=native=/opt/foo\n\
                 cargo:rerun-if-changed=wrapper.h\n\
                 cargo:rerun-if-env-changed=FOO_SYS_ROOT\n\
                 cargo:warning=using bundled foo\n"
                    .to_string(),
            ),
        )?
        .0;

    let cfg = machine.demand_i64("crate_build_script_directive_cfg_from_stdout", vec![stdout])?;
    if rendered_string(
        &machine,
        "crate_build_script_directive_cfg_from_stdout",
        cfg,
    )? != "has_native"
    {
        return Err("rustc-cfg directive was not parsed".into());
    }

    let env = machine.demand_i64("crate_build_script_directive_env_from_stdout", vec![stdout])?;
    if rendered_string(
        &machine,
        "crate_build_script_directive_env_from_stdout",
        env,
    )? != "LIB_FOO=bar"
    {
        return Err("rustc-env directive was not parsed".into());
    }

    let links = machine.demand_i64(
        "crate_build_script_directive_links_from_stdout",
        vec![stdout],
    )?;
    if rendered_string(
        &machine,
        "crate_build_script_directive_links_from_stdout",
        links,
    )? != "static=foo|native=/opt/foo"
    {
        return Err("link directives were not recorded as unit data".into());
    }

    let rerun = machine.demand_i64(
        "crate_build_script_directive_rerun_from_stdout",
        vec![stdout],
    )?;
    if rendered_string(
        &machine,
        "crate_build_script_directive_rerun_from_stdout",
        rerun,
    )? != "wrapper.h|FOO_SYS_ROOT"
    {
        return Err("rerun-if directives were not recorded as declared-world receipts".into());
    }

    Ok(())
}

#[test]
fn crate_vix_propagates_build_script_metadata_to_dep_env_values() -> Result<(), String> {
    let mut machine = crate_machine()?;
    let stdout = machine
        .intern_arg(
            "String",
            MachineArg::String(
                "cargo:include=/opt/foo/include\n\
                 cargo::metadata=libdir=/opt/foo/lib\n"
                    .to_string(),
            ),
        )?
        .0;
    let links = machine
        .intern_arg("String", MachineArg::String("FOO".to_string()))?
        .0;

    let env = machine.demand_i64(
        "crate_build_script_dep_metadata_env_from_stdout",
        vec![stdout, links],
    )?;
    if rendered_string(
        &machine,
        "crate_build_script_dep_metadata_env_from_stdout",
        env,
    )? != "DEP_FOO_include=/opt/foo/include,DEP_FOO_libdir=/opt/foo/lib"
    {
        return Err("metadata directives did not propagate as DEP_<links>_<key>".into());
    }

    Ok(())
}

#[test]
fn crate_vix_rejects_malformed_build_script_directive_lines() -> Result<(), String> {
    let mut machine = crate_machine()?;
    let stdout = machine
        .intern_arg(
            "String",
            MachineArg::String("cargo:rustc-cfg\n".to_string()),
        )?
        .0;

    match machine.demand_i64("crate_build_script_directive_cfg_from_stdout", vec![stdout]) {
        Ok(_) => Err("malformed directive line was silently accepted".into()),
        Err(err) => {
            if err.contains("cargo:rustc-cfg") || err.contains("unwrap") {
                Ok(())
            } else {
                Err(format!(
                    "malformed directive failed with unexpected error: {err}"
                ))
            }
        }
    }
}

fn crate_machine() -> Result<Machine, String> {
    Machine::load_modules(
        "root",
        BTreeMap::from([
            ("root".to_owned(), SOURCE.to_owned()),
            ("rodin".to_owned(), RODIN_SOURCE.to_owned()),
        ]),
    )
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
