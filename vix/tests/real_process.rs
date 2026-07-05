#![cfg(all(feature = "real-process", not(target_arch = "wasm32")))]

use std::collections::{BTreeMap, BTreeSet};
use std::process::Command;
use std::sync::Arc;

use vix::exec::{ExecEvent, Tree};
use vix::fetch::FakeFetchBackend;
use vix::machine::{DriveEvent, Machine, MachineArg};
use vix::real_process::RealProcessBackend;

const LUA_SOURCE: &str = include_str!("../../playgrounds/snark/src/bundled/vix/samples/lua.vix");
const LUA_URL: &str = "https://www.lua.org/ftp/lua-5.4.8.tar.gz";
const LUA_ARCHIVE_BYTES: &[u8] = b"lua-5.4.8 fixture archive";

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

#[test]
fn fake_and_real_lua_exec_substrates_emit_same_normalized_trace() -> Result<(), String> {
    if !host_cc_available() || !host_command_available("ar") {
        return Ok(());
    }

    let mut fake = Machine::load(LUA_SOURCE)?.with_fetch_backend(lua_build_fetch_backend());
    let fake_target = fake.linux_target_handle();
    let fake_lua = fake.demand_i64("lua", vec![fake_target])?;
    let fake_lua_bytes = tree_bytes(&mut fake, fake_lua, "lua")?;
    if !fake_lua_bytes.starts_with(b"obj(") {
        return Err(format!(
            "fake lua fixture produced unexpected bytes: {:?}",
            String::from_utf8_lossy(&fake_lua_bytes)
        ));
    }
    let fake_trace = normalize_exec_substrate_trace(fake.trace());

    let backend = Arc::new(RealProcessBackend::new());
    let mut real = Machine::load(LUA_SOURCE)?
        .with_fetch_backend(lua_build_fetch_backend())
        .with_exec_backend(backend);
    let real_target = real.linux_target_handle();
    let real_lua = real.demand_i64("lua", vec![real_target])?;
    let real_lua_bytes = tree_bytes(&mut real, real_lua, "lua")?;
    if real_lua_bytes.is_empty() {
        return Err("real lua fixture produced an empty executable".to_string());
    }
    let real_trace = normalize_exec_substrate_trace(real.trace());

    assert_eq!(fake_trace, real_trace);

    Ok(())
}

fn host_cc_available() -> bool {
    Command::new("cc")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

fn host_command_available(name: &str) -> bool {
    Command::new(name).output().is_ok()
}

fn lua_build_fetch_backend() -> FakeFetchBackend {
    FakeFetchBackend::new().with_archive(
        LUA_URL,
        LUA_ARCHIVE_BYTES,
        Tree::of(&[
            ("lua-5.4.8/src/lua.h", "int lua_api(void);\n"),
            (
                "lua-5.4.8/src/lapi.c",
                "#include \"lua.h\"\nint lua_api(void) { return 7; }\n",
            ),
            (
                "lua-5.4.8/src/lauxlib.c",
                "#include \"lua.h\"\nint lua_aux(void) { return lua_api(); }\n",
            ),
            (
                "lua-5.4.8/src/lua.c",
                "#include \"lua.h\"\nint main(void) { return lua_api() == 7 ? 0 : 1; }\n",
            ),
            (
                "lua-5.4.8/src/luac.c",
                "#include \"lua.h\"\nint luac_main(void) { return lua_api(); }\n",
            ),
        ]),
    )
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

// Normalization rules for the fake-real exec-substrate differential:
// - drop all timestamps;
// - collect RunRequested/RunStarted/RunCompleted by run id, preserving each
//   run's own event sequence;
// - sort the run records by their canonical request shape, so sibling overlap
//   and cross-run interleaving differences do not affect equality;
// - remove run ids after grouping;
// - compare completed output path sets, not bytes, because fake cc/ar emit
//   modeled strings while real cc/ar emit host object/archive/executable bytes;
// - compare serving as a class, so tier variants remain semantic while payload
//   counters can differ across substrate read-set granularity;
// - trim a trailing completion for runs whose output is later consumed as an
//   explicit single-file mounted input: the fake backend has computed that file
//   but does not log completion for file projection, while real-process must
//   flush to materialize host bytes for the consumer process.
fn normalize_exec_substrate_trace(trace: &[DriveEvent]) -> NormalizedTrace {
    let mut events = Vec::new();
    let mut runs: BTreeMap<u64, Vec<NormalizedRunEvent>> = BTreeMap::new();

    for event in trace {
        match event {
            DriveEvent::Demanded { fn_hash } => {
                events.push(NormalizedEvent::Demanded { fn_hash: *fn_hash });
            }
            DriveEvent::MemoHit { fn_hash } => {
                events.push(NormalizedEvent::MemoHit { fn_hash: *fn_hash });
            }
            DriveEvent::MemoProjectionHit { fn_hash, verified } => {
                events.push(NormalizedEvent::MemoProjectionHit {
                    fn_hash: *fn_hash,
                    verified: *verified,
                });
            }
            DriveEvent::MemoSemanticHit { fn_hash, verified } => {
                events.push(NormalizedEvent::MemoSemanticHit {
                    fn_hash: *fn_hash,
                    verified: *verified,
                });
            }
            DriveEvent::Spawned { fn_hash } => {
                events.push(NormalizedEvent::Spawned { fn_hash: *fn_hash });
            }
            DriveEvent::ParkedOn { fn_hash } => {
                events.push(NormalizedEvent::ParkedOn { fn_hash: *fn_hash });
            }
            DriveEvent::Completed { fn_hash } => {
                events.push(NormalizedEvent::Completed { fn_hash: *fn_hash });
            }
            DriveEvent::SpawnedInvocation { fn_hash, key_hash } => {
                events.push(NormalizedEvent::SpawnedInvocation {
                    fn_hash: *fn_hash,
                    key_hash: *key_hash,
                });
            }
            DriveEvent::StoreAlloc {
                schema_ref,
                deduped,
            } => {
                events.push(NormalizedEvent::StoreAlloc {
                    schema_ref: *schema_ref,
                    deduped: *deduped,
                });
            }
            DriveEvent::RunRequested {
                command,
                output,
                run_id,
                command_name,
                argv,
                describe,
                span,
                ..
            } => runs
                .entry(*run_id)
                .or_default()
                .push(NormalizedRunEvent::Requested {
                    command: *command,
                    output: *output,
                    command_name: command_name.clone(),
                    argv: argv.clone(),
                    describe: describe.clone(),
                    span: *span,
                }),
            DriveEvent::RunStarted {
                command,
                output,
                run_id,
                command_name,
                ..
            } => runs
                .entry(*run_id)
                .or_default()
                .push(NormalizedRunEvent::Started {
                    command: *command,
                    output: *output,
                    command_name: command_name.clone(),
                }),
            DriveEvent::RunCompleted {
                command,
                output,
                run_id,
                command_name,
                serving,
                outputs,
                ..
            } => {
                let mut output_paths = outputs
                    .iter()
                    .map(|(path, _)| path.clone())
                    .collect::<Vec<_>>();
                output_paths.sort();
                runs.entry(*run_id)
                    .or_default()
                    .push(NormalizedRunEvent::Completed {
                        command: *command,
                        output: *output,
                        command_name: command_name.clone(),
                        serving: ServingClass::from(serving),
                        output_paths,
                    });
            }
            DriveEvent::Observation {
                key,
                replayed,
                key_text,
                ..
            } => events.push(NormalizedEvent::Observation {
                key: *key,
                replayed: *replayed,
                key_text: key_text.clone(),
            }),
            DriveEvent::ArtifactProbe {
                format,
                projection,
                input,
                cache_hit,
                ..
            } => events.push(NormalizedEvent::ArtifactProbe {
                format: format.clone(),
                projection: projection.clone(),
                input: *input,
                cache_hit: *cache_hit,
            }),
        }
    }

    let mut runs = runs
        .into_values()
        .map(|events| NormalizedRun { events })
        .collect::<Vec<_>>();
    trim_single_file_materialization_completions(&mut runs);
    runs.sort();
    NormalizedTrace { events, runs }
}

fn trim_single_file_materialization_completions(runs: &mut [NormalizedRun]) {
    let consumed_files = runs
        .iter()
        .flat_map(|run| run.events.iter())
        .filter_map(|event| match event {
            NormalizedRunEvent::Requested { argv, .. } => Some(argv),
            _ => None,
        })
        .flat_map(|argv| argv.iter().filter_map(|arg| mounted_file_basename(arg)))
        .collect::<BTreeSet<_>>();

    for run in runs {
        let should_trim = run
            .events
            .iter()
            .find_map(|event| match event {
                NormalizedRunEvent::Requested {
                    command_name, argv, ..
                } => requested_output_path(command_name, argv),
                _ => None,
            })
            .and_then(|path| path.rsplit('/').next().map(str::to_string))
            .is_some_and(|basename| consumed_files.contains(&basename));
        if should_trim
            && matches!(
                run.events.last(),
                Some(NormalizedRunEvent::Completed {
                    serving: ServingClass::Ran,
                    ..
                })
            )
        {
            run.events.pop();
        }
    }
}

fn mounted_file_basename(arg: &str) -> Option<String> {
    let rest = arg.strip_prefix("/m/")?;
    let (_, path) = rest.split_once('/')?;
    path.rsplit('/').next().map(str::to_string)
}

fn requested_output_path(command_name: &str, argv: &[String]) -> Option<String> {
    match command_name {
        "cc" => argv
            .windows(2)
            .find_map(|pair| (pair[0] == "-o").then(|| pair[1].clone())),
        "ar" => argv
            .iter()
            .find(|arg| arg.as_str() != "rcs" && !arg.starts_with("/m/"))
            .cloned(),
        _ => None,
    }
}

#[derive(Debug, PartialEq, Eq)]
struct NormalizedTrace {
    events: Vec<NormalizedEvent>,
    runs: Vec<NormalizedRun>,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct NormalizedRun {
    events: Vec<NormalizedRunEvent>,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum NormalizedRunEvent {
    Requested {
        command: u64,
        output: u64,
        command_name: String,
        argv: Vec<String>,
        describe: Vec<String>,
        span: Option<(u32, u32)>,
    },
    Started {
        command: u64,
        output: u64,
        command_name: String,
    },
    Completed {
        command: u64,
        output: u64,
        command_name: String,
        serving: ServingClass,
        output_paths: Vec<String>,
    },
}

#[derive(Debug, PartialEq, Eq)]
enum NormalizedEvent {
    Demanded {
        fn_hash: u64,
    },
    MemoHit {
        fn_hash: u64,
    },
    MemoProjectionHit {
        fn_hash: u64,
        verified: usize,
    },
    MemoSemanticHit {
        fn_hash: u64,
        verified: usize,
    },
    Spawned {
        fn_hash: u64,
    },
    ParkedOn {
        fn_hash: u64,
    },
    Completed {
        fn_hash: u64,
    },
    SpawnedInvocation {
        fn_hash: u64,
        key_hash: u64,
    },
    StoreAlloc {
        schema_ref: u64,
        deduped: bool,
    },
    Observation {
        key: u64,
        replayed: bool,
        key_text: String,
    },
    ArtifactProbe {
        format: String,
        projection: String,
        input: u64,
        cache_hit: bool,
    },
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum ServingClass {
    Ran,
    Tier1Hit,
    Tier2Cutoff,
    Joined,
}

impl From<&ExecEvent> for ServingClass {
    fn from(event: &ExecEvent) -> Self {
        match event {
            ExecEvent::Ran => ServingClass::Ran,
            ExecEvent::Tier1Hit => ServingClass::Tier1Hit,
            ExecEvent::Tier2Cutoff { .. } => ServingClass::Tier2Cutoff,
            ExecEvent::Joined => ServingClass::Joined,
        }
    }
}
