//! Bidirectional connection driver for SHM transport.
//!
//! This module provides the equivalent of `roam_stream::Driver` for SHM:
//! - Dispatches incoming requests to a service
//! - Routes incoming responses to waiting callers
//! - Sends outgoing requests from ConnectionHandle
//! - Handles stream data (Data/Close/Reset)
//!
//! Key differences from stream transport:
//! - No Hello exchange (config read from segment header)
//! - No Credit messages (flow control via channel table atomics)
//!
//! shm[impl shm.handshake]
//! shm[impl shm.flow.no-credit-message]

use std::collections::HashMap;
use std::sync::Arc;

use roam_session::{
    ChannelError, ChannelRegistry, ConnectionHandle, DriverMessage, ResponseData, Role,
    ServiceDispatcher, TransportError,
};
use roam_stream::MessageTransport;
use roam_wire::Message;
use tokio::sync::{mpsc, oneshot};

use crate::auditable::{self, AuditableDequeMap, AuditableReceiver, AuditableSender};
use crate::host::ShmHost;
use crate::peer::PeerId;
use crate::transport::{ShmGuestTransport, frame_to_message, message_to_frame};

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

/// The SHM connection driver - a future that handles bidirectional RPC.
///
/// This must be spawned or awaited to drive the connection forward.
/// Use [`ConnectionHandle`] to make outgoing calls.
///
/// The type parameter `T` is the transport type (e.g., `ShmGuestTransport`).
pub struct ShmDriver<T, D> {
    io: T,
    dispatcher: D,
    #[allow(dead_code)]
    role: Role,
    negotiated: ShmNegotiated,

    /// Handle for client-side operations (streams, etc.)
    handle: ConnectionHandle,

    /// Unified channel for all messages (Call/Data/Close/Response).
    /// Single channel ensures FIFO ordering.
    driver_rx: mpsc::Receiver<DriverMessage>,

    /// Server-side stream registry (for incoming Tx/Rx from requests we serve).
    server_channel_registry: ChannelRegistry,

    /// Pending responses for outgoing calls we made.
    /// request_id → oneshot sender for the response.
    pending_responses: HashMap<u64, oneshot::Sender<Result<ResponseData, TransportError>>>,

    /// In-flight requests we're serving (to detect duplicates).
    in_flight_server_requests: std::collections::HashSet<u64>,

    /// Diagnostic state for tracking in-flight requests (for SIGUSR1 dumps).
    diagnostic_state: Option<Arc<roam_session::diagnostic::DiagnosticState>>,
}

impl<T, D> ShmDriver<T, D>
where
    T: MessageTransport,
    D: ServiceDispatcher,
{
    /// Create a new SHM driver with the given transport, dispatcher, and parameters.
    pub fn new(
        io: T,
        dispatcher: D,
        role: Role,
        negotiated: ShmNegotiated,
        handle: ConnectionHandle,
        driver_tx: mpsc::Sender<DriverMessage>,
        driver_rx: mpsc::Receiver<DriverMessage>,
        diagnostic_state: Option<Arc<roam_session::diagnostic::DiagnosticState>>,
    ) -> Self {
        // Use infinite credit for now - proper SHM flow control via channel table
        // atomics will be implemented in a future phase. This matches the current
        // behavior of stream transports which also use infinite credit.
        Self {
            io,
            dispatcher,
            role,
            negotiated,
            handle,
            driver_rx,
            server_channel_registry: ChannelRegistry::new(driver_tx),
            pending_responses: HashMap::new(),
            in_flight_server_requests: std::collections::HashSet::new(),
            diagnostic_state,
        }
    }

    /// Run the driver until the connection closes.
    pub async fn run(mut self) -> Result<(), ShmConnectionError> {
        loop {
            trace!("driver: starting select loop");
            tokio::select! {
                biased;

                // Handle all driver messages (Call/Data/Close/Response).
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

    /// Handle a driver message (Call/Data/Close/Response).
    async fn handle_driver_message(
        &mut self,
        msg: DriverMessage,
    ) -> Result<(), ShmConnectionError> {
        let wire_msg = match msg {
            DriverMessage::Call {
                request_id,
                method_id,
                metadata,
                channels,
                payload,
                response_tx,
            } => {
                trace!("handle_driver_message: Call req={}", request_id);
                // Store the response channel
                self.pending_responses.insert(request_id, response_tx);

                // Send the request
                Message::Request {
                    request_id,
                    method_id,
                    metadata,
                    channels,
                    payload,
                }
            }
            DriverMessage::Data {
                channel_id,
                payload,
            } => {
                trace!(
                    "handle_driver_message: Data ch={}, {} bytes",
                    channel_id,
                    payload.len()
                );
                Message::Data {
                    channel_id,
                    payload,
                }
            }
            DriverMessage::Close { channel_id } => {
                trace!("handle_driver_message: Close ch={}", channel_id);
                Message::Close { channel_id }
            }
            DriverMessage::Response {
                request_id,
                channels,
                payload,
            } => {
                // Only send if this request is still in-flight
                if !self.in_flight_server_requests.remove(&request_id) {
                    // Request was cancelled or already completed, skip
                    return Ok(());
                }
                // Mark request completed for diagnostics
                if let Some(diag) = &self.diagnostic_state {
                    trace!(request_id, name = %diag.name, "completing incoming request");
                    diag.complete_request(request_id);
                }
                Message::Response {
                    request_id,
                    metadata: Vec::new(),
                    channels,
                    payload,
                }
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
            Message::Goodbye { .. } => {
                // Fail all pending responses
                for (_, tx) in self.pending_responses.drain() {
                    let _ = tx.send(Err(TransportError::ConnectionClosed));
                }
                return Err(ShmConnectionError::Closed);
            }
            Message::Request {
                request_id,
                method_id,
                metadata,
                channels,
                payload,
            } => {
                debug!(
                    request_id,
                    method_id,
                    channels = ?channels,
                    "ShmDriver: received Request with channels"
                );
                self.handle_incoming_request(request_id, method_id, metadata, channels, payload)
                    .await?;
            }
            Message::Response {
                request_id,
                metadata: _,
                channels,
                payload,
            } => {
                // Route to waiting caller
                if let Some(tx) = self.pending_responses.remove(&request_id) {
                    let _ = tx.send(Ok(ResponseData { payload, channels }));
                }
                // Unknown response IDs are ignored per spec
            }
            Message::Cancel { request_id: _ } => {
                // TODO: Implement cancellation
            }
            Message::Data {
                channel_id,
                payload,
            } => {
                self.handle_data(channel_id, payload).await?;
            }
            Message::Close { channel_id } => {
                self.handle_close(channel_id).await?;
            }
            Message::Reset { channel_id } => {
                self.handle_reset(channel_id)?;
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
        request_id: u64,
        method_id: u64,
        metadata: Vec<(String, roam_wire::MetadataValue)>,
        channels: Vec<u64>,
        payload: Vec<u8>,
    ) -> Result<(), ShmConnectionError> {
        // Duplicate detection
        if !self.in_flight_server_requests.insert(request_id) {
            return Err(self
                .goodbye(
                    "call.request-id.duplicate-detection",
                    format!("Duplicate request_id={}", request_id),
                )
                .await);
        }

        // Track incoming request for diagnostics
        if let Some(diag) = &self.diagnostic_state {
            trace!(request_id, method_id, name = %diag.name, "recording incoming request");
            diag.record_incoming_request(request_id, method_id, None);
        }

        // Validate metadata
        if let Err(rule_id) = roam_wire::validate_metadata(&metadata) {
            self.in_flight_server_requests.remove(&request_id);
            if let Some(diag) = &self.diagnostic_state {
                diag.complete_request(request_id);
            }
            return Err(self
                .goodbye(rule_id, format!("Invalid metadata for request_id={}", request_id))
                .await);
        }

        // Validate payload size
        if payload.len() as u32 > self.negotiated.max_payload_size {
            self.in_flight_server_requests.remove(&request_id);
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

        // Dispatch - spawn as a task so message loop can continue.
        let handler_fut = self.dispatcher.dispatch(
            method_id,
            payload,
            channels,
            request_id,
            &mut self.server_channel_registry,
        );
        tokio::spawn(handler_fut);
        Ok(())
    }

    /// Handle incoming Data message.
    async fn handle_data(
        &mut self,
        channel_id: u64,
        payload: Vec<u8>,
    ) -> Result<(), ShmConnectionError> {
        trace!(
            "handle_data called for channel {}, {} bytes",
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

        // Try server registry first, then client registry
        let in_server = self.server_channel_registry.contains_incoming(channel_id);
        let in_client = self.handle.contains_channel(channel_id);
        let payload_len = payload.len();

        let result = if in_server {
            trace!("routing to server_channel_registry");
            let res = self
                .server_channel_registry
                .route_data(channel_id, payload)
                .await;
            trace!("server_channel_registry.route_data returned {:?}", res);
            res
        } else if in_client {
            trace!("routing to client handle");
            self.handle.route_data(channel_id, payload).await
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
    async fn handle_close(&mut self, channel_id: u64) -> Result<(), ShmConnectionError> {
        if channel_id == 0 {
            return Err(self
                .goodbye(
                    "streaming.id.zero-reserved",
                    "Close message with channel_id=0 (reserved)".into(),
                )
                .await);
        }

        // Try server registry first, then client registry
        let in_server = self.server_channel_registry.contains(channel_id);
        let in_client = self.handle.contains_channel(channel_id);

        if in_server {
            self.server_channel_registry.close(channel_id);
        } else if in_client {
            self.handle.close_channel(channel_id);
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
    fn handle_reset(&mut self, channel_id: u64) -> Result<(), ShmConnectionError> {
        // Try both registries - Reset on unknown stream is not an error
        if self.server_channel_registry.contains(channel_id) {
            self.server_channel_registry.reset(channel_id);
        } else if self.handle.contains_channel(channel_id) {
            self.handle.reset_channel(channel_id);
        }
        // Unknown stream for Reset is ignored per spec
        Ok(())
    }

    /// Send Goodbye and return error.
    async fn goodbye(&mut self, rule_id: &'static str, context: String) -> ShmConnectionError {
        // Fail all pending responses
        for (_, tx) in self.pending_responses.drain() {
            let _ = tx.send(Err(TransportError::ConnectionClosed));
        }

        if let Err(_e) = MessageTransport::send(
            &mut self.io,
            &Message::Goodbye {
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
/// Returns a handle for making calls and a driver future that must be spawned.
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
/// let (handle, driver) = establish_guest(transport, dispatcher);
/// tokio::spawn(driver.run());
/// // Use handle to make calls
/// ```
pub fn establish_guest<D>(
    transport: ShmGuestTransport,
    dispatcher: D,
) -> (ConnectionHandle, ShmDriver<ShmGuestTransport, D>)
where
    D: ServiceDispatcher,
{
    establish_guest_with_diagnostics(transport, dispatcher, None)
}

/// Create a guest connection with optional diagnostic state for SIGUSR1 dumps.
///
/// Same as [`establish_guest`] but allows passing a [`roam_session::diagnostic::DiagnosticState`]
/// for tracking in-flight requests and channels.
pub fn establish_guest_with_diagnostics<D>(
    transport: ShmGuestTransport,
    dispatcher: D,
    diagnostic_state: Option<Arc<roam_session::diagnostic::DiagnosticState>>,
) -> (ConnectionHandle, ShmDriver<ShmGuestTransport, D>)
where
    D: ServiceDispatcher,
{
    // Get config from segment header (already read during attach)
    let config = transport.config();
    let negotiated = ShmNegotiated {
        max_payload_size: config.max_payload_size,
        initial_credit: config.initial_credit,
    };

    // Create single unified channel for all messages (Call/Data/Close/Response).
    // Single channel ensures FIFO ordering.
    let (driver_tx, driver_rx) = mpsc::channel(256);

    // Guest is initiator (uses odd stream IDs)
    // Use infinite credit for now (matches current roam-stream behavior).
    let initial_credit = u32::MAX;
    let handle = ConnectionHandle::new_with_diagnostics(
        driver_tx.clone(),
        Role::Initiator,
        initial_credit,
        diagnostic_state.clone(),
    );

    let driver = ShmDriver::new(
        transport,
        dispatcher,
        Role::Initiator,
        negotiated,
        handle.clone(),
        driver_tx,
        driver_rx,
        diagnostic_state,
    );

    (handle, driver)
}

// ============================================================================
// Multi-Peer Host Driver
// ============================================================================

/// Per-peer state for the multi-peer host driver.
///
/// Uses `Box<dyn ServiceDispatcher>` to allow each peer to have a different
/// dispatcher type, enabling heterogeneous bidirectional RPC scenarios.
struct PeerConnectionState {
    /// Dispatcher for handling incoming requests from this peer.
    /// Boxed to allow different dispatcher types per peer.
    dispatcher: Box<dyn ServiceDispatcher>,

    /// Server-side stream registry for this peer.
    server_channel_registry: ChannelRegistry,

    /// Pending responses for outgoing calls we made to this peer.
    pending_responses: HashMap<u64, oneshot::Sender<Result<ResponseData, TransportError>>>,

    /// In-flight requests we're serving for this peer.
    in_flight_server_requests: std::collections::HashSet<u64>,

    /// The connection handle (kept for stream routing).
    handle: ConnectionHandle,

    /// Diagnostic state for tracking in-flight requests (for SIGUSR1 dumps).
    diagnostic_state: Option<Arc<roam_session::diagnostic::DiagnosticState>>,
}

/// Command to control the multi-peer host driver.
enum ControlCommand {
    /// Create a new peer slot and return a spawn ticket (calls host.add_peer()).
    CreatePeer {
        options: crate::spawn::AddPeerOptions,
        response: oneshot::Sender<Result<crate::spawn::SpawnTicket, std::io::Error>>,
    },
    /// Register a peer dynamically with a dispatcher.
    AddPeer {
        peer_id: PeerId,
        dispatcher: Box<dyn ServiceDispatcher>,
        diagnostic_state: Option<Arc<roam_session::diagnostic::DiagnosticState>>,
        response: oneshot::Sender<ConnectionHandle>,
    },
}

/// Multi-peer host driver for hub topology.
///
/// Unlike `ShmDriver` which handles a single peer, this driver manages
/// multiple peers over a single `ShmHost`. Each peer gets its own
/// `ConnectionHandle` for making RPC calls.
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
/// let (driver, handles) = MultiPeerHostDriver::builder(host)
///     // Simple peer only needs lifecycle dispatcher
///     .add_peer(ticket1.peer_id(), CellLifecycleDispatcher::new(lifecycle.clone()))
///     // Complex peer needs routed dispatcher for bidirectional RPC
///     // The primary dispatcher's method_ids() determines routing; fallback handles the rest
///     .add_peer(ticket2.peer_id(), RoutedDispatcher::new(
///         CellLifecycleDispatcher::new(lifecycle.clone()),  // primary: handles lifecycle methods
///         TemplateHostDispatcher::new(template_host),       // fallback: handles everything else
///     ))
///     .build();
///
/// // Spawn the driver
/// let driver_handle = driver.handle();
/// tokio::spawn(driver.run());
///
/// // Add more peers dynamically (lazy spawning)
/// let ticket3 = host.add_peer(options3)?;
/// let handle3 = driver_handle.add_peer(ticket3.peer_id(), MyDispatcher).await?;
///
/// // Use handles to make calls to specific peers
/// let client1 = MyServiceClient::new(handles[&ticket1.peer_id()].clone());
/// client1.do_thing().await?;
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

    /// Pending outbound messages waiting for backpressure to clear.
    /// When host slots are exhausted, messages are queued here and retried
    /// when the guest rings the doorbell (indicating it has consumed messages).
    pending_sends: AuditableDequeMap<PeerId, Message>,
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
    peers: Vec<(PeerId, Box<dyn ServiceDispatcher>)>,
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
        self.peers.push((peer_id, Box::new(dispatcher)));
        self
    }

    /// Build the driver and return connection handles for each peer and a driver handle.
    pub fn build(
        mut self,
    ) -> (
        MultiPeerHostDriver,
        HashMap<PeerId, ConnectionHandle>,
        MultiPeerHostDriverHandle,
    ) {
        let config = self.host.config();
        let negotiated = ShmNegotiated {
            max_payload_size: config.max_payload_size,
            initial_credit: config.initial_credit,
        };

        let mut peers = HashMap::new();
        let mut handles = HashMap::new();
        let mut doorbells = HashMap::new();

        // Create ring channel for doorbell notifications (bounded, auditable)
        let (ring_tx, ring_rx) = auditable::channel("ring_notifications", 256);

        // Create unified channel for driver messages from all peers (bounded, auditable)
        let (driver_msg_tx, driver_msg_rx) = auditable::channel("driver_messages", 1024);

        for (peer_id, dispatcher) in self.peers {
            // Create single unified channel for all messages (Call/Data/Close/Response).
            // Single channel ensures FIFO ordering.
            let (driver_tx, mut driver_rx) = mpsc::channel(256);

            // Host is acceptor (uses even stream IDs)
            let initial_credit = u32::MAX;
            let handle = ConnectionHandle::new(driver_tx.clone(), Role::Acceptor, initial_credit);

            handles.insert(peer_id, handle.clone());

            peers.insert(
                peer_id,
                PeerConnectionState {
                    dispatcher,
                    server_channel_registry: ChannelRegistry::new(driver_tx),
                    pending_responses: HashMap::new(),
                    in_flight_server_requests: std::collections::HashSet::new(),
                    handle,
                    diagnostic_state: None,
                },
            );

            // Spawn forwarder task for this peer's driver messages
            let driver_msg_tx_clone = driver_msg_tx.clone();
            tokio::spawn(async move {
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
                tokio::spawn(async move {
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
            pending_sends: AuditableDequeMap::new("pending_sends[", 1024),
        };

        let driver_handle = MultiPeerHostDriverHandle { control_tx };

        (driver, handles, driver_handle)
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
                    self.handle_control_command(cmd);
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

                    for (pid, frame) in result.messages {
                        self.last_decoded = frame.payload_bytes().to_vec();

                        let msg = match frame_to_message(frame).map_err(|e| {
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

                    // Retry pending sends for this peer - guest may have freed slots by consuming messages
                    // shm[impl shm.backpressure.host-to-guest]
                    self.retry_pending_sends(peer_id).await;
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

    /// Handle a control command.
    fn handle_control_command(&mut self, cmd: ControlCommand) {
        match cmd {
            ControlCommand::CreatePeer { options, response } => {
                // Call host.add_peer() to create a spawn ticket
                let result = self.host.add_peer(options);
                let _ = response.send(result);
            }
            ControlCommand::AddPeer {
                peer_id,
                dispatcher,
                diagnostic_state,
                response,
            } => {
                trace!("MultiPeerHostDriver: adding peer {:?} dynamically", peer_id);
                // Create single unified channel for all messages (Call/Data/Close/Response).
                let (driver_tx, mut driver_rx) = mpsc::channel(256);

                // Host is acceptor (uses even stream IDs)
                let initial_credit = u32::MAX;
                let handle = ConnectionHandle::new_with_diagnostics(
                    driver_tx.clone(),
                    Role::Acceptor,
                    initial_credit,
                    diagnostic_state.clone(),
                );

                self.peers.insert(
                    peer_id,
                    PeerConnectionState {
                        dispatcher,
                        server_channel_registry: ChannelRegistry::new(driver_tx),
                        pending_responses: HashMap::new(),
                        in_flight_server_requests: std::collections::HashSet::new(),
                        handle: handle.clone(),
                        diagnostic_state,
                    },
                );
                trace!("MultiPeerHostDriver: {} peers now active", self.peers.len());

                // Spawn forwarder task for this peer's driver messages
                let driver_msg_tx = self.driver_msg_tx.clone();
                tokio::spawn(async move {
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
                    tokio::spawn(async move {
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
                    let _ = self.ring_tx.send(peer_id);
                } else {
                    error!("AddPeer: NO DOORBELL FOUND for {:?}!", peer_id);
                }

                // Send the handle back to the caller
                let _ = response.send(handle);
            }
        }
    }

    /// Handle a driver message (Call/Data/Close/Response) for a specific peer.
    async fn handle_driver_message(
        &mut self,
        peer_id: PeerId,
        msg: DriverMessage,
    ) -> Result<(), ShmConnectionError> {
        let wire_msg = match msg {
            DriverMessage::Call {
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
                    channels = ?channels,
                    "MultiPeerHostDriver: sending Request with channels"
                );
                // Store the response channel
                if let Some(state) = self.peers.get_mut(&peer_id) {
                    state.pending_responses.insert(request_id, response_tx);
                }

                // Send the request
                Message::Request {
                    request_id,
                    method_id,
                    metadata,
                    channels,
                    payload,
                }
            }
            DriverMessage::Data {
                channel_id,
                payload,
            } => Message::Data {
                channel_id,
                payload,
            },
            DriverMessage::Close { channel_id } => Message::Close { channel_id },
            DriverMessage::Response {
                request_id,
                channels,
                payload,
            } => {
                // Only send if this request is still in-flight
                if let Some(state) = self.peers.get_mut(&peer_id) {
                    if !state.in_flight_server_requests.remove(&request_id) {
                        return Ok(());
                    }
                    // Mark request completed for diagnostics
                    if let Some(diag) = &state.diagnostic_state {
                        trace!(request_id, name = %diag.name, "completing incoming request");
                        diag.complete_request(request_id);
                    }
                }
                Message::Response {
                    request_id,
                    metadata: Vec::new(),
                    channels,
                    payload,
                }
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
            Message::Goodbye { .. } => {
                // Fail all pending responses for this peer
                if let Some(state) = self.peers.get_mut(&peer_id) {
                    for (_, tx) in state.pending_responses.drain() {
                        let _ = tx.send(Err(TransportError::ConnectionClosed));
                    }
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
                return Ok(());
            }
            Message::Request {
                request_id,
                method_id,
                metadata,
                channels,
                payload,
            } => {
                debug!(
                    request_id,
                    method_id,
                    channels = ?channels,
                    "MultiPeerHostDriver: received Request with channels"
                );
                self.handle_incoming_request(
                    peer_id, request_id, method_id, metadata, channels, payload,
                )
                .await?;
            }
            Message::Response {
                request_id,
                metadata: _,
                channels,
                payload,
            } => {
                // Route to waiting caller
                if let Some(state) = self.peers.get_mut(&peer_id)
                    && let Some(tx) = state.pending_responses.remove(&request_id)
                {
                    let _ = tx.send(Ok(ResponseData { payload, channels }));
                }
            }
            Message::Cancel { request_id: _ } => {
                // TODO: Implement cancellation
            }
            Message::Data {
                channel_id,
                payload,
            } => {
                self.handle_data(peer_id, channel_id, payload).await?;
            }
            Message::Close { channel_id } => {
                self.handle_close(peer_id, channel_id).await?;
            }
            Message::Reset { channel_id } => {
                self.handle_reset(peer_id, channel_id)?;
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

    /// Handle an incoming request from a peer.
    async fn handle_incoming_request(
        &mut self,
        peer_id: PeerId,
        request_id: u64,
        method_id: u64,
        metadata: Vec<(String, roam_wire::MetadataValue)>,
        channels: Vec<u64>,
        payload: Vec<u8>,
    ) -> Result<(), ShmConnectionError> {
        let state = match self.peers.get_mut(&peer_id) {
            Some(s) => s,
            None => return Ok(()), // Peer gone
        };

        // Duplicate detection
        if !state.in_flight_server_requests.insert(request_id) {
            return Err(self
                .goodbye(
                    peer_id,
                    "call.request-id.duplicate-detection",
                    format!("Duplicate request_id={}", request_id),
                )
                .await);
        }

        // Track incoming request for diagnostics
        if let Some(diag) = &state.diagnostic_state {
            trace!(request_id, method_id, name = %diag.name, "recording incoming request");
            diag.record_incoming_request(request_id, method_id, None);
        } else {
            trace!(request_id, method_id, "diagnostic_state is None, not tracking incoming request");
        }

        // Validate metadata
        if let Err(rule_id) = roam_wire::validate_metadata(&metadata) {
            state.in_flight_server_requests.remove(&request_id);
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
            state.in_flight_server_requests.remove(&request_id);
            if let Some(diag) = &state.diagnostic_state {
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

        // Dispatch - spawn as a task so message loop can continue.
        debug!(
            method_id,
            request_id,
            channels = ?channels,
            "handle_incoming_request: dispatching with channels"
        );
        let handler_fut = state.dispatcher.dispatch(
            method_id,
            payload,
            channels,
            request_id,
            &mut state.server_channel_registry,
        );
        tokio::spawn(handler_fut);
        Ok(())
    }

    /// Handle incoming Data message.
    async fn handle_data(
        &mut self,
        peer_id: PeerId,
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

        // Try server registry first, then client registry
        let in_server = state.server_channel_registry.contains_incoming(channel_id);
        let in_client = state.handle.contains_channel(channel_id);
        trace!(
            channel_id,
            in_server,
            in_client,
            "handle_data: checking channel registries"
        );

        let result = if in_server {
            state
                .server_channel_registry
                .route_data(channel_id, payload.clone())
                .await
        } else if in_client {
            state.handle.route_data(channel_id, payload.clone()).await
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
        channel_id: u64,
    ) -> Result<(), ShmConnectionError> {
        if channel_id == 0 {
            return Err(self
                .goodbye(
                    peer_id,
                    "streaming.id.zero-reserved",
                    format!("Close for reserved channel_id=0"),
                )
                .await);
        }

        let state = match self.peers.get_mut(&peer_id) {
            Some(s) => s,
            None => return Ok(()),
        };

        let in_server = state.server_channel_registry.contains(channel_id);
        let in_client = state.handle.contains_channel(channel_id);

        if in_server {
            state.server_channel_registry.close(channel_id);
        } else if in_client {
            state.handle.close_channel(channel_id);
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
    fn handle_reset(&mut self, peer_id: PeerId, channel_id: u64) -> Result<(), ShmConnectionError> {
        let state = match self.peers.get_mut(&peer_id) {
            Some(s) => s,
            None => return Ok(()),
        };

        if state.server_channel_registry.contains(channel_id) {
            state.server_channel_registry.reset(channel_id);
        } else if state.handle.contains_channel(channel_id) {
            state.handle.reset_channel(channel_id);
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
        let frame = message_to_frame(msg).map_err(|e| {
            ShmConnectionError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        })?;

        match self.host.send(peer_id, frame) {
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
        // If there are pending messages, queue this one too to preserve ordering
        if self.pending_sends.has_pending(&peer_id) {
            trace!(
                "send_to_peer: peer {:?} has pending messages, queuing to preserve order",
                peer_id
            );
            self.pending_sends.entry(peer_id).push_back(msg.clone());
            return Ok(());
        }

        match self.try_send_to_peer(peer_id, msg).await {
            Ok(true) => Ok(()),
            Ok(false) => {
                // Backpressure - queue for later
                trace!(
                    "send_to_peer: backpressure for peer {:?}, queuing message",
                    peer_id
                );
                self.pending_sends.entry(peer_id).push_back(msg.clone());
                Ok(())
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

    /// Send Goodbye to a peer and return error.
    async fn goodbye(
        &mut self,
        peer_id: PeerId,
        rule_id: &'static str,
        context: String,
    ) -> ShmConnectionError {
        // Fail all pending responses for this peer
        if let Some(state) = self.peers.get_mut(&peer_id) {
            for (_, tx) in state.pending_responses.drain() {
                let _ = tx.send(Err(TransportError::ConnectionClosed));
            }
        }

        let _ = self
            .send_to_peer(
                peer_id,
                &Message::Goodbye {
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
        let (response_tx, response_rx) = oneshot::channel();

        let cmd = ControlCommand::CreatePeer {
            options,
            response: response_tx,
        };

        self.control_tx
            .send(cmd)
            .await
            .map_err(|_| ShmConnectionError::Io(std::io::Error::other("driver has shut down")))?;

        response_rx
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
    /// let handle = driver_handle.add_peer(ticket.peer_id, MyDispatcher).await?;
    /// ```
    pub async fn add_peer<D>(
        &self,
        peer_id: PeerId,
        dispatcher: D,
    ) -> Result<ConnectionHandle, ShmConnectionError>
    where
        D: ServiceDispatcher + 'static,
    {
        self.add_peer_with_diagnostics(peer_id, dispatcher, None)
            .await
    }

    /// Register a peer after it's ready, with optional diagnostic state for SIGUSR1 dumps.
    ///
    /// Same as [`add_peer`] but allows passing a [`DiagnosticState`] to track
    /// in-flight requests for debugging.
    pub async fn add_peer_with_diagnostics<D>(
        &self,
        peer_id: PeerId,
        dispatcher: D,
        diagnostic_state: Option<Arc<roam_session::diagnostic::DiagnosticState>>,
    ) -> Result<ConnectionHandle, ShmConnectionError>
    where
        D: ServiceDispatcher + 'static,
    {
        let (response_tx, response_rx) = oneshot::channel();

        let cmd = ControlCommand::AddPeer {
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
            .await
            .map_err(|_| ShmConnectionError::Io(std::io::Error::other("driver failed to add peer")))
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
