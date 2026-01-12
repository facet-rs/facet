//! Reconnecting client for transparent reconnection on transport failure.
//!
//! This module provides [`ReconnectingClient`], a wrapper that automatically
//! reconnects and retries calls when the underlying transport fails.
//!
//! # Example
//!
//! ```ignore
//! use roam_stream::{ReconnectingClient, Connector, CobsFramed};
//! use roam_wire::Hello;
//! use tokio::net::UnixStream;
//!
//! struct DaemonConnector {
//!     socket_path: PathBuf,
//! }
//!
//! impl Connector for DaemonConnector {
//!     type Transport = CobsFramed<UnixStream>;
//!
//!     async fn connect(&self) -> io::Result<Self::Transport> {
//!         let stream = UnixStream::connect(&self.socket_path).await?;
//!         Ok(CobsFramed::new(stream))
//!     }
//!
//!     fn hello(&self) -> Hello {
//!         Hello::V1 {
//!             max_payload_size: 1024 * 1024,
//!             initial_channel_credit: 64 * 1024,
//!         }
//!     }
//! }
//!
//! let client = ReconnectingClient::new(DaemonConnector { socket_path });
//! let response: StatusResponse = client.call(method_id::status(), &()).await?;
//! ```

use std::future::Future;
use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use facet::Facet;
use roam_session::{
    CallError, ChannelRegistry, ConnectionHandle, RoamError, ServiceDispatcher, TaskMessage,
};
use roam_wire::Hello;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::ConnectionError;
use crate::driver::establish_initiator;
use crate::transport::MessageTransport;

// r[reconnect.connector]
/// A factory that creates new connections on demand.
///
/// Called on initial connect and after each disconnect.
pub trait Connector: Send + Sync + 'static {
    /// The transport type produced by this connector.
    // r[reconnect.connector.transport]
    type Transport: MessageTransport;

    /// Establish a new connection.
    fn connect(&self) -> impl Future<Output = io::Result<Self::Transport>> + Send;

    // r[reconnect.connector.hello]
    /// Hello parameters for the connection.
    fn hello(&self) -> Hello;
}

// r[reconnect.policy]
/// Configuration for reconnection behavior with exponential backoff.
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

// r[reconnect.policy.defaults]
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
    // r[reconnect.policy.backoff]
    /// Calculate the backoff duration for a given attempt number (1-indexed).
    fn backoff_for_attempt(&self, attempt: u32) -> Duration {
        let multiplier = self
            .backoff_multiplier
            .powi(attempt.saturating_sub(1) as i32);
        let backoff = self.initial_backoff.mul_f64(multiplier);
        backoff.min(self.max_backoff)
    }
}

// r[reconnect.error]
/// Error type for reconnecting client operations.
#[derive(Debug)]
pub enum ReconnectError {
    // r[reconnect.error.retries-exhausted]
    /// All retry attempts exhausted after a transport error.
    RetriesExhausted {
        /// The original error that caused the disconnect.
        original: io::Error,
        /// Number of reconnection attempts made.
        attempts: u32,
    },

    // r[reconnect.error.connect-failed]
    /// Connection failed.
    ConnectFailed(io::Error),

    // r[reconnect.error.rpc]
    /// RPC error (no reconnection attempted).
    Rpc(CallError),
}

impl std::fmt::Display for ReconnectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReconnectError::RetriesExhausted { original, attempts } => {
                write!(
                    f,
                    "reconnection failed after {attempts} attempts: {original}"
                )
            }
            ReconnectError::ConnectFailed(e) => write!(f, "connection failed: {e}"),
            ReconnectError::Rpc(e) => write!(f, "RPC error: {e}"),
        }
    }
}

impl std::error::Error for ReconnectError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ReconnectError::RetriesExhausted { original, .. } => Some(original),
            ReconnectError::ConnectFailed(e) => Some(e),
            ReconnectError::Rpc(e) => Some(e),
        }
    }
}

impl From<CallError> for ReconnectError {
    fn from(e: CallError) -> Self {
        ReconnectError::Rpc(e)
    }
}

/// Internal connection state.
struct ConnectionState {
    handle: ConnectionHandle,
    driver_handle: JoinHandle<Result<(), ConnectionError>>,
}

impl ConnectionState {
    fn is_alive(&self) -> bool {
        !self.driver_handle.is_finished()
    }
}

// r[reconnect.client]
/// A client that automatically reconnects on transport failure.
///
/// Wraps a [`Connector`] and provides transparent reconnection when the
/// underlying transport fails. Callers make RPC calls as normal; if the
/// connection is lost, the client automatically reconnects and retries.
///
/// `ReconnectingClient` is cheap to clone - all clones share the same
/// underlying connection state.
pub struct ReconnectingClient<C: Connector> {
    connector: Arc<C>,
    policy: RetryPolicy,
    // r[reconnect.concurrency.impl]
    state: Arc<Mutex<Option<ConnectionState>>>,
}

impl<C: Connector> Clone for ReconnectingClient<C> {
    fn clone(&self) -> Self {
        Self {
            connector: self.connector.clone(),
            policy: self.policy.clone(),
            state: self.state.clone(),
        }
    }
}

impl<C: Connector> ReconnectingClient<C> {
    // r[reconnect.construction.lazy]
    /// Create a new reconnecting client with default retry policy.
    ///
    /// Does not connect immediately. The first call triggers connection.
    pub fn new(connector: C) -> Self {
        Self {
            connector: Arc::new(connector),
            policy: RetryPolicy::default(),
            state: Arc::new(Mutex::new(None)),
        }
    }

    /// Create a new reconnecting client with a custom retry policy.
    pub fn with_policy(connector: C, policy: RetryPolicy) -> Self {
        Self {
            connector: Arc::new(connector),
            policy,
            state: Arc::new(Mutex::new(None)),
        }
    }

    // r[reconnect.handle]
    /// Get a connection handle for making calls.
    ///
    /// The returned handle may become invalid if the connection drops.
    /// Prefer using [`call_raw()`](Self::call_raw) directly for automatic retry.
    pub async fn handle(&self) -> Result<ConnectionHandle, ReconnectError> {
        self.ensure_connected().await
    }

    /// Ensure we have an active connection, reconnecting if necessary.
    async fn ensure_connected(&self) -> Result<ConnectionHandle, ReconnectError> {
        let mut state = self.state.lock().await;

        // Check if we have a live connection
        if let Some(ref conn) = *state {
            if conn.is_alive() {
                return Ok(conn.handle.clone());
            }
            // Connection is dead, clear it
            *state = None;
        }

        // Need to connect
        let conn = self.connect_internal().await?;
        let handle = conn.handle.clone();
        *state = Some(conn);
        Ok(handle)
    }

    /// Internal connection logic.
    async fn connect_internal(&self) -> Result<ConnectionState, ReconnectError> {
        let transport = self
            .connector
            .connect()
            .await
            .map_err(ReconnectError::ConnectFailed)?;

        let hello = self.connector.hello();

        // Use a no-op dispatcher since we're client-only
        let dispatcher = NoOpDispatcher;

        let (handle, driver) = establish_initiator(transport, hello, dispatcher)
            .await
            .map_err(|e| ReconnectError::ConnectFailed(connection_error_to_io(e)))?;

        // r[reconnect.driver]
        let driver_handle = tokio::spawn(async move { driver.run().await });

        Ok(ConnectionState {
            handle,
            driver_handle,
        })
    }

    // r[reconnect.call]
    /// Make an RPC call with automatic reconnection.
    ///
    /// If the call fails due to a transport error, reconnects and retries
    /// according to the retry policy.
    pub async fn call_raw(
        &self,
        method_id: u64,
        payload: Vec<u8>,
    ) -> Result<Vec<u8>, ReconnectError> {
        let mut last_error: Option<io::Error> = None;
        let mut attempt = 0u32;

        loop {
            // Get or establish connection
            let handle = match self.ensure_connected().await {
                Ok(h) => h,
                Err(ReconnectError::ConnectFailed(e)) => {
                    // r[reconnect.flow]
                    attempt += 1;
                    if attempt >= self.policy.max_attempts {
                        return Err(ReconnectError::RetriesExhausted {
                            original: last_error.unwrap_or(e),
                            attempts: attempt,
                        });
                    }
                    last_error = Some(e);
                    let backoff = self.policy.backoff_for_attempt(attempt);
                    tokio::time::sleep(backoff).await;
                    continue;
                }
                Err(e) => return Err(e),
            };

            // Attempt the call
            match handle.call_raw(method_id, payload.clone()).await {
                Ok(response) => return Ok(response),
                // r[reconnect.trigger.not-rpc]
                Err(CallError::Encode(e)) => return Err(ReconnectError::Rpc(CallError::Encode(e))),
                Err(CallError::Decode(e)) => return Err(ReconnectError::Rpc(CallError::Decode(e))),
                // r[reconnect.trigger.transport]
                Err(CallError::ConnectionClosed) | Err(CallError::DriverGone) => {
                    // Mark connection as dead
                    {
                        let mut state = self.state.lock().await;
                        *state = None;
                    }

                    attempt += 1;
                    if attempt >= self.policy.max_attempts {
                        let error = last_error.unwrap_or_else(|| {
                            io::Error::new(io::ErrorKind::ConnectionReset, "connection closed")
                        });
                        return Err(ReconnectError::RetriesExhausted {
                            original: error,
                            attempts: attempt,
                        });
                    }

                    last_error = Some(io::Error::new(
                        io::ErrorKind::ConnectionReset,
                        "connection closed",
                    ));
                    let backoff = self.policy.backoff_for_attempt(attempt);
                    tokio::time::sleep(backoff).await;
                    // Loop will reconnect on next iteration
                }
            }
        }
    }
}

// r[reconnect.call]
impl<C: Connector> roam_session::Caller for ReconnectingClient<C> {
    type Error = ReconnectError;

    /// Make an RPC call with automatic reconnection.
    ///
    /// This delegates to the underlying `ConnectionHandle::call`, which handles
    /// stream binding (Tx/Rx channel ID assignment) and serialization.
    ///
    /// If the call fails due to a transport error, reconnects and retries
    /// according to the retry policy.
    async fn call<T: Facet<'static>>(
        &self,
        method_id: u64,
        args: &mut T,
    ) -> Result<Vec<u8>, Self::Error> {
        let mut last_error: Option<io::Error> = None;
        let mut attempt = 0u32;

        loop {
            // Get or establish connection
            let handle = match self.ensure_connected().await {
                Ok(h) => h,
                Err(ReconnectError::ConnectFailed(e)) => {
                    // r[reconnect.flow]
                    attempt += 1;
                    if attempt >= self.policy.max_attempts {
                        return Err(ReconnectError::RetriesExhausted {
                            original: last_error.unwrap_or(e),
                            attempts: attempt,
                        });
                    }
                    last_error = Some(e);
                    let backoff = self.policy.backoff_for_attempt(attempt);
                    tokio::time::sleep(backoff).await;
                    continue;
                }
                Err(e) => return Err(e),
            };

            // Attempt the call - ConnectionHandle::call handles stream binding
            match handle.call(method_id, args).await {
                Ok(response) => return Ok(response),
                // r[reconnect.trigger.not-rpc]
                Err(CallError::Encode(e)) => return Err(ReconnectError::Rpc(CallError::Encode(e))),
                Err(CallError::Decode(e)) => return Err(ReconnectError::Rpc(CallError::Decode(e))),
                // r[reconnect.trigger.transport]
                Err(CallError::ConnectionClosed) | Err(CallError::DriverGone) => {
                    // Mark connection as dead
                    {
                        let mut state = self.state.lock().await;
                        *state = None;
                    }

                    attempt += 1;
                    if attempt >= self.policy.max_attempts {
                        let error = last_error.unwrap_or_else(|| {
                            io::Error::new(io::ErrorKind::ConnectionReset, "connection closed")
                        });
                        return Err(ReconnectError::RetriesExhausted {
                            original: error,
                            attempts: attempt,
                        });
                    }

                    last_error = Some(io::Error::new(
                        io::ErrorKind::ConnectionReset,
                        "connection closed",
                    ));
                    let backoff = self.policy.backoff_for_attempt(attempt);
                    tokio::time::sleep(backoff).await;
                    // Loop will reconnect on next iteration
                }
            }
        }
    }
}

/// Convert ConnectionError to io::Error for consistent error handling.
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

/// A no-op dispatcher for client-only connections.
///
/// Returns UnknownMethod for all requests since we don't serve any methods.
struct NoOpDispatcher;

impl ServiceDispatcher for NoOpDispatcher {
    fn dispatch(
        &self,
        _method_id: u64,
        _payload: Vec<u8>,
        request_id: u64,
        registry: &mut ChannelRegistry,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        // Send UnknownMethod response
        let task_tx = registry.task_tx();
        Box::pin(async move {
            let response: Result<(), RoamError<()>> = Err(RoamError::UnknownMethod);
            let payload = facet_postcard::to_vec(&response).unwrap_or_default();
            let _ = task_tx
                .send(TaskMessage::Response {
                    request_id,
                    payload,
                })
                .await;
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backoff_calculation() {
        let policy = RetryPolicy::default();

        // Attempt 1: initial_backoff * 2^0 = 100ms
        assert_eq!(policy.backoff_for_attempt(1), Duration::from_millis(100));

        // Attempt 2: initial_backoff * 2^1 = 200ms
        assert_eq!(policy.backoff_for_attempt(2), Duration::from_millis(200));

        // Attempt 3: initial_backoff * 2^2 = 400ms
        assert_eq!(policy.backoff_for_attempt(3), Duration::from_millis(400));

        // Eventually capped at max_backoff (5s)
        assert_eq!(policy.backoff_for_attempt(10), Duration::from_secs(5));
    }

    #[test]
    fn test_default_policy() {
        let policy = RetryPolicy::default();
        assert_eq!(policy.max_attempts, 3);
        assert_eq!(policy.initial_backoff, Duration::from_millis(100));
        assert_eq!(policy.max_backoff, Duration::from_secs(5));
        assert!((policy.backoff_multiplier - 2.0).abs() < f64::EPSILON);
    }
}
