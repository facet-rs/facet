use vix::exec::ExecEvent;
use vix::machine::{DriveEvent, Machine, RenderedValue};

const SOURCE: &str = r#"
use vix::{Arg, Target, Tree};
use caps::Rustc;

fn rustc_for(target: Target) -> Rustc {
    Rustc::acquire(target)
}

fn dep(rustc: Rustc) -> Tree {
    rustc! {
        --crate-name dep
        --crate-type lib
        --emit=metadata={p"libdep.rmeta"}
    }
}

pub fn argv_only(target: Target) -> [Arg] {
    let rustc = rustc_for(target);
    [
        Arg::Str("--extern"),
        Arg::Str("dep="),
        Arg::Interpolation { tree: dep(rustc), subpath: p"libdep.rmeta" },
    ]
}

fn consumer_run(target: Target) -> Tree {
    let rustc = rustc_for(target);
    let externs = argv_only(target);
    rustc! {
        --crate-name app
        --crate-type lib
        --emit=metadata={p"app.rmeta"}
        {externs}
    }
}

fn consumer_inline_run(target: Target) -> Tree {
    let rustc = rustc_for(target);
    let dep_tree = dep(rustc);
    rustc! {
        --crate-name inline_app
        --crate-type lib
        --emit=metadata={p"inline_app.rmeta"}
        {[Arg::Str("--extern"), Arg::Str("dep="), Arg::Interpolation { tree: dep_tree, subpath: p"libdep.rmeta" }]}
    }
}

pub fn consumer_full(target: Target) -> Tree {
    consumer_run(target)
}

pub fn consumer(target: Target) -> Tree {
    consumer_run(target) / p"app.rmeta"
}

pub fn consumer_inline(target: Target) -> Tree {
    consumer_inline_run(target) / p"inline_app.rmeta"
}

pub fn build_stdout(target: Target) -> String {
    let rustc = rustc_for(target);
    let build_script_binary = rustc! {
        --crate-name build_script_build
        --crate-type bin
        --emit=link={p"build_script"}
    };
    let build_script = "build-script-runner";
    let run = build_script! {
        --executable {build_script_binary / p"build_script"}
        --stdout {p"build.stdout"}
        --out-dir {p"out"}
    };
    run.text(p"build.stdout")
}

pub fn first_directive_key(target: Target) -> String {
    build_stdout(target).before("\n").after("cargo:").before("=")
}
"#;

#[test]
fn argv_inline_arg_array_splice_interns_molten_array() -> Result<(), String> {
    let mut machine = Machine::load(SOURCE)?;
    let target = machine.linux_target_handle();

    let artifact = machine.demand_i64("consumer_inline", vec![target])?;
    assert!(
        machine
            .tree_entries(artifact)?
            .contains_key("inline_app.rmeta")
    );

    let requested = machine
        .trace()
        .iter()
        .filter_map(|event| match event {
            DriveEvent::RunRequested {
                command_name, argv, ..
            } if command_name == "rustc" && argv.iter().any(|arg| arg == "inline_app") => {
                Some(argv.clone())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(requested.len(), 1, "{requested:#?}");
    assert!(
        requested[0]
            .windows(2)
            .any(|pair| { pair[0] == "--extern" && pair[1] == "dep=/m/0/libdep.rmeta" }),
        "{requested:#?}"
    );

    Ok(())
}

#[test]
fn argv_interpolation_mount_is_lazy_until_consumer_exec_is_demanded() -> Result<(), String> {
    let mut machine = Machine::load(SOURCE)?;
    let target = machine.linux_target_handle();

    let _argv = machine.demand_i64("argv_only", vec![target])?;
    assert!(
        machine
            .trace()
            .iter()
            .all(|event| !matches!(event, DriveEvent::RunStarted { .. })),
        "argv construction must not start exec runs: {:?}",
        machine.trace()
    );

    machine.clear_trace();
    let artifact = machine.demand_i64("consumer_full", vec![target])?;
    let entries = machine.tree_entries(artifact)?;
    assert!(matches!(entries.get("app.rmeta"), Some(value) if value.starts_with("rmeta(")));

    let completed = machine
        .trace()
        .iter()
        .filter_map(|event| match event {
            DriveEvent::RunCompleted {
                command_name,
                serving,
                outputs,
                ..
            } => Some((command_name.as_str(), serving, outputs)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(completed.len(), 2, "{completed:#?}");
    assert!(
        completed.iter().all(|(command, serving, _)| {
            *command == "rustc" && matches!(serving, ExecEvent::Ran)
        })
    );
    assert!(
        completed
            .iter()
            .any(|(_, _, outputs)| outputs.iter().any(|(path, _)| path == "libdep.rmeta")),
        "producer artifact was not run: {completed:#?}"
    );
    assert!(
        completed
            .iter()
            .any(|(_, _, outputs)| outputs.iter().any(|(path, _)| path == "app.rmeta")),
        "consumer artifact was not run: {completed:#?}"
    );

    Ok(())
}

#[test]
fn argv_arg_array_splices_variable_length_externs() -> Result<(), String> {
    let mut machine = Machine::load(SOURCE)?;
    let target = machine.linux_target_handle();

    let artifact = machine.demand_i64("consumer", vec![target])?;
    assert!(machine.tree_entries(artifact)?.contains_key("app.rmeta"));

    let requested = machine
        .trace()
        .iter()
        .filter_map(|event| match event {
            DriveEvent::RunRequested {
                command_name, argv, ..
            } if command_name == "rustc" && argv.iter().any(|arg| arg == "app") => {
                Some(argv.clone())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(requested.len(), 1, "{requested:#?}");
    assert!(
        requested[0]
            .windows(2)
            .any(|pair| { pair[0] == "--extern" && pair[1] == "dep=/m/0/libdep.rmeta" }),
        "{requested:#?}"
    );

    Ok(())
}

#[test]
fn tree_text_projection_feeds_pure_vix_directive_parsing() -> Result<(), String> {
    let mut machine = Machine::load(SOURCE)?;
    let target = machine.linux_target_handle();

    let key = machine.demand_i64("first_directive_key", vec![target])?;
    let rendered = machine.render_result("first_directive_key", key)?;
    assert!(matches!(
        rendered,
        RenderedValue::String { value } if value == "rustc-cfg"
    ));

    Ok(())
}
