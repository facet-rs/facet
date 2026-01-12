# Reconnecting Client Specification

## Overview

A `ReconnectingClient` wraps a roam `ConnectionHandle` and provides transparent reconnection when the underlying transport fails. Callers make RPC calls as normal; if the connection is lost, the client automatically reconnects and retries the call.

## Motivation

Roam clients currently fail permanently when the connection drops. This forces every consumer to implement their own reconnection logic at the wrong abstraction level. Reconnection belongs in roam-stream, at the transport layer.

## API

```rust
/// A client that automatically reconnects on transport failure.
pub struct ReconnectingClient<C: Connector> {
    connector: C,
    // ... internal state
}

/// Creates new connections on demand.
pub trait Connector: Send + Sync + 'static {
    type Transport: MessageTransport;

    /// Establish a new connection.
    ///
    /// Called on initial connect and after each disconnect.
    /// The implementation should handle any setup (e.g., spawning daemon).
    fn connect(&self) -> impl Future<Output = io::Result<Self::Transport>> + Send;

    /// Hello parameters for the connection.
    fn hello(&self) -> Hello;
}
```

### Construction

```rust
impl<C: Connector> ReconnectingClient<C> {
    /// Create a new reconnecting client.
    ///
    /// Does not connect immediately. First call will trigger connection.
    pub fn new(connector: C) -> Self;

    /// Create with custom retry policy.
    pub fn with_policy(connector: C, policy: RetryPolicy) -> Self;
}
```

### Retry Policy

```rust
pub struct RetryPolicy {
    /// Maximum reconnection attempts before giving up.
    /// Default: 3
    pub max_attempts: u32,

    /// Initial delay between reconnection attempts.
    /// Default: 100ms
    pub initial_backoff: Duration,

    /// Maximum delay between reconnection attempts.
    /// Default: 5s
    pub max_backoff: Duration,

    /// Backoff multiplier.
    /// Default: 2.0
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
```

### Making Calls

```rust
impl<C: Connector> ReconnectingClient<C> {
    /// Get a connection handle for making calls.
    ///
    /// The returned handle may become invalid if the connection drops.
    /// Prefer using `call()` directly for automatic retry.
    pub async fn handle(&self) -> Result<ConnectionHandle, ReconnectError>;

    /// Make an RPC call with automatic reconnection.
    ///
    /// If the call fails due to a transport error, reconnects and retries
    /// according to the retry policy.
    pub async fn call<Req, Resp>(
        &self,
        method_id: u64,
        request: &Req,
    ) -> Result<Resp, ReconnectError>
    where
        Req: for<'a> Facet<'a>,
        Resp: for<'a> Facet<'a>;
}
```

### Errors

```rust
#[derive(Debug)]
pub enum ReconnectError {
    /// Transport error that triggered reconnection, but all retries exhausted.
    RetriesExhausted {
        /// The original error that caused the disconnect.
        original: io::Error,
        /// Number of reconnection attempts made.
        attempts: u32,
    },

    /// Connection failed and connector returned an error.
    ConnectFailed(io::Error),

    /// RPC error (not a transport failure, no reconnection attempted).
    Rpc(CallError),
}
```

## Behavior

### Connection Lifecycle

1. **Lazy connection**: No connection attempt until first call
2. **On first call**: Call `connector.connect()`, perform hello exchange, spawn driver
3. **On success**: Cache the `ConnectionHandle`, complete the call
4. **On transport error during call**: Mark connection dead, trigger reconnection

### Reconnection Trigger

Reconnection is triggered when a call fails with a transport error. Transport errors include:

- Broken pipe (EPIPE)
- Connection reset (ECONNRESET)
- Connection closed by peer
- Goodbye message received
- Any `io::Error` from the transport layer

Reconnection is NOT triggered for:

- RPC-level errors (`RoamError::UnknownMethod`, etc.)
- Serialization errors
- Application errors

### Reconnection Flow

```
Call fails with transport error
         │
         ▼
   Mark connection dead
         │
         ▼
   ┌─────────────────┐
   │  Attempt = 1    │
   └────────┬────────┘
            │
            ▼
   ┌─────────────────┐
   │ connector.connect() │
   └────────┬────────┘
            │
      ┌─────┴─────┐
      │           │
   Success     Failure
      │           │
      ▼           ▼
  Hello      Wait backoff
  exchange        │
      │           │
      │     Attempt += 1
      │           │
      │     ┌─────┴─────┐
      │     │           │
      │  < max      >= max
      │     │           │
      │     ▼           ▼
      │   Retry    RetriesExhausted
      │     │
      ▼     │
   Spawn    │
   driver   │
      │     │
      ▼     │
   Retry ◄──┘
   original
   call
```

### Concurrency

- Multiple tasks may share a `ReconnectingClient` via `Arc`
- Only one reconnection attempt runs at a time
- Other callers wait for reconnection to complete
- After reconnection, all waiting callers proceed with the new connection

Implementation note: Use a `Mutex<State>` or similar to serialize reconnection attempts while allowing concurrent calls on a healthy connection.

### Driver Lifecycle

- Each connection spawns a driver task
- On disconnect, the driver task exits (or is aborted)
- New connection spawns a new driver
- Driver handle stored internally for cleanup

## Integration with Generated Clients

Generated clients (e.g., `TraceyDaemonClient`) wrap a `ConnectionHandle`. To use with `ReconnectingClient`:

```rust
// Option A: Get handle and construct client (handle may become stale)
let handle = reconnecting.handle().await?;
let client = TraceyDaemonClient::new(handle);

// Option B: Use call() directly
let response: StatusResponse = reconnecting
    .call(tracey_daemon_method_id::status(), &())
    .await?;
```

Future enhancement: Generate clients that accept `ReconnectingClient` directly.

## Example Usage

```rust
// Define connector
struct DaemonConnector {
    socket_path: PathBuf,
}

impl Connector for DaemonConnector {
    type Transport = CobsFramed<UnixStream>;

    async fn connect(&self) -> io::Result<Self::Transport> {
        let stream = UnixStream::connect(&self.socket_path).await?;
        Ok(CobsFramed::new(stream))
    }

    fn hello(&self) -> Hello {
        Hello::V1 {
            max_payload_size: 1024 * 1024,
            initial_channel_credit: 64 * 1024,
        }
    }
}

// Use it
let connector = DaemonConnector { socket_path };
let client = ReconnectingClient::new(connector);

// Calls reconnect transparently
let status: StatusResponse = client
    .call(tracey_daemon_method_id::status(), &())
    .await?;
```

## Test Cases

1. **Basic reconnection**: Connection drops mid-call, reconnects, call succeeds
2. **Retry exhaustion**: Server stays down, returns `RetriesExhausted` after N attempts
3. **Backoff timing**: Verify exponential backoff between attempts
4. **Concurrent callers**: Multiple tasks calling during reconnection all succeed after
5. **RPC errors pass through**: `RoamError::UnknownMethod` not treated as transport error
6. **Lazy connect**: No connection until first call
7. **Goodbye handling**: Server sends Goodbye, client reconnects

## Open Questions

1. Should there be a `disconnect()` method for explicit teardown?
2. Should reconnection be cancelable via a token?
3. Should there be hooks/callbacks for observability (on_disconnect, on_reconnect)?
4. Should the retry policy be dynamic (e.g., circuit breaker pattern)?
