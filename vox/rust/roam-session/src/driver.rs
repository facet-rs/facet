//! Bidirectional connection driver for message-based transports.
//!
//! This module provides the core connection handling for roam over transports
//! that already provide message framing (like WebSocket).
//!
//! For byte-stream transports (TCP, Unix sockets), see `roam-stream` which
//! wraps streams in COBS framing before using this driver.
//!
//! # Example
//!
//! ```ignore
//! use roam_session::{accept_framed, HandshakeConfig, NoDispatcher};
//! use roam_websocket::WsTransport;
//!
//! let transport = WsTransport::connect("ws://localhost:9000").await?;
//! let (handle, driver) = accept_framed(transport, HandshakeConfig::default(), NoDispatcher).await?;
//!
//! // Spawn the driver (uses runtime abstraction - works on native and WASM)
//! roam_session::runtime::spawn(async move {
//!     let _ = driver.run().await;
//! });
//!
//! // Use handle with generated client
//! let client = MyServiceClient::new(handle);
//! let response = client.echo("hello").await?;
//! ```

use std::collections::HashMap;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use facet::Facet;

use crate::runtime::{Mutex, Receiver, channel, sleep, spawn};
use crate::{
    ChannelError, ChannelRegistry, ConnectionHandle, DriverMessage, MessageTransport, ResponseData,
    RoamError, Role, ServiceDispatcher, TransportError,
};
use roam_wire::{Hello, Message};

/// Negotiated connection parameters after Hello exchange.
#[derive(Debug, Clone)]
pub struct Negotiated {
    /// Effective max payload size (min of both peers).
    pub max_payload_size: u32,
    /// Initial stream credit (min of both peers).
    pub initial_credit: u32,
}

/// Error during connection handling.
#[derive(Debug)]
pub enum ConnectionError {
    /// IO error.
    Io(std::io::Error),
    /// Protocol violation requiring Goodbye.
    ProtocolViolation {
        /// Rule ID that was violated.
        rule_id: &'static str,
        /// Human-readable context.
        context: String,
    },
    /// Dispatch error.
    Dispatch(String),
    /// Connection closed cleanly.
    Closed,
}

impl std::fmt::Display for ConnectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectionError::Io(e) => write!(f, "IO error: {e}"),
            ConnectionError::ProtocolViolation { rule_id, context } => {
                write!(f, "protocol violation: {rule_id}: {context}")
            }
            ConnectionError::Dispatch(msg) => write!(f, "dispatch error: {msg}"),
            ConnectionError::Closed => write!(f, "connection closed"),
        }
    }
}

impl std::error::Error for ConnectionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ConnectionError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for ConnectionError {
    fn from(e: std::io::Error) -> Self {
        ConnectionError::Io(e)
    }
}

/// Configuration for connection handshake.
#[derive(Debug, Clone)]
pub struct HandshakeConfig {
    /// Maximum payload size we support.
    pub max_payload_size: u32,
    /// Initial credit for channels.
    pub initial_channel_credit: u32,
}

impl Default for HandshakeConfig {
    fn default() -> Self {
        Self {
            max_payload_size: 1024 * 1024,     // 1 MiB
            initial_channel_credit: 64 * 1024, // 64 KiB
        }
    }
}

impl HandshakeConfig {
    /// Convert to Hello message.
    pub fn to_hello(&self) -> Hello {
        Hello::V1 {
            max_payload_size: self.max_payload_size,
            initial_channel_credit: self.initial_channel_credit,
        }
    }
}

/// A factory that creates new message-based connections on demand.
///
/// Used by [`connect_framed()`] for reconnection with transports that
/// already provide message framing (like WebSocket).
pub trait MessageConnector: Send + Sync + 'static {
    /// The message transport type (e.g., `WsTransport`).
    type Transport: MessageTransport;

    /// Establish a new connection.
    fn connect(&self) -> impl Future<Output = io::Result<Self::Transport>> + Send;
}

/// Configuration for reconnection behavior.
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Maximum reconnection attempts before giving up.
    pub max_attempts: u32,
    /// Initial delay between reconnection attempts.
    pub initial_backoff: Duration,
    /// Maximum delay between reconnection attempts.
    pub max_backoff: Duration,
    /// Backoff multiplier.
    pub backoff_multiplier: f64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(5),
            backoff_multiplier: 2.0,
        }
    }
}

impl RetryPolicy {
    /// Calculate the backoff duration for a given attempt number.
    pub fn backoff_for_attempt(&self, attempt: u32) -> Duration {
        let multiplier = self
            .backoff_multiplier
            .powi(attempt.saturating_sub(1) as i32);
        let backoff = self.initial_backoff.mul_f64(multiplier);
        backoff.min(self.max_backoff)
    }
}

/// Error from a reconnecting client.
#[derive(Debug)]
pub enum ConnectError {
    /// All retry attempts exhausted.
    RetriesExhausted {
        /// The original error.
        original: io::Error,
        /// Number of attempts made.
        attempts: u32,
    },
    /// Connection failed.
    ConnectFailed(io::Error),
    /// RPC error during connection setup.
    Rpc(TransportError),
}

impl std::fmt::Display for ConnectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectError::RetriesExhausted { original, attempts } => {
                write!(
                    f,
                    "reconnection failed after {attempts} attempts: {original}"
                )
            }
            ConnectError::ConnectFailed(e) => write!(f, "connection failed: {e}"),
            ConnectError::Rpc(e) => write!(f, "RPC error: {e}"),
        }
    }
}

impl std::error::Error for ConnectError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ConnectError::RetriesExhausted { original, .. } => Some(original),
            ConnectError::ConnectFailed(e) => Some(e),
            ConnectError::Rpc(e) => Some(e),
        }
    }
}

impl From<TransportError> for ConnectError {
    fn from(e: TransportError) -> Self {
        ConnectError::Rpc(e)
    }
}

// ============================================================================
// accept_framed() - For accepted connections (no reconnection)
// ============================================================================

/// Accept a connection with a pre-framed transport (e.g., WebSocket).
///
/// Use this when the transport already provides message framing.
/// Returns a handle for making calls and a driver that must be spawned.
pub async fn accept_framed<T, D>(
    transport: T,
    config: HandshakeConfig,
    dispatcher: D,
) -> Result<(ConnectionHandle, Driver<T, D>), ConnectionError>
where
    T: MessageTransport,
    D: ServiceDispatcher,
{
    establish(transport, config.to_hello(), dispatcher, Role::Acceptor).await
}

// ============================================================================
// connect_framed() - For message transports with reconnection
// ============================================================================

/// Connect using a message transport with automatic reconnection.
///
/// Returns a client that automatically reconnects on failure.
/// Implements [`Caller`](crate::Caller) so it works with generated service clients.
pub fn connect_framed<C, D>(
    connector: C,
    config: HandshakeConfig,
    dispatcher: D,
) -> FramedClient<C, D>
where
    C: MessageConnector,
    D: ServiceDispatcher + Clone,
{
    FramedClient {
        connector: Arc::new(connector),
        config,
        dispatcher,
        retry_policy: RetryPolicy::default(),
        state: Arc::new(Mutex::new(None)),
    }
}

/// Connect using a message transport with a custom retry policy.
pub fn connect_framed_with_policy<C, D>(
    connector: C,
    config: HandshakeConfig,
    dispatcher: D,
    retry_policy: RetryPolicy,
) -> FramedClient<C, D>
where
    C: MessageConnector,
    D: ServiceDispatcher + Clone,
{
    FramedClient {
        connector: Arc::new(connector),
        config,
        dispatcher,
        retry_policy,
        state: Arc::new(Mutex::new(None)),
    }
}

/// Internal connection state for FramedClient.
struct FramedClientState {
    handle: ConnectionHandle,
}

/// A client for message transports that automatically reconnects on failure.
///
/// Created by [`connect_framed()`]. Implements [`Caller`](crate::Caller) so it
/// works with generated service clients.
pub struct FramedClient<C, D> {
    connector: Arc<C>,
    config: HandshakeConfig,
    dispatcher: D,
    retry_policy: RetryPolicy,
    state: Arc<Mutex<Option<FramedClientState>>>,
}

impl<C, D> Clone for FramedClient<C, D>
where
    D: Clone,
{
    fn clone(&self) -> Self {
        Self {
            connector: self.connector.clone(),
            config: self.config.clone(),
            dispatcher: self.dispatcher.clone(),
            retry_policy: self.retry_policy.clone(),
            state: self.state.clone(),
        }
    }
}

impl<C, D> FramedClient<C, D>
where
    C: MessageConnector,
    D: ServiceDispatcher + Clone + 'static,
{
    /// Get the underlying handle if connected.
    pub async fn handle(&self) -> Result<ConnectionHandle, ConnectError> {
        self.ensure_connected().await
    }

    async fn ensure_connected(&self) -> Result<ConnectionHandle, ConnectError> {
        let mut state = self.state.lock().await;

        if let Some(ref conn) = *state {
            // Note: On WASM we can't detect dead connections via JoinHandle.
            // The connection will fail on next use if dead.
            return Ok(conn.handle.clone());
        }

        let conn = self.connect_internal().await?;
        let handle = conn.handle.clone();
        *state = Some(conn);
        Ok(handle)
    }

    async fn connect_internal(&self) -> Result<FramedClientState, ConnectError> {
        let transport = self
            .connector
            .connect()
            .await
            .map_err(ConnectError::ConnectFailed)?;

        let (handle, driver) = establish(
            transport,
            self.config.to_hello(),
            self.dispatcher.clone(),
            Role::Initiator,
        )
        .await
        .map_err(|e| ConnectError::ConnectFailed(connection_error_to_io(e)))?;

        // Spawn driver using runtime abstraction (works on native and WASM)
        spawn(async move {
            let _ = driver.run().await;
        });

        Ok(FramedClientState { handle })
    }

    /// Make a raw RPC call with automatic reconnection.
    pub async fn call_raw(
        &self,
        method_id: u64,
        payload: Vec<u8>,
    ) -> Result<Vec<u8>, ConnectError> {
        let mut last_error: Option<io::Error> = None;
        let mut attempt = 0u32;

        loop {
            let handle = match self.ensure_connected().await {
                Ok(h) => h,
                Err(ConnectError::ConnectFailed(e)) => {
                    attempt += 1;
                    if attempt >= self.retry_policy.max_attempts {
                        return Err(ConnectError::RetriesExhausted {
                            original: last_error.unwrap_or(e),
                            attempts: attempt,
                        });
                    }
                    last_error = Some(e);
                    let backoff = self.retry_policy.backoff_for_attempt(attempt);
                    sleep(backoff).await;
                    continue;
                }
                Err(e) => return Err(e),
            };

            match handle.call_raw(method_id, payload.clone()).await {
                Ok(response) => return Ok(response),
                Err(TransportError::Encode(e)) => {
                    return Err(ConnectError::Rpc(TransportError::Encode(e)));
                }
                Err(TransportError::ConnectionClosed) | Err(TransportError::DriverGone) => {
                    {
                        let mut state = self.state.lock().await;
                        *state = None;
                    }

                    attempt += 1;
                    if attempt >= self.retry_policy.max_attempts {
                        let error = last_error.unwrap_or_else(|| {
                            io::Error::new(io::ErrorKind::ConnectionReset, "connection closed")
                        });
                        return Err(ConnectError::RetriesExhausted {
                            original: error,
                            attempts: attempt,
                        });
                    }

                    last_error = Some(io::Error::new(
                        io::ErrorKind::ConnectionReset,
                        "connection closed",
                    ));
                    let backoff = self.retry_policy.backoff_for_attempt(attempt);
                    sleep(backoff).await;
                }
            }
        }
    }
}

impl<C, D> crate::Caller for FramedClient<C, D>
where
    C: MessageConnector,
    D: ServiceDispatcher + Clone + 'static,
{
    async fn call<T: Facet<'static>>(
        &self,
        method_id: u64,
        args: &mut T,
    ) -> Result<ResponseData, TransportError> {
        let mut attempt = 0u32;

        loop {
            let handle = match self.ensure_connected().await {
                Ok(h) => h,
                Err(ConnectError::ConnectFailed(_)) => {
                    attempt += 1;
                    if attempt >= self.retry_policy.max_attempts {
                        return Err(TransportError::ConnectionClosed);
                    }
                    let backoff = self.retry_policy.backoff_for_attempt(attempt);
                    sleep(backoff).await;
                    continue;
                }
                Err(ConnectError::RetriesExhausted { .. }) => {
                    return Err(TransportError::ConnectionClosed);
                }
                Err(ConnectError::Rpc(e)) => return Err(e),
            };

            match handle.call(method_id, args).await {
                Ok(response) => return Ok(response),
                Err(TransportError::Encode(e)) => {
                    return Err(TransportError::Encode(e));
                }
                Err(TransportError::ConnectionClosed) | Err(TransportError::DriverGone) => {
                    {
                        let mut state = self.state.lock().await;
                        *state = None;
                    }

                    attempt += 1;
                    if attempt >= self.retry_policy.max_attempts {
                        return Err(TransportError::ConnectionClosed);
                    }

                    let backoff = self.retry_policy.backoff_for_attempt(attempt);
                    sleep(backoff).await;
                }
            }
        }
    }

    fn bind_response_streams<R: Facet<'static>>(&self, response: &mut R, channels: &[u64]) {
        // FramedClient wraps a ConnectionHandle, but we don't have direct access to it
        // during bind_response_streams. For reconnecting clients, response stream binding
        // would need to be handled at a higher level or the client would need to store
        // the current handle.
        // For now, this is a no-op - FramedClient users should use ConnectionHandle
        // directly if they need response stream binding.
        let _ = (response, channels);
    }
}

fn connection_error_to_io(e: ConnectionError) -> io::Error {
    match e {
        ConnectionError::Io(io_err) => io_err,
        ConnectionError::ProtocolViolation { rule_id, context } => io::Error::new(
            io::ErrorKind::InvalidData,
            format!("protocol violation: {rule_id}: {context}"),
        ),
        ConnectionError::Dispatch(msg) => io::Error::other(format!("dispatch error: {msg}")),
        ConnectionError::Closed => {
            io::Error::new(io::ErrorKind::ConnectionReset, "connection closed")
        }
    }
}

// ============================================================================
// Driver - The core connection loop
// ============================================================================

/// The connection driver - a future that handles bidirectional RPC.
///
/// This must be spawned or awaited to drive the connection forward.
pub struct Driver<T, D> {
    io: T,
    dispatcher: D,
    #[allow(dead_code)]
    role: Role,
    negotiated: Negotiated,
    handle: ConnectionHandle,
    /// Unified channel for all messages (Call/Data/Close/Response).
    driver_rx: Receiver<DriverMessage>,
    server_channel_registry: ChannelRegistry,
    pending_responses:
        HashMap<u64, crate::runtime::OneshotSender<Result<ResponseData, TransportError>>>,
    in_flight_server_requests: std::collections::HashSet<u64>,
}

impl<T, D> Driver<T, D>
where
    T: MessageTransport,
    D: ServiceDispatcher,
{
    /// Run the driver until the connection closes.
    pub async fn run(mut self) -> Result<(), ConnectionError> {
        use futures_util::FutureExt;

        loop {
            futures_util::select! {
                msg = self.driver_rx.recv().fuse() => {
                    if let Some(msg) = msg {
                        self.handle_driver_message(msg).await?;
                    }
                }

                result = self.io.recv().fuse() => {
                    match self.handle_recv(result).await {
                        Ok(true) => continue,
                        Ok(false) => return Ok(()),
                        Err(e) => return Err(e),
                    }
                }
            }
        }
    }

    async fn handle_driver_message(&mut self, msg: DriverMessage) -> Result<(), ConnectionError> {
        match msg {
            DriverMessage::Call {
                request_id,
                method_id,
                metadata,
                channels,
                payload,
                response_tx,
            } => {
                self.pending_responses.insert(request_id, response_tx);
                let req = Message::Request {
                    request_id,
                    method_id,
                    metadata,
                    channels,
                    payload,
                };
                self.io.send(&req).await?;
            }
            DriverMessage::Data {
                channel_id,
                payload,
            } => {
                let wire_msg = Message::Data {
                    channel_id,
                    payload,
                };
                self.io.send(&wire_msg).await?;
            }
            DriverMessage::Close { channel_id } => {
                let wire_msg = Message::Close { channel_id };
                self.io.send(&wire_msg).await?;
            }
            DriverMessage::Response {
                request_id,
                channels,
                payload,
            } => {
                if !self.in_flight_server_requests.remove(&request_id) {
                    return Ok(());
                }
                let wire_msg = Message::Response {
                    request_id,
                    metadata: vec![],
                    channels,
                    payload,
                };
                self.io.send(&wire_msg).await?;
            }
        }
        Ok(())
    }

    async fn handle_recv(
        &mut self,
        result: std::io::Result<Option<Message>>,
    ) -> Result<bool, ConnectionError> {
        let msg = match result {
            Ok(Some(m)) => m,
            Ok(None) => return Ok(false),
            Err(e) => {
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

    async fn handle_message(&mut self, msg: Message) -> Result<(), ConnectionError> {
        match msg {
            Message::Hello(_) => {}
            Message::Goodbye { .. } => {
                for (_, tx) in self.pending_responses.drain() {
                    let _ = tx.send(Err(TransportError::ConnectionClosed));
                }
                return Err(ConnectionError::Closed);
            }
            Message::Request {
                request_id,
                method_id,
                metadata,
                channels,
                payload,
            } => {
                self.handle_incoming_request(request_id, method_id, metadata, channels, payload)
                    .await?;
            }
            Message::Response {
                request_id,
                channels,
                payload,
                ..
            } => {
                if let Some(tx) = self.pending_responses.remove(&request_id) {
                    let _ = tx.send(Ok(ResponseData { payload, channels }));
                }
            }
            Message::Cancel { .. } => {}
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

    async fn handle_incoming_request(
        &mut self,
        request_id: u64,
        method_id: u64,
        metadata: Vec<(String, roam_wire::MetadataValue)>,
        channels: Vec<u64>,
        payload: Vec<u8>,
    ) -> Result<(), ConnectionError> {
        if !self.in_flight_server_requests.insert(request_id) {
            return Err(self.goodbye("call.request-id.duplicate-detection").await);
        }

        if let Err(rule_id) = roam_wire::validate_metadata(&metadata) {
            self.in_flight_server_requests.remove(&request_id);
            return Err(self.goodbye(rule_id).await);
        }

        if payload.len() as u32 > self.negotiated.max_payload_size {
            self.in_flight_server_requests.remove(&request_id);
            return Err(self.goodbye("flow.call.payload-limit").await);
        }

        let handler_fut = self.dispatcher.dispatch(
            method_id,
            payload,
            channels,
            request_id,
            &mut self.server_channel_registry,
        );
        spawn(async move {
            handler_fut.await;
        });
        Ok(())
    }

    async fn handle_data(
        &mut self,
        channel_id: u64,
        payload: Vec<u8>,
    ) -> Result<(), ConnectionError> {
        if channel_id == 0 {
            return Err(self.goodbye("channeling.id.zero-reserved").await);
        }

        if payload.len() as u32 > self.negotiated.max_payload_size {
            return Err(self.goodbye("flow.call.payload-limit").await);
        }

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
            Err(ChannelError::Unknown) => Err(self.goodbye("channeling.unknown").await),
            Err(ChannelError::DataAfterClose) => {
                Err(self.goodbye("channeling.data-after-close").await)
            }
            Err(ChannelError::CreditOverrun) => {
                Err(self.goodbye("flow.channel.credit-overrun").await)
            }
        }
    }

    async fn handle_close(&mut self, channel_id: u64) -> Result<(), ConnectionError> {
        if channel_id == 0 {
            return Err(self.goodbye("channeling.id.zero-reserved").await);
        }

        if self.server_channel_registry.contains(channel_id) {
            self.server_channel_registry.close(channel_id);
        } else if self.handle.contains_channel(channel_id) {
            self.handle.close_channel(channel_id);
        } else {
            return Err(self.goodbye("channeling.unknown").await);
        }
        Ok(())
    }

    fn handle_reset(&mut self, channel_id: u64) -> Result<(), ConnectionError> {
        if self.server_channel_registry.contains(channel_id) {
            self.server_channel_registry.reset(channel_id);
        } else if self.handle.contains_channel(channel_id) {
            self.handle.reset_channel(channel_id);
        }
        Ok(())
    }

    fn handle_credit(&mut self, channel_id: u64, bytes: u32) -> Result<(), ConnectionError> {
        if self.server_channel_registry.contains(channel_id) {
            self.server_channel_registry
                .receive_credit(channel_id, bytes);
        } else if self.handle.contains_channel(channel_id) {
            self.handle.receive_credit(channel_id, bytes);
        }
        Ok(())
    }

    async fn goodbye(&mut self, rule_id: &'static str) -> ConnectionError {
        for (_, tx) in self.pending_responses.drain() {
            let _ = tx.send(Err(TransportError::ConnectionClosed));
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

// ============================================================================
// initiate_framed() - For initiator role
// ============================================================================

/// Initiate a connection with a pre-framed transport (e.g., WebSocket).
///
/// Use this when establishing a connection as the initiator (client).
/// Returns a handle for making calls and a driver that must be spawned.
pub async fn initiate_framed<T, D>(
    transport: T,
    config: HandshakeConfig,
    dispatcher: D,
) -> Result<(ConnectionHandle, Driver<T, D>), ConnectionError>
where
    T: MessageTransport,
    D: ServiceDispatcher,
{
    establish(transport, config.to_hello(), dispatcher, Role::Initiator).await
}

// ============================================================================
// establish() - Perform handshake and create driver (internal)
// ============================================================================

async fn establish<T, D>(
    mut io: T,
    our_hello: Hello,
    dispatcher: D,
    role: Role,
) -> Result<(ConnectionHandle, Driver<T, D>), ConnectionError>
where
    T: MessageTransport,
    D: ServiceDispatcher,
{
    // Send Hello
    io.send(&Message::Hello(our_hello.clone())).await?;

    // Wait for peer Hello with timeout
    let peer_hello = match io.recv_timeout(Duration::from_secs(5)).await {
        Ok(Some(Message::Hello(h))) => h,
        Ok(Some(_)) => {
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
        Ok(None) => return Err(ConnectionError::Closed),
        Err(e) => {
            let raw = io.last_decoded();
            let is_unknown_hello = raw.len() >= 2 && raw[0] == 0x00 && raw[1] != 0x00;
            let version = if is_unknown_hello { raw[1] } else { 0 };

            if is_unknown_hello {
                let _ = io
                    .send(&Message::Goodbye {
                        reason: "message.hello.unknown-version".into(),
                    })
                    .await;
                return Err(ConnectionError::ProtocolViolation {
                    rule_id: "message.hello.unknown-version",
                    context: format!("unknown Hello version: {version}"),
                });
            }
            return Err(ConnectionError::Io(e));
        }
    };

    // Negotiate parameters
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

    // Create unified channel for all messages
    let (driver_tx, driver_rx) = channel(256);

    let handle = ConnectionHandle::new(driver_tx.clone(), role, negotiated.initial_credit);

    let driver = Driver {
        io,
        dispatcher,
        role,
        negotiated: negotiated.clone(),
        handle: handle.clone(),
        driver_rx,
        server_channel_registry: ChannelRegistry::new_with_credit(
            negotiated.initial_credit,
            driver_tx,
        ),
        pending_responses: HashMap::new(),
        in_flight_server_requests: std::collections::HashSet::new(),
    };

    Ok((handle, driver))
}

// ============================================================================
// NoDispatcher - For client-only connections
// ============================================================================

/// A no-op dispatcher for client-only connections.
///
/// Returns UnknownMethod for all requests since we don't serve any methods.
pub struct NoDispatcher;

impl ServiceDispatcher for NoDispatcher {
    fn method_ids(&self) -> Vec<u64> {
        vec![]
    }

    fn dispatch(
        &self,
        _method_id: u64,
        _payload: Vec<u8>,
        _channels: Vec<u64>,
        request_id: u64,
        registry: &mut ChannelRegistry,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        let driver_tx = registry.driver_tx();
        Box::pin(async move {
            let response: Result<(), RoamError<()>> = Err(RoamError::UnknownMethod);
            let payload = facet_postcard::to_vec(&response).unwrap_or_default();
            let _ = driver_tx
                .send(DriverMessage::Response {
                    request_id,
                    channels: Vec::new(),
                    payload,
                })
                .await;
        })
    }
}

impl Clone for NoDispatcher {
    fn clone(&self) -> Self {
        NoDispatcher
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backoff_calculation() {
        let policy = RetryPolicy::default();
        assert_eq!(policy.backoff_for_attempt(1), Duration::from_millis(100));
        assert_eq!(policy.backoff_for_attempt(2), Duration::from_millis(200));
        assert_eq!(policy.backoff_for_attempt(3), Duration::from_millis(400));
        assert_eq!(policy.backoff_for_attempt(10), Duration::from_secs(5));
    }
}
