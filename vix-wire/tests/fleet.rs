//! The whole arc in one test: a vix program's command blocks dispatching over
//! vox to a FLEET of executors — snark parses, the binder resolves, the
//! machine demands, runs join/cut off, and trees move executor→executor while
//! the orchestrator holds hashes.

use vix::exec::{ExecEvent, Tree};
use vix::fetch::FakeFetchBackend;
use vix::machine::{DriveEvent, Machine, MachineArg, NamedArg};
use vix_wire::{ExecutorDispatcher, ExecutorService, FleetBackend, Placement, Transfer};

const LUA_URL: &str = "https://www.lua.org/ftp/lua-5.4.8.tar.gz";
const LUA_ARCHIVE_BYTES: &[u8] = b"lua-5.4.8 fixture archive";

fn lua_source() -> String {
    std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../playgrounds/snark/src/bundled/vix/samples/lua.vix"
    ))
    .expect("read lua.vix corpus")
}

fn lua_fetch_backend() -> FakeFetchBackend {
    FakeFetchBackend::new().with_archive(
        LUA_URL,
        LUA_ARCHIVE_BYTES,
        Tree::of(&[
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

async fn spawn_executor() -> String {
    let listener = vox::WsListener::bind("127.0.0.1:0").await.unwrap();
    let addr = format!("ws://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        let _ = vox::serve_listener(
            listener,
            ExecutorDispatcher::new(ExecutorService::with_default_tools()),
        )
        .await;
    });
    addr
}

fn linux_arg(name: &str) -> NamedArg {
    NamedArg {
        name: name.into(),
        value: MachineArg::LinuxTarget,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn lua_builds_across_two_machines() {
    let addr_a = spawn_executor().await;
    let addr_b = spawn_executor().await;

    // Round-robin placement forces cross-machine traffic (gravity would keep
    // everything where the src tree landed first — correct, but this test
    // wants to SEE the executor→executor hop).
    let fleet = FleetBackend::connect(Placement::RoundRobin, &[addr_a, addr_b])
        .await
        .expect("fleet connects");

    let mut machine = Machine::load(&lua_source())
        .expect("lua.vix loads")
        .with_fetch_backend(lua_fetch_backend())
        .with_exec_backend(std::sync::Arc::new(fleet));

    let out = machine.call("lua", &[linux_arg("target")]).unwrap();
    let bin = machine.tree_entries(out.0).unwrap();
    assert!(
        bin.contains_key("lua"),
        "the linked binary came back across the wire: {bin:?}"
    );

    // Demand-truthful accounting: 5 dispatches, but only 3 FLUSHES — main's
    // object and the archive are only ever PROJECTED (one path each), so
    // they never flush. Projection doesn't force, even across machines.
    let events = machine.trace();
    let spawns = events
        .iter()
        .filter(|e| matches!(e, DriveEvent::RunRequested { .. }))
        .count();
    let execs: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            DriveEvent::RunCompleted {
                command_name,
                serving,
                ..
            } => Some((command_name.clone(), serving.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(spawns, 5, "{events:?}");
    assert_eq!(execs.len(), 3, "{execs:?}");
    assert!(execs.iter().all(|(_, e)| *e == ExecEvent::Ran));

    // Warm rebuild: ONE event, the fn-level memo hit. The fleet is not even
    // consulted.
    machine.clear_trace();
    let again = machine.call("lua", &[linux_arg("target")]).unwrap();
    assert_eq!(out, again);
    let warm = machine.trace();
    let lua_hash = machine.fn_hash("lua").unwrap();
    assert_eq!(
        warm,
        &[
            DriveEvent::Demanded { fn_hash: lua_hash },
            DriveEvent::MemoHit { fn_hash: lua_hash }
        ],
        "{warm:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn trees_gravity_pull_between_executors() {
    let addr_a = spawn_executor().await;
    let addr_b = spawn_executor().await;
    let fleet = FleetBackend::connect(Placement::RoundRobin, &[addr_a, addr_b])
        .await
        .expect("fleet connects");

    let mut machine = Machine::load(&lua_source())
        .expect("lua.vix loads")
        .with_fetch_backend(lua_fetch_backend());
    // Run and THEN inspect the fleet's transfer log (with_backend consumes
    // it, so keep a probe first).
    let probe = std::sync::Arc::new(fleet);
    machine = machine.with_exec_backend(probe.clone());

    machine.call("lua", &[linux_arg("target")]).unwrap();

    let transfers = probe.transfers();
    let pulls: Vec<_> = transfers
        .iter()
        .filter(|t| matches!(t, Transfer::GravityPull { .. }))
        .collect();
    assert!(
        !pulls.is_empty(),
        "round-robin placement forces at least one executor→executor pull: {transfers:?}"
    );
    // And uploads only happen for orchestrator-born trees (fetch/extract/
    // merge results) — never as a relay for something an executor already had.
    for t in &transfers {
        if let Transfer::Upload { tree, .. } = t {
            let first_movement = transfers
                .iter()
                .find(|u| match u {
                    Transfer::Upload { tree: t2, .. } => t2 == tree,
                    Transfer::GravityPull { tree: t2, .. } => t2 == tree,
                })
                .unwrap();
            assert!(
                matches!(first_movement, Transfer::Upload { .. }),
                "a tree's FIRST movement may be an upload; afterwards it must gravity-pull"
            );
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn language_level_pipelining_b_finishes_while_a_still_runs() {
    // THE demand-driven acceptance probe. `pipeline` splices ONE PATH out of
    // A's still-producing output; under strict evaluation this test cannot
    // pass (the call would block forever on A's flush — the gate never opens
    // until after the call returns). Under demand evaluation, B compiles
    // against A's rmeta and the whole call completes while A is still gated
    // in "codegen".
    let (rustc, open_gate) = vix_wire::FakeRustc::gated();
    let listener = vox::WsListener::bind("127.0.0.1:0").await.unwrap();
    let addr = format!("ws://{}", listener.local_addr().unwrap());
    let service = ExecutorService::with_default_tools().with_tool("rustc", rustc);
    tokio::spawn(async move {
        let _ = vox::serve_listener(listener, ExecutorDispatcher::new(service)).await;
    });

    let module = r#"
use vix::{Tree, Path, Target};
use caps::{Rustc, Cc};

fn get_rustc(t: Target) -> Rustc { Rustc::acquire(t) }
fn get_cc(t: Target) -> Cc { Cc::acquire(t) }

/// B consumes exactly one path of A's output: the rmeta. A's rlib may take
/// forever; B does not care. Demand is the await.
pub fn pipeline(rustc: Rustc, cc: Cc, a_src: Tree, b_src: Tree) -> Tree {
    let a_out = rustc! { {a_src / p"lib.rs"} };
    cc! { {a_out / p"lib.rmeta"} -c {b_src / p"b.c"} -o b.o }
}
"#;
    let fleet = std::sync::Arc::new(
        FleetBackend::connect(Placement::Gravity, &[addr])
            .await
            .expect("fleet connects"),
    );
    let a_src = Tree::of(&[("lib.rs", "pub fn answer() {}")]);
    let b_src = Tree::of(&[("b.c", "int b() { return 1; }")]);

    // The call runs on a blocking thread; if evaluation were strict it would
    // hang on A's flush and the timeout below would fire (the gate opens only
    // AFTER the call completes).
    let call_task = {
        let fleet = fleet.clone();
        let module = module.to_string();
        tokio::task::spawn_blocking(move || {
            let mut machine = Machine::load(&module)
                .expect("module loads")
                .with_exec_backend(fleet);
            let rustc_v = machine.call("get_rustc", &[linux_arg("t")]).unwrap();
            let cc_v = machine.call("get_cc", &[linux_arg("t")]).unwrap();
            let args = [
                NamedArg {
                    name: "rustc".into(),
                    value: MachineArg::Handle(rustc_v),
                },
                NamedArg {
                    name: "cc".into(),
                    value: MachineArg::Handle(cc_v),
                },
                NamedArg {
                    name: "a_src".into(),
                    value: MachineArg::Tree(a_src),
                },
                NamedArg {
                    name: "b_src".into(),
                    value: MachineArg::Tree(b_src),
                },
            ];
            let out = machine.call("pipeline", &args).expect("pipeline evaluates");
            let entries = machine.tree_entries(out.0).expect("pipeline tree");
            let events = machine.trace().to_vec();
            (entries, events)
        })
    };

    let (b_out, events) = tokio::time::timeout(std::time::Duration::from_secs(120), call_task)
        .await
        .expect("DEMAND-DRIVEN: the call must complete while A is still gated")
        .expect("call task joins");

    // Only now does A get to finish its "codegen" — B never needed it.
    open_gate();

    assert!(b_out.contains_key("b.o"), "{b_out:?}");

    // Two dispatches; only B ever FLUSHED (the edge demanded it). A's run
    // has no completion event: its rmeta was projected, nothing more.
    let spawns: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            DriveEvent::RunRequested { command_name, .. } => Some(command_name.as_str()),
            _ => None,
        })
        .collect();
    let flushes: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            DriveEvent::RunCompleted { command_name, .. } => Some(command_name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(spawns, vec!["rustc", "cc"], "{events:?}");
    assert_eq!(
        flushes,
        vec!["cc"],
        "A never flushed — projection doesn't force"
    );
}
