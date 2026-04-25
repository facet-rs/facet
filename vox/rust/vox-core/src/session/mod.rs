use std::{
    collections::{BTreeMap, HashMap},
    future::Future,
    pin::Pin,
    sync::Arc,
    time::Duration,
};

use facet_core::Shape;
use moire::sync::mpsc;
use tokio::sync::{mpsc as tokio_mpsc, oneshot as tokio_oneshot, watch};
use tracing::{trace, warn};
use vox_types::{
    BoxFut, ChannelMessage, ConduitRx, ConduitTx, ConnectionAccept, ConnectionClose, ConnectionId,
    ConnectionOpen, ConnectionReject, ConnectionSettings, Handler, HandshakeResult, IdAllocator,
    MaybeSend, MaybeSync, Message, MessageFamily, MessagePayload, Metadata, Parity, RequestBody,
    RequestId, RequestMessage, RequestResponse, SchemaMessage, SelfRef, SessionResumeKey,
    SessionRole,
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

/// Metadata wrapper with typed getters for well-known `vox-*` keys.
///
/// Passed to [`ConnectionAcceptor::accept`] when a peer opens a connection.
pub struct ConnectionRequest<'a> {
    metadata: &'a [vox_types::MetadataEntry<'a>],
    service: &'a str,
}

impl<'a> ConnectionRequest<'a> {
    /// Build a connection request from metadata.
    ///
    /// Returns an error if the required `vox-service` metadata key is missing.
    pub fn new(metadata: &'a [vox_types::MetadataEntry<'a>]) -> Result<Self, SessionError> {
        let service = vox_types::metadata_get_str(metadata, "vox-service").ok_or_else(|| {
            SessionError::Protocol("missing required vox-service metadata".into())
        })?;
        Ok(Self { metadata, service })
    }

    /// The requested service name (`vox-service` metadata key).
    pub fn service(&self) -> &str {
        self.service
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
        !self.is_virtual()
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

    /// Accept this connection, run a Driver, and return a typed client for the peer.
    pub fn handle_with_client<C: crate::FromVoxSession>(
        mut self,
        handler: impl Handler<crate::DriverReplySink> + 'static,
    ) -> C {
        let handle = self
            .handle
            .take()
            .expect("PendingConnection already consumed");
        let conn_id = handle.connection_id();
        trace!(%conn_id, "PendingConnection::handle_with_client: creating driver");
        let mut driver = match self.operation_store.take() {
            Some(store) => crate::Driver::with_operation_store(handle, handler, store),
            None => crate::Driver::new(handle, handler),
        };
        let caller = crate::Caller::new(driver.caller());
        if let Some(slot) = &self.caller_slot {
            *slot.lock().unwrap() = Some(caller.clone());
        }
        #[cfg(not(target_arch = "wasm32"))]
        tokio::spawn(async move {
            trace!(%conn_id, "PendingConnection driver starting");
            driver.run().await;
            trace!(%conn_id, "PendingConnection driver exited");
        });
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move { driver.run().await });
        C::from_vox_session(caller, None)
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
pub trait ConnectionAcceptor: MaybeSend + MaybeSync + 'static {
    fn accept(
        &self,
        request: &ConnectionRequest,
        connection: PendingConnection,
    ) -> Result<(), Metadata<'static>>;
}

/// Any `Handler<DriverReplySink>` is automatically a `ConnectionAcceptor`.
impl<H> ConnectionAcceptor for H
where
    H: Handler<crate::DriverReplySink> + Clone + MaybeSend + MaybeSync + 'static,
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
        + MaybeSend
        + MaybeSync
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
        + MaybeSend
        + MaybeSync
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
        use vox_types::{MetadataEntry, MetadataFlags, MetadataValue};

        let metadata: Metadata<'static> = vec![MetadataEntry {
            key: crate::session::builders::VOX_SERVICE_METADATA_KEY.into(),
            value: MetadataValue::String(Client::SERVICE_NAME.into()),
            flags: MetadataFlags::NONE,
        }];
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
    /// Whether this session was registered in a `SessionRegistry`, meaning
    /// an external acceptor could route a reconnecting client to resume it.
    registered_in_registry: bool,
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

fn forwarded_payload<'a>(payload: &'a vox_types::Payload<'a>) -> vox_types::Payload<'a> {
    let vox_types::Payload::PostcardBytes(bytes) = payload else {
        unreachable!("proxy forwarding expects decoded incoming payload bytes")
    };
    vox_types::Payload::PostcardBytes(bytes)
}

fn forwarded_request_body<'a>(body: &'a RequestBody<'a>) -> RequestBody<'a> {
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

fn forwarded_channel_body<'a>(body: &'a vox_types::ChannelBody<'a>) -> vox_types::ChannelBody<'a> {
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
        let msg_ref = msg.get();
        let payload = match msg_ref {
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

    /// Prepare schemas for a replay response from the running code's
    /// static response shape — same source of truth fresh responses
    /// use, with the same connection-scoped dedup.
    pub fn prepare_replay_schemas(
        &self,
        request_id: RequestId,
        method_id: vox_types::MethodId,
        response_shape: &'static Shape,
        response: &mut RequestResponse<'_>,
    ) {
        self.sess_core.prepare_response_from_shape(
            self.connection_id,
            request_id,
            method_id,
            response_shape,
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

vox_types::impl_reborrow!(ConnectionMessage);

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

impl SessionError {
    /// Returns `true` if a retry of the same connection attempt may succeed.
    ///
    /// I/O errors and timeouts are transient — the remote might become available
    /// shortly. Protocol errors and explicit rejections are permanent for this
    /// peer address and will not resolve by retrying.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Io(_) | Self::ConnectTimeout | Self::NotResumable
        )
    }
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
    fn close_connection_for_protocol_error(
        &mut self,
        conn_id: ConnectionId,
        detail: impl std::fmt::Display,
    ) {
        warn!(%conn_id, "closing connection after protocol error: {detail}");
        self.remove_connection(&conn_id);
        self.maybe_request_shutdown_after_root_closed();
    }

    fn record_received_schema_cbor(
        &mut self,
        conn_id: ConnectionId,
        schema_recv_tracker: Arc<vox_types::SchemaRecvTracker>,
        method_id: vox_types::MethodId,
        direction: vox_types::BindingDirection,
        schemas_cbor: &vox_types::CborPayload,
        context: &str,
    ) -> bool {
        let payload = match vox_types::SchemaPayload::from_cbor(&schemas_cbor.0) {
            Ok(payload) => payload,
            Err(error) => {
                self.close_connection_for_protocol_error(
                    conn_id,
                    format!("{context}: invalid schema CBOR: {error}"),
                );
                return false;
            }
        };

        if let Err(error) = schema_recv_tracker.record_received(method_id, direction, payload) {
            self.close_connection_for_protocol_error(conn_id, format!("{context}: {error}"));
            return false;
        }

        true
    }

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
        Rx: ConduitRx<Msg = MessageFamily> + MaybeSend + 'static,
    {
        let (outbound_tx, outbound_rx) = tokio_mpsc::channel(256);
        let sess_core = Arc::new(SessionCore {
            inner: std::sync::Mutex::new(SessionCoreInner {
                tx: Arc::new(tx) as Arc<dyn DynConduitTx>,
                conns: HashMap::new(),
            }),
            outbound_tx,
        });
        spawn_outbound_worker(outbound_rx);
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
            registered_in_registry: false,
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
                // biased: ensure conduit EOF is processed before any resume
                // request. Without this, tokio's random branch selection can
                // pick resume_rx when BOTH branches are simultaneously ready
                // (fast client reconnect on Linux), causing the session to
                // reject a valid resume while still in CONNECTED state.
                biased;

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
        // Recovery strategy:
        // 1. If we have a recoverer (client-side auto-reconnect), use it.
        // 2. If we're registered in a SessionRegistry (server-side), wait
        //    for the registry to route a reconnecting client to us.
        // 3. Otherwise, the session is done.

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

        if !self.registered_in_registry {
            return false;
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
        let msg_ref = msg.get();
        let conn_id = msg_ref.connection_id;
        match &msg_ref.payload {
            MessagePayload::Ping(ping) => {
                let _ = self
                    .sess_core
                    .send(
                        Message {
                            connection_id: conn_id,
                            payload: MessagePayload::Pong(vox_types::Pong { nonce: ping.nonce }),
                        },
                        None,
                        None,
                    )
                    .await;
                return;
            }
            MessagePayload::Pong(pong) => {
                if conn_id.is_root() {
                    self.handle_keepalive_pong(pong.nonce, keepalive_runtime);
                }
                return;
            }
            MessagePayload::SchemaMessage(schema_msg) => {
                let schema_recv_tracker = match self.conns.get(&conn_id) {
                    Some(ConnectionSlot::Active(state)) => Arc::clone(&state.schema_recv_tracker),
                    _ => return,
                };
                let _ = self.record_received_schema_cbor(
                    conn_id,
                    schema_recv_tracker,
                    schema_msg.method_id,
                    schema_msg.direction,
                    &schema_msg.schemas,
                    "standalone schema message",
                );
                return;
            }
            _ => {}
        }
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
                let r_ref = r.get();
                vox_types::dlog!(
                    "[session {:?}] recv request: conn={:?} req={:?} body={} method={:?}",
                    self.role,
                    conn_id,
                    r_ref.id,
                    match &r_ref.body {
                        RequestBody::Call(_) => "Call",
                        RequestBody::Response(_) => "Response",
                        RequestBody::Cancel(_) => "Cancel",
                    },
                    match &r_ref.body {
                        RequestBody::Call(call) => Some(call.method_id),
                        RequestBody::Response(_) | RequestBody::Cancel(_) => None,
                    }
                );
                // Record any inlined schemas from the incoming request before routing
                let response_had_schema_payload = matches!(&r_ref.body, RequestBody::Response(resp) if !resp.schemas.is_empty());
                {
                    let schemas_cbor = match &r_ref.body {
                        RequestBody::Call(call) => Some(&call.schemas),
                        RequestBody::Response(resp) => Some(&resp.schemas),
                        _ => None,
                    };
                    vox_types::dlog!(
                        "[schema] recv ({:?}): req={:?} body={} schemas_len={:?}",
                        self.role,
                        r_ref.id,
                    match &r_ref.body {
                            RequestBody::Call(_) => "Call",
                            RequestBody::Response(_) => "Response",
                            RequestBody::Cancel(_) => "Cancel",
                        },
                        schemas_cbor.map(|s| s.0.len())
                    );
                    let schema_recv_tracker = match self.conns.get(&conn_id) {
                        Some(ConnectionSlot::Active(state)) => {
                            Arc::clone(&state.schema_recv_tracker)
                        }
                        _ => return,
                    };
                    if let Some(schemas_cbor) = schemas_cbor
                        && !schemas_cbor.is_empty()
                    {
                        let (method_id, direction) = match &r_ref.body {
                            RequestBody::Call(call) => {
                                (call.method_id, vox_types::BindingDirection::Args)
                            }
                            RequestBody::Response(_) => {
                                let Some(method_id) =
                                    self.sess_core.take_outgoing_call_method(conn_id, r_ref.id)
                                else {
                                    self.close_connection_for_protocol_error(
                                        conn_id,
                                        format!(
                                            "response schemas for unknown inflight request {:?}",
                                            r_ref.id
                                        ),
                                    );
                                    return;
                                };
                                (method_id, vox_types::BindingDirection::Response)
                            }
                            RequestBody::Cancel(_) => unreachable!(),
                        };
                        if !self.record_received_schema_cbor(
                            conn_id,
                            schema_recv_tracker,
                            method_id,
                            direction,
                            schemas_cbor,
                            "inlined request schemas",
                        ) {
                            return;
                        }
                    }
                }
                if matches!(&r_ref.body, RequestBody::Response(_)) && !response_had_schema_payload {
                    let _ = self.sess_core.take_outgoing_call_method(conn_id, r_ref.id);
                }
                // Record incoming calls so SessionCore::send() can look up
                // the method_id when sending the response.
                if let RequestBody::Call(call) = &r_ref.body {
                    self.sess_core.record_incoming_call(conn_id, r_ref.id, call.method_id);
                }
                let state = match self.conns.get(&conn_id) {
                    Some(ConnectionSlot::Active(state)) => state,
                    _ => return,
                };
                let conn_tx = state.conn_tx.clone();
                let request_id = r_ref.id;
                let body_kind = match &r_ref.body {
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
        let open = open.get();
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
        let mut metadata: Vec<vox_types::MetadataEntry<'_>> = open.metadata.to_vec();
        metadata.push(vox_types::MetadataEntry::str(
            "vox-connection-kind",
            "virtual",
        ));
        let request = match ConnectionRequest::new(&metadata) {
            Ok(r) => r,
            Err(e) => {
                trace!(%conn_id, %e, "rejecting virtual connection");
                self.conns.remove(&conn_id);
                let _ = self
                    .sess_core
                    .send(
                        Message {
                            connection_id: conn_id,
                            payload: MessagePayload::ConnectionReject(
                                vox_types::ConnectionReject {
                                    metadata: vec![vox_types::MetadataEntry::str(
                                        "error",
                                        e.to_string(),
                                    )],
                                },
                            ),
                        },
                        None,
                        None,
                    )
                    .await;
                return;
            }
        };
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
        let accept = accept.get();
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
        let reject = reject.get();
        let slot = self.remove_connection(&conn_id);
        match slot {
            Some(ConnectionSlot::PendingOutbound(mut pending)) => {
                if let Some(tx) = pending.result_tx.take() {
                    let _ = tx.send(Err(SessionError::Rejected(vox_types::metadata_into_owned(
                        reject.metadata.to_vec(),
                    ))));
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
    outbound_tx: tokio_mpsc::Sender<OutboundBatch>,
}

pub trait OutboundSendFuture: Future<Output = std::io::Result<()>> + MaybeSend + 'static {}
impl<T> OutboundSendFuture for T where T: Future<Output = std::io::Result<()>> + MaybeSend + 'static {}

type OutboundSend = Pin<Box<dyn OutboundSendFuture>>;

#[derive(Clone)]
struct PendingSchemaSend {
    method_id: vox_types::MethodId,
    direction: vox_types::BindingDirection,
    prepared: vox_types::PreparedSchemaPlan,
}

struct OutboundBatch {
    conn_id: ConnectionId,
    conn_state: Arc<std::sync::Mutex<SendConnState>>,
    tx: Arc<dyn DynConduitTx>,
    schema_sends: Vec<PendingSchemaSend>,
    payload_send: OutboundSend,
    result_tx: tokio_oneshot::Sender<std::io::Result<()>>,
}

async fn run_outbound_worker(mut rx: tokio_mpsc::Receiver<OutboundBatch>) {
    while let Some(batch) = rx.recv().await {
        let mut result = Ok(());
        for schema_send in batch.schema_sends {
            let schemas = {
                let mut conn_state = batch
                    .conn_state
                    .lock()
                    .expect("send conn state mutex poisoned");
                conn_state.send_tracker.preview_prepared_plan(
                    schema_send.method_id,
                    schema_send.direction,
                    &schema_send.prepared,
                )
            };
            if schemas.is_empty() {
                continue;
            }

            let schema_msg = Message {
                connection_id: batch.conn_id,
                payload: MessagePayload::SchemaMessage(SchemaMessage {
                    method_id: schema_send.method_id,
                    direction: schema_send.direction,
                    schemas,
                }),
            };
            let send = match batch.tx.clone().prepare_msg(schema_msg, None) {
                Ok(send) => send,
                Err(error) => {
                    result = Err(error);
                    break;
                }
            };
            if let Err(error) = send.await {
                result = Err(error);
                break;
            }
            let mut conn_state = batch
                .conn_state
                .lock()
                .expect("send conn state mutex poisoned");
            conn_state.send_tracker.mark_prepared_plan_sent(
                schema_send.method_id,
                schema_send.direction,
                &schema_send.prepared,
            );
            conn_state
                .planned_bindings
                .remove(&(schema_send.direction, schema_send.method_id));
        }
        if result.is_ok()
            && let Err(error) = batch.payload_send.await
        {
            result = Err(error);
        }
        let _ = batch.result_tx.send(result);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn spawn_outbound_worker(rx: tokio_mpsc::Receiver<OutboundBatch>) {
    if tokio::runtime::Handle::try_current().is_ok() {
        tokio::spawn(run_outbound_worker(rx));
        return;
    }

    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build outbound worker runtime");
        runtime.block_on(run_outbound_worker(rx));
    });
}

#[cfg(target_arch = "wasm32")]
fn spawn_outbound_worker(rx: tokio_mpsc::Receiver<OutboundBatch>) {
    wasm_bindgen_futures::spawn_local(run_outbound_worker(rx));
}

struct SendConnState {
    /// Tracks which schemas we have sent on this connection.
    send_tracker: vox_types::SchemaSendTracker,

    /// Maps request_id → method_id for in-flight incoming calls, so we can
    /// look up the method_id when sending the response.
    inflight_incoming: HashMap<RequestId, vox_types::MethodId>,

    /// Maps request_id → method_id for outbound calls awaiting a response, so
    /// inbound response schema payloads can bind their root TypeRef.
    inflight_outgoing: HashMap<RequestId, vox_types::MethodId>,

    /// Structured schema plans cached per binding until the first committed send.
    planned_bindings:
        HashMap<(vox_types::BindingDirection, vox_types::MethodId), vox_types::PreparedSchemaPlan>,
}

impl SendConnState {
    fn new() -> Self {
        SendConnState {
            send_tracker: vox_types::SchemaSendTracker::new(),
            inflight_incoming: HashMap::new(),
            inflight_outgoing: HashMap::new(),
            planned_bindings: HashMap::new(),
        }
    }
}

struct SessionCoreInner {
    /// Underlying conduit (tx end)
    tx: Arc<dyn DynConduitTx>,

    /// Per-connection state re: sent schemas, etc.
    conns: HashMap<ConnectionId, Arc<std::sync::Mutex<SendConnState>>>,
}

fn get_or_create_send_conn_state(
    inner: &mut SessionCoreInner,
    conn_id: ConnectionId,
) -> Arc<std::sync::Mutex<SendConnState>> {
    inner
        .conns
        .entry(conn_id)
        .or_insert_with(|| Arc::new(std::sync::Mutex::new(SendConnState::new())))
        .clone()
}

impl SessionCore {
    // r[impl schema.principles.sender-driven]
    pub(crate) async fn send<'a>(
        &self,
        mut msg: Message<'a>,
        binder: Option<&'a dyn vox_types::ChannelBinder>,
        forwarded_schemas: Option<&vox_types::SchemaRecvTracker>,
    ) -> Result<(), ()> {
        let conn_id = msg.connection_id;
        let (tx, conn_state, schema_sends) = {
            let mut inner = self.inner.lock().expect("session core mutex poisoned");
            let tx = inner.tx.clone();
            let conn_state = get_or_create_send_conn_state(&mut inner, conn_id);
            drop(inner);

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
                let schema_sends = {
                    let mut conn_state_guard =
                        conn_state.lock().expect("send conn state mutex poisoned");
                    let mut schema_sends = Vec::new();
                    match &mut req.body {
                        RequestBody::Call(call) => {
                            if let Some(schema_send) = Self::plan_call_schema_send(
                                &mut conn_state_guard,
                                req.id,
                                call.method_id,
                                call,
                                forwarded_schemas,
                            ) {
                                schema_sends.push(schema_send);
                            }
                            call.schemas = Default::default();
                        }
                        RequestBody::Response(resp) => {
                            if let Some(method_id) =
                                conn_state_guard.inflight_incoming.remove(&req.id)
                                && let Some(schema_send) = Self::plan_response_schema_send(
                                    &mut conn_state_guard,
                                    req.id,
                                    method_id,
                                    resp,
                                    forwarded_schemas,
                                )
                            {
                                schema_sends.push(schema_send);
                            }
                            resp.schemas = Default::default();
                        }
                        RequestBody::Cancel(_) => {}
                    }
                    schema_sends
                };
                (tx, conn_state, schema_sends)
            } else {
                (tx, conn_state, Vec::new())
            }
        };
        let payload_send = tx.clone().prepare_msg(msg, binder).map_err(|_| ())?;

        let (result_tx, result_rx) = tokio_oneshot::channel();
        self.outbound_tx
            .send(OutboundBatch {
                conn_id,
                conn_state,
                tx,
                schema_sends,
                payload_send,
                result_tx,
            })
            .await
            .map_err(|_| ())?;
        result_rx.await.map_err(|_| ())?.map_err(|_| ())
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
        let conn_state = get_or_create_send_conn_state(&mut inner, conn_id);
        vox_types::dlog!(
            "[schema] record_incoming_call: conn={:?} req={:?} method={:?}",
            conn_id,
            request_id,
            method_id
        );
        conn_state
            .lock()
            .expect("send conn state mutex poisoned")
            .inflight_incoming
            .insert(request_id, method_id);
    }

    pub(crate) fn take_outgoing_call_method(
        &self,
        conn_id: ConnectionId,
        request_id: RequestId,
    ) -> Option<vox_types::MethodId> {
        let inner = self.inner.lock().expect("session core mutex poisoned");
        inner.conns.get(&conn_id).and_then(|conn_state| {
            conn_state
                .lock()
                .expect("send conn state mutex poisoned")
                .inflight_outgoing
                .remove(&request_id)
        })
    }

    pub(crate) fn prepare_response_for_method(
        &self,
        conn_id: ConnectionId,
        request_id: RequestId,
        method_id: vox_types::MethodId,
        response: &mut RequestResponse<'_>,
    ) {
        let mut inner = self.inner.lock().expect("session core mutex poisoned");
        let conn_state = get_or_create_send_conn_state(&mut inner, conn_id);
        let mut conn_state = conn_state.lock().expect("send conn state mutex poisoned");
        let key = (vox_types::BindingDirection::Response, method_id);
        if conn_state
            .send_tracker
            .has_sent_binding(method_id, vox_types::BindingDirection::Response)
        {
            response.schemas = Default::default();
            return;
        }

        let prepared = match &response.ret {
            vox_types::Payload::Value { shape, .. } => {
                match Self::get_or_plan_binding_for_shape(
                    &mut conn_state,
                    key,
                    request_id,
                    "response",
                    shape,
                ) {
                    Some(prepared) => prepared,
                    None => return,
                }
            }
            vox_types::Payload::PostcardBytes(_) => {
                tracing::error!(
                    "schema attachment failed: missing forwarded response schemas for method {:?}",
                    method_id
                );
                return;
            }
        };
        response.schemas = prepared.to_cbor();
    }

    /// Prepare response schemas from an explicit canonical root type and schema source.
    pub(crate) fn prepare_response_from_source(
        &self,
        conn_id: ConnectionId,
        _request_id: RequestId,
        method_id: vox_types::MethodId,
        root_type: &vox_types::TypeRef,
        source: &dyn vox_types::SchemaSource,
        response: &mut RequestResponse<'_>,
    ) {
        let mut inner = self.inner.lock().expect("session core mutex poisoned");
        let conn_state = get_or_create_send_conn_state(&mut inner, conn_id);
        let mut conn_state = conn_state.lock().expect("send conn state mutex poisoned");
        let key = (vox_types::BindingDirection::Response, method_id);
        if conn_state
            .send_tracker
            .has_sent_binding(method_id, vox_types::BindingDirection::Response)
        {
            response.schemas = Default::default();
            return;
        }
        let prepared =
            Self::get_or_plan_binding_from_source(&mut conn_state, key, root_type, source);
        response.schemas = prepared.to_cbor();
    }

    /// Prepare response schemas for a replay using the running code's
    /// static `&'static Shape` for the response type. Same connection-
    /// scoped dedup as fresh responses.
    pub(crate) fn prepare_response_from_shape(
        &self,
        conn_id: ConnectionId,
        request_id: RequestId,
        method_id: vox_types::MethodId,
        response_shape: &'static Shape,
        response: &mut RequestResponse<'_>,
    ) {
        let mut inner = self.inner.lock().expect("session core mutex poisoned");
        let conn_state = get_or_create_send_conn_state(&mut inner, conn_id);
        let mut conn_state = conn_state.lock().expect("send conn state mutex poisoned");
        let key = (vox_types::BindingDirection::Response, method_id);
        if conn_state
            .send_tracker
            .has_sent_binding(method_id, vox_types::BindingDirection::Response)
        {
            response.schemas = Default::default();
            return;
        }
        let prepared = match Self::get_or_plan_binding_for_shape(
            &mut conn_state,
            key,
            request_id,
            "response",
            response_shape,
        ) {
            Some(prepared) => prepared,
            None => return,
        };
        response.schemas = prepared.to_cbor();
    }

    fn get_or_plan_binding_for_shape(
        conn_state: &mut SendConnState,
        key: (vox_types::BindingDirection, vox_types::MethodId),
        request_id: RequestId,
        kind: &str,
        shape: &'static Shape,
    ) -> Option<vox_types::PreparedSchemaPlan> {
        if let Some(prepared) = conn_state.planned_bindings.get(&key) {
            return Some(prepared.clone());
        }
        match vox_types::SchemaSendTracker::plan_for_shape(shape) {
            Ok(prepared) => {
                vox_types::dlog!(
                    "[schema] planned {} {} schemas for method {:?} (req {:?})",
                    prepared.schemas.len(),
                    kind,
                    key.1,
                    request_id
                );
                conn_state.planned_bindings.insert(key, prepared.clone());
                Some(prepared)
            }
            Err(e) => {
                tracing::error!("schema extraction failed: {e}");
                None
            }
        }
    }

    fn get_or_plan_binding_from_source(
        conn_state: &mut SendConnState,
        key: (vox_types::BindingDirection, vox_types::MethodId),
        root_type: &vox_types::TypeRef,
        source: &dyn vox_types::SchemaSource,
    ) -> vox_types::PreparedSchemaPlan {
        if let Some(prepared) = conn_state.planned_bindings.get(&key) {
            return prepared.clone();
        }
        let prepared = vox_types::SchemaSendTracker::plan_from_source(root_type, source);
        conn_state.planned_bindings.insert(key, prepared.clone());
        prepared
    }

    fn plan_response_schema_send(
        conn_state: &mut SendConnState,
        request_id: RequestId,
        method_id: vox_types::MethodId,
        response: &mut RequestResponse<'_>,
        forwarded_schemas: Option<&vox_types::SchemaRecvTracker>,
    ) -> Option<PendingSchemaSend> {
        if conn_state
            .send_tracker
            .has_sent_binding(method_id, vox_types::BindingDirection::Response)
        {
            response.schemas = Default::default();
            return None;
        }

        let key = (vox_types::BindingDirection::Response, method_id);
        let prepared = if !response.schemas.is_empty() {
            conn_state
                .planned_bindings
                .get(&key)
                .cloned()
                .unwrap_or_else(|| {
                    let prepared_payload = vox_types::SchemaPayload::from_cbor(&response.schemas.0)
                        .expect("prepared schema payloads must be valid CBOR");
                    vox_types::PreparedSchemaPlan {
                        schemas: prepared_payload.schemas,
                        root: prepared_payload.root,
                    }
                })
        } else {
            match &response.ret {
                vox_types::Payload::Value { shape, .. } => Self::get_or_plan_binding_for_shape(
                    conn_state, key, request_id, "response", shape,
                )?,
                vox_types::Payload::PostcardBytes(_) => {
                    let Some(source) = forwarded_schemas else {
                        tracing::error!(
                            "schema attachment failed: missing forwarded response schemas for method {:?}",
                            method_id
                        );
                        return None;
                    };
                    let Some(root) = source.get_remote_response_root(method_id) else {
                        tracing::error!(
                            "schema attachment failed: missing forwarded response root for method {:?}",
                            method_id
                        );
                        return None;
                    };
                    Self::get_or_plan_binding_from_source(conn_state, key, &root, source)
                }
            }
        };

        Some(PendingSchemaSend {
            method_id,
            direction: vox_types::BindingDirection::Response,
            prepared,
        })
    }

    fn plan_call_schema_send(
        conn_state: &mut SendConnState,
        request_id: RequestId,
        method_id: vox_types::MethodId,
        call: &mut vox_types::RequestCall<'_>,
        forwarded_schemas: Option<&vox_types::SchemaRecvTracker>,
    ) -> Option<PendingSchemaSend> {
        conn_state.inflight_outgoing.insert(request_id, method_id);
        if conn_state
            .send_tracker
            .has_sent_binding(method_id, vox_types::BindingDirection::Args)
        {
            call.schemas = Default::default();
            return None;
        }

        let key = (vox_types::BindingDirection::Args, method_id);
        let prepared = match &call.args {
            vox_types::Payload::Value { shape, .. } => {
                Self::get_or_plan_binding_for_shape(conn_state, key, request_id, "args", shape)?
            }
            vox_types::Payload::PostcardBytes(_) => {
                let Some(source) = forwarded_schemas else {
                    tracing::error!(
                        "schema attachment failed: missing forwarded args schemas for method {:?}",
                        method_id
                    );
                    return None;
                };
                let Some(root) = source.get_remote_args_root(method_id) else {
                    tracing::error!(
                        "schema attachment failed: missing forwarded args root for method {:?}",
                        method_id
                    );
                    return None;
                };
                Self::get_or_plan_binding_from_source(conn_state, key, &root, source)
            }
        };

        Some(PendingSchemaSend {
            method_id,
            direction: vox_types::BindingDirection::Args,
            prepared,
        })
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
    fn prepare_msg<'a>(
        self: Arc<Self>,
        msg: Message<'a>,
        binder: Option<&'a dyn vox_types::ChannelBinder>,
    ) -> std::io::Result<OutboundSend>;
}
pub trait DynConduitRx: MaybeSend {
    fn recv_msg<'a>(&'a mut self)
    -> BoxFut<'a, std::io::Result<Option<SelfRef<Message<'static>>>>>;
}

// r[impl zerocopy.send]
// r[impl zerocopy.framing.pipeline.outgoing]
impl<T> DynConduitTx for T
where
    T: ConduitTx<Msg = MessageFamily> + MaybeSend + MaybeSync + 'static,
{
    fn prepare_msg<'a>(
        self: Arc<Self>,
        msg: Message<'a>,
        binder: Option<&'a dyn vox_types::ChannelBinder>,
    ) -> std::io::Result<OutboundSend> {
        let prepared = if let Some(binder) = binder {
            vox_types::with_channel_binder(binder, || self.prepare_send(msg))
        } else {
            self.prepare_send(msg)
        };
        let prepared = prepared.map_err(|e| std::io::Error::other(e.to_string()))?;
        Ok(Box::pin(async move {
            self.send_prepared(prepared)
                .await
                .map_err(|e| std::io::Error::other(e.to_string()))
        }))
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
