+++
title = "roam specification"
description = "Formal roam RPC protocol specification"
weight = 10
+++

# Introduction

This is roam specification v1.1.0, last updated January 7, 2026. It canonically
lives at <https://github.com/bearcove/roam> — where you can get the latest version.

roam is a **Rust-native** RPC protocol. We don't claim to be language-neutral —
Rust is the lowest common denominator. There is no independent schema language;
Rust traits *are* the schema. Implementations for other languages (Swift,
TypeScript, etc.) are generated from Rust definitions.

This means:
- The Rust Implementation Specification [RUST-SPEC] is essential
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
with facet-postcard (see [POSTCARD]).

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

## Connections and Peers

A **connection** is a communication context between two **peers**. How
connections are established is transport-specific (TCP connect, SHM
segment mapping, etc.).

Each connection has two endpoints. For peer-to-peer transports, one is
the **initiator** (opened the connection) and the other is the **acceptor**.
This distinction affects stream ID allocation but not who can call whom —
either peer can initiate calls.

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

## Streams (Push/Pull)

A **stream** is a unidirectional, ordered sequence of typed values. At the
type level, roam provides `Push<T>` and `Pull<T>` to indicate direction.

> r[core.stream]
>
> `Push<T>` represents data flowing from **caller to callee** (input).
> `Pull<T>` represents data flowing from **callee to caller** (output).
> Each has exactly one sender and one receiver.

On the wire, both `Push<T>` and `Pull<T>` serialize as a `stream_id`
(u64). The direction is determined by the type, not the ID.
See `r[streaming.type]` for details.

> r[core.stream.return-forbidden]
>
> `Push<T>` and `Pull<T>` MUST NOT appear in return types. They may
> only appear in argument position. The return type is always a plain
> value (possibly `()` for methods that only produce output via Pull).

For bidirectional communication, use one Push (input) and one Pull (output).

### Stream Messages

The following abstract messages relate to streams:

| Message | Sender | Meaning |
|---------|--------|---------|
| **Data** | stream sender | Deliver one value of type `T` |
| **Close** | caller (for Push) | End of stream (no more Data from caller) |
| **Reset** | either peer | Abort the stream immediately |
| **Credit** | receiver | Grant permission to send more bytes |

For `Push<T>` (caller→callee), the caller sends Close when done sending.
After sending Close, the caller MUST NOT send more Data on that stream.
See `r[streaming.close]` for details.

For `Pull<T>` (callee→caller), the stream is implicitly closed when the
callee sends the Response. No explicit Close message is sent.
See `r[streaming.lifecycle.response-closes-pulls]`.

Reset forcefully terminates a stream. After sending or receiving Reset,
both peers MUST discard any pending data and consider it dead.
Any outstanding credit is lost. See `r[streaming.reset]` for details.

### Stream ID Allocation

Stream IDs must be unique within a connection (`r[streaming.id.uniqueness]`).
ID 0 is reserved (`r[streaming.id.zero-reserved]`). The **caller** allocates
all stream IDs for a call (`r[streaming.allocation.caller]`).

For peer-to-peer transports, the **initiator** (who opened the connection)
uses odd IDs (1, 3, 5, ...) and the **acceptor** uses even IDs (2, 4, 6, ...).
See `r[streaming.id.parity]` for details.

Note: "Initiator" and "acceptor" refer to who opened the connection, not
who is calling whom. If the initiator calls, they use odd IDs. If the
acceptor calls back, they use even IDs.

### Streams and Calls

Streams are established via method calls. `Push<T>` streams may outlive
the Response — the caller continues sending until they send Close.
`Pull<T>` streams are implicitly closed when Response is sent.
See `r[streaming.call-complete]` and `r[streaming.streams-outlive-response]`.

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

Examples: duplicate request ID, data after Close, unknown stream ID.

> r[core.error.goodbye-reason]
>
> The Goodbye reason MUST contain the rule ID that was violated
> (e.g., `core.stream.close`), optionally followed by context.

## Flow Control

Streams use credit-based flow control (`r[flow.stream.credit-based]`). A sender
MUST NOT send data exceeding the receiver's granted credit. Credit is measured
in bytes (`r[flow.stream.byte-accounting]`). Initial credit is established at
connection setup (`r[flow.stream.initial-credit]`).

The receiver grants additional credit via Credit messages
(`r[flow.stream.credit-grant]`). If a sender exceeds granted credit, this is
a connection error (`r[flow.stream.credit-overrun]`).

See the [Flow Control](#flow-control-1) section for complete details.

## Metadata

> r[core.metadata]
>
> Requests and Responses carry metadata: a list of key-value pairs
> for out-of-band information (tracing, auth, deadlines, etc.).

Unknown metadata keys MUST be ignored (`r[unary.metadata.unknown]`).
See the [Metadata](#metadata-1) section for complete details.

## Topologies

Transports may have different topologies:

- **Peer-to-peer** (TCP, WebSocket, QUIC): Two peers, either can call.
- **Hub** (SHM Hub): One host, multiple peers. Routing is required.

The shared memory transport [SHM-SPEC] specifies its topology separately.

---

# Transport Bindings

The following sections define how Core Semantics are encoded for specific
transport categories. Each binding specifies message encoding, framing,
connection establishment, and stream ID allocation.

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
[RUST-SPEC]. Other language
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
Other in-flight calls and streams continue normally. Examples:
  * `UnknownMethod` — no handler for this method ID
  * `InvalidPayload` — couldn't deserialize the arguments
  * `Cancelled` — caller cancelled the request

**Stream errors** affect a single stream. The stream is closed but other
streams and calls are unaffected. A peer signals stream errors by sending
Reset.

**Connection errors** are protocol violations. The peer sends a Goodbye
message (citing the violated rule) and closes the connection. Everything
on this connection is torn down. Examples:
  * Data/Close/Reset on an unknown stream ID
  * Data after Close
  * Duplicate in-flight request ID

# Unary RPC

A unary RPC is the simplest form of method call: one request, one response.
This section specifies the complete lifecycle.

## Request IDs

> r[unary.request-id.uniqueness]
>
> A request ID (u64) MUST be unique within a connection. Implementations
> SHOULD use a monotonically increasing counter starting at 1.

> r[unary.request-id.duplicate-detection]
>
> If a peer receives a Request with a `request_id` that matches an
> existing in-flight request, it MUST send a Goodbye message (reason:
> `unary.request-id.duplicate-detection`) and close the connection.

> r[unary.request-id.in-flight]
>
> A request is "in-flight" from when the Request message is sent until
> the corresponding Response message is received.

> r[unary.request-id.cancel-still-in-flight]
>
> Sending a Cancel message does NOT remove a request from in-flight status.
> The request remains in-flight until a Response is received (which may be
> a `Cancelled` error, a completed result, or any other response).

For streaming methods, the Request/Response exchange negotiates streams,
but those streams have their own lifecycle independent of the call. See
[Streaming RPC](#streaming-rpc) for details.

## Initiating a Call

> r[unary.initiate]
>
> A call is initiated by sending a Request message.

A Request contains:

```rust
Request {
    request_id: u64,
    method_id: u64,
    metadata: Vec<(String, MetadataValue)>,
    payload: Vec<u8>,  // [POSTCARD]-encoded arguments
}
```

> r[unary.request.payload-encoding]
>
> The payload MUST be the [POSTCARD] encoding of a tuple containing all
> method arguments in declaration order.

For example, a method `fn add(a: i32, b: i32) -> i64` with arguments `(3, 5)`
would have a payload that is the [POSTCARD] encoding of the tuple `(3i32, 5i32)`.

## Completing a Call

> r[unary.complete]
>
> A call is completed by sending a Response message with the same
> `request_id` as the original Request.

A Response contains:

```rust
Response {
    request_id: u64,
    metadata: Vec<(String, MetadataValue)>,
    payload: Vec<u8>,  // [POSTCARD]-encoded Result<T, RoamError<E>>
}
```

Where `T` is the method's success type and `E` is the method's error type
(if the method returns `Result<T, E>`).

## Response Encoding

> r[unary.response.encoding]
>
> The response payload MUST be the [POSTCARD] encoding of `Result<T, RoamError<E>>`,
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

> r[unary.metadata.type]
>
> Metadata is a list of key-value pairs: `Vec<(String, MetadataValue)>`.

```rust
enum MetadataValue {
    String(String),  // 0
    Bytes(Vec<u8>),  // 1
    U64(u64),        // 2
}
```

> r[unary.metadata.keys]
>
> Metadata keys are case-sensitive strings. Keys MUST be at most 256
> bytes (UTF-8 encoded).

> r[unary.metadata.duplicates]
>
> Duplicate keys are allowed. If multiple entries have the same key,
> all values are preserved in order. Consumers MAY use any of the values
> (typically the first or last).

> r[unary.metadata.order]
>
> Metadata order MUST be preserved during transmission. Order is not
> semantically meaningful for most uses, but some applications may
> rely on it (e.g., multi-value headers).

> r[unary.metadata.unknown]
>
> Unknown metadata keys MUST be ignored.

> r[unary.metadata.limits]
>
> Metadata limits:
> - At most 128 metadata entries (key-value pairs)
> - Each key at most 256 bytes
> - Each value at most 16 KB (16,384 bytes)
> - Total metadata size at most 64 KB (65,536 bytes)
>
> If a peer receives a message exceeding these limits, it MUST send a
> Goodbye message (reason: `unary.metadata.limits`) and close the
> connection.

### Example Uses

Metadata is application-defined. Common uses include:

- **Deadlines**: Absolute timestamp after which the caller no longer cares
- **Distributed tracing**: W3C traceparent/tracestate, or other trace IDs
- **Authentication**: Bearer tokens, API keys, signatures
- **Priority**: Scheduling hints for request processing order
- **Compression**: Indicating payload compression scheme

## RoamError

> r[unary.error.roam-error]
>
> `RoamError<E>` distinguishes application errors from protocol errors.
> The variant order defines wire discriminants ([POSTCARD] varint encoding):

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

> r[unary.error.user]
>
> The `User(E)` variant (discriminant 0) carries the application's error
> type. This is semantically different from protocol errors — the method
> ran and returned `Err(e)`.

> r[unary.error.protocol]
>
> Discriminants 1-3 are protocol-level errors. The method may not have
> run at all (UnknownMethod, InvalidPayload) or was interrupted
> (Cancelled).

This design means callers always know: "Did my application logic fail,
or did the RPC infrastructure fail?"

### Returning Call Errors

> r[unary.error.unknown-method]
>
> If a callee receives a Request with a `method_id` it does not recognize,
> it MUST send a Response with `Err(RoamError::UnknownMethod)`. The
> connection remains open.

> r[unary.error.invalid-payload]
>
> If a callee cannot deserialize the Request payload, it MUST send a
> Response with `Err(RoamError::InvalidPayload)`. The connection
> remains open.

## Call Lifecycle

The complete lifecycle of a unary RPC:

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

> r[unary.lifecycle.single-response]
>
> For each Request, the callee MUST send exactly one Response with the
> same `request_id`. No more, no less.

> r[unary.lifecycle.ordering]
>
> Responses MAY arrive in any order. The caller MUST use `request_id`
> for correlation, not arrival order.

> r[unary.lifecycle.unknown-request-id]
>
> If a caller receives a Response with a `request_id` that does not match
> any in-flight request, it MUST ignore the response. Implementations
> SHOULD log this as a warning.

## Cancellation

```rust
Cancel {
    request_id: u64,  // The request to cancel
}
```

> r[unary.cancel.message]
>
> A caller MAY send a Cancel message to request that the callee stop
> processing a request. The Cancel message MUST include the `request_id`
> of the request to cancel.

> r[unary.cancel.best-effort]
>
> Cancellation is best-effort. The callee MAY have already completed the
> request, or MAY be unable to cancel in-progress work. The callee MUST
> still send a Response (either the completed result or `Cancelled` error).

> r[unary.cancel.no-response-required]
>
> The caller MUST NOT wait indefinitely for a response after sending Cancel.
> Implementations SHOULD use a timeout after which the caller considers the
> request cancelled locally, even without a response.

## Pipelining

> r[unary.pipelining.allowed]
>
> Multiple requests MAY be in flight simultaneously. The caller does not
> need to wait for a response before sending the next request.

> r[unary.pipelining.independence]
>
> Each request is independent. A slow or failed request MUST NOT block
> other requests.

This enables efficient batching — a caller can send 10 requests, then
await all 10 responses, rather than round-tripping each one sequentially.

# Streaming RPC

Streaming methods have `Push<T>` (caller→callee) or `Pull<T>` (callee→caller)
in argument position. Unlike unary RPC, data flows continuously over dedicated
streams.

## Push and Pull Types

> r[streaming.type]
>
> `Push<T>` and `Pull<T>` are roam-provided types recognized by the
> `#[roam::service]` macro. On the wire, both serialize as a `u64` stream ID.

> r[streaming.caller-pov]
>
> Service definitions are written from the **caller's perspective**.
> `Push<T>` means "caller pushes data to callee". `Pull<T>` means
> "caller pulls data from callee".

> r[streaming.holder-semantics]
>
> From the holder's perspective: `Push<T>` means "I send on this",
> `Pull<T>` means "I receive from this". Generated callee handlers
> have the types flipped relative to the service definition.

Example:

```rust
// Service definition (caller's perspective)
#[roam::service]
pub trait Streaming {
    async fn sum(&self, numbers: Push<u32>) -> u32;       // caller→callee
    async fn range(&self, n: u32, output: Pull<u32>);     // callee→caller
}

// Generated caller stub — same types as definition
impl StreamingClient {
    async fn sum(&self, numbers: Push<u32>) -> u32;       // caller sends
    async fn range(&self, n: u32, output: Pull<u32>);     // caller receives
}

// Generated callee handler — types flipped
trait StreamingHandler {
    async fn sum(&self, numbers: Pull<u32>) -> u32;       // callee receives
    async fn range(&self, n: u32, output: Push<u32>);     // callee sends
}
```

The number of streams in a call is not always obvious from the method
signature — they may appear inside enums, so the actual IDs present depend
on which variant is passed.

## Stream ID Allocation

> r[streaming.allocation.caller]
>
> The **caller** allocates ALL stream IDs (both Push and Pull). All are
> serialized in the Request payload. The callee does not allocate any IDs.

> r[streaming.id.uniqueness]
>
> A stream ID MUST be unique within a connection.

> r[streaming.id.zero-reserved]
>
> Stream ID 0 is reserved. If a peer receives a stream message with
> `stream_id` of 0, it MUST send a Goodbye message (reason:
> `streaming.id.zero-reserved`) and close the connection.

> r[streaming.id.parity]
>
> For peer-to-peer transports, the **initiator** (who opened the connection)
> MUST allocate odd stream IDs (1, 3, 5, ...). The **acceptor** MUST allocate
> even stream IDs (2, 4, 6, ...). This prevents collisions when both peers
> make concurrent calls.

Note: "Initiator" and "acceptor" refer to who opened the connection, not
who is calling whom. If the initiator calls with a Push and Pull, both
use odd IDs (e.g., push=1, pull=3). If the acceptor calls, both use even
IDs (e.g., push=2, pull=4).

## Call Lifecycle with Streams

### Caller Streaming (Push): `sum(numbers: Push<u32>) -> u32`

```
Caller (initiator)                         Callee (acceptor)
    |                                          |
    |-- Request(sum, push=1) ----------------->|
    |-- Data(stream=1, 10) ------------------->|
    |-- Data(stream=1, 20) ------------------->|
    |-- Close(stream=1) ---------------------->|
    |                                          |
    |<-- Response(Ok, 30) --------------------|
```

### Callee Streaming (Pull): `range(n, output: Pull<u32>)`

```
Caller (initiator)                         Callee (acceptor)
    |                                          |
    |-- Request(range, n=3, pull=1) ---------->|
    |                                          |
    |<-- Data(stream=1, 0) --------------------|
    |<-- Data(stream=1, 1) --------------------|
    |<-- Data(stream=1, 2) --------------------|
    |<-- Response(Ok, ()) --------------------|  // pull stream implicitly closed
```

### Bidirectional: `pipe(input: Push, output: Pull)`

```
Caller (initiator)                         Callee (acceptor)
    |                                          |
    |-- Request(pipe, push=1, pull=3) -------->|
    |-- Data(stream=1, "a") ------------------>|
    |<-- Data(stream=3, "a") ------------------|
    |-- Data(stream=1, "b") ------------------>|
    |<-- Data(stream=3, "b") ------------------|
    |-- Close(stream=1) ---------------------->|
    |<-- Response(Ok, ()) --------------------|  // pull=3 closed
```

> r[streaming.lifecycle.immediate-data]
>
> The caller MAY send Data on `Push<T>` streams immediately after sending
> the Request, without waiting for Response. This enables pipelining for
> lower latency.

> r[streaming.lifecycle.speculative]
>
> If the caller sends Data before receiving Response, and the Response
> is an error (`Err(RoamError::UnknownMethod)`, `Err(RoamError::InvalidPayload)`,
> etc.), the Data was wasted. The stream IDs are "burned" — they were
> never successfully opened and MUST NOT be reused.

> r[streaming.lifecycle.response-closes-pulls]
>
> When the callee sends Response, all `Pull<T>` streams are implicitly
> closed. The callee MUST NOT send Data on any Pull stream after sending Response.

> r[streaming.lifecycle.caller-closes-pushes]
>
> The caller MUST send Close on each `Push<T>` stream when done sending.
> The callee waits for Close before it knows all input has arrived.

> r[streaming.error-no-streams]
>
> `Push<T>` and `Pull<T>` MUST NOT appear inside error types. A method's
> error type `E` in `Result<T, E>` MUST NOT contain `Push<T>` or `Pull<T>`
> at any nesting level.

## Stream Data Flow

> r[streaming.data]
>
> The sending peer sends Data messages containing [POSTCARD]-encoded values
> of the stream's element type `T`. Each Data message contains exactly
> one value.

> r[streaming.data.size-limit]
>
> Each stream element MUST NOT exceed `max_payload_size` bytes (the same
> limit that applies to Request/Response payloads). If a peer receives
> a stream element exceeding this limit, it MUST send a Goodbye message
> (reason: `streaming.data.size-limit`) and close the connection.

> r[streaming.data.invalid]
>
> If a peer receives a Data message that cannot be deserialized as the
> stream's element type, it MUST send a Goodbye message (reason:
> `streaming.data.invalid`) and close the connection.

> r[streaming.close]
>
> For `Push<T>` (caller→callee), the caller sends Close when done.
> For `Pull<T>` (callee→caller), the stream closes implicitly with Response.

> r[streaming.data-after-close]
>
> If a peer receives a Data message on a stream after it has been
> closed, it MUST send a Goodbye message (reason: `streaming.data-after-close`)
> and close the connection.

## Resetting a Stream

> r[streaming.reset]
>
> Either peer MAY send Reset to forcefully terminate a stream.
> The sender uses Reset to abandon early; the receiver uses Reset to signal
> it no longer wants data.

> r[streaming.reset.effect]
>
> Upon receiving Reset, the peer MUST consider the stream terminated.
> Any further Data, Close, or Credit messages for that ID MUST be ignored
> (they may arrive due to race conditions).

> r[streaming.reset.credit]
>
> When a stream is reset, any outstanding credit is lost.

> r[streaming.unknown]
>
> If a peer receives a stream message (Data, Close, Reset, Credit) with a
> `stream_id` that was never opened, it MUST send a Goodbye message
> (reason: `streaming.unknown`) and close the connection.

## Streams and Call Completion

> r[streaming.call-complete]
>
> The RPC call completes when the Response is received. At that point:
> - All `Pull<T>` streams are closed (callee can no longer send)
> - `Push<T>` streams may still be open (caller may still be sending)
> - The request ID is no longer in-flight

> r[streaming.streams-outlive-response]
>
> `Push<T>` streams (caller→callee) may outlive the Response. The caller
> continues sending until they send Close. The callee processes the final
> return value only after all input streams are closed.

# Flow Control

Flow control prevents fast senders from overwhelming slow receivers.
roam uses credit-based flow control for streams on all transports.

## Stream Flow Control

> r[flow.stream.credit-based]
>
> Streams use credit-based flow control. A sender MUST NOT send
> a Data message if doing so would exceed the remaining credit for that
> stream — even if the underlying transport would accept the data.

> r[flow.stream.all-transports]
>
> Credit-based flow control applies to all transports for both `Push<T>`
> and `Pull<T>` streams. On multi-stream transports (QUIC, WebTransport),
> roam credit operates independently of any transport-level flow control.
> The transport may additionally block writes, but that is transparent
> to the roam layer.

### Byte Accounting

> r[flow.stream.byte-accounting]
>
> Credits are measured in bytes. The byte count for a stream element is
> the length of its [POSTCARD] encoding — the same bytes that appear in
> `Data.payload`, or on multi-stream transports, the bytes written to the
> dedicated transport stream before [COBS] framing. Framing overhead
> ([COBS], transport headers) is NOT counted.

### Initial Credit

> r[flow.stream.initial-credit]
>
> The initial stream credit MUST be negotiated during handshake. Each
> stream starts with this amount of credit independently.

Both peers advertise their `initial_stream_credit` in Hello. The effective
initial credit is the minimum of both values. Each stream ID gets its
own independent credit counter starting at this value.

### Granting Credit

```rust
Credit {
    stream_id: u64,
    bytes: u32,  // additional bytes granted
}
```

> r[flow.stream.credit-grant]
>
> A receiver grants additional credit by sending a Credit message. The
> `bytes` field is added to the sender's available credit for that stream.

> r[flow.stream.credit-additive]
>
> Credits are additive. If a receiver grants 1000 bytes, then grants 500
> more, the sender has 1500 bytes available.

> r[flow.stream.credit-prompt]
>
> Credit messages SHOULD be processed in receive order without intentional
> delay. Starving Credit processing can cause unnecessary stalls.

### Consuming Credit

> r[flow.stream.credit-consume]
>
> Sending a stream element consumes credits equal to its byte count (see
> `r[flow.stream.byte-accounting]`). The sender MUST track remaining
> credit and MUST NOT send if it would result in negative credit.

### Credit Overrun

> r[flow.stream.credit-overrun]
>
> If a receiver receives a stream element whose byte count exceeds the
> remaining credit for that stream, it MUST send a Goodbye message
> (reason: `flow.stream.credit-overrun`) and close the connection.

Credit overrun indicates a buggy or malicious peer.

### Zero Credit

> r[flow.stream.zero-credit]
>
> If a sender has zero remaining credit for a stream, it MUST wait for
> a Credit message before sending more data. This is not a protocol
> error — the receiver controls the pace.

If progress stops entirely, implementations should use application-level
timeouts. A sender may Reset the stream or close the connection if no
credit arrives within a reasonable time.

### Close and Credit

> r[flow.stream.close-exempt]
>
> Close messages (and Reset) do not consume credit. A sender MAY always
> send Close regardless of credit state. This ensures streams can always
> be closed.

### Infinite Credit Mode

> r[flow.stream.infinite-credit]
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

## Unary RPC Flow Control

> r[flow.unary.payload-limit]
>
> Unary RPC (Request/Response) payloads are bounded by `max_payload_size`
> negotiated during handshake. No credit-based flow control is used.

The natural pipelining limit (waiting for responses) provides implicit
flow control for unary calls.

# Messages

Everything roam does — method calls, streams, control signals — is
built on messages exchanged between peers.

```rust
enum Message {
    // Control
    Hello(Hello),
    Goodbye { reason: String },
    
    // RPC
    Request { request_id: u64, method_id: u64, metadata: Vec<(String, MetadataValue)>, payload: Vec<u8> },
    Response { request_id: u64, metadata: Vec<(String, MetadataValue)>, payload: Vec<u8> },
    Cancel { request_id: u64 },
    
    // Streams
    Data { stream_id: u64, payload: Vec<u8> },
    Close { stream_id: u64 },
    Reset { stream_id: u64 },
    Credit { stream_id: u64, bytes: u32 },
}
```

Messages are [POSTCARD]-encoded. The enum discriminant identifies the message
type, and each variant contains only the fields it needs.

> r[message.unknown-variant]
>
> If a peer receives a Message with an unknown enum discriminant, it
> MUST send a Goodbye message (reason: `message.unknown-variant`) and
> close the connection.

> r[message.decode-error]
>
> If a peer cannot decode a received message (invalid [POSTCARD] encoding,
> [COBS] framing error, or malformed fields), it MUST send a Goodbye
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
    V1 {
        max_payload_size: u32,
        initial_stream_credit: u32,
    },
}
```

| Field | Description |
|-------|-------------|
| `max_payload_size` | Maximum bytes in a Request/Response payload |
| `initial_stream_credit` | Bytes of credit each stream starts with |

> r[message.hello.negotiation]
>
> The effective limits for a connection are the minimum of both peers'
> advertised values.

> r[message.hello.enforcement]
>
> If a peer receives a Request or Response whose payload exceeds the
> negotiated `max_payload_size`, it MUST send a Goodbye message
> (reason: `message.hello.enforcement`) and close the connection.

### Goodbye

> r[message.goodbye.send]
>
> A peer MUST send a Goodbye message before closing the connection due to
> a protocol error. The `reason` field MUST contain the rule ID that was
> violated (e.g., `streaming.id.zero-reserved`), optionally followed by
> additional context.

> r[message.goodbye.receive]
>
> Upon receiving a Goodbye message, a peer MUST stop sending messages
> and close the connection. All in-flight requests fail with a
> connection error (not `RoamError` — the connection itself is gone).
> All open streams are terminated.

### Request / Response / Cancel

`Request` initiates an RPC call. `Response` returns the result. `Cancel`
requests that the callee stop processing a request.

The `request_id` correlates requests with responses, enabling multiple
calls to be in flight simultaneously (pipelining).

### Data / Close / Reset

`Data` carries payload bytes on a stream, identified by `stream_id`.
`Close` signals end-of-stream — the sender is done (see `r[core.stream.close]`).
`Reset` forcefully terminates a stream.


# Transports

Different transports require different handling:

| Kind | Example | Framing | Streams |
|------|---------|---------|---------|
| Message | WebSocket | Transport provides | All in one |
| Multi-stream | QUIC | Per stream | Can map to transport streams |
| Byte stream | TCP | [COBS] | All in one |

## Message Transports

Message transports (like WebSocket) deliver discrete messages.

> r[transport.message.one-to-one]
>
> Each transport message MUST contain exactly one roam message,
> [POSTCARD]-encoded. Fragmentation and reassembly are not supported.

> r[transport.message.binary]
>
> Transport messages MUST be binary (not text). For WebSocket, this
> means binary frames, not text frames.

> r[transport.message.multiplexing]
>
> All messages (control, RPC, stream data) flow through the same
> transport connection. The `stream_id` field provides multiplexing.

## Multi-stream Transports

Multi-stream transports (like QUIC, WebTransport) provide multiple independent
streams, which can eliminate head-of-line blocking.

See the [Multi-stream Transport Specification](/multistream-spec/) for the
complete binding specification. This is tracked separately as it is not yet
implemented.

## Byte Stream Transports

Byte stream transports (like TCP) provide a single ordered byte stream.

> r[transport.bytestream.cobs]
>
> Messages MUST be framed using [COBS]. Each message MUST be followed by
> a 0x00 delimiter byte.
> 
> ```
> [COBS-encoded message][0x00][COBS-encoded message][0x00]...
> ```

All messages flow through the single byte stream. The `stream_id` field
in stream messages provides multiplexing.

# Wire Examples (Non-normative)

These examples illustrate protocol behavior on byte-stream transports.

## Hello Negotiation and Unary Call

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

## Caller Streaming (Push) with Credit

```aasvg
.--------.                                              .--------.
| Caller |                                              | Callee |
'---+----'                                              '---+----'
    |                                                       |
    |-------- Request { id=3, stream_id=1 } --------------->|
    |                                                       |
    |         .---------------------------------.            |
    |         | stream 1 open; credit=8KB      |            |
    |         '---------------------------------'            |
    |                                                       |
    |-------- Data { stream=1, 4KB } ---------------------->| credit: 8K->4K
    |-------- Data { stream=1, 4KB } ---------------------->| credit: 4K->0K
    |                                                       |
    |         .---------------------------------.            |
    |         | sender blocks, no credit       |            |
    |         '---------------------------------'            |
    |                                                       |
    |<------- Credit { stream=1, bytes=8KB } ---------------|
    |                                                       |
    |-------- Data { stream=1, 2KB } ---------------------->|
    |-------- Close { stream=1 } -------------------------->|
    |                                                       |
    |<------- Response { id=3, Ok(result) } ----------------|
    |                                                       |
```

## Callee Streaming (Pull)

```aasvg
.--------.                                              .--------.
| Caller |                                              | Callee |
'---+----'                                              '---+----'
    |                                                       |
    |-------- Request { id=4, stream_id=1 } --------------->|
    |                                                       |
    |<------- Data { stream=1, value } ---------------------|
    |<------- Data { stream=1, value } ---------------------|
    |<------- Data { stream=1, value } ---------------------|
    |<------- Response { id=4, Ok(()) } --------------------|
    |                                                       |
    |         .---------------------------------.            |
    |         | stream 1 implicitly closed     |            |
    |         '---------------------------------'            |
    |                                                       |
```

## Bidirectional (Push + Pull)

```aasvg
.--------.                                              .--------.
| Caller |                                              | Callee |
'---+----'                                              '---+----'
    |                                                       |
    |-- Request { id=5, stream_ids=[1,3] } ---------------->|
    |-- Data { stream=1, "hello" } ------------------------>|
    |<- Data { stream=3, "hello" } -------------------------|
    |-- Data { stream=1, "world" } ------------------------>|
    |<- Data { stream=3, "world" } -------------------------|
    |-- Close { stream=1 } -------------------------------->|
    |<- Response { id=5, Ok(()) } --------------------------|
    |                                                       |
    |         .---------------------------------.            |
    |         | stream 3 closed with Response  |            |
    |         '---------------------------------'            |
    |                                                       |
```

## Reset Handling

```aasvg
.--------.                                              .----------.
| Sender |                                              | Receiver |
'---+----'                                              '----+-----'
    |                                                        |
    |-------- Data { stream=5, chunk } --------------------->|
    |                                                        |
    |<------- Reset { stream=5 } ----------------------------|
    |                                                        |
    |         .---------------------------------.             |
    |         | sender stops; in-flight msgs   |             |
    |         | for stream 5 are ignored       |             |
    |         '---------------------------------'             |
    |                                                        |
```

## Connection Error (Goodbye)

```aasvg
.------.                                                .------.
| Peer |                                                | Peer |
'--+---'                                                '--+---'
   |                                                       |
   |-------- Data { stream=99, ... } --------------------->|
   |                                                       |
   |          .---------------------------------.           |
   |          | stream 99 was never opened!    |           |
   |          '---------------------------------'           |
   |                                                       |
   |<------- Goodbye { reason="streaming.unknown" } -------|
   |                                                       |
   X                [connection closed]                    X
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

Some errors (like data on an unknown stream) are connection errors
rather than stream errors because:

- They indicate a fundamental protocol mismatch or bug
- Recovery is unlikely to succeed
- Continuing could cause cascading confusion

Stream-scoped errors (Reset) are for application-level issues where
the connection can continue serving other streams.

# References

- **[POSTCARD]** Postcard Wire Format Specification  
  <https://postcard.jamesmunns.com/wire-format>

- **[RUST-SPEC]** roam Rust Implementation Specification  
  <@/rust-spec/_index.md>

- **[SHM-SPEC]** roam Shared Memory Transport Specification  
  <@/shm-spec/_index.md>

- **[COBS]** Consistent Overhead Byte Stuffing  
  <https://en.wikipedia.org/wiki/Consistent_Overhead_Byte_Stuffing>
