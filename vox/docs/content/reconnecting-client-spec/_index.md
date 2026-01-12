+++
title = "Reconnecting Client Specification"
description = "Automatic reconnection for roam clients"
weight = 40
+++

# Introduction

A `ReconnectingClient` wraps a roam `ConnectionHandle` and provides
transparent reconnection when the underlying transport fails. Callers
make RPC calls as normal; if the connection is lost, the client
automatically reconnects and retries the call.

## Motivation

Roam clients currently fail permanently when the connection drops.
This forces every consumer to implement their own reconnection logic
at the wrong abstraction level. Reconnection belongs in roam-stream,
at the transport layer.

# API

## Connector Trait

> r[reconnect.connector]
>
> A `Connector` is a factory that creates new connections on demand.
> It is called on initial connect and after each disconnect.

```rust
pub trait Connector: Send + Sync + 'static {
    type Transport: MessageTransport;

    /// Establish a new connection.
    fn connect(&self) -> impl Future<Output = io::Result<Self::Transport>> + Send;

    /// Hello parameters for the connection.
    fn hello(&self) -> Hello;
}
```

> r[reconnect.connector.transport]
>
> The `Transport` associated type MUST implement `MessageTransport`.
> This allows any roam-compatible transport to be used.

> r[reconnect.connector.hello]
>
> The `hello()` method returns the `Hello` parameters to use when
> establishing the connection. These are sent during the hello
> exchange after transport connect succeeds.

## ReconnectingClient

> r[reconnect.client]
>
> `ReconnectingClient<C>` wraps a `Connector` and provides automatic
> reconnection with configurable retry policy.

```rust
pub struct ReconnectingClient<C: Connector> {
    connector: C,
    // ... internal state
}
```

### Construction

> r[reconnect.construction.lazy]
>
> `ReconnectingClient::new()` does NOT connect immediately. The first
> call triggers the initial connection attempt.

```rust
impl<C: Connector> ReconnectingClient<C> {
    /// Create a new reconnecting client with default retry policy.
    pub fn new(connector: C) -> Self;

    /// Create with custom retry policy.
    pub fn with_policy(connector: C, policy: RetryPolicy) -> Self;
}
```

### Retry Policy

> r[reconnect.policy]
>
> `RetryPolicy` configures reconnection behavior with exponential backoff.

```rust
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
```

> r[reconnect.policy.defaults]
>
> Default retry policy values:
> - `max_attempts`: 3
> - `initial_backoff`: 100ms
> - `max_backoff`: 5s
> - `backoff_multiplier`: 2.0

> r[reconnect.policy.backoff]
>
> The delay before attempt N (1-indexed) is:
> `min(initial_backoff * backoff_multiplier^(N-1), max_backoff)`

### Making Calls

> r[reconnect.call]
>
> The `call()` method makes an RPC call with automatic reconnection.
> If the call fails due to a transport error, it reconnects and retries
> according to the retry policy.

```rust
impl<C: Connector> ReconnectingClient<C> {
    /// Make an RPC call with automatic reconnection.
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

> r[reconnect.handle]
>
> The `handle()` method returns the current `ConnectionHandle` for
> direct access. The handle may become invalid if the connection drops.

```rust
impl<C: Connector> ReconnectingClient<C> {
    /// Get a connection handle for making calls.
    ///
    /// Prefer using `call()` directly for automatic retry.
    pub async fn handle(&self) -> Result<ConnectionHandle, ReconnectError>;
}
```

## Errors

> r[reconnect.error]
>
> `ReconnectError` distinguishes transport failures from RPC errors.

```rust
#[derive(Debug)]
pub enum ReconnectError {
    /// All retry attempts exhausted.
    RetriesExhausted {
        original: io::Error,
        attempts: u32,
    },

    /// Connection failed.
    ConnectFailed(io::Error),

    /// RPC error (no reconnection attempted).
    Rpc(CallError),
}
```

> r[reconnect.error.retries-exhausted]
>
> `RetriesExhausted` is returned when a transport error occurs and
> all reconnection attempts fail. It contains the original error that
> caused the disconnect and the number of attempts made.

> r[reconnect.error.connect-failed]
>
> `ConnectFailed` is returned when the initial connection or a
> reconnection attempt fails with an error from the connector.

> r[reconnect.error.rpc]
>
> `Rpc` wraps call-level errors that are NOT transport failures.
> These errors do not trigger reconnection because the connection
> is still valid.

# Behavior

## Connection Lifecycle

> r[reconnect.lifecycle]
>
> Connection lifecycle:
> 1. **Lazy connection**: No connection attempt until first call
> 2. **On first call**: Call `connector.connect()`, perform hello exchange, spawn driver
> 3. **On success**: Cache the `ConnectionHandle`, complete the call
> 4. **On transport error during call**: Mark connection dead, trigger reconnection

## Reconnection Triggers

> r[reconnect.trigger.transport]
>
> Reconnection is triggered when a call fails with a transport error.
> Transport errors include:
> - Broken pipe (EPIPE)
> - Connection reset (ECONNRESET)
> - Connection closed by peer
> - Goodbye message received
> - Any `io::Error` from the transport layer

> r[reconnect.trigger.not-rpc]
>
> Reconnection is NOT triggered for RPC-level errors:
> - `RoamError::UnknownMethod`
> - `RoamError::InvalidPayload`
> - `RoamError::Cancelled`
> - Application errors (`RoamError::User`)
> - Serialization errors

## Reconnection Flow

> r[reconnect.flow]
>
> When a transport error occurs:
> 1. Mark the current connection as dead
> 2. Set attempt counter to 1
> 3. Call `connector.connect()`
> 4. On connect success: perform hello exchange, spawn driver, retry original call
> 5. On connect failure: wait backoff duration, increment attempt counter
> 6. If attempts < max_attempts: go to step 3
> 7. Otherwise: return `RetriesExhausted`

```
Call fails with transport error
         |
         v
   Mark connection dead
         |
         v
   +------------------+
   |  Attempt = 1     |
   +--------+---------+
            |
            v
   +------------------+
   | connector.connect() |
   +--------+---------+
            |
      +-----+-----+
      |           |
   Success     Failure
      |           |
      v           v
  Hello      Wait backoff
  exchange        |
      |           |
      |     Attempt += 1
      |           |
      |     +-----+-----+
      |     |           |
      |  < max      >= max
      |     |           |
      |     v           v
      |   Retry    RetriesExhausted
      |     |
      v     |
   Spawn    |
   driver   |
      |     |
      v     |
   Retry <--+
   original
   call
```

## Concurrency

> r[reconnect.concurrency.shared]
>
> Multiple tasks MAY share a `ReconnectingClient` via `Arc`.

> r[reconnect.concurrency.single-reconnect]
>
> Only one reconnection attempt runs at a time. If multiple calls
> fail simultaneously, they share the reconnection attempt.

> r[reconnect.concurrency.wait]
>
> Callers blocked during reconnection wait for it to complete.
> After reconnection succeeds, all waiting callers proceed with
> the new connection.

> r[reconnect.concurrency.impl]
>
> Implementation note: Use a `Mutex<State>` or similar to serialize
> reconnection attempts while allowing concurrent calls on a healthy
> connection.

## Driver Lifecycle

> r[reconnect.driver]
>
> Each connection spawns a driver task that processes incoming messages.

> r[reconnect.driver.cleanup]
>
> On disconnect, the driver task exits (or is aborted). A new
> connection spawns a new driver. The driver handle is stored
> internally for cleanup.

# Integration

## Generated Clients

> r[reconnect.integration.generated]
>
> Generated clients (e.g., `FooClient`) wrap a `ConnectionHandle`.
> To use with `ReconnectingClient`:

```rust
// Option A: Get handle and construct client (handle may become stale)
let handle = reconnecting.handle().await?;
let client = FooClient::new(handle);

// Option B: Use call() directly
let response: StatusResponse = reconnecting
    .call(foo_method_id::status(), &())
    .await?;
```

# Example Usage

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
    .call(daemon_method_id::status(), &())
    .await?;
```

# Test Requirements

> r[reconnect.test.basic]
>
> Test: Connection drops mid-call, reconnects, call succeeds.

> r[reconnect.test.exhaustion]
>
> Test: Server stays down, returns `RetriesExhausted` after max attempts.

> r[reconnect.test.backoff]
>
> Test: Verify exponential backoff timing between attempts.

> r[reconnect.test.concurrent]
>
> Test: Multiple tasks calling during reconnection all succeed after.

> r[reconnect.test.rpc-passthrough]
>
> Test: `RoamError::UnknownMethod` is not treated as transport error.

> r[reconnect.test.lazy]
>
> Test: No connection until first call.

> r[reconnect.test.goodbye]
>
> Test: Server sends Goodbye, client reconnects.
