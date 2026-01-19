//! Bidirectional connection driver with optional reconnection.
//!
//! This module provides the core connection handling for roam:
//!
//! - [`accept()`] - For accepted connections (returns handle + driver to spawn)
//! - [`connect()`] - For initiated connections (returns a reconnecting client)
//!
//! # Example (Accepted connection)
//!
//! ```ignore
//! use roam_stream::{accept, HandshakeConfig};
//! use tokio::net::TcpListener;
//!
//! let listener = TcpListener::bind("127.0.0.1:9000").await?;
//! let (stream, _) = listener.accept().await?;
//!
//! let (handle, driver) = accept(stream, HandshakeConfig::default(), dispatcher).await?;
//! tokio::spawn(driver.run());
//!
//! // Use handle with generated client
//! let client = MyServiceClient::new(handle);
//! let response = client.echo("hello").await?;
//! ```
//!
//! # Example (Initiated connection with automatic reconnection)
//!
//! ```ignore
//! use roam_stream::{connect, Connector, HandshakeConfig};
//! use tokio::net::TcpStream;
//!
//! struct MyConnector { addr: String }
//!
//! impl Connector for MyConnector {
//!     type Transport = TcpStream;
//!     async fn connect(&self) -> io::Result<TcpStream> {
//!         TcpStream::connect(&self.addr).await
//!     }
//! }
//!
//! let connector = MyConnector { addr: "127.0.0.1:9000".into() };
//! let client = connect(connector, HandshakeConfig::default(), dispatcher);
//!
//! // Client automatically reconnects on failure
//! let service = MyServiceClient::new(client);
//! let response = service.echo("hello").await?;
//! ```

use std::collections::HashMap;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use facet::Facet;
use roam_session::{
    ChannelError, ChannelRegistry, ConnectionHandle, DriverMessage, RoamError, Role,
    ServiceDispatcher, TransportError,
};
use roam_wire::{Hello, Message};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::task::JoinHandle;

use crate::framing::CobsFramed;
use roam_session::MessageTransport;

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
    fn to_hello(&self) -> Hello {
        Hello::V1 {
            max_payload_size: self.max_payload_size,
            initial_channel_credit: self.initial_channel_credit,
        }
    }
}

/// A factory that creates new byte-stream connections on demand.
///
/// Used by [`connect()`] for reconnection. The transport will be wrapped
/// in COBS framing automatically.
///
/// For transports that already provide message framing (like WebSocket),
/// use [`MessageConnector`] instead.
pub trait Connector: Send + Sync + 'static {
    /// The raw stream type (e.g., `TcpStream`, `UnixStream`).
    type Transport: AsyncRead + AsyncWrite + Unpin + Send;

    /// Establish a new connection.
    fn connect(&self) -> impl Future<Output = io::Result<Self::Transport>> + Send;
}

/// A factory that creates new message-based connections on demand.
///
/// Used by [`connect_framed()`] for reconnection with transports that
/// already provide message framing (like WebSocket).
///
/// For byte-stream transports (TCP, Unix sockets), use [`Connector`] instead.
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
    fn backoff_for_attempt(&self, attempt: u32) -> Duration {
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
// accept() - For accepted connections (no reconnection)
// ============================================================================

/// Accept a connection and perform handshake.
///
/// For connections that were accepted (e.g., from a listener). No reconnection
/// is possible since we don't know how to re-establish the connection.
///
/// Returns a handle for making calls and a driver that must be spawned.
pub async fn accept<S, D>(
    stream: S,
    config: HandshakeConfig,
    dispatcher: D,
) -> Result<(ConnectionHandle, Driver<CobsFramed<S>, D>), ConnectionError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
    D: ServiceDispatcher,
{
    let framed = CobsFramed::new(stream);
    establish(framed, config.to_hello(), dispatcher, Role::Acceptor).await
}

/// Accept a connection with a pre-framed transport (e.g., WebSocket).
///
/// Use this when the transport already provides message framing.
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
// connect() - For initiated connections (with reconnection)
// ============================================================================

/// Connect to a peer with automatic reconnection.
///
/// Returns a client that automatically reconnects on failure. The client
/// implements [`Caller`](roam_session::Caller) so it works with generated service clients.
///
/// Connection is lazy - the first call triggers connection.
pub fn connect<C, D>(connector: C, config: HandshakeConfig, dispatcher: D) -> Client<C, D>
where
    C: Connector,
    D: ServiceDispatcher + Clone,
{
    Client {
        connector: Arc::new(connector),
        config,
        dispatcher,
        retry_policy: RetryPolicy::default(),
        state: Arc::new(Mutex::new(None)),
    }
}

/// Connect with a custom retry policy.
pub fn connect_with_policy<C, D>(
    connector: C,
    config: HandshakeConfig,
    dispatcher: D,
    retry_policy: RetryPolicy,
) -> Client<C, D>
where
    C: Connector,
    D: ServiceDispatcher + Clone,
{
    Client {
        connector: Arc::new(connector),
        config,
        dispatcher,
        retry_policy,
        state: Arc::new(Mutex::new(None)),
    }
}

// ============================================================================
// connect_framed() - For message transports (WebSocket) with reconnection
// ============================================================================

/// Connect using a message transport with automatic reconnection.
///
/// Like [`connect()`], but for transports that already provide message framing
/// (like WebSocket). No COBS framing is applied.
///
/// Returns a client that automatically reconnects on failure.
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

/// Internal connection state for Client.
struct ClientState {
    handle: ConnectionHandle,
    driver_handle: JoinHandle<Result<(), ConnectionError>>,
}

impl ClientState {
    fn is_alive(&self) -> bool {
        !self.driver_handle.is_finished()
    }
}

/// A client that automatically reconnects on transport failure.
///
/// Created by [`connect()`]. Implements [`Caller`](roam_session::Caller) so it works
/// with generated service clients.
///
/// Cloning is cheap - all clones share the same connection state.
pub struct Client<C, D> {
    connector: Arc<C>,
    config: HandshakeConfig,
    dispatcher: D,
    retry_policy: RetryPolicy,
    state: Arc<Mutex<Option<ClientState>>>,
}

impl<C, D> Clone for Client<C, D>
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

impl<C, D> Client<C, D>
where
    C: Connector,
    D: ServiceDispatcher + Clone + 'static,
{
    /// Get the underlying handle if connected.
    ///
    /// Prefer using the `Caller` trait methods directly instead.
    pub async fn handle(&self) -> Result<ConnectionHandle, ConnectError> {
        self.ensure_connected().await
    }

    async fn ensure_connected(&self) -> Result<ConnectionHandle, ConnectError> {
        let mut state = self.state.lock().await;

        // Check if we have a live connection
        if let Some(ref conn) = *state {
            if conn.is_alive() {
                return Ok(conn.handle.clone());
            }
            *state = None;
        }

        // Connect
        let conn = self.connect_internal().await?;
        let handle = conn.handle.clone();
        *state = Some(conn);
        Ok(handle)
    }

    async fn connect_internal(&self) -> Result<ClientState, ConnectError> {
        let stream = self
            .connector
            .connect()
            .await
            .map_err(ConnectError::ConnectFailed)?;

        let framed = CobsFramed::new(stream);

        let (handle, driver) = establish(
            framed,
            self.config.to_hello(),
            self.dispatcher.clone(),
            Role::Initiator,
        )
        .await
        .map_err(|e| ConnectError::ConnectFailed(connection_error_to_io(e)))?;

        let driver_handle = tokio::spawn(async move { driver.run().await });

        Ok(ClientState {
            handle,
            driver_handle,
        })
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
                    tokio::time::sleep(backoff).await;
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
                    tokio::time::sleep(backoff).await;
                }
            }
        }
    }
}

impl<C, D> roam_session::Caller for Client<C, D>
where
    C: Connector,
    D: ServiceDispatcher + Clone + 'static,
{
    async fn call<T: Facet<'static>>(
        &self,
        method_id: u64,
        args: &mut T,
    ) -> Result<Vec<u8>, TransportError> {
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
                    tokio::time::sleep(backoff).await;
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
                    tokio::time::sleep(backoff).await;
                }
            }
        }
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
// FramedClient - For message transports (WebSocket) with reconnection
// ============================================================================

/// A client for message transports that automatically reconnects on failure.
///
/// Created by [`connect_framed()`]. Like [`Client`], but for transports that
/// already provide message framing (like WebSocket).
///
/// Implements [`Caller`](roam_session::Caller) so it works with generated service clients.
pub struct FramedClient<C, D> {
    connector: Arc<C>,
    config: HandshakeConfig,
    dispatcher: D,
    retry_policy: RetryPolicy,
    state: Arc<Mutex<Option<ClientState>>>,
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
            if conn.is_alive() {
                return Ok(conn.handle.clone());
            }
            *state = None;
        }

        let conn = self.connect_internal().await?;
        let handle = conn.handle.clone();
        *state = Some(conn);
        Ok(handle)
    }

    async fn connect_internal(&self) -> Result<ClientState, ConnectError> {
        let transport = self
            .connector
            .connect()
            .await
            .map_err(ConnectError::ConnectFailed)?;

        // No COBS framing - transport already provides message framing
        let (handle, driver) = establish(
            transport,
            self.config.to_hello(),
            self.dispatcher.clone(),
            Role::Initiator,
        )
        .await
        .map_err(|e| ConnectError::ConnectFailed(connection_error_to_io(e)))?;

        let driver_handle = tokio::spawn(async move { driver.run().await });

        Ok(ClientState {
            handle,
            driver_handle,
        })
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
                    tokio::time::sleep(backoff).await;
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
                    tokio::time::sleep(backoff).await;
                }
            }
        }
    }
}

impl<C, D> roam_session::Caller for FramedClient<C, D>
where
    C: MessageConnector,
    D: ServiceDispatcher + Clone + 'static,
{
    async fn call<T: Facet<'static>>(
        &self,
        method_id: u64,
        args: &mut T,
    ) -> Result<Vec<u8>, TransportError> {
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
                    tokio::time::sleep(backoff).await;
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
                    tokio::time::sleep(backoff).await;
                }
            }
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
    /// Single channel ensures FIFO ordering.
    driver_rx: mpsc::Receiver<DriverMessage>,
    server_channel_registry: ChannelRegistry,
    pending_responses: HashMap<u64, oneshot::Sender<Result<Vec<u8>, TransportError>>>,
    in_flight_server_requests: std::collections::HashSet<u64>,
}

impl<T, D> Driver<T, D>
where
    T: MessageTransport,
    D: ServiceDispatcher,
{
    /// Run the driver until the connection closes.
    pub async fn run(mut self) -> Result<(), ConnectionError> {
        loop {
            tokio::select! {
                biased;

                // All messages go through a single channel - FIFO ordering guaranteed
                Some(msg) = self.driver_rx.recv() => {
                    self.handle_driver_message(msg).await?;
                }

                result = self.io.recv() => {
                    match self.handle_recv(result).await {
                        Ok(true) => continue,
                        Ok(false) => return Ok(()),
                        Err(e) => return Err(e),
                    }
                }
            }
        }
    }

    /// Handle a message from the unified driver channel.
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
                payload,
            } => {
                // Only send response if request is still in-flight
                if !self.in_flight_server_requests.remove(&request_id) {
                    return Ok(());
                }
                let wire_msg = Message::Response {
                    request_id,
                    metadata: vec![],
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
                payload,
                ..
            } => {
                if let Some(tx) = self.pending_responses.remove(&request_id) {
                    let _ = tx.send(Ok(payload));
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
        tokio::spawn(handler_fut);
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
// Internal: establish() - Perform handshake and create driver
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

    // Wait for peer Hello
    // r[impl message.hello.unknown-version] - Check for unknown Hello versions during handshake.
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
            // Check if this is an unknown Hello version
            let raw = io.last_decoded();
            let is_unknown_hello = raw.len() >= 2 && raw[0] == 0x00 && raw[1] != 0x00;
            let version = if is_unknown_hello { raw[1] } else { 0 };

            if is_unknown_hello {
                // r[impl message.hello.unknown-version] - Unknown Hello version triggers Goodbye.
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

    // Create single unified channel for all messages (Call/Data/Close/Response).
    // Single channel ensures FIFO ordering.
    let (driver_tx, driver_rx) = mpsc::channel(256);

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
// No-op dispatcher for client-only connections
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

// ============================================================================
// Legacy compatibility (to be removed)
// ============================================================================

/// Establish a bidirectional connection as the acceptor.
#[deprecated(note = "Use accept() or accept_framed() instead")]
#[allow(deprecated)]
pub async fn establish_acceptor<T, D>(
    io: T,
    our_hello: Hello,
    dispatcher: D,
) -> Result<(ConnectionHandle, Driver<T, D>), ConnectionError>
where
    T: MessageTransport,
    D: ServiceDispatcher,
{
    establish(io, our_hello, dispatcher, Role::Acceptor).await
}

/// Establish a bidirectional connection as the initiator.
#[deprecated(note = "Use connect() instead")]
#[allow(deprecated)]
pub async fn establish_initiator<T, D>(
    io: T,
    our_hello: Hello,
    dispatcher: D,
) -> Result<(ConnectionHandle, Driver<T, D>), ConnectionError>
where
    T: MessageTransport,
    D: ServiceDispatcher,
{
    establish(io, our_hello, dispatcher, Role::Initiator).await
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
