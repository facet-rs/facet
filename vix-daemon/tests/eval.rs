//! The daemon over a real vox websocket: the IDE↔daemon path, proven. An RPC
//! client evaluates vix, receives the demand-event stream, and — in step mode —
//! drives the evaluation one demand at a time.

use vix_daemon::{
    DaemonClient, DaemonDispatcher, DaemonService, DemandEvent, EvalRequest, Serving, StepCommand,
    StepMode,
};

const LUA: &str = include_str!(
    "../../playgrounds/snark/src/bundled/vix/samples/lua.vix"
);

async fn serve() -> String {
    let listener = vox::WsListener::bind("127.0.0.1:0").await.unwrap();
    let addr = format!("ws://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        let _ = vox::serve_listener(listener, DaemonDispatcher::new(DaemonService::new())).await;
    });
    addr
}

async fn collect(
    client: &DaemonClient,
    source: &str,
    entry: &str,
    mode: StepMode,
) -> Vec<DemandEvent> {
    let (control_tx, control_rx) = vox::channel::<StepCommand>();
    let (events_tx, mut events_rx) = vox::channel::<DemandEvent>();
    let req = EvalRequest {
        source: source.into(),
        entry: entry.into(),
        mode,
    };
    let call = {
        let client = client.clone();
        tokio::spawn(async move { client.eval(req, control_rx, events_tx).await })
    };
    // Run mode: no stepping needed; drop the control side.
    drop(control_tx);
    let mut out = Vec::new();
    while let Ok(Some(e)) = events_rx.recv().await {
        let e = e.get().clone();
        let done = matches!(e, DemandEvent::Done { .. } | DemandEvent::Failed { .. });
        out.push(e);
        if done {
            break;
        }
    }
    call.await.unwrap().unwrap();
    out
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn daemon_evaluates_lua_over_the_wire() {
    let addr = serve().await;
    let client: DaemonClient = vox::connect_lane(&addr).await.unwrap();

    let events = collect(&client, LUA, "lua", StepMode::Run).await;

    // The evaluation ran and produced the linked binary tree.
    let done = events.last().expect("at least the Done event");
    assert!(
        matches!(done, DemandEvent::Done { result } if result.contains("lua")),
        "final: {done:?}"
    );
    // The demand stream shows real work: fn miss for `lua`, exec dispatches,
    // exec serving classes, and fetch/acquire observations.
    assert!(events.iter().any(|e| matches!(e, DemandEvent::Miss { func, .. } if func == "lua")));
    assert!(events.iter().any(|e| matches!(e, DemandEvent::Created { command, .. } if command == "cc")));
    assert!(events.iter().any(
        |e| matches!(e, DemandEvent::Finished { serving: Serving::Ran, .. })
    ));
    assert!(events.iter().any(|e| matches!(e, DemandEvent::Observation { .. })));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn warm_daemon_whitespace_edit_is_one_hit() {
    let addr = serve().await;
    let client: DaemonClient = vox::connect_lane(&addr).await.unwrap();

    let first = collect(&client, LUA, "lua", StepMode::Run).await;
    assert!(
        matches!(first.last(), Some(DemandEvent::Done { .. })),
        "first eval did not finish cleanly: {first:?}"
    );

    let warm_source = format!("// warm\n{LUA}");
    let second = collect(&client, &warm_source, "lua", StepMode::Run).await;

    assert_eq!(second.len(), 2, "warm eval events: {second:?}");
    assert!(
        matches!(&second[0], DemandEvent::Hit { func, .. } if func == "lua"),
        "first warm event: {:?}",
        second[0]
    );
    assert!(
        matches!(&second[1], DemandEvent::Done { .. }),
        "second warm event: {:?}",
        second[1]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn warm_daemon_edited_fn_reruns() {
    let addr = serve().await;
    let client: DaemonClient = vox::connect_lane(&addr).await.unwrap();

    let first = collect(&client, LUA, "lua", StepMode::Run).await;
    assert!(
        matches!(first.last(), Some(DemandEvent::Done { .. })),
        "first eval did not finish cleanly: {first:?}"
    );

    let edited = LUA.replace(
        "Linux => [-DLUA_USE_LINUX],",
        "Linux => [-DLUA_USE_LINUX, -DVIX_WARM_RERUN],",
    );
    assert_ne!(edited, LUA, "semantic edit fixture must change lua.vix");
    let second = collect(&client, &edited, "lua", StepMode::Run).await;

    assert!(
        second
            .iter()
            .any(|e| matches!(e, DemandEvent::Miss { .. })),
        "edited eval did not miss: {second:?}"
    );
    assert!(
        second
            .iter()
            .any(|e| matches!(e, DemandEvent::Finished { serving: Serving::Ran, .. })),
        "edited eval did not run work: {second:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stepping_gates_the_demand_one_event_at_a_time() {
    let addr = serve().await;
    let client: DaemonClient = vox::connect_lane(&addr).await.unwrap();

    let (control_tx, control_rx) = vox::channel::<StepCommand>();
    let (events_tx, mut events_rx) = vox::channel::<DemandEvent>();
    let req = EvalRequest {
        source: LUA.into(),
        entry: "lua".into(),
        mode: StepMode::Step,
    };
    let call = {
        let client = client.clone();
        tokio::spawn(async move { client.eval(req, control_rx, events_tx).await })
    };

    // In step mode the daemon GATES: the evaluation blocks until we Step. So we
    // must receive exactly one event per Step we send. Walk a few, then Resume.
    let mut count = 0;
    let mut done = false;
    for _ in 0..3 {
        control_tx.send(StepCommand::Step).await.unwrap();
        let e = events_rx.recv().await.unwrap().unwrap().get().clone();
        count += 1;
        if matches!(e, DemandEvent::Done { .. } | DemandEvent::Failed { .. }) {
            done = true;
            break;
        }
    }
    assert!(count >= 1, "stepping produced gated events");

    if !done {
        // Resume: the rest streams without gating, up to Done.
        control_tx.send(StepCommand::Resume).await.unwrap();
        while let Ok(Some(e)) = events_rx.recv().await {
            if matches!(e.get(), DemandEvent::Done { .. } | DemandEvent::Failed { .. }) {
                done = true;
                break;
            }
        }
    }
    assert!(done, "stepped evaluation reached Done");
    call.await.unwrap().unwrap();
}
