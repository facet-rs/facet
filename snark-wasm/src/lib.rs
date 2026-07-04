#![forbid(unsafe_code)]
//! WebAssembly bindings for Snark playgrounds.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use facet::Facet;
use wasm_bindgen::prelude::*;

/// Prepared Snark playground session for one grammar bundle.
#[wasm_bindgen]
pub struct SnarkPlaygroundSession {
    inner: snark_playground::PlaygroundSession,
}

#[wasm_bindgen]
impl SnarkPlaygroundSession {
    /// Prepare one grammar bundle for repeated parsing.
    #[wasm_bindgen(constructor)]
    pub fn new(request_json: &str) -> Result<SnarkPlaygroundSession, JsValue> {
        let inner = snark_playground::PlaygroundSession::prepare_json(request_json)
            .map_err(|message| JsValue::from_str(&message))?;
        Ok(Self { inner })
    }

    /// Parse one input with the prepared bundle and return a JSON response.
    #[wasm_bindgen(js_name = parse)]
    pub fn parse(&mut self, request_json: &str) -> String {
        self.inner.parse_json(request_json)
    }

    /// Reparse one edited input with the prepared bundle and return a JSON response.
    #[wasm_bindgen(js_name = reparse)]
    pub fn reparse(&mut self, request_json: &str) -> String {
        self.inner.parse_json(request_json)
    }
}

/// vix Ring-2 IDE bindings (symbols, references, unresolved) for the playground:
/// occurrence highlighting, go-to-definition, rename. Uses vix's own embedded
/// grammar — independent of whatever bundle the session has prepared.
#[wasm_bindgen(js_name = vixBindings)]
pub fn vix_bindings(source: &str) -> String {
    vix::ide::bindings_json(source)
}

/// vix syntax highlighting: the embedded highlights query over the embedded
/// grammar — clients need no grammar assets at all.
#[wasm_bindgen(js_name = vixHighlights)]
pub fn vix_highlights(source: &str) -> String {
    vix::ide::highlights_json(source)
}

#[derive(Facet)]
struct VixMachineRun {
    ok: bool,
    error: Option<String>,
    source_kind: String,
    fn_name: String,
    result: Option<VixMachineResult>,
    cold_trace: Vec<VixDriveEvent>,
    warm_trace: Vec<VixDriveEvent>,
    fn_hashes: Vec<HashLabel>,
    run_hashes: Vec<HashLabel>,
}

#[derive(Facet)]
struct VixMachineResult {
    schema: String,
    i64_value: Option<i64>,
    f64_value: Option<f64>,
    tree_entries: Vec<TreeEntry>,
}

#[derive(Facet)]
struct TreeEntry {
    path: String,
    contents: String,
}

#[derive(Facet)]
struct HashLabel {
    hash: String,
    label: String,
}

#[repr(u8)]
#[derive(Facet)]
#[facet(tag = "type")]
pub enum VixDriveEvent {
    Demanded {
        fn_hash: String,
    },
    MemoHit {
        fn_hash: String,
    },
    Spawned {
        fn_hash: String,
    },
    ParkedOn {
        fn_hash: String,
    },
    Completed {
        fn_hash: String,
    },
    SpawnedInvocation {
        fn_hash: String,
        key_hash: String,
    },
    StoreAlloc {
        schema_ref: String,
        deduped: bool,
    },
    RunRequested {
        command: String,
        output: String,
        run_id: u64,
        command_name: String,
        argv: Vec<String>,
        describe: Vec<String>,
        span: Option<VixSpan>,
        timestamp_us: u64,
    },
    RunStarted {
        command: String,
        output: String,
        run_id: u64,
        command_name: String,
        timestamp_us: u64,
    },
    RunCompleted {
        command: String,
        output: String,
        run_id: u64,
        command_name: String,
        serving: VixExecServing,
        outputs: Vec<RunOutput>,
        timestamp_us: u64,
    },
    Observation {
        key: String,
        replayed: bool,
        key_text: String,
        timestamp_us: u64,
    },
}

#[derive(Facet)]
pub struct VixSpan {
    pub start: u32,
    pub end: u32,
}

#[derive(Facet)]
pub struct RunOutput {
    pub path: String,
    pub hash: String,
}

#[repr(u8)]
#[derive(Facet)]
#[facet(tag = "type")]
pub enum VixExecServing {
    Tier1Hit,
    Tier2Cutoff { verified: u64 },
    Ran,
    Joined,
}

#[wasm_bindgen(js_name = runVixMachine)]
pub fn run_vix_machine(source: &str, fn_name: &str) -> String {
    let out = match run_vix_machine_inner(source, fn_name) {
        Ok(out) => out,
        Err(error) => VixMachineRun {
            ok: false,
            error: Some(error),
            source_kind: source_kind(source).to_string(),
            fn_name: fn_name.to_string(),
            result: None,
            cold_trace: Vec::new(),
            warm_trace: Vec::new(),
            fn_hashes: Vec::new(),
            run_hashes: run_hashes(source),
        },
    };
    facet_json::to_string(&out).expect("VixMachineRun serializes")
}

fn run_vix_machine_inner(source: &str, fn_name: &str) -> Result<VixMachineRun, String> {
    let mut machine = vix::machine::lower::Machine::load(source)?;
    if source_kind(source) == "lua" {
        machine = machine.with_fetch_backend(lua_fetch_backend());
    }
    let args = match fn_name {
        "selected" | "fallback" | "subtree_chain" | "lua" | "main" => {
            vec![machine.linux_target_handle()]
        }
        "demo" => Vec::new(),
        other => {
            return Err(format!(
                "run-on-machine does not know arguments for `{other}`"
            ));
        }
    };
    let source_kind = source_kind(source).to_string();
    let fn_hashes = function_hashes(&machine);
    let handle = match fn_name {
        "demo" => {
            let value = machine.demand_f64(fn_name, args.clone())?;
            let cold_trace = trace_events(machine.trace());
            machine.clear_trace();
            let _warm = machine.demand_f64(fn_name, args)?;
            let warm_trace = trace_events(machine.trace());
            return Ok(VixMachineRun {
                ok: true,
                error: None,
                source_kind,
                fn_name: fn_name.to_string(),
                result: Some(VixMachineResult {
                    schema: "Float".to_string(),
                    i64_value: None,
                    f64_value: Some(value),
                    tree_entries: Vec::new(),
                }),
                cold_trace,
                warm_trace,
                fn_hashes,
                run_hashes: run_hashes(source),
            });
        }
        _ => machine.demand_i64(fn_name, args.clone())?,
    };
    let tree_entries = machine
        .tree_entries(handle)?
        .into_iter()
        .map(|(path, contents)| TreeEntry { path, contents })
        .collect();
    let cold_trace = trace_events(machine.trace());
    machine.clear_trace();
    let warm = machine.demand_i64(fn_name, args)?;
    if warm != handle {
        return Err(format!(
            "warm demand for `{fn_name}` returned handle {warm}, expected {handle}"
        ));
    }
    let warm_trace = trace_events(machine.trace());
    Ok(VixMachineRun {
        ok: true,
        error: None,
        source_kind,
        fn_name: fn_name.to_string(),
        result: Some(VixMachineResult {
            schema: "Tree".to_string(),
            i64_value: Some(handle),
            f64_value: None,
            tree_entries,
        }),
        cold_trace,
        warm_trace,
        fn_hashes,
        run_hashes: run_hashes(source),
    })
}

fn trace_events(events: &[vix::machine::driver::DriveEvent]) -> Vec<VixDriveEvent> {
    events
        .iter()
        .map(|event| match event {
            vix::machine::driver::DriveEvent::Demanded { fn_hash } => VixDriveEvent::Demanded {
                fn_hash: hex_hash(*fn_hash),
            },
            vix::machine::driver::DriveEvent::MemoHit { fn_hash } => VixDriveEvent::MemoHit {
                fn_hash: hex_hash(*fn_hash),
            },
            vix::machine::driver::DriveEvent::Spawned { fn_hash } => VixDriveEvent::Spawned {
                fn_hash: hex_hash(*fn_hash),
            },
            vix::machine::driver::DriveEvent::ParkedOn { fn_hash } => VixDriveEvent::ParkedOn {
                fn_hash: hex_hash(*fn_hash),
            },
            vix::machine::driver::DriveEvent::Completed { fn_hash } => VixDriveEvent::Completed {
                fn_hash: hex_hash(*fn_hash),
            },
            vix::machine::driver::DriveEvent::SpawnedInvocation { fn_hash, key_hash } => {
                VixDriveEvent::SpawnedInvocation {
                    fn_hash: hex_hash(*fn_hash),
                    key_hash: hex_hash(*key_hash),
                }
            }
            vix::machine::driver::DriveEvent::StoreAlloc {
                schema_ref,
                deduped,
            } => VixDriveEvent::StoreAlloc {
                schema_ref: hex_hash(*schema_ref),
                deduped: *deduped,
            },
            vix::machine::driver::DriveEvent::RunRequested {
                command,
                output,
                run_id,
                command_name,
                argv,
                describe,
                span,
                timestamp_us,
            } => VixDriveEvent::RunRequested {
                command: hex_hash(*command),
                output: hex_hash(*output),
                run_id: *run_id,
                command_name: command_name.clone(),
                argv: argv.clone(),
                describe: describe.clone(),
                span: span.map(|(start, end)| VixSpan { start, end }),
                timestamp_us: *timestamp_us,
            },
            vix::machine::driver::DriveEvent::RunStarted {
                command,
                output,
                run_id,
                command_name,
                timestamp_us,
            } => VixDriveEvent::RunStarted {
                command: hex_hash(*command),
                output: hex_hash(*output),
                run_id: *run_id,
                command_name: command_name.clone(),
                timestamp_us: *timestamp_us,
            },
            vix::machine::driver::DriveEvent::RunCompleted {
                command,
                output,
                run_id,
                command_name,
                serving,
                outputs,
                timestamp_us,
            } => VixDriveEvent::RunCompleted {
                command: hex_hash(*command),
                output: hex_hash(*output),
                run_id: *run_id,
                command_name: command_name.clone(),
                serving: serving_event(serving),
                outputs: outputs
                    .iter()
                    .map(|(path, hash)| RunOutput {
                        path: path.clone(),
                        hash: hash.clone(),
                    })
                    .collect(),
                timestamp_us: *timestamp_us,
            },
            vix::machine::driver::DriveEvent::Observation {
                key,
                replayed,
                key_text,
                timestamp_us,
            } => VixDriveEvent::Observation {
                key: hex_hash(*key),
                replayed: *replayed,
                key_text: key_text.clone(),
                timestamp_us: *timestamp_us,
            },
        })
        .collect()
}

fn serving_event(event: &vix::exec::ExecEvent) -> VixExecServing {
    match event {
        vix::exec::ExecEvent::Tier1Hit => VixExecServing::Tier1Hit,
        vix::exec::ExecEvent::Tier2Cutoff { verified } => VixExecServing::Tier2Cutoff {
            verified: u64::try_from(*verified).expect("verified count fits u64"),
        },
        vix::exec::ExecEvent::Ran => VixExecServing::Ran,
        vix::exec::ExecEvent::Joined => VixExecServing::Joined,
    }
}

fn lua_fetch_backend() -> vix::fetch::FakeFetchBackend {
    vix::fetch::FakeFetchBackend::new().with_archive(
        "https://www.lua.org/ftp/lua-5.4.8.tar.gz",
        b"lua-5.4.8 fixture archive",
        vix::exec::Tree::of(&[
            ("lua-5.4.8/src/lua.h", "// lua.h api"),
            (
                "lua-5.4.8/src/lua.c",
                "#include \"lua.h\"\n// interpreter main",
            ),
            ("lua-5.4.8/src/lapi.c", "#include \"lua.h\"\n// api impl"),
            ("lua-5.4.8/src/lauxlib.c", "#include \"lua.h\"\n// aux lib"),
            (
                "lua-5.4.8/src/luac.c",
                "#include \"lua.h\"\n// compiler main",
            ),
        ]),
    )
}

fn function_hashes(machine: &vix::machine::lower::Machine) -> Vec<HashLabel> {
    let mut out: Vec<_> = [
        "selected",
        "fallback",
        "subtree_chain",
        "object",
        "eval",
        "demo",
        "lua",
        "main",
    ]
    .into_iter()
    .filter_map(|name| {
        machine.fn_hash(name).map(|hash| HashLabel {
            hash: hex_hash(hash),
            label: name.to_string(),
        })
    })
    .collect();
    out.sort_by(|left, right| left.label.cmp(&right.label));
    out.dedup_by(|left, right| left.hash == right.hash);
    out
}

fn run_hashes(source: &str) -> Vec<HashLabel> {
    let mut labels = vec![
        "cc",
        "wanted.o",
        "left.o",
        "right.o",
        "x/wanted.o",
        "lua.o",
        "lapi.o",
    ];
    if source.contains("merge-demand.vix")
        || source.contains("left.c")
        || source.contains("wanted.c")
    {
        labels.extend(["x", "wanted.c", "left.c", "right.c"]);
    }
    let mut out: Vec<_> = labels
        .into_iter()
        .map(|label| HashLabel {
            hash: hex_hash(trace_hash(label)),
            label: label.to_string(),
        })
        .collect();
    out.sort_by(|left, right| left.label.cmp(&right.label));
    out.dedup_by(|left, right| left.hash == right.hash);
    out
}

fn source_kind(source: &str) -> &'static str {
    if source.contains("left.c") && source.contains("subtree_chain") {
        "merge-demand"
    } else if source.contains("pub fn demo() -> Float") {
        "eval"
    } else if source.contains("lua-5.4.8") && source.contains("pub fn lua") {
        "lua"
    } else {
        "vix"
    }
}

fn trace_hash(value: &str) -> u64 {
    let mut h = DefaultHasher::new();
    value.hash(&mut h);
    h.finish()
}

fn hex_hash(value: u64) -> String {
    format!("{value:016x}")
}
