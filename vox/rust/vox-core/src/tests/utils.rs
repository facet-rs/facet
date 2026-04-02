use std::collections::VecDeque;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

use facet::Facet;
use moire::sync::mpsc;
use vox_types::{
    Backing, Conduit, ConduitRx, ConnectionSettings, Handler, HandshakeResult, Link, LinkRx,
    LinkTx, LinkTxPermit, Message, MessageFamily, MessagePayload, MethodId, Parity, Payload,
    ReplySink, RequestCall, RequestResponse, RetryPolicy, SelfRef, SessionResumeKey, SessionRole,
    Tx, WriteSlot, metadata_operation_id,
};

use crate::{
    Attachment, BareConduit, DriverReplySink, InMemoryOperationStore, LinkSource, OperationStore,
    memory_link_pair,
};

pub(crate) fn test_resume_key() -> SessionResumeKey {
    SessionResumeKey([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16])
}

pub(crate) fn test_acceptor_handshake() -> HandshakeResult {
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
        peer_metadata: vec![],
    }
}

pub(crate) fn test_initiator_handshake() -> HandshakeResult {
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
        peer_metadata: vec![],
    }
}

pub(crate) type MessageConduit = BareConduit<MessageFamily, crate::MemoryLink>;

pub(crate) fn message_conduit_pair() -> (MessageConduit, MessageConduit) {
    let (a, b) = memory_link_pair(64);
    (BareConduit::new(a), BareConduit::new(b))
}

pub(crate) struct BreakableLink {
    tx: mpsc::Sender<Option<Vec<u8>>>,
    rx: mpsc::Receiver<Option<Vec<u8>>>,
}

#[derive(Clone)]
pub(crate) struct BreakHandle {
    tx: mpsc::Sender<Option<Vec<u8>>>,
}

pub(crate) fn breakable_link_pair(
    buffer: usize,
) -> (BreakableLink, BreakHandle, BreakableLink, BreakHandle) {
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
    pub(crate) async fn close(&self) {
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
pub(crate) struct BreakableLinkTx {
    tx: mpsc::Sender<Option<Vec<u8>>>,
}

pub(crate) struct BreakableLinkTxPermit {
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

pub(crate) struct BreakableWriteSlot {
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

pub(crate) struct BreakableLinkRx {
    rx: mpsc::Receiver<Option<Vec<u8>>>,
}

#[derive(Debug)]
pub(crate) struct BreakableLinkRxError;

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

pub(crate) struct TestLinkSource {
    attachments: VecDeque<Attachment<BreakableLink>>,
}

impl TestLinkSource {
    pub(crate) fn new(attachments: impl IntoIterator<Item = Attachment<BreakableLink>>) -> Self {
        Self {
            attachments: attachments.into_iter().collect(),
        }
    }
}

impl LinkSource for TestLinkSource {
    type Link = BreakableLink;

    async fn next_link(&mut self) -> std::io::Result<Attachment<Self::Link>> {
        self.attachments.pop_front().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "test link source exhausted",
            )
        })
    }
}

/// Conduit wrapper used by keepalive tests: drops inbound Pong messages.
pub(crate) struct DropPongConduit<C> {
    inner: C,
}

impl<C> DropPongConduit<C> {
    pub(crate) fn new(inner: C) -> Self {
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

pub(crate) struct DropPongRx<Rx> {
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
pub(crate) struct EchoHandler;

impl Handler<DriverReplySink> for EchoHandler {
    async fn handle(
        &self,
        call: SelfRef<RequestCall<'static>>,
        reply: DriverReplySink,
        _schemas: std::sync::Arc<vox_types::SchemaRecvTracker>,
    ) {
        let args_bytes = match &call.args {
            Payload::PostcardBytes(bytes) => *bytes,
            _ => panic!("expected incoming payload"),
        };

        let result: u32 = vox_postcard::from_slice(args_bytes).expect("deserialize args");
        reply
            .send_reply(RequestResponse {
                ret: Payload::outgoing(&result),
                schemas: Default::default(),
                metadata: Default::default(),
            })
            .await;
    }
}

/// A handler that blocks forever until its task is cancelled.
/// Tracks whether cancellation occurred via a drop guard.
pub(crate) struct BlockingHandler {
    pub(crate) was_cancelled: Arc<AtomicBool>,
    pub(crate) retry: RetryPolicy,
}

impl Handler<DriverReplySink> for BlockingHandler {
    fn retry_policy(&self, _method_id: MethodId) -> RetryPolicy {
        self.retry
    }

    async fn handle(
        &self,
        _call: SelfRef<RequestCall<'static>>,
        reply: DriverReplySink,
        _schemas: std::sync::Arc<vox_types::SchemaRecvTracker>,
    ) {
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

pub(crate) struct PersistentReplyingHandler {
    pub(crate) was_cancelled: Arc<AtomicBool>,
    pub(crate) release: Arc<tokio::sync::Notify>,
}

pub(crate) struct ResumableReplyingHandler {
    pub(crate) started: Arc<tokio::sync::Notify>,
    pub(crate) release: Arc<tokio::sync::Notify>,
}

pub(crate) struct RetryAfterResumeHandler {
    pub(crate) retry: RetryPolicy,
    pub(crate) runs: Arc<AtomicUsize>,
    pub(crate) first_started: Arc<tokio::sync::Notify>,
    pub(crate) drop_first: Arc<tokio::sync::Notify>,
}

pub(crate) struct OperationIdHandler;

impl Handler<DriverReplySink> for OperationIdHandler {
    async fn handle(
        &self,
        call: SelfRef<RequestCall<'static>>,
        reply: DriverReplySink,
        _schemas: std::sync::Arc<vox_types::SchemaRecvTracker>,
    ) {
        let operation_id = metadata_operation_id(&call.metadata).expect("operation id metadata");
        reply
            .send_reply(RequestResponse {
                ret: Payload::outgoing(&operation_id.0),
                schemas: Default::default(),
                metadata: Default::default(),
            })
            .await;
    }
}

pub(crate) struct ReplayHandler {
    pub(crate) runs: Arc<std::sync::atomic::AtomicUsize>,
    pub(crate) release: Arc<tokio::sync::Notify>,
}

pub(crate) struct CountingOperationStore {
    inner: InMemoryOperationStore,
    pub(crate) admits: AtomicUsize,
}

impl CountingOperationStore {
    pub(crate) fn new() -> Self {
        Self {
            inner: InMemoryOperationStore::default(),
            admits: AtomicUsize::new(0),
        }
    }
}

impl OperationStore for CountingOperationStore {
    fn admit(&self, operation_id: vox_types::OperationId) {
        self.admits.fetch_add(1, Ordering::SeqCst);
        self.inner.admit(operation_id)
    }

    fn lookup(&self, operation_id: vox_types::OperationId) -> crate::OperationState {
        self.inner.lookup(operation_id)
    }

    fn get_sealed(&self, operation_id: vox_types::OperationId) -> Option<crate::SealedResponse> {
        self.inner.get_sealed(operation_id)
    }

    fn seal(
        &self,
        operation_id: vox_types::OperationId,
        response: &vox_types::PostcardPayload,
        root_type: &vox_types::TypeRef,
        registry: &vox_types::SchemaRegistry,
    ) {
        self.inner.seal(operation_id, response, root_type, registry)
    }

    fn remove(&self, operation_id: vox_types::OperationId) {
        self.inner.remove(operation_id)
    }

    fn schema_source(&self) -> &dyn vox_types::SchemaSource {
        self.inner.schema_source()
    }
}

impl Handler<DriverReplySink> for ReplayHandler {
    fn retry_policy(&self, _method_id: MethodId) -> RetryPolicy {
        RetryPolicy::PERSIST
    }

    async fn handle(
        &self,
        call: SelfRef<RequestCall<'static>>,
        reply: DriverReplySink,
        _schemas: std::sync::Arc<vox_types::SchemaRecvTracker>,
    ) {
        self.runs.fetch_add(1, Ordering::SeqCst);
        self.release.notified().await;
        let args_bytes = match &call.args {
            Payload::PostcardBytes(bytes) => *bytes,
            _ => panic!("expected incoming payload"),
        };
        let result: u32 = vox_postcard::from_slice(args_bytes).expect("deserialize args");
        reply
            .send_reply(RequestResponse {
                ret: Payload::outgoing(&result),
                schemas: Default::default(),
                metadata: Default::default(),
            })
            .await;
    }
}

impl Handler<DriverReplySink> for PersistentReplyingHandler {
    fn retry_policy(&self, _method_id: MethodId) -> RetryPolicy {
        RetryPolicy::PERSIST
    }

    async fn handle(
        &self,
        _call: SelfRef<RequestCall<'static>>,
        reply: DriverReplySink,
        _schemas: std::sync::Arc<vox_types::SchemaRecvTracker>,
    ) {
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
                schemas: Default::default(),
                metadata: Default::default(),
            })
            .await;
    }
}

impl Handler<DriverReplySink> for ResumableReplyingHandler {
    fn retry_policy(&self, _method_id: MethodId) -> RetryPolicy {
        RetryPolicy::PERSIST
    }

    async fn handle(
        &self,
        call: SelfRef<RequestCall<'static>>,
        reply: DriverReplySink,
        _schemas: std::sync::Arc<vox_types::SchemaRecvTracker>,
    ) {
        self.started.notify_waiters();
        self.release.notified().await;

        let args_bytes = match &call.args {
            Payload::PostcardBytes(bytes) => *bytes,
            _ => panic!("expected incoming payload"),
        };
        let result: u32 = vox_postcard::from_slice(args_bytes).expect("deserialize args");
        reply
            .send_reply(RequestResponse {
                ret: Payload::outgoing(&result),
                schemas: Default::default(),
                metadata: Default::default(),
            })
            .await;
    }
}

impl Handler<DriverReplySink> for RetryAfterResumeHandler {
    fn retry_policy(&self, _method_id: MethodId) -> RetryPolicy {
        self.retry
    }

    async fn handle(
        &self,
        call: SelfRef<RequestCall<'static>>,
        reply: DriverReplySink,
        _schemas: std::sync::Arc<vox_types::SchemaRecvTracker>,
    ) {
        let run = self.runs.fetch_add(1, Ordering::SeqCst);
        if run == 0 {
            self.first_started.notify_waiters();
            self.drop_first.notified().await;
            return;
        }

        let args_bytes = match &call.args {
            Payload::PostcardBytes(bytes) => *bytes,
            _ => panic!("expected incoming payload"),
        };
        let result: u32 = vox_postcard::from_slice(args_bytes).expect("deserialize args");
        reply
            .send_reply(RequestResponse {
                ret: Payload::outgoing(&result),
                schemas: Default::default(),
                metadata: Default::default(),
            })
            .await;
    }
}

use crate::session::{ConnectionAcceptor, ConnectionRequest, PendingConnection};

pub(crate) struct EchoAcceptor;

impl ConnectionAcceptor for EchoAcceptor {
    fn accept(
        &self,
        _request: &ConnectionRequest,
        connection: PendingConnection,
    ) -> Result<(), vox_types::Metadata<'static>> {
        connection.handle_with(EchoHandler);
        Ok(())
    }
}

/// An acceptor that rejects every connection.
pub(crate) struct RejectAcceptor;

impl ConnectionAcceptor for RejectAcceptor {
    fn accept(
        &self,
        _request: &ConnectionRequest,
        _connection: PendingConnection,
    ) -> Result<(), vox_types::Metadata<'static>> {
        Err(vec![])
    }
}

#[derive(Facet)]
pub(crate) struct SubscribeArgs {
    pub(crate) updates: Tx<u32>,
}
