use roam_core::{BareConduit, acceptor, initiator};
use roam_types::Link;

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
