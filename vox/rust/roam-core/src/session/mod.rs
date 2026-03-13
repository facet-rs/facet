use std::{collections::BTreeMap, pin::Pin, sync::Arc, time::Duration};

use moire::sync::mpsc;
use roam_types::{
    ChannelMessage, Conduit, ConduitRx, ConduitTx, ConduitTxPermit, ConnectionAccept,
    ConnectionClose, ConnectionId, ConnectionOpen, ConnectionReject, ConnectionSettings,
    IdAllocator, MaybeSend, MaybeSync, Message, MessageFamily, MessagePayload, Metadata, Parity,
    RequestBody, RequestId, RequestMessage, RequestResponse, SelfRef, SessionResumeKey,
    SessionRole, append_retry_support_metadata, append_session_resume_key_metadata,
    metadata_session_resume_key, metadata_supports_retry,
};
use tokio::sync::watch;
use tracing::{debug, warn};

mod builders;
pub use builders::*;

// r[impl session.handshake]
/// Current roam session protocol version.
pub const PROTOCOL_VERSION: u32 = 7;

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
    result_tx: moire::sync::oneshot::Sender<Result<(), SessionError>>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum DropControlRequest {
    Shutdown,
    Close(ConnectionId),
}

#[derive(Clone, Copy)]
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

    pub async fn resume<I: crate::IntoConduit>(&self, into_conduit: I) -> Result<(), SessionError>
    where
        I::Conduit: Conduit<Msg = MessageFamily> + 'static,
        <I::Conduit as Conduit>::Tx: MaybeSend + MaybeSync + 'static,
        for<'p> <<I::Conduit as Conduit>::Tx as ConduitTx>::Permit<'p>: MaybeSend,
        <I::Conduit as Conduit>::Rx: MaybeSend + 'static,
    {
        let (tx, rx) = into_conduit.into_conduit().split();
        self.resume_parts(Arc::new(tx), Box::new(rx)).await
    }

    pub(crate) async fn resume_parts(
        &self,
        tx: Arc<dyn DynConduitTx>,
        rx: Box<dyn DynConduitRx>,
    ) -> Result<(), SessionError> {
        let (result_tx, result_rx) = moire::sync::oneshot::channel("session.resume_result");
        self.resume_tx
            .send(ResumeRequest { tx, rx, result_tx })
            .await
            .map_err(|_| SessionError::Protocol("session closed".into()))?;
        result_rx
            .await
            .map_err(|_| SessionError::Protocol("session closed".into()))?
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
    conn_tx: mpsc::Sender<SelfRef<ConnectionMessage<'static>>>,
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
    sess_core: Arc<SessionCore>,
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
        }),
        RequestBody::Response(response) => RequestBody::Response(RequestResponse {
            channels: response.channels.clone(),
            metadata: response.metadata.clone(),
            ret: forwarded_payload(&response.ret),
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
    pub(crate) rx: mpsc::Receiver<SelfRef<ConnectionMessage<'static>>>,
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
            msg = left_rx.recv() => {
                let Some(msg) = msg else {
                    break;
                };
                if right_sender.send_owned(msg).await.is_err() {
                    break;
                }
            }
            msg = right_rx.recv() => {
                let Some(msg) = msg else {
                    break;
                };
                if left_sender.send_owned(msg).await.is_err() {
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

fn fresh_session_resume_key() -> Result<SessionResumeKey, SessionError> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|error| {
        SessionError::Protocol(format!("failed to generate session key: {error}"))
    })?;
    Ok(SessionResumeKey(bytes))
}

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
            tx: std::sync::Mutex::new(Arc::new(tx) as Arc<dyn DynConduitTx>),
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
        }
    }

    pub(crate) fn resume_key(&self) -> Option<SessionResumeKey> {
        self.session_resume_key
    }

    // r[impl session.handshake]
    async fn establish_as_initiator(
        &mut self,
        settings: ConnectionSettings,
        metadata: Metadata<'_>,
    ) -> Result<ConnectionHandle, SessionError> {
        use roam_types::{Hello, MessagePayload};

        self.role = SessionRole::Initiator;
        self.parity = settings.parity;
        self.conn_ids = IdAllocator::new(settings.parity);
        self.local_root_settings = settings.clone();

        let mut hello_metadata = metadata.to_vec();
        append_retry_support_metadata(&mut hello_metadata);

        // Send Hello
        self.sess_core
            .send(Message {
                connection_id: ConnectionId::ROOT,
                payload: MessagePayload::Hello(Hello {
                    version: PROTOCOL_VERSION,
                    connection_settings: settings.clone(),
                    metadata: hello_metadata,
                }),
            })
            .await
            .map_err(|_| SessionError::Protocol("failed to send Hello".into()))?;

        // Receive HelloYourself
        let (peer_settings, peer_supports_retry, session_resume_key) =
            match self.rx.recv_msg().await {
                Ok(Some(msg)) => {
                    let payload = msg.map(|m| m.payload);
                    match &*payload {
                        MessagePayload::HelloYourself(hy) => (
                            hy.connection_settings.clone(),
                            metadata_supports_retry(&hy.metadata),
                            metadata_session_resume_key(&hy.metadata),
                        ),
                        MessagePayload::ProtocolError(e) => {
                            return Err(SessionError::Protocol(e.description.to_owned()));
                        }
                        _ => {
                            return Err(SessionError::Protocol("expected HelloYourself".into()));
                        }
                    }
                }
                Ok(None) => {
                    return Err(SessionError::Protocol(
                        "peer closed during handshake".into(),
                    ));
                }
                Err(e) => return Err(SessionError::Protocol(e.to_string())),
            };
        self.peer_supports_retry = peer_supports_retry;
        self.peer_root_settings = Some(peer_settings.clone());
        self.session_resume_key = session_resume_key;
        if self.resumable && self.session_resume_key.is_none() {
            return Err(SessionError::NotResumable);
        }

        Ok(self.make_root_handle(settings, peer_settings))
    }

    // r[impl session.handshake]
    #[moire::instrument]
    async fn establish_as_acceptor(
        &mut self,
        settings: ConnectionSettings,
        metadata: Metadata<'_>,
    ) -> Result<ConnectionHandle, SessionError> {
        use roam_types::{HelloYourself, MessagePayload};

        self.role = SessionRole::Acceptor;

        // Receive Hello
        let (peer_settings, peer_supports_retry) = match self.rx.recv_msg().await {
            Ok(Some(msg)) => {
                let payload = msg.map(|m| m.payload);
                match &*payload {
                    MessagePayload::Hello(h) => {
                        if h.version != PROTOCOL_VERSION {
                            return Err(SessionError::Protocol(format!(
                                "version mismatch: got {}, expected {PROTOCOL_VERSION}",
                                h.version
                            )));
                        }
                        (
                            h.connection_settings.clone(),
                            metadata_supports_retry(&h.metadata),
                        )
                    }
                    MessagePayload::ProtocolError(e) => {
                        return Err(SessionError::Protocol(e.description.to_owned()));
                    }
                    _ => {
                        return Err(SessionError::Protocol("expected Hello".into()));
                    }
                }
            }
            Ok(None) => {
                return Err(SessionError::Protocol(
                    "peer closed during handshake".into(),
                ));
            }
            Err(e) => return Err(SessionError::Protocol(e.to_string())),
        };
        self.peer_supports_retry = peer_supports_retry;

        // Acceptor parity is opposite of initiator
        let our_settings = ConnectionSettings {
            parity: peer_settings.parity.other(),
            ..settings
        };
        self.parity = our_settings.parity;
        self.conn_ids = IdAllocator::new(our_settings.parity);
        self.local_root_settings = our_settings.clone();
        self.peer_root_settings = Some(peer_settings.clone());

        let mut hello_metadata = metadata.to_vec();
        append_retry_support_metadata(&mut hello_metadata);
        if self.resumable {
            self.session_resume_key = Some(fresh_session_resume_key()?);
            append_session_resume_key_metadata(
                &mut hello_metadata,
                self.session_resume_key
                    .as_ref()
                    .expect("resumable acceptor must set a session key"),
            );
        }

        // Send HelloYourself
        self.sess_core
            .send(Message {
                connection_id: ConnectionId::ROOT,
                payload: MessagePayload::HelloYourself(HelloYourself {
                    connection_settings: our_settings.clone(),
                    metadata: hello_metadata,
                }),
            })
            .await
            .map_err(|_| SessionError::Protocol("failed to send HelloYourself".into()))?;

        Ok(self.make_root_handle(our_settings, peer_settings))
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
        let (conn_tx, conn_rx) = mpsc::channel::<SelfRef<ConnectionMessage<'static>>>(&label, 64);
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
                        Ok(Some(msg)) => self.handle_message(msg, &mut keepalive_runtime).await,
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
                Ok((tx, rx)) => {
                    let result = self.resume_session(tx, rx).await;
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
                    let result = self.resume_session(req.tx, req.rx).await;
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

    async fn resume_session(
        &mut self,
        tx: Arc<dyn DynConduitTx>,
        mut rx: Box<dyn DynConduitRx>,
    ) -> Result<(), SessionError> {
        let Some(peer_settings) = self.peer_root_settings.clone() else {
            return Err(SessionError::Protocol("missing peer root settings".into()));
        };

        match self.role {
            SessionRole::Initiator => {
                let Some(session_resume_key) = self.session_resume_key else {
                    return Err(SessionError::NotResumable);
                };
                let mut metadata = Vec::new();
                append_retry_support_metadata(&mut metadata);
                append_session_resume_key_metadata(&mut metadata, &session_resume_key);
                tx.send_msg(Message {
                    connection_id: ConnectionId::ROOT,
                    payload: MessagePayload::Hello(roam_types::Hello {
                        version: PROTOCOL_VERSION,
                        connection_settings: self.local_root_settings.clone(),
                        metadata,
                    }),
                })
                .await
                .map_err(SessionError::Io)?;

                let msg = rx.recv_msg().await.map_err(SessionError::Io)?;
                let Some(msg) = msg else {
                    return Err(SessionError::Protocol(
                        "peer closed during session resume".into(),
                    ));
                };
                let payload = msg.map(|m| m.payload);
                let hy = match &*payload {
                    MessagePayload::HelloYourself(hy) => hy,
                    MessagePayload::ProtocolError(e) => {
                        return Err(SessionError::Protocol(e.description.to_owned()));
                    }
                    _ => {
                        return Err(SessionError::Protocol(
                            "expected HelloYourself during session resume".into(),
                        ));
                    }
                };
                if hy.connection_settings != peer_settings {
                    return Err(SessionError::Protocol(
                        "peer root settings changed across session resume".into(),
                    ));
                }
                self.peer_supports_retry = metadata_supports_retry(&hy.metadata);
                self.session_resume_key =
                    metadata_session_resume_key(&hy.metadata).or(self.session_resume_key);
            }
            SessionRole::Acceptor => {
                let msg = rx.recv_msg().await.map_err(SessionError::Io)?;
                let Some(msg) = msg else {
                    return Err(SessionError::Protocol(
                        "peer closed during session resume".into(),
                    ));
                };
                let payload = msg.map(|m| m.payload);
                let hello = match &*payload {
                    MessagePayload::Hello(hello) => hello,
                    MessagePayload::ProtocolError(e) => {
                        return Err(SessionError::Protocol(e.description.to_owned()));
                    }
                    _ => {
                        return Err(SessionError::Protocol(
                            "expected Hello during session resume".into(),
                        ));
                    }
                };
                if hello.version != PROTOCOL_VERSION {
                    return Err(SessionError::Protocol(format!(
                        "version mismatch: got {}, expected {PROTOCOL_VERSION}",
                        hello.version
                    )));
                }
                if hello.connection_settings != peer_settings {
                    return Err(SessionError::Protocol(
                        "peer root settings changed across session resume".into(),
                    ));
                }
                let Some(expected_key) = self.session_resume_key else {
                    return Err(SessionError::NotResumable);
                };
                let Some(actual_key) = metadata_session_resume_key(&hello.metadata) else {
                    return Err(SessionError::Protocol("missing session resume key".into()));
                };
                if actual_key != expected_key {
                    return Err(SessionError::Protocol("session resume key mismatch".into()));
                }
                self.peer_supports_retry = metadata_supports_retry(&hello.metadata);

                let mut metadata = Vec::new();
                append_retry_support_metadata(&mut metadata);
                append_session_resume_key_metadata(&mut metadata, &expected_key);
                tx.send_msg(Message {
                    connection_id: ConnectionId::ROOT,
                    payload: MessagePayload::HelloYourself(roam_types::HelloYourself {
                        connection_settings: self.local_root_settings.clone(),
                        metadata,
                    }),
                })
                .await
                .map_err(SessionError::Io)?;
            }
        }

        self.sess_core.replace_tx(tx);
        self.rx = rx;
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
                let conn_tx = match self.conns.get(&conn_id) {
                    Some(ConnectionSlot::Active(state)) => state.conn_tx.clone(),
                    _ => return,
                };
                if conn_tx.send(r.map(ConnectionMessage::Request)).await.is_err() {
                    self.remove_connection(&conn_id);
                    self.maybe_request_shutdown_after_root_closed();
                }
            }
            MessagePayload::ChannelMessage(c) => {
                let conn_tx = match self.conns.get(&conn_id) {
                    Some(ConnectionSlot::Active(state)) => state.conn_tx.clone(),
                    _ => return,
                };
                if conn_tx.send(c.map(ConnectionMessage::Channel)).await.is_err() {
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
            // Hello, HelloYourself, ProtocolError: not valid post-handshake, drop.
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
    tx: std::sync::Mutex<Arc<dyn DynConduitTx>>,
}

impl SessionCore {
    pub(crate) async fn send<'a>(&self, msg: Message<'a>) -> Result<(), ()> {
        let tx = self.tx.lock().expect("session tx mutex poisoned").clone();
        tx.send_msg(msg).await.map_err(|_| ())
    }

    fn replace_tx(&self, tx: Arc<dyn DynConduitTx>) {
        *self.tx.lock().expect("session tx mutex poisoned") = tx;
    }
}

#[cfg(not(target_arch = "wasm32"))]
type BoxFuture<'a, T> = Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;
#[cfg(target_arch = "wasm32")]
type BoxFuture<'a, T> = Pin<Box<dyn std::future::Future<Output = T> + 'a>>;

#[cfg(not(target_arch = "wasm32"))]
pub(crate) trait ConduitRecoverer: Send {
    fn next_conduit<'a>(
        &'a mut self,
    ) -> BoxFuture<'a, Result<(Arc<dyn DynConduitTx>, Box<dyn DynConduitRx>), SessionError>>;
}

#[cfg(target_arch = "wasm32")]
pub(crate) trait ConduitRecoverer {
    fn next_conduit<'a>(
        &'a mut self,
    ) -> BoxFuture<'a, Result<(Arc<dyn DynConduitTx>, Box<dyn DynConduitRx>), SessionError>>;
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
