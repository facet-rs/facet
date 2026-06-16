use std::{
    collections::{BTreeMap, HashMap},
    future::Future,
    pin::Pin,
    sync::Arc,
    time::Duration,
};

use facet_core::Shape;
use tokio::sync::{mpsc as tokio_mpsc, oneshot as tokio_oneshot, watch};
use tracing::{trace, warn};
use vox_rt::sync::mpsc;
use vox_types::{
    BoxFut, ChannelMessage, ConduitRx, ConduitTx, ConnectionRole, ConnectionSettings, Handler,
    HandshakeResult, IdAllocator, LaneAccept, LaneClose, LaneId, LaneOpen, LaneReject, MaybeSend,
    MaybeSync, Message, MessageFamily, MessagePayload, Metadata, Parity, RequestBody, RequestId,
    RequestMessage, RequestResponse, SchemaMessage, SelfRef, TrySendError, VoxDebugSnapshot,
    VoxObserverHandle,
};
use vox_types::{
    ConnectionCloseReason, ConnectionDebugSnapshot, ConnectionDebugState, DecodeErrorKind,
    DriverTaskStatus,
};

mod builders;
pub use builders::*;

/// Connection-level protocol keepalive configuration.
#[derive(Debug, Clone, Copy)]
pub struct ConnectionKeepaliveConfig {
    pub ping_interval: Duration,
    pub pong_timeout: Duration,
}

// ---------------------------------------------------------------------------
// Connection acceptor trait
// ---------------------------------------------------------------------------

/// Metadata wrapper with typed getters for well-known `vox-*` keys.
///
/// Passed to [`LaneAcceptor::accept`] when a peer opens a connection.
pub struct LaneRequest<'a> {
    metadata: &'a vox_types::Metadata,
    service: &'a str,
}

impl<'a> LaneRequest<'a> {
    /// Build a connection request from metadata.
    ///
    /// Returns an error if the required `vox-service` metadata key is missing.
    pub fn new(metadata: &'a vox_types::Metadata) -> Result<Self, ConnectionError> {
        let service = vox_types::metadata_get_str(metadata, "vox-service").ok_or_else(|| {
            ConnectionError::Protocol("missing required vox-service metadata".into())
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

    /// Access the raw metadata map.
    pub fn metadata(&self) -> &'a vox_types::Metadata {
        self.metadata
    }
}

/// A connection that has been opened but not yet accepted.
///
/// The acceptor receives this and decides its fate by calling one of:
/// - `handle_with(handler)` — run a Driver with this handler (common case)
/// - `proxy_to(other_handle)` — pipe messages to/from another connection
/// - `into_handle()` — take the raw LaneHandle for custom use
pub struct PendingLane {
    handle: Option<LaneHandle>,
}

impl PendingLane {
    fn new(handle: LaneHandle) -> Self {
        Self {
            handle: Some(handle),
        }
    }

    /// Accept this connection and run a Driver with the given handler.
    pub fn handle_with(mut self, handler: impl Handler<crate::DriverReplySink> + 'static) {
        let handle = self.handle.take().expect("PendingLane already consumed");
        let conn_id = handle.connection_id();
        trace!(%conn_id, "PendingLane::handle_with: creating driver");
        let mut driver = crate::Driver::new(handle, handler);
        #[cfg(not(target_arch = "wasm32"))]
        tokio::spawn(async move {
            trace!(%conn_id, "PendingLane driver starting");
            driver.run().await;
            trace!(%conn_id, "PendingLane driver exited");
        });
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move { driver.run().await });
    }

    /// Accept this connection, run a Driver, and return a typed client for the peer.
    pub fn handle_with_client<C: crate::FromVoxLane>(
        mut self,
        handler: impl Handler<crate::DriverReplySink> + 'static,
    ) -> C {
        let handle = self.handle.take().expect("PendingLane already consumed");
        let conn_id = handle.connection_id();
        trace!(%conn_id, "PendingLane::handle_with_client: creating driver");
        let mut driver = crate::Driver::new(handle, handler);
        let caller = crate::Caller::new(driver.caller());
        #[cfg(not(target_arch = "wasm32"))]
        tokio::spawn(async move {
            trace!(%conn_id, "PendingLane driver starting");
            driver.run().await;
            trace!(%conn_id, "PendingLane driver exited");
        });
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move { driver.run().await });
        C::from_vox_lane(caller, None)
    }

    /// Accept this connection and proxy all traffic to/from another connection.
    pub fn proxy_to(mut self, other: LaneHandle) {
        let handle = self.handle.take().expect("PendingLane already consumed");
        #[cfg(not(target_arch = "wasm32"))]
        tokio::spawn(async move {
            let _ = proxy_lanes(handle, other).await;
        });
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move {
            let _ = proxy_lanes(handle, other).await;
        });
    }

    /// Take the raw LaneHandle for custom use.
    pub fn into_handle(mut self) -> LaneHandle {
        self.handle.take().expect("PendingLane already consumed")
    }
}

impl Drop for PendingLane {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let conn_id = handle.connection_id();
            warn!(%conn_id, "PendingLane dropped without being consumed — closing connection");
            if let Some(tx) = handle.control_tx.as_ref() {
                let _ = send_drop_control(tx, DropControlRequest::Close(conn_id));
            }
        }
    }
}

// r[impl rpc.virtual-connection.accept]
pub trait LaneAcceptor: MaybeSend + MaybeSync + 'static {
    fn accept(&self, request: &LaneRequest, connection: PendingLane) -> Result<(), Metadata>;
}

/// Any `Handler<DriverReplySink>` is automatically a `LaneAcceptor`.
impl<H> LaneAcceptor for H
where
    H: Handler<crate::DriverReplySink> + Clone + MaybeSend + MaybeSync + 'static,
{
    fn accept(&self, _request: &LaneRequest, connection: PendingLane) -> Result<(), Metadata> {
        connection.handle_with(self.clone());
        Ok(())
    }
}

/// Wrapper that turns a closure into a `LaneAcceptor`.
pub struct LaneAcceptorFn<F>(pub F);

impl<F> LaneAcceptor for LaneAcceptorFn<F>
where
    F: Fn(&LaneRequest, PendingLane) -> Result<(), Metadata> + MaybeSend + MaybeSync + 'static,
{
    fn accept(&self, request: &LaneRequest, connection: PendingLane) -> Result<(), Metadata> {
        (self.0)(request, connection)
    }
}

/// Create a `LaneAcceptor` from a closure.
pub fn lane_acceptor_fn<F>(f: F) -> LaneAcceptorFn<F>
where
    F: Fn(&LaneRequest, PendingLane) -> Result<(), Metadata> + MaybeSend + MaybeSync + 'static,
{
    LaneAcceptorFn(f)
}

// ---------------------------------------------------------------------------
// Open/close request types (from ConnectionHandle → run loop)
// ---------------------------------------------------------------------------

struct OpenRequest {
    settings: ConnectionSettings,
    metadata: Metadata,
    result_tx: vox_rt::sync::oneshot::Sender<Result<LaneHandle, ConnectionError>>,
}

struct CloseRequest {
    conn_id: LaneId,
    metadata: Metadata,
    result_tx: vox_rt::sync::oneshot::Sender<Result<(), ConnectionError>>,
}

#[derive(Debug, Clone)]
pub(crate) enum DropControlRequest {
    Shutdown,
    Close(LaneId),
    ProtocolClose {
        conn_id: LaneId,
        description: String,
    },
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
// ConnectionHandle — cloneable handle for opening/closing service lanes
// ---------------------------------------------------------------------------

/// Cloneable handle for opening and closing service lanes.
///
/// The connection's `run()` loop must be running concurrently for lane-open
/// requests and RPC traffic to be processed.
// r[impl rpc.virtual-connection.open]
#[derive(Clone)]
pub struct ConnectionHandle {
    open_tx: mpsc::Sender<OpenRequest>,
    close_tx: mpsc::Sender<CloseRequest>,
    control_tx: mpsc::UnboundedSender<DropControlRequest>,
    _control_caller: Option<crate::Caller>,
}

impl ConnectionHandle {
    /// Resolve when the connection's private control lane closes.
    pub async fn closed(&self) {
        if let Some(caller) = &self._control_caller {
            caller.closed().await;
        }
    }

    /// Open a typed service lane on this connection using default lane limits.
    ///
    /// Sends `vox-service` metadata automatically from the client's
    /// `SERVICE_NAME`. Creates a `Driver` and spawns it, returning
    /// a ready-to-use typed client.
    pub async fn open_lane<Client: crate::FromVoxLane>(&self) -> Result<Client, ConnectionError> {
        self.open_lane_with_settings(ConnectionSettings {
            parity: Parity::Odd,
            max_concurrent_requests: 64,
            initial_channel_credit: vox_types::DEFAULT_INITIAL_CHANNEL_CREDIT,
        })
        .await
    }

    /// Open a typed service lane with explicit lane settings.
    pub async fn open_lane_with_settings<Client: crate::FromVoxLane>(
        &self,
        settings: ConnectionSettings,
    ) -> Result<Client, ConnectionError> {
        use crate::{Caller, Driver};

        let metadata = vox_types::metadata()
            .str(
                crate::connection::builders::VOX_SERVICE_METADATA_KEY,
                Client::SERVICE_NAME,
            )
            .build();
        let handle = self.open_lane_handle(settings, metadata).await?;
        let mut driver = Driver::new(handle, ());
        let caller = Caller::new(driver.caller());
        #[cfg(not(target_arch = "wasm32"))]
        tokio::spawn(async move { driver.run().await });
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move { driver.run().await });
        Ok(Client::from_vox_lane(caller, Some(self.clone())))
    }

    /// Open a raw service lane on this connection.
    ///
    /// Allocates a lane ID, sends `LaneOpen` to the peer, and waits for
    /// `LaneAccept` or `LaneReject`. The connection's `run()` loop processes
    /// the response and completes the returned future.
    // r[impl connection.open]
    pub async fn open_lane_handle(
        &self,
        settings: ConnectionSettings,
        metadata: Metadata,
    ) -> Result<LaneHandle, ConnectionError> {
        let (result_tx, result_rx) = vox_rt::sync::oneshot::channel("session.open_result");
        self.open_tx
            .send(OpenRequest {
                settings,
                metadata,
                result_tx,
            })
            .await
            .map_err(|_| ConnectionError::Protocol("connection closed".into()))?;
        result_rx
            .await
            .map_err(|_| ConnectionError::Protocol("connection closed".into()))?
    }

    /// Close an open service lane.
    ///
    /// Sends `LaneClose` to the peer and removes the lane slot. After this
    /// returns, no further messages will be routed to the lane's driver.
    // r[impl connection.close]
    pub async fn close_lane(
        &self,
        lane_id: LaneId,
        metadata: Metadata,
    ) -> Result<(), ConnectionError> {
        let (result_tx, result_rx) = vox_rt::sync::oneshot::channel("session.close_result");
        self.close_tx
            .send(CloseRequest {
                conn_id: lane_id,
                metadata,
                result_tx,
            })
            .await
            .map_err(|_| ConnectionError::Protocol("connection closed".into()))?;
        result_rx
            .await
            .map_err(|_| ConnectionError::Protocol("connection closed".into()))?
    }

    /// Request shutdown of the entire connection and all lanes.
    pub fn shutdown(&self) -> Result<(), ConnectionError> {
        send_drop_control(&self.control_tx, DropControlRequest::Shutdown)
            .map_err(|_| ConnectionError::Protocol("connection closed".into()))
    }
}

// ---------------------------------------------------------------------------
// Connection
// ---------------------------------------------------------------------------

/// Connection state machine.
// r[impl session]
// r[impl rpc.one-service-per-connection]
pub struct Connection {
    /// Conduit receiver
    rx: Box<dyn DynConduitRx>,

    // r[impl session.role]
    role: ConnectionRole,

    /// Our local parity — determines which connection IDs we allocate.
    // r[impl session.parity]
    parity: Parity,

    /// Shared core (for sending) — also held by all ConnectionSenders.
    sess_core: Arc<SessionCore>,
    local_root_settings: ConnectionSettings,
    peer_root_settings: Option<ConnectionSettings>,

    /// Connection state (active, pending inbound, pending outbound).
    conns: BTreeMap<LaneId, ConnectionSlot>,
    /// Allocator for outbound virtual connection IDs (uses session parity).
    conn_ids: IdAllocator<LaneId>,

    /// Callback for accepting inbound virtual connections.
    on_connection: Option<Arc<dyn LaneAcceptor>>,

    /// Receiver for open requests from ConnectionHandle.
    open_rx: mpsc::Receiver<OpenRequest>,

    /// Receiver for close requests from ConnectionHandle.
    close_rx: mpsc::Receiver<CloseRequest>,

    /// Sender/receiver for drop-driven session/connection control requests.
    control_tx: mpsc::UnboundedSender<DropControlRequest>,
    control_rx: mpsc::UnboundedReceiver<DropControlRequest>,

    /// Optional proactive keepalive runtime config for connection ID 0.
    keepalive: Option<ConnectionKeepaliveConfig>,

    observer: Option<VoxObserverHandle>,
}

#[derive(Debug)]
struct KeepaliveRuntime {
    ping_interval: Duration,
    pong_timeout: Duration,
    next_ping_at: vox_types::time::tokio::Instant,
    waiting_pong_nonce: Option<u64>,
    pong_deadline: vox_types::time::tokio::Instant,
    next_ping_nonce: u64,
}

// r[impl connection]
/// Static data for one active connection.
#[derive(Debug)]
pub struct ConnectionState {
    /// Unique connection identifier
    pub id: LaneId,

    /// Our settings
    pub local_settings: ConnectionSettings,

    /// The peer's settings
    pub peer_settings: ConnectionSettings,

    /// Sender for routing incoming messages to the per-connection driver task.
    conn_tx: mpsc::Sender<RecvMessage>,
    closed_tx: watch::Sender<Option<ConnectionCloseReason>>,

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
    result_tx: Option<vox_rt::sync::oneshot::Sender<Result<LaneHandle, ConnectionError>>>,
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
    lane_id: LaneId,
    pub(crate) sess_core: Arc<SessionCore>,
    failures: Arc<mpsc::UnboundedSender<(RequestId, FailureDisposition)>>,
}

fn forwarded_payload<'a>(payload: &'a vox_types::Payload<'a>) -> vox_types::Payload<'a> {
    let vox_types::Payload::Encoded(bytes) = payload else {
        unreachable!("proxy forwarding expects decoded incoming payload bytes")
    };
    vox_types::Payload::Encoded(bytes)
}

fn forwarded_request_body<'a>(body: &'a RequestBody<'a>) -> RequestBody<'a> {
    match body {
        RequestBody::Call(call) => RequestBody::Call(vox_types::RequestCall {
            method_id: call.method_id,
            channels: call.channels.clone(),
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

/// Swap a `Call`'s args to already-encoded `bytes`, narrowing the message
/// lifetime to that of `bytes` (`Message` is covariant in its lifetime). Used by
/// the out-of-band channel send path, where the args were pre-encoded into a
/// local buffer that outlives the synchronous `prepare_msg`.
fn swap_call_args_to_bytes<'s>(mut msg: Message<'s>, bytes: &'s [u8]) -> Message<'s> {
    if let MessagePayload::RequestMessage(req) = &mut msg.payload
        && let RequestBody::Call(call) = &mut req.body
    {
        call.args = vox_types::Payload::Encoded(bytes);
    }
    msg
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
    pub(crate) fn connection_id(&self) -> LaneId {
        self.lane_id
    }

    pub(crate) async fn send_with_binder<'a>(
        &self,
        msg: ConnectionMessage<'a>,
        binder: Option<&'a dyn vox_types::ChannelBinder>,
    ) -> Result<(), ()> {
        self.send_with_binder_and_method(msg, binder, None).await
    }

    pub(crate) async fn send_with_binder_and_method<'a>(
        &self,
        msg: ConnectionMessage<'a>,
        binder: Option<&'a dyn vox_types::ChannelBinder>,
        channel_method: Option<&'static vox_types::MethodDescriptor>,
    ) -> Result<(), ()> {
        let payload = match msg {
            ConnectionMessage::Request(r) => MessagePayload::RequestMessage(r),
            ConnectionMessage::Channel(c) => MessagePayload::ChannelMessage(c),
            ConnectionMessage::Schema(s) => MessagePayload::SchemaMessage(s),
        };
        let message = Message {
            lane_id: self.lane_id,
            payload,
        };
        self.sess_core
            .send_with_options(message, binder, None, channel_method, Vec::new())
            .await
            .map_err(|_| ())
    }

    pub(crate) async fn send_channel_with_writer_schema<'a>(
        &self,
        channel: ChannelMessage<'a>,
        writer_schema: Option<vox_types::ChannelWriterSchemaPlan>,
    ) -> Result<(), ()> {
        let extra_schema_sends = writer_schema
            .map(PendingSchemaSend::from)
            .into_iter()
            .collect();
        self.sess_core
            .send_with_options(
                Message {
                    lane_id: self.lane_id,
                    payload: MessagePayload::ChannelMessage(channel),
                },
                None,
                None,
                None,
                extra_schema_sends,
            )
            .await
            .map_err(|_| ())
    }

    /// Send an arbitrary connection message
    pub async fn send<'a>(&self, msg: ConnectionMessage<'a>) -> Result<(), ()> {
        self.send_with_binder(msg, None).await
    }

    pub(crate) fn try_send_channel_with_writer_schema<'a>(
        &self,
        channel: ChannelMessage<'a>,
        writer_schema: Option<vox_types::ChannelWriterSchemaPlan>,
    ) -> Result<(), TrySendError<()>> {
        let extra_schema_sends = writer_schema
            .map(PendingSchemaSend::from)
            .into_iter()
            .collect();
        self.sess_core.try_send_with_options(
            Message {
                lane_id: self.lane_id,
                payload: MessagePayload::ChannelMessage(channel),
            },
            None,
            None,
            None,
            extra_schema_sends,
        )
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
            ConnectionMessage::Schema(schema) => MessagePayload::SchemaMessage(SchemaMessage {
                method_id: schema.method_id,
                direction: schema.direction,
                schemas: schema.schemas.clone(),
            }),
        };

        self.sess_core
            .send(
                Message {
                    lane_id: self.lane_id,
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
        self.sess_core
            .prepare_response_for_method(self.lane_id, request_id, method_id, response);
    }

    /// Attach the method's response schema for an explicit wire `shape`. Used when the
    /// driver synthesizes an error response whose payload is an erased `Result` but
    /// which must advertise the method's real response schema so the caller can decode.
    pub(crate) fn prepare_response_for_shape(
        &self,
        request_id: RequestId,
        method_id: vox_types::MethodId,
        shape: &'static Shape,
        response: &mut RequestResponse<'_>,
    ) {
        self.sess_core.prepare_response_for_shape(
            self.lane_id,
            request_id,
            method_id,
            shape,
            response,
        );
    }

    /// Mark a request as failed by removing any pending response slot.
    /// Called when a send error occurs or no reply was sent.
    pub fn mark_failure(&self, request_id: RequestId, disposition: FailureDisposition) {
        let _ = self.failures.send((request_id, disposition));
    }
}

pub struct LaneHandle {
    pub(crate) sender: ConnectionSender,
    pub(crate) rx: mpsc::Receiver<RecvMessage>,
    pub(crate) failures_rx: mpsc::UnboundedReceiver<(RequestId, FailureDisposition)>,
    pub(crate) control_tx: Option<mpsc::UnboundedSender<DropControlRequest>>,
    pub(crate) closed_rx: watch::Receiver<Option<ConnectionCloseReason>>,
    pub(crate) local_settings: ConnectionSettings,
    pub(crate) peer_settings: ConnectionSettings,
    /// The parity this side should use for allocating request/channel IDs.
    pub parity: Parity,
    pub(crate) observer: Option<VoxObserverHandle>,
}

impl std::fmt::Debug for LaneHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LaneHandle")
            .field("lane_id", &self.sender.lane_id)
            .finish()
    }
}

pub(crate) enum ConnectionMessage<'payload> {
    Request(RequestMessage<'payload>),
    Channel(ChannelMessage<'payload>),
    Schema(SchemaMessage),
}

vox_types::impl_reborrow!(ConnectionMessage);

/// A message routed to a driver, carrying the `SchemaRecvTracker` that was
/// current when the session received it. This ensures each message uses the
/// correct tracker even across reconnections.
pub(crate) struct RecvMessage {
    pub schemas: Arc<vox_types::SchemaRecvTracker>,
    pub msg: SelfRef<ConnectionMessage<'static>>,
    /// Descriptors that arrived with this frame (`SCM_RIGHTS`). Threaded to
    /// the typed-decode site; `()` off-Unix.
    pub fds: vox_types::FrameFds,
}

impl LaneHandle {
    /// Returns the connection ID for this handle.
    pub fn connection_id(&self) -> LaneId {
        self.sender.lane_id
    }

    /// Resolve when this connection closes.
    pub async fn closed(&self) {
        if self.closed_rx.borrow().is_some() {
            return;
        }
        let mut rx = self.closed_rx.clone();
        while rx.changed().await.is_ok() {
            if rx.borrow().is_some() {
                return;
            }
        }
    }

    /// Return whether this connection is still considered connected.
    pub fn is_connected(&self) -> bool {
        self.closed_rx.borrow().is_none()
    }

    pub fn close_reason(&self) -> Option<ConnectionCloseReason> {
        *self.closed_rx.borrow()
    }

    // r[impl rpc.debug.snapshot]
    pub fn debug_snapshot(&self) -> VoxDebugSnapshot {
        let (outbound_queue_depth, outbound_queue_capacity) =
            self.sender.sess_core.outbound_queue_stats();
        VoxDebugSnapshot {
            connections: vec![ConnectionDebugSnapshot {
                connection_id: self.connection_id(),
                endpoint: None,
                surface: None,
                component: None,
                state: if self.closed_rx.borrow().is_some() {
                    ConnectionDebugState::Closed
                } else {
                    ConnectionDebugState::Open
                },
                outstanding_requests: 0,
                requests: Vec::new(),
                open_channels: Vec::new(),
                outbound_queue_depth: Some(outbound_queue_depth),
                outbound_queue_capacity: Some(outbound_queue_capacity),
                local_control_queue_depth: None,
                local_control_queue_capacity: None,
                last_inbound_message_at: None,
                last_outbound_message_at: None,
                last_progress_at: None,
                close_reason: *self.closed_rx.borrow(),
                driver_task_status: DriverTaskStatus::Unknown,
            }],
        }
    }

    pub fn dump_debug_snapshot(&self) -> VoxDebugSnapshot {
        let snapshot = self.debug_snapshot();
        tracing::info!(?snapshot, "vox debug snapshot");
        snapshot
    }
}

/// Forward all request/channel traffic between two connections.
///
/// This is a protocol-level bridge: it does not inspect service schemas or method IDs.
/// It exits when either side closes or a forward send fails, then requests closure of
/// both underlying connections.
pub async fn proxy_lanes(left: LaneHandle, right: LaneHandle) -> Result<(), ConnectionError> {
    if left.parity == right.parity {
        return Err(ConnectionError::Protocol(
            "proxy_lanes requires opposite parities".into(),
        ));
    }
    let left_conn_id = left.connection_id();
    let right_conn_id = right.connection_id();
    let LaneHandle {
        sender: left_sender,
        rx: mut left_rx,
        failures_rx: _left_failures_rx,
        control_tx: left_control_tx,
        closed_rx: _left_closed_rx,
        local_settings: _left_local_settings,
        peer_settings: _left_peer_settings,
        parity: _left_parity,
        observer: _left_observer,
    } = left;
    let LaneHandle {
        sender: right_sender,
        rx: mut right_rx,
        failures_rx: _right_failures_rx,
        control_tx: right_control_tx,
        closed_rx: _right_closed_rx,
        local_settings: _right_local_settings,
        peer_settings: _right_peer_settings,
        parity: _right_parity,
        observer: _right_observer,
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
pub enum ConnectionError {
    Io(std::io::Error),
    Protocol(String),
    Rejected(Metadata),
    ConnectTimeout,
}

impl ConnectionError {
    /// Returns `true` if a later connection attempt may succeed.
    ///
    /// I/O errors and timeouts are transient — the remote might become available
    /// shortly. Protocol errors and explicit rejections are permanent for this
    /// peer address.
    pub fn is_transient_connect_failure(&self) -> bool {
        matches!(self, Self::Io(_) | Self::ConnectTimeout)
    }
}

impl std::fmt::Display for ConnectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io error: {e}"),
            Self::Protocol(msg) => write!(f, "protocol error: {msg}"),
            Self::Rejected(_) => write!(f, "connection rejected"),
            Self::ConnectTimeout => write!(f, "connect timeout"),
        }
    }
}

impl std::error::Error for ConnectionError {}

fn classify_session_recv_error(error: &std::io::Error) -> ConnectionCloseReason {
    let message = error.to_string();
    if message.contains("decode error") || message.contains("protocol") {
        ConnectionCloseReason::Protocol
    } else {
        ConnectionCloseReason::Transport
    }
}

fn classify_decode_error(error: &std::io::Error) -> Option<DecodeErrorKind> {
    let message = error.to_string();
    if message.contains("decode error") {
        Some(DecodeErrorKind::Payload)
    } else {
        None
    }
}

impl Connection {
    // r[impl rpc.observability.session-errors]
    // r[impl rpc.observability.driver]
    fn observe_session_recv_error(&self, error: &std::io::Error) {
        let Some(observer) = &self.observer else {
            return;
        };

        if let Some(kind) = classify_decode_error(error) {
            for conn_id in self.conns.iter().filter_map(|(conn_id, slot)| {
                matches!(slot, ConnectionSlot::Active(_)).then_some(*conn_id)
            }) {
                observer.driver_event(vox_types::DriverEvent::DecodeError {
                    connection_id: conn_id,
                    kind,
                });
            }
            return;
        }

        observer.transport_event(vox_types::TransportEvent::Closed {
            connection_id: None,
            reason: classify_session_recv_error(error),
        });
    }

    fn close_connection_for_protocol_error(
        &mut self,
        conn_id: LaneId,
        detail: impl std::fmt::Display,
    ) {
        warn!(%conn_id, "closing connection after protocol error: {detail}");
        self.remove_connection_with_reason(&conn_id, ConnectionCloseReason::Protocol);
    }

    fn record_received_schema_bytes(
        &mut self,
        _conn_id: LaneId,
        schema_recv_tracker: Arc<vox_types::SchemaRecvTracker>,
        method_id: vox_types::MethodId,
        direction: vox_types::BindingDirection,
        schema_bytes: &vox_types::SchemaBytes,
        _context: &str,
    ) -> bool {
        // The `schemas` field carries the peer's phon self-describing schema closure.
        // Store it verbatim; recording is best-effort/idempotent (`r[schema.exchange]`),
        // so a duplicate binding is not a protocol error.
        schema_recv_tracker.record_received(method_id, direction, schema_bytes.0.clone());
        true
    }

    #[allow(clippy::too_many_arguments)]
    fn pre_handshake<Tx, Rx>(
        tx: Tx,
        rx: Rx,
        on_connection: Option<Arc<dyn LaneAcceptor>>,
        open_rx: mpsc::Receiver<OpenRequest>,
        close_rx: mpsc::Receiver<CloseRequest>,
        control_tx: mpsc::UnboundedSender<DropControlRequest>,
        control_rx: mpsc::UnboundedReceiver<DropControlRequest>,
        keepalive: Option<ConnectionKeepaliveConfig>,
        observer: Option<VoxObserverHandle>,
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
            observer: observer.clone(),
            channel_gates: std::sync::Mutex::new(HashMap::new()),
        });
        spawn_outbound_worker(outbound_rx);
        Connection {
            rx: Box::new(rx),
            role: ConnectionRole::Initiator, // overwritten in establish_as_*
            parity: Parity::Odd,             // overwritten in establish_as_*
            sess_core,
            local_root_settings: ConnectionSettings {
                parity: Parity::Odd,
                max_concurrent_requests: 64,
                initial_channel_credit: 16,
            },
            peer_root_settings: None,
            conns: BTreeMap::new(),
            conn_ids: IdAllocator::new(Parity::Odd), // overwritten in establish_as_*
            on_connection,
            open_rx,
            close_rx,
            control_tx,
            control_rx,
            keepalive,
            observer,
        }
    }

    // r[impl session.handshake]
    fn establish_from_handshake(
        &mut self,
        result: HandshakeResult,
    ) -> Result<LaneHandle, ConnectionError> {
        self.role = result.role;
        self.parity = result.our_settings.parity;
        self.conn_ids = IdAllocator::new(result.our_settings.parity);
        self.local_root_settings = result.our_settings.clone();
        self.peer_root_settings = Some(result.peer_settings.clone());

        Ok(self.make_root_handle(result.our_settings, result.peer_settings))
    }

    fn make_root_handle(
        &mut self,
        local_settings: ConnectionSettings,
        peer_settings: ConnectionSettings,
    ) -> LaneHandle {
        self.make_connection_handle(LaneId::ROOT, local_settings, peer_settings)
    }

    fn make_connection_handle(
        &mut self,
        conn_id: LaneId,
        local_settings: ConnectionSettings,
        peer_settings: ConnectionSettings,
    ) -> LaneHandle {
        let label = format!("session.conn{}", conn_id.0);
        let (conn_tx, conn_rx) = mpsc::channel::<RecvMessage>(&label, 64);
        let (failures_tx, failures_rx) = mpsc::unbounded_channel(format!("{label}.failures"));
        let (closed_tx, closed_rx) = watch::channel(None);
        let sender = ConnectionSender {
            lane_id: conn_id,
            sess_core: Arc::clone(&self.sess_core),
            failures: Arc::new(failures_tx),
        };

        let parity = local_settings.parity;
        let handle_local_settings = local_settings.clone();
        let handle_peer_settings = peer_settings.clone();
        trace!(%conn_id, "make_connection_handle: inserting slot into conns");
        if let Some(observer) = &self.observer {
            observer.driver_event(vox_types::DriverEvent::ConnectionOpened {
                connection_id: conn_id,
            });
        }
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

        LaneHandle {
            sender,
            rx: conn_rx,
            failures_rx,
            control_tx: Some(self.control_tx.clone()),
            closed_rx,
            local_settings: handle_local_settings,
            peer_settings: handle_peer_settings,
            parity,
            observer: self.observer.clone(),
        }
    }

    /// Run the session recv loop: read from the conduit, demux by connection
    /// ID, and route to the appropriate connection's driver. Also processes
    /// open/close requests from the ConnectionHandle.
    // r[impl session.message]
    pub async fn run(&mut self) {
        let mut keepalive_runtime = self.make_keepalive_runtime();
        let mut keepalive_tick = keepalive_runtime.as_ref().map(|_| {
            let mut interval = vox_types::time::tokio::interval(Duration::from_millis(10));
            interval.set_missed_tick_behavior(vox_types::time::tokio::MissedTickBehavior::Delay);
            interval
        });

        loop {
            tokio::select! {
                biased;

                msg = self.rx.recv_msg() => {
                    vox_types::dlog!("[session {:?}] recv_msg returned", self.role);
                    match msg {
                        Ok(Some(msg)) => {
                            // Capture the frame's descriptors before the next
                            // recv overwrites them; thread them with the msg.
                            let fds = self.rx.take_frame_fds();
                            self.handle_message(msg, fds, &mut keepalive_runtime).await;
                        }
                        Ok(None) => {
                            vox_types::dlog!("[session {:?}] recv loop: conduit returned EOF", self.role);
                            self.close_all_connections(ConnectionCloseReason::Remote);
                            break;
                        }
                        Err(error) => {
                            let close_reason = classify_session_recv_error(&error);
                            self.observe_session_recv_error(&error);
                            warn!(
                                role = ?self.role,
                                %error,
                                ?close_reason,
                                "session receive failed; closing connections if recovery is unavailable"
                            );
                            vox_types::dlog!("[session {:?}] recv loop: conduit recv error: {}", self.role, error);
                            self.close_all_connections(close_reason);
                            break;
                        }
                    }
                }
                Some(req) = self.open_rx.recv() => {
                    self.handle_open_request(req).await;
                }
                Some(req) = self.close_rx.recv() => {
                    self.handle_close_request(req).await;
                }
                Some(req) = self.control_rx.recv() => {
                    if !self.handle_drop_control_request(req).await {
                        self.close_all_connections(ConnectionCloseReason::Local);
                        break;
                    }
                }
                _ = async {
                    if let Some(interval) = keepalive_tick.as_mut() {
                        interval.tick().await;
                    }
                }, if keepalive_tick.is_some() => {
                    if !self.handle_keepalive_tick(&mut keepalive_runtime).await {
                        self.close_all_connections(ConnectionCloseReason::Protocol);
                        break;
                    }
                }
            }
        }

        // Drop all connection slots so per-connection drivers exit immediately.
        self.close_all_connections(ConnectionCloseReason::SessionShutdown);
        trace!("session recv loop exited");
    }

    async fn handle_message(
        &mut self,
        msg: SelfRef<Message<'static>>,
        fds: vox_types::FrameFds,
        keepalive_runtime: &mut Option<KeepaliveRuntime>,
    ) {
        let msg_ref = msg.get();
        let conn_id = msg_ref.lane_id;
        match &msg_ref.payload {
            MessagePayload::Ping(ping) => {
                // r[impl session.keepalive]
                let _ = self
                    .sess_core
                    .send(
                        Message {
                            lane_id: conn_id,
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
                    // r[impl session.keepalive]
                    self.handle_keepalive_pong(pong.nonce, keepalive_runtime);
                }
                return;
            }
            MessagePayload::SchemaMessage(schema_msg) => {
                let (schema_recv_tracker, conn_tx) = match self.conns.get(&conn_id) {
                    Some(ConnectionSlot::Active(state)) => (
                        Arc::clone(&state.schema_recv_tracker),
                        state.conn_tx.clone(),
                    ),
                    _ => return,
                };
                let _ = self.record_received_schema_bytes(
                    conn_id,
                    Arc::clone(&schema_recv_tracker),
                    schema_msg.method_id,
                    schema_msg.direction,
                    &schema_msg.schemas,
                    "standalone schema message",
                );
                let recv_msg = RecvMessage {
                    schemas: schema_recv_tracker,
                    msg: msg.map(|m| match m.payload {
                        MessagePayload::SchemaMessage(schema) => ConnectionMessage::Schema(schema),
                        _ => unreachable!(),
                    }),
                    fds,
                };
                if conn_tx.send(recv_msg).await.is_err() {
                    self.remove_connection_with_reason(&conn_id, ConnectionCloseReason::Unknown);
                }
                return;
            }
            _ => {}
        }
        vox_types::selfref_match!(msg, payload {
            // r[impl connection.close.semantics]
            MessagePayload::LaneClose(_) => {
                if conn_id.is_root() {
                    warn!("received LaneClose for root connection");
                } else {
                    trace!(conn_id = conn_id.0, "received LaneClose for virtual connection");
                }
                // Remove the connection — dropping conn_tx causes the Driver's rx
                // to return None, which exits its run loop. All in-flight handlers
                // are dropped, triggering DriverReplySink::drop → Cancelled responses.
                self.remove_connection_with_reason(&conn_id, ConnectionCloseReason::Remote);
            }
            MessagePayload::LaneOpen(open) => {
                self.handle_inbound_open(conn_id, open).await;
            }
            MessagePayload::LaneAccept(accept) => {
                self.handle_inbound_accept(conn_id, accept);
            }
            MessagePayload::LaneReject(reject) => {
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
                    let schema_bytes = match &r_ref.body {
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
                        schema_bytes.map(|s| s.0.len())
                    );
                    let schema_recv_tracker = match self.conns.get(&conn_id) {
                        Some(ConnectionSlot::Active(state)) => {
                            Arc::clone(&state.schema_recv_tracker)
                        }
                        _ => return,
                    };
                    if let Some(schema_bytes) = schema_bytes
                        && !schema_bytes.is_empty()
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
                        if !self.record_received_schema_bytes(
                            conn_id,
                            schema_recv_tracker,
                            method_id,
                            direction,
                            schema_bytes,
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
                    fds,
                };
                vox_types::dlog!(
                    "[session {:?}] dispatch request: conn={:?} req={:?} body={}",
                    self.role,
                    conn_id,
                    request_id,
                    body_kind
                );
                if conn_tx.send(recv_msg).await.is_err() {
                    self.remove_connection_with_reason(&conn_id, ConnectionCloseReason::Unknown);
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
                    fds,
                };
                if conn_tx.send(recv_msg).await.is_err() {
                    self.remove_connection_with_reason(&conn_id, ConnectionCloseReason::Unknown);
                }
            }
            MessagePayload::ProtocolError(_) => {
                warn!(%conn_id, "received protocol error from peer");
                self.close_all_connections(ConnectionCloseReason::Protocol);
                let _ = send_drop_control(&self.control_tx, DropControlRequest::Shutdown);
            }
        })
    }

    // r[impl session.keepalive]
    fn make_keepalive_runtime(&self) -> Option<KeepaliveRuntime> {
        let config = self.keepalive?;
        if config.ping_interval.is_zero() || config.pong_timeout.is_zero() {
            warn!("keepalive disabled due to non-positive interval/timeout");
            return None;
        }
        let now = vox_types::time::tokio::Instant::now();
        Some(KeepaliveRuntime {
            ping_interval: config.ping_interval,
            pong_timeout: config.pong_timeout,
            next_ping_at: now + config.ping_interval,
            waiting_pong_nonce: None,
            pong_deadline: now,
            next_ping_nonce: 1,
        })
    }

    // r[impl session.keepalive]
    fn handle_keepalive_pong(&self, nonce: u64, keepalive_runtime: &mut Option<KeepaliveRuntime>) {
        let Some(runtime) = keepalive_runtime.as_mut() else {
            return;
        };
        if runtime.waiting_pong_nonce != Some(nonce) {
            return;
        }
        runtime.waiting_pong_nonce = None;
        runtime.next_ping_at = vox_types::time::tokio::Instant::now() + runtime.ping_interval;
    }

    // r[impl session.keepalive]
    async fn handle_keepalive_tick(
        &mut self,
        keepalive_runtime: &mut Option<KeepaliveRuntime>,
    ) -> bool {
        let Some(runtime) = keepalive_runtime.as_mut() else {
            return true;
        };
        let now = vox_types::time::tokio::Instant::now();

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
                    lane_id: LaneId::ROOT,
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

    async fn handle_inbound_open(&mut self, conn_id: LaneId, open: SelfRef<LaneOpen>) {
        // Validate: connection ID must match peer's parity (opposite of ours).
        let peer_parity = self.parity.other();
        if !conn_id.has_parity(peer_parity) {
            // Protocol error: wrong parity. For now, just reject.
            let _ = self
                .sess_core
                .send(
                    Message {
                        lane_id: conn_id,
                        payload: MessagePayload::LaneReject(vox_types::LaneReject {
                            metadata: vox_types::Metadata::default(),
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
                        lane_id: conn_id,
                        payload: MessagePayload::LaneReject(vox_types::LaneReject {
                            metadata: vox_types::Metadata::default(),
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
                        lane_id: conn_id,
                        payload: MessagePayload::LaneReject(vox_types::LaneReject {
                            metadata: vox_types::Metadata::default(),
                        }),
                    },
                    None,
                    None,
                )
                .await;
            return;
        }

        // Derive settings: opposite parity, same limits for now.
        let open = open.get();
        if open.connection_settings.initial_channel_credit == 0 {
            let _ = self
                .sess_core
                .send(
                    Message {
                        lane_id: conn_id,
                        payload: MessagePayload::LaneReject(vox_types::LaneReject {
                            metadata: vox_types::metadata()
                                .str("error", "initial_channel_credit must be greater than zero")
                                .build(),
                        }),
                    },
                    None,
                    None,
                )
                .await;
            return;
        }

        let our_settings = ConnectionSettings {
            parity: open.connection_settings.parity.other(),
            max_concurrent_requests: open.connection_settings.max_concurrent_requests,
            initial_channel_credit: open.connection_settings.initial_channel_credit,
        };

        // Create the connection handle and activate it.
        let handle = self.make_connection_handle(
            conn_id,
            our_settings.clone(),
            open.connection_settings.clone(),
        );

        // Let the acceptor decide the connection's fate.
        let mut metadata = open.metadata.clone();
        vox_types::meta_set(&mut metadata, "vox-connection-kind", "virtual");
        let request = match LaneRequest::new(&metadata) {
            Ok(r) => r,
            Err(e) => {
                trace!(%conn_id, %e, "rejecting virtual connection");
                self.conns.remove(&conn_id);
                let _ = self
                    .sess_core
                    .send(
                        Message {
                            lane_id: conn_id,
                            payload: MessagePayload::LaneReject(vox_types::LaneReject {
                                metadata: vox_types::metadata().str("error", e.to_string()).build(),
                            }),
                        },
                        None,
                        None,
                    )
                    .await;
                return;
            }
        };
        let pending = PendingLane::new(handle);
        let acceptor = self.on_connection.as_ref().unwrap();
        trace!(%conn_id, "calling acceptor for virtual connection");
        match acceptor.accept(&request, pending) {
            Ok(()) => {
                trace!(%conn_id, "acceptor accepted virtual connection, sending LaneAccept");
                let _ = self
                    .sess_core
                    .send(
                        Message {
                            lane_id: conn_id,
                            payload: MessagePayload::LaneAccept(vox_types::LaneAccept {
                                connection_settings: our_settings,
                                metadata: vox_types::Metadata::default(),
                            }),
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
                            lane_id: conn_id,
                            payload: MessagePayload::LaneReject(vox_types::LaneReject {
                                metadata: reject_metadata,
                            }),
                        },
                        None,
                        None,
                    )
                    .await;
            }
        }
    }

    fn handle_inbound_accept(&mut self, conn_id: LaneId, accept: SelfRef<LaneAccept>) {
        let accept = accept.get();
        let slot = self.remove_connection(&conn_id);
        match slot {
            Some(ConnectionSlot::PendingOutbound(mut pending))
                if accept.connection_settings.initial_channel_credit == 0 =>
            {
                if let Some(tx) = pending.result_tx.take() {
                    let _ = tx.send(Err(ConnectionError::Protocol(
                        "initial_channel_credit must be greater than zero".into(),
                    )));
                }
            }
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

    fn handle_inbound_reject(&mut self, conn_id: LaneId, reject: SelfRef<LaneReject>) {
        let reject = reject.get();
        let slot = self.remove_connection(&conn_id);
        match slot {
            Some(ConnectionSlot::PendingOutbound(mut pending)) => {
                if let Some(tx) = pending.result_tx.take() {
                    let _ = tx.send(Err(ConnectionError::Rejected(reject.metadata.clone())));
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
        if req.settings.initial_channel_credit == 0 {
            let _ = req.result_tx.send(Err(ConnectionError::Protocol(
                "initial_channel_credit must be greater than zero".into(),
            )));
            return;
        }

        let conn_id = self.conn_ids.alloc();

        // Send LaneOpen to the peer.
        let send_result = self
            .sess_core
            .send(
                Message {
                    lane_id: conn_id,
                    payload: MessagePayload::LaneOpen(LaneOpen {
                        connection_settings: req.settings.clone(),
                        metadata: req.metadata,
                    }),
                },
                None,
                None,
            )
            .await;

        if send_result.is_err() {
            let _ = req.result_tx.send(Err(ConnectionError::Protocol(
                "failed to send LaneOpen".into(),
            )));
            return;
        }

        // Store the pending state. The run loop will complete the oneshot
        // when LaneAccept or LaneReject arrives.
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
            let _ = req.result_tx.send(Err(ConnectionError::Protocol(
                "cannot close root connection".into(),
            )));
            return;
        }

        // Remove the connection slot — this drops conn_tx and causes the
        // Driver to exit cleanly.
        if self
            .remove_connection_with_reason(&req.conn_id, ConnectionCloseReason::Local)
            .is_none()
        {
            let _ = req.result_tx.send(Err(ConnectionError::Protocol(
                "connection not found".into(),
            )));
            return;
        }

        // Send LaneClose to the peer.
        let send_result = self
            .sess_core
            .send(
                Message {
                    lane_id: req.conn_id,
                    payload: MessagePayload::LaneClose(LaneClose {
                        metadata: req.metadata,
                    }),
                },
                None,
                None,
            )
            .await;

        if send_result.is_err() {
            let _ = req.result_tx.send(Err(ConnectionError::Protocol(
                "failed to send LaneClose".into(),
            )));
            return;
        }

        let _ = req.result_tx.send(Ok(()));
    }

    async fn handle_drop_control_request(&mut self, req: DropControlRequest) -> bool {
        match req {
            DropControlRequest::Shutdown => {
                trace!("session shutdown requested");
                false
            }
            DropControlRequest::Close(conn_id) => {
                if conn_id.is_root() {
                    trace!("ignoring root close control request");
                    return true;
                }

                if self
                    .remove_connection_with_reason(&conn_id, ConnectionCloseReason::Local)
                    .is_some()
                {
                    let _ = self
                        .sess_core
                        .send(
                            Message {
                                lane_id: conn_id,
                                payload: MessagePayload::LaneClose(LaneClose {
                                    metadata: vox_types::Metadata::default(),
                                }),
                            },
                            None,
                            None,
                        )
                        .await;
                }

                true
            }
            DropControlRequest::ProtocolClose {
                conn_id,
                description,
            } => {
                trace!(%conn_id, %description, "protocol close requested");
                let _ = self
                    .sess_core
                    .send(
                        Message {
                            lane_id: LaneId::ROOT,
                            payload: MessagePayload::ProtocolError(vox_types::ProtocolError {
                                description: &description,
                            }),
                        },
                        None,
                        None,
                    )
                    .await;
                self.close_all_connections(ConnectionCloseReason::Protocol);
                false
            }
        }
    }

    fn remove_connection(&mut self, conn_id: &LaneId) -> Option<ConnectionSlot> {
        self.remove_connection_with_reason(conn_id, ConnectionCloseReason::Unknown)
    }

    fn remove_connection_with_reason(
        &mut self,
        conn_id: &LaneId,
        reason: ConnectionCloseReason,
    ) -> Option<ConnectionSlot> {
        trace!(%conn_id, "remove_connection called");
        let slot = self.conns.remove(conn_id);
        if let Some(ConnectionSlot::Active(state)) = &slot {
            let _ = state.closed_tx.send(Some(reason));
            if let Some(observer) = &self.observer {
                observer.driver_event(vox_types::DriverEvent::ConnectionClosed {
                    connection_id: *conn_id,
                    reason,
                });
            }
        }
        slot
    }

    // r[impl rpc.observability.session-errors]
    fn close_all_connections(&mut self, reason: ConnectionCloseReason) {
        trace!(role = ?self.role, count = self.conns.len(), "close_all_connections");
        vox_types::dlog!(
            "[session {:?}] close_all_connections: {} slots",
            self.role,
            self.conns.len()
        );
        for (conn_id, slot) in self.conns.iter() {
            if let ConnectionSlot::Active(state) = slot {
                vox_types::dlog!("[session {:?}] closing connection {:?}", self.role, conn_id);
                let _ = state.closed_tx.send(Some(reason));
                if let Some(observer) = &self.observer {
                    observer.driver_event(vox_types::DriverEvent::ConnectionClosed {
                        connection_id: *conn_id,
                        reason,
                    });
                }
            }
        }
        self.conns.clear();
    }
}

/// A one-shot open gate for a locally-opened outbound channel. Created CLOSED when
/// the channel id is allocated during a Call's arg-encode, and opened once that Call
/// has been pushed to the outbound queue. Channel items wait on it, so a `tx.send`
/// the application fires concurrently with the call cannot reach the wire before the
/// Call that declares the channel — the sender upholds the frame-ordering invariant
/// (`r[impl rpc.channel.item]`).
struct ChannelGate {
    opened: std::sync::atomic::AtomicBool,
    notify: tokio::sync::Notify,
}

pub(crate) struct SessionCore {
    inner: std::sync::Mutex<SessionCoreInner>,
    outbound_tx: tokio_mpsc::Sender<OutboundBatch>,
    observer: Option<VoxObserverHandle>,
    /// Open gates for channels the local side opened but whose declaring Call has not
    /// yet been enqueued. Keyed by channel id; entries removed when opened.
    channel_gates: std::sync::Mutex<HashMap<vox_types::ChannelId, Arc<ChannelGate>>>,
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

impl From<vox_types::ChannelWriterSchemaPlan> for PendingSchemaSend {
    fn from(plan: vox_types::ChannelWriterSchemaPlan) -> Self {
        let _ = plan.role;
        Self {
            method_id: plan.method_id,
            direction: plan.direction,
            prepared: plan.prepared,
        }
    }
}

struct OutboundBatch {
    conn_id: LaneId,
    request_id: Option<RequestId>,
    payload_kind: &'static str,
    conn_state: Arc<std::sync::Mutex<SendConnState>>,
    tx: Arc<dyn DynConduitTx>,
    schema_sends: Vec<PendingSchemaSend>,
    payload_send: OutboundSend,
    result_tx: tokio_oneshot::Sender<std::io::Result<()>>,
}

type PreparedOutboundBatch = (
    OutboundBatch,
    tokio_oneshot::Receiver<std::io::Result<()>>,
    Vec<vox_types::ChannelId>,
);

async fn run_outbound_worker(mut rx: tokio_mpsc::Receiver<OutboundBatch>) {
    while let Some(batch) = rx.recv().await {
        trace!(
            conn_id = %batch.conn_id,
            request_id = ?batch.request_id,
            payload_kind = batch.payload_kind,
            schema_count = batch.schema_sends.len(),
            "session outbound worker received batch"
        );
        let mut result = Ok(());
        for schema_send in batch.schema_sends {
            trace!(
                conn_id = %batch.conn_id,
                request_id = ?batch.request_id,
                method_id = ?schema_send.method_id,
                direction = ?schema_send.direction,
                "session outbound worker sending schema batch"
            );
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
                lane_id: batch.conn_id,
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
            trace!(
                conn_id = %batch.conn_id,
                request_id = ?batch.request_id,
                payload_kind = batch.payload_kind,
                ?error,
                "session outbound worker payload send failed"
            );
            result = Err(error);
        }
        trace!(
            conn_id = %batch.conn_id,
            request_id = ?batch.request_id,
            payload_kind = batch.payload_kind,
            ok = result.is_ok(),
            "session outbound worker finished batch"
        );
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
    conns: HashMap<LaneId, Arc<std::sync::Mutex<SendConnState>>>,
}

fn get_or_create_send_conn_state(
    inner: &mut SessionCoreInner,
    conn_id: LaneId,
) -> Arc<std::sync::Mutex<SendConnState>> {
    inner
        .conns
        .entry(conn_id)
        .or_insert_with(|| Arc::new(std::sync::Mutex::new(SendConnState::new())))
        .clone()
}

/// The channel id whose open-gate must be honored before sending `msg`, if `msg` is an
/// outbound channel item or close. Other messages (Calls, credit, reset, non-channel)
/// are never gated.
fn gated_channel_id(msg: &Message<'_>) -> Option<vox_types::ChannelId> {
    match &msg.payload {
        MessagePayload::ChannelMessage(ch) => match &ch.body {
            vox_types::ChannelBody::Item(_) | vox_types::ChannelBody::Close(_) => Some(ch.id),
            _ => None,
        },
        _ => None,
    }
}

impl SessionCore {
    pub(crate) fn outbound_queue_stats(&self) -> (usize, usize) {
        let capacity = self.outbound_tx.max_capacity();
        let available = self.outbound_tx.capacity();
        (capacity.saturating_sub(available), capacity)
    }

    /// Register a CLOSED open-gate for a freshly-allocated outbound channel. Called by
    /// the channel binder at allocation time (during a Call's arg-encode), BEFORE the
    /// Tx sink is bound — so a concurrently-parked `tx.send` that wakes on the bind
    /// finds the gate and waits. Idempotent. (`r[impl rpc.channel.item]`)
    pub(crate) fn register_channel_gate(&self, channel_id: vox_types::ChannelId) {
        self.channel_gates
            .lock()
            .expect("channel gates mutex poisoned")
            .entry(channel_id)
            .or_insert_with(|| {
                Arc::new(ChannelGate {
                    opened: std::sync::atomic::AtomicBool::new(false),
                    notify: tokio::sync::Notify::new(),
                })
            });
    }

    /// Open the gates for `channels` (the channels a just-enqueued Call declared),
    /// releasing any parked channel-item sends so they reach the wire AFTER the Call.
    fn open_channel_gates(&self, channels: &[vox_types::ChannelId]) {
        if channels.is_empty() {
            return;
        }
        let mut gates = self
            .channel_gates
            .lock()
            .expect("channel gates mutex poisoned");
        for id in channels {
            if let Some(gate) = gates.remove(id) {
                gate.opened
                    .store(true, std::sync::atomic::Ordering::Release);
                gate.notify.notify_waiters();
            }
        }
    }

    /// Wait until channel `channel_id`'s declaring Call has been enqueued (its gate is
    /// open), if a gate exists. No gate (or an already-open one) returns immediately —
    /// so a non-channel item, or an item whose Call already went out, never blocks.
    async fn await_channel_gate(&self, channel_id: vox_types::ChannelId) {
        let gate = {
            let gates = self
                .channel_gates
                .lock()
                .expect("channel gates mutex poisoned");
            match gates.get(&channel_id) {
                Some(gate) => Arc::clone(gate),
                None => return,
            }
        };
        loop {
            if gate.opened.load(std::sync::atomic::Ordering::Acquire) {
                return;
            }
            let notified = gate.notify.notified();
            if gate.opened.load(std::sync::atomic::Ordering::Acquire) {
                return;
            }
            notified.await;
        }
    }

    /// Whether channel `channel_id` may send now: true if it has no gate (not a
    /// locally-opened, not-yet-declared channel) or its gate is already open.
    fn channel_gate_open(&self, channel_id: vox_types::ChannelId) -> bool {
        self.channel_gates
            .lock()
            .expect("channel gates mutex poisoned")
            .get(&channel_id)
            .is_none_or(|gate| gate.opened.load(std::sync::atomic::Ordering::Acquire))
    }

    fn prepare_outbound_batch<'a>(
        &self,
        mut msg: Message<'a>,
        binder: Option<&'a dyn vox_types::ChannelBinder>,
        forwarded_schemas: Option<&vox_types::SchemaRecvTracker>,
        channel_method: Option<&'static vox_types::MethodDescriptor>,
        extra_schema_sends: Vec<PendingSchemaSend>,
    ) -> Result<PreparedOutboundBatch, ()> {
        let conn_id = msg.lane_id;
        let (request_id, payload_kind) = match &msg.payload {
            MessagePayload::RequestMessage(req) => {
                let kind = match &req.body {
                    RequestBody::Call(_) => "request.call",
                    RequestBody::Response(_) => "request.response",
                    RequestBody::Cancel(_) => "request.cancel",
                };
                (Some(req.id), kind)
            }
            MessagePayload::SchemaMessage(_) => (None, "schema"),
            MessagePayload::ChannelMessage(_) => (None, "channel"),
            MessagePayload::LaneOpen(_) => (None, "connection.open"),
            MessagePayload::LaneAccept(_) => (None, "connection.accept"),
            MessagePayload::LaneReject(_) => (None, "connection.reject"),
            MessagePayload::LaneClose(_) => (None, "connection.close"),
            MessagePayload::ProtocolError(_) => (None, "protocol.error"),
            MessagePayload::Ping(_) => (None, "ping"),
            MessagePayload::Pong(_) => (None, "pong"),
        };
        trace!(
            conn_id = %conn_id,
            ?request_id,
            payload_kind,
            "session preparing outbound message"
        );
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
                    let mut schema_sends = extra_schema_sends;
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
                (tx, conn_state, extra_schema_sends)
            }
        };
        trace!(
            conn_id = %conn_id,
            ?request_id,
            payload_kind,
            schema_count = schema_sends.len(),
            "session preparing outbound payload"
        );

        // Out-of-band channel allocation. If this Call's args carry `Tx`/`Rx`
        // handles, pre-encode the args now under a channel collector (with the
        // binder installed) so the allocated `ChannelId`s ride in `call.channels`
        // and each handle goes on the wire as a small index. The args then travel
        // as already-encoded bytes — the schema for them was already attached
        // above from the `Value` shape. r[impl rpc.request] r[impl rpc.channel.allocation]
        // Channels this Call opens, whose send-gates must be opened once the Call has
        // been enqueued (so a concurrent `tx.send` cannot beat the Call to the wire).
        let mut gated_channels: Vec<vox_types::ChannelId> = Vec::new();
        let channel_storage: Option<Vec<u8>> = if let MessagePayload::RequestMessage(req) =
            &msg.payload
            && let RequestBody::Call(call) = &req.body
            && let vox_types::Payload::Value { ptr, shape, .. } = &call.args
            && vox_types::shape_contains_channel(shape)
        {
            let (ptr, shape) = (*ptr, *shape);
            // The binder registers a CLOSED gate per channel it allocates here (before
            // binding the Tx sink), so an app `tx.send` that wakes mid-encode parks.
            let encode_args = || match binder {
                Some(b) => {
                    vox_types::with_channel_binder(b, || vox_phon::to_vec_for_shape(ptr, shape))
                }
                None => vox_phon::to_vec_for_shape(ptr, shape),
            };
            let (encoded, channels) = match channel_method {
                Some(method) => vox_types::collect_channels_for_method(method, encode_args),
                None => vox_types::collect_channels(encode_args),
            };
            gated_channels = channels.clone();
            let encoded = match encoded {
                Ok(encoded) => encoded,
                Err(_) => {
                    // Encode failed after gates were registered: release them so any
                    // parked `tx.send` unblocks (and then fails on the dead call/conn)
                    // rather than hanging forever.
                    self.open_channel_gates(&gated_channels);
                    return Err(());
                }
            };
            if let MessagePayload::RequestMessage(req) = &mut msg.payload
                && let RequestBody::Call(call) = &mut req.body
            {
                call.channels = channels;
            }
            Some(encoded)
        } else {
            None
        };

        let prepared = if let Some(bytes) = &channel_storage {
            // Narrow `msg`'s lifetime to the pre-encoded `bytes` (covariance) and
            // swap the args to the encoded payload, then encode the envelope. The
            // `bytes` outlive `prepare_msg`, which consumes the message synchronously.
            let msg = swap_call_args_to_bytes(msg, bytes);
            tx.clone().prepare_msg(msg, binder)
        } else {
            tx.clone().prepare_msg(msg, binder)
        };
        let payload_send = match prepared {
            Ok(send) => send,
            Err(_) => {
                self.open_channel_gates(&gated_channels);
                return Err(());
            }
        };
        trace!(
            conn_id = %conn_id,
            ?request_id,
            payload_kind,
            "session prepared outbound payload"
        );

        let (result_tx, result_rx) = tokio_oneshot::channel();
        Ok((
            OutboundBatch {
                conn_id,
                request_id,
                payload_kind,
                conn_state,
                tx,
                schema_sends,
                payload_send,
                result_tx,
            },
            result_rx,
            gated_channels,
        ))
    }

    // r[impl schema.principles.sender-driven]
    pub(crate) async fn send<'a>(
        &self,
        msg: Message<'a>,
        binder: Option<&'a dyn vox_types::ChannelBinder>,
        forwarded_schemas: Option<&vox_types::SchemaRecvTracker>,
    ) -> Result<(), ()> {
        self.send_with_options(msg, binder, forwarded_schemas, None, Vec::new())
            .await
    }

    async fn send_with_options<'a>(
        &self,
        msg: Message<'a>,
        binder: Option<&'a dyn vox_types::ChannelBinder>,
        forwarded_schemas: Option<&vox_types::SchemaRecvTracker>,
        channel_method: Option<&'static vox_types::MethodDescriptor>,
        extra_schema_sends: Vec<PendingSchemaSend>,
    ) -> Result<(), ()> {
        let connection_id = msg.lane_id;
        // r[impl rpc.channel.item] Hold an outbound channel item/close until the Call
        // that opened its channel has been enqueued — the sender upholds frame order.
        if let Some(channel_id) = gated_channel_id(&msg) {
            self.await_channel_gate(channel_id).await;
        }
        let (batch, result_rx, gated_channels) = self.prepare_outbound_batch(
            msg,
            binder,
            forwarded_schemas,
            channel_method,
            extra_schema_sends,
        )?;
        let queued = self.outbound_tx.send(batch).await;
        // This Call is now on the queue (or the queue is gone): release its channels'
        // gates so parked items follow it — unconditionally, so a failed enqueue never
        // strands a parked `tx.send`.
        self.open_channel_gates(&gated_channels);
        if queued.is_err() {
            if let Some(observer) = &self.observer {
                observer
                    .driver_event(vox_types::DriverEvent::OutboundQueueClosed { connection_id });
            }
            return Err(());
        }
        trace!(conn_id = %connection_id, "session queued outbound batch");
        let result = result_rx.await.map_err(|_| ());
        trace!(
            conn_id = %connection_id,
            ok = result.as_ref().map(|inner| inner.is_ok()).unwrap_or(false),
            "session outbound batch completed"
        );
        match result? {
            Ok(()) => Ok(()),
            Err(_) => {
                if let Some(observer) = &self.observer {
                    observer.driver_event(vox_types::DriverEvent::EncodeError {
                        connection_id,
                        kind: vox_types::EncodeErrorKind::Transport,
                    });
                }
                Err(())
            }
        }
    }

    // r[impl rpc.flow-control.credit.try-send]
    fn try_send_with_options<'a>(
        &self,
        msg: Message<'a>,
        binder: Option<&'a dyn vox_types::ChannelBinder>,
        forwarded_schemas: Option<&vox_types::SchemaRecvTracker>,
        channel_method: Option<&'static vox_types::MethodDescriptor>,
        extra_schema_sends: Vec<PendingSchemaSend>,
    ) -> Result<(), TrySendError<()>> {
        let connection_id = msg.lane_id;
        // r[impl rpc.channel.item] A channel item whose declaring Call hasn't been
        // enqueued yet can't go out (frame order); signal backpressure so the caller
        // retries — the async `send` path parks instead.
        if let Some(channel_id) = gated_channel_id(&msg)
            && !self.channel_gate_open(channel_id)
        {
            return Err(TrySendError::Full(()));
        }
        let (batch, _result_rx, gated_channels) = self
            .prepare_outbound_batch(
                msg,
                binder,
                forwarded_schemas,
                channel_method,
                extra_schema_sends,
            )
            .map_err(|_| TrySendError::Closed(()))?;
        let result = self.outbound_tx.try_send(batch).map_err(|err| match err {
            tokio_mpsc::error::TrySendError::Full(_) => {
                if let Some(observer) = &self.observer {
                    observer
                        .driver_event(vox_types::DriverEvent::OutboundQueueFull { connection_id });
                }
                TrySendError::Full(())
            }
            tokio_mpsc::error::TrySendError::Closed(_) => {
                if let Some(observer) = &self.observer {
                    observer.driver_event(vox_types::DriverEvent::OutboundQueueClosed {
                        connection_id,
                    });
                }
                TrySendError::Closed(())
            }
        });
        // Release this Call's channel gates now that it's been handed to the queue.
        self.open_channel_gates(&gated_channels);
        result
    }

    /// Record that an incoming call was received, so we can look up the
    /// method_id when sending the response.
    pub(crate) fn record_incoming_call(
        &self,
        conn_id: LaneId,
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
        conn_id: LaneId,
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
        conn_id: LaneId,
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
            vox_types::Payload::Encoded(_) => {
                tracing::error!(
                    "schema attachment failed: missing forwarded response schemas for method {:?}",
                    method_id
                );
                return;
            }
        };
        response.schemas = prepared.to_payload();
    }

    /// Attach the method's response schema for an explicit wire `shape` (the
    /// erased-error-response path). Commits directly (best-effort dedup,
    /// `r[schema.exchange]`).
    pub(crate) fn prepare_response_for_shape(
        &self,
        conn_id: LaneId,
        _request_id: RequestId,
        method_id: vox_types::MethodId,
        shape: &'static Shape,
        response: &mut RequestResponse<'_>,
    ) {
        let mut inner = self.inner.lock().expect("session core mutex poisoned");
        let conn_state = get_or_create_send_conn_state(&mut inner, conn_id);
        let mut conn_state = conn_state.lock().expect("send conn state mutex poisoned");
        if conn_state
            .send_tracker
            .has_sent_binding(method_id, vox_types::BindingDirection::Response)
        {
            response.schemas = Default::default();
            return;
        }
        match vox_types::SchemaSendTracker::plan_for_shape(shape) {
            Ok(prepared) => {
                response.schemas = conn_state.send_tracker.commit_prepared_plan(
                    method_id,
                    vox_types::BindingDirection::Response,
                    prepared,
                );
            }
            Err(e) => tracing::error!("error-response schema extraction failed: {e}"),
        }
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
                    prepared.bytes.len(),
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

    /// Forward a binding's schema for the proxy/relay path: source the peer's phon
    /// schema-closure bytes from the receive tracker (where they were stored when the
    /// upstream sent them) and re-send them verbatim. `None` if not received yet.
    fn get_or_plan_binding_from_tracker(
        conn_state: &mut SendConnState,
        key: (vox_types::BindingDirection, vox_types::MethodId),
        tracker: &vox_types::SchemaRecvTracker,
    ) -> Option<vox_types::PreparedSchemaPlan> {
        if let Some(prepared) = conn_state.planned_bindings.get(&key) {
            return Some(prepared.clone());
        }
        let bytes = tracker.writer_schema_bytes(key.1, key.0)?;
        let prepared = vox_types::PreparedSchemaPlan { bytes };
        conn_state.planned_bindings.insert(key, prepared.clone());
        Some(prepared)
    }

    fn plan_response_schema_send(
        conn_state: &mut SendConnState,
        request_id: RequestId,
        method_id: vox_types::MethodId,
        response: &mut RequestResponse<'_>,
        forwarded_schemas: Option<&vox_types::SchemaRecvTracker>,
    ) -> Option<PendingSchemaSend> {
        // r[impl schema.exchange.callee]
        if conn_state
            .send_tracker
            .has_sent_binding(method_id, vox_types::BindingDirection::Response)
        {
            response.schemas = Default::default();
            return None;
        }

        let key = (vox_types::BindingDirection::Response, method_id);
        let prepared = if !response.schemas.is_empty() {
            // The response already carries its phon schema closure (forwarded from
            // upstream, or set by an earlier stage) — re-send it verbatim.
            conn_state
                .planned_bindings
                .get(&key)
                .cloned()
                .unwrap_or_else(|| vox_types::PreparedSchemaPlan {
                    bytes: response.schemas.0.clone(),
                })
        } else {
            match &response.ret {
                vox_types::Payload::Value { shape, .. } => Self::get_or_plan_binding_for_shape(
                    conn_state, key, request_id, "response", shape,
                )?,
                vox_types::Payload::Encoded(_) => {
                    let Some(source) = forwarded_schemas else {
                        tracing::error!(
                            "schema attachment failed: missing forwarded response schemas for method {:?}",
                            method_id
                        );
                        return None;
                    };
                    Self::get_or_plan_binding_from_tracker(conn_state, key, source)?
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
        // r[impl schema.exchange.caller]
        if conn_state
            .send_tracker
            .has_sent_binding(method_id, vox_types::BindingDirection::Args)
        {
            call.schemas = Default::default();
            return None;
        }

        let key = (vox_types::BindingDirection::Args, method_id);
        let prepared = if !call.schemas.is_empty() {
            conn_state
                .planned_bindings
                .get(&key)
                .cloned()
                .unwrap_or_else(|| vox_types::PreparedSchemaPlan {
                    bytes: call.schemas.0.clone(),
                })
        } else {
            match &call.args {
                vox_types::Payload::Value { shape, .. } => {
                    Self::get_or_plan_binding_for_shape(conn_state, key, request_id, "args", shape)?
                }
                vox_types::Payload::Encoded(_) => {
                    let Some(source) = forwarded_schemas else {
                        tracing::error!(
                            "schema attachment failed: missing forwarded args schemas for method {:?}",
                            method_id
                        );
                        return None;
                    };
                    Self::get_or_plan_binding_from_tracker(conn_state, key, source)?
                }
            }
        };

        Some(PendingSchemaSend {
            method_id,
            direction: vox_types::BindingDirection::Args,
            prepared,
        })
    }
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

    /// Descriptors that arrived with the frame from the most recent
    /// `recv_msg`. Threaded alongside the message to the typed-decode site.
    fn take_frame_fds(&mut self) -> vox_types::FrameFds;
}

// r[impl session.message]
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

    fn take_frame_fds(&mut self) -> vox_types::FrameFds {
        ConduitRx::take_frame_fds(self)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use vox_rt::sync::mpsc;
    use vox_types::{
        Backing, BindingDirection, Conduit, DriverEvent, HandshakeResult, LaneAccept, LaneReject,
        Payload, RequestCall, SelfRef, TransportEvent, VoxObserverHandle,
    };

    use super::*;

    #[derive(Clone)]
    struct CapturingTx {
        sent: Arc<Mutex<Vec<CapturedMessage>>>,
    }

    #[derive(Debug)]
    struct CapturedMessage {
        lane_id: LaneId,
        payload: CapturedPayload,
    }

    #[derive(Debug)]
    enum CapturedPayload {
        Schema {
            method_id: vox_types::MethodId,
            direction: BindingDirection,
            schemas: Vec<u8>,
        },
        Call {
            request_id: RequestId,
            method_id: vox_types::MethodId,
            schemas_len: usize,
        },
        Response {
            request_id: RequestId,
            schemas_len: usize,
        },
        Other,
    }

    impl ConduitTx for CapturingTx {
        type Error = std::io::Error;
        type Msg = MessageFamily;
        type Prepared = CapturedMessage;

        fn prepare_send(&self, item: Message<'_>) -> Result<Self::Prepared, Self::Error> {
            let payload = match &item.payload {
                MessagePayload::SchemaMessage(schema) => CapturedPayload::Schema {
                    method_id: schema.method_id,
                    direction: schema.direction,
                    schemas: schema.schemas.0.clone(),
                },
                MessagePayload::RequestMessage(request) => match &request.body {
                    RequestBody::Call(call) => CapturedPayload::Call {
                        request_id: request.id,
                        method_id: call.method_id,
                        schemas_len: call.schemas.0.len(),
                    },
                    RequestBody::Response(response) => CapturedPayload::Response {
                        request_id: request.id,
                        schemas_len: response.schemas.0.len(),
                    },
                    _ => CapturedPayload::Other,
                },
                _ => CapturedPayload::Other,
            };
            Ok(CapturedMessage {
                lane_id: item.lane_id,
                payload,
            })
        }

        async fn send_prepared(&self, prepared: Self::Prepared) -> Result<(), Self::Error> {
            self.sent
                .lock()
                .expect("captured message mutex poisoned")
                .push(prepared);
            Ok(())
        }

        async fn close(self) -> std::io::Result<()> {
            Ok(())
        }
    }

    struct PendingRx;

    impl ConduitRx for PendingRx {
        type Error = std::io::Error;
        type Msg = MessageFamily;

        async fn recv(&mut self) -> Result<Option<SelfRef<Message<'static>>>, Self::Error> {
            std::future::pending().await
        }
    }

    struct RecordingObserver {
        driver_events: Arc<Mutex<Vec<DriverEvent>>>,
        transport_events: Arc<Mutex<Vec<TransportEvent>>>,
    }

    impl vox_types::VoxObserver for RecordingObserver {
        fn driver_event(&self, event: DriverEvent) {
            self.driver_events
                .lock()
                .expect("driver events mutex poisoned")
                .push(event);
        }

        fn transport_event(&self, event: TransportEvent) {
            self.transport_events
                .lock()
                .expect("transport events mutex poisoned")
                .push(event);
        }
    }

    fn make_session() -> Connection {
        let (a, b) = crate::memory_link_pair(32);
        // Keep the peer link alive so sess_core sends don't fail with broken pipe.
        std::mem::forget(b);
        let conduit = crate::BareConduit::new(a);
        let (tx, rx) = conduit.split();
        let (_open_tx, open_rx) = mpsc::channel::<OpenRequest>("session.open.test", 4);
        let (_close_tx, close_rx) = mpsc::channel::<CloseRequest>("session.close.test", 4);
        let (control_tx, control_rx) = mpsc::unbounded_channel("session.control.test");
        Connection::pre_handshake(
            tx, rx, None, open_rx, close_rx, control_tx, control_rx, None, None,
        )
    }

    fn make_session_with_observer(observer: VoxObserverHandle) -> Connection {
        let (a, b) = crate::memory_link_pair(32);
        std::mem::forget(b);
        let conduit = crate::BareConduit::new(a);
        let (tx, rx) = conduit.split();
        let (_open_tx, open_rx) = mpsc::channel::<OpenRequest>("session.open.observed.test", 4);
        let (_close_tx, close_rx) = mpsc::channel::<CloseRequest>("session.close.observed.test", 4);
        let (control_tx, control_rx) = mpsc::unbounded_channel("session.control.observed.test");
        Connection::pre_handshake(
            tx,
            rx,
            None,
            open_rx,
            close_rx,
            control_tx,
            control_rx,
            None,
            Some(observer),
        )
    }

    fn make_capturing_session(sent: Arc<Mutex<Vec<CapturedMessage>>>) -> (Connection, LaneHandle) {
        let (_open_tx, open_rx) = mpsc::channel::<OpenRequest>("session.open.capture.test", 4);
        let (_close_tx, close_rx) = mpsc::channel::<CloseRequest>("session.close.capture.test", 4);
        let (control_tx, control_rx) = mpsc::unbounded_channel("session.control.capture.test");
        let mut session = Connection::pre_handshake(
            CapturingTx { sent },
            PendingRx,
            None,
            open_rx,
            close_rx,
            control_tx,
            control_rx,
            None,
            None,
        );
        let handle = session
            .establish_from_handshake(test_handshake(
                ConnectionSettings {
                    parity: Parity::Odd,
                    max_concurrent_requests: 64,
                    initial_channel_credit: 16,
                },
                ConnectionSettings {
                    parity: Parity::Even,
                    max_concurrent_requests: 64,
                    initial_channel_credit: 16,
                },
            ))
            .expect("establish captured session");
        (session, handle)
    }

    fn test_handshake(
        our_settings: ConnectionSettings,
        peer_settings: ConnectionSettings,
    ) -> HandshakeResult {
        HandshakeResult {
            role: ConnectionRole::Initiator,
            our_settings,
            peer_settings,
            our_schema: vec![],
            peer_schema: vec![],
            peer_metadata: vox_types::Metadata::default(),
        }
    }

    fn accept_ref() -> SelfRef<LaneAccept> {
        SelfRef::owning(
            Backing::Boxed(Box::<[u8]>::default()),
            LaneAccept {
                connection_settings: ConnectionSettings {
                    parity: Parity::Even,
                    max_concurrent_requests: 64,
                    initial_channel_credit: 16,
                },
                metadata: vox_types::Metadata::default(),
            },
        )
    }

    fn zero_credit_accept_ref() -> SelfRef<LaneAccept> {
        SelfRef::owning(
            Backing::Boxed(Box::<[u8]>::default()),
            LaneAccept {
                connection_settings: ConnectionSettings {
                    parity: Parity::Even,
                    max_concurrent_requests: 64,
                    initial_channel_credit: 0,
                },
                metadata: vox_types::Metadata::default(),
            },
        )
    }

    fn reject_ref() -> SelfRef<LaneReject> {
        SelfRef::owning(
            Backing::Boxed(Box::<[u8]>::default()),
            LaneReject {
                metadata: vox_types::Metadata::default(),
            },
        )
    }

    // r[verify rpc.observability.session-errors]
    #[test]
    fn session_receive_errors_emit_diagnostics_and_non_graceful_close_reasons() {
        fn run_case(
            error: std::io::Error,
            expected_reason: ConnectionCloseReason,
            expect_decode_error: bool,
        ) {
            let driver_events = Arc::new(Mutex::new(Vec::new()));
            let transport_events = Arc::new(Mutex::new(Vec::new()));
            let observer: VoxObserverHandle = Arc::new(RecordingObserver {
                driver_events: driver_events.clone(),
                transport_events: transport_events.clone(),
            });

            let mut session = make_session_with_observer(observer);
            let handle = session
                .establish_from_handshake(test_handshake(
                    ConnectionSettings {
                        parity: Parity::Odd,
                        max_concurrent_requests: 64,
                        initial_channel_credit: 16,
                    },
                    ConnectionSettings {
                        parity: Parity::Even,
                        max_concurrent_requests: 64,
                        initial_channel_credit: 16,
                    },
                ))
                .expect("establish observed session");

            driver_events
                .lock()
                .expect("driver events mutex poisoned")
                .clear();
            transport_events
                .lock()
                .expect("transport events mutex poisoned")
                .clear();

            session.observe_session_recv_error(&error);
            session.close_all_connections(classify_session_recv_error(&error));

            assert_eq!(handle.close_reason(), Some(expected_reason));

            let driver_events = driver_events.lock().expect("driver events mutex poisoned");
            assert!(driver_events.iter().any(|event| matches!(
                event,
                DriverEvent::ConnectionClosed {
                    connection_id: LaneId::ROOT,
                    reason
                } if *reason == expected_reason
            )));
            assert_eq!(
                driver_events.iter().any(|event| matches!(
                    event,
                    DriverEvent::DecodeError {
                        connection_id: LaneId::ROOT,
                        kind: DecodeErrorKind::Payload,
                    }
                )),
                expect_decode_error
            );

            let transport_events = transport_events
                .lock()
                .expect("transport events mutex poisoned");
            assert_eq!(
                transport_events.iter().any(|event| matches!(
                    event,
                    TransportEvent::Closed {
                        connection_id: None,
                        reason
                    } if *reason == expected_reason
                )),
                !expect_decode_error
            );
        }

        run_case(
            std::io::Error::other("decode error: invalid Message payload"),
            ConnectionCloseReason::Protocol,
            true,
        );
        run_case(
            std::io::Error::other("connection reset by peer"),
            ConnectionCloseReason::Transport,
            false,
        );
    }

    // r[verify schema.exchange.caller]
    #[tokio::test]
    async fn caller_schema_exchange_sends_binding_once_before_request() {
        use facet::Facet;

        let sent = Arc::new(Mutex::new(Vec::new()));
        let (_session, handle) = make_capturing_session(Arc::clone(&sent));
        let method_id = vox_types::MethodId(700);

        let first_arg = 42_u32;
        handle
            .sender
            .send(ConnectionMessage::Request(RequestMessage {
                id: RequestId(1),
                body: RequestBody::Call(RequestCall {
                    method_id,
                    channels: Vec::new(),
                    metadata: Metadata::default(),
                    args: Payload::outgoing(&first_arg),
                    schemas: Default::default(),
                }),
            }))
            .await
            .expect("first call send");

        {
            let captured = sent.lock().expect("captured message mutex poisoned");
            assert_eq!(captured.len(), 2);
            assert_eq!(captured[0].lane_id, LaneId::ROOT);
            match &captured[0].payload {
                CapturedPayload::Schema {
                    method_id: actual_method_id,
                    direction,
                    schemas,
                } => {
                    assert_eq!(*actual_method_id, method_id);
                    assert_eq!(*direction, BindingDirection::Args);
                    let parsed =
                        vox_phon::parse_schema_bytes(schemas).expect("parse args schema binding");
                    let expected_root =
                        vox_phon::schema_id_for_shape(<u32 as Facet>::SHAPE).expect("u32 root");
                    assert_eq!(parsed.root, expected_root);
                }
                other => panic!("expected schema message before request, got {other:?}"),
            }
            assert_eq!(captured[1].lane_id, LaneId::ROOT);
            match &captured[1].payload {
                CapturedPayload::Call {
                    request_id,
                    method_id: actual_method_id,
                    schemas_len,
                } => {
                    assert_eq!(*request_id, RequestId(1));
                    assert_eq!(*actual_method_id, method_id);
                    assert_eq!(*schemas_len, 0);
                }
                other => panic!("expected request after schema message, got {other:?}"),
            }
        }

        let second_arg = 43_u32;
        handle
            .sender
            .send(ConnectionMessage::Request(RequestMessage {
                id: RequestId(3),
                body: RequestBody::Call(RequestCall {
                    method_id,
                    channels: Vec::new(),
                    metadata: Metadata::default(),
                    args: Payload::outgoing(&second_arg),
                    schemas: Default::default(),
                }),
            }))
            .await
            .expect("second call send");

        let captured = sent.lock().expect("captured message mutex poisoned");
        assert_eq!(captured.len(), 3);
        match &captured[2].payload {
            CapturedPayload::Call {
                request_id,
                method_id: actual_method_id,
                schemas_len,
            } => {
                assert_eq!(*request_id, RequestId(3));
                assert_eq!(*actual_method_id, method_id);
                assert_eq!(*schemas_len, 0);
            }
            other => panic!("expected second request without schema resend, got {other:?}"),
        }
    }

    // r[verify schema.exchange.callee]
    #[tokio::test]
    async fn callee_schema_exchange_sends_binding_once_before_response() {
        use facet::Facet;

        let sent = Arc::new(Mutex::new(Vec::new()));
        let (_session, handle) = make_capturing_session(Arc::clone(&sent));
        let method_id = vox_types::MethodId(701);
        let request_id = RequestId(11);
        handle
            .sender
            .sess_core
            .record_incoming_call(LaneId::ROOT, request_id, method_id);

        let first_response: Result<u32, vox_types::VoxError<core::convert::Infallible>> = Ok(99);
        handle
            .sender
            .send_response(
                request_id,
                RequestResponse {
                    metadata: Metadata::default(),
                    ret: Payload::outgoing(&first_response),
                    schemas: Default::default(),
                },
            )
            .await
            .expect("first response send");

        {
            let captured = sent.lock().expect("captured message mutex poisoned");
            assert_eq!(captured.len(), 2);
            assert_eq!(captured[0].lane_id, LaneId::ROOT);
            match &captured[0].payload {
                CapturedPayload::Schema {
                    method_id: actual_method_id,
                    direction,
                    schemas,
                } => {
                    assert_eq!(*actual_method_id, method_id);
                    assert_eq!(*direction, BindingDirection::Response);
                    let parsed = vox_phon::parse_schema_bytes(schemas)
                        .expect("parse response schema binding");
                    let expected_root = vox_phon::schema_id_for_shape(
                        <Result<
                            u32,
                            vox_types::VoxError<core::convert::Infallible>,
                        > as Facet>::SHAPE,
                    )
                    .expect("response root");
                    assert_eq!(parsed.root, expected_root);
                }
                other => panic!("expected schema message before response, got {other:?}"),
            }
            assert_eq!(captured[1].lane_id, LaneId::ROOT);
            match &captured[1].payload {
                CapturedPayload::Response {
                    request_id: actual_request_id,
                    schemas_len,
                } => {
                    assert_eq!(*actual_request_id, request_id);
                    assert_eq!(*schemas_len, 0);
                }
                other => panic!("expected response after schema message, got {other:?}"),
            }
        }

        let second_request_id = RequestId(13);
        handle
            .sender
            .sess_core
            .record_incoming_call(LaneId::ROOT, second_request_id, method_id);
        let second_response: Result<u32, vox_types::VoxError<core::convert::Infallible>> = Ok(100);
        handle
            .sender
            .send_response(
                second_request_id,
                RequestResponse {
                    metadata: Metadata::default(),
                    ret: Payload::outgoing(&second_response),
                    schemas: Default::default(),
                },
            )
            .await
            .expect("second response send");

        let captured = sent.lock().expect("captured message mutex poisoned");
        assert_eq!(captured.len(), 3);
        match &captured[2].payload {
            CapturedPayload::Response {
                request_id: actual_request_id,
                schemas_len,
            } => {
                assert_eq!(*actual_request_id, second_request_id);
                assert_eq!(*schemas_len, 0);
            }
            other => panic!("expected second response without schema resend, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn duplicate_connection_accept_is_ignored_after_first() {
        let mut session = make_session();
        let conn_id = LaneId(1);
        let (result_tx, result_rx) = vox_rt::sync::oneshot::channel("session.test.open_result");

        session.conns.insert(
            conn_id,
            ConnectionSlot::PendingOutbound(PendingOutboundData {
                local_settings: ConnectionSettings {
                    parity: Parity::Odd,
                    max_concurrent_requests: 64,
                    initial_channel_credit: 16,
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
        let conn_id = LaneId(1);
        let (result_tx, result_rx) = vox_rt::sync::oneshot::channel("session.test.open_result");

        session.conns.insert(
            conn_id,
            ConnectionSlot::PendingOutbound(PendingOutboundData {
                local_settings: ConnectionSettings {
                    parity: Parity::Odd,
                    max_concurrent_requests: 64,
                    initial_channel_credit: 16,
                },
                result_tx: Some(result_tx),
            }),
        );

        session.handle_inbound_reject(conn_id, reject_ref());
        let result = result_rx
            .await
            .expect("pending outbound result should resolve");
        assert!(
            matches!(result, Err(ConnectionError::Rejected(_))),
            "expected rejection, got: {result:?}"
        );

        session.handle_inbound_reject(conn_id, reject_ref());
        assert!(
            !session.conns.contains_key(&conn_id),
            "duplicate reject should not recreate connection state"
        );
    }

    // r[verify rpc.flow-control.credit.initial.zero]
    #[tokio::test]
    async fn inbound_accept_with_zero_initial_credit_rejects_pending_open() {
        let mut session = make_session();
        let conn_id = LaneId(1);
        let (result_tx, result_rx) = vox_rt::sync::oneshot::channel("session.test.open_result");

        session.conns.insert(
            conn_id,
            ConnectionSlot::PendingOutbound(PendingOutboundData {
                local_settings: ConnectionSettings {
                    parity: Parity::Odd,
                    max_concurrent_requests: 64,
                    initial_channel_credit: 16,
                },
                result_tx: Some(result_tx),
            }),
        );

        session.handle_inbound_accept(conn_id, zero_credit_accept_ref());
        let result = result_rx
            .await
            .expect("pending outbound result should resolve");
        assert!(
            matches!(
                result,
                Err(ConnectionError::Protocol(ref message))
                    if message == "initial_channel_credit must be greater than zero"
            ),
            "expected zero-credit protocol error, got: {result:?}"
        );
        assert!(
            !session.conns.contains_key(&conn_id),
            "zero-credit accept should not create an active connection"
        );
    }

    #[test]
    fn out_of_order_accept_or_reject_without_pending_is_ignored() {
        let mut session = make_session();
        let conn_id = LaneId(99);

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
        let (open_result_tx, open_result_rx) =
            vox_rt::sync::oneshot::channel("session.open.result");
        let (close_result_tx, close_result_rx) =
            vox_rt::sync::oneshot::channel("session.close.result");

        session.conns.insert(
            LaneId(1),
            ConnectionSlot::PendingOutbound(PendingOutboundData {
                local_settings: ConnectionSettings {
                    parity: Parity::Odd,
                    max_concurrent_requests: 64,
                    initial_channel_credit: 16,
                },
                result_tx: Some(open_result_tx),
            }),
        );

        session
            .handle_close_request(CloseRequest {
                conn_id: LaneId(1),
                metadata: vox_types::Metadata::default(),
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
