//! Byte-stream specific connection handling.
//!
//! This module provides connection handling for byte-stream transports
//! (TCP, Unix sockets) that need length-prefixed framing.
//!
//! For message-based transports (WebSocket), use `roam_session` directly.

use peeps::Mutex;
use std::future::Future;
use std::io;
use std::sync::Arc;

use crate::peeps::prelude::*;
use facet::Facet;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::task::JoinHandle;

use crate::framing::LengthPrefixedFramed;
use roam_session::{
    Caller, ConnectError, ConnectionError, ConnectionHandle, HandshakeConfig, ResponseData,
    RetryPolicy, SendPtr, ServiceDispatcher, TransportError,
};

#[track_caller]
fn source_id_here() -> peeps::SourceId {
    crate::peeps::PEEPS_SOURCE_LEFT.resolve().into()
}

/// A factory that creates new byte-stream connections on demand.
///
/// Used by [`connect()`] for reconnection. The transport will be wrapped
/// in length-prefixed framing automatically.
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
/// Wraps the stream in length-prefixed framing, then delegates to `accept_framed`.
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
        roam_session::Driver<roam_session::DiagnosticTransport<LengthPrefixedFramed<S>>, D>,
    ),
    ConnectionError,
>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
    D: ServiceDispatcher,
{
    let framed = LengthPrefixedFramed::new(stream);
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
        state: Arc::new(Mutex::new("Client.state", None, source_id_here())),
        current_handle: Arc::new(Mutex::new("Client.current_handle", None, source_id_here())),
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
        state: Arc::new(Mutex::new("Client.state", None, source_id_here())),
        current_handle: Arc::new(Mutex::new("Client.current_handle", None, source_id_here())),
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
    current_handle: Arc<Mutex<Option<ConnectionHandle>>>,
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
            current_handle: self.current_handle.clone(),
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
        // Check under lock (don't hold across await)
        {
            let mut state = self.state.lock();
            if let Some(ref conn) = *state {
                if conn.is_alive() {
                    let handle = conn.handle.clone();
                    *self.current_handle.lock() = Some(handle.clone());
                    return Ok(handle);
                }
                *state = None;
                *self.current_handle.lock() = None;
            }
        }

        // Not connected — connect without holding lock
        let conn = self.connect_internal().await?;
        let handle = conn.handle.clone();
        *self.state.lock() = Some(conn);
        *self.current_handle.lock() = Some(handle.clone());
        Ok(handle)
    }

    async fn connect_internal(&self) -> Result<ClientState<C::Transport>, ConnectError> {
        let stream = peeps::net::connect(self.connector.connect(), "roam-stream", "tcp")
            .await
            .map_err(ConnectError::ConnectFailed)?;

        let framed = LengthPrefixedFramed::new(stream);

        let (handle, _incoming, driver) =
            roam_session::initiate_framed(framed, self.config.clone(), self.dispatcher.clone())
                .await
                .map_err(|e| ConnectError::ConnectFailed(connection_error_to_io(e)))?;

        // Note: We drop `_incoming` - this client doesn't accept sub-connections.
        // Any Connect requests from the server will be automatically rejected.

        let driver_handle =
            peeps::spawn_tracked!("roam_stream_driver", async move { driver.run().await });

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
        method_name: &str,
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
                    peeps::sleep!(backoff, "reconnect.backoff").await;
                    continue;
                }
                Err(e) => return Err(e),
            };

            match handle
                .call_raw(method_id, method_name, payload.clone())
                .await
            {
                Ok(response) => return Ok(response),
                Err(TransportError::Encode(e)) => {
                    return Err(ConnectError::Rpc(TransportError::Encode(e)));
                }
                Err(TransportError::ConnectionClosed) | Err(TransportError::DriverGone) => {
                    {
                        let mut state = self.state.lock();
                        *state = None;
                    }
                    *self.current_handle.lock() = None;

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
                    peeps::sleep!(backoff, "reconnect.backoff").await;
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
    async fn call_with_metadata<T: Facet<'static> + Send>(
        &self,
        method_id: u64,
        method_name: &str,
        args: &mut T,
        args_plan: &roam_session::RpcPlan,
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
                    peeps::sleep!(backoff, "reconnect.backoff").await;
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

            let args_ptr = args as *mut T as *mut ();
            #[allow(unsafe_code)]
            let call_result = unsafe {
                roam_session::ConnectionHandle::call_with_metadata_by_plan_with_source(
                    &handle,
                    method_id,
                    method_name,
                    args_ptr,
                    args_plan,
                    metadata.clone(),
                    source_id_here(),
                )
                .await
            };
            match call_result {
                Ok(response) => return Ok(response),
                Err(TransportError::Encode(e)) => {
                    return Err(TransportError::Encode(e));
                }
                Err(TransportError::ConnectionClosed) | Err(TransportError::DriverGone) => {
                    {
                        let mut state = self.state.lock();
                        *state = None;
                    }
                    *self.current_handle.lock() = None;

                    attempt += 1;
                    if attempt >= self.retry_policy.max_attempts {
                        return Err(TransportError::ConnectionClosed);
                    }

                    let backoff = self.retry_policy.backoff_for_attempt(attempt);
                    peeps::sleep!(backoff, "reconnect.backoff").await;
                }
            }
        }
    }

    fn bind_response_channels<R: Facet<'static>>(
        &self,
        response: &mut R,
        plan: &roam_session::RpcPlan,
        channels: &[u64],
    ) {
        let handle = self.current_handle.lock().as_ref().cloned();
        if let Some(handle) = handle {
            handle.bind_response_channels(response, plan, channels);
        } else {
            debug_assert!(
                false,
                "Client::bind_response_channels called without an active ConnectionHandle"
            );
        }
    }

    #[allow(unsafe_code)]
    fn call_with_metadata_by_plan(
        &self,
        method_id: u64,
        method_name: &str,
        args_ptr: SendPtr,
        args_plan: &'static std::sync::Arc<roam_session::RpcPlan>,
        metadata: roam_wire::Metadata,
        source: peeps::SourceId,
    ) -> impl std::future::Future<Output = Result<ResponseData, TransportError>> + Send {
        let this = self.clone();
        let method_name = method_name.to_owned();

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
                        peeps::sleep!(backoff, "reconnect.backoff").await;
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
                    handle.call_with_metadata_by_plan_with_source(
                        method_id,
                        &method_name,
                        args_ptr.as_ptr(),
                        args_plan,
                        metadata.clone(),
                        source,
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
                            let mut state = this.state.lock();
                            *state = None;
                        }
                        *this.current_handle.lock() = None;

                        attempt += 1;
                        if attempt >= this.retry_policy.max_attempts {
                            return Err(TransportError::ConnectionClosed);
                        }

                        let backoff = this.retry_policy.backoff_for_attempt(attempt);
                        peeps::sleep!(backoff, "reconnect.backoff").await;
                    }
                }
            }
        }
    }

    #[allow(unsafe_code)]
    unsafe fn bind_response_channels_by_plan(
        &self,
        response_ptr: *mut (),
        response_plan: &roam_session::RpcPlan,
        channels: &[u64],
    ) {
        let handle = self.current_handle.lock().as_ref().cloned();
        if let Some(handle) = handle {
            // SAFETY: The Caller trait contract guarantees these pointers/plans are valid.
            unsafe {
                handle.bind_response_channels_by_plan(response_ptr, response_plan, channels);
            }
        } else {
            debug_assert!(
                false,
                "Client::bind_response_channels_by_plan called without an active ConnectionHandle"
            );
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
        ConnectionError::UnsupportedProtocolVersion => io::Error::new(
            io::ErrorKind::InvalidData,
            "unsupported protocol version (expected V4)",
        ),
    }
}
