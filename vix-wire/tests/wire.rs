//! Over the REAL wire (vox websocket on localhost): producing handles,
//! observer closures, and executor→executor gravity — no orchestrator relay.

use std::collections::HashMap;
use std::sync::Arc;

use vix::exec::{ExecPlan, Role, Tree};
use vix::oracle::{Oracle, Value};
use vix_wire::{
    ExecutorClient, ExecutorDispatcher, ExecutorService, FakeRustc, WireExecEvent,
    WireExecRequest, WireMount, WireTool, tree_to_bytes,
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
        mounts: vec![WireMount { at: "/m/0".into(), tree: src_hash }],
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
    let bytes = client.fetch_tree(flushed).await.unwrap().expect("flushed tree");
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
        mounts: vec![WireMount { at: "/m/0".into(), tree: src_hash }],
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
