+++
title = "roam specification"
description = "Formal roam RPC protocol specification"
weight = 10
+++

# Introduction

This is roam specification v4.0.0, last updated February 6, 2026. It canonically
lives at <https://github.com/bearcove/roam> — where you can get the latest version.

roam is a **Rust-native** RPC protocol. We don't claim to be language-neutral —
Rust is the lowest common denominator. There is no independent schema language;
Rust traits *are* the schema. Implementations for other languages (Swift,
TypeScript, etc.) are generated from Rust definitions.

This means:
- The Rust Implementation Specification[^RUST-SPEC] is essential
- Other implementations use Rust tooling for code generation
- Fully independent implementations are a non-goal

Services are defined inside of Rust "proto" crates, annotating traits with
the `#[roam::service]` proc macro attribute:

```rust
#[roam::service]
pub trait TemplateHost {
    /// Load a template by name.
    async fn load_template(&self, context_id: ContextId, name: String) -> LoadTemplateResult;

    /// Call a template function on the host.
    async fn call_function(
        &self,
        context_id: ContextId,
        name: String,
        args: Vec<Value>,
        kwargs: Vec<(String, Value)>,
    ) -> CallFunctionResult;
    
    // etc.
}
```

All types that occur as arguments or in return position must implement
[Facet](https://facet.rs), so that they might be serialized and deserialized
with facet-postcard (see [^POSTCARD]).

Bindings for other languages (Swift, TypeScript) are generated using
a Rust codegen package which is linked together with the "proto" crate to
output Swift/TypeScript packages.

This specification exists to ensure that various implementations are compatible, and
to ensure that those implementations are specified — that their code corresponds to
natural-language requirements, rather than just floating out there.

# Core Semantics

This section defines transport-agnostic semantics that all roam
implementations MUST follow. Transport bindings (networked, SHM) encode
these concepts differently but preserve the same meaning.

## Transports, Links, and Connections

A **transport** is a mechanism for communication (TCP, WebSocket, SHM, etc.).
A **link** is an instance of a transport between two **peers** — the actual
connection over which messages flow.

A link carries one or more **virtual connections**. Each connection is an
independent communication context with its own request ID space, channel ID
space, and dispatcher.

> r[core.link]
>
> A link is a bidirectional communication channel between two peers,
> established via a specific transport. The link carries virtual
> connections, each identified by a `conn_id` (u64).

> r[core.link.connection-zero]
>
> Connection 0 is established implicitly when the link is created.
> Both peers can immediately send messages on connection 0 after the
> Hello exchange.

For peer-to-peer transports, one peer is the **initiator** (opened the
link) and the other is the **acceptor**. This distinction affects
ID allocation but not capabilities — either peer can initiate calls or
open new connections.

## Virtual Connections

Either peer may open additional virtual connections on the link.
This enables multiplexing: a proxy can map each downstream client to
a separate upstream connection, preserving session identity.

> r[core.conn.open]
>
> A peer opens a new connection by sending a `Connect` message with a
> `request_id`. The remote peer responds with `Accept` (providing the
> new `conn_id`) or `Reject`.

> r[core.conn.accept-required]
>
> The remote peer MUST have called `take_incoming_connections()` on
> connection 0 to accept new connections. If not listening, the peer
> MUST respond with `Reject`.

> r[core.conn.id-allocation]
>
> Connection IDs are allocated by the **acceptor** of the `Connect`
> request (the peer receiving Connect, not the link acceptor).
> IDs MUST be unique within the link and MUST NOT be 0.

> r[core.conn.only-root-accepts]
>
> Only connection 0 (the root connection) can accept incoming connections.
> Virtual connections opened via `Connect`/`Accept` cannot themselves
> accept further nested connections.

> r[core.conn.lifecycle]
>
> A virtual connection is closed when either peer sends `Goodbye` on
> that connection. Closing a connection terminates all in-flight calls
> and channels on that connection.

> r[core.conn.independence]
>
> Virtual connections are independent. An error or closure on one
> connection does not affect other connections on the same link.
> Each connection has its own:
> - Request ID space (IDs are unique per-connection, not per-link)
> - Channel ID space
> - Dispatcher (service handler)

### Connection Messages

| Message | Sender | Meaning |
|---------|--------|---------|
| **Connect** | either | Request to open a new virtual connection |
| **Accept** | receiver of Connect | Connection accepted, here is the `conn_id` |
| **Reject** | receiver of Connect | Connection refused (not listening) |

## Calls

A **call** is a request/response exchange identified by a `request_id` (u64).

> r[core.call]
>
> A call consists of exactly one Request and exactly one Response with
> the same `request_id`. The caller sends the Request; the callee sends
> the Response.

> r[core.call.request-id]
>
> Request IDs MUST be unique within a connection. Implementations
> SHOULD use a monotonically increasing counter starting at 1.

### Call Messages

The following abstract messages relate to calls:

| Message | Sender | Meaning |
|---------|--------|---------|
| **Request** | caller | Initiate a call with `request_id`, `method_id`, and payload |
| **Response** | callee | Complete a call with result or error |
| **Cancel** | caller | Request that the callee abandon the call |

> r[core.call.cancel]
>
> Cancel is advisory. The callee MAY ignore it if the call is already
> complete. A Response may still arrive after Cancel is sent (either
> the completed result or a `Cancelled` error). Implementations MUST
> handle late Responses gracefully.

## Channels (Tx/Rx)

A **channel** is a unidirectional, ordered sequence of typed values. At the
type level, roam provides `Tx<T>` and `Rx<T>` to indicate direction.

> r[core.channel]
>
> `Tx<T>` represents data flowing from **caller to callee** (input).
> `Rx<T>` represents data flowing from **callee to caller** (output).
> Each has exactly one sender and one receiver.

On the wire, both `Tx<T>` and `Rx<T>` serialize as a `channel_id`
(u64). The direction is determined by the type, not the ID.
See `r[channeling.type]` for details.

> r[core.channel.return-forbidden]
>
> `Tx<T>` and `Rx<T>` MUST NOT appear in return types. They may
> only appear in argument position. The return type is always a plain
> value (possibly `()` for methods that only produce output via Rx).

For bidirectional communication, use one Tx (input) and one Rx (output).

### Channel Messages

The following abstract messages relate to channels:

| Message | Sender | Meaning |
|---------|--------|---------|
| **Data** | channel sender | Deliver one value of type `T` |
| **Close** | caller (for Push) | End of channel (no more Data from caller) |
| **Reset** | either peer | Abort the channel immediately |
| **Credit** | receiver | Grant permission to send more bytes |

For `Tx<T>` (caller→callee), the caller sends Close when done sending.
After sending Close, the caller MUST NOT send more Data on that channel.
See `r[channeling.close]` for details.

For `Rx<T>` (callee→caller), the channel is implicitly closed when the
callee sends the Response. No explicit Close message is sent.
See `r[channeling.lifecycle.response-closes-pulls]`.

Reset forcefully terminates a channel. After sending or receiving Reset,
both peers MUST discard any pending data and consider it dead.
Any outstanding credit is lost. See `r[channeling.reset]` for details.

### Channel ID Allocation

Channel IDs must be unique within a connection (`r[channeling.id.uniqueness]`).
ID 0 is reserved (`r[channeling.id.zero-reserved]`). The **caller** allocates
all channel IDs for a call (`r[channeling.allocation.caller]`).

For peer-to-peer transports, the **link initiator** uses odd IDs (1, 3, 5, ...)
and the **link acceptor** uses even IDs (2, 4, 6, ...). This applies to all
virtual connections on the link. See `r[channeling.id.parity]` for details.

Note: "Initiator" and "acceptor" refer to who opened the link, not who is
calling whom. If the link initiator calls, they use odd IDs. If the link
acceptor calls back, they use even IDs.

### Channels and Calls

Channels are established via method calls. `Tx<T>` channels may outlive
the Response — the caller continues sending until they send Close.
`Rx<T>` channels are implicitly closed when Response is sent.
See `r[channeling.call-complete]` and `r[channeling.channels-outlive-response]`.

## Errors

### Call Errors

> r[core.error.roam-error]
>
> Call results are wrapped in `RoamError<E>` which distinguishes
> application errors from protocol errors:

| Variant | Meaning |
|---------|---------|
| `User(E)` | Application returned an error (method ran) |
| `UnknownMethod` | No handler for `method_id` |
| `InvalidPayload` | Could not deserialize request |
| `Cancelled` | Call was cancelled |

> r[core.error.call-vs-connection]
>
> Call errors affect only that call. The connection remains open.
> Multiple calls can be in flight, and one failing does not affect others.

### Connection Errors

> r[core.error.connection]
>
> Connection errors are unrecoverable protocol violations. The peer
> detecting the error MUST send a **Goodbye** message with a reason
> and close the connection.

Examples: duplicate request ID, data after Close, unknown channel ID.

> r[core.error.goodbye-reason]
>
> The Goodbye reason MUST contain the rule ID that was violated
> (e.g., `core.channel.close`), optionally followed by context.

## Flow Control

Channels use credit-based flow control (`r[flow.channel.credit-based]`). A sender
MUST NOT send data exceeding the receiver's granted credit. Credit is measured
in bytes (`r[flow.channel.byte-accounting]`). Initial credit is established at
connection setup (`r[flow.channel.initial-credit]`).

The receiver grants additional credit via Credit messages
(`r[flow.channel.credit-grant]`). If a sender exceeds granted credit, this is
a connection error (`r[flow.channel.credit-overrun]`).

See the [Flow Control](#flow-control-1) section for complete details.

## Metadata

> r[core.metadata]
>
> Requests and Responses carry metadata: a list of key-value pairs
> for out-of-band information (tracing, auth, deadlines, etc.).

Unknown metadata keys MUST be ignored (`r[call.metadata.unknown]`).
See the [Metadata](#metadata-1) section for complete details.

## Idempotency

Connection failures create uncertainty: did the server process the request
before the connection dropped? Nonces enable safe retries across any transport.

> r[core.nonce]
>
> Clients MAY include a nonce in request metadata to enable idempotent
> delivery. The metadata key is `roam-nonce` and the value MUST be
> `MetadataValue::Bytes` containing exactly 16 bytes (128 bits).

> r[core.nonce.generation]
>
> Nonces MUST be generated using a cryptographically secure random source.
> UUIDv4 (random variant) is acceptable.

> r[core.nonce.uniqueness]
>
> Each logically distinct request MUST use a unique nonce. Retrying the
> same logical request (due to transport failure) MUST reuse the original
> nonce.

### Server Deduplication

> r[core.nonce.dedup]
>
> If a server receives a request with a nonce it has processed before
> (within its retention window), it MUST return the cached response
> without re-executing the method handler.

> r[core.nonce.retention]
>
> Servers implementing nonce deduplication MUST retain nonce→response
> mappings for at least 5 minutes. Servers MAY retain them longer.

> r[core.nonce.scope]
>
> Nonce uniqueness is scoped to the server (or logical service instance).
> The same nonce sent to different servers is not deduplicated.

> r[core.nonce.storage]
>
> Servers storing nonce→response mappings MUST protect them appropriately.
> Responses may contain sensitive data.

### Client Retry Behavior

> r[core.nonce.retry]
>
> When retrying a request due to transport failure (connection reset,
> timeout, etc.), clients MUST use the same nonce as the original request.

> r[core.nonce.new-request]
>
> For logically new requests (not retries), clients MUST generate a
> fresh nonce.

### Optional Feature

> r[core.nonce.optional]
>
> Nonces are optional. Requests without a `roam-nonce` metadata entry
> are processed normally without deduplication. Retrying such requests
> may cause duplicate execution.

> r[core.nonce.server-support]
>
> Servers are not required to implement nonce deduplication. Servers
> that do not support it MUST ignore the `roam-nonce` metadata key
> (per `r[call.metadata.unknown]`).

### Integration with Reconnecting Clients

> r[core.nonce.reconnect]
>
> Auto-reconnecting client implementations (see Reconnecting Client
> Specification) SHOULD automatically attach nonces to requests and
> reuse them on retry. This makes reconnection transparent to callers.

> r[core.nonce.channels]
>
> Nonces apply to the initial Request that establishes channels.
> Channel state (data sent/received, credit) is not preserved across
> reconnection. Applications requiring resumable streams should implement
> checkpointing at the application level.

## Topologies

Transports may have different topologies:

- **Peer-to-peer** (TCP, WebSocket, QUIC): Two peers, either can call.
- **Hub** (SHM Hub): One host, multiple peers. Routing is required.

The shared memory transport[^SHM-SPEC] specifies its topology separately.

---

# Transport Bindings

The following sections define how Core Semantics are encoded for specific
transport categories. Each binding specifies message encoding, framing,
connection establishment, and channel ID allocation.

## Service Definitions

A "proto crate" contains one or more "services" (Rust async traits) which
themselves contain one or more "methods" (not functions), which have parameters
and a return type:

```rust
// proto.rs

#[roam::service]
//└────┬────┘         Service definition
pub trait TemplateHost {
//         └────┬─────┘  Service name
    async fn load_template(&self, context_id: ContextId, name: String) -> LoadTemplateResult;
    //       └─────┬──────┘       └──────────────┬────────────────┘    └────────┬──────────┘
    //          Method                       Parameters                     Return type
}

// More services can be defined in the same proto crate...
```

## Method Identity

Every method has a unique 64-bit identifier derived from its service name,
method name, and signature. This is what gets sent on the wire in `Request`
messages.

The method ID ensures that:
- Different services can have methods with the same name without collision
- Changing a method's signature produces a different ID (incompatible)

Collisions are astronomically unlikely — the 64-bit hash space is large enough
that accidental collisions between legitimately different methods won't happen
in practice.

The exact algorithm for computing method IDs is defined in the
[^RUST-SPEC]. Other language
implementations receive pre-computed method IDs from code generation.

## Schema Evolution

Adding new methods to a service is always safe — peers that don't know about
a method will simply report it as unknown.

Most other changes are breaking:
- Renaming a service or method
- Changing argument types, order, or return type
- Changing the structure of any type used in the signature (field names, order, enum variants)
- Substituting container types (e.g., `Vec<T>` → `HashSet<T>`) — these have
  different signature tags even if wire-compatible at the POSTCARD level

Note: Argument *names* are not part of the wire format and can be changed
freely. Only types and their order matter.

## Error Scoping

Errors in roam have different scopes, from narrowest to widest:

**Application errors** are part of the method's return type. A method that
returns `Result<User, UserError>::Err(NotFound)` is a *successful* RPC —
the method ran and returned a value. These are not RPC errors.

**Call errors** mean an RPC failed, but only that specific call is affected.
Other in-flight calls and channels continue normally. Examples:
  * `UnknownMethod` — no handler for this method ID
  * `InvalidPayload` — couldn't deserialize the arguments
  * `Cancelled` — caller cancelled the request

**Channel errors** affect a single channel. The channel is closed but other
channels and calls are unaffected. A peer signals channel errors by sending
Reset.

**Connection errors** are protocol violations. The peer sends a Goodbye
message (citing the violated rule) and closes the connection. Everything
on this connection is torn down. Examples:
  * Data/Close/Reset on an unknown channel ID
  * Data after Close
  * Duplicate in-flight request ID

# RPC Calls

An RPC call is a request/response exchange: one request, one response.
This section specifies the complete lifecycle.

## Request IDs

> r[call.request-id.uniqueness]
>
> A request ID (u64) MUST be unique within a connection. Implementations
> SHOULD use a monotonically increasing counter starting at 1.

> r[call.request-id.duplicate-detection]
>
> If a peer receives a Request with a `request_id` that matches an
> existing in-flight request, it MUST send a Goodbye message (reason:
> `call.request-id.duplicate-detection`) and close the connection.

> r[call.request-id.in-flight]
>
> A request is "in-flight" from when the Request message is sent until
> the corresponding Response message is received.

> r[call.request-id.cancel-still-in-flight]
>
> Sending a Cancel message does NOT remove a request from in-flight status.
> The request remains in-flight until a Response is received (which may be
> a `Cancelled` error, a completed result, or any other response).

For channeling methods, the Request/Response exchange negotiates channels,
but those channels have their own lifecycle independent of the call. See
[Channeling RPC](#channeling-rpc) for details.

## Initiating a Call

> r[call.initiate]
>
> A call is initiated by sending a Request message.

A Request contains:

```rust
Request {
    request_id: u64,
    method_id: u64,
    metadata: Metadata,
    channels: Vec<u64>,  // Channel IDs used by this call, in declaration order
    payload: Vec<u8>,  // [^POSTCARD]-encoded arguments
}
```

> r[call.request.channels]
>
> The `channels` field MUST contain all channel IDs used by the call (both
> `Tx<T>` and `Rx<T>` parameters), in declaration order. This enables
> transparent proxying without parsing the payload.

> r[call.request.channels.schema-driven]
>
> Channel discovery is defined by the method schema, not by byte-by-byte
> inspection of payload values. Implementations MUST traverse only struct
> fields (including tuples) and active enum variant fields when collecting
> channel IDs. They MUST NOT traverse list or map container elements. For
> example, `Vec<T>` values (lists) and `HashMap<K, V>` values (maps) are not
> traversed for channel discovery.

> r[call.request.payload-encoding]
>
> The payload MUST be the [^POSTCARD] encoding of a tuple containing all
> method arguments in declaration order.

For example, a method `fn add(a: i32, b: i32) -> i64` with arguments `(3, 5)`
would have a payload that is the [^POSTCARD] encoding of the tuple `(3i32, 5i32)`.

## Completing a Call

> r[call.complete]
>
> A call is completed by sending a Response message with the same
> `request_id` as the original Request.

A Response contains:

```rust
Response {
    request_id: u64,
    metadata: Metadata,
    payload: Vec<u8>,  // [^POSTCARD]-encoded Result<T, RoamError<E>>
}
```

Where `T` is the method's success type and `E` is the method's error type
(if the method returns `Result<T, E>`).

## Response Encoding

> r[call.response.encoding]
>
> The response payload MUST be the [^POSTCARD] encoding of `Result<T, RoamError<E>>`,
> where `T` and `E` come from the method signature.

For a method declared as:

```rust
async fn get_user(&self, id: UserId) -> Result<User, UserError>;
```

The response payload is `Result<User, RoamError<UserError>>`.

For a method that cannot fail at the application level:

```rust
async fn ping(&self) -> Pong;
```

The response payload is `Result<Pong, RoamError<Infallible>>` (or an
equivalent encoding where the `User` variant cannot occur).

## Metadata

Requests and Responses carry a `metadata` field for out-of-band information.

> r[call.metadata.type]
>
> Metadata is a list of entries: `Vec<(String, MetadataValue, u64)>`.
> Each entry is a triple of (key, value, flags).

```rust
enum MetadataValue {
    String(String),  // 0
    Bytes(Vec<u8>),  // 1
    U64(u64),        // 2
}

// Metadata entry: (key, value, flags)
type Metadata = Vec<(String, MetadataValue, u64)>;
```

> r[call.metadata.flags]
>
> The flags field is a `u64` bitfield controlling metadata handling:
>
> | Bit | Name | Meaning |
> |-----|------|---------|
> | 0 | `SENSITIVE` | Value MUST NOT be logged, traced, or included in error messages |
> | 1 | `NO_PROPAGATE` | Value MUST NOT be forwarded to downstream calls |
> | 2-63 | Reserved | MUST be zero; peers MUST ignore unknown flag bits |
>
> Flags are encoded as a varint, so common values (0, 1, 2, 3) use only 1 byte.

> r[call.metadata.keys]
>
> Metadata keys are case-sensitive strings. Keys MUST be at most 256
> bytes (UTF-8 encoded).

> r[call.metadata.duplicates]
>
> Duplicate keys are allowed. If multiple entries have the same key,
> all values are preserved in order. Consumers MAY use any of the values
> (typically the first or last).

> r[call.metadata.order]
>
> Metadata order MUST be preserved during transmission. Order is not
> semantically meaningful for most uses, but some applications may
> rely on it (e.g., multi-value headers).

> r[call.metadata.unknown]
>
> Unknown metadata keys MUST be ignored.

> r[call.metadata.limits]
>
> Metadata limits:
> - At most 128 metadata entries (key-value pairs)
> - Each key at most 256 bytes
> - Each value at most 16 KB (16,384 bytes)
> - Total metadata size at most 64 KB (65,536 bytes)
>
> If a peer receives a message exceeding these limits, it MUST send a
> Goodbye message (reason: `call.metadata.limits`) and close the
> connection.

### Example Uses

Metadata is application-defined. Common uses include:

- **Deadlines**: Absolute timestamp after which the caller no longer cares
- **Distributed tracing**: W3C traceparent/tracestate, or other trace IDs
- **Authentication**: Bearer tokens, API keys, signatures (with `SENSITIVE` flag)
- **Priority**: Scheduling hints for request processing order
- **Compression**: Indicating payload compression scheme

### Flag Usage Examples

Authentication tokens should be marked sensitive to prevent logging:

```rust
metadata.push((
    "authorization".to_string(),
    MetadataValue::String("Bearer sk-...".to_string()),
    SENSITIVE,  // bit 0 = don't log this value
));
```

Session tokens that shouldn't leak to downstream services:

```rust
metadata.push((
    "session-id".to_string(),
    MetadataValue::String(session_id),
    SENSITIVE | NO_PROPAGATE,  // bits 0+1 = don't log, don't forward
));
```

## RoamError

> r[call.error.roam-error]
>
> `RoamError<E>` distinguishes application errors from protocol errors.
> The variant order defines wire discriminants ([^POSTCARD] varint encoding):

| Discriminant | Variant | Payload | Meaning |
|--------------|---------|---------|---------|
| 0 | `User` | `E` | Application returned an error |
| 1 | `UnknownMethod` | none | No handler for this `method_id` |
| 2 | `InvalidPayload` | none | Could not deserialize request arguments |
| 3 | `Cancelled` | none | Caller cancelled the request |

In Rust syntax (for clarity):

```rust
enum RoamError<E> {
    User(E),         // 0
    UnknownMethod,   // 1
    InvalidPayload,  // 2
    Cancelled,       // 3
}
```

> r[call.error.user]
>
> The `User(E)` variant (discriminant 0) carries the application's error
> type. This is semantically different from protocol errors — the method
> ran and returned `Err(e)`.

> r[call.error.protocol]
>
> Discriminants 1-3 are protocol-level errors. The method may not have
> run at all (UnknownMethod, InvalidPayload) or was interrupted
> (Cancelled).

This design means callers always know: "Did my application logic fail,
or did the RPC infrastructure fail?"

### Returning Call Errors

> r[call.error.unknown-method]
>
> If a callee receives a Request with a `method_id` it does not recognize,
> it MUST send a Response with `Err(RoamError::UnknownMethod)`. The
> connection remains open.

> r[call.error.invalid-payload]
>
> If a callee cannot deserialize the Request payload, it MUST send a
> Response with `Err(RoamError::InvalidPayload)`. The connection
> remains open.

## Call Lifecycle

The complete lifecycle of an RPC call:

```aasvg
.--------.                                        .--------.
| Caller |                                        | Callee |
'---+----'                                        '---+----'
    |                                                 |
    +-------- Request(id=1, method, payload) -------->|
    |                                                 |
    |                                      [execute handler]
    |                                                 |
    |<------- Response(id=1, Ok(payload)) ------------+
    |                                                 |
```

> r[call.lifecycle.single-response]
>
> For each Request, the callee MUST send exactly one Response with the
> same `request_id`. No more, no less.

> r[call.lifecycle.ordering]
>
> Responses MAY arrive in any order. The caller MUST use `request_id`
> for correlation, not arrival order.

> r[call.response.unknown-request-id]
>
> If a caller receives a Response with a `request_id` that does not match
> any in-flight request, it MUST treat this as a protocol violation:
> send `Goodbye` on connection 0 (reason: `call.response.unknown-request-id`)
> and close the link.

> r[call.response.stale-timeout]
>
> Implementations MAY enforce a timeout for pending responses. If the timeout
> is exceeded, they MUST fail pending calls with a connection error, send
> `Goodbye` on connection 0 (reason: `call.response.stale-timeout`), and close
> the link.

## Cancellation

```rust
Cancel {
    request_id: u64,  // The request to cancel
}
```

> r[call.cancel.message]
>
> A caller MAY send a Cancel message to request that the callee stop
> processing a request. The Cancel message MUST include the `request_id`
> of the request to cancel.

> r[call.cancel.best-effort]
>
> Cancellation is best-effort. The callee MAY have already completed the
> request, or MAY be unable to cancel in-progress work. The callee MUST
> still send a Response (either the completed result or `Cancelled` error).

> r[call.cancel.no-response-required]
>
> The caller MUST NOT wait indefinitely for a response after sending Cancel.
> Implementations SHOULD use a timeout after which the caller considers the
> request cancelled locally, even without a response.

## Pipelining

> r[call.pipelining.allowed]
>
> Multiple requests MAY be in flight simultaneously. The caller does not
> need to wait for a response before sending the next request.

> r[call.pipelining.independence]
>
> Each request is independent. A slow or failed request MUST NOT block
> other requests.

This enables efficient batching — a caller can send 10 requests, then
await all 10 responses, rather than round-tripping each one sequentially.

# Channeling RPC

Channeling methods have `Tx<T>` (caller→callee) or `Rx<T>` (callee→caller)
in argument position. Unlike simple RPC calls, data flows continuously over dedicated
channels.

## Tx and Rx Types

> r[channeling.type]
>
> `Tx<T>` and `Rx<T>` are roam-provided types recognized by the
> `#[roam::service]` macro. On the wire, both serialize as a `u64` channel ID.

> r[channeling.caller-pov]
>
> Service definitions are written from the **caller's perspective**.
> `Tx<T>` means "caller transmits data to callee". `Rx<T>` means
> "caller receives data from callee".

> r[channeling.holder-semantics]
>
> From the holder's perspective: `Tx<T>` means "I send on this",
> `Rx<T>` means "I receive from this". Generated callee handlers
> have the types flipped relative to the service definition.

Example:

```rust
// Service definition (caller's perspective)
#[roam::service]
pub trait Channeling {
    async fn sum(&self, numbers: Tx<u32>) -> u32;       // caller→callee
    async fn range(&self, n: u32, output: Rx<u32>);     // callee→caller
}

// Generated caller stub — same types as definition
impl ChannelingClient {
    async fn sum(&self, numbers: Tx<u32>) -> u32;       // caller sends
    async fn range(&self, n: u32, output: Rx<u32>);     // caller receives
}

// Generated callee handler — types flipped
trait ChannelingHandler {
    async fn sum(&self, numbers: Rx<u32>) -> u32;       // callee receives
    async fn range(&self, n: u32, output: Tx<u32>);     // callee sends
}
```

The number of channels in a call is not always obvious from the method
signature — they may appear inside enums, so the actual IDs present depend
on which variant is passed.

## Channel ID Allocation

> r[channeling.allocation.caller]
>
> The **caller** allocates ALL channel IDs (both Tx and Rx). Channel IDs
> are listed in the Request's `channels` field (see `r[call.request.channels]`)
> and also serialized within `Tx<T>`/`Rx<T>` values in the payload.
> The callee does not allocate any IDs.
>
> On the server side, implementations MUST use the channel IDs from the
> `channels` field as authoritative, patching them into deserialized args
> before binding streams. This ensures transparent proxying can work without
> parsing the payload.

> r[channeling.id.uniqueness]
>
> A channel ID MUST be unique within a connection.

> r[channeling.id.zero-reserved]
>
> Channel ID 0 is reserved. If a peer receives a channel message with
> `channel_id` of 0, it MUST send a Goodbye message (reason:
> `channeling.id.zero-reserved`) and close the connection.

> r[channeling.id.parity]
>
> For peer-to-peer transports, the **link initiator** (who opened the link)
> MUST allocate odd channel IDs (1, 3, 5, ...). The **link acceptor** MUST
> allocate even channel IDs (2, 4, 6, ...). This prevents collisions when
> both peers make concurrent calls.

> r[channeling.id.parity.virtual]
>
> For virtual connections (opened via `Connect`/`Accept`), the parity rule
> is inherited from the link: the link initiator uses odd IDs on all
> connections, and the link acceptor uses even IDs on all connections.
> This is independent of who sent the `Connect` request.

Note: "Initiator" and "acceptor" refer to who opened the link, not who is
calling whom or who opened the virtual connection. If the link initiator
calls with a Tx and Rx, both use odd IDs (e.g., tx=1, rx=3). If the link
acceptor calls back, they use even IDs (e.g., tx=2, rx=4).

## Call Lifecycle with Channels

### Caller Channeling (Tx): `sum(numbers: Tx<u32>) -> u32`

```
Caller (initiator)                         Callee (acceptor)
    |                                          |
    |-- Request(sum, tx=1) ------------------->|
    |-- Data(channel=1, 10) ------------------>|
    |-- Data(channel=1, 20) ------------------>|
    |-- Close(channel=1) --------------------->|
    |                                          |
    |<-- Response(Ok, 30) --------------------|
```

### Callee Channeling (Rx): `range(n, output: Rx<u32>)`

```
Caller (initiator)                         Callee (acceptor)
    |                                          |
    |-- Request(range, n=3, rx=1) ------------>|
    |                                          |
    |<-- Data(channel=1, 0) -------------------|
    |<-- Data(channel=1, 1) -------------------|
    |<-- Data(channel=1, 2) -------------------|
    |<-- Response(Ok, ()) --------------------|  // rx channel implicitly closed
```

### Bidirectional: `pipe(input: Tx, output: Rx)`

```
Caller (initiator)                         Callee (acceptor)
    |                                          |
    |-- Request(pipe, tx=1, rx=3) ------------>|
    |-- Data(channel=1, "a") ----------------->|
    |<-- Data(channel=3, "a") -----------------|
    |-- Data(channel=1, "b") ----------------->|
    |<-- Data(channel=3, "b") -----------------|
    |-- Close(channel=1) --------------------->|
    |<-- Response(Ok, ()) --------------------|  // rx=3 closed
```

> r[channeling.lifecycle.immediate-data]
>
> The caller MAY send Data on `Tx<T>` channels immediately after sending
> the Request, without waiting for Response. This enables pipelining for
> lower latency.

> r[channeling.lifecycle.speculative]
>
> If the caller sends Data before receiving Response, and the Response
> is an error (`Err(RoamError::UnknownMethod)`, `Err(RoamError::InvalidPayload)`,
> etc.), the Data was wasted. The channel IDs are "burned" — they were
> never successfully opened and MUST NOT be reused.

> r[channeling.lifecycle.response-closes-pulls]
>
> When the callee sends Response, all `Rx<T>` channels are implicitly
> closed. The callee MUST NOT send Data on any Rx channel after sending Response.

> r[channeling.lifecycle.caller-closes-pushes]
>
> The caller MUST send Close on each `Tx<T>` channel when done sending.
> The callee waits for Close before it knows all input has arrived.

> r[channeling.error-no-channels]
>
> `Tx<T>` and `Rx<T>` MUST NOT appear inside error types. A method's
> error type `E` in `Result<T, E>` MUST NOT contain `Tx<T>` or `Rx<T>`
> at any nesting level.

## Channel Data Flow

> r[channeling.data]
>
> The sending peer sends Data messages containing [^POSTCARD]-encoded values
> of the channel's element type `T`. Each Data message contains exactly
> one value.

> r[channeling.data.size-limit]
>
> Each channel element MUST NOT exceed `max_payload_size` bytes (the same
> limit that applies to Request/Response payloads). If a peer receives
> a channel element exceeding this limit, it MUST send a Goodbye message
> (reason: `channeling.data.size-limit`) and close the connection.

> r[channeling.data.invalid]
>
> If a peer receives a Data message that cannot be deserialized as the
> channel's element type, it MUST send a Goodbye message (reason:
> `channeling.data.invalid`) and close the connection.

> r[channeling.close]
>
> For `Tx<T>` (caller→callee), the caller sends Close when done.
> For `Rx<T>` (callee→caller), the channel closes implicitly with Response.

> r[channeling.data-after-close]
>
> If a peer receives a Data message on a channel after it has been
> closed, it MUST send a Goodbye message (reason: `channeling.data-after-close`)
> and close the connection.

## Resetting a Channel

> r[channeling.reset]
>
> Either peer MAY send Reset to forcefully terminate a channel.
> The sender uses Reset to abandon early; the receiver uses Reset to signal
> it no longer wants data.

> r[channeling.reset.effect]
>
> Upon receiving Reset, the peer MUST consider the channel terminated.
> Any further Data, Close, or Credit messages for that ID MUST be ignored
> (they may arrive due to race conditions).

> r[channeling.reset.credit]
>
> When a channel is reset, any outstanding credit is lost.

> r[channeling.unknown]
>
> If a peer receives a channel message (Data, Close, Reset, Credit) with a
> `channel_id` that was never opened, it MUST send a Goodbye message
> (reason: `channeling.unknown`) and close the connection.

## Channels and Call Completion

> r[channeling.call-complete]
>
> The RPC call completes when the Response is received. At that point:
> - All `Rx<T>` channels are closed (callee can no longer send)
> - `Tx<T>` channels may still be open (caller may still be sending)
> - The request ID is no longer in-flight

> r[channeling.channels-outlive-response]
>
> `Tx<T>` channels (caller→callee) may outlive the Response. The caller
> continues sending until they send Close. The callee processes the final
> return value only after all input channels are closed.

# Flow Control

Flow control prevents fast senders from overwhelming slow receivers.
roam uses credit-based flow control for channels on all transports.

## Channel Flow Control

> r[flow.channel.credit-based]
>
> Channels use credit-based flow control. A sender MUST NOT send
> a Data message if doing so would exceed the remaining credit for that
> channel — even if the underlying transport would accept the data.

> r[flow.channel.all-transports]
>
> Credit-based flow control applies to all transports for both `Tx<T>`
> and `Rx<T>` channels. On multi-stream transports (QUIC, WebTransport),
> roam credit operates independently of any transport-level flow control.
> The transport may additionally block writes, but that is transparent
> to the roam layer.

### Byte Accounting

> r[flow.channel.byte-accounting]
>
> Credits are measured in bytes. The byte count for a channel element is
> the length of its [^POSTCARD] encoding — the same bytes that appear in
> `Data.payload`, or on multi-stream transports, the bytes written to the
> dedicated transport stream before length-prefix framing. Framing overhead
> (length-prefix header, transport headers) is NOT counted.

### Initial Credit

> r[flow.channel.initial-credit]
>
> The initial channel credit MUST be negotiated during handshake. Each
> channel starts with this amount of credit independently.

Both peers advertise their `initial_channel_credit` in Hello. The effective
initial credit is the minimum of both values. Each channel ID gets its
own independent credit counter starting at this value.

### Granting Credit

```rust
Credit {
    channel_id: u64,
    bytes: u32,  // additional bytes granted
}
```

> r[flow.channel.credit-grant]
>
> A receiver grants additional credit by sending a Credit message. The
> `bytes` field is added to the sender's available credit for that channel.

> r[flow.channel.credit-additive]
>
> Credits are additive. If a receiver grants 1000 bytes, then grants 500
> more, the sender has 1500 bytes available.

> r[flow.channel.credit-prompt]
>
> Credit messages SHOULD be processed in receive order without intentional
> delay. Starving Credit processing can cause unnecessary stalls.

### Consuming Credit

> r[flow.channel.credit-consume]
>
> Sending a channel element consumes credits equal to its byte count (see
> `r[flow.channel.byte-accounting]`). The sender MUST track remaining
> credit and MUST NOT send if it would result in negative credit.

### Credit Overrun

> r[flow.channel.credit-overrun]
>
> If a receiver receives a channel element whose byte count exceeds the
> remaining credit for that channel, it MUST send a Goodbye message
> (reason: `flow.channel.credit-overrun`) and close the connection.

Credit overrun indicates a buggy or malicious peer.

### Zero Credit

> r[flow.channel.zero-credit]
>
> If a sender has zero remaining credit for a channel, it MUST wait for
> a Credit message before sending more data. This is not a protocol
> error — the receiver controls the pace.

If progress stops entirely, implementations should use application-level
timeouts. A sender may Reset the channel or close the connection if no
credit arrives within a reasonable time.

### Close and Credit

> r[flow.channel.close-exempt]
>
> Close messages (and Reset) do not consume credit. A sender MAY always
> send Close regardless of credit state. This ensures channels can always
> be closed.

### Infinite Credit Mode

> r[flow.channel.infinite-credit]
>
> Implementations MAY use "infinite credit" mode by setting a very large
> initial credit (e.g., `u32::MAX`). This disables backpressure but
> simplifies implementation. The protocol semantics remain the same.

### Implementation Guidance (Non-normative)

When to grant credits:

- **Simplest**: Grant credit after your application has consumed buffered
  data. This provides true end-to-end backpressure.
- **Acceptable**: Grant credit when you buffer incoming data into a bounded
  queue (you've reserved space). This allows some pipelining.
- **Avoid**: Granting far ahead without a hard cap, unless you truly want
  infinite-credit behavior.

Hysteresis pattern: Maintain a target window `W` (often equal to the
negotiated initial credit). When remaining credit drops below `W/2`,
send a Credit message to bring it back near `W`. This avoids sending
many small Credit messages.

## RPC Call Flow Control

> r[flow.call.payload-limit]
>
> RPC call (Request/Response) payloads are bounded by `max_payload_size`
> negotiated during handshake. No credit-based flow control is used.

The natural pipelining limit (waiting for responses) provides implicit
flow control for RPC calls.

# Messages

Everything roam does — method calls, channels, control signals — is
built on messages exchanged between peers.

```rust
enum Message {
    // Link control (no conn_id - applies to entire link)
    Hello(Hello),
    
    // Virtual connection control
    Connect { request_id: u64, metadata: Metadata },
    Accept { request_id: u64, conn_id: u64, metadata: Metadata },
    Reject { request_id: u64, reason: String, metadata: Metadata },
    
    // Connection control (conn_id scoped)
    Goodbye { conn_id: u64, reason: String },
    
    // RPC (conn_id scoped)
    Request { conn_id: u64, request_id: u64, method_id: u64, metadata: Metadata, channels: Vec<u64>, payload: Vec<u8> },
    Response { conn_id: u64, request_id: u64, metadata: Metadata, payload: Vec<u8> },
    Cancel { conn_id: u64, request_id: u64 },
    
    // Channels (conn_id scoped)
    Data { conn_id: u64, channel_id: u64, payload: Vec<u8> },
    Close { conn_id: u64, channel_id: u64 },
    Reset { conn_id: u64, channel_id: u64 },
    Credit { conn_id: u64, channel_id: u64, bytes: u32 },
}
```

> r[message.conn-id]
>
> All messages except `Hello`, `Connect`, `Accept`, and `Reject` include
> a `conn_id` field identifying which virtual connection they belong to.
> Messages with an unknown `conn_id` MUST be handled as a protocol error
> (send Goodbye on connection 0 and close the link).

Messages are [^POSTCARD]-encoded. The enum discriminant identifies the message
type, and each variant contains only the fields it needs.

> r[message.unknown-variant]
>
> If a peer receives a Message with an unknown enum discriminant, it
> MUST send a Goodbye message (reason: `message.unknown-variant`) and
> close the connection.

> r[message.decode-error]
>
> If a peer cannot decode a received message (invalid [^POSTCARD] encoding,
> length-prefix framing error, or malformed fields), it MUST send a Goodbye
> message (reason: `message.decode-error`) and close the connection.

## Message Types

### Hello

> r[message.hello.timing]
>
> Both peers MUST send a Hello message immediately after connection
> establishment, before any other message.

> r[message.hello.structure]
>
> Hello is an enum to allow future versions.

> r[message.hello.unknown-version]
>
> If a peer receives a Hello with an unknown variant, it MUST send a
> Goodbye message (with reason containing `message.hello.unknown-version`)
> and close the connection.

> r[message.hello.ordering]
>
> A peer MUST NOT send any message other than Hello until it has both
> sent and received Hello.

```rust
enum Hello {
    V4 {
        max_payload_size: u32,
        initial_channel_credit: u32,
    },
}
```

| Field | Description |
|-------|-------------|
| `max_payload_size` | Maximum bytes in a Request/Response payload |
| `initial_channel_credit` | Bytes of credit each channel starts with |

> r[message.hello.negotiation]
>
> The effective limits for a connection are the minimum of both peers'
> advertised values.

> r[message.hello.enforcement]
>
> If a peer receives a Request or Response whose payload exceeds the
> negotiated `max_payload_size`, it MUST send a Goodbye message
> (reason: `message.hello.enforcement`) and close the link.

### Connect / Accept / Reject

These messages manage virtual connections on a link.

```rust
Connect { request_id: u64, metadata: Metadata }
Accept { request_id: u64, conn_id: u64, metadata: Metadata }
Reject { request_id: u64, reason: String, metadata: Metadata }
```

> r[message.connect.initiate]
>
> Either peer MAY send `Connect` to request a new virtual connection.
> The `request_id` is used to correlate the response (Accept or Reject).
> Connect request IDs are scoped to the link, not to any connection.

> r[message.connect.metadata]
>
> Connect metadata MAY include authentication tokens, routing hints,
> tracing context, or application-specific data. The same metadata
> limits apply as for RPC calls (see `r[call.metadata.limits]`).

> r[message.connect.request-id]
>
> Connect `request_id`s MUST be unique among in-flight Connect requests
> on the link. They are independent of RPC request IDs.

> r[message.accept.response]
>
> If the peer is listening for incoming connections (via
> `take_incoming_connections()` on connection 0), it responds with
> `Accept`, providing the newly allocated `conn_id`. The new connection
> is immediately usable.

> r[message.accept.metadata]
>
> Accept metadata MAY include server-assigned session information,
> connection-specific configuration, or tracing context.

> r[message.reject.response]
>
> If the peer is not listening for incoming connections, or if the
> connection is rejected for any other reason (auth failure, rate
> limiting, etc.), it MUST respond with `Reject`.

> r[message.reject.reason]
>
> The `reason` field SHOULD describe why the connection was rejected.
> Common reasons include: "not listening", "unauthorized", "rate limited".
> Metadata MAY provide additional structured rejection information.

> r[message.connect.timeout]
>
> If no Accept or Reject is received within a reasonable time,
> implementations SHOULD treat the Connect as failed. The `request_id`
> is "burned" and MUST NOT be reused.

### Goodbye

Goodbye closes a virtual connection. The `conn_id` field specifies which
connection to close.

```rust
Goodbye { conn_id: u64, reason: String }
```

> r[message.goodbye.send]
>
> A peer MUST send a Goodbye message before closing a virtual connection
> due to a protocol error. The `reason` field MUST contain the rule ID
> that was violated (e.g., `channeling.id.zero-reserved`), optionally
> followed by additional context.

> r[message.goodbye.receive]
>
> Upon receiving a Goodbye message, a peer MUST stop sending messages
> on that connection. All in-flight requests on that connection fail
> with a connection error. All open channels on that connection are
> terminated. Other connections on the same link are unaffected.

> r[message.goodbye.connection-zero]
>
> Sending Goodbye on connection 0 closes the entire link. All
> virtual connections are terminated, and the underlying transport
> (TCP socket, WebSocket, etc.) is closed.

> r[message.goodbye.graceful]
>
> A peer MAY send Goodbye with an empty reason for graceful shutdown
> (not due to an error). This is the normal way to close a connection.

### Request / Response / Cancel

`Request` initiates an RPC call. `Response` returns the result. `Cancel`
requests that the callee stop processing a request.

The `request_id` correlates requests with responses, enabling multiple
calls to be in flight simultaneously (pipelining).

### Data / Close / Reset

`Data` carries payload bytes on a channel, identified by `channel_id`.
`Close` signals end-of-channel — the sender is done (see `r[core.channel.close]`).
`Reset` forcefully terminates a channel.


# Transports

Different transports require different handling:

| Kind | Example | Framing | Channels |
|------|---------|---------|---------|
| Message | WebSocket | Transport provides | All in one |
| Multi-stream | QUIC | Per stream | Can map to transport streams |
| Byte stream | TCP | 4-byte length prefix | All in one |

## Message Transports

Message transports (like WebSocket) deliver discrete messages.

> r[transport.message.one-to-one]
>
> Each transport message MUST contain exactly one roam message,
> [^POSTCARD]-encoded. Fragmentation and reassembly are not supported.

> r[transport.message.binary]
>
> Transport messages MUST be binary (not text). For WebSocket, this
> means binary frames, not text frames.

> r[transport.message.multiplexing]
>
> All messages (control, RPC, channel data) flow through the same
> link. The `channel_id` and `conn_id` fields provide multiplexing.

## Multi-stream Transports

Multi-stream transports (like QUIC, WebTransport) provide multiple independent
streams, which can eliminate head-of-line blocking.

See the [Multi-stream Transport Specification](/multistream-spec/) for the
complete binding specification. This is tracked separately as it is not yet
implemented.

## Byte Stream Transports

Byte stream transports (like TCP) provide a single ordered byte stream.

> r[transport.bytestream.length-prefix]
>
> Messages MUST be framed using a 4-byte little-endian length prefix
> followed by exactly that many message bytes.
> 
> ```
> [len: u32 LE][message bytes][len: u32 LE][message bytes]...
> ```

All messages flow through the single byte stream. The `channel_id` field
in channel messages provides multiplexing.

# Wire Examples (Non-normative)

These examples illustrate protocol behavior on byte-stream transports.

## Hello Negotiation and RPC Call

```aasvg
.-----------.                                           .-----------.
| Initiator |                                           | Acceptor  |
'-----+-----'                                           '-----+-----'
      |                                                       |
      |-------- Hello { max=64KB, credit=16KB } ------------->|
      |<------- Hello { max=32KB, credit=8KB } ---------------|
      |                                                       |
      |            .----------------------------.             |
      |            | negotiated: max=32KB       |             |
      |            |            credit=8KB      |             |
      |            '----------------------------'             |
      |                                                       |
      |-------- Request { id=1, method=0xABC } -------------->|
      |                                                       |
      |<------- Response { id=1, Ok(result) } ----------------|
      |                                                       |
```

## Unknown Method Error

```aasvg
.--------.                                              .--------.
| Caller |                                              | Callee |
'---+----'                                              '---+----'
    |                                                       |
    |-------- Request { id=2, method=0xDEAD } ------------->|
    |                                                       |
    |<------- Response { id=2, Err(UnknownMethod) } --------|
    |                                                       |
    |                [connection remains open]              |
    |                                                       |
```

## Caller Channeling (Push) with Credit

```aasvg
.--------.                                              .--------.
| Caller |                                              | Callee |
'---+----'                                              '---+----'
    |                                                       |
    |-------- Request { id=3, channel_id=1 } -------------->|
    |                                                       |
    |         .---------------------------------.            |
    |         | channel 1 open; credit=8KB     |            |
    |         '---------------------------------'            |
    |                                                       |
    |-------- Data { channel=1, 4KB } --------------------->| credit: 8K->4K
    |-------- Data { channel=1, 4KB } --------------------->| credit: 4K->0K
    |                                                       |
    |         .---------------------------------.            |
    |         | sender blocks, no credit       |            |
    |         '---------------------------------'            |
    |                                                       |
    |<------- Credit { channel=1, bytes=8KB } --------------|
    |                                                       |
    |-------- Data { channel=1, 2KB } --------------------->|
    |-------- Close { channel=1 } ------------------------>|
    |                                                       |
    |<------- Response { id=3, Ok(result) } ----------------|
    |                                                       |
```

## Callee Channeling (Pull)

```aasvg
.--------.                                              .--------.
| Caller |                                              | Callee |
'---+----'                                              '---+----'
    |                                                       |
    |-------- Request { id=4, channel_id=1 } -------------->|
    |                                                       |
    |<------- Data { channel=1, value } --------------------|
    |<------- Data { channel=1, value } --------------------|
    |<------- Data { channel=1, value } --------------------|
    |<------- Response { id=4, Ok(()) } --------------------|
    |                                                       |
    |         .---------------------------------.            |
    |         | channel 1 implicitly closed    |            |
    |         '---------------------------------'            |
    |                                                       |
```

## Bidirectional (Push + Pull)

```aasvg
.--------.                                              .--------.
| Caller |                                              | Callee |
'---+----'                                              '---+----'
    |                                                       |
    |-- Request { id=5, channel_ids=[1,3] } --------------->|
    |-- Data { channel=1, "hello" } ----------------------->|
    |<- Data { channel=3, "hello" } ------------------------|
    |-- Data { channel=1, "world" } ----------------------->|
    |<- Data { channel=3, "world" } ------------------------|
    |-- Close { channel=1 } ------------------------------->|
    |<- Response { id=5, Ok(()) } --------------------------|
    |                                                       |
    |         .---------------------------------.            |
    |         | channel 3 closed with Response |            |
    |         '---------------------------------'            |
    |                                                       |
```

## Reset Handling

```aasvg
.--------.                                              .----------.
| Sender |                                              | Receiver |
'---+----'                                              '----+-----'
    |                                                        |
    |-------- Data { channel=5, chunk } -------------------->|
    |                                                        |
    |<------- Reset { channel=5 } ---------------------------|
    |                                                        |
    |         .---------------------------------.            |
    |         | sender stops; in-flight msgs   |             |
    |         | for channel 5 are ignored      |             |
    |         '---------------------------------'            |
    |                                                        |
```

## Connection Error (Goodbye)

```aasvg
.------.                                                .------.
| Peer |                                                | Peer |
'--+---'                                                '--+---'
   |                                                       |
   |-------- Data { conn=0, channel=99 } ----------------->|
   |                                                       |
   |          .---------------------------------.          |
   |          | channel 99 was never opened!   |           |
   |          '---------------------------------'          |
   |                                                       |
   |<------- Goodbye { conn=0, reason="channeling.unknown" }|
   |                                                       |
   X                  [link closed]                        X
   |                                                       |
```

## Virtual Connection: Opening

```aasvg
.-----------.                                           .-----------.
| Initiator |                                           | Acceptor  |
'-----+-----'                                           '-----+-----'
      |                                                       |
      |          [Hello exchange on transport]                |
      |                                                       |
      |-------- Connect { request_id=1 } -------------------->|
      |                                                       |
      |          .---------------------------------.          |
      |          | Acceptor is listening          |           |
      |          | (called take_incoming_conns)   |           |
      |          '---------------------------------'          |
      |                                                       |
      |<------- Accept { request_id=1, conn_id=1 } -----------|
      |                                                       |
      |          .---------------------------------.          |
      |          | Connection 1 now usable        |           |
      |          '---------------------------------'          |
      |                                                       |
      |-- Request { conn=1, id=1, method=0xABC } ------------>|
      |<- Response { conn=1, id=1, Ok(result) } --------------|
      |                                                       |
```

## Virtual Connection: Rejected

```aasvg
.-----------.                                           .-----------.
| Initiator |                                           | Acceptor  |
'-----+-----'                                           '-----+-----'
      |                                                       |
      |-------- Connect { request_id=1 } -------------------->|
      |                                                       |
      |          .---------------------------------.          |
      |          | Acceptor is NOT listening      |           |
      |          '---------------------------------'          |
      |                                                       |
      |<------- Reject { request_id=1 } ----------------------|
      |                                                       |
      |          .---------------------------------.          |
      |          | Connection refused             |           |
      |          | Link still open                |           |
      |          '---------------------------------'          |
      |                                                       |
```

## Virtual Connection: Proxy Multiplexing

This example shows the motivating use case — a proxy (cell-http) maps
multiple downstream clients to separate upstream virtual connections.

```aasvg
.----------.          .-----------.          .------.
| Browser1 |          | cell-http |          | Host |
'----+-----'          '-----+-----'          '--+---'
     |                      |                   |
     |                      |<===== transport ==|  (single TCP conn)
     |                      |                   |
     |<-- ws1 connected --->|                   |
     |                      |-- Connect{id=1} ->|
     |                      |<- Accept{id=1,c=1}|  (conn 1 for browser1)
     |                      |                   |
     |<-- ws2 connected --->|                   |
     |                      |-- Connect{id=2} ->|
     |                      |<- Accept{id=2,c=2}|  (conn 2 for browser2)
     |                      |                   |
     |-- subscribe("/x") -->|                   |
     |                      |-- Request{c=1} -->|  Host sees conn 1
     |                      |<- Response{c=1} --|
     |<- Rx<Update> --------|                   |
     |                      |                   |
     |                      |                   |
     |                      |<- Request{c=1} ---|  Host calls back on conn 1
     |<-- callback ---------|                   |  → routed to browser1
     |                      |                   |
```

## Virtual Connection: Closing

```aasvg
.-----------.                                           .-----------.
| Peer A    |                                           | Peer B    |
'-----+-----'                                           '-----+-----'
      |                                                       |
      |          [conn 1 established, in use]                 |
      |                                                       |
      |-------- Goodbye { conn=1, reason="" } --------------->|
      |                                                       |
      |          .---------------------------------.          |
      |          | Connection 1 closed            |           |
      |          | Connection 0 still open        |           |
      |          '---------------------------------'          |
      |                                                       |
      |-- Request { conn=0, id=5, method } ------------------>|
      |<- Response { conn=0, id=5, Ok } ----------------------|
      |                                                       |
```

# Introspection

Peers MAY implement introspection services to help debug method mismatches
and explore available services. See the
[roam-discovery](https://crates.io/crates/roam-discovery) crate for
the standard introspection service definition and types.

# Design Rationale (Non-normative)

This section explains key design decisions.

## Why Tuple Encoding for Arguments?

Method arguments are encoded as a tuple, not a struct with named fields.
This matches how Rust function calls work — argument names are not part
of the ABI. It also produces smaller wire payloads since field names
aren't transmitted.

The tradeoff is that argument order matters for compatibility. Reordering
arguments is a breaking change.


## Why Signature Hashing Includes Field/Variant Names?

Including struct field names and enum variant names in the signature
hash means renaming them is a breaking change. This is intentional:

- Field names affect serialization (POSTCARD uses field order, but
  other formats might use names)
- Variant names are semantically meaningful
- Silent mismatches are worse than loud failures

If you need to rename a field, add a new method instead.

## Why Connection-Level Errors for Some Violations?

Some errors (like data on an unknown channel) are connection errors
rather than channel errors because:

- They indicate a fundamental protocol mismatch or bug
- Recovery is unlikely to succeed
- Continuing could cause cascading confusion

Channel-scoped errors (Reset) are for application-level issues where
the connection can continue serving other channels.

# References

[^POSTCARD]: Postcard Wire Format Specification - <https://postcard.jamesmunns.com/wire-format>

[^RUST-SPEC]: roam Rust Implementation Specification - <@/rust-spec/_index.md>

[^SHM-SPEC]: roam Shared Memory Transport Specification - <@/shm-spec/_index.md>
