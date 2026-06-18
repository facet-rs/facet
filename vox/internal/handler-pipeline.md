# Handler Pipeline

## Goals

1. `Connection` is the protocol envelope over one `Link`.
2. `Lane` is the service namespace inside a connection.
3. `RequestScope` owns the lifetime of one request and its raw channels.
4. `Driver` is RPC orchestration on top of lane endpoints.
5. Zero-copy payload flow is preserved end-to-end.
6. Backpressure is explicit and bounded.

---

## Layering

```text
Link
  -> Connection (handshake, peer settings, lane table, control messages)
    -> Lane endpoints (no public lane id routing)
      -> Driver (routes Request/Response/Channel cores)
        -> Caller API / Handler API
```

`Connection` does not expose raw lane IDs above its boundary except through
diagnostic snapshots. The internal control lane is not an application-visible
service lane.

---

## Core Principle: Capability Split

Caller-side and handler-side must not share the same arbitrary-send surface.

- Caller capability: can initiate calls and await responses.
- Handler capability: can respond to a specific inbound request only.
- Lane control capability: can open, accept, reject, or close service lanes.
- Neither caller nor handler gets a generic "send arbitrary message" interface.

---

## Request Scope

Raw channels are request-scoped. A channel associated with a request remains
live while that request scope is live, and terminal request outcomes close the
associated raw channels.

```rust
pub struct RequestScope {
    lane: LaneHandle,
    request_id: RequestId,
    channels: RequestChannels,
}
```

This is intentionally not an operation identity system. Durable streams,
retries, and resumable delivery are service-level protocols layered above Vox
core.

---

## Zero-Copy Message Cores

Instead of giving higher layers raw `Message`, connection/driver code maps wire
messages to small typed cores.

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

The backing stays alive in `SelfRef`; `Payload` bytes remain borrowed.

---

## Connection And Lane API Shape

```rust
pub struct ConnectionBuilder<T: Link> {
    link: T,
    role: ConnectionRole,
    connection_settings: ConnectionSettings,
    metadata: Metadata,
}

impl<T: Link> ConnectionBuilder<T> {
    pub async fn establish(self) -> Result<(Connection, ConnectionHandle, LaneHandle), ConnectionError>;
}
```

`ConnectionHandle` is the only place that can create or close lanes.

```rust
pub struct ConnectionHandle {
    // command channel into the connection driver
}

impl ConnectionHandle {
    pub async fn open_lane(&self, service: ServiceId, settings: ConnectionSettings)
        -> Result<LaneHandle, LaneOpenError>;

    pub async fn close(&self, reason: CloseReason) -> Result<(), ConnectionError>;

    pub async fn shutdown(self, reason: CloseReason) -> Result<(), ConnectionError>;
}
```

Inbound lane openings go through a structured acceptor:

```rust
pub trait LaneAcceptor {
    fn accept(&self, request: LaneOpenRequest) -> LaneOpenDecision;
}

pub enum LaneOpenDecision {
    Accept { handler: HandlerLink, settings: ConnectionSettings },
    Reject(LaneRejection),
}
```

No API above the connection boundary accepts a raw lane id from user code.

---

## Driver API Shape

Driver sits on top of `LaneHandle` objects.

```rust
pub struct Driver<H> {
    lane: LaneHandle,
    handler: H,
}

impl<H> Driver<H> {
    pub fn new(lane: LaneHandle, handler: H) -> Self;
    pub async fn run(&mut self);
    pub fn caller(&self) -> DriverCaller;
}
```

The caller and handler surfaces stay capability-based:

```rust
#[derive(Clone)]
pub struct DriverCaller {
    // bounded request queue + inflight tracker
}

pub struct InboundCall {
    pub request: SelfRef<RequestCore<'static>>,
    pub responder: Responder,
    pub context: RequestContext,
}

pub struct Responder {
    // one-shot capability bound to one request on one lane
}
```

Properties:

- Handler cannot choose request id.
- Handler cannot choose lane id.
- Handler cannot emit arbitrary wire messages.
- Caller cannot keep raw request channels alive after the request terminates.

---

## Backpressure Model

Three bounded resources matter:

1. Per-lane outbound call permit pool (`peer.max_concurrent_requests`).
2. Per-lane handler ingress queue.
3. Per-connection outbound wire queue.

Rules:

- `DriverCaller::call` acquires a permit before enqueueing request work.
- Permit is released only when final response/cancel completion occurs.
- Handler queue full means driver stops draining that lane's request stream.
- No unbounded queues in connection/driver code.

---

## Timeouts And Observability

Request timeout is an idle-progress timeout, not an overall wall-clock limit.
Request-associated protocol/runtime activity resets it; incidental logs or
spans do not.

Connection establishment is observed separately from request progress:

- link or transport prologue where the transport has one;
- Vox handshake;
- lane open, accept, reject, and close;
- connection receive/send errors and graceful shutdown.

The observability path may use the same codec and transport machinery, but it
must not deadlock behind the lane or request it is trying to explain.

---

## State Machines

### Lane FSM

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

### Request FSM

```text
Created -> Enqueued -> Sent -> InFlight -> Completed
                                 -> CancelRequested -> Completed
```

Only one terminal completion per request.

---

## Public Surface Checklist

Thin public surface should teach only:

- `Link`
- `Connection`, `ConnectionHandle`, `ConnectionBuilder`
- `LaneHandle`, `LaneAcceptor`, `LaneOpenDecision`, `LaneRejection`
- `RequestScope`, `RequestContext`
- `Driver`, `DriverCaller`, `Handler`, `Responder`
- `RequestCore`, `ResponseCore`, `ChannelCore`

Everything else stays crate-private or diagnostic-only.

---

## Refactor Order

1. Keep specs and Tracey rule names on `Connection`, `Lane`, and `RequestScope`.
2. Remove public control-lane ownership semantics.
3. Make driver/shutdown lifecycle explicit in examples and generated clients.
4. Keep raw channels request-scoped and terminalized by request outcomes.
5. Keep establishment, lane decisions, shutdown, and connection errors observable.
6. Document durable streams, retries, and operation identity as service-level protocols.
