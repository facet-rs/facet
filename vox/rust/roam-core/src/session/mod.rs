use std::{
    collections::{BTreeMap, HashMap},
    pin::Pin,
    sync::Arc,
    time::Duration,
};

use moire::sync::mpsc;
use roam_types::{
    ChannelMessage, Conduit, ConduitRx, ConduitTx, ConduitTxPermit, ConnectionAccept,
    ConnectionClose, ConnectionId, ConnectionOpen, ConnectionReject, ConnectionSettings,
    HandshakeResult, IdAllocator, MaybeSend, MaybeSync, Message, MessageFamily, MessagePayload,
    Metadata, Parity, Payload, RequestBody, RequestId, RequestMessage, RequestResponse,
    SchemaSendTracker, SelfRef, SessionResumeKey, SessionRole,
};
use tokio::sync::watch;
use tracing::{debug, warn};

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
/// Registered on the session via the builder's `.on_connection()` method.
/// Called synchronously from the session run loop when a peer sends
/// `ConnectionOpen`. The acceptor returns either an `AcceptedConnection`
/// (with settings, metadata, and a setup callback that spawns the driver)
/// or rejection metadata.
// r[impl rpc.virtual-connection.accept]
pub trait ConnectionAcceptor: Send + 'static {
    fn accept(
        &self,
        conn_id: ConnectionId,
        peer_settings: &ConnectionSettings,
        metadata: &[roam_types::MetadataEntry],
    ) -> Result<AcceptedConnection, Metadata<'static>>;
}

/// Result of accepting a virtual connection.
pub struct AcceptedConnection {
    /// Our settings for this connection.
    pub settings: ConnectionSettings,
    /// Metadata to send back in ConnectionAccept.
    pub metadata: Metadata<'static>,
    /// Callback that receives the ConnectionHandle and spawns a Driver.
    pub setup: Box<dyn FnOnce(ConnectionHandle) + Send>,
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
    on_connection: Option<Box<dyn ConnectionAcceptor>>,

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

    /// Schema recv tracker — shared across all connections, reset on reconnection.
    schema_recv_tracker: Arc<roam_types::SchemaRecvTracker>,
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

fn forwarded_payload<'a>(payload: &'a roam_types::Payload<'static>) -> roam_types::Payload<'a> {
    let roam_types::Payload::Incoming(bytes) = payload else {
        unreachable!("proxy forwarding expects decoded incoming payload bytes")
    };
    roam_types::Payload::Incoming(bytes)
}

fn forwarded_request_body<'a>(body: &'a RequestBody<'static>) -> RequestBody<'a> {
    match body {
        RequestBody::Call(call) => RequestBody::Call(roam_types::RequestCall {
            method_id: call.method_id,
            channels: call.channels.clone(),
            metadata: call.metadata.clone(),
            args: forwarded_payload(&call.args),
            schemas: call.schemas.clone(),
        }),
        RequestBody::Response(response) => RequestBody::Response(RequestResponse {
            channels: response.channels.clone(),
            metadata: response.metadata.clone(),
            ret: forwarded_payload(&response.ret),
            schemas: response.schemas.clone(),
        }),
        RequestBody::Cancel(cancel) => RequestBody::Cancel(roam_types::RequestCancel {
            metadata: cancel.metadata.clone(),
        }),
    }
}

fn forwarded_channel_body<'a>(
    body: &'a roam_types::ChannelBody<'static>,
) -> roam_types::ChannelBody<'a> {
    match body {
        roam_types::ChannelBody::Item(item) => {
            roam_types::ChannelBody::Item(roam_types::ChannelItem {
                item: forwarded_payload(&item.item),
            })
        }
        roam_types::ChannelBody::Close(close) => {
            roam_types::ChannelBody::Close(roam_types::ChannelClose {
                metadata: close.metadata.clone(),
            })
        }
        roam_types::ChannelBody::Reset(reset) => {
            roam_types::ChannelBody::Reset(roam_types::ChannelReset {
                metadata: reset.metadata.clone(),
            })
        }
        roam_types::ChannelBody::GrantCredit(credit) => {
            roam_types::ChannelBody::GrantCredit(roam_types::ChannelGrantCredit {
                additional: credit.additional,
            })
        }
    }
}

impl ConnectionSender {
    /// Send an arbitrary connection message
    pub async fn send<'a>(&self, msg: ConnectionMessage<'a>) -> Result<(), ()> {
        let payload = match msg {
            ConnectionMessage::Request(r) => MessagePayload::RequestMessage(r),
            ConnectionMessage::Channel(c) => MessagePayload::ChannelMessage(c),
        };
        let message = Message {
            connection_id: self.connection_id,
            payload,
        };
        self.sess_core.send(message).await.map_err(|_| ())
    }

    /// Send a received connection message without re-materializing payload values.
    pub(crate) async fn send_owned(
        &self,
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
            .send(Message {
                connection_id: self.connection_id,
                payload,
            })
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

    /// Mark a request as failed by removing any pending response slot.
    /// Called when a send error occurs or no reply was sent.
    pub fn mark_failure(&self, request_id: RequestId, disposition: FailureDisposition) {
        let _ = self.failures.send((request_id, disposition));
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
    pub schemas: Arc<roam_types::SchemaRecvTracker>,
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
pub async fn proxy_connections(left: ConnectionHandle, right: ConnectionHandle) {
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
                if right_sender.send_owned(recv.msg).await.is_err() {
                    break;
                }
            }
            recv = right_rx.recv() => {
                let Some(recv) = recv else {
                    break;
                };
                if left_sender.send_owned(recv.msg).await.is_err() {
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
}

/// Errors that can occur during session establishment or operation.
#[derive(Debug)]
pub enum SessionError {
    Io(std::io::Error),
    Protocol(String),
    Rejected(Metadata<'static>),
    NotResumable,
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io error: {e}"),
            Self::Protocol(msg) => write!(f, "protocol error: {msg}"),
            Self::Rejected(_) => write!(f, "connection rejected"),
            Self::NotResumable => write!(f, "session is not resumable"),
        }
    }
}

impl std::error::Error for SessionError {}

impl Session {
    #[allow(clippy::too_many_arguments)]
    fn pre_handshake<Tx, Rx>(
        tx: Tx,
        rx: Rx,
        on_connection: Option<Box<dyn ConnectionAcceptor>>,
        open_rx: mpsc::Receiver<OpenRequest>,
        close_rx: mpsc::Receiver<CloseRequest>,
        resume_rx: mpsc::Receiver<ResumeRequest>,
        control_tx: mpsc::UnboundedSender<DropControlRequest>,
        control_rx: mpsc::UnboundedReceiver<DropControlRequest>,
        keepalive: Option<SessionKeepaliveConfig>,
        resumable: bool,
        recoverer: Option<Box<dyn ConduitRecoverer>>,
    ) -> Self
    where
        Tx: ConduitTx<Msg = MessageFamily> + MaybeSend + MaybeSync + 'static,
        for<'p> <Tx as ConduitTx>::Permit<'p>: MaybeSend,
        Rx: ConduitRx<Msg = MessageFamily> + MaybeSend + 'static,
    {
        let sess_core = Arc::new(SessionCore {
            inner: std::sync::Mutex::new(SessionCoreInner {
                tx: Arc::new(tx) as Arc<dyn DynConduitTx>,
                send_tracker: roam_types::SchemaSendTracker::new(),
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
            schema_recv_tracker: Arc::new(roam_types::SchemaRecvTracker::new()),
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
        self.conns.insert(
            conn_id,
            ConnectionSlot::Active(ConnectionState {
                id: conn_id,
                local_settings,
                peer_settings,
                conn_tx,
                closed_tx,
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
                    match msg {
                        Ok(Some(msg)) => {
                            tracing::debug!(conn_id = msg.connection_id.0, "session received message");
                            self.handle_message(msg, &mut keepalive_runtime).await;
                        }
                        Ok(None) => {
                            warn!("session recv loop ended: conduit returned EOF");
                            if !self.handle_conduit_break(&mut keepalive_runtime).await {
                                break;
                            }
                        }
                        Err(error) => {
                            warn!(error = %error, "session recv loop ended: conduit recv error");
                            if !self.handle_conduit_break(&mut keepalive_runtime).await {
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
        debug!("session recv loop exited");
    }

    async fn handle_conduit_break(
        &mut self,
        keepalive_runtime: &mut Option<KeepaliveRuntime>,
    ) -> bool {
        if !self.resumable {
            return false;
        }

        if let Some(recoverer) = self.recoverer.as_mut() {
            match recoverer.next_conduit().await {
                Ok((tx, rx, handshake_result)) => {
                    let result = self.resume_from_handshake(tx, rx, handshake_result);
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
                    let result = self.resume_from_handshake(req.tx, req.rx, req.handshake_result);
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

        if result.peer_settings != peer_settings {
            return Err(SessionError::Protocol(
                "peer root settings changed across session resume".into(),
            ));
        }

        self.peer_supports_retry = result.peer_supports_retry;
        self.session_resume_key = result.session_resume_key.or(self.session_resume_key);

        self.sess_core.replace_tx_and_reset_schemas(tx);
        self.rx = rx;
        // Reset the recv tracker on reconnection — type IDs are per-connection
        self.schema_recv_tracker = Arc::new(roam_types::SchemaRecvTracker::new());
        Ok(())
    }

    async fn handle_message(
        &mut self,
        msg: SelfRef<Message<'static>>,
        keepalive_runtime: &mut Option<KeepaliveRuntime>,
    ) {
        let conn_id = msg.connection_id;
        roam_types::selfref_match!(msg, payload {
            // r[impl connection.close.semantics]
            MessagePayload::ConnectionClose(_) => {
                if conn_id.is_root() {
                    warn!("received ConnectionClose for root connection");
                } else {
                    debug!(conn_id = conn_id.0, "received ConnectionClose for virtual connection");
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
                // Record any inlined schemas from the incoming request before routing
                {
                    let schemas_cbor = match &r.body {
                        RequestBody::Call(call) => Some(&call.schemas),
                        RequestBody::Response(resp) => Some(&resp.schemas),
                        _ => None,
                    };
                    if let Some(schemas_cbor) = schemas_cbor {
                        if !schemas_cbor.is_empty() {
                            match schemas_cbor.parse() {
                                Ok(payload) => {
                                    if let Err(e) = self.schema_recv_tracker.record_received(payload) {
                                        warn!("failed to record received schemas: {}", e);
                                    }
                                }
                                Err(e) => {
                                    warn!("failed to parse inlined schemas: {}", e);
                                }
                            }
                        }
                    }
                }
                let conn_tx = match self.conns.get(&conn_id) {
                    Some(ConnectionSlot::Active(state)) => state.conn_tx.clone(),
                    _ => return,
                };
                let recv_msg = RecvMessage {
                    schemas: Arc::clone(&self.schema_recv_tracker),
                    msg: r.map(ConnectionMessage::Request),
                };
                if conn_tx.send(recv_msg).await.is_err() {
                    self.remove_connection(&conn_id);
                    self.maybe_request_shutdown_after_root_closed();
                }
            }
            MessagePayload::ChannelMessage(c) => {
                let conn_tx = match self.conns.get(&conn_id) {
                    Some(ConnectionSlot::Active(state)) => state.conn_tx.clone(),
                    _ => return,
                };
                let recv_msg = RecvMessage {
                    schemas: Arc::clone(&self.schema_recv_tracker),
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
                        payload: MessagePayload::Pong(roam_types::Pong { nonce: ping.nonce }),
                    })
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
            .send(Message {
                connection_id: ConnectionId::ROOT,
                payload: MessagePayload::Ping(roam_types::Ping { nonce }),
            })
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
                .send(Message {
                    connection_id: conn_id,
                    payload: MessagePayload::ConnectionReject(roam_types::ConnectionReject {
                        metadata: vec![],
                    }),
                })
                .await;
            return;
        }

        // Validate: connection ID must not already be in use.
        if self.conns.contains_key(&conn_id) {
            // Protocol error: duplicate connection ID.
            let _ = self
                .sess_core
                .send(Message {
                    connection_id: conn_id,
                    payload: MessagePayload::ConnectionReject(roam_types::ConnectionReject {
                        metadata: vec![],
                    }),
                })
                .await;
            return;
        }

        // r[impl connection.open.rejection]
        // Call the acceptor callback. If none is registered, reject.
        let acceptor = match &self.on_connection {
            Some(a) => a,
            None => {
                let _ = self
                    .sess_core
                    .send(Message {
                        connection_id: conn_id,
                        payload: MessagePayload::ConnectionReject(roam_types::ConnectionReject {
                            metadata: vec![],
                        }),
                    })
                    .await;
                return;
            }
        };

        match acceptor.accept(conn_id, &open.connection_settings, &open.metadata) {
            Ok(accepted) => {
                // Create the connection handle and activate it.
                let handle = self.make_connection_handle(
                    conn_id,
                    accepted.settings.clone(),
                    open.connection_settings.clone(),
                );

                // Send ConnectionAccept to the peer.
                let _ = self
                    .sess_core
                    .send(Message {
                        connection_id: conn_id,
                        payload: MessagePayload::ConnectionAccept(roam_types::ConnectionAccept {
                            connection_settings: accepted.settings,
                            metadata: accepted.metadata,
                        }),
                    })
                    .await;

                // Let the acceptor set up its driver.
                (accepted.setup)(handle);
            }
            Err(reject_metadata) => {
                let _ = self
                    .sess_core
                    .send(Message {
                        connection_id: conn_id,
                        payload: MessagePayload::ConnectionReject(roam_types::ConnectionReject {
                            metadata: reject_metadata,
                        }),
                    })
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
            .send(Message {
                connection_id: conn_id,
                payload: MessagePayload::ConnectionOpen(ConnectionOpen {
                    connection_settings: req.settings.clone(),
                    metadata: req.metadata,
                }),
            })
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
            .send(Message {
                connection_id: req.conn_id,
                payload: MessagePayload::ConnectionClose(ConnectionClose {
                    metadata: req.metadata,
                }),
            })
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
                debug!("session shutdown requested");
                false
            }
            DropControlRequest::Close(conn_id) => {
                // r[impl rpc.caller.liveness.last-drop-closes-connection]
                if conn_id.is_root() {
                    // r[impl rpc.caller.liveness.root-internal-close]
                    debug!("root callers dropped; internally closing root connection");
                    self.root_closed_internal = true;
                    // r[impl rpc.caller.liveness.root-teardown-condition]
                    return self.has_virtual_connections();
                }

                if self.remove_connection(&conn_id).is_some() {
                    let _ = self
                        .sess_core
                        .send(Message {
                            connection_id: conn_id,
                            payload: MessagePayload::ConnectionClose(ConnectionClose {
                                metadata: vec![],
                            }),
                        })
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
        let slot = self.conns.remove(conn_id);
        if let Some(ConnectionSlot::Active(state)) = &slot {
            let _ = state.closed_tx.send(true);
        }
        slot
    }

    fn close_all_connections(&mut self) {
        for slot in self.conns.values() {
            if let ConnectionSlot::Active(state) = slot {
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
    /// Tracks which schemas we have sent
    send_tracker: SchemaSendTracker,

    /// Used to pair outgoing responses to a MethodID, which is going to
    /// let us know if we need to attach response schemas via the send tracker
    inflight_incoming: HashMap<RequestId, roam_types::MethodId>,
}

struct SessionCoreInner {
    /// Underlying conduit (tx end)
    tx: Arc<dyn DynConduitTx>,

    /// Per-connection state re: sent schemas, etc.
    conns: HashMap<ConnectionId, SendConnState>,
}

impl SessionCore {
    pub(crate) async fn send<'a>(&self, msg: Message<'a>) -> Result<(), ()> {
        let tx = self
            .inner
            .lock()
            .expect("session core mutex poisoned")
            .tx
            .clone();
        tx.send_msg(msg).await.map_err(|_| ())
    }

    fn replace_tx_and_reset_schemas(&self, tx: Arc<dyn DynConduitTx>) {
        let mut inner = self.inner.lock().expect("session core mutex poisoned");
        inner.tx = tx;
        inner.send_tracker.reset();
    }

    /// Prepare call schemas for a method's arg type.
    ///
    /// Called by `DriverCaller::call` before serializing the call, because
    /// the call is serialized to bytes before reaching `SessionCore::send()`.
    pub(crate) fn prepare_call_schemas(
        &self,
        method_id: roam_types::MethodId,
        arg_shape: &'static facet_core::Shape,
    ) -> roam_types::CborPayload {
        let mut inner = self.inner.lock().expect("session core mutex poisoned");
        inner.send_tracker.prepare_send_for_method(
            method_id,
            arg_shape,
            roam_types::BindingDirection::Args,
        )
    }

    /// Prepare response schemas for a method's response type.
    ///
    /// Called by `DriverReplySink::send_response_schemas` before the handler runs.
    /// Returns a `CborPayload` that should be attached to the `RequestResponse`.
    pub(crate) fn prepare_response_schemas(
        &self,
        method_id: roam_types::MethodId,
        response_shape: &'static facet_core::Shape,
    ) -> roam_types::CborPayload {
        let mut inner = self.inner.lock().expect("session core mutex poisoned");
        inner.send_tracker.prepare_send_for_method(
            method_id,
            response_shape,
            roam_types::BindingDirection::Response,
        )
    }
}

#[cfg(not(target_arch = "wasm32"))]
type BoxFuture<'a, T> = Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;
#[cfg(target_arch = "wasm32")]
type BoxFuture<'a, T> = Pin<Box<dyn std::future::Future<Output = T> + 'a>>;

#[cfg(not(target_arch = "wasm32"))]
pub(crate) trait ConduitRecoverer: Send {
    #[allow(clippy::type_complexity)]
    fn next_conduit<'a>(
        &'a mut self,
    ) -> BoxFuture<
        'a,
        Result<
            (
                Arc<dyn DynConduitTx>,
                Box<dyn DynConduitRx>,
                HandshakeResult,
            ),
            SessionError,
        >,
    >;
}

#[cfg(target_arch = "wasm32")]
pub(crate) trait ConduitRecoverer {
    #[allow(clippy::type_complexity)]
    fn next_conduit<'a>(
        &'a mut self,
    ) -> BoxFuture<
        'a,
        Result<
            (
                Arc<dyn DynConduitTx>,
                Box<dyn DynConduitRx>,
                HandshakeResult,
            ),
            SessionError,
        >,
    >;
}

#[cfg(not(target_arch = "wasm32"))]
pub trait DynConduitTx: Send + Sync {
    fn send_msg<'a>(&'a self, msg: Message<'a>) -> BoxFuture<'a, std::io::Result<()>>;
}
#[cfg(target_arch = "wasm32")]
pub trait DynConduitTx {
    fn send_msg<'a>(&'a self, msg: Message<'a>) -> BoxFuture<'a, std::io::Result<()>>;
}

#[cfg(not(target_arch = "wasm32"))]
pub trait DynConduitRx: Send {
    fn recv_msg<'a>(
        &'a mut self,
    ) -> BoxFuture<'a, std::io::Result<Option<SelfRef<Message<'static>>>>>;
}
#[cfg(target_arch = "wasm32")]
pub trait DynConduitRx {
    fn recv_msg<'a>(
        &'a mut self,
    ) -> BoxFuture<'a, std::io::Result<Option<SelfRef<Message<'static>>>>>;
}

// r[impl zerocopy.send]
// r[impl zerocopy.framing.pipeline.outgoing]
impl<T> DynConduitTx for T
where
    T: ConduitTx<Msg = MessageFamily> + MaybeSend + MaybeSync,
    for<'p> <T as ConduitTx>::Permit<'p>: MaybeSend,
{
    fn send_msg<'a>(&'a self, msg: Message<'a>) -> BoxFuture<'a, std::io::Result<()>> {
        Box::pin(async move {
            let permit = self.reserve().await?;
            permit
                .send(msg)
                .map_err(|e| std::io::Error::other(e.to_string()))
        })
    }
}

impl<T> DynConduitRx for T
where
    T: ConduitRx<Msg = MessageFamily> + MaybeSend,
{
    fn recv_msg<'a>(
        &'a mut self,
    ) -> BoxFuture<'a, std::io::Result<Option<SelfRef<Message<'static>>>>> {
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
    use roam_types::{Backing, Conduit, ConnectionAccept, ConnectionReject, SelfRef};

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
        )
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
}
