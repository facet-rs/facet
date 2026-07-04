#![cfg(all(feature = "real-process", not(target_arch = "wasm32")))]

use std::sync::Arc;

use vix::exec::{ExecEvent, Tree};
use vix::machine::{DriveEvent, Machine, MachineArg};
use vix::real_process::RealProcessBackend;

const SOURCE: &str = r#"
use vix::{Tree, Path, Target};
use caps::Cc;

fn get_cc(target: Target) -> Cc {
    Cc::acquire(target)
}

fn build_a(cc: Cc, src: Tree, unit: Path) -> Tree {
    cc! { -O2 -Wall -I {src} -c {src / unit} -o {unit.with_ext("o")} }
}

fn build_b(cc: Cc, src: Tree, unit: Path) -> Tree {
    cc! { -Wall -O2 -I {src} -c {src / unit} -o {unit.with_ext("o")} }
}

pub fn object_kind(cc: Cc, src: Tree, unit: Path) -> String {
    elf(build_a(cc, src, unit)).kind
}
"#;

#[test]
fn real_process_cc_runs_through_machine_exec_cache() -> Result<(), String> {
    if !host_cc_available() {
        return Ok(());
    }

    let backend = Arc::new(RealProcessBackend::new());
    let mut machine = Machine::load(SOURCE)?.with_exec_backend(backend);
    let target = machine.linux_target_handle();
    let cc = machine.demand_i64("get_cc", vec![target])?;
    let unit = machine
        .intern_arg("Path", MachineArg::Path("hello.c".to_string()))?
        .0;
    let source_v1 = source_tree("original README");
    let source_v2 = source_tree("edited README that is not a declared input");
    let tree_v1 = machine.intern_arg("Tree", MachineArg::Tree(source_v1))?.0;
    let tree_v2 = machine.intern_arg("Tree", MachineArg::Tree(source_v2))?.0;

    machine.clear_trace();
    let first = machine.demand_i64("build_a", vec![cc, tree_v1, unit])?;
    let first_object = tree_bytes(&mut machine, first, "hello.o")?;
    assert_run_serving(&machine, |event| matches!(event, ExecEvent::Ran));
    assert_object_magic(&first_object)?;

    machine.clear_trace();
    let warm = machine.demand_i64("build_b", vec![cc, tree_v1, unit])?;
    assert_eq!(tree_bytes(&mut machine, warm, "hello.o")?, first_object);
    assert_run_serving(&machine, |event| matches!(event, ExecEvent::Tier1Hit));

    machine.clear_trace();
    let cutoff = machine.demand_i64("build_a", vec![cc, tree_v2, unit])?;
    assert_eq!(tree_bytes(&mut machine, cutoff, "hello.o")?, first_object);
    assert_run_serving(&machine, |event| {
        matches!(event, ExecEvent::Tier2Cutoff { verified: 2 })
    });

    #[cfg(target_os = "linux")]
    {
        let kind = machine.call(
            "object_kind",
            &[
                vix::machine::NamedArg {
                    name: "cc".to_string(),
                    value: MachineArg::Word(cc),
                },
                vix::machine::NamedArg {
                    name: "src".to_string(),
                    value: MachineArg::Word(tree_v1),
                },
                vix::machine::NamedArg {
                    name: "unit".to_string(),
                    value: MachineArg::Word(unit),
                },
            ],
        )?;
        let rendered = machine.render_value("String", kind.0)?;
        assert!(matches!(
            rendered,
            vix::machine::RenderedValue::String { value } if value == "rel"
        ));
    }

    Ok(())
}

fn host_cc_available() -> bool {
    std::process::Command::new("cc")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

fn source_tree(readme: &str) -> Tree {
    Tree::of(&[
        (
            "hello.c",
            "int vix_real_process_probe(void) { return 42; }\n",
        ),
        ("README", readme),
    ])
}

fn assert_run_serving(machine: &Machine, matches_event: impl Fn(&ExecEvent) -> bool) {
    assert!(
        machine.trace().iter().any(|event| matches!(
            event,
            DriveEvent::RunCompleted {
                command_name,
                serving,
                ..
            } if command_name == "cc" && matches_event(serving)
        )),
        "{:?}",
        machine.trace()
    );
}

fn tree_bytes(machine: &mut Machine, handle: i64, path: &str) -> Result<Vec<u8>, String> {
    if let Some(bytes) = machine.tree_blob_entries(handle)?.remove(path) {
        return Ok(bytes);
    }
    machine
        .tree_entries(handle)?
        .remove(path)
        .map(String::into_bytes)
        .ok_or_else(|| format!("missing `{path}` in real-process output tree"))
}

fn assert_object_magic(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() < 4 {
        return Err(format!("object file too short: {} bytes", bytes.len()));
    }
    #[cfg(target_os = "linux")]
    if bytes.starts_with(b"\x7fELF") {
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    if matches!(
        &bytes[..4],
        [0xcf, 0xfa, 0xed, 0xfe]
            | [0xfe, 0xed, 0xfa, 0xcf]
            | [0xce, 0xfa, 0xed, 0xfe]
            | [0xfe, 0xed, 0xfa, 0xce]
    ) {
        return Ok(());
    }
    Err(format!("unrecognized object magic: {:02x?}", &bytes[..4]))
}
