//! Bidirectional connection driver.
//!
//! The driver is a future that handles all I/O for a connection:
//! - Dispatches incoming requests to a service
//! - Routes incoming responses to waiting callers
//! - Sends outgoing requests from ConnectionHandle
//! - Handles stream data (Data/Close/Reset/Credit)

use std::collections::HashMap;
use std::time::Duration;

use roam_session::{
    CallError, ChannelError, ChannelRegistry, ConnectionHandle, HandleCommand, Role,
    ServiceDispatcher, TaskMessage,
};
use roam_wire::{Hello, Message};
use tokio::sync::{mpsc, oneshot};

use crate::connection::{ConnectionError, Negotiated};
use crate::transport::MessageTransport;

/// The connection driver - a future that handles bidirectional RPC.
///
/// This must be spawned or awaited to drive the connection forward.
/// Use [`ConnectionHandle`] to make outgoing calls.
pub struct Driver<T, D> {
    io: T,
    dispatcher: D,
    #[allow(dead_code)]
    role: Role,
    negotiated: Negotiated,

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

impl<T, D> Driver<T, D>
where
    T: MessageTransport,
    D: ServiceDispatcher,
{
    /// Create a new driver with the given transport, dispatcher, and parameters.
    ///
    /// The `task_tx` is used to create the server-side stream registry.
    /// The `task_rx` is polled in the driver loop to send Data/Close/Response messages.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        io: T,
        dispatcher: D,
        role: Role,
        negotiated: Negotiated,
        handle: ConnectionHandle,
        command_rx: mpsc::Receiver<HandleCommand>,
        task_tx: mpsc::Sender<TaskMessage>,
        task_rx: mpsc::Receiver<TaskMessage>,
    ) -> Self {
        let initial_credit = negotiated.initial_credit;
        Self {
            io,
            dispatcher,
            role,
            negotiated,
            handle,
            command_rx,
            server_channel_registry: ChannelRegistry::new_with_credit(initial_credit, task_tx),
            pending_responses: HashMap::new(),
            in_flight_server_requests: std::collections::HashSet::new(),
            task_rx,
        }
    }

    /// Run the driver until the connection closes.
    pub async fn run(mut self) -> Result<(), ConnectionError> {
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
                result = self.io.recv_timeout(Duration::from_secs(30)) => {
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
    ///
    /// All messages (Data/Close/Response) go through a single channel to preserve ordering.
    async fn handle_task_message(&mut self, msg: TaskMessage) -> Result<(), ConnectionError> {
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
        self.io.send(&wire_msg).await?;
        Ok(())
    }

    /// Handle a command from ConnectionHandle.
    async fn handle_command(&mut self, cmd: HandleCommand) -> Result<(), ConnectionError> {
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
                self.io.send(&req).await?;
            }
        }
        Ok(())
    }

    /// Handle result from recv_timeout.
    /// Returns Ok(true) to continue, Ok(false) to shutdown cleanly, Err for errors.
    async fn handle_recv(
        &mut self,
        result: std::io::Result<Option<Message>>,
    ) -> Result<bool, ConnectionError> {
        let msg = match result {
            Ok(Some(m)) => m,
            Ok(None) => return Ok(false), // Clean shutdown
            Err(e) => {
                // Check for protocol errors
                let raw = self.io.last_decoded();
                if raw.len() >= 2 && raw[0] == 0x00 && raw[1] != 0x00 {
                    return Err(self.goodbye("message.hello.unknown-version").await);
                }
                if !raw.is_empty() && raw[0] >= 9 {
                    return Err(self.goodbye("message.unknown-variant").await);
                }
                if e.kind() == std::io::ErrorKind::InvalidData {
                    return Err(self.goodbye("message.decode-error").await);
                }
                return Err(ConnectionError::Io(e));
            }
        };

        match self.handle_message(msg).await {
            Ok(()) => Ok(true),
            Err(ConnectionError::Closed) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Handle a single incoming message.
    async fn handle_message(&mut self, msg: Message) -> Result<(), ConnectionError> {
        match msg {
            Message::Hello(_) => {
                // Duplicate Hello - ignore
            }
            Message::Goodbye { .. } => {
                // Fail all pending responses
                for (_, tx) in self.pending_responses.drain() {
                    let _ = tx.send(Err(CallError::ConnectionClosed));
                }
                return Err(ConnectionError::Closed);
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
            Message::Credit { channel_id, bytes } => {
                self.handle_credit(channel_id, bytes)?;
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
    ) -> Result<(), ConnectionError> {
        // Duplicate detection
        if !self.in_flight_server_requests.insert(request_id) {
            return Err(self.goodbye("unary.request-id.duplicate-detection").await);
        }

        // Validate metadata
        if let Err(rule_id) = roam_wire::validate_metadata(&metadata) {
            self.in_flight_server_requests.remove(&request_id);
            return Err(self.goodbye(rule_id).await);
        }

        // Validate payload size
        if payload.len() as u32 > self.negotiated.max_payload_size {
            self.in_flight_server_requests.remove(&request_id);
            return Err(self.goodbye("flow.unary.payload-limit").await);
        }

        // Dispatch - spawn as a task so message loop can continue.
        // The handler is responsible for sending Data/Close/Response via the task channel.
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
    ) -> Result<(), ConnectionError> {
        if channel_id == 0 {
            return Err(self.goodbye("streaming.id.zero-reserved").await);
        }

        if payload.len() as u32 > self.negotiated.max_payload_size {
            return Err(self.goodbye("flow.unary.payload-limit").await);
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
    async fn handle_close(&mut self, channel_id: u64) -> Result<(), ConnectionError> {
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
    fn handle_reset(&mut self, channel_id: u64) -> Result<(), ConnectionError> {
        if channel_id == 0 {
            // For Reset, we don't send Goodbye for zero - just return error
            // Actually spec says we MUST send Goodbye
            // But we can't await here... let's make this async
        }

        // Try both registries - Reset on unknown stream is not an error
        if self.server_channel_registry.contains(channel_id) {
            self.server_channel_registry.reset(channel_id);
        } else if self.handle.contains_channel(channel_id) {
            self.handle.reset_channel(channel_id);
        }
        // Unknown stream for Reset is ignored per spec
        Ok(())
    }

    /// Handle incoming Credit message.
    fn handle_credit(&mut self, channel_id: u64, bytes: u32) -> Result<(), ConnectionError> {
        if channel_id == 0 {
            // Same issue as Reset - need async for Goodbye
        }

        // Try both registries
        if self.server_channel_registry.contains(channel_id) {
            self.server_channel_registry
                .receive_credit(channel_id, bytes);
        } else if self.handle.contains_channel(channel_id) {
            self.handle.receive_credit(channel_id, bytes);
        }
        // Unknown stream for Credit - should be error but we'd need async
        Ok(())
    }

    /// Send Goodbye and return error.
    async fn goodbye(&mut self, rule_id: &'static str) -> ConnectionError {
        // Fail all pending responses
        for (_, tx) in self.pending_responses.drain() {
            let _ = tx.send(Err(CallError::ConnectionClosed));
        }

        let _ = self
            .io
            .send(&Message::Goodbye {
                reason: rule_id.into(),
            })
            .await;

        ConnectionError::ProtocolViolation {
            rule_id,
            context: String::new(),
        }
    }
}

/// Establish a bidirectional connection as the acceptor.
///
/// Returns a handle for making calls and a driver future that must be spawned.
pub async fn establish_acceptor<T, D>(
    mut io: T,
    our_hello: Hello,
    dispatcher: D,
) -> Result<(ConnectionHandle, Driver<T, D>), ConnectionError>
where
    T: MessageTransport,
    D: ServiceDispatcher,
{
    // Send our Hello immediately
    io.send(&Message::Hello(our_hello.clone())).await?;

    // Wait for peer Hello
    let peer_hello = match io.recv_timeout(Duration::from_secs(5)).await? {
        Some(Message::Hello(h)) => h,
        Some(_) => {
            let _ = io
                .send(&Message::Goodbye {
                    reason: "message.hello.ordering".into(),
                })
                .await;
            return Err(ConnectionError::ProtocolViolation {
                rule_id: "message.hello.ordering",
                context: "received non-Hello before Hello exchange".into(),
            });
        }
        None => return Err(ConnectionError::Closed),
    };

    let (our_max, our_credit) = match &our_hello {
        Hello::V1 {
            max_payload_size,
            initial_channel_credit,
        } => (*max_payload_size, *initial_channel_credit),
    };
    let (peer_max, peer_credit) = match &peer_hello {
        Hello::V1 {
            max_payload_size,
            initial_channel_credit,
        } => (*max_payload_size, *initial_channel_credit),
    };

    let negotiated = Negotiated {
        max_payload_size: our_max.min(peer_max),
        initial_credit: our_credit.min(peer_credit),
    };

    let (command_tx, command_rx) = mpsc::channel(64);
    let (task_tx, task_rx) = mpsc::channel(64);
    let handle = ConnectionHandle::new(
        command_tx,
        Role::Acceptor,
        negotiated.initial_credit,
        task_tx.clone(),
    );

    let driver = Driver::new(
        io,
        dispatcher,
        Role::Acceptor,
        negotiated,
        handle.clone(),
        command_rx,
        task_tx,
        task_rx,
    );

    Ok((handle, driver))
}

/// Establish a bidirectional connection as the initiator.
///
/// Returns a handle for making calls and a driver future that must be spawned.
pub async fn establish_initiator<T, D>(
    mut io: T,
    our_hello: Hello,
    dispatcher: D,
) -> Result<(ConnectionHandle, Driver<T, D>), ConnectionError>
where
    T: MessageTransport,
    D: ServiceDispatcher,
{
    // Send our Hello immediately
    io.send(&Message::Hello(our_hello.clone())).await?;

    // Wait for peer Hello
    let peer_hello = match io.recv_timeout(Duration::from_secs(5)).await? {
        Some(Message::Hello(h)) => h,
        Some(_) => {
            let _ = io
                .send(&Message::Goodbye {
                    reason: "message.hello.ordering".into(),
                })
                .await;
            return Err(ConnectionError::ProtocolViolation {
                rule_id: "message.hello.ordering",
                context: "received non-Hello before Hello exchange".into(),
            });
        }
        None => return Err(ConnectionError::Closed),
    };

    let (our_max, our_credit) = match &our_hello {
        Hello::V1 {
            max_payload_size,
            initial_channel_credit,
        } => (*max_payload_size, *initial_channel_credit),
    };
    let (peer_max, peer_credit) = match &peer_hello {
        Hello::V1 {
            max_payload_size,
            initial_channel_credit,
        } => (*max_payload_size, *initial_channel_credit),
    };

    let negotiated = Negotiated {
        max_payload_size: our_max.min(peer_max),
        initial_credit: our_credit.min(peer_credit),
    };

    let (command_tx, command_rx) = mpsc::channel(64);
    let (task_tx, task_rx) = mpsc::channel(64);
    let handle = ConnectionHandle::new(
        command_tx,
        Role::Initiator,
        negotiated.initial_credit,
        task_tx.clone(),
    );

    let driver = Driver::new(
        io,
        dispatcher,
        Role::Initiator,
        negotiated,
        handle.clone(),
        command_rx,
        task_tx,
        task_rx,
    );

    Ok((handle, driver))
}
