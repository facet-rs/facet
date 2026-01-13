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
use std::time::Duration;

use roam_session::{
    ChannelError, ChannelRegistry, ConnectionHandle, HandleCommand, Role, ServiceDispatcher,
    TaskMessage, TransportError,
};
use roam_stream::MessageTransport;
use roam_wire::Message;
use tokio::sync::{mpsc, oneshot};

use crate::host::ShmHost;
use crate::peer::PeerId;
use crate::transport::{
    OwnedShmHostTransport, ShmGuestTransport, frame_to_message, message_to_frame,
};

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
/// The type parameter `T` is the transport type (e.g., `ShmGuestTransport` or
/// `OwnedShmHostTransport`).
pub struct ShmDriver<T, D> {
    io: T,
    dispatcher: D,
    #[allow(dead_code)]
    role: Role,
    negotiated: ShmNegotiated,

    /// Handle for client-side operations (streams, etc.)
    handle: ConnectionHandle,

    /// Receive commands from ConnectionHandle (outgoing calls).
    command_rx: mpsc::Receiver<HandleCommand>,

    /// Server-side stream registry (for incoming Tx/Rx from requests we serve).
    server_channel_registry: ChannelRegistry,

    /// Pending responses for outgoing calls we made.
    /// request_id → oneshot sender for the response.
    pending_responses: HashMap<u64, oneshot::Sender<Result<Vec<u8>, TransportError>>>,

    /// In-flight requests we're serving (to detect duplicates).
    in_flight_server_requests: std::collections::HashSet<u64>,

    /// Channel for receiving task messages (Data/Close/Response) from spawned handlers.
    /// Using a single channel ensures correct ordering: Data/Close before Response.
    task_rx: mpsc::Receiver<TaskMessage>,
}

impl<T, D> ShmDriver<T, D>
where
    T: MessageTransport,
    D: ServiceDispatcher,
{
    /// Create a new SHM driver with the given transport, dispatcher, and parameters.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        io: T,
        dispatcher: D,
        role: Role,
        negotiated: ShmNegotiated,
        handle: ConnectionHandle,
        command_rx: mpsc::Receiver<HandleCommand>,
        task_tx: mpsc::Sender<TaskMessage>,
        task_rx: mpsc::Receiver<TaskMessage>,
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
            command_rx,
            server_channel_registry: ChannelRegistry::new(task_tx),
            pending_responses: HashMap::new(),
            in_flight_server_requests: std::collections::HashSet::new(),
            task_rx,
        }
    }

    /// Run the driver until the connection closes.
    pub async fn run(mut self) -> Result<(), ShmConnectionError> {
        loop {
            tokio::select! {
                biased;

                // Handle task messages (Data/Close/Response from spawned handlers)
                // Using a single channel ensures correct ordering.
                Some(msg) = self.task_rx.recv() => {
                    self.handle_task_message(msg).await?;
                }

                // Handle commands from ConnectionHandle (client-side outgoing calls)
                Some(cmd) = self.command_rx.recv() => {
                    self.handle_command(cmd).await?;
                }

                // Handle incoming messages from peer
                result = MessageTransport::recv_timeout(&mut self.io, Duration::from_secs(30)) => {
                    match self.handle_recv(result).await {
                        Ok(true) => continue,
                        Ok(false) => return Ok(()), // Clean shutdown
                        Err(e) => return Err(e),
                    }
                }
            }
        }
    }

    /// Handle a task message from a spawned handler.
    async fn handle_task_message(&mut self, msg: TaskMessage) -> Result<(), ShmConnectionError> {
        let wire_msg = match msg {
            TaskMessage::Data {
                channel_id,
                payload,
            } => Message::Data {
                channel_id,
                payload,
            },
            TaskMessage::Close { channel_id } => Message::Close { channel_id },
            TaskMessage::Response {
                request_id,
                payload,
            } => {
                // Only send if this request is still in-flight
                if !self.in_flight_server_requests.remove(&request_id) {
                    // Request was cancelled or already completed, skip
                    return Ok(());
                }
                Message::Response {
                    request_id,
                    metadata: Vec::new(),
                    payload,
                }
            }
        };
        MessageTransport::send(&mut self.io, &wire_msg).await?;
        Ok(())
    }

    /// Handle a command from ConnectionHandle.
    async fn handle_command(&mut self, cmd: HandleCommand) -> Result<(), ShmConnectionError> {
        match cmd {
            HandleCommand::Call {
                request_id,
                method_id,
                metadata,
                payload,
                response_tx,
            } => {
                // Store the response channel
                self.pending_responses.insert(request_id, response_tx);

                // Send the request
                let req = Message::Request {
                    request_id,
                    method_id,
                    metadata,
                    payload,
                };
                MessageTransport::send(&mut self.io, &req).await?;
            }
        }
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
                    return Err(self.goodbye("message.hello.unknown-version").await);
                }
                if !raw.is_empty() && raw[0] >= 9 {
                    return Err(self.goodbye("message.unknown-variant").await);
                }
                if e.kind() == std::io::ErrorKind::InvalidData {
                    return Err(self.goodbye("message.decode-error").await);
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
                return Err(self.goodbye("shm.handshake").await);
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
                payload,
            } => {
                self.handle_incoming_request(request_id, method_id, metadata, payload)
                    .await?;
            }
            Message::Response {
                request_id,
                metadata: _,
                payload,
            } => {
                // Route to waiting caller
                if let Some(tx) = self.pending_responses.remove(&request_id) {
                    let _ = tx.send(Ok(payload));
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
                return Err(self.goodbye("shm.flow.no-credit-message").await);
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
        payload: Vec<u8>,
    ) -> Result<(), ShmConnectionError> {
        // Duplicate detection
        if !self.in_flight_server_requests.insert(request_id) {
            return Err(self.goodbye("call.request-id.duplicate-detection").await);
        }

        // Validate metadata
        if let Err(rule_id) = roam_wire::validate_metadata(&metadata) {
            self.in_flight_server_requests.remove(&request_id);
            return Err(self.goodbye(rule_id).await);
        }

        // Validate payload size
        if payload.len() as u32 > self.negotiated.max_payload_size {
            self.in_flight_server_requests.remove(&request_id);
            return Err(self.goodbye("flow.call.payload-limit").await);
        }

        // Dispatch - spawn as a task so message loop can continue.
        let handler_fut = self.dispatcher.dispatch(
            method_id,
            payload,
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
        if channel_id == 0 {
            return Err(self.goodbye("streaming.id.zero-reserved").await);
        }

        if payload.len() as u32 > self.negotiated.max_payload_size {
            return Err(self.goodbye("flow.call.payload-limit").await);
        }

        // Try server registry first, then client registry
        let result = if self.server_channel_registry.contains_incoming(channel_id) {
            self.server_channel_registry
                .route_data(channel_id, payload)
                .await
        } else if self.handle.contains_channel(channel_id) {
            self.handle.route_data(channel_id, payload).await
        } else {
            Err(ChannelError::Unknown)
        };

        match result {
            Ok(()) => Ok(()),
            Err(ChannelError::Unknown) => Err(self.goodbye("streaming.unknown").await),
            Err(ChannelError::DataAfterClose) => {
                Err(self.goodbye("streaming.data-after-close").await)
            }
            Err(ChannelError::CreditOverrun) => {
                Err(self.goodbye("flow.stream.credit-overrun").await)
            }
        }
    }

    /// Handle incoming Close message.
    async fn handle_close(&mut self, channel_id: u64) -> Result<(), ShmConnectionError> {
        if channel_id == 0 {
            return Err(self.goodbye("streaming.id.zero-reserved").await);
        }

        // Try server registry first, then client registry
        if self.server_channel_registry.contains(channel_id) {
            self.server_channel_registry.close(channel_id);
        } else if self.handle.contains_channel(channel_id) {
            self.handle.close_channel(channel_id);
        } else {
            return Err(self.goodbye("streaming.unknown").await);
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
    async fn goodbye(&mut self, rule_id: &'static str) -> ShmConnectionError {
        // Fail all pending responses
        for (_, tx) in self.pending_responses.drain() {
            let _ = tx.send(Err(TransportError::ConnectionClosed));
        }

        let _ = MessageTransport::send(
            &mut self.io,
            &Message::Goodbye {
                reason: rule_id.into(),
            },
        )
        .await;

        ShmConnectionError::ProtocolViolation {
            rule_id,
            context: format!("SHM protocol violation: {}", rule_id),
        }
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
/// use roam_shm::{ShmGuest, ShmGuestTransport};
/// use roam_shm::driver::establish_guest;
///
/// let guest = ShmGuest::attach_path("/dev/shm/myapp")?;
/// let transport = ShmGuestTransport::new(guest);
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
    // Get config from segment header (already read during attach)
    let config = transport.config();
    let negotiated = ShmNegotiated {
        max_payload_size: config.max_payload_size,
        initial_credit: config.initial_credit,
    };

    let (command_tx, command_rx) = mpsc::channel(64);
    let (task_tx, task_rx) = mpsc::channel(64);

    // Guest is initiator (uses odd stream IDs)
    // Use infinite credit for now (matches current roam-stream behavior).
    let initial_credit = u32::MAX;
    let handle =
        ConnectionHandle::new(command_tx, Role::Initiator, initial_credit, task_tx.clone());

    let driver = ShmDriver::new(
        transport,
        dispatcher,
        Role::Initiator,
        negotiated,
        handle.clone(),
        command_rx,
        task_tx,
        task_rx,
    );

    (handle, driver)
}

/// Establish an SHM connection as the host for a specific peer.
///
/// Returns a handle for making calls and a driver future that must be spawned.
///
/// **Note:** This function takes ownership of the `ShmHost`. For scenarios with
/// multiple guests, you'll need a different architecture (e.g., shared host with
/// per-peer message routing).
///
/// # Arguments
///
/// * `host` - The SHM host (takes ownership)
/// * `peer_id` - The peer ID to communicate with
/// * `dispatcher` - Service dispatcher for handling incoming requests
///
/// # Example
///
/// ```ignore
/// use roam_shm::{ShmHost, SegmentConfig, PeerId};
/// use roam_shm::driver::establish_host_peer;
///
/// let host = ShmHost::create("/dev/shm/myapp", SegmentConfig::default())?;
/// let peer_id = PeerId::new(1).unwrap();
/// let (handle, driver) = establish_host_peer(host, peer_id, dispatcher);
/// tokio::spawn(driver.run());
/// // Use handle to make calls to the guest
/// ```
pub fn establish_host_peer<D>(
    host: ShmHost,
    peer_id: PeerId,
    dispatcher: D,
) -> (ConnectionHandle, ShmDriver<OwnedShmHostTransport, D>)
where
    D: ServiceDispatcher,
{
    let transport = OwnedShmHostTransport::new(host, peer_id);

    // Get config from segment
    let config = transport.config();
    let negotiated = ShmNegotiated {
        max_payload_size: config.max_payload_size,
        initial_credit: config.initial_credit,
    };

    let (command_tx, command_rx) = mpsc::channel(64);
    let (task_tx, task_rx) = mpsc::channel(64);

    // Host is acceptor (uses even stream IDs)
    // Use infinite credit for now (matches current roam-stream behavior).
    let initial_credit = u32::MAX;
    let handle = ConnectionHandle::new(command_tx, Role::Acceptor, initial_credit, task_tx.clone());

    let driver = ShmDriver::new(
        transport,
        dispatcher,
        Role::Acceptor,
        negotiated,
        handle.clone(),
        command_rx,
        task_tx,
        task_rx,
    );

    (handle, driver)
}

// ============================================================================
// Multi-Peer Host Driver
// ============================================================================

/// Per-peer state for the multi-peer host driver.
struct PeerConnectionState<D> {
    /// Dispatcher for handling incoming requests from this peer.
    dispatcher: D,

    /// Channel for receiving commands from the ConnectionHandle.
    command_rx: mpsc::Receiver<HandleCommand>,

    /// Channel for receiving task messages from spawned handlers.
    task_rx: mpsc::Receiver<TaskMessage>,

    /// Server-side stream registry for this peer.
    server_channel_registry: ChannelRegistry,

    /// Pending responses for outgoing calls we made to this peer.
    pending_responses: HashMap<u64, oneshot::Sender<Result<Vec<u8>, TransportError>>>,

    /// In-flight requests we're serving for this peer.
    in_flight_server_requests: std::collections::HashSet<u64>,

    /// The connection handle (kept for stream routing).
    handle: ConnectionHandle,
}

/// Multi-peer host driver for hub topology.
///
/// Unlike `ShmDriver` which handles a single peer, this driver manages
/// multiple peers over a single `ShmHost`. Each peer gets its own
/// `ConnectionHandle` for making RPC calls.
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
/// // Create driver with dispatchers for each peer
/// let (driver, handles) = MultiPeerHostDriver::new(host)
///     .add_peer(ticket1.peer_id(), dispatcher1)
///     .add_peer(ticket2.peer_id(), dispatcher2)
///     .build();
///
/// // Spawn the driver
/// tokio::spawn(driver.run());
///
/// // Use handles to make calls to specific peers
/// let client1 = MyServiceClient::new(handles[&ticket1.peer_id()].clone());
/// client1.do_thing().await?;
/// ```
pub struct MultiPeerHostDriver<D> {
    /// The SHM host (owned).
    host: ShmHost,

    /// Negotiated parameters from segment config.
    negotiated: ShmNegotiated,

    /// Per-peer connection state.
    peers: HashMap<PeerId, PeerConnectionState<D>>,

    /// Buffer for last decoded bytes (for error detection).
    last_decoded: Vec<u8>,
}

/// Builder for `MultiPeerHostDriver`.
pub struct MultiPeerHostDriverBuilder<D> {
    host: ShmHost,
    peers: Vec<(PeerId, D)>,
}

impl<D> MultiPeerHostDriverBuilder<D>
where
    D: ServiceDispatcher,
{
    /// Add a peer with its dispatcher.
    pub fn add_peer(mut self, peer_id: PeerId, dispatcher: D) -> Self {
        self.peers.push((peer_id, dispatcher));
        self
    }

    /// Build the driver and return connection handles for each peer.
    pub fn build(self) -> (MultiPeerHostDriver<D>, HashMap<PeerId, ConnectionHandle>) {
        let config = self.host.config();
        let negotiated = ShmNegotiated {
            max_payload_size: config.max_payload_size,
            initial_credit: config.initial_credit,
        };

        let mut peers = HashMap::new();
        let mut handles = HashMap::new();

        for (peer_id, dispatcher) in self.peers {
            let (command_tx, command_rx) = mpsc::channel(64);
            let (task_tx, task_rx) = mpsc::channel(64);

            // Host is acceptor (uses even stream IDs)
            let initial_credit = u32::MAX;
            let handle =
                ConnectionHandle::new(command_tx, Role::Acceptor, initial_credit, task_tx.clone());

            handles.insert(peer_id, handle.clone());

            peers.insert(
                peer_id,
                PeerConnectionState {
                    dispatcher,
                    command_rx,
                    task_rx,
                    server_channel_registry: ChannelRegistry::new(task_tx),
                    pending_responses: HashMap::new(),
                    in_flight_server_requests: std::collections::HashSet::new(),
                    handle,
                },
            );
        }

        let driver = MultiPeerHostDriver {
            host: self.host,
            negotiated,
            peers,
            last_decoded: Vec::new(),
        };

        (driver, handles)
    }
}

impl<D> MultiPeerHostDriver<D>
where
    D: ServiceDispatcher,
{
    /// Create a new builder for the multi-peer host driver.
    pub fn new(host: ShmHost) -> MultiPeerHostDriverBuilder<D> {
        MultiPeerHostDriverBuilder {
            host,
            peers: Vec::new(),
        }
    }

    /// Run the driver until all peers disconnect or an error occurs.
    pub async fn run(mut self) -> Result<(), ShmConnectionError> {
        loop {
            // Process all pending work in one pass, then yield
            let mut did_work = false;

            // 1. Process task messages from all peers
            let task_msgs: Vec<_> = self
                .peers
                .iter_mut()
                .filter_map(|(peer_id, state)| {
                    state.task_rx.try_recv().ok().map(|msg| (*peer_id, msg))
                })
                .collect();

            for (peer_id, msg) in task_msgs {
                self.handle_task_message(peer_id, msg).await?;
                did_work = true;
            }

            // 2. Process commands from all peers
            let commands: Vec<_> = self
                .peers
                .iter_mut()
                .filter_map(|(peer_id, state)| {
                    state.command_rx.try_recv().ok().map(|cmd| (*peer_id, cmd))
                })
                .collect();

            for (peer_id, cmd) in commands {
                self.handle_command(peer_id, cmd).await?;
                did_work = true;
            }

            // 3. Poll SHM host for incoming messages
            let messages = self.host.poll();
            for (peer_id, frame) in messages {
                self.last_decoded = frame.payload_bytes().to_vec();

                let msg = frame_to_message(frame).map_err(|e| {
                    ShmConnectionError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
                })?;

                self.handle_message(peer_id, msg).await?;
                did_work = true;
            }

            // Check if all peers are gone
            if self.peers.is_empty() {
                return Ok(());
            }

            // Yield to avoid busy-spinning when idle
            if !did_work {
                tokio::task::yield_now().await;
            }
        }
    }

    /// Handle a task message from a spawned handler for a specific peer.
    async fn handle_task_message(
        &mut self,
        peer_id: PeerId,
        msg: TaskMessage,
    ) -> Result<(), ShmConnectionError> {
        let wire_msg = match msg {
            TaskMessage::Data {
                channel_id,
                payload,
            } => Message::Data {
                channel_id,
                payload,
            },
            TaskMessage::Close { channel_id } => Message::Close { channel_id },
            TaskMessage::Response {
                request_id,
                payload,
            } => {
                // Only send if this request is still in-flight
                if let Some(state) = self.peers.get_mut(&peer_id) {
                    if !state.in_flight_server_requests.remove(&request_id) {
                        return Ok(());
                    }
                }
                Message::Response {
                    request_id,
                    metadata: Vec::new(),
                    payload,
                }
            }
        };

        self.send_to_peer(peer_id, &wire_msg).await
    }

    /// Handle a command from a ConnectionHandle for a specific peer.
    async fn handle_command(
        &mut self,
        peer_id: PeerId,
        cmd: HandleCommand,
    ) -> Result<(), ShmConnectionError> {
        match cmd {
            HandleCommand::Call {
                request_id,
                method_id,
                metadata,
                payload,
                response_tx,
            } => {
                // Store the response channel
                if let Some(state) = self.peers.get_mut(&peer_id) {
                    state.pending_responses.insert(request_id, response_tx);
                }

                // Send the request
                let req = Message::Request {
                    request_id,
                    method_id,
                    metadata,
                    payload,
                };
                self.send_to_peer(peer_id, &req).await?;
            }
        }
        Ok(())
    }

    /// Handle an incoming message from a specific peer.
    async fn handle_message(
        &mut self,
        peer_id: PeerId,
        msg: Message,
    ) -> Result<(), ShmConnectionError> {
        match msg {
            Message::Hello(_) => {
                return Err(self.goodbye(peer_id, "shm.handshake").await);
            }
            Message::Goodbye { .. } => {
                // Fail all pending responses for this peer
                if let Some(state) = self.peers.get_mut(&peer_id) {
                    for (_, tx) in state.pending_responses.drain() {
                        let _ = tx.send(Err(TransportError::ConnectionClosed));
                    }
                }
                // Remove the peer
                self.peers.remove(&peer_id);
                return Ok(());
            }
            Message::Request {
                request_id,
                method_id,
                metadata,
                payload,
            } => {
                self.handle_incoming_request(peer_id, request_id, method_id, metadata, payload)
                    .await?;
            }
            Message::Response {
                request_id,
                metadata: _,
                payload,
            } => {
                // Route to waiting caller
                if let Some(state) = self.peers.get_mut(&peer_id) {
                    if let Some(tx) = state.pending_responses.remove(&request_id) {
                        let _ = tx.send(Ok(payload));
                    }
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
                return Err(self.goodbye(peer_id, "shm.flow.no-credit-message").await);
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
        payload: Vec<u8>,
    ) -> Result<(), ShmConnectionError> {
        let state = match self.peers.get_mut(&peer_id) {
            Some(s) => s,
            None => return Ok(()), // Peer gone
        };

        // Duplicate detection
        if !state.in_flight_server_requests.insert(request_id) {
            return Err(self
                .goodbye(peer_id, "call.request-id.duplicate-detection")
                .await);
        }

        // Validate metadata
        if let Err(rule_id) = roam_wire::validate_metadata(&metadata) {
            state.in_flight_server_requests.remove(&request_id);
            return Err(self.goodbye(peer_id, rule_id).await);
        }

        // Validate payload size
        if payload.len() as u32 > self.negotiated.max_payload_size {
            state.in_flight_server_requests.remove(&request_id);
            return Err(self.goodbye(peer_id, "flow.call.payload-limit").await);
        }

        // Dispatch - spawn as a task so message loop can continue.
        let handler_fut = state.dispatcher.dispatch(
            method_id,
            payload,
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
            return Err(self.goodbye(peer_id, "streaming.id.zero-reserved").await);
        }

        if payload.len() as u32 > self.negotiated.max_payload_size {
            return Err(self.goodbye(peer_id, "flow.call.payload-limit").await);
        }

        let state = match self.peers.get_mut(&peer_id) {
            Some(s) => s,
            None => return Ok(()),
        };

        // Try server registry first, then client registry
        let result = if state.server_channel_registry.contains_incoming(channel_id) {
            state
                .server_channel_registry
                .route_data(channel_id, payload)
                .await
        } else if state.handle.contains_channel(channel_id) {
            state.handle.route_data(channel_id, payload).await
        } else {
            Err(ChannelError::Unknown)
        };

        match result {
            Ok(()) => Ok(()),
            Err(ChannelError::Unknown) => Err(self.goodbye(peer_id, "streaming.unknown").await),
            Err(ChannelError::DataAfterClose) => {
                Err(self.goodbye(peer_id, "streaming.data-after-close").await)
            }
            Err(ChannelError::CreditOverrun) => {
                Err(self.goodbye(peer_id, "flow.stream.credit-overrun").await)
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
            return Err(self.goodbye(peer_id, "streaming.id.zero-reserved").await);
        }

        let state = match self.peers.get_mut(&peer_id) {
            Some(s) => s,
            None => return Ok(()),
        };

        if state.server_channel_registry.contains(channel_id) {
            state.server_channel_registry.close(channel_id);
        } else if state.handle.contains_channel(channel_id) {
            state.handle.close_channel(channel_id);
        } else {
            return Err(self.goodbye(peer_id, "streaming.unknown").await);
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

    /// Send a message to a specific peer.
    async fn send_to_peer(
        &mut self,
        peer_id: PeerId,
        msg: &Message,
    ) -> Result<(), ShmConnectionError> {
        let frame = message_to_frame(msg).map_err(|e| {
            ShmConnectionError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        })?;

        self.host.send(peer_id, frame).map_err(|e| {
            ShmConnectionError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("send error: {:?}", e),
            ))
        })
    }

    /// Send Goodbye to a peer and return error.
    async fn goodbye(&mut self, peer_id: PeerId, rule_id: &'static str) -> ShmConnectionError {
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
            context: format!(
                "SHM protocol violation from peer {:?}: {}",
                peer_id, rule_id
            ),
        }
    }
}

/// Establish a multi-peer host driver.
///
/// This is a convenience function that creates a `MultiPeerHostDriver` for
/// scenarios where all peers use the same dispatcher type.
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
) -> (MultiPeerHostDriver<D>, HashMap<PeerId, ConnectionHandle>)
where
    D: ServiceDispatcher,
    I: IntoIterator<Item = (PeerId, D)>,
{
    let mut builder = MultiPeerHostDriver::new(host);
    for (peer_id, dispatcher) in peers {
        builder = builder.add_peer(peer_id, dispatcher);
    }
    builder.build()
}
