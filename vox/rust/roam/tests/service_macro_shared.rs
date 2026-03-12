use roam_core::{BareConduit, acceptor, initiator};
use roam_types::Link;
use std::sync::{Arc, Mutex};

type MessageConduit<L> = BareConduit<roam_types::MessageFamily, L>;

#[roam::service]
trait Adder {
    async fn add(&self, a: i32, b: i32) -> i32;
}

#[derive(Clone)]
struct MyAdder;

impl Adder for MyAdder {
    async fn add(&self, a: i32, b: i32) -> i32 {
        a + b
    }
}

#[roam::service]
trait ContextProbe {
    #[roam::context]
    async fn describe(&self) -> String;

    async fn plain(&self) -> String;
}

#[derive(Clone)]
struct ContextProbeService;

impl ContextProbe for ContextProbeService {
    async fn describe(&self, cx: &roam::RequestContext<'_>) -> String {
        format!("{}:{}", cx.method().method_name, cx.metadata().len(),)
    }

    async fn plain(&self) -> String {
        "plain".to_string()
    }
}

#[roam::service]
trait MiddlewareProbe {
    #[roam::context]
    async fn inspect(&self) -> String;
}

#[derive(Clone)]
struct MiddlewareProbeService;

#[derive(Clone, Debug, PartialEq, Eq)]
struct MiddlewareValue(String);

impl MiddlewareProbe for MiddlewareProbeService {
    async fn inspect(&self, cx: &roam::RequestContext<'_>) -> String {
        cx.extensions()
            .get_cloned::<MiddlewareValue>()
            .expect("middleware should have populated request extensions")
            .0
    }
}

#[derive(Clone)]
struct RecordingMiddleware {
    name: &'static str,
    events: Arc<Mutex<Vec<String>>>,
    mode: MiddlewareMode,
}

#[derive(Clone, Copy)]
enum MiddlewareMode {
    Seed,
    Append(&'static str),
}

impl roam::ServerMiddleware for RecordingMiddleware {
    fn pre<'a>(&'a self, context: &'a roam::RequestContext<'a>) -> roam::BoxMiddlewareFuture<'a> {
        Box::pin(async move {
            record_event(&self.events, format!("{}:pre", self.name));
            match self.mode {
                MiddlewareMode::Seed => {
                    let _ = context
                        .extensions()
                        .insert(MiddlewareValue(self.name.to_string()));
                }
                MiddlewareMode::Append(suffix) => {
                    let updated = context
                        .extensions()
                        .with_mut::<MiddlewareValue, _>(|value| {
                            value.0.push_str(suffix);
                        });
                    assert!(updated.is_some(), "seed middleware should run first");
                }
            }
        })
    }

    fn post<'a>(
        &'a self,
        _context: &'a roam::RequestContext<'a>,
        outcome: roam::ServerCallOutcome,
    ) -> roam::BoxMiddlewareFuture<'a> {
        Box::pin(async move {
            record_event(&self.events, format!("{}:post:{outcome:?}", self.name));
        })
    }
}

fn record_event(events: &Arc<Mutex<Vec<String>>>, event: String) {
    events.lock().expect("events mutex poisoned").push(event);
}

pub async fn run_adder_end_to_end<L>(
    message_conduit_pair: impl FnOnce() -> (MessageConduit<L>, MessageConduit<L>),
) where
    L: Link + Send + 'static,
    L::Tx: Send + 'static,
    L::Rx: Send + 'static,
{
    let (client_conduit, server_conduit) = message_conduit_pair();

    let (server_ready_tx, server_ready_rx) = tokio::sync::oneshot::channel::<()>();
    let server_task = tokio::task::spawn(async move {
        let (server_caller_guard, _sh) = acceptor(server_conduit)
            .establish::<AdderClient>(AdderDispatcher::new(MyAdder))
            .await
            .expect("server handshake failed");
        let _ = server_ready_tx.send(());
        let _server_caller_guard = server_caller_guard;
        std::future::pending::<()>().await;
    });

    let (client, _sh) = initiator(client_conduit)
        .establish::<AdderClient>(())
        .await
        .expect("client handshake failed");

    server_ready_rx.await.expect("server setup failed");
    let response = client.add(3, 5).await.expect("add call should succeed");
    assert_eq!(response, 8);

    let response = client.add(100, -42).await.expect("add call should succeed");
    assert_eq!(response, 58);
    server_task.abort();
}

pub async fn run_request_context_end_to_end<L>(
    message_conduit_pair: impl FnOnce() -> (MessageConduit<L>, MessageConduit<L>),
) where
    L: Link + Send + 'static,
    L::Tx: Send + 'static,
    L::Rx: Send + 'static,
{
    let (client_conduit, server_conduit) = message_conduit_pair();

    let (server_ready_tx, server_ready_rx) = tokio::sync::oneshot::channel::<()>();
    let server_task = tokio::task::spawn(async move {
        let (server_caller_guard, _sh) = acceptor(server_conduit)
            .establish::<ContextProbeClient>(ContextProbeDispatcher::new(ContextProbeService))
            .await
            .expect("server handshake failed");
        let _ = server_ready_tx.send(());
        let _server_caller_guard = server_caller_guard;
        std::future::pending::<()>().await;
    });

    let (client, _sh) = initiator(client_conduit)
        .establish::<ContextProbeClient>(())
        .await
        .expect("client handshake failed");

    server_ready_rx.await.expect("server setup failed");

    let described = client
        .describe()
        .await
        .expect("describe call should succeed");
    assert_eq!(described, "describe:0");

    let plain = client.plain().await.expect("plain call should succeed");
    assert_eq!(plain, "plain");

    server_task.abort();
}

pub async fn run_server_middleware_end_to_end<L>(
    message_conduit_pair: impl FnOnce() -> (MessageConduit<L>, MessageConduit<L>),
) where
    L: Link + Send + 'static,
    L::Tx: Send + 'static,
    L::Rx: Send + 'static,
{
    let (client_conduit, server_conduit) = message_conduit_pair();
    let events = Arc::new(Mutex::new(Vec::new()));

    let (server_ready_tx, server_ready_rx) = tokio::sync::oneshot::channel::<()>();
    let server_events = Arc::clone(&events);
    let server_task = tokio::task::spawn(async move {
        let first = RecordingMiddleware {
            name: "first",
            events: Arc::clone(&server_events),
            mode: MiddlewareMode::Seed,
        };
        let second = RecordingMiddleware {
            name: "second",
            events: Arc::clone(&server_events),
            mode: MiddlewareMode::Append("+second"),
        };
        let dispatcher = MiddlewareProbeDispatcher::new(MiddlewareProbeService)
            .with_middleware(first)
            .with_middleware(second);
        let (server_caller_guard, _sh) = acceptor(server_conduit)
            .establish::<MiddlewareProbeClient>(dispatcher)
            .await
            .expect("server handshake failed");
        let _ = server_ready_tx.send(());
        let _server_caller_guard = server_caller_guard;
        std::future::pending::<()>().await;
    });

    let (client, _sh) = initiator(client_conduit)
        .establish::<MiddlewareProbeClient>(())
        .await
        .expect("client handshake failed");

    server_ready_rx.await.expect("server setup failed");

    let observed = client.inspect().await.expect("inspect call should succeed");
    assert_eq!(observed, "first+second");

    for _ in 0..32 {
        if events.lock().expect("events mutex poisoned").len() == 4 {
            break;
        }
        tokio::task::yield_now().await;
    }

    let events = events.lock().expect("events mutex poisoned").clone();
    assert_eq!(
        events,
        vec![
            "first:pre".to_string(),
            "second:pre".to_string(),
            "second:post:Replied".to_string(),
            "first:post:Replied".to_string(),
        ]
    );

    server_task.abort();
}
