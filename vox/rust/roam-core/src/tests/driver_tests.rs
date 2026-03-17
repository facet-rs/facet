use facet::Facet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use moire::sync::mpsc;
use moire::task::FutureExt;
use roam_types::{
    Backing, Caller, ChannelBinder, ChannelBody, ChannelClose, ChannelGrantCredit, ChannelId,
    ChannelItem, ChannelMessage, ChannelSink, Conduit, ConduitRx, ConnectionSettings, Handler,
    IncomingChannelMessage, Link, LinkRx, LinkTx, LinkTxPermit, Message, MessageFamily,
    MessagePayload, Metadata, MethodId, Parity, Payload, ReplySink, RequestBody, RequestCall,
    RequestCancel, RequestId, RequestMessage, RequestResponse, RetryPolicy, RoamError, RpcPlan,
    SelfRef, Tx, WriteSlot, bind_channels_caller_args, channel, ensure_operation_id,
    metadata_operation_id,
};

use roam_types::{HandshakeResult, SessionResumeKey, SessionRole};

use crate::session::{
    AcceptedConnection, ConnectionAcceptor, ConnectionMessage, SessionAcceptOutcome, SessionError,
    SessionHandle, SessionKeepaliveConfig, SessionRegistry, acceptor, acceptor_on,
    initiator_conduit, initiator_on, proxy_connections,
};
use crate::{
    BareConduit, Driver, DriverCaller, DriverReplySink, InMemoryOperationStore, OperationAdmit,
    OperationCancel, OperationStore, TransportMode, initiate_transport, memory_link_pair,
};

fn test_resume_key() -> SessionResumeKey {
    SessionResumeKey([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16])
}

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
        session_resume_key: Some(test_resume_key()),
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
        session_resume_key: Some(test_resume_key()),
        peer_resume_key: None,
        our_schema: vec![],
        peer_schema: vec![],
    }
}

type MessageConduit = BareConduit<MessageFamily, crate::MemoryLink>;

fn message_conduit_pair() -> (MessageConduit, MessageConduit) {
    let (a, b) = memory_link_pair(64);
    (BareConduit::new(a), BareConduit::new(b))
}

struct BreakableLink {
    tx: mpsc::Sender<Option<Vec<u8>>>,
    rx: mpsc::Receiver<Option<Vec<u8>>>,
}

#[derive(Clone)]
struct BreakHandle {
    tx: mpsc::Sender<Option<Vec<u8>>>,
}

fn breakable_link_pair(buffer: usize) -> (BreakableLink, BreakHandle, BreakableLink, BreakHandle) {
    let (tx_a, rx_b) = mpsc::channel("breakable_link.a→b", buffer);
    let (tx_b, rx_a) = mpsc::channel("breakable_link.b→a", buffer);

    let a_handle = BreakHandle { tx: tx_b.clone() };
    let b_handle = BreakHandle { tx: tx_a.clone() };

    (
        BreakableLink { tx: tx_a, rx: rx_a },
        a_handle,
        BreakableLink { tx: tx_b, rx: rx_b },
        b_handle,
    )
}

impl BreakHandle {
    async fn close(&self) {
        let _ = self.tx.send(None).await;
    }
}

impl Link for BreakableLink {
    type Tx = BreakableLinkTx;
    type Rx = BreakableLinkRx;

    fn split(self) -> (Self::Tx, Self::Rx) {
        (
            BreakableLinkTx { tx: self.tx },
            BreakableLinkRx { rx: self.rx },
        )
    }
}

#[derive(Clone)]
struct BreakableLinkTx {
    tx: mpsc::Sender<Option<Vec<u8>>>,
}

struct BreakableLinkTxPermit {
    permit: mpsc::OwnedPermit<Option<Vec<u8>>>,
}

impl LinkTx for BreakableLinkTx {
    type Permit = BreakableLinkTxPermit;

    async fn reserve(&self) -> std::io::Result<Self::Permit> {
        let permit = self.tx.clone().reserve_owned().await.map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::ConnectionReset, "receiver dropped")
        })?;
        Ok(BreakableLinkTxPermit { permit })
    }

    async fn close(self) -> std::io::Result<()> {
        drop(self.tx);
        Ok(())
    }
}

struct BreakableWriteSlot {
    buf: Vec<u8>,
    permit: mpsc::OwnedPermit<Option<Vec<u8>>>,
}

impl LinkTxPermit for BreakableLinkTxPermit {
    type Slot = BreakableWriteSlot;

    fn alloc(self, len: usize) -> std::io::Result<Self::Slot> {
        Ok(BreakableWriteSlot {
            buf: vec![0u8; len],
            permit: self.permit,
        })
    }
}

impl WriteSlot for BreakableWriteSlot {
    fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.buf
    }

    fn commit(self) {
        drop(self.permit.send(Some(self.buf)));
    }
}

struct BreakableLinkRx {
    rx: mpsc::Receiver<Option<Vec<u8>>>,
}

#[derive(Debug)]
struct BreakableLinkRxError;

impl std::fmt::Display for BreakableLinkRxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "breakable link rx error")
    }
}

impl std::error::Error for BreakableLinkRxError {}

impl LinkRx for BreakableLinkRx {
    type Error = BreakableLinkRxError;

    async fn recv(&mut self) -> Result<Option<Backing>, Self::Error> {
        match self.rx.recv().await {
            Some(Some(bytes)) => Ok(Some(Backing::Boxed(bytes.into_boxed_slice()))),
            Some(None) | None => Ok(None),
        }
    }
}

/// Conduit wrapper used by keepalive tests: drops inbound Pong messages.
struct DropPongConduit<C> {
    inner: C,
}

impl<C> DropPongConduit<C> {
    fn new(inner: C) -> Self {
        Self { inner }
    }
}

impl<C> Conduit for DropPongConduit<C>
where
    C: Conduit<Msg = MessageFamily>,
    C::Rx: Send,
{
    type Msg = MessageFamily;
    type Tx = C::Tx;
    type Rx = DropPongRx<C::Rx>;

    fn split(self) -> (Self::Tx, Self::Rx) {
        let (tx, rx) = self.inner.split();
        (tx, DropPongRx { inner: rx })
    }
}

impl<C> crate::IntoConduit for DropPongConduit<C>
where
    C: Conduit<Msg = MessageFamily>,
    C::Rx: Send,
{
    type Conduit = Self;

    fn into_conduit(self) -> Self {
        self
    }
}

struct DropPongRx<Rx> {
    inner: Rx,
}

impl<Rx> ConduitRx for DropPongRx<Rx>
where
    Rx: ConduitRx<Msg = MessageFamily> + Send,
{
    type Msg = MessageFamily;
    type Error = Rx::Error;

    async fn recv(&mut self) -> Result<Option<SelfRef<Message<'static>>>, Self::Error> {
        loop {
            let Some(msg) = self.inner.recv().await? else {
                return Ok(None);
            };
            if matches!(&msg.payload, MessagePayload::Pong(_)) {
                continue;
            }
            return Ok(Some(msg));
        }
    }
}

/// A handler that echoes back the raw args payload as the response.
struct EchoHandler;

impl Handler<DriverReplySink> for EchoHandler {
    async fn handle(&self, call: SelfRef<RequestCall<'static>>, reply: DriverReplySink) {
        let args_bytes = match &call.args {
            Payload::Incoming(bytes) => *bytes,
            _ => panic!("expected incoming payload"),
        };

        let result: u32 = roam_postcard::from_slice(args_bytes).expect("deserialize args");
        reply
            .send_reply(RequestResponse {
                ret: Payload::outgoing(&result),
                channels: vec![],
                metadata: Default::default(),
            })
            .await;
    }
}

/// A handler that blocks forever until its task is cancelled.
/// Tracks whether cancellation occurred via a drop guard.
struct BlockingHandler {
    was_cancelled: Arc<AtomicBool>,
    retry: RetryPolicy,
}

impl Handler<DriverReplySink> for BlockingHandler {
    fn retry_policy(&self, _method_id: MethodId) -> RetryPolicy {
        self.retry
    }

    async fn handle(&self, _call: SelfRef<RequestCall<'static>>, reply: DriverReplySink) {
        let was_cancelled = self.was_cancelled.clone();
        // Hold the reply to prevent premature DriverReplySink::drop
        let _reply = reply;
        // Create a drop guard that records cancellation
        struct DropGuard(Arc<AtomicBool>);
        impl Drop for DropGuard {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let _guard = DropGuard(was_cancelled);
        // Block forever — only cancellation (abort) will stop this
        std::future::pending::<()>().await;
    }
}

struct PersistentReplyingHandler {
    was_cancelled: Arc<AtomicBool>,
    release: Arc<tokio::sync::Notify>,
}

struct ResumableReplyingHandler {
    started: Arc<tokio::sync::Notify>,
    release: Arc<tokio::sync::Notify>,
}

struct RetryAfterResumeHandler {
    retry: RetryPolicy,
    runs: Arc<AtomicUsize>,
    first_started: Arc<tokio::sync::Notify>,
    drop_first: Arc<tokio::sync::Notify>,
}

struct OperationIdHandler;

impl Handler<DriverReplySink> for OperationIdHandler {
    async fn handle(&self, call: SelfRef<RequestCall<'static>>, reply: DriverReplySink) {
        let operation_id = metadata_operation_id(&call.metadata).expect("operation id metadata");
        reply
            .send_reply(RequestResponse {
                ret: Payload::outgoing(&operation_id),
                channels: vec![],
                metadata: Default::default(),
            })
            .await;
    }
}

struct ReplayHandler {
    runs: Arc<std::sync::atomic::AtomicUsize>,
    release: Arc<tokio::sync::Notify>,
}

struct CountingOperationStore {
    inner: InMemoryOperationStore,
    admits: AtomicUsize,
}

impl CountingOperationStore {
    fn new() -> Self {
        Self {
            inner: InMemoryOperationStore::default(),
            admits: AtomicUsize::new(0),
        }
    }
}

impl OperationStore for CountingOperationStore {
    fn admit(
        &self,
        operation_id: u64,
        method_id: MethodId,
        args: &[u8],
        retry: RetryPolicy,
        request_id: RequestId,
    ) -> OperationAdmit {
        self.admits.fetch_add(1, Ordering::SeqCst);
        self.inner
            .admit(operation_id, method_id, args, retry, request_id)
    }

    fn seal(
        &self,
        operation_id: u64,
        owner_request_id: RequestId,
        encoded_response: Arc<[u8]>,
    ) -> Vec<RequestId> {
        self.inner
            .seal(operation_id, owner_request_id, encoded_response)
    }

    fn fail_without_reply(&self, operation_id: u64, owner_request_id: RequestId) -> Vec<RequestId> {
        self.inner
            .fail_without_reply(operation_id, owner_request_id)
    }

    fn cancel(&self, request_id: RequestId) -> OperationCancel {
        self.inner.cancel(request_id)
    }
}

impl Handler<DriverReplySink> for ReplayHandler {
    fn retry_policy(&self, _method_id: MethodId) -> RetryPolicy {
        RetryPolicy::PERSIST
    }

    async fn handle(&self, call: SelfRef<RequestCall<'static>>, reply: DriverReplySink) {
        self.runs.fetch_add(1, Ordering::SeqCst);
        self.release.notified().await;
        let args_bytes = match &call.args {
            Payload::Incoming(bytes) => *bytes,
            _ => panic!("expected incoming payload"),
        };
        let result: u32 = roam_postcard::from_slice(args_bytes).expect("deserialize args");
        reply
            .send_reply(RequestResponse {
                ret: Payload::outgoing(&result),
                channels: vec![],
                metadata: Default::default(),
            })
            .await;
    }
}

impl Handler<DriverReplySink> for PersistentReplyingHandler {
    fn retry_policy(&self, _method_id: MethodId) -> RetryPolicy {
        RetryPolicy::PERSIST
    }

    async fn handle(&self, _call: SelfRef<RequestCall<'static>>, reply: DriverReplySink) {
        let was_cancelled = Arc::clone(&self.was_cancelled);
        let release = Arc::clone(&self.release);
        let completed = Arc::new(AtomicBool::new(false));

        struct DropGuard {
            was_cancelled: Arc<AtomicBool>,
            completed: Arc<AtomicBool>,
        }

        impl Drop for DropGuard {
            fn drop(&mut self) {
                if !self.completed.load(Ordering::SeqCst) {
                    self.was_cancelled.store(true, Ordering::SeqCst);
                }
            }
        }

        let _guard = DropGuard {
            was_cancelled,
            completed: Arc::clone(&completed),
        };

        release.notified().await;
        completed.store(true, Ordering::SeqCst);
        reply
            .send_reply(RequestResponse {
                ret: Payload::outgoing(&123_u32),
                channels: vec![],
                metadata: Default::default(),
            })
            .await;
    }
}

impl Handler<DriverReplySink> for ResumableReplyingHandler {
    fn retry_policy(&self, _method_id: MethodId) -> RetryPolicy {
        RetryPolicy::PERSIST
    }

    async fn handle(&self, call: SelfRef<RequestCall<'static>>, reply: DriverReplySink) {
        self.started.notify_waiters();
        self.release.notified().await;

        let args_bytes = match &call.args {
            Payload::Incoming(bytes) => *bytes,
            _ => panic!("expected incoming payload"),
        };
        let result: u32 = roam_postcard::from_slice(args_bytes).expect("deserialize args");
        reply
            .send_reply(RequestResponse {
                ret: Payload::outgoing(&result),
                channels: vec![],
                metadata: Default::default(),
            })
            .await;
    }
}

impl Handler<DriverReplySink> for RetryAfterResumeHandler {
    fn retry_policy(&self, _method_id: MethodId) -> RetryPolicy {
        self.retry
    }

    async fn handle(&self, call: SelfRef<RequestCall<'static>>, reply: DriverReplySink) {
        let run = self.runs.fetch_add(1, Ordering::SeqCst);
        if run == 0 {
            self.first_started.notify_waiters();
            self.drop_first.notified().await;
            return;
        }

        let args_bytes = match &call.args {
            Payload::Incoming(bytes) => *bytes,
            _ => panic!("expected incoming payload"),
        };
        let result: u32 = roam_postcard::from_slice(args_bytes).expect("deserialize args");
        reply
            .send_reply(RequestResponse {
                ret: Payload::outgoing(&result),
                channels: vec![],
                metadata: Default::default(),
            })
            .await;
    }
}

#[derive(Facet)]
struct SubscribeArgs {
    updates: Tx<u32, 16>,
}

// r[verify rpc.caller.liveness.refcounted]
#[tokio::test]
async fn dropping_one_root_caller_clone_keeps_session_alive_until_last_drop() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let (client_session_tx, client_session_rx) =
        tokio::sync::oneshot::channel::<moire::task::JoinHandle<()>>();
    let (server_session_tx, server_session_rx) =
        tokio::sync::oneshot::channel::<moire::task::JoinHandle<()>>();

    let server_task = moire::task::spawn(
        async move {
            let (server_caller, _sh) = acceptor(server_conduit, test_acceptor_handshake())
                .spawn_fn(move |fut| {
                    let handle = moire::task::spawn(fut.named("server_session"));
                    let _ = server_session_tx.send(handle);
                })
                .establish::<DriverCaller>(EchoHandler)
                .await
                .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    let (caller, _sh) = initiator_conduit(client_conduit, test_initiator_handshake())
        .spawn_fn(move |fut| {
            let handle = moire::task::spawn(fut.named("client_session"));
            let _ = client_session_tx.send(handle);
        })
        .establish::<DriverCaller>(())
        .await
        .expect("client handshake failed");

    let server_caller_guard = server_task.await.expect("server setup failed");
    let client_session = client_session_rx.await.expect("client session handle sent");
    let server_session = server_session_rx.await.expect("server session handle sent");

    let caller_clone = caller.clone();
    drop(caller_clone);

    let response = caller
        .call(RequestCall {
            method_id: MethodId(1),
            args: Payload::outgoing(&42_u32),
            channels: vec![],
            metadata: Default::default(),
        })
        .await
        .expect("call should still succeed while one root caller remains");
    let ret_bytes = match &response.ret {
        Payload::Incoming(bytes) => *bytes,
        _ => panic!("expected incoming payload in response"),
    };
    let echoed: u32 = roam_postcard::from_slice(ret_bytes).expect("deserialize response");
    assert_eq!(echoed, 42);

    drop(caller);
    drop(server_caller_guard);

    tokio::time::timeout(std::time::Duration::from_millis(500), client_session)
        .await
        .expect("timed out waiting for client session to exit")
        .expect("client session task failed");
    tokio::time::timeout(std::time::Duration::from_millis(500), server_session)
        .await
        .expect("timed out waiting for server session to exit")
        .expect("server session task failed");
}

// r[verify rpc.caller.liveness.root-internal-close]
// r[verify rpc.caller.liveness.root-teardown-condition]
#[tokio::test]
async fn dropping_root_caller_waits_for_virtual_connections_before_session_shutdown() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let (client_session_tx, client_session_rx) =
        tokio::sync::oneshot::channel::<moire::task::JoinHandle<()>>();

    struct LocalEchoAcceptor;

    impl ConnectionAcceptor for LocalEchoAcceptor {
        fn accept(
            &self,
            _conn_id: roam_types::ConnectionId,
            peer_settings: &ConnectionSettings,
            _metadata: &[roam_types::MetadataEntry],
        ) -> Result<AcceptedConnection, Metadata<'static>> {
            let peer_parity = peer_settings.parity;
            Ok(AcceptedConnection {
                settings: ConnectionSettings {
                    parity: peer_parity.other(),
                    max_concurrent_requests: 64,
                },
                metadata: vec![],
                setup: Box::new(move |handle| {
                    let mut driver = Driver::new(handle, EchoHandler);
                    moire::task::spawn(
                        async move { driver.run().await }.named("vconn_server_driver"),
                    );
                }),
            })
        }
    }

    let server_task = moire::task::spawn(
        async move {
            let (server_caller, _sh) = acceptor(server_conduit, test_acceptor_handshake())
                .on_connection(LocalEchoAcceptor)
                .establish::<DriverCaller>(())
                .await
                .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    let (root_caller, session_handle) =
        initiator_conduit(client_conduit, test_initiator_handshake())
            .spawn_fn(move |fut| {
                let handle = moire::task::spawn(fut.named("client_session"));
                let _ = client_session_tx.send(handle);
            })
            .establish::<DriverCaller>(())
            .await
            .expect("client handshake failed");

    let server_caller_guard = server_task.await.expect("server setup failed");
    let client_session = client_session_rx.await.expect("client session handle sent");

    let vconn_handle = session_handle
        .open_connection(
            ConnectionSettings {
                parity: Parity::Odd,
                max_concurrent_requests: 64,
            },
            vec![],
        )
        .await
        .expect("open virtual connection");

    let mut vconn_driver = Driver::new(vconn_handle, ());
    let vconn_caller = vconn_driver.caller();
    moire::task::spawn(async move { vconn_driver.run().await }.named("vconn_client_driver"));

    drop(root_caller);
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(
        !client_session.is_finished(),
        "session should remain alive while a virtual connection is still caller-live"
    );

    let response = vconn_caller
        .call(RequestCall {
            method_id: MethodId(1),
            args: Payload::outgoing(&7_u32),
            channels: vec![],
            metadata: Default::default(),
        })
        .await
        .expect("virtual connection should still be usable after root caller drop");
    let ret_bytes = match &response.ret {
        Payload::Incoming(bytes) => *bytes,
        _ => panic!("expected incoming payload in response"),
    };
    let echoed: u32 = roam_postcard::from_slice(ret_bytes).expect("deserialize response");
    assert_eq!(echoed, 7);

    drop(vconn_caller);
    drop(server_caller_guard);

    tokio::time::timeout(std::time::Duration::from_millis(500), client_session)
        .await
        .expect("timed out waiting for client session to exit")
        .expect("client session task failed");
}

// r[verify rpc.channel.binding.caller-args.tx]
#[tokio::test]
async fn dropping_root_caller_keeps_session_alive_while_bound_stream_rx_exists() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let (client_session_tx, client_session_rx) =
        tokio::sync::oneshot::channel::<moire::task::JoinHandle<()>>();

    let server_task = moire::task::spawn(
        async move {
            let (server_caller, _sh) = acceptor(server_conduit, test_acceptor_handshake())
                .establish::<DriverCaller>(())
                .await
                .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    let (root_caller, _sh) = initiator_conduit(client_conduit, test_initiator_handshake())
        .spawn_fn(move |fut| {
            let handle = moire::task::spawn(fut.named("client_session"));
            let _ = client_session_tx.send(handle);
        })
        .establish::<DriverCaller>(())
        .await
        .expect("client handshake failed");

    let server_caller = server_task.await.expect("server setup failed");
    let client_session = client_session_rx.await.expect("client session handle sent");

    let (updates_tx, mut updates_rx) = channel::<u32>();
    let mut args = SubscribeArgs {
        updates: updates_tx,
    };
    let channel_ids = unsafe {
        bind_channels_caller_args(
            (&mut args as *mut SubscribeArgs).cast::<u8>(),
            RpcPlan::for_type::<SubscribeArgs>(),
            &root_caller,
        )
    };
    assert_eq!(channel_ids.as_slice(), &[ChannelId(1)]);
    drop(args);
    drop(root_caller);

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(
        !client_session.is_finished(),
        "session should remain alive while a bound stream handle still exists"
    );

    let value = 123_u32;
    server_caller
        .connection_sender()
        .send(ConnectionMessage::Channel(ChannelMessage {
            id: channel_ids[0],
            body: ChannelBody::Item(ChannelItem {
                item: Payload::outgoing(&value),
            }),
        }))
        .await
        .expect("send channel item");

    let received = updates_rx
        .recv()
        .await
        .expect("stream should remain usable")
        .expect("channel should yield one item");
    assert_eq!(*received, 123);

    server_caller
        .connection_sender()
        .send(ConnectionMessage::Channel(ChannelMessage {
            id: channel_ids[0],
            body: ChannelBody::Close(ChannelClose {
                metadata: Default::default(),
            }),
        }))
        .await
        .expect("send channel close");

    assert!(
        updates_rx
            .recv()
            .await
            .expect("close should be delivered")
            .is_none(),
        "stream should end after explicit close"
    );

    drop(updates_rx);
    drop(server_caller);

    tokio::time::timeout(std::time::Duration::from_millis(500), client_session)
        .await
        .expect("timed out waiting for client session to exit")
        .expect("client session task failed");
}

// r[verify rpc.cancel]
// r[verify rpc.cancel.channels]
#[tokio::test]
async fn cancel_aborts_in_flight_handler() {
    facet_testhelpers::setup();
    let (client_conduit, server_conduit) = message_conduit_pair();

    let was_cancelled = Arc::new(AtomicBool::new(false));
    let was_cancelled_check = was_cancelled.clone();

    let server_task = moire::task::spawn(
        async move {
            let (server_caller, _sh) = acceptor(server_conduit, test_acceptor_handshake())
                .establish::<DriverCaller>(BlockingHandler {
                    was_cancelled,
                    retry: RetryPolicy::VOLATILE,
                })
                .await
                .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    // Set up client side. We need both the Caller (for sending the call) and
    // the raw sender (for sending the cancel message with the same request ID).
    let (caller, _sh) = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<DriverCaller>(())
        .await
        .expect("client handshake failed");
    let client_sender = caller.connection_sender().clone();

    let _server_caller_guard = server_task.await.expect("server setup failed");

    // Spawn the call as a task so we can concurrently send a cancel.
    let call_task = moire::task::spawn(
        async move {
            let args_value: u32 = 99;
            caller
                .call(RequestCall {
                    method_id: MethodId(1),
                    args: Payload::outgoing(&args_value),
                    channels: vec![],
                    metadata: Default::default(),
                })
                .await
        }
        .named("client_call"),
    );

    // Give the call time to reach the server and start the handler.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Send a cancel for request ID 1 (the first request on an Odd-parity
    // connection allocates ID 1).
    let cancel_req_id = roam_types::RequestId(1);
    client_sender
        .send(ConnectionMessage::Request(RequestMessage {
            id: cancel_req_id,
            body: RequestBody::Cancel(RequestCancel {
                metadata: Metadata::default(),
            }),
        }))
        .await
        .expect("send cancel");

    // The call should resolve with an Err(Cancelled) in the wire Result envelope.
    let result = call_task.await.expect("call task join");
    let response = result.expect("call should receive a response");
    let ret_bytes = match &response.ret {
        Payload::Incoming(bytes) => *bytes,
        _ => panic!("expected incoming payload in response"),
    };
    let error: Result<(), RoamError> =
        roam_postcard::from_slice(ret_bytes).expect("deserialize response");
    assert!(
        matches!(error, Err(RoamError::Cancelled)),
        "expected Err(RoamError::Cancelled) in response payload"
    );

    // Wait for the handler abort to propagate (drop guard sets the flag).
    for _ in 0..20 {
        if was_cancelled_check.load(Ordering::SeqCst) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    // Verify the handler was actually cancelled (drop guard fired).
    assert!(
        was_cancelled_check.load(Ordering::SeqCst),
        "handler should have been cancelled"
    );
}

#[tokio::test]
async fn cancel_does_not_abort_persist_handler() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let was_cancelled = Arc::new(AtomicBool::new(false));
    let was_cancelled_check = Arc::clone(&was_cancelled);
    let release = Arc::new(tokio::sync::Notify::new());
    let release_server = Arc::clone(&release);

    let server_task = moire::task::spawn(
        async move {
            let (server_caller, _sh) = acceptor(server_conduit, test_acceptor_handshake())
                .establish::<DriverCaller>(PersistentReplyingHandler {
                    was_cancelled,
                    release: release_server,
                })
                .await
                .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    let (caller, _sh) = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<DriverCaller>(())
        .await
        .expect("client handshake failed");
    let client_sender = caller.connection_sender().clone();

    let _server_caller_guard = server_task.await.expect("server setup failed");

    let call_task = moire::task::spawn(
        async move {
            caller
                .call(RequestCall {
                    method_id: MethodId(1),
                    args: Payload::outgoing(&99_u32),
                    channels: vec![],
                    metadata: Default::default(),
                })
                .await
        }
        .named("client_call_persist"),
    );

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    client_sender
        .send(ConnectionMessage::Request(RequestMessage {
            id: roam_types::RequestId(1),
            body: RequestBody::Cancel(RequestCancel {
                metadata: Metadata::default(),
            }),
        }))
        .await
        .expect("send cancel");

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(
        !was_cancelled_check.load(Ordering::SeqCst),
        "persist handler should not be cancelled by explicit cancel"
    );

    release.notify_waiters();

    let response = tokio::time::timeout(std::time::Duration::from_millis(500), call_task)
        .await
        .expect("timed out waiting for persist handler to finish")
        .expect("call task join")
        .expect("persist call should still receive a response");
    let ret_bytes = match &response.ret {
        Payload::Incoming(bytes) => *bytes,
        _ => panic!("expected incoming payload in response"),
    };
    let value: u32 = roam_postcard::from_slice(ret_bytes).expect("deserialize response");
    assert_eq!(value, 123);
}

#[tokio::test]
async fn caller_injects_operation_id_when_peer_supports_retry() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = moire::task::spawn(
        async move {
            let (server_caller, _sh) = acceptor(server_conduit, test_acceptor_handshake())
                .establish::<DriverCaller>(OperationIdHandler)
                .await
                .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    let (caller, _sh) = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<DriverCaller>(())
        .await
        .expect("client handshake failed");

    let _server_caller_guard = server_task.await.expect("server setup failed");

    let response = caller
        .call(RequestCall {
            method_id: MethodId(1),
            args: Payload::outgoing(&7_u32),
            channels: vec![],
            metadata: Default::default(),
        })
        .await
        .expect("call should succeed");

    let ret_bytes = match &response.ret {
        Payload::Incoming(bytes) => *bytes,
        _ => panic!("expected incoming payload in response"),
    };
    let operation_id: u64 = roam_postcard::from_slice(ret_bytes).expect("deserialize response");
    assert_ne!(operation_id, 0);
}

#[tokio::test]
async fn builder_uses_custom_operation_store() {
    let (client_conduit, server_conduit) = message_conduit_pair();
    let store = Arc::new(CountingOperationStore::new());
    let store_check = Arc::clone(&store);

    let server_task = moire::task::spawn(
        async move {
            let (server_caller, _sh) = acceptor(server_conduit, test_acceptor_handshake())
                .operation_store(store)
                .establish::<DriverCaller>(OperationIdHandler)
                .await
                .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    let (caller, _sh) = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<DriverCaller>(())
        .await
        .expect("client handshake failed");

    let _server_caller_guard = server_task.await.expect("server setup failed");

    let _response = caller
        .call(RequestCall {
            method_id: MethodId(1),
            args: Payload::outgoing(&7_u32),
            channels: vec![],
            metadata: Default::default(),
        })
        .await
        .expect("call should succeed");

    assert_ne!(store_check.admits.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn duplicate_operation_id_attaches_live_and_replays_sealed_outcome() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let runs = Arc::new(AtomicUsize::new(0));
    let runs_check = Arc::clone(&runs);
    let release = Arc::new(tokio::sync::Notify::new());
    let release_server = Arc::clone(&release);

    let server_task = moire::task::spawn(
        async move {
            let (server_caller, _sh) = acceptor(server_conduit, test_acceptor_handshake())
                .establish::<DriverCaller>(ReplayHandler {
                    runs,
                    release: release_server,
                })
                .await
                .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    let (caller, _sh) = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<DriverCaller>(())
        .await
        .expect("client handshake failed");

    let _server_caller_guard = server_task.await.expect("server setup failed");

    let mut metadata = Metadata::default();
    ensure_operation_id(&mut metadata, 99);

    let first = moire::task::spawn(
        {
            let caller = caller.clone();
            let metadata = metadata.clone();
            async move {
                caller
                    .call(RequestCall {
                        method_id: MethodId(1),
                        args: Payload::outgoing(&11_u32),
                        channels: vec![],
                        metadata,
                    })
                    .await
            }
        }
        .named("first_duplicate_call"),
    );

    let second = moire::task::spawn(
        {
            let caller = caller.clone();
            let metadata = metadata.clone();
            async move {
                caller
                    .call(RequestCall {
                        method_id: MethodId(1),
                        args: Payload::outgoing(&11_u32),
                        channels: vec![],
                        metadata,
                    })
                    .await
            }
        }
        .named("second_duplicate_call"),
    );

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert_eq!(runs_check.load(Ordering::SeqCst), 1);

    release.notify_waiters();

    for response in [
        first.await.expect("first join"),
        second.await.expect("second join"),
    ] {
        let response = response.expect("duplicate call should succeed");
        let ret_bytes = match &response.ret {
            Payload::Incoming(bytes) => *bytes,
            _ => panic!("expected incoming payload in response"),
        };
        let value: u32 = roam_postcard::from_slice(ret_bytes).expect("deserialize response");
        assert_eq!(value, 11);
    }
    assert_eq!(runs_check.load(Ordering::SeqCst), 1);

    let replayed = caller
        .call(RequestCall {
            method_id: MethodId(1),
            args: Payload::outgoing(&11_u32),
            channels: vec![],
            metadata,
        })
        .await
        .expect("sealed replay should succeed");
    let ret_bytes = match &replayed.ret {
        Payload::Incoming(bytes) => *bytes,
        _ => panic!("expected incoming payload in response"),
    };
    let value: u32 = roam_postcard::from_slice(ret_bytes).expect("deserialize response");
    assert_eq!(value, 11);
    assert_eq!(runs_check.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn resumable_session_keeps_pending_call_alive_across_manual_resume() {
    let (client_link1, client_break1, server_link1, server_break1) = breakable_link_pair(64);
    let client_conduit1 = BareConduit::new(client_link1);
    let server_conduit1 = BareConduit::new(server_link1);

    let started = Arc::new(tokio::sync::Notify::new());
    let started_for_wait = Arc::clone(&started);
    let started_wait = started_for_wait.notified();
    let release = Arc::new(tokio::sync::Notify::new());

    let (server_established, client_established) = tokio::try_join!(
        tokio::time::timeout(
            Duration::from_secs(1),
            acceptor(server_conduit1, test_acceptor_handshake())
                .resumable()
                .establish::<DriverCaller>(ResumableReplyingHandler {
                    started,
                    release: Arc::clone(&release),
                }),
        ),
        tokio::time::timeout(
            Duration::from_secs(1),
            initiator_conduit(client_conduit1, test_initiator_handshake())
                .resumable()
                .establish::<DriverCaller>(()),
        ),
    )
    .expect("initial session establishment timed out");
    let (_server_caller, server_session_handle) =
        server_established.expect("server handshake failed");
    let (caller, client_session_handle) = client_established.expect("client handshake failed");

    let call_task = moire::task::spawn(
        async move {
            caller
                .call(RequestCall {
                    method_id: MethodId(1),
                    args: Payload::outgoing(&55_u32),
                    channels: vec![],
                    metadata: Default::default(),
                })
                .await
        }
        .named("resume_pending_call"),
    );

    tokio::time::timeout(Duration::from_secs(1), started_wait)
        .await
        .expect("timed out waiting for handler start");

    client_break1.close().await;
    server_break1.close().await;
    tokio::time::sleep(Duration::from_millis(25)).await;

    let (client_link2, client_break2, server_link2, server_break2) = breakable_link_pair(64);
    tokio::try_join!(
        client_session_handle.resume(BareConduit::new(client_link2), test_initiator_handshake()),
        server_session_handle.resume(BareConduit::new(server_link2), test_acceptor_handshake()),
    )
    .expect("session resume should succeed");

    release.notify_waiters();

    let response = call_task
        .await
        .expect("call task join")
        .expect("call should succeed after session resume");
    let ret_bytes = match &response.ret {
        Payload::Incoming(bytes) => *bytes,
        _ => panic!("expected incoming payload in response"),
    };
    let value: u32 = roam_postcard::from_slice(ret_bytes).expect("deserialize response");
    assert_eq!(value, 55);

    let _ = client_session_handle.shutdown();
    let _ = server_session_handle.shutdown();
    client_break2.close().await;
    server_break2.close().await;
}

#[tokio::test]
async fn resumable_acceptor_registry_keeps_pending_call_alive_across_auto_resume() {
    let registry = SessionRegistry::default();
    let (client_link1, client_break1, server_link1, server_break1) = breakable_link_pair(64);
    let started = Arc::new(tokio::sync::Notify::new());
    let started_for_wait = Arc::clone(&started);
    let started_wait = started_for_wait.notified();
    let release = Arc::new(tokio::sync::Notify::new());

    let (server_established, client_established) = tokio::try_join!(
        tokio::time::timeout(
            Duration::from_secs(1),
            acceptor_on(server_link1)
                .session_registry(registry.clone())
                .establish_or_resume::<DriverCaller>(ResumableReplyingHandler {
                    started,
                    release: Arc::clone(&release),
                }),
        ),
        tokio::time::timeout(
            Duration::from_secs(1),
            initiator_on(client_link1, TransportMode::Bare)
                .resumable()
                .establish::<DriverCaller>(()),
        ),
    )
    .expect("initial session establishment timed out");
    let (server_caller, _server_session_handle) =
        match server_established.expect("server handshake failed") {
            SessionAcceptOutcome::Established(client, handle) => (client, handle),
            SessionAcceptOutcome::Resumed => panic!("first accept should establish a new session"),
        };
    let (caller, client_session_handle) = client_established.expect("client handshake failed");

    let call_task = moire::task::spawn(
        async move {
            caller
                .call(RequestCall {
                    method_id: MethodId(1),
                    args: Payload::outgoing(&66_u32),
                    channels: vec![],
                    metadata: Default::default(),
                })
                .await
        }
        .named("registry_resume_pending_call"),
    );

    tokio::time::timeout(Duration::from_secs(1), started_wait)
        .await
        .expect("timed out waiting for handler start");

    client_break1.close().await;
    server_break1.close().await;
    tokio::time::sleep(Duration::from_millis(25)).await;

    let (client_link2, client_break2, server_link2, server_break2) = breakable_link_pair(64);
    let (resume_result, server_accept_result) = tokio::join!(
        async {
            let mut resumed_link = initiate_transport(client_link2, TransportMode::Bare)
                .await
                .expect("client transport prologue should succeed");
            let handshake_result = crate::handshake_as_initiator(
                &resumed_link.tx,
                &mut resumed_link.rx,
                ConnectionSettings {
                    parity: Parity::Odd,
                    max_concurrent_requests: 64,
                },
                true,
                client_session_handle.resume_key(),
            )
            .await
            .expect("client CBOR handshake should succeed");
            client_session_handle
                .resume(BareConduit::new(resumed_link), handshake_result)
                .await
        },
        acceptor_on(server_link2)
            .session_registry(registry.clone())
            .establish_or_resume::<DriverCaller>(ResumableReplyingHandler {
                started: Arc::new(tokio::sync::Notify::new()),
                release: Arc::clone(&release),
            }),
    );
    resume_result.expect("client session resume should succeed");
    match server_accept_result.expect("server accept should succeed") {
        SessionAcceptOutcome::Resumed => {}
        SessionAcceptOutcome::Established(_, _) => {
            panic!("registry accept should have resumed the existing session")
        }
    }

    release.notify_waiters();

    let response = call_task
        .await
        .expect("call task join")
        .expect("call should succeed after registry-driven session resume");
    let ret_bytes = match &response.ret {
        Payload::Incoming(bytes) => *bytes,
        _ => panic!("expected incoming payload in response"),
    };
    let value: u32 = roam_postcard::from_slice(ret_bytes).expect("deserialize response");
    assert_eq!(value, 66);

    drop(server_caller);
    let _ = client_session_handle.shutdown();
    client_break2.close().await;
    server_break2.close().await;
}

#[tokio::test]
async fn resumable_session_reruns_released_idem_call_after_manual_resume() {
    let (client_link1, client_break1, server_link1, server_break1) = breakable_link_pair(64);
    let client_conduit1 = BareConduit::new(client_link1);
    let server_conduit1 = BareConduit::new(server_link1);

    let first_started = Arc::new(tokio::sync::Notify::new());
    let first_started_waiter = Arc::clone(&first_started);
    let first_started_wait = first_started_waiter.notified();
    let drop_first = Arc::new(tokio::sync::Notify::new());
    let runs = Arc::new(AtomicUsize::new(0));

    let (server_established, client_established) = tokio::try_join!(
        tokio::time::timeout(
            Duration::from_secs(1),
            acceptor(server_conduit1, test_acceptor_handshake())
                .resumable()
                .establish::<DriverCaller>(RetryAfterResumeHandler {
                    retry: RetryPolicy::IDEM,
                    runs: Arc::clone(&runs),
                    first_started,
                    drop_first: Arc::clone(&drop_first),
                }),
        ),
        tokio::time::timeout(
            Duration::from_secs(1),
            initiator_conduit(client_conduit1, test_initiator_handshake())
                .resumable()
                .establish::<DriverCaller>(()),
        ),
    )
    .expect("initial session establishment timed out");
    let (_server_caller, server_session_handle) =
        server_established.expect("server handshake failed");
    let (caller, client_session_handle) = client_established.expect("client handshake failed");

    let call_task = moire::task::spawn(
        async move {
            caller
                .call(RequestCall {
                    method_id: MethodId(1),
                    args: Payload::outgoing(&77_u32),
                    channels: vec![],
                    metadata: Default::default(),
                })
                .await
        }
        .named("resume_retry_idem"),
    );

    tokio::time::timeout(Duration::from_secs(1), first_started_wait)
        .await
        .expect("timed out waiting for first handler start");

    client_break1.close().await;
    server_break1.close().await;
    drop_first.notify_waiters();
    tokio::time::sleep(Duration::from_millis(25)).await;

    let (client_link2, client_break2, server_link2, server_break2) = breakable_link_pair(64);
    tokio::try_join!(
        client_session_handle.resume(BareConduit::new(client_link2), test_initiator_handshake()),
        server_session_handle.resume(BareConduit::new(server_link2), test_acceptor_handshake()),
    )
    .expect("session resume should succeed");

    let response = call_task
        .await
        .expect("call task join")
        .expect("idem call should succeed after retry");
    let ret_bytes = match &response.ret {
        Payload::Incoming(bytes) => *bytes,
        _ => panic!("expected incoming payload in response"),
    };
    let value: u32 = roam_postcard::from_slice(ret_bytes).expect("deserialize response");
    assert_eq!(value, 77);
    assert_eq!(runs.load(Ordering::SeqCst), 2);

    let _ = client_session_handle.shutdown();
    let _ = server_session_handle.shutdown();
    client_break2.close().await;
    server_break2.close().await;
}

#[tokio::test]
async fn resumable_session_returns_indeterminate_for_released_non_idem_call_after_manual_resume() {
    let (client_link1, client_break1, server_link1, server_break1) = breakable_link_pair(64);
    let client_conduit1 = BareConduit::new(client_link1);
    let server_conduit1 = BareConduit::new(server_link1);

    let first_started = Arc::new(tokio::sync::Notify::new());
    let first_started_waiter = Arc::clone(&first_started);
    let first_started_wait = first_started_waiter.notified();
    let drop_first = Arc::new(tokio::sync::Notify::new());
    let runs = Arc::new(AtomicUsize::new(0));

    let (server_established, client_established) = tokio::try_join!(
        tokio::time::timeout(
            Duration::from_secs(1),
            acceptor(server_conduit1, test_acceptor_handshake())
                .resumable()
                .establish::<DriverCaller>(RetryAfterResumeHandler {
                    retry: RetryPolicy::VOLATILE,
                    runs: Arc::clone(&runs),
                    first_started,
                    drop_first: Arc::clone(&drop_first),
                }),
        ),
        tokio::time::timeout(
            Duration::from_secs(1),
            initiator_conduit(client_conduit1, test_initiator_handshake())
                .resumable()
                .establish::<DriverCaller>(()),
        ),
    )
    .expect("initial session establishment timed out");
    let (_server_caller, server_session_handle) =
        server_established.expect("server handshake failed");
    let (caller, client_session_handle) = client_established.expect("client handshake failed");

    let call_task = moire::task::spawn(
        async move {
            caller
                .call(RequestCall {
                    method_id: MethodId(1),
                    args: Payload::outgoing(&88_u32),
                    channels: vec![],
                    metadata: Default::default(),
                })
                .await
        }
        .named("resume_retry_non_idem"),
    );

    tokio::time::timeout(Duration::from_secs(1), first_started_wait)
        .await
        .expect("timed out waiting for first handler start");

    client_break1.close().await;
    server_break1.close().await;
    drop_first.notify_waiters();
    tokio::time::sleep(Duration::from_millis(25)).await;

    let (client_link2, client_break2, server_link2, server_break2) = breakable_link_pair(64);
    tokio::try_join!(
        client_session_handle.resume(BareConduit::new(client_link2), test_initiator_handshake()),
        server_session_handle.resume(BareConduit::new(server_link2), test_acceptor_handshake()),
    )
    .expect("session resume should succeed");

    let response = call_task
        .await
        .expect("call task join")
        .expect("runtime should return a response envelope");
    let ret_bytes = match &response.ret {
        Payload::Incoming(bytes) => *bytes,
        _ => panic!("expected incoming payload in response"),
    };
    let result: Result<u32, RoamError> =
        roam_postcard::from_slice(ret_bytes).expect("deserialize response");
    assert!(matches!(result, Err(RoamError::Indeterminate)));
    assert_eq!(runs.load(Ordering::SeqCst), 1);

    let _ = client_session_handle.shutdown();
    let _ = server_session_handle.shutdown();
    client_break2.close().await;
    server_break2.close().await;
}

#[tokio::test]
async fn in_flight_call_returns_cancelled_when_peer_closes() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let was_cancelled = Arc::new(AtomicBool::new(false));
    let was_cancelled_check = was_cancelled.clone();

    let (session_tx, session_rx) = tokio::sync::oneshot::channel::<moire::task::JoinHandle<()>>();
    let server_task = moire::task::spawn(
        async move {
            let (server_caller, _sh) = acceptor(server_conduit, test_acceptor_handshake())
                .spawn_fn(move |fut| {
                    let handle = moire::task::spawn(fut);
                    let _ = session_tx.send(handle);
                })
                .establish::<DriverCaller>(BlockingHandler {
                    was_cancelled,
                    retry: RetryPolicy::VOLATILE,
                })
                .await
                .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    let (caller, _sh) = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<DriverCaller>(())
        .await
        .expect("client handshake failed");

    let server_caller_guard = server_task.await.expect("server setup failed");
    let server_session_task = session_rx.await.expect("session handle sent");

    let call_task = moire::task::spawn(
        async move {
            caller
                .call(RequestCall {
                    method_id: MethodId(1),
                    args: Payload::outgoing(&123_u32),
                    channels: vec![],
                    metadata: Default::default(),
                })
                .await
        }
        .named("client_call"),
    );

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    drop(server_caller_guard);
    server_session_task.abort();

    let result = tokio::time::timeout(std::time::Duration::from_millis(500), call_task)
        .await
        .expect("timed out waiting for call to resolve after peer close")
        .expect("call task join");
    assert!(
        matches!(result, Err(RoamError::Cancelled)),
        "expected cancelled after peer close"
    );

    for _ in 0..20 {
        if was_cancelled_check.load(Ordering::SeqCst) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(
        was_cancelled_check.load(Ordering::SeqCst),
        "server handler should be cancelled when peer connection closes"
    );
}

#[tokio::test]
async fn keepalive_timeout_returns_cancelled_when_pongs_are_missing() {
    let (client_link, server_link) = memory_link_pair(64);
    let client_conduit = DropPongConduit::new(BareConduit::new(client_link));
    let server_conduit = BareConduit::new(server_link);

    let server_task = moire::task::spawn(
        async move {
            let (server_caller, _sh) = acceptor(server_conduit, test_acceptor_handshake())
                .establish::<DriverCaller>(BlockingHandler {
                    was_cancelled: Arc::new(AtomicBool::new(false)),
                    retry: RetryPolicy::VOLATILE,
                })
                .await
                .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    let (caller, _sh) = initiator_conduit(client_conduit, test_initiator_handshake())
        .keepalive(SessionKeepaliveConfig {
            ping_interval: std::time::Duration::from_millis(20),
            pong_timeout: std::time::Duration::from_millis(50),
        })
        .establish::<DriverCaller>(())
        .await
        .expect("client handshake failed");

    let _server_caller_guard = server_task.await.expect("server setup failed");

    let call_task = moire::task::spawn(
        async move {
            caller
                .call(RequestCall {
                    method_id: MethodId(1),
                    args: Payload::outgoing(&123_u32),
                    channels: vec![],
                    metadata: Default::default(),
                })
                .await
        }
        .named("client_call"),
    );

    let result = tokio::time::timeout(std::time::Duration::from_secs(1), call_task)
        .await
        .expect("timed out waiting for call to resolve after keepalive timeout")
        .expect("call task join");
    assert!(
        matches!(result, Err(RoamError::Cancelled)),
        "expected cancelled after keepalive timeout"
    );
}

#[tokio::test]
async fn dropping_root_caller_shuts_down_session() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let (client_session_tx, client_session_rx) =
        tokio::sync::oneshot::channel::<moire::task::JoinHandle<()>>();
    let (server_session_tx, server_session_rx) =
        tokio::sync::oneshot::channel::<moire::task::JoinHandle<()>>();

    let server_task = moire::task::spawn(
        async move {
            let (server_caller, _sh) = acceptor(server_conduit, test_acceptor_handshake())
                .spawn_fn(move |fut| {
                    let handle = moire::task::spawn(fut.named("server_session"));
                    let _ = server_session_tx.send(handle);
                })
                .establish::<DriverCaller>(EchoHandler)
                .await
                .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    let (caller, _sh) = initiator_conduit(client_conduit, test_initiator_handshake())
        .spawn_fn(move |fut| {
            let handle = moire::task::spawn(fut.named("client_session"));
            let _ = client_session_tx.send(handle);
        })
        .establish::<DriverCaller>(())
        .await
        .expect("client handshake failed");

    let _server_caller_guard = server_task.await.expect("server setup failed");

    let client_session = client_session_rx.await.expect("client session handle sent");
    let server_session = server_session_rx.await.expect("server session handle sent");

    drop(caller);

    tokio::time::timeout(std::time::Duration::from_millis(500), client_session)
        .await
        .expect("timed out waiting for client session to exit")
        .expect("client session task failed");
    tokio::time::timeout(std::time::Duration::from_millis(500), server_session)
        .await
        .expect("timed out waiting for server session to exit")
        .expect("server session task failed");
}

// ---------------------------------------------------------------------------
// Virtual connection tests
// ---------------------------------------------------------------------------

/// An acceptor that spawns an EchoHandler driver on each accepted connection.
struct EchoAcceptor;

impl ConnectionAcceptor for EchoAcceptor {
    fn accept(
        &self,
        _conn_id: roam_types::ConnectionId,
        peer_settings: &ConnectionSettings,
        _metadata: &[roam_types::MetadataEntry],
    ) -> Result<AcceptedConnection, Metadata<'static>> {
        let peer_parity = peer_settings.parity;
        Ok(AcceptedConnection {
            settings: ConnectionSettings {
                parity: peer_parity.other(),
                max_concurrent_requests: 64,
            },
            metadata: vec![],
            setup: Box::new(move |handle| {
                let mut driver = Driver::new(handle, EchoHandler);
                moire::task::spawn(async move { driver.run().await }.named("vconn_server_driver"));
            }),
        })
    }
}

/// An acceptor that rejects every connection.
struct RejectAcceptor;

impl ConnectionAcceptor for RejectAcceptor {
    fn accept(
        &self,
        _conn_id: roam_types::ConnectionId,
        _peer_settings: &ConnectionSettings,
        _metadata: &[roam_types::MetadataEntry],
    ) -> Result<AcceptedConnection, Metadata<'static>> {
        Err(vec![])
    }
}

// r[verify rpc.virtual-connection.open]
// r[verify rpc.virtual-connection.accept]
// r[verify connection.open]
#[tokio::test]
async fn open_virtual_connection_and_call() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = moire::task::spawn(
        async move {
            let (server_caller, _sh) = acceptor(server_conduit, test_acceptor_handshake())
                .on_connection(EchoAcceptor)
                .establish::<DriverCaller>(())
                .await
                .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    let (_client_caller_guard, session_handle) =
        initiator_conduit(client_conduit, test_initiator_handshake())
            .establish::<DriverCaller>(())
            .await
            .expect("client handshake failed");

    let _server_caller_guard = server_task.await.expect("server setup failed");

    // Open a virtual connection.
    let vconn_handle = session_handle
        .open_connection(
            ConnectionSettings {
                parity: Parity::Odd,
                max_concurrent_requests: 64,
            },
            vec![],
        )
        .await
        .expect("open virtual connection");

    // Set up a driver on the client side for the virtual connection.
    let mut vconn_driver = Driver::new(vconn_handle, ());
    let caller = vconn_driver.caller();
    moire::task::spawn(async move { vconn_driver.run().await }.named("vconn_client_driver"));

    // Make a call on the virtual connection.
    let args_value: u32 = 123;
    let response = caller
        .call(RequestCall {
            method_id: MethodId(1),
            args: Payload::outgoing(&args_value),
            channels: vec![],
            metadata: Default::default(),
        })
        .await
        .expect("call should succeed");

    let ret_bytes = match &response.ret {
        Payload::Incoming(bytes) => *bytes,
        _ => panic!("expected incoming payload in response"),
    };
    let result: u32 = roam_postcard::from_slice(ret_bytes).expect("deserialize response");
    assert_eq!(result, 123);
}

#[tokio::test]
async fn initiator_builder_customization_controls_allocated_connection_parity() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = moire::task::spawn(
        async move {
            let (server_caller, _sh) = acceptor(
                server_conduit,
                HandshakeResult {
                    role: SessionRole::Acceptor,
                    our_settings: ConnectionSettings {
                        parity: Parity::Odd,
                        max_concurrent_requests: 32,
                    },
                    peer_settings: ConnectionSettings {
                        parity: Parity::Even,
                        max_concurrent_requests: 64,
                    },
                    peer_supports_retry: false,
                    session_resume_key: None,
                    peer_resume_key: None,
                    our_schema: vec![],
                    peer_schema: vec![],
                },
            )
            .on_connection(EchoAcceptor)
            .establish::<DriverCaller>(())
            .await
            .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    let (_client_caller_guard, session_handle) = initiator_conduit(
        client_conduit,
        HandshakeResult {
            role: SessionRole::Initiator,
            our_settings: ConnectionSettings {
                parity: Parity::Even,
                max_concurrent_requests: 64,
            },
            peer_settings: ConnectionSettings {
                parity: Parity::Odd,
                max_concurrent_requests: 32,
            },
            peer_supports_retry: false,
            session_resume_key: None,
            peer_resume_key: None,
            our_schema: vec![],
            peer_schema: vec![],
        },
    )
    .establish::<DriverCaller>(())
    .await
    .expect("client handshake failed");

    let _server_caller_guard = server_task.await.expect("server setup failed");

    let vconn_handle = session_handle
        .open_connection(
            ConnectionSettings {
                parity: Parity::Even,
                max_concurrent_requests: 64,
            },
            vec![],
        )
        .await
        .expect("open virtual connection");

    let conn_id = vconn_handle.connection_id();
    assert!(
        conn_id.has_parity(Parity::Even),
        "initiator parity should drive allocated connection ids"
    );
}

#[tokio::test]
async fn acceptor_builder_customization_supports_opening_connections() {
    let (client_conduit, acceptor_conduit) = message_conduit_pair();

    let initiator_task = moire::task::spawn(
        async move {
            let (initiator_caller, _initiator_session_handle) = initiator_conduit(
                client_conduit,
                HandshakeResult {
                    role: SessionRole::Initiator,
                    our_settings: ConnectionSettings {
                        parity: Parity::Even,
                        max_concurrent_requests: 64,
                    },
                    peer_settings: ConnectionSettings {
                        parity: Parity::Odd,
                        max_concurrent_requests: 32,
                    },
                    peer_supports_retry: false,
                    session_resume_key: None,
                    peer_resume_key: None,
                    our_schema: vec![],
                    peer_schema: vec![],
                },
            )
            .on_connection(EchoAcceptor)
            .establish::<DriverCaller>(())
            .await
            .expect("initiator handshake failed");
            initiator_caller
        }
        .named("initiator_setup"),
    );

    let (_acceptor_caller_guard, acceptor_session_handle) = acceptor(
        acceptor_conduit,
        HandshakeResult {
            role: SessionRole::Acceptor,
            our_settings: ConnectionSettings {
                parity: Parity::Odd,
                max_concurrent_requests: 32,
            },
            peer_settings: ConnectionSettings {
                parity: Parity::Even,
                max_concurrent_requests: 64,
            },
            peer_supports_retry: false,
            session_resume_key: None,
            peer_resume_key: None,
            our_schema: vec![],
            peer_schema: vec![],
        },
    )
    .establish::<DriverCaller>(())
    .await
    .expect("acceptor handshake failed");

    let _initiator_caller_guard = initiator_task.await.expect("initiator setup failed");

    let vconn_handle = acceptor_session_handle
        .open_connection(
            ConnectionSettings {
                parity: Parity::Odd,
                max_concurrent_requests: 64,
            },
            vec![],
        )
        .await
        .expect("acceptor opens virtual connection");

    let conn_id = vconn_handle.connection_id();
    assert!(
        conn_id.has_parity(Parity::Odd),
        "acceptor should allocate odd ids when peer initiator parity is even"
    );
}

// r[verify connection.open.rejection]
#[tokio::test]
async fn reject_virtual_connection() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = moire::task::spawn(
        async move {
            let (server_caller, _sh) = acceptor(server_conduit, test_acceptor_handshake())
                .on_connection(RejectAcceptor)
                .establish::<DriverCaller>(())
                .await
                .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    let (_client_caller_guard, session_handle) =
        initiator_conduit(client_conduit, test_initiator_handshake())
            .establish::<DriverCaller>(())
            .await
            .expect("client handshake failed");

    let _server_caller_guard = server_task.await.expect("server setup failed");

    // Try to open a virtual connection — should be rejected.
    let result = session_handle
        .open_connection(
            ConnectionSettings {
                parity: Parity::Odd,
                max_concurrent_requests: 64,
            },
            vec![],
        )
        .await;

    assert!(
        matches!(result, Err(SessionError::Rejected(_))),
        "expected Rejected, got: {result:?}"
    );
}

// r[verify connection.open.rejection]
#[tokio::test]
async fn open_virtual_connection_without_acceptor_is_rejected() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = moire::task::spawn(
        async move {
            let (server_caller, _sh) = acceptor(server_conduit, test_acceptor_handshake())
                .establish::<DriverCaller>(())
                .await
                .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    let (_client_caller_guard, session_handle) =
        initiator_conduit(client_conduit, test_initiator_handshake())
            .establish::<DriverCaller>(())
            .await
            .expect("client handshake failed");

    let _server_caller_guard = server_task.await.expect("server setup failed");

    let result = session_handle
        .open_connection(
            ConnectionSettings {
                parity: Parity::Odd,
                max_concurrent_requests: 64,
            },
            vec![],
        )
        .await;

    assert!(
        matches!(result, Err(SessionError::Rejected(_))),
        "expected Rejected, got: {result:?}"
    );
}

// r[verify connection.close]
#[tokio::test]
async fn close_root_connection_is_rejected() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = moire::task::spawn(
        async move {
            let (server_caller, _sh) = acceptor(server_conduit, test_acceptor_handshake())
                .establish::<DriverCaller>(EchoHandler)
                .await
                .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    let (_client_caller_guard, session_handle) =
        initiator_conduit(client_conduit, test_initiator_handshake())
            .establish::<DriverCaller>(())
            .await
            .expect("client handshake failed");

    let _server_caller_guard = server_task.await.expect("server setup failed");

    let result = session_handle
        .close_connection(roam_types::ConnectionId::ROOT, vec![])
        .await;
    assert!(
        matches!(result, Err(SessionError::Protocol(ref msg)) if msg == "cannot close root connection"),
        "expected root-close protocol error, got: {result:?}"
    );
}

// r[verify connection.close]
#[tokio::test]
async fn close_unknown_virtual_connection_is_rejected() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = moire::task::spawn(
        async move {
            let (server_caller, _sh) = acceptor(server_conduit, test_acceptor_handshake())
                .establish::<DriverCaller>(EchoHandler)
                .await
                .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    let (_client_caller_guard, session_handle) =
        initiator_conduit(client_conduit, test_initiator_handshake())
            .establish::<DriverCaller>(())
            .await
            .expect("client handshake failed");

    let _server_caller_guard = server_task.await.expect("server setup failed");

    let missing_conn_id = roam_types::ConnectionId(1);
    let result = session_handle
        .close_connection(missing_conn_id, vec![])
        .await;
    assert!(
        matches!(result, Err(SessionError::Protocol(ref msg)) if msg == "connection not found"),
        "expected missing-connection protocol error, got: {result:?}"
    );
}

// r[verify connection.close]
// r[verify connection.close.semantics]
// r[verify rpc.caller.liveness.last-drop-closes-connection]
#[tokio::test]
async fn close_virtual_connection() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    // Track whether the server-side virtual connection driver has exited.
    let server_driver_exited = Arc::new(AtomicBool::new(false));
    let server_driver_exited_check = server_driver_exited.clone();

    /// An acceptor that tracks server driver exit.
    struct TrackingAcceptor {
        exited: Arc<AtomicBool>,
    }

    impl ConnectionAcceptor for TrackingAcceptor {
        fn accept(
            &self,
            _conn_id: roam_types::ConnectionId,
            peer_settings: &ConnectionSettings,
            _metadata: &[roam_types::MetadataEntry],
        ) -> Result<AcceptedConnection, Metadata<'static>> {
            let peer_parity = peer_settings.parity;
            let exited = self.exited.clone();
            Ok(AcceptedConnection {
                settings: ConnectionSettings {
                    parity: peer_parity.other(),
                    max_concurrent_requests: 64,
                },
                metadata: vec![],
                setup: Box::new(move |handle| {
                    let mut driver = Driver::new(handle, EchoHandler);
                    moire::task::spawn(
                        async move {
                            driver.run().await;
                            exited.store(true, Ordering::SeqCst);
                        }
                        .named("vconn_server_driver"),
                    );
                }),
            })
        }
    }

    let server_task = moire::task::spawn(
        async move {
            let (server_caller, _sh) = acceptor(server_conduit, test_acceptor_handshake())
                .on_connection(TrackingAcceptor {
                    exited: server_driver_exited,
                })
                .establish::<DriverCaller>(())
                .await
                .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    let (_client_caller_guard, session_handle) =
        initiator_conduit(client_conduit, test_initiator_handshake())
            .establish::<DriverCaller>(())
            .await
            .expect("client handshake failed");

    let _server_caller_guard = server_task.await.expect("server setup failed");

    // Open a virtual connection.
    let vconn_handle = session_handle
        .open_connection(
            ConnectionSettings {
                parity: Parity::Odd,
                max_concurrent_requests: 64,
            },
            vec![],
        )
        .await
        .expect("open virtual connection");

    let conn_id = vconn_handle.connection_id();
    assert!(!conn_id.is_root(), "virtual connection should not be root");

    // Set up a driver on the client side.
    let mut vconn_driver = Driver::new(vconn_handle, ());
    let caller = vconn_driver.caller();
    let caller_closed = caller.clone();
    moire::task::spawn(async move { vconn_driver.run().await }.named("vconn_client_driver"));

    // Make a call to confirm the connection works.
    let args_value: u32 = 42;
    let response = caller
        .call(RequestCall {
            method_id: MethodId(1),
            args: Payload::outgoing(&args_value),
            channels: vec![],
            metadata: Default::default(),
        })
        .await
        .expect("call should succeed before close");

    let ret_bytes = match &response.ret {
        Payload::Incoming(bytes) => *bytes,
        _ => panic!("expected incoming payload"),
    };
    let result: u32 = roam_postcard::from_slice(ret_bytes).expect("deserialize");
    assert_eq!(result, 42);

    // Close the virtual connection.
    session_handle
        .close_connection(conn_id, vec![])
        .await
        .expect("close virtual connection");

    tokio::time::timeout(std::time::Duration::from_secs(1), caller_closed.closed())
        .await
        .expect("caller closed() should resolve after virtual connection close");
    assert!(
        !caller.is_connected(),
        "caller should report disconnected after virtual connection close"
    );

    // The server-side driver should exit because `ConnectionClose` causes the
    // peer session to drop the connection slot, which drops conn_tx, causing
    // the driver's rx to return None.
    for _ in 0..20 {
        if server_driver_exited_check.load(Ordering::SeqCst) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    assert!(
        server_driver_exited_check.load(Ordering::SeqCst),
        "server-side driver should have exited after close"
    );
}

// r[verify rpc.caller.liveness.last-drop-closes-connection]
#[tokio::test]
async fn dropping_last_virtual_caller_closes_virtual_connection() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_driver_exited = Arc::new(AtomicBool::new(false));
    let server_driver_exited_check = server_driver_exited.clone();

    struct TrackingAcceptor {
        exited: Arc<AtomicBool>,
    }

    impl ConnectionAcceptor for TrackingAcceptor {
        fn accept(
            &self,
            _conn_id: roam_types::ConnectionId,
            peer_settings: &ConnectionSettings,
            _metadata: &[roam_types::MetadataEntry],
        ) -> Result<AcceptedConnection, Metadata<'static>> {
            let peer_parity = peer_settings.parity;
            let exited = self.exited.clone();
            Ok(AcceptedConnection {
                settings: ConnectionSettings {
                    parity: peer_parity.other(),
                    max_concurrent_requests: 64,
                },
                metadata: vec![],
                setup: Box::new(move |handle| {
                    let mut driver = Driver::new(handle, EchoHandler);
                    moire::task::spawn(
                        async move {
                            driver.run().await;
                            exited.store(true, Ordering::SeqCst);
                        }
                        .named("vconn_server_driver"),
                    );
                }),
            })
        }
    }

    let server_task = moire::task::spawn(
        async move {
            let (server_caller, _sh) = acceptor(server_conduit, test_acceptor_handshake())
                .on_connection(TrackingAcceptor {
                    exited: server_driver_exited,
                })
                .establish::<DriverCaller>(())
                .await
                .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    let (_client_caller_guard, session_handle) =
        initiator_conduit(client_conduit, test_initiator_handshake())
            .establish::<DriverCaller>(())
            .await
            .expect("client handshake failed");

    let _server_caller_guard = server_task.await.expect("server setup failed");

    let vconn_handle = session_handle
        .open_connection(
            ConnectionSettings {
                parity: Parity::Odd,
                max_concurrent_requests: 64,
            },
            vec![],
        )
        .await
        .expect("open virtual connection");

    let mut vconn_driver = Driver::new(vconn_handle, ());
    let vconn_caller = vconn_driver.caller();
    moire::task::spawn(async move { vconn_driver.run().await }.named("vconn_client_driver"));

    let response = vconn_caller
        .call(RequestCall {
            method_id: MethodId(1),
            args: Payload::outgoing(&11_u32),
            channels: vec![],
            metadata: Default::default(),
        })
        .await
        .expect("call should succeed before dropping virtual caller");
    let ret_bytes = match &response.ret {
        Payload::Incoming(bytes) => *bytes,
        _ => panic!("expected incoming payload in response"),
    };
    let echoed: u32 = roam_postcard::from_slice(ret_bytes).expect("deserialize response");
    assert_eq!(echoed, 11);

    drop(vconn_caller);

    for _ in 0..20 {
        if server_driver_exited_check.load(Ordering::SeqCst) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    assert!(
        server_driver_exited_check.load(Ordering::SeqCst),
        "server-side virtual driver should exit after last virtual caller drops"
    );
}

// r[verify connection.close.semantics]
// r[verify rpc.channel.close]
#[tokio::test]
async fn close_virtual_connection_closes_registered_rx_channels() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = moire::task::spawn(
        async move {
            let (server_caller, _sh) = acceptor(server_conduit, test_acceptor_handshake())
                .on_connection(EchoAcceptor)
                .establish::<DriverCaller>(())
                .await
                .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    let (_client_caller_guard, session_handle) =
        initiator_conduit(client_conduit, test_initiator_handshake())
            .establish::<DriverCaller>(())
            .await
            .expect("client handshake failed");

    let _server_caller_guard = server_task.await.expect("server setup failed");

    let vconn_handle = session_handle
        .open_connection(
            ConnectionSettings {
                parity: Parity::Odd,
                max_concurrent_requests: 64,
            },
            vec![],
        )
        .await
        .expect("open virtual connection");

    let conn_id = vconn_handle.connection_id();
    let mut vconn_driver = Driver::new(vconn_handle, ());
    let caller = vconn_driver.caller();
    moire::task::spawn(async move { vconn_driver.run().await }.named("vconn_client_driver"));

    let (_channel_id, bound_rx) = caller.create_rx(16);
    let mut rx_items = bound_rx.receiver;

    session_handle
        .close_connection(conn_id, vec![])
        .await
        .expect("close virtual connection");

    let recv_result = tokio::time::timeout(std::time::Duration::from_millis(200), rx_items.recv())
        .await
        .expect("timed out waiting for channel receiver to close");
    assert!(
        recv_result.is_none(),
        "registered Rx channel should close when virtual connection closes"
    );
}

#[tokio::test]
async fn echo_call_across_memory_link() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    // Server and client handshakes must run concurrently — both sides exchange
    // settings before either can proceed.
    let server_task = moire::task::spawn(
        async move {
            let (server_caller, _sh) = acceptor(server_conduit, test_acceptor_handshake())
                .establish::<DriverCaller>(EchoHandler)
                .await
                .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    // Set up client side (runs concurrently with server_task above).
    let (caller, _sh) = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<DriverCaller>(())
        .await
        .expect("client handshake failed");

    let _server_caller_guard = server_task.await.expect("server setup failed");

    // Make a call: serialize a u32 as the args payload.
    let args_value: u32 = 42;
    let response = caller
        .call(RequestCall {
            method_id: MethodId(1),
            args: Payload::outgoing(&args_value),
            channels: vec![],
            metadata: Default::default(),
        })
        .await
        .expect("call should succeed");

    // The echo handler sends back the same bytes. Deserialize the response.
    let ret_bytes = match &response.ret {
        Payload::Incoming(bytes) => *bytes,
        _ => panic!("expected incoming payload in response"),
    };
    let result: u32 = roam_postcard::from_slice(ret_bytes).expect("deserialize response");
    assert_eq!(result, 42);
}

#[tokio::test]
async fn buffers_inbound_channel_items_until_rx_is_registered() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = moire::task::spawn(
        async move {
            let (server_caller, _sh) = acceptor(server_conduit, test_acceptor_handshake())
                .establish::<DriverCaller>(())
                .await
                .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    let (client_caller, _sh) = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<DriverCaller>(())
        .await
        .expect("client handshake failed");
    let client_sender = client_caller.connection_sender().clone();

    let server_caller = server_task.await.expect("server setup failed");

    let channel_id = ChannelId(99);
    let value = 123_u32;
    client_sender
        .send(crate::session::ConnectionMessage::Channel(ChannelMessage {
            id: channel_id,
            body: ChannelBody::Item(ChannelItem {
                item: Payload::outgoing(&value),
            }),
        }))
        .await
        .expect("send channel item");

    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let mut rx = server_caller.register_rx_channel(channel_id, 16).receiver;
    let msg = tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv())
        .await
        .expect("timed out waiting for buffered channel item")
        .expect("channel receiver closed unexpectedly");

    let IncomingChannelMessage::Item(item) = msg else {
        panic!("expected buffered item");
    };
    let bytes = match item.item {
        Payload::Incoming(bytes) => bytes,
        _ => panic!("expected incoming payload"),
    };
    let decoded: u32 = roam_postcard::from_slice(bytes).expect("deserialize buffered item");
    assert_eq!(decoded, 123);
}

#[tokio::test]
async fn grant_credit_unblocks_driver_created_tx_channel() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = moire::task::spawn(
        async move {
            let (server_caller, _sh) = acceptor(server_conduit, test_acceptor_handshake())
                .establish::<DriverCaller>(())
                .await
                .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    let (client_caller, _sh) = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<DriverCaller>(())
        .await
        .expect("client handshake failed");
    let client_sender = client_caller.connection_sender().clone();

    let server_caller = server_task.await.expect("server setup failed");
    let (channel_id, sink) = server_caller.create_tx_channel(0);

    let send_task = moire::task::spawn(async move {
        let value = 42_u32;
        sink.send_payload(Payload::outgoing(&value)).await
    });

    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    assert!(
        !send_task.is_finished(),
        "send should block when initial credit is zero"
    );

    client_sender
        .send(crate::session::ConnectionMessage::Channel(ChannelMessage {
            id: channel_id,
            body: ChannelBody::GrantCredit(ChannelGrantCredit { additional: 1 }),
        }))
        .await
        .expect("send grant credit");

    let send_result = tokio::time::timeout(std::time::Duration::from_millis(200), send_task)
        .await
        .expect("timed out waiting for send to unblock")
        .expect("send task join");
    assert!(
        send_result.is_ok(),
        "send should succeed after credit grant"
    );
}

#[tokio::test]
async fn buffered_close_before_registration_keeps_channel_terminal() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = moire::task::spawn(
        async move {
            let (server_caller, _sh) = acceptor(server_conduit, test_acceptor_handshake())
                .establish::<DriverCaller>(())
                .await
                .expect("server handshake failed");
            server_caller
        }
        .named("server_setup"),
    );

    let (client_caller, _sh) = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<DriverCaller>(())
        .await
        .expect("client handshake failed");
    let client_sender = client_caller.connection_sender().clone();

    let server_caller = server_task.await.expect("server setup failed");
    let channel_id = ChannelId(77);

    client_sender
        .send(crate::session::ConnectionMessage::Channel(ChannelMessage {
            id: channel_id,
            body: ChannelBody::Close(ChannelClose {
                metadata: Metadata::default(),
            }),
        }))
        .await
        .expect("send buffered close");

    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let mut rx = server_caller.register_rx_channel(channel_id, 16).receiver;
    let close = tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv())
        .await
        .expect("timed out waiting for buffered close")
        .expect("channel receiver closed before buffered close");
    assert!(
        matches!(close, IncomingChannelMessage::Close(_)),
        "expected buffered close first"
    );

    let value = 999_u32;
    client_sender
        .send(crate::session::ConnectionMessage::Channel(ChannelMessage {
            id: channel_id,
            body: ChannelBody::Item(ChannelItem {
                item: Payload::outgoing(&value),
            }),
        }))
        .await
        .expect("send post-close item");

    let next = tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv())
        .await
        .expect("timed out waiting for channel termination");
    assert!(
        next.is_none(),
        "channel should be terminal after buffered close"
    );
}

#[tokio::test]
async fn unsolicited_response_id_is_ignored_and_does_not_break_calls() {
    let (client_conduit, server_conduit) = message_conduit_pair();

    let server_task = moire::task::spawn(
        async move {
            let (server_caller, _sh) = acceptor(server_conduit, test_acceptor_handshake())
                .establish::<DriverCaller>(EchoHandler)
                .await
                .expect("server handshake failed");
            (server_caller.connection_sender().clone(), server_caller)
        }
        .named("server_setup"),
    );

    let (caller, _sh) = initiator_conduit(client_conduit, test_initiator_handshake())
        .establish::<DriverCaller>(())
        .await
        .expect("client handshake failed");

    let (server_sender, _server_caller_guard) = server_task.await.expect("server setup failed");

    server_sender
        .send(crate::session::ConnectionMessage::Request(RequestMessage {
            id: roam_types::RequestId(9999),
            body: RequestBody::Response(RequestResponse {
                ret: Payload::outgoing(&123_u32),
                channels: vec![],
                metadata: Default::default(),
            }),
        }))
        .await
        .expect("send unsolicited response");

    let args_value: u32 = 42;
    let response = caller
        .call(RequestCall {
            method_id: MethodId(1),
            args: Payload::outgoing(&args_value),
            channels: vec![],
            metadata: Default::default(),
        })
        .await
        .expect("call should still succeed after unsolicited response");

    let ret_bytes = match &response.ret {
        Payload::Incoming(bytes) => *bytes,
        _ => panic!("expected incoming payload"),
    };
    let result: u32 = roam_postcard::from_slice(ret_bytes).expect("deserialize response");
    assert_eq!(result, 42);
}

#[tokio::test]
async fn proxy_connections_forwards_calls_without_service_specific_proxy_code() {
    let (host_a_conduit, guest_a_conduit) = message_conduit_pair();
    let (host_b_conduit, guest_b_conduit) = message_conduit_pair();

    struct GuestBAcceptor;
    impl ConnectionAcceptor for GuestBAcceptor {
        fn accept(
            &self,
            _conn_id: roam_types::ConnectionId,
            peer_settings: &ConnectionSettings,
            _metadata: &[roam_types::MetadataEntry],
        ) -> Result<AcceptedConnection, Metadata<'static>> {
            Ok(AcceptedConnection {
                settings: ConnectionSettings {
                    parity: peer_settings.parity.other(),
                    max_concurrent_requests: 64,
                },
                metadata: vec![],
                setup: Box::new(|handle| {
                    let mut driver = Driver::new(handle, EchoHandler);
                    moire::task::spawn(async move { driver.run().await }.named("guest_b_vconn"));
                }),
            })
        }
    }

    struct ProxyHostAcceptor {
        upstream_session: SessionHandle,
    }
    impl ConnectionAcceptor for ProxyHostAcceptor {
        fn accept(
            &self,
            _conn_id: roam_types::ConnectionId,
            peer_settings: &ConnectionSettings,
            _metadata: &[roam_types::MetadataEntry],
        ) -> Result<AcceptedConnection, Metadata<'static>> {
            let upstream_session = self.upstream_session.clone();
            Ok(AcceptedConnection {
                settings: ConnectionSettings {
                    parity: peer_settings.parity.other(),
                    max_concurrent_requests: 64,
                },
                metadata: vec![],
                setup: Box::new(move |incoming| {
                    moire::task::spawn(
                        async move {
                            let upstream = upstream_session
                                .open_connection(
                                    ConnectionSettings {
                                        parity: Parity::Odd,
                                        max_concurrent_requests: 64,
                                    },
                                    vec![],
                                )
                                .await
                                .expect("host->guest-b open_connection");
                            proxy_connections(incoming, upstream).await;
                        }
                        .named("host_proxy_vconn"),
                    );
                }),
            })
        }
    }

    let guest_b_task = moire::task::spawn(
        async move {
            let (guard, _) = acceptor(guest_b_conduit, test_acceptor_handshake())
                .on_connection(GuestBAcceptor)
                .establish::<DriverCaller>(())
                .await
                .expect("guest-b establish");
            let _guard = guard;
            std::future::pending::<()>().await;
        }
        .named("guest_b_root"),
    );

    let (_host_to_b_guard, host_to_b_session) =
        initiator_conduit(host_b_conduit, test_initiator_handshake())
            .establish::<DriverCaller>(())
            .await
            .expect("host<->guest-b establish");

    let host_for_a_task = moire::task::spawn(
        async move {
            let (guard, _) = acceptor(host_a_conduit, test_acceptor_handshake())
                .on_connection(ProxyHostAcceptor {
                    upstream_session: host_to_b_session,
                })
                .establish::<DriverCaller>(())
                .await
                .expect("host<->guest-a establish");
            let _guard = guard;
            std::future::pending::<()>().await;
        }
        .named("host_for_guest_a_root"),
    );

    let (_guest_a_root_guard, guest_a_session) =
        initiator_conduit(guest_a_conduit, test_initiator_handshake())
            .establish::<DriverCaller>(())
            .await
            .expect("guest-a<->host establish");

    let proxy_conn = guest_a_session
        .open_connection(
            ConnectionSettings {
                parity: Parity::Odd,
                max_concurrent_requests: 64,
            },
            vec![],
        )
        .await
        .expect("guest-a open proxy connection");
    let proxy_conn_id = proxy_conn.connection_id();

    let mut proxy_driver = Driver::new(proxy_conn, ());
    let proxy_caller = proxy_driver.caller();
    let proxy_driver_task =
        moire::task::spawn(async move { proxy_driver.run().await }.named("guest_a_proxy_driver"));

    let args_value: u32 = 777;
    let response = proxy_caller
        .call(RequestCall {
            method_id: MethodId(1),
            args: Payload::outgoing(&args_value),
            channels: vec![],
            metadata: Default::default(),
        })
        .await
        .expect("proxied call should succeed");
    let ret_bytes = match &response.ret {
        Payload::Incoming(bytes) => *bytes,
        _ => panic!("expected incoming payload"),
    };
    let result: u32 = roam_postcard::from_slice(ret_bytes).expect("deserialize proxied response");
    assert_eq!(result, args_value);

    guest_a_session
        .close_connection(proxy_conn_id, vec![])
        .await
        .expect("close proxy connection");

    proxy_driver_task.abort();
    guest_b_task.abort();
    host_for_a_task.abort();
}
