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
    CallError, ChannelError, ChannelRegistry, ConnectionHandle, HandleCommand, Role,
    ServiceDispatcher, TaskMessage,
};
use roam_stream::MessageTransport;
use roam_wire::Message;
use tokio::sync::{mpsc, oneshot};

use crate::host::ShmHost;
use crate::peer::PeerId;
use crate::transport::{OwnedShmHostTransport, ShmGuestTransport};

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
    pending_responses: HashMap<u64, oneshot::Sender<Result<Vec<u8>, CallError>>>,

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
                    let _ = tx.send(Err(CallError::ConnectionClosed));
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
            let _ = tx.send(Err(CallError::ConnectionClosed));
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
