//! The whole arc in one test: a vix program's command blocks dispatching over
//! vox to a FLEET of executors — snark parses, the binder resolves, the
//! oracle demands, runs join/cut off, and trees move executor→executor while
//! the orchestrator holds hashes.

use vix::exec::ExecEvent;
use vix::oracle::{Event, Oracle, Value};
use vix_wire::{ExecutorDispatcher, ExecutorService, FleetBackend, Placement, Transfer};

fn lua_source() -> String {
    std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../playgrounds/snark/src/bundled/vix/samples/lua.vix"
    ))
    .expect("read lua.vix corpus")
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

fn target() -> Value {
    Value::Struct {
        name: "Target".into(),
        fields: vec![("os".into(), Value::Str("linux-x86_64".into()))],
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

    let oracle = Oracle::load(&lua_source())
        .expect("lua.vix loads")
        .with_backend(Box::new(fleet));

    let out = oracle.call("lua", &[("target", target())]).unwrap();
    let Value::Tree(bin) = &out else {
        panic!("lua() returns a Tree, got {out:?}");
    };
    assert!(
        bin.entries.contains_key("lua"),
        "the linked binary came back across the wire: {bin:?}"
    );

    // All five execs (3 compiles + ar + link) ran FRESH, spread by round-robin.
    let execs: Vec<_> = oracle
        .events()
        .into_iter()
        .filter_map(|e| match e {
            Event::Exec { command, event } => Some((command, event)),
            _ => None,
        })
        .collect();
    assert_eq!(execs.len(), 5, "{execs:?}");
    assert!(execs.iter().all(|(_, e)| *e == ExecEvent::Ran));

    // Warm rebuild: ONE event, the fn-level memo hit. The fleet is not even
    // consulted.
    let before = oracle.events().len();
    let again = oracle.call("lua", &[("target", target())]).unwrap();
    assert_eq!(out, again);
    assert_eq!(
        &oracle.events()[before..],
        &[Event::Hit { func: "lua".into() }]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn trees_gravity_pull_between_executors() {
    let addr_a = spawn_executor().await;
    let addr_b = spawn_executor().await;
    let fleet = FleetBackend::connect(Placement::RoundRobin, &[addr_a, addr_b])
        .await
        .expect("fleet connects");

    let oracle = Oracle::load(&lua_source()).expect("lua.vix loads");
    // Run and THEN inspect the fleet's transfer log (with_backend consumes
    // it, so keep a probe first).
    let probe = std::sync::Arc::new(fleet);
    struct Shared(std::sync::Arc<FleetBackend>);
    impl vix::oracle::ExecBackend for Shared {
        fn exec(
            &self,
            command: &str,
            plan: &vix::exec::ExecPlan,
            capability: u64,
            mounts: &[vix::exec::Mount],
        ) -> Result<(vix::exec::Tree, ExecEvent), String> {
            self.0.exec(command, plan, capability, mounts)
        }
    }
    let oracle = oracle.with_backend(Box::new(Shared(probe.clone())));

    oracle.call("lua", &[("target", target())]).unwrap();

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
