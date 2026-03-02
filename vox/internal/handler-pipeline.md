# Handler Pipeline (Session-First Draft)

## Goals

1. `Session` is a pure multiplexer + connection state machine.
2. `Driver` is RPC orchestration on top of per-connection streams.
3. Zero-copy payload flow is preserved end-to-end.
4. Public API surface is thin and capability-based.
5. Backpressure is explicit and bounded.

---

## Layering

```
Wire Transport
  -> Session (owns conn_id, validates state machine, demux/mux)
    -> Connection endpoints (no public conn_id)
      -> Driver (routes Request/Response/Channel cores)
        -> Caller API / Handler API
```

`Session` does not expose raw `conn_id` above its boundary.

---

## Core Principle: Capability Split

Caller-side and handler-side must not share the same arbitrary-send surface.

- Caller capability: can initiate calls and await responses.
- Handler capability: can respond to a specific inbound request only.
- Neither side gets a generic “send arbitrary message” interface.

---

## Zero-Copy Message Cores

Instead of giving higher layers raw `Message`, session/driver map wire messages to small typed cores.

```rust
pub struct RequestCore<'a> {
    method_id: MethodId,
    args: Payload<'a>,
    channels: &'a [ChannelId],
    metadata: &'a Metadata,
}

pub struct ResponseCore<'a> {
    payload: Payload<'a>,
    channels: &'a [ChannelId],
    metadata: &'a Metadata,
}

pub struct CancelCore<'a> {
    metadata: &'a Metadata,
}

pub enum ChannelCore<'a> {
    Item { channel_id: ChannelId, payload: Payload<'a> },
    Close { channel_id: ChannelId, metadata: &'a Metadata },
    Reset { channel_id: ChannelId, metadata: &'a Metadata },
    Credit { channel_id: ChannelId, additional: u32 },
}
```

Inbound values are carried as:

```rust
SelfRef<RequestCore<'static>>
SelfRef<ResponseCore<'static>>
```

The backing stays alive in `SelfRef`; `Payload` bytes remain borrowed (no reserialize, no copy).

---

## Session Draft API (Multiplexer)

```rust
pub struct Session<T: Transport> { /* owns wire IO + conn table */ }

pub struct SessionBuilder<T: Transport> {
    transport: T,
    role: SessionRole,
    root_settings: ConnectionSettings,
    metadata: Metadata,
}

impl<T: Transport> SessionBuilder<T> {
    pub async fn establish(self) -> Result<(Session<T>, SessionControl, SessionConnection), SessionError>;
}
```

`SessionControl` is the only place that can create/accept/reject virtual connections.

```rust
pub struct SessionControl { /* command channel into session task */ }

impl SessionControl {
    pub async fn open(&self, settings: ConnectionSettings, metadata: Metadata)
        -> Result<SessionConnection, SessionError>;

    pub async fn next_incoming(&mut self)
        -> Result<IncomingConnection, SessionError>;
}

pub struct IncomingConnection {
    pub peer_settings: ConnectionSettings,
    pub metadata: Metadata,
    // no public conn_id
}

impl IncomingConnection {
    pub async fn accept(self, settings: ConnectionSettings, metadata: Metadata)
        -> Result<SessionConnection, SessionError>;

    pub async fn reject(self, metadata: Metadata) -> Result<(), SessionError>;
}
```

Per-connection endpoint surface:

```rust
pub struct SessionConnection {
    // recv side for this specific connection only
}

pub enum ConnInbound {
    Request(SelfRef<RequestCore<'static>>),
    Response(ResponseEnvelope),
    Cancel(CancelEnvelope),
    Channel(ChannelEnvelope),
    Closed { reason: CloseReason, metadata: Metadata },
}

impl SessionConnection {
    pub async fn recv(&mut self) -> Result<ConnInbound, SessionError>;

    pub async fn send_response(
        &self,
        token: ResponseToken,
        payload: Payload<'_>,
        channels: &[ChannelId],
        metadata: Metadata,
    ) -> Result<(), SessionError>;

    pub async fn send_channel(&self, msg: ChannelOut<'_>) -> Result<(), SessionError>;

    pub async fn send_cancel(&self, token: CancelToken, metadata: Metadata)
        -> Result<(), SessionError>;

    pub async fn close(&self, metadata: Metadata) -> Result<(), SessionError>;
}
```

Note:
- `ResponseToken`/`CancelToken` are opaque capability tokens bound to a specific inbound request.
- No method takes raw request id or conn id from user code.

---

## Driver Draft API (RPC Router)

Driver sits on top of `SessionConnection` objects.

```rust
pub struct Driver {
    root_caller: CallerLink,
    incoming: IncomingDrivers,
}

pub struct IncomingDrivers { /* stream of accepted virtual drivers */ }

pub struct ConnectionDriver {
    caller: CallerLink,
    handler: HandlerLink,
}

impl Driver {
    pub fn spawn(root: SessionConnection, control: SessionControl, root_handler: HandlerLink)
        -> Driver;

    pub fn root_caller(&self) -> CallerLink;
    pub fn incoming(&mut self) -> &mut IncomingDrivers;
}

impl IncomingDrivers {
    pub async fn next(&mut self) -> Option<IncomingConnectionDriver>;
}

pub struct IncomingConnectionDriver {
    pub peer_settings: ConnectionSettings,
    pub metadata: Metadata,
}

impl IncomingConnectionDriver {
    pub async fn accept(self, handler: HandlerLink, local: ConnectionSettings)
        -> Result<ConnectionDriver, DriverError>;

    pub async fn reject(self, metadata: Metadata) -> Result<(), DriverError>;
}
```

---

## Handler Side Draft API

Handler sees inbound requests and returns outputs through a tightly-scoped responder.

```rust
pub struct HandlerLink {
    rx: mpsc::Receiver<InboundCall>,
}

pub struct InboundCall {
    pub request: SelfRef<RequestCore<'static>>,
    pub responder: Responder,
    pub context: CallContext,
}

pub struct Responder {
    // one-shot capability bound to one request on one connection
}

impl Responder {
    pub async fn ok(self, payload: Payload<'_>, channels: &[ChannelId], metadata: Metadata)
        -> Result<(), DriverError>;

    pub async fn user_err(self, payload: Payload<'_>, metadata: Metadata)
        -> Result<(), DriverError>;

    pub async fn unknown_method(self, metadata: Metadata) -> Result<(), DriverError>;

    pub async fn invalid_payload(self, metadata: Metadata) -> Result<(), DriverError>;

    pub async fn cancelled(self, metadata: Metadata) -> Result<(), DriverError>;
}
```

Properties:
- Handler cannot choose request id.
- Handler cannot choose connection id.
- Handler cannot emit arbitrary wire messages.

---

## Caller Side Draft API

Caller gets a request-only surface.

```rust
#[derive(Clone)]
pub struct CallerLink {
    // bounded request queue + inflight tracker
}

impl CallerLink {
    pub fn call<'a>(
        &self,
        method_id: MethodId,
        args: Payload<'a>,
        channels: &'a [ChannelId],
        metadata: Metadata,
    ) -> impl Future<Output = Result<SelfRef<ResponseCore<'static>>, CallError>> + 'a;

    pub async fn open_virtual(
        &self,
        settings: ConnectionSettings,
        metadata: Metadata,
    ) -> Result<ConnectionDriver, DriverError>;
}
```

No generic `send(Message)` on caller API.

---

## Backpressure Model

Three bounded queues/semaphores:

1. Per-connection outbound call permit pool (`peer.max_concurrent_requests`).
2. Per-connection handler ingress queue.
3. Per-connection outbound wire queue.

Rules:
- `CallerLink::call` acquires permit before enqueueing request.
- Permit is released only when final response/cancel completion occurs.
- Handler queue full means driver stops draining that connection’s request stream.
- No unbounded queues in session/driver.

---

## State Machines

### Session connection FSM

```text
Absent
  -> PendingOutboundOpen
  -> Active
  -> Closed

Absent
  -> PendingInboundOpen
  -> Active
  -> Closed

Pending* -> Closed (reject/teardown)
Active -> Closed (close/protocol teardown)
```

### Request FSM (driver-level)

```text
Created -> Enqueued -> Sent -> InFlight -> Completed
                                 -> CancelRequested -> Completed
```

Only one terminal completion per request.

---

## API Surface Checklist

Thin public surface should include only:

- `SessionBuilder`, `SessionControl`, `SessionConnection`
- `Driver`, `ConnectionDriver`, `IncomingConnectionDriver`
- `CallerLink`, `HandlerLink`, `InboundCall`, `Responder`
- `RequestCore`, `ResponseCore`, `ChannelCore`

Everything else stays crate-private.

---

## Refactor Order (Session First)

1. Introduce core views (`RequestCore`, `ResponseCore`, `ChannelCore`) + tests.
2. Refactor `Session` into multiplexer task with `SessionControl` and per-connection `SessionConnection`.
3. Remove public conn-id routing surfaces from upper layers.
4. Rebuild `Driver` on new `SessionConnection` API.
5. Rewire generated caller/dispatcher glue to `CallerLink` and `HandlerLink`.
6. Re-verify zero-copy and backpressure behavior with focused tests.

---

## Open Questions

1. Should `Session` itself spawn its internal wire loop, or expose `run()` and let caller spawn?
2. Should handler `metadata` be borrowed (`&Metadata`) or compact-copied for isolation?
3. Do we allow per-connection custom queue sizes, or strictly derive from negotiated settings?
