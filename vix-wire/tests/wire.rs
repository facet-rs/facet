//! Over the REAL wire (vox websocket on localhost): producing handles,
//! observer closures, and executor→executor gravity — no orchestrator relay.

use std::collections::HashMap;
use std::sync::Arc;

use vix::exec::{ExecPlan, Role, Tree};
use vix::oracle::{Oracle, Value};
use vix_wire::{
    ExecutorClient, ExecutorDispatcher, ExecutorService, FakeRustc, WireExecEvent, WireExecRequest,
    WireMount, WireTool, tree_to_bytes,
};

async fn serve(service: ExecutorService) -> (String, tokio::task::JoinHandle<()>) {
    let listener = vox::WsListener::bind("127.0.0.1:0").await.unwrap();
    let addr = format!("ws://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let _ = vox::serve_listener(listener, ExecutorDispatcher::new(service)).await;
    });
    (addr, task)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn rmeta_streams_before_rlib_finishes() {
    // The canonical pipelining probe, now across a real socket: the consumer
    // fetches lib.rmeta while "codegen" (the gated tool) is still running —
    // before the whole-tree hash can exist.
    let (rustc, open_gate) = FakeRustc::gated();
    let tools: HashMap<String, Arc<dyn WireTool>> =
        HashMap::from([("rustc".to_string(), rustc as Arc<dyn WireTool>)]);
    let (addr, _server) = serve(ExecutorService::new(tools)).await;

    let client: ExecutorClient = vox::connect_lane(&addr).await.unwrap();

    let src = Tree::of(&[("lib.rs", "pub fn answer() -> i32 { 42 }")]);
    let src_hash = client.put_tree(tree_to_bytes(&src)).await.unwrap();

    let request = WireExecRequest {
        plan: ExecPlan {
            argv: vec![("/m/0/lib.rs".to_string(), Role::Input)],
        },
        mounts: vec![WireMount {
            at: "/m/0".into(),
            tree: src_hash,
        }],
        capability: 0xcafe,
        command: "rustc".into(),
        observer: None,
        module: String::new(),
    };

    let (tx, mut rx) = vox::channel::<WireExecEvent>();
    let run_id = 1u64;
    let exec_call = {
        let client = client.clone();
        tokio::spawn(async move { client.exec(request, run_id, tx).await })
    };

    // Wait for the rmeta to land — the PRODUCING handle resolving one path.
    let mut saw_rmeta = false;
    while let Ok(Some(event)) = rx.recv().await {
        match event.get().clone() {
            WireExecEvent::PathReady { path, .. } if path == "lib.rmeta" => {
                saw_rmeta = true;
                break;
            }
            WireExecEvent::Failed { error } => panic!("exec failed early: {error}"),
            _ => {}
        }
    }
    assert!(saw_rmeta, "rmeta must stream out before the run finishes");

    // THE point: fetch the rmeta NOW, while the tool is still gated (the rlib
    // does not exist; the run is unfinished; no whole-tree hash exists).
    let rmeta = client
        .fetch_path(run_id, "lib.rmeta".into())
        .await
        .unwrap()
        .expect("rmeta is fetchable mid-run");
    assert!(rmeta.starts_with("rmeta("), "{rmeta}");
    let rlib_yet = client.fetch_path(run_id, "lib.rlib".into()).await.unwrap();
    assert!(rlib_yet.is_none(), "the rlib is still in flight");

    // Let "codegen" finish; the run completes and flushes an immutable tree.
    open_gate();
    let mut finished_tree = None;
    while let Ok(Some(event)) = rx.recv().await {
        match event.get().clone() {
            WireExecEvent::Finished { ok, tree, .. } => {
                assert!(ok);
                finished_tree = Some(tree);
                break;
            }
            WireExecEvent::Failed { error } => panic!("exec failed: {error}"),
            _ => {}
        }
    }
    exec_call.await.unwrap().unwrap();
    let flushed = finished_tree.expect("run finished");
    let bytes = client
        .fetch_tree(flushed)
        .await
        .unwrap()
        .expect("flushed tree");
    let tree = vix_wire::tree_from_bytes(&bytes).unwrap();
    assert!(tree.entries.contains_key("lib.rmeta"));
    assert!(tree.entries.contains_key("lib.rlib"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn observer_closure_evaluates_on_the_executor() {
    // The observer is a SHIPPED vix closure, generic over its return type;
    // only its result crosses back.
    let (addr, _server) = serve(ExecutorService::with_default_tools()).await;
    let client: ExecutorClient = vox::connect_lane(&addr).await.unwrap();

    let module = r#"
fn make_observer() -> fn(Run) -> Path {
    |run| run.out.glob("*.o")
}
"#;
    let oracle = Oracle::load(module).unwrap();
    let observer = oracle.call("make_observer", &[]).unwrap();
    let observer_bytes = vix::oracle::ship(&observer).unwrap();

    let src = Tree::of(&[("main.c", "int main() { return 0; }")]);
    let src_hash = client.put_tree(tree_to_bytes(&src)).await.unwrap();

    let request = WireExecRequest {
        plan: ExecPlan {
            argv: vec![
                ("-O2".to_string(), Role::Flag),
                ("/m/0/main.c".to_string(), Role::Input),
                ("-o".to_string(), Role::Flag),
                ("main.o".to_string(), Role::Output),
            ],
        },
        mounts: vec![WireMount {
            at: "/m/0".into(),
            tree: src_hash,
        }],
        capability: 0xcc,
        command: "cc".into(),
        observer: Some(observer_bytes),
        module: module.to_string(),
    };

    let (tx, mut rx) = vox::channel::<WireExecEvent>();
    let exec_call = {
        let client = client.clone();
        tokio::spawn(async move { client.exec(request, 7, tx).await })
    };

    let mut observed = None;
    while let Ok(Some(event)) = rx.recv().await {
        match event.get().clone() {
            WireExecEvent::ObserverResult { value } => {
                observed = Some(vix::oracle::receive(&value).unwrap());
            }
            WireExecEvent::Finished { .. } => break,
            WireExecEvent::Failed { error } => panic!("exec failed: {error}"),
            _ => {}
        }
    }
    exec_call.await.unwrap().unwrap();

    // The observer globbed the output tree ON THE EXECUTOR; what crossed the
    // wire is the projection, not the world it observed.
    assert_eq!(
        observed.expect("observer result crossed"),
        Value::Array(vec![Value::Path("main.o".into())])
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn trees_move_executor_to_executor_not_through_the_orchestrator() {
    // Gravity: B pulls from A directly; this test (the "orchestrator") only
    // ever handles hashes.
    let (addr_a, _sa) = serve(ExecutorService::with_default_tools()).await;
    let (addr_b, _sb) = serve(ExecutorService::with_default_tools()).await;
    let a: ExecutorClient = vox::connect_lane(&addr_a).await.unwrap();
    let b: ExecutorClient = vox::connect_lane(&addr_b).await.unwrap();

    let tree = Tree::of(&[("liblua.a", "archive(feedface)")]);
    let hash = a.put_tree(tree_to_bytes(&tree)).await.unwrap();

    assert_eq!(b.have(vec![hash]).await.unwrap(), vec![false]);
    assert!(b.pull_from(addr_a.clone(), hash).await.unwrap());
    assert_eq!(b.have(vec![hash]).await.unwrap(), vec![true]);

    // And B serves it now — the bytes took exactly one hop, A→B.
    let bytes = b.fetch_tree(hash).await.unwrap().expect("B has the tree");
    assert_eq!(vix_wire::tree_from_bytes(&bytes).unwrap(), tree);
}

// ---------------------------------------------------------------------------
// Producing-vs-L1 probes: the cache points at the RUN mid-flight.
// ---------------------------------------------------------------------------

use std::sync::atomic::{AtomicUsize, Ordering};
use vix_wire::CacheSource;

/// Counts actual tool executions — the ground truth under all the caching.
struct Counting<T: WireTool>(T, Arc<AtomicUsize>);

impl<T: WireTool> WireTool for Counting<T> {
    fn run(
        &self,
        plan: &ExecPlan,
        world: &mut vix::exec::ObservedWorld<'_>,
        emit: &mut dyn FnMut(&str, &str),
    ) -> Result<(), String> {
        self.1.fetch_add(1, Ordering::SeqCst);
        self.0.run(plan, world, emit)
    }
}

async fn drain(rx: &mut vox::Rx<WireExecEvent>) -> (Vec<WireExecEvent>, Option<CacheSource>) {
    let mut events = Vec::new();
    let mut source = None;
    while let Ok(Some(event)) = rx.recv().await {
        let event = event.get().clone();
        if let WireExecEvent::Serving { source: s } = &event {
            source = Some(s.clone());
        }
        let last = matches!(
            event,
            WireExecEvent::Finished { .. } | WireExecEvent::Failed { .. }
        );
        events.push(event);
        if last {
            break;
        }
    }
    (events, source)
}

fn cc_request(src_hash: u64) -> WireExecRequest {
    WireExecRequest {
        plan: ExecPlan {
            argv: vec![
                ("/m/0/main.c".to_string(), Role::Input),
                ("-o".to_string(), Role::Flag),
                ("main.o".to_string(), Role::Output),
            ],
        },
        mounts: vec![WireMount {
            at: "/m/0".into(),
            tree: src_hash,
        }],
        capability: 0xcc,
        command: "cc".into(),
        observer: None,
        module: String::new(),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn identical_concurrent_demands_join_one_process() {
    // Mid-flight there is no cache entry — there is a live run to ATTACH to.
    let (rustc, open_gate) = FakeRustc::gated();
    let runs = Arc::new(AtomicUsize::new(0));
    let counting: Arc<dyn WireTool> = Arc::new(Counting(ArcTool(rustc), runs.clone()));
    let tools: HashMap<String, Arc<dyn WireTool>> =
        HashMap::from([("rustc".to_string(), counting)]);
    let (addr, _server) = serve(ExecutorService::new(tools)).await;
    let client: ExecutorClient = vox::connect_lane(&addr).await.unwrap();

    let src = Tree::of(&[("lib.rs", "pub fn answer() {}")]);
    let src_hash = client.put_tree(tree_to_bytes(&src)).await.unwrap();
    let request = WireExecRequest {
        plan: ExecPlan {
            argv: vec![("/m/0/lib.rs".to_string(), Role::Input)],
        },
        mounts: vec![WireMount {
            at: "/m/0".into(),
            tree: src_hash,
        }],
        capability: 0xcafe,
        command: "rustc".into(),
        observer: None,
        module: String::new(),
    };

    // First demand drives; wait until its rmeta lands so the run is live.
    let (tx1, mut rx1) = vox::channel::<WireExecEvent>();
    let c1 = client.clone();
    let r1 = request.clone();
    let first = tokio::spawn(async move { c1.exec(r1, 101, tx1).await });
    loop {
        let Ok(Some(event)) = rx1.recv().await else {
            panic!("first stream ended early")
        };
        if matches!(event.get(), WireExecEvent::PathReady { .. }) {
            break;
        }
    }

    // Second identical demand while the tool is gated: it must JOIN.
    let (tx2, mut rx2) = vox::channel::<WireExecEvent>();
    let c2 = client.clone();
    let r2 = request.clone();
    let second = tokio::spawn(async move { c2.exec(r2, 202, tx2).await });
    // It sees the already-produced rmeta from the log without a new process.
    let (events2_task, gate) = (
        tokio::spawn(async move { drain(&mut rx2).await }),
        open_gate,
    );

    // Give the joiner a beat to attach, then let "codegen" finish.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    gate();

    let (events2, source2) = events2_task.await.unwrap();
    let (events1, source1) = {
        let mut rest = Vec::new();
        let mut source = None;
        while let Ok(Some(event)) = rx1.recv().await {
            let event = event.get().clone();
            if let WireExecEvent::Serving { source: s } = &event {
                source = Some(s.clone());
            }
            let last = matches!(event, WireExecEvent::Finished { .. });
            rest.push(event);
            if last {
                break;
            }
        }
        (rest, source)
    };
    let _ = (events1, source1);
    first.await.unwrap().unwrap();
    second.await.unwrap().unwrap();

    assert_eq!(source2, Some(CacheSource::Joined), "{events2:?}");
    assert!(
        events2
            .iter()
            .any(|e| matches!(e, WireExecEvent::PathReady { path, .. } if path == "lib.rmeta")),
        "joiner replays the log: {events2:?}"
    );
    assert_eq!(runs.load(Ordering::SeqCst), 1, "ONE process, two demands");

    // And both output spaces alias the same producing tree.
    let a = client.fetch_path(101, "lib.rlib".into()).await.unwrap();
    let b = client.fetch_path(202, "lib.rlib".into()).await.unwrap();
    assert_eq!(a, b);
    assert!(a.is_some());
}

/// FakeRustc is already Arc'd by gated(); adapt it to the wrapper.
struct ArcTool(Arc<FakeRustc>);
impl WireTool for ArcTool {
    fn run(
        &self,
        plan: &ExecPlan,
        world: &mut vix::exec::ObservedWorld<'_>,
        emit: &mut dyn FnMut(&str, &str),
    ) -> Result<(), String> {
        self.0.run(plan, world, emit)
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn one_run_many_observers_distinct_values() {
    // The run dedups by computation identity; observers memoize by
    // (run × observer) — distinct observers never alias (the L1 proof).
    let runs = Arc::new(AtomicUsize::new(0));
    let counting: Arc<dyn WireTool> =
        Arc::new(Counting(vix_wire::Atomic(vix::exec::FakeCc), runs.clone()));
    let tools: HashMap<String, Arc<dyn WireTool>> = HashMap::from([("cc".to_string(), counting)]);
    let (addr, _server) = serve(ExecutorService::new(tools)).await;
    let client: ExecutorClient = vox::connect_lane(&addr).await.unwrap();

    let src = Tree::of(&[("main.c", "int main() { return 0; }")]);
    let src_hash = client.put_tree(tree_to_bytes(&src)).await.unwrap();

    let module = r#"
fn globs() -> fn(Run) -> Path {
    |run| run.out.glob("*.o")
}
fn just_ok() -> fn(Run) -> Bool {
    |run| run.ok
}
"#;
    let oracle = Oracle::load(module).unwrap();
    let glob_obs = vix::oracle::ship(&oracle.call("globs", &[]).unwrap()).unwrap();
    let ok_obs = vix::oracle::ship(&oracle.call("just_ok", &[]).unwrap()).unwrap();

    let mut base = cc_request(src_hash);
    base.module = module.to_string();

    let mut with_glob = base.clone();
    with_glob.observer = Some(glob_obs);
    let (tx, mut rx) = vox::channel::<WireExecEvent>();
    client.exec(with_glob, 1, tx).await.unwrap();
    let (events_a, source_a) = drain(&mut rx).await;

    let mut with_ok = base.clone();
    with_ok.observer = Some(ok_obs);
    let (tx, mut rx) = vox::channel::<WireExecEvent>();
    client.exec(with_ok, 2, tx).await.unwrap();
    let (events_b, source_b) = drain(&mut rx).await;

    assert_eq!(source_a, Some(CacheSource::Fresh));
    assert_eq!(source_b, Some(CacheSource::Tier1), "same run, replayed");
    assert_eq!(runs.load(Ordering::SeqCst), 1, "one process, two observers");

    let value = |events: &[WireExecEvent]| {
        events.iter().find_map(|e| match e {
            WireExecEvent::ObserverResult { value } => Some(vix::oracle::receive(value).unwrap()),
            _ => None,
        })
    };
    assert_eq!(
        value(&events_a),
        Some(Value::Array(vec![Value::Path("main.o".into())]))
    );
    assert_eq!(value(&events_b), Some(Value::Bool(true)));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn unread_mount_change_cuts_off_at_tier2_over_the_wire() {
    let runs = Arc::new(AtomicUsize::new(0));
    let counting: Arc<dyn WireTool> =
        Arc::new(Counting(vix_wire::Atomic(vix::exec::FakeCc), runs.clone()));
    let tools: HashMap<String, Arc<dyn WireTool>> = HashMap::from([("cc".to_string(), counting)]);
    let (addr, _server) = serve(ExecutorService::new(tools)).await;
    let client: ExecutorClient = vox::connect_lane(&addr).await.unwrap();

    let src = Tree::of(&[("main.c", "int main() { return 0; }")]);
    let src_hash = client.put_tree(tree_to_bytes(&src)).await.unwrap();
    let (tx, mut rx) = vox::channel::<WireExecEvent>();
    client.exec(cc_request(src_hash), 1, tx).await.unwrap();
    let (_, source) = drain(&mut rx).await;
    assert_eq!(source, Some(CacheSource::Fresh));

    // Same computation, new world: an UNREAD file appears in the mount.
    let src2 = Tree::of(&[
        ("main.c", "int main() { return 0; }"),
        ("README", "never read by cc"),
    ]);
    let src2_hash = client.put_tree(tree_to_bytes(&src2)).await.unwrap();
    let (tx, mut rx) = vox::channel::<WireExecEvent>();
    client.exec(cc_request(src2_hash), 2, tx).await.unwrap();
    let (_, source) = drain(&mut rx).await;
    assert!(
        matches!(source, Some(CacheSource::Tier2 { .. })),
        "read-set verified, no rerun: {source:?}"
    );
    assert_eq!(runs.load(Ordering::SeqCst), 1, "the tool ran exactly once");
}
