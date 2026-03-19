use std::convert::Infallible;
use std::sync::{Arc, Mutex};

use roam_core::{BareConduit, acceptor_conduit, initiator_conduit};
use roam_types::{ConnectionSettings, HandshakeResult, Link, Parity, SessionRole};

fn test_acceptor_handshake() -> HandshakeResult {
    HandshakeResult {
        role: SessionRole::Acceptor,
        our_settings: ConnectionSettings {
            parity: Parity::Even,
            max_concurrent_requests: 64,
        },
        peer_settings: ConnectionSettings {
            parity: Parity::Odd,
            max_concurrent_requests: 64,
        },
        peer_supports_retry: true,
        session_resume_key: None,
        peer_resume_key: None,
        our_schema: vec![],
        peer_schema: vec![],
    }
}

fn test_initiator_handshake() -> HandshakeResult {
    HandshakeResult {
        role: SessionRole::Initiator,
        our_settings: ConnectionSettings {
            parity: Parity::Odd,
            max_concurrent_requests: 64,
        },
        peer_settings: ConnectionSettings {
            parity: Parity::Even,
            max_concurrent_requests: 64,
        },
        peer_supports_retry: true,
        session_resume_key: None,
        peer_resume_key: None,
        our_schema: vec![],
        peer_schema: vec![],
    }
}

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
trait ClientMiddlewareProbe {
    #[roam::context]
    async fn inspect(&self) -> String;
}

#[derive(Clone)]
struct ClientMiddlewareProbeService;

impl ClientMiddlewareProbe for ClientMiddlewareProbeService {
    async fn inspect(&self, cx: &roam::RequestContext<'_>) -> String {
        cx.metadata()
            .iter()
            .find(|entry| entry.key == "x-client-value")
            .and_then(|entry| match entry.value {
                roam::MetadataValue::String(value) => Some(value.to_string()),
                _ => None,
            })
            .expect("client middleware should inject request metadata")
    }
}

#[roam::service]
trait MiddlewareProbe {
    #[roam::context]
    async fn inspect(&self) -> String;
}

#[derive(Clone)]
struct MiddlewareProbeService;

#[repr(u8)]
#[derive(Clone, Copy, facet::Facet)]
pub enum BorrowedPayloadKind {
    Inline = 1,
    SlotRef = 2,
    MmapRef = 3,
}

const INLINE_PAYLOAD_LEN: usize = 64;
const SLOT_REF_PAYLOAD_LEN: usize = 1024;
const MMAP_REF_PAYLOAD_LEN: usize = 8192;

#[roam::service]
trait BorrowedPayloadProbe {
    async fn payload(&self, kind: BorrowedPayloadKind) -> &'roam str;
}

#[derive(Clone)]
struct BorrowedPayloadProbeService {
    inline: &'static str,
    slot_ref: &'static str,
    mmap_ref: &'static str,
}

impl BorrowedPayloadProbeService {
    fn new() -> Self {
        Self {
            inline: Box::leak(patterned_payload(INLINE_PAYLOAD_LEN, b'i').into_boxed_str()),
            slot_ref: Box::leak(patterned_payload(SLOT_REF_PAYLOAD_LEN, b's').into_boxed_str()),
            mmap_ref: Box::leak(patterned_payload(MMAP_REF_PAYLOAD_LEN, b'm').into_boxed_str()),
        }
    }

    fn expected_text(kind: BorrowedPayloadKind) -> String {
        match kind {
            BorrowedPayloadKind::Inline => patterned_payload(INLINE_PAYLOAD_LEN, b'i'),
            BorrowedPayloadKind::SlotRef => patterned_payload(SLOT_REF_PAYLOAD_LEN, b's'),
            BorrowedPayloadKind::MmapRef => patterned_payload(MMAP_REF_PAYLOAD_LEN, b'm'),
        }
    }
}

impl BorrowedPayloadProbe for BorrowedPayloadProbeService {
    async fn payload<'roam>(
        &self,
        call: impl roam::Call<'roam, &'roam str, Infallible>,
        kind: BorrowedPayloadKind,
    ) {
        let text = match kind {
            BorrowedPayloadKind::Inline => self.inline,
            BorrowedPayloadKind::SlotRef => self.slot_ref,
            BorrowedPayloadKind::MmapRef => self.mmap_ref,
        };
        call.ok(text).await;
    }
}

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

fn patterned_payload(len: usize, seed: u8) -> String {
    (0..len)
        .map(|index| (seed + (index % 26) as u8) as char)
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ClientMiddlewareSeed(String);

#[derive(Clone)]
struct RecordingClientMiddleware {
    name: &'static str,
    events: Arc<Mutex<Vec<String>>>,
    inject_metadata: bool,
}

impl roam::ClientMiddleware for RecordingClientMiddleware {
    fn pre<'a, 'call>(
        &'a self,
        context: &'a roam::ClientContext<'a>,
        request: &'a mut roam::ClientRequest<'call, 'a>,
    ) -> roam::BoxMiddlewareFuture<'a> {
        Box::pin(async move {
            record_event(&self.events, format!("{}:pre", self.name));
            match self.name {
                "first" => {
                    context
                        .extensions()
                        .insert(ClientMiddlewareSeed(self.name.to_string()));
                }
                "second" => {
                    assert_eq!(
                        context.extensions().get_cloned::<ClientMiddlewareSeed>(),
                        Some(ClientMiddlewareSeed("first".to_string()))
                    );
                    assert_eq!(
                        context.method().map(|method| method.method_name),
                        Some("inspect")
                    );
                }
                _ => {}
            }

            if self.inject_metadata {
                request.push_string_metadata(
                    "x-client-value",
                    format!("{}-value", self.name),
                    roam::MetadataFlags::NONE,
                );
            }
        })
    }

    fn post<'a>(
        &'a self,
        _context: &'a roam::ClientContext<'a>,
        outcome: roam::ClientCallOutcome<'a>,
    ) -> roam::BoxMiddlewareFuture<'a> {
        Box::pin(async move {
            record_event(
                &self.events,
                format!(
                    "{}:post:{}",
                    self.name,
                    if outcome.is_ok() { "ok" } else { "err" }
                ),
            );
        })
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
        let (server_caller_guard, _sh) =
            acceptor_conduit(server_conduit, test_acceptor_handshake())
                .establish::<AdderClient>(AdderDispatcher::new(MyAdder))
                .await
                .expect("server handshake failed");
        let _ = server_ready_tx.send(());
        let _server_caller_guard = server_caller_guard;
        std::future::pending::<()>().await;
    });

    let (client, _sh) = initiator_conduit(client_conduit, test_initiator_handshake())
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
        let (server_caller_guard, _sh) =
            acceptor_conduit(server_conduit, test_acceptor_handshake())
                .establish::<ContextProbeClient>(ContextProbeDispatcher::new(ContextProbeService))
                .await
                .expect("server handshake failed");
        let _ = server_ready_tx.send(());
        let _server_caller_guard = server_caller_guard;
        std::future::pending::<()>().await;
    });

    let (client, _sh) = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<ContextProbeClient>(())
        .await
        .expect("client handshake failed");

    server_ready_rx.await.expect("server setup failed");

    let described = client
        .describe()
        .await
        .expect("describe call should succeed");
    assert_eq!(described, "describe:1");

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
        let (server_caller_guard, _sh) =
            acceptor_conduit(server_conduit, test_acceptor_handshake())
                .establish::<MiddlewareProbeClient>(dispatcher)
                .await
                .expect("server handshake failed");
        let _ = server_ready_tx.send(());
        let _server_caller_guard = server_caller_guard;
        std::future::pending::<()>().await;
    });

    let (client, _sh) = initiator_conduit(client_conduit, test_initiator_handshake())
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

pub async fn run_client_middleware_end_to_end<L>(
    message_conduit_pair: impl FnOnce() -> (MessageConduit<L>, MessageConduit<L>),
) where
    L: Link + Send + 'static,
    L::Tx: Send + 'static,
    L::Rx: Send + 'static,
{
    let (client_conduit, server_conduit) = message_conduit_pair();
    let events = Arc::new(Mutex::new(Vec::new()));

    let (server_ready_tx, server_ready_rx) = tokio::sync::oneshot::channel::<()>();
    let server_task = tokio::task::spawn(async move {
        let (server_caller_guard, _sh) =
            acceptor_conduit(server_conduit, test_acceptor_handshake())
                .establish::<ClientMiddlewareProbeClient>(ClientMiddlewareProbeDispatcher::new(
                    ClientMiddlewareProbeService,
                ))
                .await
                .expect("server handshake failed");
        let _ = server_ready_tx.send(());
        let _server_caller_guard = server_caller_guard;
        std::future::pending::<()>().await;
    });

    let (client, _sh) = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<ClientMiddlewareProbeClient>(())
        .await
        .expect("client handshake failed");

    server_ready_rx.await.expect("server setup failed");

    let client = client
        .with_middleware(RecordingClientMiddleware {
            name: "first",
            events: Arc::clone(&events),
            inject_metadata: true,
        })
        .with_middleware(RecordingClientMiddleware {
            name: "second",
            events: Arc::clone(&events),
            inject_metadata: false,
        });

    let observed = client.inspect().await.expect("inspect call should succeed");
    assert_eq!(observed, "first-value");

    let events = events.lock().expect("events mutex poisoned").clone();
    assert_eq!(
        events,
        vec![
            "first:pre".to_string(),
            "second:pre".to_string(),
            "second:post:ok".to_string(),
            "first:post:ok".to_string(),
        ]
    );

    server_task.abort();
}

pub async fn run_borrowed_return_survives_teardown_over_generated_client<L>(
    message_conduit_pair: impl FnOnce() -> (MessageConduit<L>, MessageConduit<L>),
    kind: BorrowedPayloadKind,
) where
    L: Link + Send + 'static,
    L::Tx: Send + 'static,
    L::Rx: Send + 'static,
{
    let (client_conduit, server_conduit) = message_conduit_pair();
    let expected = BorrowedPayloadProbeService::expected_text(kind);

    let (server_ready_tx, server_ready_rx) = tokio::sync::oneshot::channel::<()>();
    let server_task = tokio::task::spawn(async move {
        let (server_caller_guard, _sh) =
            acceptor_conduit(server_conduit, test_acceptor_handshake())
                .establish::<BorrowedPayloadProbeClient>(BorrowedPayloadProbeDispatcher::new(
                    BorrowedPayloadProbeService::new(),
                ))
                .await
                .expect("server handshake failed");
        let _ = server_ready_tx.send(());
        let _server_caller_guard = server_caller_guard;
        std::future::pending::<()>().await;
    });

    let (client, client_session_handle) =
        initiator_conduit(client_conduit, test_initiator_handshake())
            .establish::<BorrowedPayloadProbeClient>(())
            .await
            .expect("client handshake failed");

    server_ready_rx.await.expect("server setup failed");

    let payload = client
        .payload(kind)
        .await
        .expect("borrowed payload call should succeed");

    drop(client);
    drop(client_session_handle);
    server_task.abort();
    let _ = server_task.await;

    assert_eq!(&*payload, &expected);
}
