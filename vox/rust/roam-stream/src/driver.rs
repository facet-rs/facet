//! Byte-stream specific connection handling.
//!
//! This module provides connection handling for byte-stream transports
//! (TCP, Unix sockets) that need COBS framing.
//!
//! For message-based transports (WebSocket), use `roam_session` directly.

use std::future::Future;
use std::io;
use std::sync::Arc;

use facet::Facet;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::framing::CobsFramed;
use roam_session::{
    Caller, ConnectError, ConnectionError, ConnectionHandle, Driver, HandshakeConfig, ResponseData,
    RetryPolicy, ServiceDispatcher, TransportError,
};

/// A factory that creates new byte-stream connections on demand.
///
/// Used by [`connect()`] for reconnection. The transport will be wrapped
/// in COBS framing automatically.
///
/// For transports that already provide message framing (like WebSocket),
/// use [`roam_session::MessageConnector`] instead.
pub trait Connector: Send + Sync + 'static {
    /// The raw stream type (e.g., `TcpStream`, `UnixStream`).
    type Transport: AsyncRead + AsyncWrite + Unpin + Send;

    /// Establish a new connection.
    fn connect(&self) -> impl Future<Output = io::Result<Self::Transport>> + Send;
}

// ============================================================================
// accept() - For accepted byte-stream connections
// ============================================================================

/// Accept a byte-stream connection and perform handshake.
///
/// Wraps the stream in COBS framing, then delegates to `accept_framed`.
/// Returns:
/// - A handle for making calls on connection 0 (root)
/// - A receiver for incoming virtual connection requests
/// - A driver that must be spawned
pub async fn accept<S, D>(
    stream: S,
    config: HandshakeConfig,
    dispatcher: D,
) -> Result<
    (
        ConnectionHandle,
        roam_session::IncomingConnections,
        Driver<CobsFramed<S>, D>,
    ),
    ConnectionError,
>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
    D: ServiceDispatcher,
{
    let framed = CobsFramed::new(stream);
    roam_session::accept_framed(framed, config, dispatcher).await
}

// ============================================================================
// connect() - For initiated byte-stream connections with reconnection
// ============================================================================

/// Connect to a peer with automatic reconnection.
///
/// Returns a client that automatically reconnects on failure. The client
/// implements [`Caller`] so it works with generated service clients.
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

/// Internal connection state for Client.
struct ClientState<S> {
    handle: ConnectionHandle,
    driver_handle: JoinHandle<Result<(), ConnectionError>>,
    _marker: std::marker::PhantomData<S>,
}

impl<S> ClientState<S> {
    fn is_alive(&self) -> bool {
        !self.driver_handle.is_finished()
    }
}

/// A client that automatically reconnects on transport failure.
///
/// Created by [`connect()`]. Implements [`Caller`] so it works
/// with generated service clients.
///
/// Cloning is cheap - all clones share the same connection state.
pub struct Client<C: Connector, D> {
    connector: Arc<C>,
    config: HandshakeConfig,
    dispatcher: D,
    retry_policy: RetryPolicy,
    state: Arc<Mutex<Option<ClientState<C::Transport>>>>,
}

impl<C, D> Clone for Client<C, D>
where
    C: Connector,
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

    async fn connect_internal(&self) -> Result<ClientState<C::Transport>, ConnectError> {
        let stream = self
            .connector
            .connect()
            .await
            .map_err(ConnectError::ConnectFailed)?;

        let framed = CobsFramed::new(stream);

        let (handle, _incoming, driver) =
            roam_session::initiate_framed(framed, self.config.clone(), self.dispatcher.clone())
                .await
                .map_err(|e| ConnectError::ConnectFailed(connection_error_to_io(e)))?;

        // Note: We drop `_incoming` - this client doesn't accept sub-connections.
        // Any Connect requests from the server will be automatically rejected.

        let driver_handle = tokio::spawn(async move { driver.run().await });

        Ok(ClientState {
            handle,
            driver_handle,
            _marker: std::marker::PhantomData,
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

impl<C, D> Caller for Client<C, D>
where
    C: Connector,
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
                    tokio::time::sleep(backoff).await;
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

    fn bind_response_streams<R: Facet<'static>>(&self, response: &mut R, channels: &[u64]) {
        // Client wraps a ConnectionHandle, but we don't have direct access to it
        // during bind_response_streams. For reconnecting clients, response stream binding
        // would need to be handled at a higher level or the client would need to store
        // the current handle.
        // For now, this is a no-op - Client users should use ConnectionHandle
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
