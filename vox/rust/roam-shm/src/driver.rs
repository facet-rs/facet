//! Bidirectional connection driver for SHM transport.
//!
//! This module provides the equivalent of `roam_stream::Driver` for SHM:
//! - Dispatches incoming requests to a service
//! - Routes incoming responses to waiting callers
//! - Sends outgoing requests from ConnectionHandle
//! - Handles stream data (Data/Close/Reset)
//! - Supports virtual connections (Connect/Accept/Reject)
//!
//! Key differences from stream transport:
//! - No Hello exchange (config read from segment header)
//! - No Credit messages (flow control via channel table atomics)
//!
//! shm[impl shm.handshake]
//! shm[impl shm.flow.no-credit-message]

use std::collections::HashMap;
use std::sync::Arc;

use roam_session::diagnostic::DiagnosticState;
use roam_session::{
    ChannelError, ChannelRegistry, ConnectError, ConnectionHandle, Context, DriverMessage,
    ResponseData, Role, ServiceDispatcher, TransportError,
};
use roam_stream::MessageTransport;
use roam_wire::{ConnectionId, Message};

use crate::auditable::{self, AuditableDequeMap, AuditableReceiver, AuditableSender};
use crate::host::ShmHost;
use crate::peer::PeerId;
use crate::transport::{ShmGuestTransport, message_to_shm_msg, shm_msg_to_message};

fn task_context_from_metadata(metadata: &roam_wire::Metadata) -> (Option<u64>, Option<String>) {
    let mut task_id = None;
    let mut task_name = None;
    for (key, value, _flags) in metadata {
        if key == "peeps.task_id" {
            task_id = match value {
                roam_wire::MetadataValue::U64(id) => Some(*id),
                roam_wire::MetadataValue::String(s) => s.parse::<u64>().ok(),
                roam_wire::MetadataValue::Bytes(_) => None,
            };
        } else if key == "peeps.task_name" {
            task_name = match value {
                roam_wire::MetadataValue::String(name) => Some(name.clone()),
                roam_wire::MetadataValue::U64(id) => Some(id.to_string()),
                roam_wire::MetadataValue::Bytes(_) => None,
            };
        }
    }
    (task_id, task_name)
}

/// Get a human-readable name for a message type.
fn msg_type_name(msg: &Message) -> &'static str {
    match msg {
        Message::Hello(_) => "Hello",
        Message::Goodbye { .. } => "Goodbye",
        Message::Request { .. } => "Request",
        Message::Response { .. } => "Response",
        Message::Cancel { .. } => "Cancel",
        Message::Data { .. } => "Data",
        Message::Close { .. } => "Close",
        Message::Reset { .. } => "Reset",
        Message::Credit { .. } => "Credit",
        Message::Connect { .. } => "Connect",
        Message::Accept { .. } => "Accept",
        Message::Reject { .. } => "Reject",
    }
}

/// Negotiated connection parameters from SHM segment header.
///
/// shm[impl shm.handshake.no-negotiation]
///
/// Unlike stream transport, SHM parameters are set unilaterally by the host.
/// Guests accept these by attaching to the segment.
#[derive(Debug, Clone)]
pub struct ShmNegotiated {
    /// Maximum payload size per message.
    pub max_payload_size: u32,
    /// Initial stream credit.
    pub initial_credit: u32,
}

/// Error during SHM connection handling.
#[derive(Debug)]
pub enum ShmConnectionError {
    /// IO error.
    Io(std::io::Error),
    /// Protocol violation.
    ProtocolViolation {
        /// Rule ID that was violated.
        rule_id: &'static str,
        /// Human-readable context.
        context: String,
    },
    /// Connection closed cleanly.
    Closed,
}

impl std::fmt::Display for ShmConnectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShmConnectionError::Io(e) => write!(f, "IO error: {}", e),
            ShmConnectionError::ProtocolViolation { rule_id, context } => {
                write!(f, "protocol violation [{}]: {}", rule_id, context)
            }
            ShmConnectionError::Closed => write!(f, "connection closed"),
        }
    }
}

impl std::error::Error for ShmConnectionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ShmConnectionError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for ShmConnectionError {
    fn from(e: std::io::Error) -> Self {
        ShmConnectionError::Io(e)
    }
}

// ============================================================================
// Virtual Connection Types
// ============================================================================

/// State for a single virtual connection.
///
/// Each virtual connection has its own request ID space, channel registry,
/// and pending responses. Connection 0 (ROOT) is created implicitly.
/// Additional connections are opened via Connect/Accept.
struct VirtualConnectionState {
    /// The connection ID (for debugging/logging).
    #[allow(dead_code)]
    conn_id: ConnectionId,
    /// Client-side handle for making calls on this connection.
    handle: ConnectionHandle,
    /// Server-side channel registry for incoming Rx/Tx streams.
    server_channel_registry: ChannelRegistry,
    /// Dispatcher for handling incoming requests on this connection.
    /// If None, inherits from the parent link's dispatcher.
    dispatcher: Option<Box<dyn ServiceDispatcher>>,
    /// Pending responses (request_id -> response sender).
    pending_responses:
        HashMap<u64, peeps_sync::OneshotSender<Result<ResponseData, TransportError>>>,
    /// In-flight server requests with their abort handles.
    in_flight_server_requests: HashMap<u64, tokio::task::AbortHandle>,
}

impl VirtualConnectionState {
    /// Create a new virtual connection state.
    fn new(
        conn_id: ConnectionId,
        driver_tx: peeps_sync::Sender<DriverMessage>,
        role: Role,
        initial_credit: u32,
        diagnostic_state: Option<Arc<DiagnosticState>>,
        dispatcher: Option<Box<dyn ServiceDispatcher>>,
    ) -> Self {
        let handle = ConnectionHandle::new_with_diagnostics(
            conn_id,
            driver_tx.clone(),
            role,
            initial_credit,
            diagnostic_state.clone(),
        );
        let mut server_channel_registry =
            ChannelRegistry::new_with_credit_and_role(conn_id, initial_credit, driver_tx, role);
        server_channel_registry.set_diagnostic_state(diagnostic_state.clone());
        Self {
            conn_id,
            handle,
            server_channel_registry,
            dispatcher,
            pending_responses: HashMap::new(),
            in_flight_server_requests: HashMap::new(),
        }
    }

    /// Fail all pending responses (on connection close).
    fn fail_pending_responses(&mut self) {
        for (_, tx) in self.pending_responses.drain() {
            let _ = tx.send(Err(TransportError::ConnectionClosed));
        }
    }

    /// Abort all in-flight server requests (on connection close).
    fn abort_in_flight_requests(&mut self) {
        for (_, abort_handle) in self.in_flight_server_requests.drain() {
            abort_handle.abort();
        }
    }
}

/// Pending outgoing Connect request.
struct PendingConnect {
    response_tx: peeps_sync::OneshotSender<Result<ConnectionHandle, ConnectError>>,
    dispatcher: Option<Box<dyn ServiceDispatcher>>,
}

/// An incoming virtual connection request.
///
/// Received via the `IncomingConnections` receiver returned from `establish_guest`.
/// Call `accept()` to accept the connection and get a handle,
/// or `reject()` to refuse it.
pub struct IncomingConnection {
    /// The request ID for this Connect request.
    request_id: u64,
    /// Metadata from the Connect message.
    pub metadata: roam_wire::Metadata,
    /// Channel to send the Accept/Reject response.
    response_tx: peeps_sync::OneshotSender<IncomingConnectionResponse>,
}

impl IncomingConnection {
    /// Accept this connection and receive a handle for it.
    ///
    /// The `metadata` will be sent in the Accept message.
    ///
    /// The `dispatcher` will handle incoming requests on this virtual connection.
    /// If None, the parent link's dispatcher will be used (and only calls can be made,
    /// not received).
    pub async fn accept(
        self,
        metadata: roam_wire::Metadata,
        dispatcher: Option<Box<dyn ServiceDispatcher>>,
    ) -> Result<ConnectionHandle, TransportError> {
        let (handle_tx, handle_rx) = peeps_sync::oneshot_channel("shm_incoming_conn_accept");
        let _ = self.response_tx.send(IncomingConnectionResponse::Accept {
            request_id: self.request_id,
            metadata,
            dispatcher,
            handle_tx,
        });
        handle_rx
            .recv()
            .await
            .map_err(|_| TransportError::DriverGone)?
    }

    /// Reject this connection with a reason.
    pub fn reject(self, reason: String, metadata: roam_wire::Metadata) {
        let _ = self.response_tx.send(IncomingConnectionResponse::Reject {
            request_id: self.request_id,
            reason,
            metadata,
        });
    }
}

/// Internal response for incoming connection handling.
pub enum IncomingConnectionResponse {
    Accept {
        request_id: u64,
        metadata: roam_wire::Metadata,
        dispatcher: Option<Box<dyn ServiceDispatcher>>,
        handle_tx: peeps_sync::OneshotSender<Result<ConnectionHandle, TransportError>>,
    },
    Reject {
        request_id: u64,
        reason: String,
        metadata: roam_wire::Metadata,
    },
}

/// Receiver for incoming virtual connection requests.
pub type IncomingConnections = peeps_sync::Receiver<IncomingConnection>;

// ============================================================================
// ShmDriver - Single-peer driver (guest side)
// ============================================================================

/// The SHM connection driver - a future that handles bidirectional RPC.
///
/// This must be spawned or awaited to drive the connection forward.
/// Use [`ConnectionHandle`] to make outgoing calls.
///
/// The type parameter `T` is the transport type (e.g., `ShmGuestTransport`).
pub struct ShmDriver<T, D> {
    io: T,
    dispatcher: D,
    role: Role,
    negotiated: ShmNegotiated,

    /// Sender for driver messages (cloned to ConnectionHandles).
    driver_tx: peeps_sync::Sender<DriverMessage>,

    /// Unified channel for all messages (Call/Data/Close/Response).
    /// Single channel ensures FIFO ordering.
    driver_rx: peeps_sync::Receiver<DriverMessage>,

    /// All virtual connections (including ROOT).
    connections: HashMap<ConnectionId, VirtualConnectionState>,

    /// Next connection ID to allocate (for Accept responses).
    next_conn_id: u64,

    /// Pending outgoing Connect requests (request_id -> response channel).
    pending_connects: HashMap<u64, PendingConnect>,

    /// Channel for incoming connection requests.
    incoming_connections_tx: Option<peeps_sync::Sender<IncomingConnection>>,

    /// Channel for incoming connection responses (Accept/Reject from app code).
    incoming_response_rx: peeps_sync::Receiver<IncomingConnectionResponse>,
    incoming_response_tx: peeps_sync::Sender<IncomingConnectionResponse>,

    /// Diagnostic state for tracking in-flight requests (for diagnostics dumps).
    diagnostic_state: Option<Arc<DiagnosticState>>,
}

impl<T, D> ShmDriver<T, D>
where
    T: MessageTransport,
    D: ServiceDispatcher,
{
    /// Run the driver until the connection closes.
    pub async fn run(mut self) -> Result<(), ShmConnectionError> {
        loop {
            trace!("driver: starting select loop");
            tokio::select! {
                biased;

                // Handle incoming connection responses (Accept/Reject from app code).
                Some(response) = self.incoming_response_rx.recv() => {
                    trace!("driver: received incoming connection response");
                    self.handle_incoming_response(response).await?;
                }

                // Handle all driver messages (Call/Data/Close/Response/Connect).
                // Single channel ensures FIFO ordering.
                Some(msg) = self.driver_rx.recv() => {
                    trace!("driver: received driver message");
                    self.handle_driver_message(msg).await?;
                }

                // Handle incoming messages from peer (waits on doorbell, no timeout)
                result = MessageTransport::recv(&mut self.io) => {
                    trace!("driver: received message from peer");
                    match self.handle_recv(result).await {
                        Ok(true) => {
                            trace!("driver: handle_recv returned Ok(true), continuing");
                            continue;
                        }
                        Ok(false) => {
                            trace!("driver: handle_recv returned Ok(false), shutting down");
                            return Ok(());
                        }
                        Err(e) => {
                            trace!("driver: handle_recv returned Err, shutting down");
                            return Err(e);
                        }
                    }
                }
            }
        }
    }

    /// Handle an Accept/Reject response from application code.
    async fn handle_incoming_response(
        &mut self,
        response: IncomingConnectionResponse,
    ) -> Result<(), ShmConnectionError> {
        match response {
            IncomingConnectionResponse::Accept {
                request_id,
                metadata,
                dispatcher,
                handle_tx,
            } => {
                // Allocate a new connection ID
                let conn_id = ConnectionId::new(self.next_conn_id);
                self.next_conn_id += 1;

                // Create connection state
                let conn_state = VirtualConnectionState::new(
                    conn_id,
                    self.driver_tx.clone(),
                    self.role,
                    self.negotiated.initial_credit,
                    self.diagnostic_state.clone(),
                    dispatcher,
                );
                let handle = conn_state.handle.clone();
                self.connections.insert(conn_id, conn_state);

                // Send Accept message
                let msg = Message::Accept {
                    request_id,
                    conn_id,
                    metadata,
                };
                MessageTransport::send(&mut self.io, &msg).await?;

                // Return the handle to the caller
                let _ = handle_tx.send(Ok(handle));
            }
            IncomingConnectionResponse::Reject {
                request_id,
                reason,
                metadata,
            } => {
                let msg = Message::Reject {
                    request_id,
                    reason,
                    metadata,
                };
                MessageTransport::send(&mut self.io, &msg).await?;
            }
        }
        Ok(())
    }

    /// Handle a driver message (Call/Data/Close/Response/Connect).
    async fn handle_driver_message(
        &mut self,
        msg: DriverMessage,
    ) -> Result<(), ShmConnectionError> {
        #[allow(unreachable_patterns)]
        let wire_msg = match msg {
            DriverMessage::Call {
                conn_id,
                request_id,
                method_id,
                metadata,
                channels,
                payload,
                response_tx,
            } => {
                trace!(
                    "handle_driver_message: Call req={} conn={:?}",
                    request_id, conn_id
                );
                // Store the response channel in the connection's state
                if let Some(conn) = self.connections.get_mut(&conn_id) {
                    conn.pending_responses.insert(request_id, response_tx);
                } else {
                    // Unknown connection - fail the call
                    let _ = response_tx.send(Err(TransportError::ConnectionClosed));
                    return Ok(());
                }

                // Send the request
                Message::Request {
                    conn_id,
                    request_id,
                    method_id,
                    metadata,
                    channels,
                    payload,
                }
            }
            DriverMessage::Data {
                conn_id,
                channel_id,
                payload,
            } => {
                trace!(
                    "handle_driver_message: Data ch={}, {} bytes",
                    channel_id,
                    payload.len()
                );
                Message::Data {
                    conn_id,
                    channel_id,
                    payload,
                }
            }
            DriverMessage::Close {
                conn_id,
                channel_id,
            } => {
                trace!("handle_driver_message: Close ch={}", channel_id);
                Message::Close {
                    conn_id,
                    channel_id,
                }
            }
            DriverMessage::Response {
                conn_id,
                request_id,
                channels,
                payload,
            } => {
                // Check that the request is in-flight for this connection
                // r[impl call.cancel.best-effort] - If cancelled, abort handle was removed,
                // so this will return None and we won't send a duplicate response.
                let should_send = if let Some(conn) = self.connections.get_mut(&conn_id) {
                    conn.in_flight_server_requests.remove(&request_id).is_some()
                } else {
                    false
                };
                if !should_send {
                    return Ok(());
                }
                // Mark request completed for diagnostics
                if let Some(diag) = &self.diagnostic_state {
                    trace!(request_id, name = %diag.name, "completing incoming request");
                    diag.complete_request(request_id);
                }
                Message::Response {
                    conn_id,
                    request_id,
                    metadata: Vec::new(),
                    channels,
                    payload,
                }
            }
            DriverMessage::Connect {
                request_id,
                metadata,
                response_tx,
                dispatcher,
            } => {
                // Store pending connect request
                self.pending_connects.insert(
                    request_id,
                    PendingConnect {
                        response_tx,
                        dispatcher,
                    },
                );
                // Send Connect message
                Message::Connect {
                    request_id,
                    metadata,
                }
            }
            DriverMessage::SweepPendingResponses => {
                // Session driver's stale-response watchdog is link-local and does not
                // map to any wire message on SHM transports.
                return Ok(());
            }
            _ => {
                trace!("handle_driver_message: ignoring unsupported driver message variant");
                return Ok(());
            }
        };
        trace!("handle_driver_message: sending wire message");
        MessageTransport::send(&mut self.io, &wire_msg).await?;
        trace!("handle_driver_message: wire message sent");
        Ok(())
    }

    /// Handle result from recv_timeout.
    /// Returns Ok(true) to continue, Ok(false) to shutdown cleanly, Err for errors.
    async fn handle_recv(
        &mut self,
        result: std::io::Result<Option<Message>>,
    ) -> Result<bool, ShmConnectionError> {
        let msg = match result {
            Ok(Some(m)) => m,
            Ok(None) => return Ok(false), // Clean shutdown
            Err(e) => {
                // Check for protocol errors
                let raw = MessageTransport::last_decoded(&self.io);
                if raw.len() >= 2 && raw[0] == 0x00 && raw[1] != 0x00 {
                    return Err(self
                        .goodbye(
                            "message.hello.unknown-version",
                            format!("Unknown Hello version: {:02x}", raw[1]),
                        )
                        .await);
                }
                if !raw.is_empty() && raw[0] >= 9 {
                    return Err(self
                        .goodbye(
                            "message.unknown-variant",
                            format!("Unknown message variant: {:02x}", raw[0]),
                        )
                        .await);
                }
                if e.kind() == std::io::ErrorKind::InvalidData {
                    return Err(self
                        .goodbye(
                            "message.decode-error",
                            format!("Failed to decode message: {}", e),
                        )
                        .await);
                }
                return Err(ShmConnectionError::Io(e));
            }
        };

        match self.handle_message(msg).await {
            Ok(()) => Ok(true),
            Err(ShmConnectionError::Closed) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Handle a single incoming message.
    async fn handle_message(&mut self, msg: Message) -> Result<(), ShmConnectionError> {
        match msg {
            Message::Hello(_) => {
                // shm[impl shm.handshake]
                // SHM doesn't use Hello - this shouldn't happen as transport rejects it
                return Err(self
                    .goodbye(
                        "shm.handshake",
                        "Received Hello message over SHM (not supported)".into(),
                    )
                    .await);
            }
            Message::Connect {
                request_id,
                metadata,
            } => {
                // Handle incoming virtual connection request
                if let Some(tx) = &self.incoming_connections_tx {
                    // Create a oneshot that routes through incoming_response_tx
                    let (response_tx, response_rx) =
                        peeps_sync::oneshot_channel("shm_conn_response");
                    let incoming = IncomingConnection {
                        request_id,
                        metadata,
                        response_tx,
                    };
                    if tx.try_send(incoming).is_ok() {
                        // Spawn a task to forward the response
                        let incoming_response_tx = self.incoming_response_tx.clone();
                        peeps_tasks::spawn_tracked("roam_shm_forward_response", async move {
                            if let Ok(response) = response_rx.recv().await {
                                let _ = incoming_response_tx.send(response).await;
                            }
                        });
                    } else {
                        // Channel full or closed - reject
                        let msg = Message::Reject {
                            request_id,
                            reason: "not listening".into(),
                            metadata: vec![],
                        };
                        MessageTransport::send(&mut self.io, &msg).await?;
                    }
                } else {
                    // Not listening - reject
                    let msg = Message::Reject {
                        request_id,
                        reason: "not listening".into(),
                        metadata: vec![],
                    };
                    MessageTransport::send(&mut self.io, &msg).await?;
                }
            }
            Message::Accept {
                request_id,
                conn_id,
                metadata: _,
            } => {
                // Handle response to our outgoing Connect request
                if let Some(pending) = self.pending_connects.remove(&request_id) {
                    // Create connection state for the new virtual connection
                    // r[impl core.conn.dispatcher-custom]
                    // Use the dispatcher provided by the initiator
                    let conn_state = VirtualConnectionState::new(
                        conn_id,
                        self.driver_tx.clone(),
                        self.role,
                        self.negotiated.initial_credit,
                        self.diagnostic_state.clone(),
                        pending.dispatcher,
                    );
                    let handle = conn_state.handle.clone();
                    self.connections.insert(conn_id, conn_state);
                    let _ = pending.response_tx.send(Ok(handle));
                }
                // Unknown request_id - ignore (may be late/duplicate)
            }
            Message::Reject {
                request_id,
                reason,
                metadata: _,
            } => {
                // Handle rejection of our outgoing Connect request
                if let Some(pending) = self.pending_connects.remove(&request_id) {
                    let _ = pending
                        .response_tx
                        .send(Err(ConnectError::Rejected(reason)));
                }
                // Unknown request_id - ignore
            }
            Message::Goodbye { conn_id, reason: _ } => {
                if conn_id.is_root() {
                    // Goodbye on root closes entire link
                    for (_, mut conn) in self.connections.drain() {
                        conn.fail_pending_responses();
                        conn.abort_in_flight_requests();
                    }
                    return Err(ShmConnectionError::Closed);
                } else {
                    // Close just this virtual connection
                    if let Some(mut conn) = self.connections.remove(&conn_id) {
                        conn.fail_pending_responses();
                        conn.abort_in_flight_requests();
                    }
                }
            }
            Message::Request {
                conn_id,
                request_id,
                method_id,
                metadata,
                channels,
                payload,
            } => {
                debug!(
                    request_id,
                    method_id,
                    ?conn_id,
                    channels = ?channels,
                    "ShmDriver: received Request with channels"
                );
                self.handle_incoming_request(
                    conn_id, request_id, method_id, metadata, channels, payload,
                )
                .await?;
            }
            Message::Response {
                conn_id,
                request_id,
                channels,
                payload,
                ..
            } => {
                // Route to waiting caller on the appropriate connection
                if let Some(conn) = self.connections.get_mut(&conn_id)
                    && let Some(tx) = conn.pending_responses.remove(&request_id)
                {
                    let _ = tx.send(Ok(ResponseData { payload, channels }));
                }
                // Unknown response IDs are ignored per spec
            }
            Message::Cancel {
                conn_id,
                request_id,
            } => {
                // r[impl call.cancel.message] - Cancel requests callee stop processing.
                // r[impl call.cancel.best-effort] - Cancellation is best-effort.
                self.handle_cancel(conn_id, request_id).await?;
            }
            Message::Data {
                conn_id,
                channel_id,
                payload,
            } => {
                self.handle_data(conn_id, channel_id, payload).await?;
            }
            Message::Close {
                conn_id,
                channel_id,
            } => {
                self.handle_close(conn_id, channel_id).await?;
            }
            Message::Reset {
                conn_id,
                channel_id,
            } => {
                self.handle_reset(conn_id, channel_id)?;
            }
            Message::Credit { .. } => {
                // shm[impl shm.flow.no-credit-message]
                // SHM doesn't use Credit messages - this shouldn't happen
                return Err(self
                    .goodbye(
                        "shm.flow.no-credit-message",
                        "Received Credit message over SHM (not supported)".into(),
                    )
                    .await);
            }
        }
        Ok(())
    }

    /// Handle an incoming request (we're the server for this call).
    async fn handle_incoming_request(
        &mut self,
        conn_id: ConnectionId,
        request_id: u64,
        method_id: u64,
        metadata: roam_wire::Metadata,
        channels: Vec<u64>,
        payload: Vec<u8>,
    ) -> Result<(), ShmConnectionError> {
        let conn = match self.connections.get_mut(&conn_id) {
            Some(c) => c,
            None => {
                // Unknown connection - this is a protocol error
                return Err(self
                    .goodbye(
                        "core.conn.unknown",
                        format!("Request for unknown conn_id={:?}", conn_id),
                    )
                    .await);
            }
        };

        // Duplicate detection
        // r[impl call.request-id.duplicate-detection]
        if conn.in_flight_server_requests.contains_key(&request_id) {
            return Err(self
                .goodbye(
                    "call.request-id.duplicate-detection",
                    format!("Duplicate request_id={}", request_id),
                )
                .await);
        }

        let (request_task_id, request_task_name) = task_context_from_metadata(&metadata);
        // Track incoming request for diagnostics
        if let Some(diag) = &self.diagnostic_state {
            trace!(request_id, method_id, name = %diag.name, "recording incoming request");
            diag.record_incoming_request(
                request_id,
                method_id,
                Some(&metadata),
                request_task_id,
                request_task_name,
                None,
            );
        }

        // Validate metadata
        if let Err(rule_id) = roam_wire::validate_metadata(&metadata) {
            if let Some(diag) = &self.diagnostic_state {
                diag.complete_request(request_id);
            }
            return Err(self
                .goodbye(
                    rule_id,
                    format!("Invalid metadata for request_id={}", request_id),
                )
                .await);
        }

        // Validate payload size
        if payload.len() as u32 > self.negotiated.max_payload_size {
            if let Some(diag) = &self.diagnostic_state {
                diag.complete_request(request_id);
            }
            return Err(self
                .goodbye(
                    "flow.call.payload-limit",
                    format!(
                        "Request payload too large: {} bytes (max {}) for request_id={}",
                        payload.len(),
                        self.negotiated.max_payload_size,
                        request_id
                    ),
                )
                .await);
        }

        // Re-borrow conn for dispatch
        let conn = self.connections.get_mut(&conn_id).unwrap();

        // Build context for dispatch
        let cx = Context::new(
            conn_id,
            roam_wire::RequestId::new(request_id),
            roam_wire::MethodId::new(method_id),
            metadata,
            channels,
        );

        // r[impl core.conn.dispatcher] - Use connection-specific dispatcher if available
        let dispatcher: &dyn ServiceDispatcher = if let Some(ref conn_dispatcher) = conn.dispatcher
        {
            conn_dispatcher.as_ref()
        } else {
            &self.dispatcher
        };

        debug!(
            conn_id = conn_id.raw(),
            request_id, method_id, "dispatching incoming request"
        );

        conn.server_channel_registry
            .set_current_request_id(Some(request_id));
        let handler_fut = dispatcher.dispatch(cx, payload, &mut conn.server_channel_registry);
        conn.server_channel_registry.set_current_request_id(None);

        // r[impl call.cancel.best-effort] - Store abort handle for cancellation support
        let join_handle = peeps_tasks::spawn_tracked("roam_shm_handle_request", handler_fut);
        conn.in_flight_server_requests
            .insert(request_id, join_handle.abort_handle());
        Ok(())
    }

    /// Handle a Cancel message from the remote peer.
    ///
    /// r[impl call.cancel.message] - Cancel requests callee stop processing.
    /// r[impl call.cancel.best-effort] - Cancellation is best-effort; handler may have completed.
    /// r[impl call.cancel.no-response-required] - We still send a Cancelled response.
    async fn handle_cancel(
        &mut self,
        conn_id: ConnectionId,
        request_id: u64,
    ) -> Result<(), ShmConnectionError> {
        // Get the connection
        let conn = match self.connections.get_mut(&conn_id) {
            Some(c) => c,
            None => {
                // Unknown connection - ignore (may have been closed)
                return Ok(());
            }
        };

        // Remove and abort the in-flight request if it exists
        if let Some(abort_handle) = conn.in_flight_server_requests.remove(&request_id) {
            // Abort the handler task (best-effort)
            abort_handle.abort();

            // Mark request completed for diagnostics
            if let Some(diag) = &self.diagnostic_state {
                diag.complete_request(request_id);
            }

            // Send a Cancelled response
            // r[impl call.cancel.best-effort] - The callee MUST still send a Response.
            let wire_msg = Message::Response {
                conn_id,
                request_id,
                metadata: vec![],
                channels: vec![],
                // Cancelled error: Result::Err(1) + RoamError::Cancelled(3)
                payload: vec![1, 3],
            };
            self.io.send(&wire_msg).await?;
        }
        // If request not found, it already completed - nothing to do

        Ok(())
    }

    /// Handle incoming Data message.
    async fn handle_data(
        &mut self,
        conn_id: ConnectionId,
        channel_id: u64,
        payload: Vec<u8>,
    ) -> Result<(), ShmConnectionError> {
        trace!(
            "handle_data called for conn {:?} channel {}, {} bytes",
            conn_id,
            channel_id,
            payload.len()
        );
        if channel_id == 0 {
            return Err(self
                .goodbye(
                    "streaming.id.zero-reserved",
                    "Data message with channel_id=0 (reserved)".into(),
                )
                .await);
        }

        if payload.len() as u32 > self.negotiated.max_payload_size {
            return Err(self
                .goodbye(
                    "flow.call.payload-limit",
                    format!(
                        "Data payload too large: {} bytes (max {})",
                        payload.len(),
                        self.negotiated.max_payload_size
                    ),
                )
                .await);
        }

        let conn = match self.connections.get_mut(&conn_id) {
            Some(c) => c,
            None => {
                return Err(self
                    .goodbye(
                        "core.conn.unknown",
                        format!("Data for unknown conn_id={:?}", conn_id),
                    )
                    .await);
            }
        };

        // Try server registry first, then client handle
        let in_server = conn.server_channel_registry.contains_incoming(channel_id);
        let in_client = conn.handle.contains_channel(channel_id);
        let payload_len = payload.len();

        let result = if in_server {
            trace!("routing to server_channel_registry");
            conn.server_channel_registry
                .route_data(channel_id, payload)
                .await
        } else if in_client {
            trace!("routing to client handle");
            conn.handle.route_data(channel_id, payload).await
        } else {
            trace!("channel {} unknown", channel_id);
            Err(ChannelError::Unknown)
        };

        match result {
            Ok(()) => Ok(()),
            Err(ChannelError::Unknown) => {
                Err(self
                    .goodbye(
                        "streaming.unknown",
                        format!(
                            "Data for unknown channel_id={} (in_server={}, in_client={}, payload_len={})",
                            channel_id, in_server, in_client, payload_len
                        ),
                    )
                    .await)
            }
            Err(ChannelError::DataAfterClose) => {
                Err(self
                    .goodbye(
                        "streaming.data-after-close",
                        format!("Data after close on channel_id={}", channel_id),
                    )
                    .await)
            }
            Err(ChannelError::CreditOverrun) => {
                Err(self
                    .goodbye(
                        "flow.stream.credit-overrun",
                        format!("Credit overrun on channel_id={}", channel_id),
                    )
                    .await)
            }
        }
    }

    /// Handle incoming Close message.
    async fn handle_close(
        &mut self,
        conn_id: ConnectionId,
        channel_id: u64,
    ) -> Result<(), ShmConnectionError> {
        if channel_id == 0 {
            return Err(self
                .goodbye(
                    "streaming.id.zero-reserved",
                    "Close message with channel_id=0 (reserved)".into(),
                )
                .await);
        }

        let conn = match self.connections.get_mut(&conn_id) {
            Some(c) => c,
            None => {
                return Err(self
                    .goodbye(
                        "core.conn.unknown",
                        format!("Close for unknown conn_id={:?}", conn_id),
                    )
                    .await);
            }
        };

        // Try server registry first, then client handle
        let in_server = conn.server_channel_registry.contains(channel_id);
        let in_client = conn.handle.contains_channel(channel_id);

        if in_server {
            conn.server_channel_registry.close(channel_id);
        } else if in_client {
            conn.handle.close_channel(channel_id);
        } else {
            return Err(self
                .goodbye(
                    "streaming.unknown",
                    format!(
                        "Close for unknown channel_id={} (in_server={}, in_client={})",
                        channel_id, in_server, in_client
                    ),
                )
                .await);
        }
        Ok(())
    }

    /// Handle incoming Reset message.
    fn handle_reset(
        &mut self,
        conn_id: ConnectionId,
        channel_id: u64,
    ) -> Result<(), ShmConnectionError> {
        let conn = match self.connections.get_mut(&conn_id) {
            Some(c) => c,
            None => {
                // Reset on unknown connection is silently ignored
                return Ok(());
            }
        };

        // Try both registries - Reset on unknown stream is not an error
        if conn.server_channel_registry.contains(channel_id) {
            conn.server_channel_registry.reset(channel_id);
        } else if conn.handle.contains_channel(channel_id) {
            conn.handle.reset_channel(channel_id);
        }
        // Unknown stream for Reset is ignored per spec
        Ok(())
    }

    /// Send Goodbye and return error.
    async fn goodbye(&mut self, rule_id: &'static str, context: String) -> ShmConnectionError {
        // Fail all pending responses and abort in-flight requests for all connections
        for (_, mut conn) in self.connections.drain() {
            conn.fail_pending_responses();
            conn.abort_in_flight_requests();
        }

        // Fail all pending connect requests
        for (_, pending) in self.pending_connects.drain() {
            let _ = pending
                .response_tx
                .send(Err(ConnectError::Rejected("connection closing".into())));
        }

        if let Err(_e) = MessageTransport::send(
            &mut self.io,
            &Message::Goodbye {
                conn_id: ConnectionId::ROOT,
                reason: rule_id.into(),
            },
        )
        .await
        {
            // Goodbye message failed - peer likely already disconnected
        }

        ShmConnectionError::ProtocolViolation { rule_id, context }
    }
}

/// Establish an SHM connection as a guest.
///
/// Returns:
/// - A handle for making calls on connection 0 (root)
/// - A receiver for incoming virtual connection requests
/// - A driver that must be spawned
///
/// The `IncomingConnections` receiver allows accepting sub-connections opened
/// by the remote peer. If you don't need sub-connections, you can drop it and
/// all Connect requests will be automatically rejected.
///
/// # Arguments
///
/// * `transport` - The SHM guest transport (already attached to segment)
/// * `dispatcher` - Service dispatcher for handling incoming requests
///
/// # Example
///
/// ```ignore
/// use roam_shm::transport::ShmGuestTransport;
/// use roam_shm::driver::establish_guest;
///
/// // For spawned processes, use from_spawn_args:
/// let transport = ShmGuestTransport::from_spawn_args(&args)?;
/// let (handle, incoming, driver) = establish_guest(transport, dispatcher);
/// tokio::spawn(driver.run());
/// // Use handle to make calls
/// // Use incoming.recv() to accept virtual connections
/// ```
pub fn establish_guest<D>(
    transport: ShmGuestTransport,
    dispatcher: D,
) -> (
    ConnectionHandle,
    IncomingConnections,
    ShmDriver<ShmGuestTransport, D>,
)
where
    D: ServiceDispatcher,
{
    establish_guest_with_diagnostics(transport, dispatcher, None)
}

/// Create a guest connection with optional diagnostic state for diagnostics dumps.
///
/// Same as [`establish_guest`] but allows passing a [`roam_session::diagnostic::DiagnosticState`]
/// for tracking in-flight requests and channels.
pub fn establish_guest_with_diagnostics<D>(
    transport: ShmGuestTransport,
    dispatcher: D,
    diagnostic_state: Option<Arc<DiagnosticState>>,
) -> (
    ConnectionHandle,
    IncomingConnections,
    ShmDriver<ShmGuestTransport, D>,
)
where
    D: ServiceDispatcher,
{
    #[cfg(feature = "diagnostics")]
    let diagnostic_state = diagnostic_state.or_else(|| {
        let state = Arc::new(DiagnosticState::new("shm-guest"));
        // Note: guest name can be set later via state.set_peer_name() if needed
        roam_session::diagnostic::register_diagnostic_state(&state);
        Some(state)
    });

    // Get config from segment header (already read during attach)
    let config = transport.config();
    let negotiated = ShmNegotiated {
        max_payload_size: config.max_payload_size,
        initial_credit: config.initial_credit,
    };

    // Create single unified channel for all messages (Call/Data/Close/Response).
    // Single channel ensures FIFO ordering.
    let (driver_tx, driver_rx) = peeps_sync::channel("shm_driver", 256);

    // Guest is initiator (uses odd stream IDs)
    let role = Role::Initiator;
    // Use infinite credit for now (matches current roam-stream behavior).
    let initial_credit = u32::MAX;

    // Create root connection state
    let root_conn = VirtualConnectionState::new(
        ConnectionId::ROOT,
        driver_tx.clone(),
        role,
        initial_credit,
        diagnostic_state.clone(),
        None,
    );
    let handle = root_conn.handle.clone();

    let mut connections = HashMap::new();
    connections.insert(ConnectionId::ROOT, root_conn);

    // Create channel for incoming connection requests
    let (incoming_connections_tx, incoming_connections_rx) =
        peeps_sync::channel("shm_incoming_connections", 64);

    // Create channel for incoming connection responses (Accept/Reject from app code)
    let (incoming_response_tx, incoming_response_rx) =
        peeps_sync::channel("shm_incoming_responses", 64);

    let driver = ShmDriver {
        io: transport,
        dispatcher,
        role,
        negotiated,
        driver_tx,
        driver_rx,
        connections,
        next_conn_id: 1, // 0 is ROOT, start allocating at 1
        pending_connects: HashMap::new(),
        incoming_connections_tx: Some(incoming_connections_tx),
        incoming_response_rx,
        incoming_response_tx,
        diagnostic_state,
    };

    (handle, incoming_connections_rx, driver)
}

// ============================================================================
// Multi-Peer Host Driver
// ============================================================================

/// Per-peer state for the multi-peer host driver.
///
/// Uses `Box<dyn ServiceDispatcher>` to allow each peer to have a different
/// dispatcher type, enabling heterogeneous bidirectional RPC scenarios.
///
/// Each peer can have multiple virtual connections (Connection 0 = ROOT, plus
/// any opened via Connect/Accept).
struct PeerConnectionState {
    /// Dispatcher for handling incoming requests from this peer.
    /// Boxed to allow different dispatcher types per peer.
    dispatcher: Box<dyn ServiceDispatcher>,

    /// All virtual connections for this peer (including ROOT).
    connections: HashMap<ConnectionId, VirtualConnectionState>,

    /// Next connection ID to allocate (for Accept responses).
    next_conn_id: u64,

    /// Pending outgoing Connect requests (request_id -> response channel).
    pending_connects: HashMap<u64, PendingConnect>,

    /// Channel for incoming connection requests from this peer.
    incoming_connections_tx: Option<peeps_sync::Sender<IncomingConnection>>,

    /// Channel for incoming connection responses (Accept/Reject from app code).
    incoming_response_tx: peeps_sync::Sender<IncomingConnectionResponse>,

    /// Diagnostic state for tracking in-flight requests (for diagnostics dumps).
    diagnostic_state: Option<Arc<DiagnosticState>>,
}

/// Command to control the multi-peer host driver.
enum ControlCommand {
    /// Create a new peer slot and return a spawn ticket (calls host.add_peer()).
    Create {
        options: crate::spawn::AddPeerOptions,
        response: peeps_sync::OneshotSender<Result<crate::spawn::SpawnTicket, std::io::Error>>,
    },
    /// Register a peer dynamically with a dispatcher.
    Add {
        peer_id: PeerId,
        dispatcher: Box<dyn ServiceDispatcher>,
        diagnostic_state: Option<Arc<DiagnosticState>>,
        response: peeps_sync::OneshotSender<(ConnectionHandle, IncomingConnections)>,
    },
    /// Release a previously reserved peer slot.
    Release {
        peer_id: PeerId,
        response: peeps_sync::OneshotSender<()>,
    },
}

/// Multi-peer host driver for hub topology.
///
/// Unlike `ShmDriver` which handles a single peer, this driver manages
/// multiple peers over a single `ShmHost`. Each peer gets its own
/// `ConnectionHandle` for making RPC calls, plus support for virtual connections.
///
/// This driver supports **heterogeneous dispatchers**: each peer can have a
/// different dispatcher type. This enables bidirectional RPC scenarios where
/// different cells need different callback services from the host.
///
/// Peers can be added either at build time or dynamically while the driver is
/// running, enabling lazy spawning patterns.
///
/// # Example
///
/// ```ignore
/// use roam_shm::{ShmHost, SegmentConfig};
/// use roam_shm::driver::MultiPeerHostDriver;
///
/// let host = ShmHost::create("/dev/shm/myapp", SegmentConfig::default())?;
///
/// // Add peers and get tickets for spawning
/// let ticket1 = host.add_peer(options1)?;
/// let ticket2 = host.add_peer(options2)?;
///
/// // Create driver with different dispatchers per peer
/// let (driver, handles, incoming_map) = MultiPeerHostDriver::builder(host)
///     // Simple peer only needs lifecycle dispatcher
///     .add_peer(ticket1.peer_id(), CellLifecycleDispatcher::new(lifecycle.clone()))
///     // Complex peer needs routed dispatcher for bidirectional RPC
///     .add_peer(ticket2.peer_id(), RoutedDispatcher::new(
///         CellLifecycleDispatcher::new(lifecycle.clone()),
///         TemplateHostDispatcher::new(template_host),
///     ))
///     .build();
///
/// // Spawn the driver
/// let driver_handle = driver.handle();
/// tokio::spawn(driver.run());
///
/// // Accept virtual connections from peers
/// while let Some(conn) = incoming_map[&ticket1.peer_id()].recv().await {
///     let handle = conn.accept(vec![]).await?;
///     // handle is now a virtual connection for this specific browser
/// }
/// ```
pub struct MultiPeerHostDriver {
    /// The SHM host (owned).
    host: ShmHost,

    /// Negotiated parameters from segment config.
    negotiated: ShmNegotiated,

    /// Per-peer connection state.
    peers: HashMap<PeerId, PeerConnectionState>,

    /// Doorbells for each peer (shared with waiter tasks via Arc).
    /// Used to signal guests when we send them messages.
    doorbells: HashMap<PeerId, Arc<shm_primitives::Doorbell>>,

    /// Buffer for last decoded bytes (for error detection).
    last_decoded: Vec<u8>,

    /// Control channel for dynamic peer addition (bounded, auditable).
    control_rx: AuditableReceiver<ControlCommand>,

    /// Ring notifications from doorbell tasks (peer has messages ready).
    ring_rx: AuditableReceiver<PeerId>,

    /// Sender for ring notifications (cloned for each doorbell task).
    ring_tx: AuditableSender<PeerId>,

    /// Unified channel for driver messages from all peers (outgoing calls).
    driver_msg_rx: AuditableReceiver<(PeerId, DriverMessage)>,

    /// Sender for unified driver messages (cloned for each peer forwarder task).
    driver_msg_tx: AuditableSender<(PeerId, DriverMessage)>,

    /// Unified channel for incoming connection responses from all peers.
    incoming_response_rx: AuditableReceiver<(PeerId, IncomingConnectionResponse)>,

    /// Sender for incoming connection responses (cloned for each peer).
    incoming_response_tx: AuditableSender<(PeerId, IncomingConnectionResponse)>,

    /// Pending outbound messages waiting for backpressure to clear.
    /// When host slots are exhausted, messages are queued here and retried
    /// when the guest rings the doorbell (indicating it has consumed messages).
    pending_sends: AuditableDequeMap<PeerId, Message>,

    /// Registered SHM segment view for global diagnostics dumps.
    #[cfg(feature = "diagnostics")]
    _shm_diagnostic_view: Option<Arc<crate::diagnostic::ShmDiagnosticView>>,
}

/// Handle for controlling a running MultiPeerHostDriver.
///
/// This handle allows adding new peers dynamically after the driver has started.
#[derive(Clone)]
pub struct MultiPeerHostDriverHandle {
    control_tx: AuditableSender<ControlCommand>,
}

/// Builder for `MultiPeerHostDriver`.
pub struct MultiPeerHostDriverBuilder {
    host: ShmHost,
    peers: Vec<(PeerId, Box<dyn ServiceDispatcher>, Option<String>)>,
}

impl MultiPeerHostDriverBuilder {
    /// Add a peer with its dispatcher.
    ///
    /// Each peer can have a different dispatcher type, enabling heterogeneous
    /// bidirectional RPC scenarios where different cells need different
    /// callback services from the host.
    pub fn add_peer<D>(mut self, peer_id: PeerId, dispatcher: D) -> Self
    where
        D: ServiceDispatcher + 'static,
    {
        self.peers.push((peer_id, Box::new(dispatcher), None));
        self
    }

    /// Add a peer with its dispatcher and a human-readable name for diagnostics.
    pub fn add_peer_named<D>(
        mut self,
        peer_id: PeerId,
        dispatcher: D,
        name: impl Into<String>,
    ) -> Self
    where
        D: ServiceDispatcher + 'static,
    {
        self.peers
            .push((peer_id, Box::new(dispatcher), Some(name.into())));
        self
    }

    /// Build the driver and return connection handles, incoming connections, and a driver handle.
    ///
    /// Returns:
    /// - The driver (must be spawned)
    /// - A map of peer ID to root connection handle
    /// - A map of peer ID to incoming virtual connection receiver
    /// - A driver handle for dynamic peer management
    pub fn build(
        mut self,
    ) -> (
        MultiPeerHostDriver,
        HashMap<PeerId, ConnectionHandle>,
        HashMap<PeerId, IncomingConnections>,
        MultiPeerHostDriverHandle,
    ) {
        #[cfg(feature = "diagnostics")]
        let shm_diagnostic_view = {
            let view = Arc::new(crate::diagnostic::ShmDiagnosticView::from_host(&self.host));
            crate::diagnostic::register_shm_diagnostic_view(&view);
            Some(view)
        };

        let config = self.host.config();
        let negotiated = ShmNegotiated {
            max_payload_size: config.max_payload_size,
            initial_credit: config.initial_credit,
        };

        let mut peers = HashMap::new();
        let mut handles = HashMap::new();
        let mut incoming_connections_map = HashMap::new();
        let mut doorbells = HashMap::new();

        // Create ring channel for doorbell notifications (bounded, auditable)
        let (ring_tx, ring_rx) = auditable::channel("ring_notifications", 256);

        // Create unified channel for driver messages from all peers (bounded, auditable)
        let (driver_msg_tx, driver_msg_rx) = auditable::channel("driver_messages", 1024);

        // Create unified channel for incoming connection responses from all peers
        let (incoming_response_tx, incoming_response_rx) =
            auditable::channel("incoming_responses", 256);

        for (peer_id, dispatcher, _peer_name) in self.peers {
            // Create single unified channel for all messages (Call/Data/Close/Response).
            // Single channel ensures FIFO ordering.
            let (driver_tx, mut driver_rx) = peeps_sync::channel("shm_host_driver", 256);

            // Host is acceptor (uses even stream IDs)
            let role = Role::Acceptor;
            let initial_credit = u32::MAX;

            // Create root connection state
            let root_conn = VirtualConnectionState::new(
                ConnectionId::ROOT,
                driver_tx.clone(),
                role,
                initial_credit,
                None,
                None,
            );
            let handle = root_conn.handle.clone();

            let mut connections = HashMap::new();
            connections.insert(ConnectionId::ROOT, root_conn);

            // Create channel for incoming connection requests from this peer
            let (incoming_connections_tx, incoming_connections_rx) =
                peeps_sync::channel("shm_host_incoming_connections", 64);

            #[cfg(feature = "diagnostics")]
            let peer_diagnostic_state = {
                let diag_name = if let Some(ref name) = _peer_name {
                    format!("shm-host {}", name)
                } else {
                    format!("shm-host-peer-{}", peer_id.get())
                };
                let state = Arc::new(DiagnosticState::new(diag_name));
                roam_session::diagnostic::register_diagnostic_state(&state);
                Some(state)
            };
            #[cfg(not(feature = "diagnostics"))]
            let peer_diagnostic_state: Option<Arc<DiagnosticState>> = None;

            // Create per-peer incoming response forwarder
            let peer_incoming_response_tx = incoming_response_tx.clone();
            let (peer_response_tx, mut peer_response_rx) =
                peeps_sync::channel::<IncomingConnectionResponse>("shm_peer_response", 64);
            peeps_tasks::spawn_tracked("roam_shm_peer_response_router", async move {
                while let Some(response) = peer_response_rx.recv().await {
                    if peer_incoming_response_tx
                        .send((peer_id, response))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            });

            handles.insert(peer_id, handle);
            incoming_connections_map.insert(peer_id, incoming_connections_rx);

            peers.insert(
                peer_id,
                PeerConnectionState {
                    dispatcher,
                    connections,
                    next_conn_id: 1, // 0 is ROOT
                    pending_connects: HashMap::new(),
                    incoming_connections_tx: Some(incoming_connections_tx),
                    incoming_response_tx: peer_response_tx,
                    diagnostic_state: peer_diagnostic_state,
                },
            );

            // Spawn forwarder task for this peer's driver messages
            let driver_msg_tx_clone = driver_msg_tx.clone();
            peeps_tasks::spawn_tracked("roam_shm_peer_driver_fwd", async move {
                while let Some(msg) = driver_rx.recv().await {
                    if driver_msg_tx_clone.send((peer_id, msg)).await.is_err() {
                        // Driver shut down
                        break;
                    }
                }
            });

            // Set up doorbell for this peer (shared via Arc)
            if let Some(doorbell) = self.host.take_doorbell(peer_id) {
                let doorbell = Arc::new(doorbell);
                doorbells.insert(peer_id, doorbell.clone());

                // Spawn doorbell waiter task with cloned Arc
                let ring_tx_clone = ring_tx.clone();
                peeps_tasks::spawn_tracked("roam_shm_doorbell_waiter", async move {
                    trace!("Doorbell waiter task started for peer {:?}", peer_id);
                    // On Windows, accept the named pipe connection from the guest
                    if let Err(e) = doorbell.accept().await {
                        trace!("Doorbell accept failed for peer {:?}: {:?}", peer_id, e);
                        return;
                    }
                    trace!(
                        "Doorbell waiter: accepted connection for peer {:?}",
                        peer_id
                    );
                    loop {
                        trace!("Doorbell waiter: waiting for peer {:?}", peer_id);
                        match doorbell.wait().await {
                            Ok(()) => {
                                trace!("Doorbell waiter: peer {:?} rang doorbell!", peer_id);
                                // Peer rang doorbell, notify driver
                                if ring_tx_clone.send(peer_id).await.is_err() {
                                    trace!(
                                        "Doorbell waiter: driver shut down for peer {:?}",
                                        peer_id
                                    );
                                    break;
                                }
                                trace!("Doorbell waiter: notification sent for peer {:?}", peer_id);
                            }
                            Err(e) => {
                                trace!("Doorbell waiter: error for peer {:?}: {:?}", peer_id, e);
                                // Doorbell error (peer died or fd closed)
                                break;
                            }
                        }
                    }
                    trace!("Doorbell waiter task exiting for peer {:?}", peer_id);
                });

                // Manually trigger an immediate SHM poll for this peer to catch any messages
                // that arrived before the doorbell waiter task started waiting.
                let _ = ring_tx.try_send(peer_id);
            }
        }

        // Create control channel for dynamic peer addition (bounded, auditable)
        let (control_tx, control_rx) = auditable::channel("control_commands", 64);

        let driver = MultiPeerHostDriver {
            host: self.host,
            negotiated,
            peers,
            doorbells,
            last_decoded: Vec::new(),
            control_rx,
            ring_rx,
            ring_tx: ring_tx.clone(),
            driver_msg_rx,
            driver_msg_tx: driver_msg_tx.clone(),
            incoming_response_rx,
            incoming_response_tx,
            pending_sends: AuditableDequeMap::new("pending_sends[", 1024),
            #[cfg(feature = "diagnostics")]
            _shm_diagnostic_view: shm_diagnostic_view,
        };

        let driver_handle = MultiPeerHostDriverHandle { control_tx };

        (driver, handles, incoming_connections_map, driver_handle)
    }
}

impl MultiPeerHostDriver {
    /// Create a new builder for the multi-peer host driver.
    pub fn builder(host: ShmHost) -> MultiPeerHostDriverBuilder {
        MultiPeerHostDriverBuilder {
            host,
            peers: Vec::new(),
        }
    }

    /// Run the driver until all peers disconnect or an error occurs.
    pub async fn run(self) -> Result<(), ShmConnectionError> {
        let result: Result<Result<(), ShmConnectionError>, Box<dyn std::any::Any + Send>> =
            futures_util::FutureExt::catch_unwind(std::panic::AssertUnwindSafe(self.run_inner()))
                .await;

        match result {
            Ok(res) => {
                if res.is_ok() {
                    warn!("MultiPeerHostDriver returned normally without error");
                } else {
                    warn!(
                        "MultiPeerHostDriver returned normally with error: {:?}",
                        res
                    );
                }
                res
            }
            Err(panic) => {
                if let Some(s) = panic.downcast_ref::<String>() {
                    error!("MultiPeerHostDriver panicked: {}", s);
                } else if let Some(s) = panic.downcast_ref::<&str>() {
                    error!("MultiPeerHostDriver panicked: {}", s);
                } else {
                    error!("MultiPeerHostDriver panicked with unknown payload");
                }
                std::panic::resume_unwind(panic);
            }
        }
    }

    async fn run_inner(mut self) -> Result<(), ShmConnectionError> {
        debug!(
            "MultiPeerHostDriver::run() entered, peers={}",
            self.peers.len()
        );
        loop {
            trace!("top of loop, peers={}", self.peers.len());
            tokio::select! {
                // Control commands (add peers dynamically)
                Some(cmd) = self.control_rx.recv() => {
                    trace!("MultiPeerHostDriver: received control command");
                    self.handle_control_command(cmd).await;
                }

                // Incoming connection responses (Accept/Reject from app code)
                Some((peer_id, response)) = self.incoming_response_rx.recv() => {
                    trace!("MultiPeerHostDriver: received incoming connection response for peer {:?}", peer_id);
                    if let Err(e) = self.handle_incoming_response(peer_id, response).await {
                        warn!("MultiPeerHostDriver: error handling incoming response for peer {:?}: {:?}", peer_id, e);
                    }
                }

                // Doorbell rang - peer has SHM messages ready
                Some(peer_id) = self.ring_rx.recv() => {
                    trace!("MultiPeerHostDriver: doorbell rang for peer {:?}", peer_id);
                    // Poll SHM host for ALL ready messages (not just from the peer that rang)
                    let result = self.host.poll();
                    trace!("MultiPeerHostDriver: poll returned {} messages", result.messages.len());

                    // Ring doorbells for guests whose slots were freed (backpressure wakeup)
                    for freed_peer_id in result.slots_freed_for {
                        if let Some(doorbell) = self.doorbells.get(&freed_peer_id) {
                            doorbell.signal().await;
                        }
                    }

                    for (pid, shm_msg) in result.messages {
                        self.last_decoded = shm_msg.payload_bytes().to_vec();

                        let msg = match shm_msg_to_message(shm_msg).map_err(|e| {
                            ShmConnectionError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
                        }) {
                            Ok(m) => m,
                            Err(e) => {
                                warn!("MultiPeerHostDriver: failed to decode message from peer {:?}: {:?}", pid, e);
                                continue;
                            }
                        };

                        trace!("MultiPeerHostDriver: handling SHM message from peer {:?}: {}", pid, msg_type_name(&msg));
                        if let Err(e) = self.handle_message(pid, msg).await {
                            warn!("MultiPeerHostDriver: error handling message from peer {:?}: {:?}", pid, e);
                            // Continue processing other peers - don't let one peer's error crash the driver
                        }
                    }

                    // Retry pending sends for ALL peers - any doorbell activity is a good time to drain
                    // queues, since the peer that rang isn't necessarily the one with stuck messages.
                    // shm[impl shm.backpressure.host-to-guest]
                    self.retry_all_pending_sends().await;
                }

                // Driver message (outgoing call from ConnectionHandle)
                Some((peer_id, msg)) = self.driver_msg_rx.recv() => {
                    trace!("MultiPeerHostDriver: received driver message for peer {:?}", peer_id);
                    if let Err(e) = self.handle_driver_message(peer_id, msg).await {
                        warn!("MultiPeerHostDriver: error sending to peer {:?}: {:?}", peer_id, e);
                        // Continue processing other peers - don't let one peer's error crash the driver
                    }
                }

                // All channels closed - shut down
                else => {
                    warn!("MultiPeerHostDriver: all channels closed, shutting down");
                    return Ok(());
                }
            }
        }
    }

    /// Handle an Accept/Reject response from application code for a specific peer.
    async fn handle_incoming_response(
        &mut self,
        peer_id: PeerId,
        response: IncomingConnectionResponse,
    ) -> Result<(), ShmConnectionError> {
        let state = match self.peers.get_mut(&peer_id) {
            Some(s) => s,
            None => return Ok(()), // Peer gone
        };

        match response {
            IncomingConnectionResponse::Accept {
                request_id,
                metadata,
                dispatcher,
                handle_tx,
            } => {
                // Allocate a new connection ID
                let conn_id = ConnectionId::new(state.next_conn_id);
                state.next_conn_id += 1;

                // Get driver_tx from existing root connection
                let driver_tx = state
                    .connections
                    .get(&ConnectionId::ROOT)
                    .map(|c| c.handle.driver_tx().clone())
                    .unwrap();

                // Create connection state
                // r[impl core.conn.dispatcher-custom]
                // Use the dispatcher provided by the initiator
                let conn_state = VirtualConnectionState::new(
                    conn_id,
                    driver_tx,
                    Role::Acceptor,
                    self.negotiated.initial_credit,
                    state.diagnostic_state.clone(),
                    dispatcher,
                );
                let handle = conn_state.handle.clone();
                state.connections.insert(conn_id, conn_state);

                // Send Accept message
                let msg = Message::Accept {
                    request_id,
                    conn_id,
                    metadata,
                };
                self.send_to_peer(peer_id, &msg).await?;

                // Return the handle to the caller
                let _ = handle_tx.send(Ok(handle));
            }
            IncomingConnectionResponse::Reject {
                request_id,
                reason,
                metadata,
            } => {
                let msg = Message::Reject {
                    request_id,
                    reason,
                    metadata,
                };
                self.send_to_peer(peer_id, &msg).await?;
            }
        }
        Ok(())
    }

    /// Handle a control command.
    async fn handle_control_command(&mut self, cmd: ControlCommand) {
        match cmd {
            ControlCommand::Create { options, response } => {
                // Call host.add_peer() to create a spawn ticket
                let result = self.host.add_peer(options);
                let _ = response.send(result);
            }
            ControlCommand::Add {
                peer_id,
                dispatcher,
                diagnostic_state,
                response,
            } => {
                trace!("MultiPeerHostDriver: adding peer {:?} dynamically", peer_id);
                #[cfg(feature = "diagnostics")]
                let diagnostic_state = diagnostic_state.or_else(|| {
                    let state = Arc::new(DiagnosticState::new(format!(
                        "shm-host-peer-{}",
                        peer_id.get()
                    )));
                    roam_session::diagnostic::register_diagnostic_state(&state);
                    Some(state)
                });
                // Create single unified channel for all messages (Call/Data/Close/Response).
                let (driver_tx, mut driver_rx) =
                    peeps_sync::channel("shm_dynamic_peer_driver", 256);

                // Host is acceptor (uses even stream IDs)
                let role = Role::Acceptor;
                let initial_credit = u32::MAX;

                // Create root connection state
                // r[impl core.conn.dispatcher-default]
                // Root uses None for dispatcher - it uses the peer's dispatcher
                let root_conn = VirtualConnectionState::new(
                    ConnectionId::ROOT,
                    driver_tx.clone(),
                    role,
                    initial_credit,
                    diagnostic_state.clone(),
                    None,
                );
                let handle = root_conn.handle.clone();

                let mut connections = HashMap::new();
                connections.insert(ConnectionId::ROOT, root_conn);

                // Create channel for incoming connection requests from this peer
                let (incoming_connections_tx, incoming_connections_rx) =
                    peeps_sync::channel("shm_dynamic_incoming_connections", 64);

                // Create per-peer incoming response forwarder
                let peer_incoming_response_tx = self.incoming_response_tx.clone();
                let (peer_response_tx, mut peer_response_rx) = peeps_sync::channel::<
                    IncomingConnectionResponse,
                >(
                    "shm_dynamic_peer_response", 64
                );
                peeps_tasks::spawn_tracked("roam_shm_peer_response_fwd", async move {
                    while let Some(resp) = peer_response_rx.recv().await {
                        if peer_incoming_response_tx
                            .send((peer_id, resp))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                });

                self.peers.insert(
                    peer_id,
                    PeerConnectionState {
                        dispatcher,
                        connections,
                        next_conn_id: 1,
                        pending_connects: HashMap::new(),
                        incoming_connections_tx: Some(incoming_connections_tx),
                        incoming_response_tx: peer_response_tx,
                        diagnostic_state,
                    },
                );
                trace!("MultiPeerHostDriver: {} peers now active", self.peers.len());

                // Spawn forwarder task for this peer's driver messages
                let driver_msg_tx = self.driver_msg_tx.clone();
                peeps_tasks::spawn_tracked("roam_shm_peer_driver_fwd", async move {
                    while let Some(msg) = driver_rx.recv().await {
                        if driver_msg_tx.send((peer_id, msg)).await.is_err() {
                            // Driver shut down
                            break;
                        }
                    }
                });

                // Set up doorbell for this peer (shared via Arc)
                trace!("AddPeer: looking for doorbell for {:?}", peer_id);
                if let Some(doorbell) = self.host.take_doorbell(peer_id) {
                    trace!(
                        "AddPeer: found doorbell for {:?}, spawning waiter task",
                        peer_id
                    );
                    let doorbell = Arc::new(doorbell);
                    self.doorbells.insert(peer_id, doorbell.clone());

                    // Spawn doorbell waiter task with cloned Arc
                    let ring_tx = self.ring_tx.clone();
                    peeps_tasks::spawn_tracked("roam_shm_doorbell_waiter", async move {
                        trace!("Doorbell waiter task started for peer {:?}", peer_id);
                        // On Windows, accept the named pipe connection from the guest
                        trace!("Doorbell waiter: calling accept() for {:?}", peer_id);
                        if let Err(e) = doorbell.accept().await {
                            trace!("Doorbell accept failed for peer {:?}: {:?}", peer_id, e);
                            return;
                        }
                        trace!("Doorbell waiter: accept() returned for peer {:?}", peer_id);
                        loop {
                            trace!("Doorbell waiter: calling wait() for peer {:?}", peer_id);
                            match doorbell.wait().await {
                                Ok(()) => {
                                    trace!("Doorbell waiter: peer {:?} rang doorbell!", peer_id);
                                    // Peer rang doorbell, notify driver
                                    if ring_tx.send(peer_id).await.is_err() {
                                        trace!(
                                            "Doorbell waiter: driver shut down for peer {:?}",
                                            peer_id
                                        );
                                        break;
                                    }
                                    trace!(
                                        "Doorbell waiter: notification sent for peer {:?}",
                                        peer_id
                                    );
                                }
                                Err(e) => {
                                    trace!(
                                        "Doorbell waiter: error for peer {:?}: {:?}",
                                        peer_id, e
                                    );
                                    // Doorbell error (peer died or fd closed)
                                    break;
                                }
                            }
                        }
                        trace!("Doorbell waiter task exiting for peer {:?}", peer_id);
                    });

                    // Manually trigger an immediate SHM poll for this peer to catch any messages
                    // that arrived before the doorbell waiter task started waiting.
                    if self.ring_tx.send(peer_id).await.is_err() {
                        trace!(
                            "Initial ring notification failed for peer {:?}: driver shutting down",
                            peer_id
                        );
                    }
                } else {
                    error!("AddPeer: NO DOORBELL FOUND for {:?}!", peer_id);
                }

                // Send the handle and incoming connections receiver back to the caller
                if response.send((handle, incoming_connections_rx)).is_err() {
                    trace!(
                        "AddPeer response channel closed for peer {:?}: caller dropped",
                        peer_id
                    );
                }
            }
            ControlCommand::Release { peer_id, response } => {
                self.peers.remove(&peer_id);
                self.doorbells.remove(&peer_id);
                self.pending_sends.remove(&peer_id);
                self.host.release_peer(peer_id);
                let _ = response.send(());
            }
        }
    }

    /// Handle a driver message (Call/Data/Close/Response/Connect) for a specific peer.
    async fn handle_driver_message(
        &mut self,
        peer_id: PeerId,
        msg: DriverMessage,
    ) -> Result<(), ShmConnectionError> {
        #[allow(unreachable_patterns)]
        let wire_msg = match msg {
            DriverMessage::Call {
                conn_id,
                request_id,
                method_id,
                metadata,
                channels,
                payload,
                response_tx,
            } => {
                debug!(
                    request_id,
                    method_id,
                    ?conn_id,
                    channels = ?channels,
                    "MultiPeerHostDriver: sending Request with channels"
                );
                // Store the response channel in the connection's state
                if let Some(state) = self.peers.get_mut(&peer_id) {
                    if let Some(conn) = state.connections.get_mut(&conn_id) {
                        conn.pending_responses.insert(request_id, response_tx);
                    } else {
                        let _ = response_tx.send(Err(TransportError::ConnectionClosed));
                        return Ok(());
                    }
                } else {
                    let _ = response_tx.send(Err(TransportError::ConnectionClosed));
                    return Ok(());
                }

                // Send the request
                Message::Request {
                    conn_id,
                    request_id,
                    method_id,
                    metadata,
                    channels,
                    payload,
                }
            }
            DriverMessage::Data {
                conn_id,
                channel_id,
                payload,
            } => Message::Data {
                conn_id,
                channel_id,
                payload,
            },
            DriverMessage::Close {
                conn_id,
                channel_id,
            } => Message::Close {
                conn_id,
                channel_id,
            },
            DriverMessage::Response {
                conn_id,
                request_id,
                channels,
                payload,
            } => {
                // Check that the request is in-flight for this connection
                // r[impl call.cancel.best-effort] - If cancelled, abort handle was removed,
                // so this will return None and we won't send a duplicate response.
                let should_send = if let Some(state) = self.peers.get_mut(&peer_id) {
                    if let Some(conn) = state.connections.get_mut(&conn_id) {
                        conn.in_flight_server_requests.remove(&request_id).is_some()
                    } else {
                        false
                    }
                } else {
                    false
                };
                if !should_send {
                    return Ok(());
                }
                // Mark request completed for diagnostics
                if let Some(state) = self.peers.get(&peer_id)
                    && let Some(diag) = &state.diagnostic_state
                {
                    trace!(request_id, name = %diag.name, "completing incoming request");
                    diag.complete_request(request_id);
                }
                Message::Response {
                    conn_id,
                    request_id,
                    metadata: Vec::new(),
                    channels,
                    payload,
                }
            }
            DriverMessage::Connect {
                request_id,
                metadata,
                response_tx,
                dispatcher,
            } => {
                // Store pending connect request
                if let Some(state) = self.peers.get_mut(&peer_id) {
                    state.pending_connects.insert(
                        request_id,
                        PendingConnect {
                            response_tx,
                            dispatcher,
                        },
                    );
                } else {
                    let _ = response_tx.send(Err(ConnectError::Rejected("peer gone".into())));
                    return Ok(());
                }
                // Send Connect message
                Message::Connect {
                    request_id,
                    metadata,
                }
            }
            DriverMessage::SweepPendingResponses => {
                // Session driver's stale-response watchdog is link-local and does not
                // map to any wire message on SHM transports.
                return Ok(());
            }
            _ => {
                trace!(
                    peer = ?peer_id,
                    "MultiPeerHostDriver: ignoring unsupported driver message variant"
                );
                return Ok(());
            }
        };

        self.send_to_peer(peer_id, &wire_msg).await
    }

    /// Handle an incoming message from a specific peer.
    async fn handle_message(
        &mut self,
        peer_id: PeerId,
        msg: Message,
    ) -> Result<(), ShmConnectionError> {
        match msg {
            Message::Hello(_) => {
                return Err(self
                    .goodbye(
                        peer_id,
                        "shm.handshake",
                        "Received Hello message over SHM (not supported)".into(),
                    )
                    .await);
            }
            Message::Connect {
                request_id,
                metadata,
            } => {
                // Handle incoming virtual connection request
                let state = match self.peers.get(&peer_id) {
                    Some(s) => s,
                    None => return Ok(()),
                };
                if let Some(tx) = &state.incoming_connections_tx {
                    // Create a oneshot that routes through incoming_response_tx
                    let (response_tx, response_rx) =
                        peeps_sync::oneshot_channel("shm_conn_response");
                    let incoming = IncomingConnection {
                        request_id,
                        metadata,
                        response_tx,
                    };
                    if tx.try_send(incoming).is_ok() {
                        // Spawn a task to forward the response
                        let incoming_response_tx = state.incoming_response_tx.clone();
                        peeps_tasks::spawn_tracked("roam_shm_connect_response_relay", async move {
                            if let Ok(response) = response_rx.recv().await {
                                let _ = incoming_response_tx.send(response).await;
                            }
                        });
                    } else {
                        // Channel full or closed - reject
                        let msg = Message::Reject {
                            request_id,
                            reason: "not listening".into(),
                            metadata: vec![],
                        };
                        self.send_to_peer(peer_id, &msg).await?;
                    }
                } else {
                    // Not listening - reject
                    let msg = Message::Reject {
                        request_id,
                        reason: "not listening".into(),
                        metadata: vec![],
                    };
                    self.send_to_peer(peer_id, &msg).await?;
                }
            }
            Message::Accept {
                request_id,
                conn_id,
                metadata: _,
            } => {
                // Handle response to our outgoing Connect request
                let state = match self.peers.get_mut(&peer_id) {
                    Some(s) => s,
                    None => return Ok(()),
                };
                if let Some(pending) = state.pending_connects.remove(&request_id) {
                    // Get driver_tx from existing root connection
                    let driver_tx = state
                        .connections
                        .get(&ConnectionId::ROOT)
                        .map(|c| c.handle.driver_tx().clone())
                        .unwrap();

                    // Create connection state for the new virtual connection
                    // r[impl core.conn.dispatcher-custom]
                    // Use the dispatcher provided by the initiator
                    let conn_state = VirtualConnectionState::new(
                        conn_id,
                        driver_tx,
                        Role::Acceptor,
                        self.negotiated.initial_credit,
                        state.diagnostic_state.clone(),
                        pending.dispatcher,
                    );
                    let handle = conn_state.handle.clone();
                    state.connections.insert(conn_id, conn_state);
                    let _ = pending.response_tx.send(Ok(handle));
                }
                // Unknown request_id - ignore (may be late/duplicate)
            }
            Message::Reject {
                request_id,
                reason,
                metadata: _,
            } => {
                // Handle rejection of our outgoing Connect request
                let state = match self.peers.get_mut(&peer_id) {
                    Some(s) => s,
                    None => return Ok(()),
                };
                if let Some(pending) = state.pending_connects.remove(&request_id) {
                    let _ = pending
                        .response_tx
                        .send(Err(ConnectError::Rejected(reason)));
                }
                // Unknown request_id - ignore
            }
            Message::Goodbye { conn_id, reason: _ } => {
                let state = match self.peers.get_mut(&peer_id) {
                    Some(s) => s,
                    None => return Ok(()),
                };
                if conn_id.is_root() {
                    // Goodbye on root closes entire link - fail all pending responses for this peer
                    for (_, mut conn) in state.connections.drain() {
                        conn.fail_pending_responses();
                        conn.abort_in_flight_requests();
                    }
                    // Remove the peer
                    info!(
                        "MultiPeerHostDriver: peer {:?} disconnected, removing from registry",
                        peer_id
                    );
                    self.peers.remove(&peer_id);
                    info!(
                        "MultiPeerHostDriver: {} peers remaining after disconnect",
                        self.peers.len()
                    );
                } else {
                    // Close just this virtual connection
                    if let Some(mut conn) = state.connections.remove(&conn_id) {
                        conn.fail_pending_responses();
                        conn.abort_in_flight_requests();
                    }
                }
                return Ok(());
            }
            Message::Request {
                conn_id,
                request_id,
                method_id,
                metadata,
                channels,
                payload,
            } => {
                debug!(
                    request_id,
                    method_id,
                    ?conn_id,
                    channels = ?channels,
                    "MultiPeerHostDriver: received Request with channels"
                );
                self.handle_incoming_request(
                    peer_id, conn_id, request_id, method_id, metadata, channels, payload,
                )
                .await?;
            }
            Message::Response {
                conn_id,
                request_id,
                channels,
                payload,
                ..
            } => {
                // Route to waiting caller on the appropriate connection
                if let Some(state) = self.peers.get_mut(&peer_id)
                    && let Some(conn) = state.connections.get_mut(&conn_id)
                    && let Some(tx) = conn.pending_responses.remove(&request_id)
                {
                    let _ = tx.send(Ok(ResponseData { payload, channels }));
                }
                // Unknown response IDs are ignored per spec
            }
            Message::Cancel {
                conn_id,
                request_id,
            } => {
                // r[impl call.cancel.message] - Cancel requests callee stop processing.
                // r[impl call.cancel.best-effort] - Cancellation is best-effort.
                self.handle_cancel(peer_id, conn_id, request_id).await?;
            }
            Message::Data {
                conn_id,
                channel_id,
                payload,
            } => {
                self.handle_data(peer_id, conn_id, channel_id, payload)
                    .await?;
            }
            Message::Close {
                conn_id,
                channel_id,
            } => {
                self.handle_close(peer_id, conn_id, channel_id).await?;
            }
            Message::Reset {
                conn_id,
                channel_id,
            } => {
                self.handle_reset(peer_id, conn_id, channel_id)?;
            }
            Message::Credit { .. } => {
                return Err(self
                    .goodbye(
                        peer_id,
                        "shm.flow.no-credit-message",
                        "Received Credit message over SHM (not supported)".into(),
                    )
                    .await);
            }
        }
        Ok(())
    }

    /// Handle an incoming request from a peer on a specific connection.
    #[allow(clippy::too_many_arguments)]
    async fn handle_incoming_request(
        &mut self,
        peer_id: PeerId,
        conn_id: ConnectionId,
        request_id: u64,
        method_id: u64,
        metadata: roam_wire::Metadata,
        channels: Vec<u64>,
        payload: Vec<u8>,
    ) -> Result<(), ShmConnectionError> {
        let state = match self.peers.get_mut(&peer_id) {
            Some(s) => s,
            None => return Ok(()), // Peer gone
        };

        let conn = match state.connections.get_mut(&conn_id) {
            Some(c) => c,
            None => {
                return Err(self
                    .goodbye(
                        peer_id,
                        "core.conn.unknown",
                        format!("Request for unknown conn_id={:?}", conn_id),
                    )
                    .await);
            }
        };

        // Duplicate detection
        // r[impl call.request-id.duplicate-detection]
        if conn.in_flight_server_requests.contains_key(&request_id) {
            return Err(self
                .goodbye(
                    peer_id,
                    "call.request-id.duplicate-detection",
                    format!("Duplicate request_id={}", request_id),
                )
                .await);
        }

        let (request_task_id, request_task_name) = task_context_from_metadata(&metadata);
        // Track incoming request for diagnostics
        if let Some(diag) = &state.diagnostic_state {
            trace!(request_id, method_id, name = %diag.name, "recording incoming request");
            diag.record_incoming_request(
                request_id,
                method_id,
                Some(&metadata),
                request_task_id,
                request_task_name,
                None,
            );
        } else {
            trace!(
                request_id,
                method_id, "diagnostic_state is None, not tracking incoming request"
            );
        }

        // Validate metadata
        if let Err(rule_id) = roam_wire::validate_metadata(&metadata) {
            if let Some(diag) = &state.diagnostic_state {
                diag.complete_request(request_id);
            }
            return Err(self
                .goodbye(
                    peer_id,
                    rule_id,
                    format!("Invalid metadata for request_id={}", request_id),
                )
                .await);
        }

        // Validate payload size
        if payload.len() as u32 > self.negotiated.max_payload_size {
            if let Some(state) = self.peers.get_mut(&peer_id)
                && let Some(diag) = &state.diagnostic_state
            {
                diag.complete_request(request_id);
            }
            return Err(self
                .goodbye(
                    peer_id,
                    "flow.call.payload-limit",
                    format!(
                        "Request payload too large: {} bytes (max {}) for request_id={}",
                        payload.len(),
                        self.negotiated.max_payload_size,
                        request_id
                    ),
                )
                .await);
        }

        // Re-borrow for dispatch
        let state = self.peers.get_mut(&peer_id).unwrap();
        let conn = state.connections.get_mut(&conn_id).unwrap();

        // Build context for dispatch
        let cx = Context::new(
            conn_id,
            roam_wire::RequestId::new(request_id),
            roam_wire::MethodId::new(method_id),
            metadata,
            channels,
        );

        // r[impl core.conn.dispatcher] - Use connection-specific dispatcher if available
        let dispatcher: &dyn ServiceDispatcher = if let Some(ref conn_dispatcher) = conn.dispatcher
        {
            conn_dispatcher.as_ref()
        } else {
            state.dispatcher.as_ref()
        };

        // Dispatch - spawn as a task so message loop can continue.
        debug!(
            conn_id = conn_id.raw(),
            request_id, method_id, "dispatching incoming request"
        );
        conn.server_channel_registry
            .set_current_request_id(Some(request_id));
        let handler_fut = dispatcher.dispatch(cx, payload, &mut conn.server_channel_registry);
        conn.server_channel_registry.set_current_request_id(None);

        // r[impl call.cancel.best-effort] - Store abort handle for cancellation support
        let join_handle = peeps_tasks::spawn_tracked("roam_shm_handle_request", handler_fut);
        conn.in_flight_server_requests
            .insert(request_id, join_handle.abort_handle());
        Ok(())
    }

    /// Handle a Cancel message from a peer.
    ///
    /// r[impl call.cancel.message] - Cancel requests callee stop processing.
    /// r[impl call.cancel.best-effort] - Cancellation is best-effort; handler may have completed.
    /// r[impl call.cancel.no-response-required] - We still send a Cancelled response.
    async fn handle_cancel(
        &mut self,
        peer_id: PeerId,
        conn_id: ConnectionId,
        request_id: u64,
    ) -> Result<(), ShmConnectionError> {
        // Get the peer state
        let state = match self.peers.get_mut(&peer_id) {
            Some(s) => s,
            None => {
                // Unknown peer - ignore
                return Ok(());
            }
        };

        // Get the connection
        let conn = match state.connections.get_mut(&conn_id) {
            Some(c) => c,
            None => {
                // Unknown connection - ignore (may have been closed)
                return Ok(());
            }
        };

        // Remove and abort the in-flight request if it exists
        if let Some(abort_handle) = conn.in_flight_server_requests.remove(&request_id) {
            // Abort the handler task (best-effort)
            abort_handle.abort();

            // Mark request completed for diagnostics
            if let Some(diag) = &state.diagnostic_state {
                diag.complete_request(request_id);
            }

            // Send a Cancelled response
            // r[impl call.cancel.best-effort] - The callee MUST still send a Response.
            let wire_msg = Message::Response {
                conn_id,
                request_id,
                metadata: vec![],
                channels: vec![],
                // Cancelled error: Result::Err(1) + RoamError::Cancelled(3)
                payload: vec![1, 3],
            };
            self.send_to_peer(peer_id, &wire_msg).await?;
        }
        // If request not found, it already completed - nothing to do

        Ok(())
    }

    /// Handle incoming Data message.
    async fn handle_data(
        &mut self,
        peer_id: PeerId,
        conn_id: ConnectionId,
        channel_id: u64,
        payload: Vec<u8>,
    ) -> Result<(), ShmConnectionError> {
        if channel_id == 0 {
            return Err(self
                .goodbye(
                    peer_id,
                    "streaming.id.zero-reserved",
                    "Data message with channel_id=0 (reserved)".into(),
                )
                .await);
        }

        if payload.len() as u32 > self.negotiated.max_payload_size {
            return Err(self
                .goodbye(
                    peer_id,
                    "flow.call.payload-limit",
                    format!(
                        "Data payload too large: {} bytes (max {})",
                        payload.len(),
                        self.negotiated.max_payload_size
                    ),
                )
                .await);
        }

        let state = match self.peers.get_mut(&peer_id) {
            Some(s) => s,
            None => return Ok(()),
        };

        let conn = match state.connections.get_mut(&conn_id) {
            Some(c) => c,
            None => {
                return Err(self
                    .goodbye(
                        peer_id,
                        "core.conn.unknown",
                        format!("Data for unknown conn_id={:?}", conn_id),
                    )
                    .await);
            }
        };

        // Try server registry first, then client handle
        let in_server = conn.server_channel_registry.contains_incoming(channel_id);
        let in_client = conn.handle.contains_channel(channel_id);
        trace!(
            channel_id,
            in_server, in_client, "handle_data: checking channel registries"
        );

        let result = if in_server {
            conn.server_channel_registry
                .route_data(channel_id, payload.clone())
                .await
        } else if in_client {
            conn.handle.route_data(channel_id, payload.clone()).await
        } else {
            warn!(
                channel_id,
                "handle_data: channel not found in either registry"
            );
            Err(ChannelError::Unknown)
        };

        match result {
            Ok(()) => Ok(()),
            Err(ChannelError::Unknown) => {
                Err(self
                    .goodbye(
                        peer_id,
                        "streaming.unknown",
                        format!(
                            "Data for unknown channel_id={} (in_server={}, in_client={}, payload_len={})",
                            channel_id, in_server, in_client, payload.len()
                        ),
                    )
                    .await)
            }
            Err(ChannelError::DataAfterClose) => {
                Err(self
                    .goodbye(
                        peer_id,
                        "streaming.data-after-close",
                        format!("Data after close on channel_id={}", channel_id),
                    )
                    .await)
            }
            Err(ChannelError::CreditOverrun) => {
                Err(self
                    .goodbye(
                        peer_id,
                        "flow.stream.credit-overrun",
                        format!("Credit overrun on channel_id={}", channel_id),
                    )
                    .await)
            }
        }
    }

    /// Handle incoming Close message.
    async fn handle_close(
        &mut self,
        peer_id: PeerId,
        conn_id: ConnectionId,
        channel_id: u64,
    ) -> Result<(), ShmConnectionError> {
        if channel_id == 0 {
            return Err(self
                .goodbye(
                    peer_id,
                    "streaming.id.zero-reserved",
                    "Close for reserved channel_id=0".to_string(),
                )
                .await);
        }

        let state = match self.peers.get_mut(&peer_id) {
            Some(s) => s,
            None => return Ok(()),
        };

        let conn = match state.connections.get_mut(&conn_id) {
            Some(c) => c,
            None => {
                return Err(self
                    .goodbye(
                        peer_id,
                        "core.conn.unknown",
                        format!("Close for unknown conn_id={:?}", conn_id),
                    )
                    .await);
            }
        };

        let in_server = conn.server_channel_registry.contains(channel_id);
        let in_client = conn.handle.contains_channel(channel_id);

        if in_server {
            conn.server_channel_registry.close(channel_id);
        } else if in_client {
            conn.handle.close_channel(channel_id);
        } else {
            return Err(self
                .goodbye(
                    peer_id,
                    "streaming.unknown",
                    format!(
                        "Close for unknown channel_id={} (in_server={}, in_client={})",
                        channel_id, in_server, in_client
                    ),
                )
                .await);
        }
        Ok(())
    }

    /// Handle incoming Reset message.
    fn handle_reset(
        &mut self,
        peer_id: PeerId,
        conn_id: ConnectionId,
        channel_id: u64,
    ) -> Result<(), ShmConnectionError> {
        let state = match self.peers.get_mut(&peer_id) {
            Some(s) => s,
            None => return Ok(()),
        };

        let conn = match state.connections.get_mut(&conn_id) {
            Some(c) => c,
            None => return Ok(()), // Reset on unknown connection is silently ignored
        };

        if conn.server_channel_registry.contains(channel_id) {
            conn.server_channel_registry.reset(channel_id);
        } else if conn.handle.contains_channel(channel_id) {
            conn.handle.reset_channel(channel_id);
        }
        Ok(())
    }

    /// Try to send a message to a specific peer.
    /// Returns `Ok(true)` if sent, `Ok(false)` if backpressure (should queue), `Err` for fatal errors.
    async fn try_send_to_peer(
        &mut self,
        peer_id: PeerId,
        msg: &Message,
    ) -> Result<bool, ShmConnectionError> {
        let shm_msg = message_to_shm_msg(msg).map_err(|e| {
            ShmConnectionError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        })?;

        match self.host.send(peer_id, &shm_msg) {
            Ok(()) => {
                // Ring doorbell to wake up guest waiting for messages
                if let Some(doorbell) = self.doorbells.get(&peer_id) {
                    doorbell.signal().await;
                }
                Ok(true)
            }
            Err(crate::host::SendError::SlotExhausted | crate::host::SendError::RingFull) => {
                // Backpressure - caller should queue and retry later
                Ok(false)
            }
            Err(e) => {
                // Fatal error
                Err(ShmConnectionError::Io(std::io::Error::other(format!(
                    "send error: {:?}",
                    e
                ))))
            }
        }
    }

    /// Send a message to a specific peer, queuing if backpressure.
    ///
    /// IMPORTANT: If there are already pending messages for this peer, we MUST
    /// queue this message too to preserve ordering. Otherwise a Close could
    /// arrive at the peer before pending Data messages.
    async fn send_to_peer(
        &mut self,
        peer_id: PeerId,
        msg: &Message,
    ) -> Result<(), ShmConnectionError> {
        // Enforce queue limits to prevent infinite buffering and memory exhaustion.
        // If the queue is full, we drop the message and perform a clean abort if possible.
        // This corresponds to "Clean Abort" strategy for slot exhaustion.
        let check_queue_and_push = |driver: &mut MultiPeerHostDriver,
                                    msg: &Message|
         -> Result<(), ShmConnectionError> {
            let pending = driver.pending_sends.entry(peer_id);
            if pending.len() >= pending.capacity() {
                match msg {
                    Message::Data {
                        conn_id,
                        channel_id,
                        ..
                    } => {
                        warn!(
                            "Backpressure queue full ({}) for peer {:?}, checking aborting stream {}/{}",
                            pending.len(),
                            peer_id,
                            conn_id,
                            channel_id
                        );
                        // Send Reset to cleanly abort the stream on the peer side
                        pending.push_back(Message::Reset {
                            conn_id: *conn_id,
                            channel_id: *channel_id,
                        });
                        return Ok(());
                    }
                    _ => {
                        warn!(
                            "Backpressure queue full ({}) for peer {:?}, dropping message {:?}",
                            pending.len(),
                            peer_id,
                            msg_type_name(msg)
                        );
                        // For other messages, we just drop them to avoid growing the queue forever.
                        // Ideally we'd notify the caller, but send_to_peer is often fire-and-forget.
                        return Ok(());
                    }
                }
            }
            pending.push_back(msg.clone());
            Ok(())
        };

        // If there are pending messages, queue this one too to preserve ordering
        if self.pending_sends.has_pending(&peer_id) {
            trace!(
                "send_to_peer: peer {:?} has pending messages, queuing to preserve order",
                peer_id
            );
            return check_queue_and_push(self, msg);
        }

        match self.try_send_to_peer(peer_id, msg).await {
            Ok(true) => Ok(()),
            Ok(false) => {
                // Backpressure - queue for later
                trace!(
                    "send_to_peer: backpressure for peer {:?}, queuing message",
                    peer_id
                );
                check_queue_and_push(self, msg)
            }
            Err(e) => Err(e),
        }
    }

    /// Retry sending pending messages for a peer after backpressure clears.
    /// Returns the number of messages successfully sent.
    async fn retry_pending_sends(&mut self, peer_id: PeerId) -> usize {
        let mut sent = 0;

        loop {
            // Take the front message from the queue (if any)
            let msg = {
                let Some(queue) = self.pending_sends.get_mut(&peer_id) else {
                    break;
                };
                match queue.pop_front() {
                    Some(m) => m,
                    None => break,
                }
            };

            // Try to send it
            match self.try_send_to_peer(peer_id, &msg).await {
                Ok(true) => {
                    // Sent successfully
                    sent += 1;
                }
                Ok(false) => {
                    // Still backpressured - put it back at the front and stop
                    self.pending_sends.entry(peer_id).push_front(msg);
                    break;
                }
                Err(e) => {
                    // Fatal error - log and drop message
                    warn!(
                        "retry_pending_sends: error sending to peer {:?}: {:?}",
                        peer_id, e
                    );
                    // Don't put it back - continue to next message
                }
            }
        }

        // Clean up empty queue
        if let Some(queue) = self.pending_sends.get(&peer_id)
            && queue.is_empty()
        {
            self.pending_sends.remove(&peer_id);
        }

        if sent > 0 {
            trace!(
                "retry_pending_sends: sent {} pending messages to peer {:?}",
                sent, peer_id
            );
        }

        sent
    }

    /// Retry sending pending messages for ALL peers.
    /// Called when any peer rings the doorbell, since that peer consuming messages
    /// doesn't help other peers whose pending_sends are stuck.
    async fn retry_all_pending_sends(&mut self) {
        // Collect keys first to avoid borrow issues
        let peer_ids: Vec<PeerId> = self.pending_sends.keys().cloned().collect();
        for peer_id in peer_ids {
            self.retry_pending_sends(peer_id).await;
        }
    }

    /// Send Goodbye to a peer and return error.
    async fn goodbye(
        &mut self,
        peer_id: PeerId,
        rule_id: &'static str,
        context: String,
    ) -> ShmConnectionError {
        // Fail all pending responses for this peer (across all connections)
        if let Some(state) = self.peers.get_mut(&peer_id) {
            for conn in state.connections.values_mut() {
                for (_, tx) in conn.pending_responses.drain() {
                    let _ = tx.send(Err(TransportError::ConnectionClosed));
                }
            }
        }

        let _ = self
            .send_to_peer(
                peer_id,
                &Message::Goodbye {
                    conn_id: roam_wire::ConnectionId::ROOT,
                    reason: rule_id.into(),
                },
            )
            .await;

        ShmConnectionError::ProtocolViolation {
            rule_id,
            context: format!("peer {:?}: {}", peer_id, context),
        }
    }
}

impl MultiPeerHostDriverHandle {
    /// Create a new peer slot and get a spawn ticket (true lazy spawning).
    ///
    /// This calls `host.add_peer()` on the driver's owned host, enabling
    /// dynamic peer creation at runtime without needing to pre-allocate all
    /// slots before building the driver.
    ///
    /// # Returns
    ///
    /// A `SpawnTicket` that can be used to spawn the process later.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Driver is running (owns the host)
    /// let (driver, handles, driver_handle) = build_driver(host);
    /// tokio::spawn(driver.run());
    ///
    /// // Later, on first access (true lazy spawning):
    /// let ticket = driver_handle.create_peer(AddPeerOptions::default()).await?;
    /// ticket.spawn(Command::new("my-cell"))?;
    /// let handle = driver_handle.add_peer(ticket.peer_id, MyDispatcher).await?;
    /// ```
    pub async fn create_peer(
        &self,
        options: crate::spawn::AddPeerOptions,
    ) -> Result<crate::spawn::SpawnTicket, ShmConnectionError> {
        let (response_tx, response_rx) = peeps_sync::oneshot_channel("shm_conn_response");

        let cmd = ControlCommand::Create {
            options,
            response: response_tx,
        };

        self.control_tx
            .send(cmd)
            .await
            .map_err(|_| ShmConnectionError::Io(std::io::Error::other("driver has shut down")))?;

        response_rx
            .recv()
            .await
            .map_err(|_| {
                ShmConnectionError::Io(std::io::Error::other("driver failed to create peer"))
            })?
            .map_err(ShmConnectionError::Io)
    }

    /// Register a peer dynamically with the running driver.
    ///
    /// This adds the peer's dispatcher so the driver can handle incoming
    /// requests from this peer. Call this after spawning the process with
    /// a ticket from `create_peer()`.
    ///
    /// # Arguments
    ///
    /// * `peer_id` - The peer ID (from SpawnTicket)
    /// * `dispatcher` - The service dispatcher for handling incoming requests
    ///
    /// # Returns
    ///
    /// A `ConnectionHandle` for making RPC calls to this peer.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // After calling create_peer and spawning:
    /// let (handle, incoming) = driver_handle.add_peer(ticket.peer_id, MyDispatcher).await?;
    /// ```
    pub async fn add_peer<D>(
        &self,
        peer_id: PeerId,
        dispatcher: D,
    ) -> Result<(ConnectionHandle, IncomingConnections), ShmConnectionError>
    where
        D: ServiceDispatcher + 'static,
    {
        self.add_peer_with_diagnostics(peer_id, dispatcher, None)
            .await
    }

    /// Register a peer after it's ready, with optional diagnostic state for diagnostics dumps.
    ///
    /// Same as [`Self::add_peer`] but allows passing a [`DiagnosticState`] to track
    /// in-flight requests for debugging.
    ///
    /// Returns a tuple of (ConnectionHandle, IncomingConnections) where IncomingConnections
    /// is a receiver for incoming virtual connection requests from this peer.
    pub async fn add_peer_with_diagnostics<D>(
        &self,
        peer_id: PeerId,
        dispatcher: D,
        diagnostic_state: Option<Arc<DiagnosticState>>,
    ) -> Result<(ConnectionHandle, IncomingConnections), ShmConnectionError>
    where
        D: ServiceDispatcher + 'static,
    {
        let (response_tx, response_rx) = peeps_sync::oneshot_channel("shm_conn_response");

        let cmd = ControlCommand::Add {
            peer_id,
            dispatcher: Box::new(dispatcher),
            diagnostic_state,
            response: response_tx,
        };

        self.control_tx
            .send(cmd)
            .await
            .map_err(|_| ShmConnectionError::Io(std::io::Error::other("driver has shut down")))?;

        response_rx
            .recv()
            .await
            .map_err(|_| ShmConnectionError::Io(std::io::Error::other("driver failed to add peer")))
    }

    /// Release a reserved peer slot.
    ///
    /// Call this when a bootstrap/spawn attempt fails after `create_peer`.
    pub async fn release_peer(&self, peer_id: PeerId) -> Result<(), ShmConnectionError> {
        let (response_tx, response_rx) = peeps_sync::oneshot_channel("shm_conn_response");
        let cmd = ControlCommand::Release {
            peer_id,
            response: response_tx,
        };

        self.control_tx
            .send(cmd)
            .await
            .map_err(|_| ShmConnectionError::Io(std::io::Error::other("driver has shut down")))?;

        response_rx.recv().await.map_err(|_| {
            ShmConnectionError::Io(std::io::Error::other("driver failed to release peer"))
        })
    }
}

/// Establish a multi-peer host driver with homogeneous dispatchers.
///
/// This is a convenience function that creates a `MultiPeerHostDriver` for
/// scenarios where all peers use the same dispatcher type. For heterogeneous
/// dispatchers (different types per peer), use `MultiPeerHostDriver::builder()`
/// directly with the builder pattern.
///
/// # Arguments
///
/// * `host` - The SHM host (takes ownership)
/// * `peers` - Iterator of (PeerId, Dispatcher) pairs
///
/// # Returns
///
/// A tuple of (driver, handles) where handles is a map from PeerId to ConnectionHandle.
///
/// # Example
///
/// ```ignore
/// let host = ShmHost::create("/dev/shm/myapp", config)?;
/// let ticket1 = host.add_peer(options)?;
/// let ticket2 = host.add_peer(options)?;
///
/// // Homogeneous dispatchers (same type for all peers)
/// let (driver, handles) = establish_multi_peer_host(
///     host,
///     vec![
///         (ticket1.peer_id(), dispatcher1),
///         (ticket2.peer_id(), dispatcher2),
///     ],
/// );
///
/// tokio::spawn(driver.run());
///
/// // Make calls to specific peers
/// let handle1 = handles.get(&ticket1.peer_id()).unwrap();
/// ```
pub fn establish_multi_peer_host<D, I>(
    host: ShmHost,
    peers: I,
) -> (
    MultiPeerHostDriver,
    HashMap<PeerId, ConnectionHandle>,
    HashMap<PeerId, IncomingConnections>,
    MultiPeerHostDriverHandle,
)
where
    D: ServiceDispatcher + 'static,
    I: IntoIterator<Item = (PeerId, D)>,
{
    let mut builder = MultiPeerHostDriver::builder(host);
    for (peer_id, dispatcher) in peers {
        builder = builder.add_peer(peer_id, dispatcher);
    }
    builder.build()
}
