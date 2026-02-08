//! Bidirectional connection driver for message-based transports.
//!
//! This module provides the core connection handling for roam over transports
//! that already provide message framing (like WebSocket).
//!
//! For byte-stream transports (TCP, Unix sockets), see `roam-stream` which
//! wraps streams in length-prefixed framing before using this driver.
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
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

use facet::Facet;

use crate::runtime::{Mutex, Receiver, channel, sleep, spawn, spawn_with_abort};
use crate::{
    ChannelError, ChannelRegistry, ConnectionHandle, Context, DriverMessage, MessageTransport,
    ResponseData, RoamError, Role, ServiceDispatcher, TransportError,
};
use roam_wire::{ConnectionId, Hello, Message};

/// Negotiated connection parameters after Hello exchange.
#[derive(Debug, Clone)]
pub struct Negotiated {
    /// Effective max payload size (min of both peers).
    pub max_payload_size: u32,
    /// Initial channel credit (min of both peers).
    pub initial_credit: u32,
    /// Maximum concurrent in-flight requests per connection (min of both peers).
    pub max_concurrent_requests: u32,
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
    /// Unsupported protocol version (peer sent V1 or V2 Hello).
    UnsupportedProtocolVersion,
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
            ConnectionError::UnsupportedProtocolVersion => {
                write!(f, "unsupported protocol version (expected V4 or V5)")
            }
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
    /// Maximum in-flight concurrent requests per connection.
    pub max_concurrent_requests: u32,
}

impl Default for HandshakeConfig {
    fn default() -> Self {
        Self {
            max_payload_size: 1024 * 1024,     // 1 MiB
            initial_channel_credit: 64 * 1024, // 64 KiB
            max_concurrent_requests: 64,
        }
    }
}

impl HandshakeConfig {
    /// Convert to Hello message (v5 format).
    pub fn to_hello(&self) -> Hello {
        Hello::V5 {
            max_payload_size: self.max_payload_size,
            initial_channel_credit: self.initial_channel_credit,
            max_concurrent_requests: self.max_concurrent_requests,
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
    /// Virtual connection request was rejected by the remote peer.
    Rejected(String),
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
            ConnectError::Rejected(reason) => write!(f, "connection rejected: {reason}"),
        }
    }
}

impl std::error::Error for ConnectError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ConnectError::RetriesExhausted { original, .. } => Some(original),
            ConnectError::ConnectFailed(e) => Some(e),
            ConnectError::Rpc(e) => Some(e),
            ConnectError::Rejected(_) => None,
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
/// Returns:
/// - A handle for making calls on connection 0 (root)
/// - A receiver for incoming virtual connection requests
/// - A driver that must be spawned
///
/// The `IncomingConnections` receiver allows accepting sub-connections opened
/// by the remote peer. If you don't need sub-connections, you can drop it and
/// all Connect requests will be automatically rejected.
///
/// # Example
///
/// ```ignore
/// let (handle, incoming, driver) = accept_framed(transport, config, dispatcher).await?;
///
/// // Spawn the driver
/// spawn(driver.run());
///
/// // Optionally handle incoming connections in another task
/// spawn(async move {
///     while let Some(conn) = incoming.recv().await {
///         let sub_handle = conn.accept(vec![]).await?;
///         // Use sub_handle for this virtual connection...
///     }
/// });
///
/// // Use handle for calls on the root connection
/// let response = handle.call_raw(method_id, payload).await?;
/// ```
pub async fn accept_framed<T, D>(
    transport: T,
    config: HandshakeConfig,
    dispatcher: D,
) -> Result<(ConnectionHandle, IncomingConnections, Driver<T, D>), ConnectionError>
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

        let (handle, _incoming, driver) = establish(
            transport,
            self.config.to_hello(),
            self.dispatcher.clone(),
            Role::Initiator,
        )
        .await
        .map_err(|e| ConnectError::ConnectFailed(connection_error_to_io(e)))?;

        // Note: We drop `_incoming` - this client doesn't accept sub-connections.
        // Any Connect requests from the server will be automatically rejected.

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
    async fn call_with_metadata<T: Facet<'static> + Send>(
        &self,
        method_id: u64,
        args: &mut T,
        metadata: roam_wire::Metadata,
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
                Err(ConnectError::Rejected(_)) => {
                    // Virtual connection rejected - this shouldn't happen for link-level connect
                    return Err(TransportError::ConnectionClosed);
                }
            };

            match handle
                .call_with_metadata(method_id, args, metadata.clone())
                .await
            {
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

    fn bind_response_channels<R: Facet<'static>>(&self, response: &mut R, channels: &[u64]) {
        // FramedClient wraps a ConnectionHandle, but we don't have direct access to it
        // during bind_response_channels. For reconnecting clients, response channel binding
        // would need to be handled at a higher level or the client would need to store
        // the current handle.
        // For now, this is a no-op - FramedClient users should use ConnectionHandle
        // directly if they need response channel binding.
        let _ = (response, channels);
    }

    #[allow(unsafe_code)]
    #[cfg(not(target_arch = "wasm32"))]
    fn call_with_metadata_by_plan(
        &self,
        method_id: u64,
        args_ptr: crate::SendPtr,
        args_plan: &'static std::sync::Arc<crate::RpcPlan>,
        metadata: roam_wire::Metadata,
    ) -> impl std::future::Future<Output = Result<ResponseData, TransportError>> + Send {
        // Capture self for use in async block
        let this = self.clone();

        async move {
            let mut attempt = 0u32;

            loop {
                let handle = match this.ensure_connected().await {
                    Ok(h) => h,
                    Err(ConnectError::ConnectFailed(_)) => {
                        attempt += 1;
                        if attempt >= this.retry_policy.max_attempts {
                            return Err(TransportError::ConnectionClosed);
                        }
                        let backoff = this.retry_policy.backoff_for_attempt(attempt);
                        sleep(backoff).await;
                        continue;
                    }
                    Err(ConnectError::RetriesExhausted { .. }) => {
                        return Err(TransportError::ConnectionClosed);
                    }
                    Err(ConnectError::Rpc(e)) => return Err(e),
                    Err(ConnectError::Rejected(_)) => {
                        return Err(TransportError::ConnectionClosed);
                    }
                };

                // SAFETY: args_ptr was created from valid, initialized, Send data
                match unsafe {
                    handle.call_with_metadata_by_plan(
                        method_id,
                        args_ptr.as_ptr(),
                        args_plan,
                        metadata.clone(),
                    )
                }
                .await
                {
                    Ok(response) => return Ok(response),
                    Err(TransportError::Encode(e)) => {
                        return Err(TransportError::Encode(e));
                    }
                    Err(TransportError::ConnectionClosed) | Err(TransportError::DriverGone) => {
                        {
                            let mut state = this.state.lock().await;
                            *state = None;
                        }

                        attempt += 1;
                        if attempt >= this.retry_policy.max_attempts {
                            return Err(TransportError::ConnectionClosed);
                        }

                        let backoff = this.retry_policy.backoff_for_attempt(attempt);
                        sleep(backoff).await;
                    }
                }
            }
        }
    }

    #[allow(unsafe_code)]
    #[cfg(target_arch = "wasm32")]
    fn call_with_metadata_by_plan(
        &self,
        method_id: u64,
        args_ptr: crate::SendPtr,
        args_plan: &'static std::sync::Arc<crate::RpcPlan>,
        metadata: roam_wire::Metadata,
    ) -> impl std::future::Future<Output = Result<ResponseData, TransportError>> {
        // Capture self for use in async block
        let this = self.clone();

        async move {
            let mut attempt = 0u32;

            loop {
                let handle = match this.ensure_connected().await {
                    Ok(h) => h,
                    Err(ConnectError::ConnectFailed(_)) => {
                        attempt += 1;
                        if attempt >= this.retry_policy.max_attempts {
                            return Err(TransportError::ConnectionClosed);
                        }
                        let backoff = this.retry_policy.backoff_for_attempt(attempt);
                        sleep(backoff).await;
                        continue;
                    }
                    Err(ConnectError::RetriesExhausted { .. }) => {
                        return Err(TransportError::ConnectionClosed);
                    }
                    Err(ConnectError::Rpc(e)) => return Err(e),
                    Err(ConnectError::Rejected(_)) => {
                        return Err(TransportError::ConnectionClosed);
                    }
                };

                // SAFETY: args_ptr was created from valid, initialized, Send data
                match unsafe {
                    handle.call_with_metadata_by_plan(
                        method_id,
                        args_ptr.as_ptr(),
                        args_plan,
                        metadata.clone(),
                    )
                }
                .await
                {
                    Ok(response) => return Ok(response),
                    Err(TransportError::Encode(e)) => {
                        return Err(TransportError::Encode(e));
                    }
                    Err(TransportError::ConnectionClosed) | Err(TransportError::DriverGone) => {
                        {
                            let mut state = this.state.lock().await;
                            *state = None;
                        }

                        attempt += 1;
                        if attempt >= this.retry_policy.max_attempts {
                            return Err(TransportError::ConnectionClosed);
                        }

                        let backoff = this.retry_policy.backoff_for_attempt(attempt);
                        sleep(backoff).await;
                    }
                }
            }
        }
    }

    #[allow(unsafe_code)]
    unsafe fn bind_response_channels_by_plan(
        &self,
        response_ptr: *mut (),
        response_plan: &crate::RpcPlan,
        channels: &[u64],
    ) {
        // Same as bind_response_channels - this is a no-op for FramedClient.
        // Users should use ConnectionHandle directly if they need response channel binding.
        let _ = (response_ptr, response_plan, channels);
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
        ConnectionError::UnsupportedProtocolVersion => io::Error::new(
            io::ErrorKind::InvalidData,
            "unsupported protocol version (expected V4 or V5)",
        ),
    }
}

// ============================================================================
// Virtual Connection State
// ============================================================================

/// State for a single virtual connection on a link.
///
/// Each virtual connection has its own request ID space, channel ID space,
/// and dispatcher instance. Connection 0 (ROOT) is created implicitly on
/// link establishment. Additional connections are opened via Connect/Accept.
///
/// r[impl core.conn.independence]
struct ConnectionState {
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
    pending_responses: HashMap<u64, PendingResponse>,
    /// In-flight server requests with their abort handles.
    /// r[impl call.cancel.best-effort] - We track abort handles to allow best-effort cancellation.
    in_flight_server_requests: HashMap<u64, crate::runtime::AbortHandle>,
}

struct PendingResponse {
    #[cfg(not(target_arch = "wasm32"))]
    created_at: Instant,
    #[cfg(not(target_arch = "wasm32"))]
    warned_stale: bool,
    tx: crate::runtime::OneshotSender<Result<ResponseData, TransportError>>,
}

impl ConnectionState {
    /// Create a new connection state.
    fn new(
        conn_id: ConnectionId,
        driver_tx: crate::runtime::Sender<DriverMessage>,
        role: Role,
        initial_credit: u32,
        max_concurrent_requests: u32,
        diagnostic_state: Option<Arc<crate::diagnostic::DiagnosticState>>,
        dispatcher: Option<Box<dyn ServiceDispatcher>>,
    ) -> Self {
        let handle = ConnectionHandle::new_with_diagnostics_and_limits(
            conn_id,
            driver_tx.clone(),
            role,
            initial_credit,
            max_concurrent_requests,
            diagnostic_state,
        );
        let server_channel_registry =
            ChannelRegistry::new_with_credit_and_role(conn_id, initial_credit, driver_tx, role);
        Self {
            conn_id,
            handle,
            server_channel_registry,
            dispatcher,
            pending_responses: HashMap::new(),
            in_flight_server_requests: HashMap::new(),
        }
    }

    /// Fail all pending responses (connection closing).
    fn fail_pending_responses(&mut self) {
        for (_, pending) in self.pending_responses.drain() {
            let _ = pending.tx.send(Err(TransportError::ConnectionClosed));
        }
    }

    /// Abort all in-flight server requests (connection closing).
    fn abort_in_flight_requests(&mut self) {
        for (_, abort_handle) in self.in_flight_server_requests.drain() {
            abort_handle.abort();
        }
    }
}

/// An incoming virtual connection request.
///
/// Received via the `IncomingConnections` receiver returned from `accept_framed()`.
/// Call `accept()` to accept the connection and get a handle,
/// or `reject()` to refuse it.
pub struct IncomingConnection {
    /// The request ID for this Connect request.
    request_id: u64,
    /// Metadata from the Connect message.
    pub metadata: roam_wire::Metadata,
    /// Channel to send the Accept/Reject response.
    response_tx: crate::runtime::OneshotSender<IncomingConnectionResponse>,
}

impl IncomingConnection {
    /// Accept this connection and receive a handle for it.
    ///
    /// The `metadata` will be sent in the Accept message.
    ///
    /// The `dispatcher` will handle incoming requests on this virtual connection.
    /// If None, the parent link's dispatcher will be used (and only calls can be made,
    /// not received).
    ///
    /// Note: The returned `ConnectionHandle` cannot itself accept nested connections.
    /// r[impl core.conn.only-root-accepts]
    pub async fn accept(
        self,
        metadata: roam_wire::Metadata,
        dispatcher: Option<Box<dyn ServiceDispatcher>>,
    ) -> Result<ConnectionHandle, TransportError> {
        let (handle_tx, handle_rx) = crate::runtime::oneshot();
        let _ = self.response_tx.send(IncomingConnectionResponse::Accept {
            request_id: self.request_id,
            metadata,
            dispatcher,
            handle_tx,
        });
        let result: Result<ConnectionHandle, _> =
            handle_rx.await.map_err(|_| TransportError::DriverGone)?;
        result
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
enum IncomingConnectionResponse {
    Accept {
        request_id: u64,
        metadata: roam_wire::Metadata,
        dispatcher: Option<Box<dyn ServiceDispatcher>>,
        handle_tx: crate::runtime::OneshotSender<Result<ConnectionHandle, TransportError>>,
    },
    Reject {
        request_id: u64,
        reason: String,
        metadata: roam_wire::Metadata,
    },
}

/// Pending outgoing Connect request.
struct PendingConnect {
    /// Channel to send the response handle.
    response_tx: crate::runtime::OneshotSender<Result<ConnectionHandle, ConnectError>>,
    /// Dispatcher to use for this virtual connection (can receive calls).
    dispatcher: Option<Box<dyn ServiceDispatcher>>,
}

// ============================================================================
// Driver - The core connection loop
// ============================================================================

/// The connection driver - a future that handles bidirectional RPC.
///
/// This must be spawned or awaited to drive the connection forward.
///
/// The driver manages multiple virtual connections on a single link.
/// Connection 0 (ROOT) is created implicitly. Additional connections
/// can be opened via `Connect`/`Accept` messages.
pub struct Driver<T, D> {
    io: T,
    dispatcher: D,
    #[allow(dead_code)]
    role: Role,
    negotiated: Negotiated,
    /// Unified channel for all messages (Call/Data/Close/Response).
    driver_rx: Receiver<DriverMessage>,
    /// Sender for driver messages (passed to new connections).
    driver_tx: crate::runtime::Sender<DriverMessage>,
    /// All virtual connections on this link, keyed by conn_id.
    connections: HashMap<ConnectionId, ConnectionState>,
    /// Next connection ID to allocate (for Accept responses).
    /// r[impl core.conn.id-allocation]
    next_conn_id: u64,
    /// Pending outgoing Connect requests (request_id -> response channel).
    pending_connects: HashMap<u64, PendingConnect>,
    /// Channel for incoming connection requests (only root can accept).
    /// r[impl core.conn.accept-required]
    incoming_connections_tx: Option<crate::runtime::Sender<IncomingConnection>>,
    /// Channel for incoming connection responses.
    incoming_response_rx: Option<Receiver<IncomingConnectionResponse>>,
    incoming_response_tx: crate::runtime::Sender<IncomingConnectionResponse>,
    /// Diagnostic state for debugging.
    diagnostic_state: Option<Arc<crate::diagnostic::DiagnosticState>>,
}

#[cfg(not(target_arch = "wasm32"))]
const PENDING_RESPONSE_SWEEP_INTERVAL: Duration = Duration::from_secs(5);
#[cfg(not(target_arch = "wasm32"))]
const PENDING_RESPONSE_WARN_AFTER: Duration = Duration::from_secs(30);
#[cfg(not(target_arch = "wasm32"))]
const PENDING_RESPONSE_KILL_AFTER: Duration = Duration::from_secs(60);

impl<T, D> Driver<T, D>
where
    T: MessageTransport,
    D: ServiceDispatcher,
{
    /// Get the handle for the root connection (connection 0).
    ///
    /// This is the main handle returned from `establish()` and should be used
    /// for most operations. Virtual connections can be obtained via `connect()`.
    pub fn root_handle(&self) -> ConnectionHandle {
        self.connections
            .get(&ConnectionId::ROOT)
            .expect("root connection always exists")
            .handle
            .clone()
    }

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

                // Handle incoming connection accept/reject responses
                response = async {
                    if let Some(rx) = &mut self.incoming_response_rx {
                        rx.recv().await
                    } else {
                        std::future::pending().await
                    }
                }.fuse() => {
                    if let Some(response) = response {
                        self.handle_incoming_response(response).await?;
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

    /// Handle an Accept/Reject response from application code.
    async fn handle_incoming_response(
        &mut self,
        response: IncomingConnectionResponse,
    ) -> Result<(), ConnectionError> {
        match response {
            IncomingConnectionResponse::Accept {
                request_id,
                metadata,
                dispatcher,
                handle_tx,
            } => {
                // Allocate a new connection ID
                // r[impl core.conn.id-allocation]
                let conn_id = ConnectionId::new(self.next_conn_id);
                self.next_conn_id += 1;

                // Create connection state
                let conn_state = ConnectionState::new(
                    conn_id,
                    self.driver_tx.clone(),
                    self.role,
                    self.negotiated.initial_credit,
                    self.negotiated.max_concurrent_requests,
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
                self.io.send(&msg).await?;

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
                self.io.send(&msg).await?;
            }
        }
        Ok(())
    }

    async fn handle_driver_message(&mut self, msg: DriverMessage) -> Result<(), ConnectionError> {
        match msg {
            DriverMessage::Call {
                conn_id,
                request_id,
                method_id,
                metadata,
                channels,
                payload,
                response_tx,
            } => {
                // Store pending response in the connection's state
                if let Some(conn) = self.connections.get_mut(&conn_id) {
                    conn.pending_responses.insert(
                        request_id,
                        PendingResponse {
                            #[cfg(not(target_arch = "wasm32"))]
                            created_at: Instant::now(),
                            #[cfg(not(target_arch = "wasm32"))]
                            warned_stale: false,
                            tx: response_tx,
                        },
                    );
                } else {
                    // Unknown connection - fail the call
                    let _ = response_tx.send(Err(TransportError::ConnectionClosed));
                    return Ok(());
                }
                let req = Message::Request {
                    conn_id,
                    request_id,
                    method_id,
                    metadata,
                    channels,
                    payload,
                };
                self.io.send(&req).await?;
            }
            DriverMessage::Data {
                conn_id,
                channel_id,
                payload,
            } => {
                let wire_msg = Message::Data {
                    conn_id,
                    channel_id,
                    payload,
                };
                self.io.send(&wire_msg).await?;
            }
            DriverMessage::Close {
                conn_id,
                channel_id,
            } => {
                let wire_msg = Message::Close {
                    conn_id,
                    channel_id,
                };
                self.io.send(&wire_msg).await?;
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
                // r[impl flow.call.payload-limit] - Outgoing responses are also bounded
                // by max_payload_size. If a handler produces a too-large response, send
                // a Cancelled error instead so the call doesn't hang.
                let (payload, channels) = if payload.len() as u32 > self.negotiated.max_payload_size
                {
                    error!(
                        conn_id = conn_id.raw(),
                        request_id,
                        payload_len = payload.len(),
                        max_payload_size = self.negotiated.max_payload_size,
                        "outgoing response exceeds max_payload_size, sending Cancelled"
                    );
                    // Cancelled error: Result::Err(1) + RoamError::Cancelled(3)
                    (vec![1, 3], vec![])
                } else {
                    (payload, channels)
                };
                let wire_msg = Message::Response {
                    conn_id,
                    request_id,
                    metadata: vec![],
                    channels,
                    payload,
                };
                self.io.send(&wire_msg).await?;
            }
            DriverMessage::Connect {
                request_id,
                metadata,
                response_tx,
                dispatcher,
            } => {
                // r[impl message.connect.initiate]
                // r[impl message.connect.request-id]
                // r[impl message.connect.metadata]
                // Store pending connect request
                self.pending_connects.insert(
                    request_id,
                    PendingConnect {
                        response_tx,
                        dispatcher,
                    },
                );
                // Send Connect message
                let wire_msg = Message::Connect {
                    request_id,
                    metadata,
                };
                self.io.send(&wire_msg).await?;
            }
            DriverMessage::SweepPendingResponses => {
                if self.sweep_pending_response_staleness() {
                    return Err(self.goodbye("call.response.stale-timeout").await);
                }
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
                if !raw.is_empty() && raw[0] >= 12 {
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
            Message::Hello(_) => {
                // Already handled during handshake, ignore duplicates
            }
            Message::Connect {
                request_id,
                metadata,
            } => {
                // r[impl core.conn.accept-required]
                // Only root connection can accept incoming connections
                if let Some(tx) = &self.incoming_connections_tx {
                    // Create a oneshot that routes through incoming_response_tx
                    let (response_tx, response_rx) = crate::runtime::oneshot();
                    let incoming = IncomingConnection {
                        request_id,
                        metadata,
                        response_tx,
                    };
                    if tx.try_send(incoming).is_ok() {
                        // Spawn a task to forward the response
                        let incoming_response_tx = self.incoming_response_tx.clone();
                        spawn(async move {
                            if let Ok(response) = response_rx.await {
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
                        self.io.send(&msg).await?;
                    }
                } else {
                    // Not listening - reject
                    // r[impl message.reject.response]
                    let msg = Message::Reject {
                        request_id,
                        reason: "not listening".into(),
                        metadata: vec![],
                    };
                    self.io.send(&msg).await?;
                }
            }
            Message::Accept {
                request_id,
                conn_id,
                metadata: _,
            } => {
                // r[impl message.accept.response]
                // r[impl message.accept.metadata]
                // r[impl core.conn.id-allocation]
                // Handle response to our outgoing Connect request
                if let Some(pending) = self.pending_connects.remove(&request_id) {
                    // Create connection state for the new virtual connection
                    // r[impl core.conn.dispatcher-custom]
                    // Use the dispatcher provided by the initiator
                    let conn_state = ConnectionState::new(
                        conn_id,
                        self.driver_tx.clone(),
                        self.role,
                        self.negotiated.initial_credit,
                        self.negotiated.max_concurrent_requests,
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
                // r[impl message.reject.response]
                // r[impl message.reject.reason]
                // Handle rejection of our outgoing Connect request
                if let Some(pending) = self.pending_connects.remove(&request_id) {
                    let _ = pending
                        .response_tx
                        .send(Err(ConnectError::Rejected(reason)));
                }
                // Unknown request_id - ignore
            }
            Message::Goodbye { conn_id, reason: _ } => {
                // r[impl message.goodbye.connection-zero]
                if conn_id.is_root() {
                    // Goodbye on root closes entire link
                    for (_, mut conn) in self.connections.drain() {
                        conn.fail_pending_responses();
                        conn.abort_in_flight_requests();
                    }
                    return Err(ConnectionError::Closed);
                } else {
                    // Close just this virtual connection
                    // r[impl core.conn.lifecycle]
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
                // Route to the correct connection
                if let Some(conn) = self.connections.get_mut(&conn_id)
                    && let Some(pending_response) = conn.pending_responses.remove(&request_id)
                {
                    if pending_response
                        .tx
                        .send(Ok(ResponseData { payload, channels }))
                        .is_err()
                    {
                        warn!(
                            conn_id = conn_id.raw(),
                            request_id, "response receiver dropped before delivery"
                        );
                    }
                } else if !self.connections.contains_key(&conn_id) {
                    warn!(
                        conn_id = conn_id.raw(),
                        request_id, "received response for unknown conn_id"
                    );
                    return Err(self.goodbye("message.conn-id").await);
                } else {
                    warn!(
                        conn_id = conn_id.raw(),
                        request_id, "received response for unknown request_id - protocol violation"
                    );
                    return Err(self.goodbye("call.response.unknown-request-id").await);
                }
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
            Message::Credit {
                conn_id,
                channel_id,
                bytes,
            } => {
                self.handle_credit(conn_id, channel_id, bytes)?;
            }
        }
        Ok(())
    }

    async fn handle_incoming_request(
        &mut self,
        conn_id: ConnectionId,
        request_id: u64,
        method_id: u64,
        metadata: roam_wire::Metadata,
        channels: Vec<u64>,
        payload: Vec<u8>,
    ) -> Result<(), ConnectionError> {
        // Get or validate the connection
        let conn = match self.connections.get_mut(&conn_id) {
            Some(c) => c,
            None => {
                // r[impl message.conn-id] - Unknown conn_id is a protocol error
                return Err(self.goodbye("message.conn-id").await);
            }
        };

        // r[impl call.request-id.duplicate-detection]
        if conn.in_flight_server_requests.contains_key(&request_id) {
            return Err(self.goodbye("call.request-id.duplicate-detection").await);
        }
        if conn.in_flight_server_requests.len() >= self.negotiated.max_concurrent_requests as usize
        {
            return Err(self.goodbye("flow.request.concurrent-overrun").await);
        }

        if let Err(rule_id) = roam_wire::validate_metadata(&metadata) {
            return Err(self.goodbye(rule_id).await);
        }

        if payload.len() as u32 > self.negotiated.max_payload_size {
            return Err(self.goodbye("flow.call.payload-limit").await);
        }

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

        let handler_fut = dispatcher.dispatch(cx, payload, &mut conn.server_channel_registry);

        // r[impl call.cancel.best-effort] - Store abort handle for cancellation support
        let abort_handle = spawn_with_abort(async move {
            handler_fut.await;
        });
        conn.in_flight_server_requests
            .insert(request_id, abort_handle);
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
    ) -> Result<(), ConnectionError> {
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

    async fn handle_data(
        &mut self,
        conn_id: ConnectionId,
        channel_id: u64,
        payload: Vec<u8>,
    ) -> Result<(), ConnectionError> {
        if channel_id == 0 {
            return Err(self.goodbye("channeling.id.zero-reserved").await);
        }

        if payload.len() as u32 > self.negotiated.max_payload_size {
            return Err(self.goodbye("flow.call.payload-limit").await);
        }

        // Find the connection and route data
        let conn = match self.connections.get_mut(&conn_id) {
            Some(c) => c,
            None => return Err(self.goodbye("message.conn-id").await),
        };

        let result = if conn.server_channel_registry.contains_incoming(channel_id) {
            conn.server_channel_registry
                .route_data(channel_id, payload)
                .await
        } else if conn.handle.contains_channel(channel_id) {
            conn.handle.route_data(channel_id, payload).await
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

    async fn handle_close(
        &mut self,
        conn_id: ConnectionId,
        channel_id: u64,
    ) -> Result<(), ConnectionError> {
        if channel_id == 0 {
            return Err(self.goodbye("channeling.id.zero-reserved").await);
        }

        let conn = match self.connections.get_mut(&conn_id) {
            Some(c) => c,
            None => return Err(self.goodbye("message.conn-id").await),
        };

        if conn.server_channel_registry.contains(channel_id) {
            conn.server_channel_registry.close(channel_id);
        } else if conn.handle.contains_channel(channel_id) {
            conn.handle.close_channel(channel_id);
        } else {
            return Err(self.goodbye("channeling.unknown").await);
        }
        Ok(())
    }

    fn handle_reset(
        &mut self,
        conn_id: ConnectionId,
        channel_id: u64,
    ) -> Result<(), ConnectionError> {
        if let Some(conn) = self.connections.get_mut(&conn_id) {
            if conn.server_channel_registry.contains(channel_id) {
                conn.server_channel_registry.reset(channel_id);
            } else if conn.handle.contains_channel(channel_id) {
                conn.handle.reset_channel(channel_id);
            }
        }
        Ok(())
    }

    fn handle_credit(
        &mut self,
        conn_id: ConnectionId,
        channel_id: u64,
        bytes: u32,
    ) -> Result<(), ConnectionError> {
        if let Some(conn) = self.connections.get_mut(&conn_id) {
            if conn.server_channel_registry.contains(channel_id) {
                conn.server_channel_registry
                    .receive_credit(channel_id, bytes);
            } else if conn.handle.contains_channel(channel_id) {
                conn.handle.receive_credit(channel_id, bytes);
            }
        }
        Ok(())
    }

    async fn goodbye(&mut self, rule_id: &'static str) -> ConnectionError {
        // Fail all pending responses and abort in-flight requests on all connections
        for (_, conn) in self.connections.iter_mut() {
            conn.fail_pending_responses();
            conn.abort_in_flight_requests();
        }

        let _ = self
            .io
            .send(&Message::Goodbye {
                conn_id: ConnectionId::ROOT,
                reason: rule_id.into(),
            })
            .await;

        ConnectionError::ProtocolViolation {
            rule_id,
            context: String::new(),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[cfg_attr(not(feature = "tracing"), allow(unused_variables))]
    fn sweep_pending_response_staleness(&mut self) -> bool {
        let now = Instant::now();
        let mut timed_out_connections = Vec::new();
        for (conn_id, conn) in self.connections.iter_mut() {
            let conn_id_raw = conn_id.raw();
            let mut should_kill_connection = false;
            for (request_id, pending) in conn.pending_responses.iter_mut() {
                let age = now.saturating_duration_since(pending.created_at);
                if age >= PENDING_RESPONSE_KILL_AFTER {
                    should_kill_connection = true;
                    warn!(
                        conn_id = conn_id_raw,
                        request_id = *request_id,
                        age_ms = age.as_millis(),
                        "pending response exceeded hard timeout"
                    );
                } else if age >= PENDING_RESPONSE_WARN_AFTER && !pending.warned_stale {
                    pending.warned_stale = true;
                    warn!(
                        conn_id = conn_id_raw,
                        request_id = *request_id,
                        age_ms = age.as_millis(),
                        "pending response has gone stale"
                    );
                }
            }
            if should_kill_connection {
                timed_out_connections.push(*conn_id);
            }
        }
        let should_teardown_link = !timed_out_connections.is_empty();
        for conn_id in timed_out_connections {
            if let Some(conn) = self.connections.get_mut(&conn_id) {
                for (request_id, pending) in conn.pending_responses.drain() {
                    warn!(
                        conn_id = conn_id.raw(),
                        request_id,
                        "failing pending response due to stale-timeout connection teardown"
                    );
                    let _ = pending.tx.send(Err(TransportError::ConnectionClosed));
                }
                conn.abort_in_flight_requests();
            }
        }
        should_teardown_link
    }

    #[cfg(target_arch = "wasm32")]
    fn sweep_pending_response_staleness(&mut self) -> bool {
        false
    }
}

// ============================================================================
// initiate_framed() - For initiator role
// ============================================================================

/// Initiate a connection with a pre-framed transport (e.g., WebSocket).
///
/// Use this when establishing a connection as the initiator (client).
/// Returns:
/// - A handle for making calls on connection 0 (root)
/// - A receiver for incoming virtual connection requests
/// - A driver that must be spawned
///
/// For clients that don't need to accept sub-connections, you can drop
/// the `IncomingConnections` receiver and all Connect requests from
/// the server will be automatically rejected.
pub async fn initiate_framed<T, D>(
    transport: T,
    config: HandshakeConfig,
    dispatcher: D,
) -> Result<(ConnectionHandle, IncomingConnections, Driver<T, D>), ConnectionError>
where
    T: MessageTransport,
    D: ServiceDispatcher,
{
    establish(transport, config.to_hello(), dispatcher, Role::Initiator).await
}

// ============================================================================
// establish() - Perform handshake and create driver (internal)
// ============================================================================

/// Receiver for incoming virtual connection requests.
///
/// Returned from `accept_framed()`. Each item is an `IncomingConnection`
/// that can be accepted or rejected.
///
/// If this receiver is dropped, all pending and future Connect requests
/// will be automatically rejected with "not listening".
pub type IncomingConnections = Receiver<IncomingConnection>;

async fn establish<T, D>(
    mut io: T,
    our_hello: Hello,
    dispatcher: D,
    role: Role,
) -> Result<(ConnectionHandle, IncomingConnections, Driver<T, D>), ConnectionError>
where
    T: MessageTransport,
    D: ServiceDispatcher,
{
    // Send Hello
    io.send(&Message::Hello(our_hello.clone())).await?;

    // Wait for peer Hello with timeout
    let peer_hello = match io.recv_timeout(Duration::from_secs(5)).await {
        Ok(Some(Message::Hello(hello))) => hello,
        Ok(Some(_)) => {
            let _ = io
                .send(&Message::Goodbye {
                    conn_id: ConnectionId::ROOT,
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
            // Hello discriminants: V1=0, V2=1, V3=2, V4=3, V5=4. Unknown if > 4.
            let is_unknown_hello = raw.len() >= 2 && raw[0] == 0x00 && raw[1] > 0x04;
            let version = if is_unknown_hello { raw[1] } else { 0 };

            if is_unknown_hello {
                let _ = io
                    .send(&Message::Goodbye {
                        conn_id: ConnectionId::ROOT,
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

    // Negotiate parameters (both sides MUST use V4 or V5).
    let (our_max, our_credit, our_max_concurrent_requests) = match &our_hello {
        Hello::V4 {
            max_payload_size,
            initial_channel_credit,
        } => (*max_payload_size, *initial_channel_credit, u32::MAX),
        Hello::V5 {
            max_payload_size,
            initial_channel_credit,
            max_concurrent_requests,
        } => (
            *max_payload_size,
            *initial_channel_credit,
            *max_concurrent_requests,
        ),
        _ => return Err(ConnectionError::UnsupportedProtocolVersion),
    };
    let (peer_max, peer_credit, peer_max_concurrent_requests) = match &peer_hello {
        Hello::V4 {
            max_payload_size,
            initial_channel_credit,
        } => (*max_payload_size, *initial_channel_credit, u32::MAX),
        Hello::V5 {
            max_payload_size,
            initial_channel_credit,
            max_concurrent_requests,
        } => (
            *max_payload_size,
            *initial_channel_credit,
            *max_concurrent_requests,
        ),
        _ => return Err(ConnectionError::UnsupportedProtocolVersion),
    };

    let negotiated = Negotiated {
        max_payload_size: our_max.min(peer_max),
        initial_credit: our_credit.min(peer_credit),
        max_concurrent_requests: our_max_concurrent_requests.min(peer_max_concurrent_requests),
    };

    debug!(
        max_payload_size = negotiated.max_payload_size,
        initial_credit = negotiated.initial_credit,
        max_concurrent_requests = negotiated.max_concurrent_requests,
        "handshake complete"
    );

    // Create unified channel for all messages
    let (driver_tx, driver_rx) = channel(256);

    // Create root connection (connection 0)
    // r[impl core.link.connection-zero]
    // Root uses None for dispatcher - it uses the link's dispatcher
    let root_conn = ConnectionState::new(
        ConnectionId::ROOT,
        driver_tx.clone(),
        role,
        negotiated.initial_credit,
        negotiated.max_concurrent_requests,
        None,
        None,
    );
    let handle = root_conn.handle.clone();

    let mut connections = HashMap::new();
    connections.insert(ConnectionId::ROOT, root_conn);

    // Create channel for incoming connection requests
    // r[impl core.conn.accept-required]
    let (incoming_connections_tx, incoming_connections_rx) = channel(64);

    // Create channel for incoming connection responses (Accept/Reject from app code)
    let (incoming_response_tx, incoming_response_rx) = channel(64);

    let driver = Driver {
        io,
        dispatcher,
        role,
        negotiated: negotiated.clone(),
        driver_rx,
        driver_tx,
        connections,
        next_conn_id: 1, // 0 is ROOT, start allocating at 1
        pending_connects: HashMap::new(),
        incoming_connections_tx: Some(incoming_connections_tx), // Always created upfront
        incoming_response_rx: Some(incoming_response_rx),
        incoming_response_tx,
        diagnostic_state: None,
    };

    #[cfg(not(target_arch = "wasm32"))]
    {
        let watchdog_tx = driver.driver_tx.clone();
        spawn(async move {
            loop {
                sleep(PENDING_RESPONSE_SWEEP_INTERVAL).await;
                if watchdog_tx
                    .try_send(DriverMessage::SweepPendingResponses)
                    .is_err()
                    && watchdog_tx.is_closed()
                {
                    break;
                }
            }
        });
    }

    Ok((handle, incoming_connections_rx, driver))
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
        cx: Context,
        _payload: Vec<u8>,
        registry: &mut ChannelRegistry,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        let conn_id = cx.conn_id;
        let request_id = cx.request_id.raw();
        let driver_tx = registry.driver_tx();
        Box::pin(async move {
            let response: Result<(), RoamError<()>> = Err(RoamError::UnknownMethod);
            let payload = facet_postcard::to_vec(&response).unwrap_or_default();
            let _ = driver_tx
                .send(DriverMessage::Response {
                    conn_id,
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
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex as StdMutex};
    use std::time::Duration;

    #[test]
    fn test_backoff_calculation() {
        let policy = RetryPolicy::default();
        assert_eq!(policy.backoff_for_attempt(1), Duration::from_millis(100));
        assert_eq!(policy.backoff_for_attempt(2), Duration::from_millis(200));
        assert_eq!(policy.backoff_for_attempt(3), Duration::from_millis(400));
        assert_eq!(policy.backoff_for_attempt(10), Duration::from_secs(5));
    }

    #[derive(Clone)]
    struct TestTransport {
        state: Arc<StdMutex<TestTransportState>>,
        last_decoded: Vec<u8>,
    }

    struct TestTransportState {
        sent: Vec<Message>,
        recv_timeout_queue: VecDeque<std::io::Result<Option<Message>>>,
        recv_queue: VecDeque<std::io::Result<Option<Message>>>,
    }

    impl TestTransport {
        fn scripted(
            recv_timeout_queue: Vec<std::io::Result<Option<Message>>>,
            recv_queue: Vec<std::io::Result<Option<Message>>>,
        ) -> Self {
            Self {
                state: Arc::new(StdMutex::new(TestTransportState {
                    sent: Vec::new(),
                    recv_timeout_queue: recv_timeout_queue.into(),
                    recv_queue: recv_queue.into(),
                })),
                last_decoded: Vec::new(),
            }
        }

        fn sent_messages(&self) -> Vec<Message> {
            self.state.lock().unwrap().sent.clone()
        }
    }

    impl MessageTransport for TestTransport {
        fn send(
            &mut self,
            msg: &Message,
        ) -> impl std::future::Future<Output = std::io::Result<()>> + Send {
            let state = self.state.clone();
            let msg = msg.clone();
            async move {
                state.lock().unwrap().sent.push(msg);
                Ok(())
            }
        }

        fn recv_timeout(
            &mut self,
            _timeout: Duration,
        ) -> impl std::future::Future<Output = std::io::Result<Option<Message>>> + Send {
            let state = self.state.clone();
            async move {
                state
                    .lock()
                    .unwrap()
                    .recv_timeout_queue
                    .pop_front()
                    .unwrap_or(Ok(None))
            }
        }

        fn recv(
            &mut self,
        ) -> impl std::future::Future<Output = std::io::Result<Option<Message>>> + Send {
            let state = self.state.clone();
            async move {
                state
                    .lock()
                    .unwrap()
                    .recv_queue
                    .pop_front()
                    .unwrap_or(Ok(None))
            }
        }

        fn last_decoded(&self) -> &[u8] {
            &self.last_decoded
        }
    }

    #[tokio::test]
    async fn response_with_unknown_conn_id_is_protocol_violation() {
        let peer_hello = Message::Hello(Hello::V4 {
            max_payload_size: 1024 * 1024,
            initial_channel_credit: 64 * 1024,
        });
        let unknown_conn_response = Message::Response {
            conn_id: ConnectionId::new(999),
            request_id: 7,
            metadata: vec![],
            channels: vec![],
            payload: vec![1, 2, 3],
        };
        let transport = TestTransport::scripted(
            vec![Ok(Some(peer_hello))],
            vec![Ok(Some(unknown_conn_response))],
        );
        let probe = transport.clone();

        let (_handle, _incoming, driver) =
            initiate_framed(transport, HandshakeConfig::default(), NoDispatcher)
                .await
                .expect("handshake should succeed");

        let err = driver.run().await.expect_err("driver should fail loudly");
        assert!(matches!(
            err,
            ConnectionError::ProtocolViolation { rule_id, .. } if rule_id == "message.conn-id"
        ));

        let sent = probe.sent_messages();
        assert!(
            sent.iter().any(|msg| matches!(
                msg,
                Message::Goodbye { conn_id, reason }
                    if conn_id.is_root() && reason == "message.conn-id"
            )),
            "driver should send Goodbye(message.conn-id) before closing"
        );
    }

    #[tokio::test]
    async fn response_with_unknown_request_id_is_protocol_violation() {
        let peer_hello = Message::Hello(Hello::V4 {
            max_payload_size: 1024 * 1024,
            initial_channel_credit: 64 * 1024,
        });
        let unknown_request_response = Message::Response {
            conn_id: ConnectionId::ROOT,
            request_id: 4242,
            metadata: vec![],
            channels: vec![],
            payload: vec![9, 9, 9],
        };
        let transport = TestTransport::scripted(
            vec![Ok(Some(peer_hello))],
            vec![Ok(Some(unknown_request_response)), Ok(None)],
        );
        let probe = transport.clone();

        let (_handle, _incoming, driver) =
            initiate_framed(transport, HandshakeConfig::default(), NoDispatcher)
                .await
                .expect("handshake should succeed");

        let err = driver.run().await.expect_err("driver should fail loudly");
        assert!(matches!(
            err,
            ConnectionError::ProtocolViolation { rule_id, .. }
            if rule_id == "call.response.unknown-request-id"
        ));

        let sent = probe.sent_messages();
        assert!(
            sent.iter().any(|msg| matches!(
                msg,
                Message::Goodbye { conn_id, reason }
                    if conn_id.is_root() && reason == "call.response.unknown-request-id"
            )),
            "driver should send Goodbye(call.response.unknown-request-id) before closing"
        );
    }

    #[tokio::test]
    async fn stale_pending_response_triggers_teardown_and_fails_pending() {
        let peer_hello = Message::Hello(Hello::V4 {
            max_payload_size: 1024 * 1024,
            initial_channel_credit: 64 * 1024,
        });
        let transport = TestTransport::scripted(vec![Ok(Some(peer_hello))], vec![]);
        let probe = transport.clone();

        let (_handle, _incoming, mut driver) =
            initiate_framed(transport, HandshakeConfig::default(), NoDispatcher)
                .await
                .expect("handshake should succeed");

        let (response_tx, response_rx) = crate::runtime::oneshot();
        driver
            .connections
            .get_mut(&ConnectionId::ROOT)
            .expect("root connection exists")
            .pending_responses
            .insert(
                1337,
                PendingResponse {
                    created_at: Instant::now()
                        - (PENDING_RESPONSE_KILL_AFTER + Duration::from_secs(1)),
                    warned_stale: true,
                    tx: response_tx,
                },
            );

        let err = driver
            .handle_driver_message(DriverMessage::SweepPendingResponses)
            .await
            .expect_err("sweep should escalate stale pending responses");
        assert!(matches!(
            err,
            ConnectionError::ProtocolViolation { rule_id, .. }
            if rule_id == "call.response.stale-timeout"
        ));

        let pending_result = response_rx
            .await
            .expect("pending response should be failed");
        assert!(matches!(
            pending_result,
            Err(TransportError::ConnectionClosed)
        ));

        let sent = probe.sent_messages();
        assert!(
            sent.iter().any(|msg| matches!(
                msg,
                Message::Goodbye { conn_id, reason }
                    if conn_id.is_root() && reason == "call.response.stale-timeout"
            )),
            "driver should send Goodbye(call.response.stale-timeout) before closing"
        );
    }

    #[tokio::test]
    async fn response_delivery_to_dropped_receiver_is_non_fatal() {
        let peer_hello = Message::Hello(Hello::V4 {
            max_payload_size: 1024 * 1024,
            initial_channel_credit: 64 * 1024,
        });
        let transport = TestTransport::scripted(vec![Ok(Some(peer_hello))], vec![]);
        let probe = transport.clone();

        let (_handle, _incoming, mut driver) =
            initiate_framed(transport, HandshakeConfig::default(), NoDispatcher)
                .await
                .expect("handshake should succeed");

        let (response_tx, response_rx) = crate::runtime::oneshot();
        drop(response_rx);
        driver
            .connections
            .get_mut(&ConnectionId::ROOT)
            .expect("root connection exists")
            .pending_responses
            .insert(
                9001,
                PendingResponse {
                    created_at: Instant::now(),
                    warned_stale: false,
                    tx: response_tx,
                },
            );

        driver
            .handle_message(Message::Response {
                conn_id: ConnectionId::ROOT,
                request_id: 9001,
                metadata: vec![],
                channels: vec![],
                payload: vec![7, 7, 7],
            })
            .await
            .expect("dropped response receiver should not be fatal");

        assert!(
            !driver
                .connections
                .get(&ConnectionId::ROOT)
                .expect("root connection exists")
                .pending_responses
                .contains_key(&9001),
            "pending response should be removed even when receiver was dropped"
        );

        let sent = probe.sent_messages();
        assert!(
            !sent
                .iter()
                .any(|msg| matches!(msg, Message::Goodbye { .. })),
            "dropped receiver path should not send Goodbye"
        );
    }

    #[tokio::test]
    async fn request_concurrency_overrun_sends_goodbye() {
        let peer_hello = Message::Hello(Hello::V4 {
            max_payload_size: 1024 * 1024,
            initial_channel_credit: 64 * 1024,
        });
        let transport = TestTransport::scripted(vec![Ok(Some(peer_hello))], vec![]);
        let probe = transport.clone();

        let config = HandshakeConfig {
            max_concurrent_requests: 1,
            ..HandshakeConfig::default()
        };

        let (_handle, _incoming, mut driver) = initiate_framed(transport, config, NoDispatcher)
            .await
            .expect("handshake should succeed");

        let root = driver
            .connections
            .get_mut(&ConnectionId::ROOT)
            .expect("root connection exists");
        let never_finishes = crate::runtime::spawn_with_abort(async {
            std::future::pending::<()>().await;
        });
        root.in_flight_server_requests.insert(1, never_finishes);

        let err = driver
            .handle_message(Message::Request {
                conn_id: ConnectionId::ROOT,
                request_id: 2,
                method_id: 42,
                metadata: vec![],
                channels: vec![],
                payload: vec![],
            })
            .await
            .expect_err("request overrun should fail connection");

        assert!(matches!(
            err,
            ConnectionError::ProtocolViolation { rule_id, .. }
            if rule_id == "flow.request.concurrent-overrun"
        ));

        let sent = probe.sent_messages();
        assert!(
            sent.iter().any(|msg| matches!(
                msg,
                Message::Goodbye { conn_id, reason }
                    if conn_id.is_root() && reason == "flow.request.concurrent-overrun"
            )),
            "driver should send Goodbye(flow.request.concurrent-overrun) before closing"
        );
    }
}
