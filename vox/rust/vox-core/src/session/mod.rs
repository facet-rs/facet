use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use moire::sync::mpsc;
use tokio::sync::watch;
use tracing::{trace, warn};
use vox_types::{
    BoxFut, ChannelMessage, Conduit, ConduitRx, ConduitTx, ConduitTxPermit, ConnectionAccept,
    ConnectionClose, ConnectionId, ConnectionOpen, ConnectionReject, ConnectionSettings, Handler,
    HandshakeResult, IdAllocator, MaybeSend, MaybeSync, Message, MessageFamily, MessagePayload,
    Metadata, Parity, RequestBody, RequestId, RequestMessage, RequestResponse, SelfRef,
    SessionResumeKey, SessionRole,
};

mod builders;
pub use builders::*;

/// Session-level protocol keepalive configuration.
#[derive(Debug, Clone, Copy)]
pub struct SessionKeepaliveConfig {
    pub ping_interval: Duration,
    pub pong_timeout: Duration,
}

// ---------------------------------------------------------------------------
// Connection acceptor trait
// ---------------------------------------------------------------------------

/// Callback for accepting or rejecting inbound virtual connections.
///
/// Handles incoming connections (both root and virtual).
///
/// Called when a peer opens a connection. The acceptor receives the peer's
/// metadata (including `vox-service` for service routing) and returns either
/// an [`AcceptedConnection`] or rejection metadata.
///
/// Metadata wrapper with typed getters for well-known `vox-*` keys.
pub struct ConnectionRequest<'a> {
    metadata: &'a [vox_types::MetadataEntry<'a>],
}

impl<'a> ConnectionRequest<'a> {
    pub fn new(metadata: &'a [vox_types::MetadataEntry<'a>]) -> Self {
        Self { metadata }
    }

    /// The requested service name (`vox-service` metadata key).
    pub fn service(&self) -> Option<&str> {
        vox_types::metadata_get_str(self.metadata, "vox-service")
    }

    /// The transport type (`vox-transport` metadata key).
    pub fn transport(&self) -> Option<&str> {
        vox_types::metadata_get_str(self.metadata, "vox-transport")
    }

    /// The peer address (`vox-peer-addr` metadata key).
    pub fn peer_addr(&self) -> Option<&str> {
        vox_types::metadata_get_str(self.metadata, "vox-peer-addr")
    }

    /// Whether this is a root or virtual connection.
    pub fn is_root(&self) -> bool {
        vox_types::metadata_get_str(self.metadata, "vox-connection-kind") == Some("root")
    }

    /// Whether this is a virtual connection.
    pub fn is_virtual(&self) -> bool {
        vox_types::metadata_get_str(self.metadata, "vox-connection-kind") == Some("virtual")
    }

    /// Look up a string value by key.
    pub fn get_str(&self, key: &str) -> Option<&str> {
        vox_types::metadata_get_str(self.metadata, key)
    }

    /// Look up a u64 value by key.
    pub fn get_u64(&self, key: &str) -> Option<u64> {
        vox_types::metadata_get_u64(self.metadata, key)
    }

    /// Access the raw metadata entries.
    pub fn metadata(&self) -> &[vox_types::MetadataEntry<'a>] {
        self.metadata
    }
}

/// A connection that has been opened but not yet accepted.
///
/// The acceptor receives this and decides its fate by calling one of:
/// - `handle_with(handler)` — run a Driver with this handler (common case)
/// - `proxy_to(other_handle)` — pipe messages to/from another connection
/// - `into_handle()` — take the raw ConnectionHandle for custom use
pub struct PendingConnection {
    handle: Option<ConnectionHandle>,
    caller_slot: Option<Arc<std::sync::Mutex<Option<crate::Caller>>>>,
    operation_store: Option<Arc<dyn crate::OperationStore>>,
}

impl PendingConnection {
    fn new(handle: ConnectionHandle) -> Self {
        Self {
            handle: Some(handle),
            caller_slot: None,
            operation_store: None,
        }
    }

    /// Create a PendingConnection that captures the Caller when handle_with is called.
    fn with_caller_slot(
        handle: ConnectionHandle,
        caller_slot: Arc<std::sync::Mutex<Option<crate::Caller>>>,
        operation_store: Option<Arc<dyn crate::OperationStore>>,
    ) -> Self {
        Self {
            handle: Some(handle),
            caller_slot: Some(caller_slot),
            operation_store,
        }
    }

    /// Accept this connection and run a Driver with the given handler.
    pub fn handle_with(mut self, handler: impl Handler<crate::DriverReplySink> + 'static) {
        let handle = self
            .handle
            .take()
            .expect("PendingConnection already consumed");
        let conn_id = handle.connection_id();
        trace!(%conn_id, "PendingConnection::handle_with: creating driver");
        let mut driver = match self.operation_store.take() {
            Some(store) => crate::Driver::with_operation_store(handle, handler, store),
            None => crate::Driver::new(handle, handler),
        };
        if let Some(slot) = &self.caller_slot {
            let caller = crate::Caller::new(driver.caller());
            *slot.lock().unwrap() = Some(caller);
        }
        #[cfg(not(target_arch = "wasm32"))]
        tokio::spawn(async move {
            trace!(%conn_id, "PendingConnection driver starting");
            driver.run().await;
            trace!(%conn_id, "PendingConnection driver exited");
        });
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move { driver.run().await });
    }

    /// Accept this connection and proxy all traffic to/from another connection.
    pub fn proxy_to(mut self, other: ConnectionHandle) {
        let handle = self
            .handle
            .take()
            .expect("PendingConnection already consumed");
        #[cfg(not(target_arch = "wasm32"))]
        tokio::spawn(async move {
            let _ = proxy_connections(handle, other).await;
        });
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move {
            let _ = proxy_connections(handle, other).await;
        });
    }

    /// Take the raw ConnectionHandle for custom use.
    pub fn into_handle(mut self) -> ConnectionHandle {
        self.handle
            .take()
            .expect("PendingConnection already consumed")
    }
}

impl Drop for PendingConnection {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let conn_id = handle.connection_id();
            warn!(%conn_id, "PendingConnection dropped without being consumed — closing connection");
            if let Some(tx) = handle.control_tx.as_ref() {
                let _ = send_drop_control(tx, DropControlRequest::Close(conn_id));
            }
        }
    }
}

// r[impl rpc.virtual-connection.accept]
pub trait ConnectionAcceptor: Send + Sync + 'static {
    fn accept(
        &self,
        request: &ConnectionRequest,
        connection: PendingConnection,
    ) -> Result<(), Metadata<'static>>;
}

/// Any `Handler<DriverReplySink>` is automatically a `ConnectionAcceptor`.
impl<H> ConnectionAcceptor for H
where
    H: Handler<crate::DriverReplySink> + Clone + Send + 'static,
{
    fn accept(
        &self,
        _request: &ConnectionRequest,
        connection: PendingConnection,
    ) -> Result<(), Metadata<'static>> {
        connection.handle_with(self.clone());
        Ok(())
    }
}

/// Wrapper that turns a closure into a `ConnectionAcceptor`.
pub struct AcceptorFn<F>(pub F);

impl<F> ConnectionAcceptor for AcceptorFn<F>
where
    F: Fn(&ConnectionRequest, PendingConnection) -> Result<(), Metadata<'static>>
        + Send
        + Sync
        + 'static,
{
    fn accept(
        &self,
        request: &ConnectionRequest,
        connection: PendingConnection,
    ) -> Result<(), Metadata<'static>> {
        (self.0)(request, connection)
    }
}

/// Create a `ConnectionAcceptor` from a closure.
pub fn acceptor_fn<F>(f: F) -> AcceptorFn<F>
where
    F: Fn(&ConnectionRequest, PendingConnection) -> Result<(), Metadata<'static>>
        + Send
        + Sync
        + 'static,
{
    AcceptorFn(f)
}

// ---------------------------------------------------------------------------
// Open/close request types (from SessionHandle → run loop)
// ---------------------------------------------------------------------------

struct OpenRequest {
    settings: ConnectionSettings,
    metadata: Metadata<'static>,
    result_tx: moire::sync::oneshot::Sender<Result<ConnectionHandle, SessionError>>,
}

struct CloseRequest {
    conn_id: ConnectionId,
    metadata: Metadata<'static>,
    result_tx: moire::sync::oneshot::Sender<Result<(), SessionError>>,
}

struct ResumeRequest {
    tx: Arc<dyn DynConduitTx>,
    rx: Box<dyn DynConduitRx>,
    handshake_result: HandshakeResult,
    result_tx: moire::sync::oneshot::Sender<Result<(), SessionError>>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum DropControlRequest {
    Shutdown,
    Close(ConnectionId),
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum FailureDisposition {
    Cancelled,
    Indeterminate,
}

#[cfg(not(target_arch = "wasm32"))]
fn send_drop_control(
    tx: &mpsc::UnboundedSender<DropControlRequest>,
    req: DropControlRequest,
) -> Result<(), ()> {
    tx.send(req).map_err(|_| ())
}

#[cfg(target_arch = "wasm32")]
fn send_drop_control(
    tx: &mpsc::UnboundedSender<DropControlRequest>,
    req: DropControlRequest,
) -> Result<(), ()> {
    tx.try_send(req).map_err(|_| ())
}

// ---------------------------------------------------------------------------
// SessionHandle — cloneable handle for opening/closing virtual connections
// ---------------------------------------------------------------------------

/// Cloneable handle for opening and closing virtual connections.
///
/// Returned by the session builder alongside the `Session` and root
/// `ConnectionHandle`. The session's `run()` loop must be running
/// concurrently for requests to be processed.
// r[impl rpc.virtual-connection.open]
#[derive(Clone)]
pub struct SessionHandle {
    open_tx: mpsc::Sender<OpenRequest>,
    close_tx: mpsc::Sender<CloseRequest>,
    resume_tx: mpsc::Sender<ResumeRequest>,
    control_tx: mpsc::UnboundedSender<DropControlRequest>,
    resume_key: Option<SessionResumeKey>,
}

impl SessionHandle {
    /// Open a typed virtual connection on the session.
    ///
    /// Sends `vox-service` metadata automatically from the client's
    /// `SERVICE_NAME`. Creates a `Driver` and spawns it, returning
    /// a ready-to-use typed client.
    pub async fn open<Client: crate::FromVoxSession>(
        &self,
        settings: ConnectionSettings,
    ) -> Result<Client, SessionError> {
        use crate::{Caller, Driver};
        use vox_types::{Handler, MetadataEntry, MetadataFlags, MetadataValue};

        let mut metadata: Metadata<'static> = vec![];
        if let Some(name) = Client::SERVICE_NAME {
            metadata.push(MetadataEntry {
                key: crate::session::builders::VOX_SERVICE_METADATA_KEY.into(),
                value: MetadataValue::String(name.into()),
                flags: MetadataFlags::NONE,
            });
        }
        let handle = self.open_connection(settings, metadata).await?;
        let mut driver = Driver::new(handle, ());
        let caller = Caller::new(driver.caller());
        #[cfg(not(target_arch = "wasm32"))]
        tokio::spawn(async move { driver.run().await });
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move { driver.run().await });
        Ok(Client::from_vox_session(caller, None))
    }

    /// Open a new virtual connection on the session.
    ///
    /// Allocates a connection ID, sends `ConnectionOpen` to the peer, and
    /// waits for `ConnectionAccept` or `ConnectionReject`. The session's
    /// `run()` loop processes the response and completes the returned future.
    // r[impl connection.open]
    pub async fn open_connection(
        &self,
        settings: ConnectionSettings,
        metadata: Metadata<'static>,
    ) -> Result<ConnectionHandle, SessionError> {
        let (result_tx, result_rx) = moire::sync::oneshot::channel("session.open_result");
        self.open_tx
            .send(OpenRequest {
                settings,
                metadata,
                result_tx,
            })
            .await
            .map_err(|_| SessionError::Protocol("session closed".into()))?;
        result_rx
            .await
            .map_err(|_| SessionError::Protocol("session closed".into()))?
    }

    /// Close a virtual connection.
    ///
    /// Sends `ConnectionClose` to the peer and removes the connection slot.
    /// After this returns, no further messages will be routed to the
    /// connection's driver.
    // r[impl connection.close]
    pub async fn close_connection(
        &self,
        conn_id: ConnectionId,
        metadata: Metadata<'static>,
    ) -> Result<(), SessionError> {
        let (result_tx, result_rx) = moire::sync::oneshot::channel("session.close_result");
        self.close_tx
            .send(CloseRequest {
                conn_id,
                metadata,
                result_tx,
            })
            .await
            .map_err(|_| SessionError::Protocol("session closed".into()))?;
        result_rx
            .await
            .map_err(|_| SessionError::Protocol("session closed".into()))?
    }

    pub async fn resume<I: crate::IntoConduit>(
        &self,
        into_conduit: I,
        handshake_result: HandshakeResult,
    ) -> Result<(), SessionError>
    where
        I::Conduit: Conduit<Msg = MessageFamily> + 'static,
        <I::Conduit as Conduit>::Tx: MaybeSend + MaybeSync + 'static,
        for<'p> <<I::Conduit as Conduit>::Tx as ConduitTx>::Permit<'p>: MaybeSend,
        <I::Conduit as Conduit>::Rx: MaybeSend + 'static,
    {
        let (tx, rx) = into_conduit.into_conduit().split();
        self.resume_parts(Arc::new(tx), Box::new(rx), handshake_result)
            .await
    }

    pub(crate) async fn resume_parts(
        &self,
        tx: Arc<dyn DynConduitTx>,
        rx: Box<dyn DynConduitRx>,
        handshake_result: HandshakeResult,
    ) -> Result<(), SessionError> {
        let (result_tx, result_rx) = moire::sync::oneshot::channel("session.resume_result");
        self.resume_tx
            .send(ResumeRequest {
                tx,
                rx,
                handshake_result,
                result_tx,
            })
            .await
            .map_err(|_| SessionError::Protocol("session closed".into()))?;
        result_rx
            .await
            .map_err(|_| SessionError::Protocol("session closed".into()))?
    }

    /// Returns the session resume key, if the session is resumable.
    pub fn resume_key(&self) -> Option<&SessionResumeKey> {
        self.resume_key.as_ref()
    }

    /// Request shutdown of the entire session (root + all virtual connections).
    pub fn shutdown(&self) -> Result<(), SessionError> {
        send_drop_control(&self.control_tx, DropControlRequest::Shutdown)
            .map_err(|_| SessionError::Protocol("session closed".into()))
    }
}

// ---------------------------------------------------------------------------
// Session
// ---------------------------------------------------------------------------

/// Session state machine.
// r[impl session]
// r[impl rpc.one-service-per-connection]
pub struct Session {
    /// Conduit receiver
    rx: Box<dyn DynConduitRx>,

    // r[impl session.role]
    role: SessionRole,

    /// Our local parity — determines which connection IDs we allocate.
    // r[impl session.parity]
    parity: Parity,

    /// Shared core (for sending) — also held by all ConnectionSenders.
    sess_core: Arc<SessionCore>,
    peer_supports_retry: bool,
    local_root_settings: ConnectionSettings,
    peer_root_settings: Option<ConnectionSettings>,
    resumable: bool,
    session_resume_key: Option<SessionResumeKey>,

    /// Connection state (active, pending inbound, pending outbound).
    conns: BTreeMap<ConnectionId, ConnectionSlot>,
    /// Whether the root connection was internally closed because all root callers dropped.
    root_closed_internal: bool,

    /// Allocator for outbound virtual connection IDs (uses session parity).
    conn_ids: IdAllocator<ConnectionId>,

    /// Callback for accepting inbound virtual connections.
    on_connection: Option<Arc<dyn ConnectionAcceptor>>,

    /// Receiver for open requests from SessionHandle.
    open_rx: mpsc::Receiver<OpenRequest>,

    /// Receiver for close requests from SessionHandle.
    close_rx: mpsc::Receiver<CloseRequest>,

    /// Receiver for resume requests from SessionHandle.
    resume_rx: mpsc::Receiver<ResumeRequest>,

    /// Sender/receiver for drop-driven session/connection control requests.
    control_tx: mpsc::UnboundedSender<DropControlRequest>,
    control_rx: mpsc::UnboundedReceiver<DropControlRequest>,

    /// Optional proactive keepalive runtime config for connection ID 0.
    keepalive: Option<SessionKeepaliveConfig>,
    resume_notifier: watch::Sender<u64>,
    recoverer: Option<Box<dyn ConduitRecoverer>>,
    recovery_timeout: Option<Duration>,
}

#[derive(Debug)]
struct KeepaliveRuntime {
    ping_interval: Duration,
    pong_timeout: Duration,
    next_ping_at: tokio::time::Instant,
    waiting_pong_nonce: Option<u64>,
    pong_deadline: tokio::time::Instant,
    next_ping_nonce: u64,
}

// r[impl connection]
/// Static data for one active connection.
#[derive(Debug)]
pub struct ConnectionState {
    /// Unique connection identifier
    pub id: ConnectionId,

    /// Our settings
    pub local_settings: ConnectionSettings,

    /// The peer's settings
    pub peer_settings: ConnectionSettings,

    /// Sender for routing incoming messages to the per-connection driver task.
    conn_tx: mpsc::Sender<RecvMessage>,
    closed_tx: watch::Sender<bool>,

    /// Per-connection schema recv tracker — schemas are scoped to a connection.
    schema_recv_tracker: Arc<vox_types::SchemaRecvTracker>,
}

#[derive(Debug)]
enum ConnectionSlot {
    Active(ConnectionState),
    PendingOutbound(PendingOutboundData),
}

/// Debug-printable wrapper that omits the oneshot sender.
struct PendingOutboundData {
    local_settings: ConnectionSettings,
    result_tx: Option<moire::sync::oneshot::Sender<Result<ConnectionHandle, SessionError>>>,
}

impl std::fmt::Debug for PendingOutboundData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingOutbound")
            .field("local_settings", &self.local_settings)
            .finish()
    }
}

#[derive(Clone)]
pub(crate) struct ConnectionSender {
    connection_id: ConnectionId,
    pub(crate) sess_core: Arc<SessionCore>,
    failures: Arc<mpsc::UnboundedSender<(RequestId, FailureDisposition)>>,
}

fn forwarded_payload<'a>(payload: &'a vox_types::Payload<'static>) -> vox_types::Payload<'a> {
    let vox_types::Payload::PostcardBytes(bytes) = payload else {
        unreachable!("proxy forwarding expects decoded incoming payload bytes")
    };
    vox_types::Payload::PostcardBytes(bytes)
}

fn forwarded_request_body<'a>(body: &'a RequestBody<'static>) -> RequestBody<'a> {
    match body {
        RequestBody::Call(call) => RequestBody::Call(vox_types::RequestCall {
            method_id: call.method_id,
            metadata: call.metadata.clone(),
            args: forwarded_payload(&call.args),
            schemas: call.schemas.clone(),
        }),
        RequestBody::Response(response) => RequestBody::Response(RequestResponse {
            metadata: response.metadata.clone(),
            ret: forwarded_payload(&response.ret),
            schemas: response.schemas.clone(),
        }),
        RequestBody::Cancel(cancel) => RequestBody::Cancel(vox_types::RequestCancel {
            metadata: cancel.metadata.clone(),
        }),
    }
}

fn forwarded_channel_body<'a>(
    body: &'a vox_types::ChannelBody<'static>,
) -> vox_types::ChannelBody<'a> {
    match body {
        vox_types::ChannelBody::Item(item) => {
            vox_types::ChannelBody::Item(vox_types::ChannelItem {
                item: forwarded_payload(&item.item),
            })
        }
        vox_types::ChannelBody::Close(close) => {
            vox_types::ChannelBody::Close(vox_types::ChannelClose {
                metadata: close.metadata.clone(),
            })
        }
        vox_types::ChannelBody::Reset(reset) => {
            vox_types::ChannelBody::Reset(vox_types::ChannelReset {
                metadata: reset.metadata.clone(),
            })
        }
        vox_types::ChannelBody::GrantCredit(credit) => {
            vox_types::ChannelBody::GrantCredit(vox_types::ChannelGrantCredit {
                additional: credit.additional,
            })
        }
    }
}

impl ConnectionSender {
    pub(crate) fn connection_id(&self) -> ConnectionId {
        self.connection_id
    }

    pub(crate) async fn send_with_binder<'a>(
        &self,
        msg: ConnectionMessage<'a>,
        binder: Option<&'a dyn vox_types::ChannelBinder>,
    ) -> Result<(), ()> {
        let payload = match msg {
            ConnectionMessage::Request(r) => MessagePayload::RequestMessage(r),
            ConnectionMessage::Channel(c) => MessagePayload::ChannelMessage(c),
        };
        let message = Message {
            connection_id: self.connection_id,
            payload,
        };
        self.sess_core
            .send(message, binder, None)
            .await
            .map_err(|_| ())
    }

    /// Send an arbitrary connection message
    pub async fn send<'a>(&self, msg: ConnectionMessage<'a>) -> Result<(), ()> {
        self.send_with_binder(msg, None).await
    }

    /// Send a received connection message without re-materializing payload values.
    pub(crate) async fn send_owned(
        &self,
        schemas: Arc<vox_types::SchemaRecvTracker>,
        msg: SelfRef<ConnectionMessage<'static>>,
    ) -> Result<(), ()> {
        let payload = match &*msg {
            ConnectionMessage::Request(request) => MessagePayload::RequestMessage(RequestMessage {
                id: request.id,
                body: forwarded_request_body(&request.body),
            }),
            ConnectionMessage::Channel(channel) => MessagePayload::ChannelMessage(ChannelMessage {
                id: channel.id,
                body: forwarded_channel_body(&channel.body),
            }),
        };

        self.sess_core
            .send(
                Message {
                    connection_id: self.connection_id,
                    payload,
                },
                None,
                Some(&*schemas),
            )
            .await
            .map_err(|_| ())
    }

    /// Send a response specifically
    pub async fn send_response<'a>(
        &self,
        request_id: RequestId,
        response: RequestResponse<'a>,
    ) -> Result<(), ()> {
        self.send(ConnectionMessage::Request(RequestMessage {
            id: request_id,
            body: RequestBody::Response(response),
        }))
        .await
    }

    /// Shape a response using an explicit method ID, then send it.
    pub async fn send_response_for_method<'a>(
        &self,
        request_id: RequestId,
        method_id: vox_types::MethodId,
        mut response: RequestResponse<'a>,
    ) -> Result<(), ()> {
        self.prepare_response_for_method(request_id, method_id, &mut response);
        self.send(ConnectionMessage::Request(RequestMessage {
            id: request_id,
            body: RequestBody::Response(response),
        }))
        .await
    }

    /// Shape a response using an explicit method ID without sending it yet.
    pub(crate) fn prepare_response_for_method(
        &self,
        request_id: RequestId,
        method_id: vox_types::MethodId,
        response: &mut RequestResponse<'_>,
    ) {
        self.sess_core.prepare_response_for_method(
            self.connection_id,
            request_id,
            method_id,
            response,
        );
    }

    /// Shape a response using an explicit canonical root type and schema source.
    pub(crate) fn prepare_response_from_source(
        &self,
        request_id: RequestId,
        method_id: vox_types::MethodId,
        root_type: &vox_types::TypeRef,
        source: &dyn vox_types::SchemaSource,
        response: &mut RequestResponse<'_>,
    ) {
        self.sess_core.prepare_response_from_source(
            self.connection_id,
            request_id,
            method_id,
            root_type,
            source,
            response,
        );
    }

    /// Mark a request as failed by removing any pending response slot.
    /// Called when a send error occurs or no reply was sent.
    pub fn mark_failure(&self, request_id: RequestId, disposition: FailureDisposition) {
        let _ = self.failures.send((request_id, disposition));
    }

    /// Get the schema registry for this connection's send tracker.
    pub fn schema_registry(&self) -> vox_types::SchemaRegistry {
        self.sess_core.schema_registry(self.connection_id)
    }

    /// Prepare schemas for a replay response using the operation store as schema source.
    pub fn prepare_replay_schemas(
        &self,
        request_id: RequestId,
        method_id: vox_types::MethodId,
        root_type: &vox_types::TypeRef,
        store: &dyn crate::OperationStore,
        response: &mut RequestResponse<'_>,
    ) {
        self.prepare_response_from_source(
            request_id,
            method_id,
            root_type,
            store.schema_source(),
            response,
        );
    }
}

pub struct ConnectionHandle {
    pub(crate) sender: ConnectionSender,
    pub(crate) rx: mpsc::Receiver<RecvMessage>,
    pub(crate) failures_rx: mpsc::UnboundedReceiver<(RequestId, FailureDisposition)>,
    pub(crate) control_tx: Option<mpsc::UnboundedSender<DropControlRequest>>,
    pub(crate) closed_rx: watch::Receiver<bool>,
    pub(crate) resumed_rx: watch::Receiver<u64>,
    /// The parity this side should use for allocating request/channel IDs.
    pub parity: Parity,
    pub(crate) peer_supports_retry: bool,
}

impl std::fmt::Debug for ConnectionHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectionHandle")
            .field("connection_id", &self.sender.connection_id)
            .finish()
    }
}

pub(crate) enum ConnectionMessage<'payload> {
    Request(RequestMessage<'payload>),
    Channel(ChannelMessage<'payload>),
}

/// A message routed to a driver, carrying the `SchemaRecvTracker` that was
/// current when the session received it. This ensures each message uses the
/// correct tracker even across reconnections.
pub(crate) struct RecvMessage {
    pub schemas: Arc<vox_types::SchemaRecvTracker>,
    pub msg: SelfRef<ConnectionMessage<'static>>,
}

impl ConnectionHandle {
    /// Returns the connection ID for this handle.
    pub fn connection_id(&self) -> ConnectionId {
        self.sender.connection_id
    }

    /// Resolve when this connection closes.
    pub async fn closed(&self) {
        if *self.closed_rx.borrow() {
            return;
        }
        let mut rx = self.closed_rx.clone();
        while rx.changed().await.is_ok() {
            if *rx.borrow() {
                return;
            }
        }
    }

    /// Return whether this connection is still considered connected.
    pub fn is_connected(&self) -> bool {
        !*self.closed_rx.borrow()
    }

    pub fn peer_supports_retry(&self) -> bool {
        self.peer_supports_retry
    }
}

/// Forward all request/channel traffic between two connections.
///
/// This is a protocol-level bridge: it does not inspect service schemas or method IDs.
/// It exits when either side closes or a forward send fails, then requests closure of
/// both underlying connections.
pub async fn proxy_connections(
    left: ConnectionHandle,
    right: ConnectionHandle,
) -> Result<(), SessionError> {
    if left.parity == right.parity {
        return Err(SessionError::Protocol(
            "proxy_connections requires opposite parities".into(),
        ));
    }
    let left_conn_id = left.connection_id();
    let right_conn_id = right.connection_id();
    let ConnectionHandle {
        sender: left_sender,
        rx: mut left_rx,
        failures_rx: _left_failures_rx,
        control_tx: left_control_tx,
        closed_rx: _left_closed_rx,
        resumed_rx: _left_resumed_rx,
        parity: _left_parity,
        peer_supports_retry: _left_peer_supports_retry,
    } = left;
    let ConnectionHandle {
        sender: right_sender,
        rx: mut right_rx,
        failures_rx: _right_failures_rx,
        control_tx: right_control_tx,
        closed_rx: _right_closed_rx,
        resumed_rx: _right_resumed_rx,
        parity: _right_parity,
        peer_supports_retry: _right_peer_supports_retry,
    } = right;

    loop {
        tokio::select! {
            recv = left_rx.recv() => {
                let Some(recv) = recv else {
                    break;
                };
                if right_sender.send_owned(recv.schemas, recv.msg).await.is_err() {
                    break;
                }
            }
            recv = right_rx.recv() => {
                let Some(recv) = recv else {
                    break;
                };
                if left_sender.send_owned(recv.schemas, recv.msg).await.is_err() {
                    break;
                }
            }
        }
    }

    if let Some(tx) = left_control_tx.as_ref() {
        let _ = send_drop_control(tx, DropControlRequest::Close(left_conn_id));
    }
    if let Some(tx) = right_control_tx.as_ref() {
        let _ = send_drop_control(tx, DropControlRequest::Close(right_conn_id));
    }
    Ok(())
}

/// Errors that can occur during session establishment or operation.
#[derive(Debug)]
pub enum SessionError {
    Io(std::io::Error),
    Protocol(String),
    Rejected(Metadata<'static>),
    NotResumable,
    ConnectTimeout,
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io error: {e}"),
            Self::Protocol(msg) => write!(f, "protocol error: {msg}"),
            Self::Rejected(_) => write!(f, "connection rejected"),
            Self::NotResumable => write!(f, "session is not resumable"),
            Self::ConnectTimeout => write!(f, "connect timeout"),
        }
    }
}

impl std::error::Error for SessionError {}

impl Session {
    #[allow(clippy::too_many_arguments)]
    fn pre_handshake<Tx, Rx>(
        tx: Tx,
        rx: Rx,
        on_connection: Option<Arc<dyn ConnectionAcceptor>>,
        open_rx: mpsc::Receiver<OpenRequest>,
        close_rx: mpsc::Receiver<CloseRequest>,
        resume_rx: mpsc::Receiver<ResumeRequest>,
        control_tx: mpsc::UnboundedSender<DropControlRequest>,
        control_rx: mpsc::UnboundedReceiver<DropControlRequest>,
        keepalive: Option<SessionKeepaliveConfig>,
        resumable: bool,
        recoverer: Option<Box<dyn ConduitRecoverer>>,
        recovery_timeout: Option<Duration>,
    ) -> Self
    where
        Tx: ConduitTx<Msg = MessageFamily> + MaybeSend + MaybeSync + 'static,
        for<'p> <Tx as ConduitTx>::Permit<'p>: MaybeSend,
        Rx: ConduitRx<Msg = MessageFamily> + MaybeSend + 'static,
    {
        let sess_core = Arc::new(SessionCore {
            inner: std::sync::Mutex::new(SessionCoreInner {
                tx: Arc::new(tx) as Arc<dyn DynConduitTx>,
                conns: HashMap::new(),
            }),
        });
        let (resume_notifier, _resume_rx) = watch::channel(0_u64);
        Session {
            rx: Box::new(rx),
            role: SessionRole::Initiator, // overwritten in establish_as_*
            parity: Parity::Odd,          // overwritten in establish_as_*
            sess_core,
            peer_supports_retry: false,
            local_root_settings: ConnectionSettings {
                parity: Parity::Odd,
                max_concurrent_requests: 64,
            },
            peer_root_settings: None,
            resumable,
            session_resume_key: None,
            conns: BTreeMap::new(),
            root_closed_internal: false,
            conn_ids: IdAllocator::new(Parity::Odd), // overwritten in establish_as_*
            on_connection,
            open_rx,
            close_rx,
            resume_rx,
            control_tx,
            control_rx,
            keepalive,
            resume_notifier,
            recoverer,
            recovery_timeout,
        }
    }

    pub(crate) fn resume_key(&self) -> Option<SessionResumeKey> {
        self.session_resume_key
    }

    // r[impl session.handshake]
    fn establish_from_handshake(
        &mut self,
        result: HandshakeResult,
    ) -> Result<ConnectionHandle, SessionError> {
        self.role = result.role;
        self.parity = result.our_settings.parity;
        self.conn_ids = IdAllocator::new(result.our_settings.parity);
        self.local_root_settings = result.our_settings.clone();
        self.peer_root_settings = Some(result.peer_settings.clone());
        self.peer_supports_retry = result.peer_supports_retry;
        self.session_resume_key = result.session_resume_key;

        if self.resumable && self.session_resume_key.is_none() {
            return Err(SessionError::NotResumable);
        }

        Ok(self.make_root_handle(result.our_settings, result.peer_settings))
    }

    fn make_root_handle(
        &mut self,
        local_settings: ConnectionSettings,
        peer_settings: ConnectionSettings,
    ) -> ConnectionHandle {
        self.make_connection_handle(ConnectionId::ROOT, local_settings, peer_settings)
    }

    fn make_connection_handle(
        &mut self,
        conn_id: ConnectionId,
        local_settings: ConnectionSettings,
        peer_settings: ConnectionSettings,
    ) -> ConnectionHandle {
        let label = format!("session.conn{}", conn_id.0);
        let (conn_tx, conn_rx) = mpsc::channel::<RecvMessage>(&label, 64);
        let (failures_tx, failures_rx) = mpsc::unbounded_channel(format!("{label}.failures"));
        let (closed_tx, closed_rx) = watch::channel(false);
        let resumed_rx = self.resume_notifier.subscribe();

        let sender = ConnectionSender {
            connection_id: conn_id,
            sess_core: Arc::clone(&self.sess_core),
            failures: Arc::new(failures_tx),
        };

        let parity = local_settings.parity;
        trace!(%conn_id, "make_connection_handle: inserting slot into conns");
        self.conns.insert(
            conn_id,
            ConnectionSlot::Active(ConnectionState {
                id: conn_id,
                local_settings,
                peer_settings,
                conn_tx,
                closed_tx,
                schema_recv_tracker: Arc::new(vox_types::SchemaRecvTracker::new()),
            }),
        );

        ConnectionHandle {
            sender,
            rx: conn_rx,
            failures_rx,
            control_tx: Some(self.control_tx.clone()),
            closed_rx,
            resumed_rx,
            parity,
            peer_supports_retry: self.peer_supports_retry,
        }
    }

    /// Run the session recv loop: read from the conduit, demux by connection
    /// ID, and route to the appropriate connection's driver. Also processes
    /// open/close requests from the SessionHandle.
    // r[impl zerocopy.framing.pipeline.incoming]
    pub async fn run(&mut self) {
        let mut keepalive_runtime = self.make_keepalive_runtime();
        let mut keepalive_tick = keepalive_runtime.as_ref().map(|_| {
            let mut interval = tokio::time::interval(Duration::from_millis(10));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            interval
        });

        loop {
            tokio::select! {
                msg = self.rx.recv_msg() => {
                    vox_types::dlog!("[session {:?}] recv_msg returned", self.role);
                    match msg {
                        Ok(Some(msg)) => {
                            self.handle_message(msg, &mut keepalive_runtime).await;
                        }
                        Ok(None) => {
                            vox_types::dlog!("[session {:?}] recv loop: conduit returned EOF", self.role);
                            if !self.handle_conduit_break(&mut keepalive_runtime).await {
                                vox_types::dlog!("[session {:?}] recv loop: breaking (not resumable)", self.role);
                                break;
                            }
                        }
                        Err(error) => {
                            vox_types::dlog!("[session {:?}] recv loop: conduit recv error: {}", self.role, error);
                            if !self.handle_conduit_break(&mut keepalive_runtime).await {
                                vox_types::dlog!("[session {:?}] recv loop: breaking (not resumable)", self.role);
                                break;
                            }
                        }
                    }
                }
                Some(req) = self.open_rx.recv() => {
                    self.handle_open_request(req).await;
                }
                Some(req) = self.close_rx.recv() => {
                    self.handle_close_request(req).await;
                }
                Some(req) = self.resume_rx.recv() => {
                    let _ = req.result_tx.send(Err(SessionError::Protocol(
                        "resume is only valid while the session is disconnected".into(),
                    )));
                }
                Some(req) = self.control_rx.recv() => {
                    if !self.handle_drop_control_request(req).await {
                        break;
                    }
                }
                _ = async {
                    if let Some(interval) = keepalive_tick.as_mut() {
                        interval.tick().await;
                    }
                }, if keepalive_tick.is_some() => {
                    if !self.handle_keepalive_tick(&mut keepalive_runtime).await {
                        break;
                    }
                }
            }
        }

        // Drop all connection slots so per-connection drivers exit immediately.
        self.close_all_connections();
        trace!("session recv loop exited");
    }

    async fn handle_conduit_break(
        &mut self,
        keepalive_runtime: &mut Option<KeepaliveRuntime>,
    ) -> bool {
        if !self.resumable {
            return false;
        }

        if let Some(recoverer) = self.recoverer.as_mut() {
            let recovery_fut = recoverer.next_conduit(self.session_resume_key.as_ref());
            let recovery_result = match self.recovery_timeout {
                Some(timeout) => match tokio::time::timeout(timeout, recovery_fut).await {
                    Ok(r) => r,
                    Err(_) => return false,
                },
                None => recovery_fut.await,
            };
            match recovery_result {
                Ok(recovered) => {
                    let result =
                        self.resume_from_handshake(recovered.tx, recovered.rx, recovered.handshake);
                    match result {
                        Ok(()) => {
                            let next_generation = self.resume_notifier.borrow().wrapping_add(1);
                            let _ = self.resume_notifier.send(next_generation);
                            *keepalive_runtime = self.make_keepalive_runtime();
                            return true;
                        }
                        Err(_) => return false,
                    }
                }
                Err(_) => return false,
            }
        }

        loop {
            tokio::select! {
                Some(req) = self.resume_rx.recv() => {
                    let result =
                        self.resume_from_handshake(req.tx, req.rx, req.handshake_result);
                    let ok = result.is_ok();
                    let _ = req.result_tx.send(result);
                    if ok {
                        let next_generation = self.resume_notifier.borrow().wrapping_add(1);
                        let _ = self.resume_notifier.send(next_generation);
                        *keepalive_runtime = self.make_keepalive_runtime();
                        return true;
                    }
                }
                Some(req) = self.control_rx.recv() => {
                    if !self.handle_drop_control_request(req).await {
                        return false;
                    }
                }
                Some(req) = self.open_rx.recv() => {
                    let _ = req.result_tx.send(Err(SessionError::Protocol(
                        "session is disconnected; resume before opening connections".into(),
                    )));
                }
                Some(req) = self.close_rx.recv() => {
                    let _ = req.result_tx.send(Err(SessionError::Protocol(
                        "session is disconnected; resume before closing connections".into(),
                    )));
                }
                else => return false,
            }
        }
    }

    // r[impl session.handshake.resume]
    fn resume_from_handshake(
        &mut self,
        tx: Arc<dyn DynConduitTx>,
        rx: Box<dyn DynConduitRx>,
        result: HandshakeResult,
    ) -> Result<(), SessionError> {
        let Some(peer_settings) = self.peer_root_settings.clone() else {
            return Err(SessionError::Protocol("missing peer root settings".into()));
        };

        if result.our_settings != self.local_root_settings {
            return Err(SessionError::Protocol(
                "local root settings changed across session resume".into(),
            ));
        }

        if result.peer_settings != peer_settings {
            return Err(SessionError::Protocol(
                "peer root settings changed across session resume".into(),
            ));
        }

        self.peer_supports_retry = result.peer_supports_retry;
        self.session_resume_key = result.session_resume_key.or(self.session_resume_key);

        self.sess_core.replace_tx_and_reset_schemas(tx);
        self.rx = rx;
        // Reset the root connection's recv tracker on reconnection —
        // type IDs are per-connection and must not carry over.
        if let Some(ConnectionSlot::Active(state)) = self.conns.get_mut(&ConnectionId::ROOT) {
            state.schema_recv_tracker = Arc::new(vox_types::SchemaRecvTracker::new());
        }
        Ok(())
    }

    async fn handle_message(
        &mut self,
        msg: SelfRef<Message<'static>>,
        keepalive_runtime: &mut Option<KeepaliveRuntime>,
    ) {
        let conn_id = msg.connection_id;
        vox_types::selfref_match!(msg, payload {
            // r[impl connection.close.semantics]
            MessagePayload::ConnectionClose(_) => {
                if conn_id.is_root() {
                    warn!("received ConnectionClose for root connection");
                } else {
                    trace!(conn_id = conn_id.0, "received ConnectionClose for virtual connection");
                }
                // Remove the connection — dropping conn_tx causes the Driver's rx
                // to return None, which exits its run loop. All in-flight handlers
                // are dropped, triggering DriverReplySink::drop → Cancelled responses.
                self.remove_connection(&conn_id);
                self.maybe_request_shutdown_after_root_closed();
            }
            MessagePayload::ConnectionOpen(open) => {
                self.handle_inbound_open(conn_id, open).await;
            }
            MessagePayload::ConnectionAccept(accept) => {
                self.handle_inbound_accept(conn_id, accept);
            }
            MessagePayload::ConnectionReject(reject) => {
                self.handle_inbound_reject(conn_id, reject);
            }
            MessagePayload::RequestMessage(r) => {
                vox_types::dlog!(
                    "[session {:?}] recv request: conn={:?} req={:?} body={} method={:?}",
                    self.role,
                    conn_id,
                    r.id,
                    match &r.body {
                        RequestBody::Call(_) => "Call",
                        RequestBody::Response(_) => "Response",
                        RequestBody::Cancel(_) => "Cancel",
                    },
                    match &r.body {
                        RequestBody::Call(call) => Some(call.method_id),
                        RequestBody::Response(_) | RequestBody::Cancel(_) => None,
                    }
                );
                // Record any inlined schemas from the incoming request before routing
                let response_had_schema_payload = matches!(&r.body, RequestBody::Response(resp) if !resp.schemas.is_empty());
                {
                    let schemas_cbor = match &r.body {
                        RequestBody::Call(call) => Some(&call.schemas),
                        RequestBody::Response(resp) => Some(&resp.schemas),
                        _ => None,
                    };
                    vox_types::dlog!(
                        "[schema] recv ({:?}): req={:?} body={} schemas_len={:?}",
                        self.role,
                        r.id,
                        match &r.body {
                            RequestBody::Call(_) => "Call",
                            RequestBody::Response(_) => "Response",
                            RequestBody::Cancel(_) => "Cancel",
                        },
                        schemas_cbor.map(|s| s.0.len())
                    );
                    let state = match self.conns.get(&conn_id) {
                        Some(ConnectionSlot::Active(state)) => state,
                        _ => return,
                    };
                    if let Some(schemas_cbor) = schemas_cbor
                        && !schemas_cbor.is_empty()
                    {
                        let payload = vox_types::SchemaPayload::from_cbor(&schemas_cbor.0)
                            .expect("inlined schemas must be valid CBOR");
                        let (method_id, direction) = match &r.body {
                            RequestBody::Call(call) => {
                                (call.method_id, vox_types::BindingDirection::Args)
                            }
                            RequestBody::Response(_) => {
                                let method_id = self
                                    .sess_core
                                    .take_outgoing_call_method(conn_id, r.id)
                                    .expect("response schemas require an inflight method binding");
                                (method_id, vox_types::BindingDirection::Response)
                            }
                            RequestBody::Cancel(_) => unreachable!(),
                        };
                        state
                            .schema_recv_tracker
                            .record_received(method_id, direction, payload)
                            .expect("received schemas must not contain duplicate type IDs");
                    }
                }
                if matches!(&r.body, RequestBody::Response(_)) && !response_had_schema_payload {
                    let _ = self.sess_core.take_outgoing_call_method(conn_id, r.id);
                }
                // Record incoming calls so SessionCore::send() can look up
                // the method_id when sending the response.
                if let RequestBody::Call(call) = &r.body {
                    self.sess_core.record_incoming_call(conn_id, r.id, call.method_id);
                }
                let state = match self.conns.get(&conn_id) {
                    Some(ConnectionSlot::Active(state)) => state,
                    _ => return,
                };
                let conn_tx = state.conn_tx.clone();
                let request_id = r.id;
                let body_kind = match &r.body {
                    RequestBody::Call(_) => "Call",
                    RequestBody::Response(_) => "Response",
                    RequestBody::Cancel(_) => "Cancel",
                };
                let recv_msg = RecvMessage {
                    schemas: Arc::clone(&state.schema_recv_tracker),
                    msg: r.map(ConnectionMessage::Request),
                };
                vox_types::dlog!(
                    "[session {:?}] dispatch request: conn={:?} req={:?} body={}",
                    self.role,
                    conn_id,
                    request_id,
                    body_kind
                );
                if conn_tx.send(recv_msg).await.is_err() {
                    self.remove_connection(&conn_id);
                    self.maybe_request_shutdown_after_root_closed();
                }
            }
            MessagePayload::ChannelMessage(c) => {
                let state = match self.conns.get(&conn_id) {
                    Some(ConnectionSlot::Active(state)) => state,
                    _ => return,
                };
                let conn_tx = state.conn_tx.clone();
                let recv_msg = RecvMessage {
                    schemas: Arc::clone(&state.schema_recv_tracker),
                    msg: c.map(ConnectionMessage::Channel),
                };
                if conn_tx.send(recv_msg).await.is_err() {
                    self.remove_connection(&conn_id);
                    self.maybe_request_shutdown_after_root_closed();
                }
            }
            MessagePayload::Ping(ping) => {
                let _ = self
                    .sess_core
                    .send(Message {
                        connection_id: conn_id,
                        payload: MessagePayload::Pong(vox_types::Pong { nonce: ping.nonce }),
                    }, None, None)
                    .await;
            }
            MessagePayload::Pong(pong) => {
                if conn_id.is_root() {
                    self.handle_keepalive_pong(pong.nonce, keepalive_runtime);
                }
            }
            // ProtocolError: not valid post-handshake, drop.
        })
    }

    fn make_keepalive_runtime(&self) -> Option<KeepaliveRuntime> {
        let config = self.keepalive?;
        if config.ping_interval.is_zero() || config.pong_timeout.is_zero() {
            warn!("keepalive disabled due to non-positive interval/timeout");
            return None;
        }
        let now = tokio::time::Instant::now();
        Some(KeepaliveRuntime {
            ping_interval: config.ping_interval,
            pong_timeout: config.pong_timeout,
            next_ping_at: now + config.ping_interval,
            waiting_pong_nonce: None,
            pong_deadline: now,
            next_ping_nonce: 1,
        })
    }

    fn handle_keepalive_pong(&self, nonce: u64, keepalive_runtime: &mut Option<KeepaliveRuntime>) {
        let Some(runtime) = keepalive_runtime.as_mut() else {
            return;
        };
        if runtime.waiting_pong_nonce != Some(nonce) {
            return;
        }
        runtime.waiting_pong_nonce = None;
        runtime.next_ping_at = tokio::time::Instant::now() + runtime.ping_interval;
    }

    async fn handle_keepalive_tick(
        &mut self,
        keepalive_runtime: &mut Option<KeepaliveRuntime>,
    ) -> bool {
        let Some(runtime) = keepalive_runtime.as_mut() else {
            return true;
        };
        let now = tokio::time::Instant::now();

        if let Some(waiting_nonce) = runtime.waiting_pong_nonce {
            if now >= runtime.pong_deadline {
                warn!(
                    nonce = waiting_nonce,
                    timeout_ms = runtime.pong_timeout.as_millis(),
                    "keepalive timeout waiting for pong"
                );
                return false;
            }
            return true;
        }

        if now < runtime.next_ping_at {
            return true;
        }

        let nonce = runtime.next_ping_nonce;
        if self
            .sess_core
            .send(
                Message {
                    connection_id: ConnectionId::ROOT,
                    payload: MessagePayload::Ping(vox_types::Ping { nonce }),
                },
                None,
                None,
            )
            .await
            .is_err()
        {
            warn!("failed to send keepalive ping");
            return false;
        }

        runtime.waiting_pong_nonce = Some(nonce);
        runtime.pong_deadline = now + runtime.pong_timeout;
        runtime.next_ping_at = now + runtime.ping_interval;
        runtime.next_ping_nonce = runtime.next_ping_nonce.wrapping_add(1);
        true
    }

    async fn handle_inbound_open(
        &mut self,
        conn_id: ConnectionId,
        open: SelfRef<ConnectionOpen<'static>>,
    ) {
        // Validate: connection ID must match peer's parity (opposite of ours).
        let peer_parity = self.parity.other();
        if !conn_id.has_parity(peer_parity) {
            // Protocol error: wrong parity. For now, just reject.
            let _ = self
                .sess_core
                .send(
                    Message {
                        connection_id: conn_id,
                        payload: MessagePayload::ConnectionReject(vox_types::ConnectionReject {
                            metadata: vec![],
                        }),
                    },
                    None,
                    None,
                )
                .await;
            return;
        }

        // Validate: connection ID must not already be in use.
        if self.conns.contains_key(&conn_id) {
            // Protocol error: duplicate connection ID.
            let _ = self
                .sess_core
                .send(
                    Message {
                        connection_id: conn_id,
                        payload: MessagePayload::ConnectionReject(vox_types::ConnectionReject {
                            metadata: vec![],
                        }),
                    },
                    None,
                    None,
                )
                .await;
            return;
        }

        // r[impl connection.open.rejection]
        // Call the acceptor callback. If none is registered, reject.
        if self.on_connection.is_none() {
            let _ = self
                .sess_core
                .send(
                    Message {
                        connection_id: conn_id,
                        payload: MessagePayload::ConnectionReject(vox_types::ConnectionReject {
                            metadata: vec![],
                        }),
                    },
                    None,
                    None,
                )
                .await;
            return;
        }

        // Derive settings: opposite parity, same max concurrent requests.
        let our_settings = ConnectionSettings {
            parity: open.connection_settings.parity.other(),
            max_concurrent_requests: open.connection_settings.max_concurrent_requests,
        };

        // Create the connection handle and activate it.
        let handle = self.make_connection_handle(
            conn_id,
            our_settings.clone(),
            open.connection_settings.clone(),
        );

        // Let the acceptor decide the connection's fate.
        let mut metadata: Vec<vox_types::MetadataEntry<'_>> =
            open.metadata.iter().cloned().collect();
        metadata.push(vox_types::MetadataEntry::str(
            "vox-connection-kind",
            "virtual",
        ));
        let request = ConnectionRequest::new(&metadata);
        let pending = PendingConnection::new(handle);
        let acceptor = self.on_connection.as_ref().unwrap();
        trace!(%conn_id, "calling acceptor for virtual connection");
        match acceptor.accept(&request, pending) {
            Ok(()) => {
                trace!(%conn_id, "acceptor accepted virtual connection, sending ConnectionAccept");
                let _ = self
                    .sess_core
                    .send(
                        Message {
                            connection_id: conn_id,
                            payload: MessagePayload::ConnectionAccept(
                                vox_types::ConnectionAccept {
                                    connection_settings: our_settings,
                                    metadata: vec![],
                                },
                            ),
                        },
                        None,
                        None,
                    )
                    .await;
            }
            Err(reject_metadata) => {
                // Clean up the connection slot we created.
                trace!(%conn_id, "acceptor rejected, removing conn slot");
                self.conns.remove(&conn_id);
                let _ = self
                    .sess_core
                    .send(
                        Message {
                            connection_id: conn_id,
                            payload: MessagePayload::ConnectionReject(
                                vox_types::ConnectionReject {
                                    metadata: reject_metadata,
                                },
                            ),
                        },
                        None,
                        None,
                    )
                    .await;
            }
        }
    }

    fn handle_inbound_accept(
        &mut self,
        conn_id: ConnectionId,
        accept: SelfRef<ConnectionAccept<'static>>,
    ) {
        let slot = self.remove_connection(&conn_id);
        match slot {
            Some(ConnectionSlot::PendingOutbound(mut pending)) => {
                let handle = self.make_connection_handle(
                    conn_id,
                    pending.local_settings.clone(),
                    accept.connection_settings.clone(),
                );

                if let Some(tx) = pending.result_tx.take() {
                    let _ = tx.send(Ok(handle));
                }
            }
            Some(other) => {
                // Not pending outbound — put it back and ignore.
                self.conns.insert(conn_id, other);
            }
            None => {
                // No pending open for this ID — ignore.
            }
        }
    }

    fn handle_inbound_reject(
        &mut self,
        conn_id: ConnectionId,
        reject: SelfRef<ConnectionReject<'static>>,
    ) {
        let slot = self.remove_connection(&conn_id);
        match slot {
            Some(ConnectionSlot::PendingOutbound(mut pending)) => {
                if let Some(tx) = pending.result_tx.take() {
                    let _ = tx.send(Err(SessionError::Rejected(reject.metadata.to_vec())));
                }
            }
            Some(other) => {
                self.conns.insert(conn_id, other);
            }
            None => {}
        }
    }

    // r[impl connection.open]
    async fn handle_open_request(&mut self, req: OpenRequest) {
        let conn_id = self.conn_ids.alloc();

        // Send ConnectionOpen to the peer.
        let send_result = self
            .sess_core
            .send(
                Message {
                    connection_id: conn_id,
                    payload: MessagePayload::ConnectionOpen(ConnectionOpen {
                        connection_settings: req.settings.clone(),
                        metadata: req.metadata,
                    }),
                },
                None,
                None,
            )
            .await;

        if send_result.is_err() {
            let _ = req.result_tx.send(Err(SessionError::Protocol(
                "failed to send ConnectionOpen".into(),
            )));
            return;
        }

        // Store the pending state. The run loop will complete the oneshot
        // when ConnectionAccept or ConnectionReject arrives.
        self.conns.insert(
            conn_id,
            ConnectionSlot::PendingOutbound(PendingOutboundData {
                local_settings: req.settings,
                result_tx: Some(req.result_tx),
            }),
        );
    }

    // r[impl connection.close]
    async fn handle_close_request(&mut self, req: CloseRequest) {
        if req.conn_id.is_root() {
            let _ = req.result_tx.send(Err(SessionError::Protocol(
                "cannot close root connection".into(),
            )));
            return;
        }

        // Remove the connection slot — this drops conn_tx and causes the
        // Driver to exit cleanly.
        if self.remove_connection(&req.conn_id).is_none() {
            let _ = req
                .result_tx
                .send(Err(SessionError::Protocol("connection not found".into())));
            return;
        }

        // Send ConnectionClose to the peer.
        let send_result = self
            .sess_core
            .send(
                Message {
                    connection_id: req.conn_id,
                    payload: MessagePayload::ConnectionClose(ConnectionClose {
                        metadata: req.metadata,
                    }),
                },
                None,
                None,
            )
            .await;

        if send_result.is_err() {
            let _ = req.result_tx.send(Err(SessionError::Protocol(
                "failed to send ConnectionClose".into(),
            )));
            return;
        }

        let _ = req.result_tx.send(Ok(()));
        self.maybe_request_shutdown_after_root_closed();
    }

    async fn handle_drop_control_request(&mut self, req: DropControlRequest) -> bool {
        match req {
            DropControlRequest::Shutdown => {
                trace!("session shutdown requested");
                false
            }
            DropControlRequest::Close(conn_id) => {
                // r[impl rpc.caller.liveness.last-drop-closes-connection]
                if conn_id.is_root() {
                    // r[impl rpc.caller.liveness.root-internal-close]
                    trace!("root callers dropped; internally closing root connection");
                    self.root_closed_internal = true;
                    // r[impl rpc.caller.liveness.root-teardown-condition]
                    return self.has_virtual_connections();
                }

                if self.remove_connection(&conn_id).is_some() {
                    let _ = self
                        .sess_core
                        .send(
                            Message {
                                connection_id: conn_id,
                                payload: MessagePayload::ConnectionClose(ConnectionClose {
                                    metadata: vec![],
                                }),
                            },
                            None,
                            None,
                        )
                        .await;
                }

                !self.root_closed_internal || self.has_virtual_connections()
            }
        }
    }

    fn has_virtual_connections(&self) -> bool {
        self.conns.keys().any(|id| !id.is_root())
    }

    fn remove_connection(&mut self, conn_id: &ConnectionId) -> Option<ConnectionSlot> {
        trace!(%conn_id, "remove_connection called");
        let slot = self.conns.remove(conn_id);
        if let Some(ConnectionSlot::Active(state)) = &slot {
            let _ = state.closed_tx.send(true);
        }
        slot
    }

    fn close_all_connections(&mut self) {
        trace!(role = ?self.role, count = self.conns.len(), "close_all_connections");
        vox_types::dlog!(
            "[session {:?}] close_all_connections: {} slots",
            self.role,
            self.conns.len()
        );
        for (conn_id, slot) in self.conns.iter() {
            if let ConnectionSlot::Active(state) = slot {
                vox_types::dlog!("[session {:?}] closing connection {:?}", self.role, conn_id);
                let _ = state.closed_tx.send(true);
            }
        }
        self.conns.clear();
    }

    fn maybe_request_shutdown_after_root_closed(&self) {
        if self.root_closed_internal && !self.has_virtual_connections() {
            let _ = send_drop_control(&self.control_tx, DropControlRequest::Shutdown);
        }
    }
}

pub(crate) struct SessionCore {
    inner: std::sync::Mutex<SessionCoreInner>,
}

struct SendConnState {
    /// Tracks which methods we've already sent schemas for (per direction).
    /// If set, we don't need to extract schemas and go through send tracker.
    method_tracker: HashSet<(vox_types::BindingDirection, vox_types::MethodId)>,

    /// Tracks which schemas we have sent on this connection.
    send_tracker: vox_types::SchemaSendTracker,

    /// Maps request_id → method_id for in-flight incoming calls, so we can
    /// look up the method_id when sending the response.
    inflight_incoming: HashMap<RequestId, vox_types::MethodId>,

    /// Maps request_id → method_id for outbound calls awaiting a response, so
    /// inbound response schema payloads can bind their root TypeRef.
    inflight_outgoing: HashMap<RequestId, vox_types::MethodId>,
}

impl SendConnState {
    fn new() -> Self {
        SendConnState {
            method_tracker: HashSet::new(),
            send_tracker: vox_types::SchemaSendTracker::new(),
            inflight_incoming: HashMap::new(),
            inflight_outgoing: HashMap::new(),
        }
    }
}

struct SessionCoreInner {
    /// Underlying conduit (tx end)
    tx: Arc<dyn DynConduitTx>,

    /// Per-connection state re: sent schemas, etc.
    conns: HashMap<ConnectionId, SendConnState>,
}

impl SessionCore {
    // r[impl schema.principles.sender-driven]
    pub(crate) async fn send<'a>(
        &self,
        mut msg: Message<'a>,
        binder: Option<&'a dyn vox_types::ChannelBinder>,
        forwarded_schemas: Option<&vox_types::SchemaRecvTracker>,
    ) -> Result<(), ()> {
        let tx = {
            let mut inner = self.inner.lock().expect("session core mutex poisoned");
            let conn_id = msg.connection_id;

            if let MessagePayload::RequestMessage(req) = &mut msg.payload {
                vox_types::dlog!(
                    "[session-core] send request: conn={:?} req={:?} body={} forwarded={}",
                    conn_id,
                    req.id,
                    match &req.body {
                        RequestBody::Call(_) => "Call",
                        RequestBody::Response(_) => "Response",
                        RequestBody::Cancel(_) => "Cancel",
                    },
                    forwarded_schemas.is_some()
                );
                let conn_state = inner
                    .conns
                    .entry(conn_id)
                    .or_insert_with(SendConnState::new);
                match &mut req.body {
                    RequestBody::Call(call) => {
                        Self::prepare_call_schemas(
                            conn_state,
                            req.id,
                            call.method_id,
                            call,
                            forwarded_schemas,
                        );
                    }
                    RequestBody::Response(resp) => {
                        if let Some(method_id) = conn_state.inflight_incoming.remove(&req.id) {
                            Self::prepare_response_schemas(
                                conn_state,
                                req.id,
                                method_id,
                                resp,
                                forwarded_schemas,
                            );
                        }
                    }
                    RequestBody::Cancel(_) => {}
                }
            }

            inner.tx.clone()
        };
        tx.send_msg(msg, binder).await.map_err(|_| ())
    }

    /// Record that an incoming call was received, so we can look up the
    /// method_id when sending the response.
    pub(crate) fn record_incoming_call(
        &self,
        conn_id: ConnectionId,
        request_id: RequestId,
        method_id: vox_types::MethodId,
    ) {
        let mut inner = self.inner.lock().expect("session core mutex poisoned");
        let conn_state = inner
            .conns
            .entry(conn_id)
            .or_insert_with(SendConnState::new);
        vox_types::dlog!(
            "[schema] record_incoming_call: conn={:?} req={:?} method={:?}",
            conn_id,
            request_id,
            method_id
        );
        conn_state.inflight_incoming.insert(request_id, method_id);
    }

    pub(crate) fn take_outgoing_call_method(
        &self,
        conn_id: ConnectionId,
        request_id: RequestId,
    ) -> Option<vox_types::MethodId> {
        let mut inner = self.inner.lock().expect("session core mutex poisoned");
        inner
            .conns
            .get_mut(&conn_id)
            .and_then(|conn_state| conn_state.inflight_outgoing.remove(&request_id))
    }

    pub(crate) fn prepare_response_for_method(
        &self,
        conn_id: ConnectionId,
        request_id: RequestId,
        method_id: vox_types::MethodId,
        response: &mut RequestResponse<'_>,
    ) {
        let mut inner = self.inner.lock().expect("session core mutex poisoned");
        let conn_state = inner
            .conns
            .entry(conn_id)
            .or_insert_with(SendConnState::new);
        conn_state.inflight_incoming.remove(&request_id);
        Self::prepare_response_schemas(conn_state, request_id, method_id, response, None);
    }

    /// Borrow the send tracker's schema registry for the given connection.
    /// Used by the driver to pass to the operation store on seal.
    pub(crate) fn schema_registry(&self, conn_id: ConnectionId) -> vox_types::SchemaRegistry {
        let inner = self.inner.lock().expect("session core mutex poisoned");
        inner
            .conns
            .get(&conn_id)
            .map(|cs| cs.send_tracker.registry().clone())
            .unwrap_or_default()
    }

    /// Prepare response schemas from an explicit canonical root type and schema source.
    pub(crate) fn prepare_response_from_source(
        &self,
        conn_id: ConnectionId,
        request_id: RequestId,
        method_id: vox_types::MethodId,
        root_type: &vox_types::TypeRef,
        source: &dyn vox_types::SchemaSource,
        response: &mut RequestResponse<'_>,
    ) {
        let mut inner = self.inner.lock().expect("session core mutex poisoned");
        let conn_state = inner
            .conns
            .entry(conn_id)
            .or_insert_with(SendConnState::new);
        conn_state.inflight_incoming.remove(&request_id);
        let key = (vox_types::BindingDirection::Response, method_id);
        if conn_state.method_tracker.contains(&key) {
            return;
        }
        let cbor = conn_state.send_tracker.prepare_send(
            method_id,
            vox_types::BindingDirection::Response,
            root_type,
            source,
        );
        if !cbor.is_empty() {
            response.schemas = cbor;
        }
        conn_state.method_tracker.insert(key);
    }

    fn prepare_response_schemas(
        conn_state: &mut SendConnState,
        request_id: RequestId,
        method_id: vox_types::MethodId,
        response: &mut RequestResponse<'_>,
        forwarded_schemas: Option<&vox_types::SchemaRecvTracker>,
    ) {
        let key = (vox_types::BindingDirection::Response, method_id);
        if conn_state.method_tracker.contains(&key) {
            return;
        }

        let prepared = match &response.ret {
            vox_types::Payload::Value { shape, .. } => {
                match conn_state
                    .send_tracker
                    .attach_schemas_for_shape_if_needed(method_id, shape, response)
                {
                    Ok(schemas) => {
                        vox_types::dlog!(
                            "[schema] prepared {} bytes of response schemas for method {:?} (req {:?})",
                            schemas.0.len(),
                            method_id,
                            request_id
                        );
                        true
                    }
                    Err(e) => {
                        tracing::error!("schema extraction failed: {e}");
                        false
                    }
                }
            }
            vox_types::Payload::PostcardBytes(_) => {
                let Some(source) = forwarded_schemas else {
                    tracing::error!(
                        "schema attachment failed: missing forwarded response schemas for method {:?}",
                        method_id
                    );
                    return;
                };
                let Some(root) = source.get_remote_response_root(method_id) else {
                    tracing::error!(
                        "schema attachment failed: missing forwarded response root for method {:?}",
                        method_id
                    );
                    return;
                };
                let schemas = conn_state.send_tracker.prepare_send(
                    method_id,
                    vox_types::BindingDirection::Response,
                    &root,
                    source,
                );
                response.schemas = schemas.clone();
                vox_types::dlog!(
                    "[schema] prepared {} bytes of forwarded response schemas for method {:?} (req {:?})",
                    schemas.0.len(),
                    method_id,
                    request_id
                );
                true
            }
        };

        if prepared {
            conn_state.method_tracker.insert(key);
        }
    }

    fn prepare_call_schemas(
        conn_state: &mut SendConnState,
        request_id: RequestId,
        method_id: vox_types::MethodId,
        call: &mut vox_types::RequestCall<'_>,
        forwarded_schemas: Option<&vox_types::SchemaRecvTracker>,
    ) {
        conn_state.inflight_outgoing.insert(request_id, method_id);
        let key = (vox_types::BindingDirection::Args, method_id);
        if conn_state.method_tracker.contains(&key) {
            return;
        }

        let prepared = match &call.args {
            vox_types::Payload::Value { shape, .. } => {
                match conn_state
                    .send_tracker
                    .attach_schemas_for_shape_if_needed(method_id, shape, call)
                {
                    Ok(_) => true,
                    Err(e) => {
                        tracing::error!("schema extraction failed: {e}");
                        false
                    }
                }
            }
            vox_types::Payload::PostcardBytes(_) => {
                let Some(source) = forwarded_schemas else {
                    tracing::error!(
                        "schema attachment failed: missing forwarded args schemas for method {:?}",
                        method_id
                    );
                    return;
                };
                let Some(root) = source.get_remote_args_root(method_id) else {
                    tracing::error!(
                        "schema attachment failed: missing forwarded args root for method {:?}",
                        method_id
                    );
                    return;
                };
                call.schemas = conn_state.send_tracker.prepare_send(
                    method_id,
                    vox_types::BindingDirection::Args,
                    &root,
                    source,
                );
                true
            }
        };

        if prepared {
            conn_state.method_tracker.insert(key);
        }
    }

    fn replace_tx_and_reset_schemas(&self, tx: Arc<dyn DynConduitTx>) {
        let mut inner = self.inner.lock().expect("session core mutex poisoned");
        inner.tx = tx;
        inner.conns.clear();
    }
}

pub(crate) struct RecoveredConduit {
    pub tx: Arc<dyn DynConduitTx>,
    pub rx: Box<dyn DynConduitRx>,
    pub handshake: HandshakeResult,
}

pub(crate) trait ConduitRecoverer: MaybeSend {
    fn next_conduit<'a>(
        &'a mut self,
        resume_key: Option<&'a SessionResumeKey>,
    ) -> BoxFut<'a, Result<RecoveredConduit, SessionError>>;
}

pub trait DynConduitTx: MaybeSend + MaybeSync {
    fn send_msg<'a>(
        &'a self,
        msg: Message<'a>,
        binder: Option<&'a dyn vox_types::ChannelBinder>,
    ) -> BoxFut<'a, std::io::Result<()>>;
}
pub trait DynConduitRx: MaybeSend {
    fn recv_msg<'a>(&'a mut self)
    -> BoxFut<'a, std::io::Result<Option<SelfRef<Message<'static>>>>>;
}

// r[impl zerocopy.send]
// r[impl zerocopy.framing.pipeline.outgoing]
impl<T> DynConduitTx for T
where
    T: ConduitTx<Msg = MessageFamily> + MaybeSend + MaybeSync,
    for<'p> <T as ConduitTx>::Permit<'p>: MaybeSend,
{
    fn send_msg<'a>(
        &'a self,
        msg: Message<'a>,
        binder: Option<&'a dyn vox_types::ChannelBinder>,
    ) -> BoxFut<'a, std::io::Result<()>> {
        Box::pin(async move {
            let permit = self.reserve().await?;
            let result = if let Some(binder) = binder {
                vox_types::with_channel_binder(binder, || permit.send(msg))
            } else {
                permit.send(msg)
            };
            result.map_err(|e| std::io::Error::other(e.to_string()))
        })
    }
}

impl<T> DynConduitRx for T
where
    T: ConduitRx<Msg = MessageFamily> + MaybeSend,
{
    fn recv_msg<'a>(
        &'a mut self,
    ) -> BoxFut<'a, std::io::Result<Option<SelfRef<Message<'static>>>>> {
        Box::pin(async move {
            self.recv()
                .await
                .map_err(|error| std::io::Error::other(error.to_string()))
        })
    }
}

#[cfg(test)]
mod tests {
    use moire::sync::mpsc;
    use vox_types::{
        Backing, Conduit, ConnectionAccept, ConnectionReject, HandshakeResult, SelfRef,
    };

    use super::*;

    fn make_session() -> Session {
        let (a, b) = crate::memory_link_pair(32);
        // Keep the peer link alive so sess_core sends don't fail with broken pipe.
        std::mem::forget(b);
        let conduit = crate::BareConduit::new(a);
        let (tx, rx) = conduit.split();
        let (_open_tx, open_rx) = mpsc::channel::<OpenRequest>("session.open.test", 4);
        let (_close_tx, close_rx) = mpsc::channel::<CloseRequest>("session.close.test", 4);
        let (_resume_tx, resume_rx) = mpsc::channel::<ResumeRequest>("session.resume.test", 1);
        let (control_tx, control_rx) = mpsc::unbounded_channel("session.control.test");
        Session::pre_handshake(
            tx, rx, None, open_rx, close_rx, resume_rx, control_tx, control_rx, None, false, None,
            None,
        )
    }

    fn resumed_handshake(
        our_settings: ConnectionSettings,
        peer_settings: ConnectionSettings,
    ) -> HandshakeResult {
        HandshakeResult {
            role: SessionRole::Initiator,
            our_settings,
            peer_settings,
            peer_supports_retry: true,
            session_resume_key: Some(SessionResumeKey([7; 16])),
            peer_resume_key: None,
            our_schema: vec![],
            peer_schema: vec![],
            peer_metadata: vec![],
        }
    }

    fn accept_ref() -> SelfRef<ConnectionAccept<'static>> {
        SelfRef::owning(
            Backing::Boxed(Box::<[u8]>::default()),
            ConnectionAccept {
                connection_settings: ConnectionSettings {
                    parity: Parity::Even,
                    max_concurrent_requests: 64,
                },
                metadata: vec![],
            },
        )
    }

    fn reject_ref() -> SelfRef<ConnectionReject<'static>> {
        SelfRef::owning(
            Backing::Boxed(Box::<[u8]>::default()),
            ConnectionReject { metadata: vec![] },
        )
    }

    #[tokio::test]
    async fn duplicate_connection_accept_is_ignored_after_first() {
        let mut session = make_session();
        let conn_id = ConnectionId(1);
        let (result_tx, result_rx) = moire::sync::oneshot::channel("session.test.open_result");

        session.conns.insert(
            conn_id,
            ConnectionSlot::PendingOutbound(PendingOutboundData {
                local_settings: ConnectionSettings {
                    parity: Parity::Odd,
                    max_concurrent_requests: 64,
                },
                result_tx: Some(result_tx),
            }),
        );

        session.handle_inbound_accept(conn_id, accept_ref());
        let handle = result_rx
            .await
            .expect("pending outbound result should resolve")
            .expect("accept should resolve as Ok");
        assert_eq!(handle.connection_id(), conn_id);

        session.handle_inbound_accept(conn_id, accept_ref());
        assert!(
            matches!(
                session.conns.get(&conn_id),
                Some(ConnectionSlot::Active(ConnectionState { id, .. })) if *id == conn_id
            ),
            "duplicate accept should keep existing active connection state"
        );
    }

    #[tokio::test]
    async fn duplicate_connection_reject_is_ignored_after_first() {
        let mut session = make_session();
        let conn_id = ConnectionId(1);
        let (result_tx, result_rx) = moire::sync::oneshot::channel("session.test.open_result");

        session.conns.insert(
            conn_id,
            ConnectionSlot::PendingOutbound(PendingOutboundData {
                local_settings: ConnectionSettings {
                    parity: Parity::Odd,
                    max_concurrent_requests: 64,
                },
                result_tx: Some(result_tx),
            }),
        );

        session.handle_inbound_reject(conn_id, reject_ref());
        let result = result_rx
            .await
            .expect("pending outbound result should resolve");
        assert!(
            matches!(result, Err(SessionError::Rejected(_))),
            "expected rejection, got: {result:?}"
        );

        session.handle_inbound_reject(conn_id, reject_ref());
        assert!(
            !session.conns.contains_key(&conn_id),
            "duplicate reject should not recreate connection state"
        );
    }

    #[test]
    fn out_of_order_accept_or_reject_without_pending_is_ignored() {
        let mut session = make_session();
        let conn_id = ConnectionId(99);

        session.handle_inbound_accept(conn_id, accept_ref());
        session.handle_inbound_reject(conn_id, reject_ref());

        assert!(
            session.conns.is_empty(),
            "out-of-order accept/reject should not mutate empty connection table"
        );
    }

    #[tokio::test]
    async fn close_request_clears_pending_outbound_open() {
        let mut session = make_session();
        let (open_result_tx, open_result_rx) = moire::sync::oneshot::channel("session.open.result");
        let (close_result_tx, close_result_rx) =
            moire::sync::oneshot::channel("session.close.result");

        session.conns.insert(
            ConnectionId(1),
            ConnectionSlot::PendingOutbound(PendingOutboundData {
                local_settings: ConnectionSettings {
                    parity: Parity::Odd,
                    max_concurrent_requests: 64,
                },
                result_tx: Some(open_result_tx),
            }),
        );

        session
            .handle_close_request(CloseRequest {
                conn_id: ConnectionId(1),
                metadata: vec![],
                result_tx: close_result_tx,
            })
            .await;

        let close_result = close_result_rx
            .await
            .expect("close result should be delivered");
        assert!(
            close_result.is_ok(),
            "close should succeed for pending outbound connection"
        );

        assert!(
            open_result_rx.await.is_err(),
            "pending open result channel should be closed once the pending slot is removed"
        );
    }

    #[test]
    fn resume_rejects_changed_local_root_settings() {
        let mut session = make_session();
        let local_settings = ConnectionSettings {
            parity: Parity::Odd,
            max_concurrent_requests: 64,
        };
        let peer_settings = ConnectionSettings {
            parity: Parity::Even,
            max_concurrent_requests: 64,
        };
        let _root = session
            .establish_from_handshake(resumed_handshake(
                local_settings.clone(),
                peer_settings.clone(),
            ))
            .expect("initial handshake should establish session");

        let (link_a, _link_b) = crate::memory_link_pair(32);
        let conduit = crate::BareConduit::new(link_a);
        let (tx, rx) = conduit.split();

        let result = session.resume_from_handshake(
            Arc::new(tx),
            Box::new(rx),
            resumed_handshake(
                ConnectionSettings {
                    parity: Parity::Odd,
                    max_concurrent_requests: 65,
                },
                peer_settings,
            ),
        );

        assert!(
            matches!(
                &result,
                Err(SessionError::Protocol(message))
                    if message == "local root settings changed across session resume"
            ),
            "expected local-root-settings mismatch, got: {result:?}"
        );
    }

    #[test]
    fn resume_rejects_changed_peer_root_settings() {
        let mut session = make_session();
        let local_settings = ConnectionSettings {
            parity: Parity::Odd,
            max_concurrent_requests: 64,
        };
        let peer_settings = ConnectionSettings {
            parity: Parity::Even,
            max_concurrent_requests: 64,
        };
        let _root = session
            .establish_from_handshake(resumed_handshake(
                local_settings.clone(),
                peer_settings.clone(),
            ))
            .expect("initial handshake should establish session");

        let (link_a, _link_b) = crate::memory_link_pair(32);
        let conduit = crate::BareConduit::new(link_a);
        let (tx, rx) = conduit.split();

        let result = session.resume_from_handshake(
            Arc::new(tx),
            Box::new(rx),
            resumed_handshake(
                local_settings,
                ConnectionSettings {
                    parity: Parity::Even,
                    max_concurrent_requests: 65,
                },
            ),
        );

        assert!(
            matches!(
                &result,
                Err(SessionError::Protocol(message))
                    if message == "peer root settings changed across session resume"
            ),
            "expected peer-root-settings mismatch, got: {result:?}"
        );
    }
}
