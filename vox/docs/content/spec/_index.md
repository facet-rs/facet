+++
title = "Rapace specification"
description = "Formal Rapace RPC protocol specification"
+++

# Introduction

This is Rapace specification v1.0.0, last updated January 5, 2026. It canonically
lives at <https://github.com/bearcove/rapace> — where you can get the latest version.

Rapace is a **Rust-native** RPC protocol. We don't claim to be language-neutral —
Rust is the lowest common denominator. There is no independent schema language;
Rust traits *are* the schema. Clients and servers for other languages (Swift,
TypeScript, etc.) are generated from Rust definitions.

This means:
- The Rust Implementation Specification [RUST-SPEC] is essential
- Other implementations use Rust tooling for code generation
- Fully independent implementations are a non-goal

Services are defined inside of Rust "proto" crates, annotating traits with
the `#[rapace::service]` proc macro attribute:

```rust
#[rapace::service]
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

This section defines transport-agnostic semantics that all Rapace
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
> Request IDs MUST be unique within a connection. Implementations MUST
> use a monotonically increasing counter.

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
> complete. The caller MUST still wait for a Response (which may be a
> `Cancelled` error or the actual result).

## Streams

A **stream** is a unidirectional, ordered sequence of typed values.

> r[core.stream]
>
> A stream has exactly one sender and one receiver. Values flow from
> sender to receiver. The sender controls the stream lifecycle.

> r[core.stream.type]
>
> `Stream<T>` in a method signature represents a stream of values of
> type `T`. On the wire, a `Stream<T>` is represented by a `stream_id`
> (u64).

For bidirectional communication, use two streams (one in each direction).

### Stream Messages

The following abstract messages relate to streams:

| Message | Sender | Meaning |
|---------|--------|---------|
| **Data** | stream sender | Deliver one value of type `T` |
| **Close** | stream sender | End of stream (no more Data) |
| **Reset** | either peer | Abort the stream immediately |
| **Credit** | stream receiver | Grant permission to send more bytes |

> r[core.stream.close]
>
> After sending Close, the sender MUST NOT send more Data on that stream.
> Close is a normal termination signaling that all values have been sent.

> r[core.stream.reset]
>
> Reset forcefully terminates a stream. After sending or receiving Reset,
> both peers MUST discard any pending data and consider the stream dead.
> Any outstanding credit is lost.

### Stream ID Allocation

> r[core.stream.id.unique]
>
> Stream IDs MUST be unique within a connection.

> r[core.stream.id.zero-reserved]
>
> Stream ID 0 is reserved. Using it is a connection error.

> r[core.stream.id.disjoint]
>
> Each peer MUST allocate stream IDs from a disjoint space to prevent
> collisions. Transport bindings specify the allocation scheme (e.g.,
> odd/even parity, ranges).

### Streams and Calls

Streams are established via method calls. The caller allocates stream IDs
for `Stream<T>` arguments; the callee allocates stream IDs for `Stream<T>`
in the return type.

> r[core.stream.call-lifecycle]
>
> Streams established by a call are independent of the call lifecycle.
> Once the Response is received, the call is complete, but streams may
> continue until they are Closed or Reset.

## Errors

### Call Errors

> r[core.error.rapace-error]
>
> Call results are wrapped in `RapaceError<E>` which distinguishes
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

> r[core.flow.credit-based]
>
> Streams use credit-based flow control. A sender MUST NOT send data
> exceeding the receiver's granted credit.

> r[core.flow.byte-accounting]
>
> Credit is measured in bytes — the serialized size of stream values.
> Transport bindings specify exactly what bytes are counted (typically
> the payload encoding, not framing overhead).

> r[core.flow.initial-credit]
>
> Initial credit is established at connection setup. All streams start
> with this amount.

> r[core.flow.credit-grant]
>
> The receiver grants additional credit. On message-based transports,
> this is a Credit message. On shared-memory transports, this may be
> a shared counter update. The mechanism is binding-specific, but the
> semantic effect is the same: credits are additive.

> r[core.flow.overrun]
>
> If a sender exceeds granted credit, this is a connection error.

## Metadata

> r[core.metadata]
>
> Requests and Responses carry metadata: a list of key-value pairs
> for out-of-band information (tracing, auth, deadlines, etc.).

> r[core.metadata.unknown]
>
> Unknown metadata keys MUST be ignored.

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

#[rapace::service]
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

Note: Argument *names* are not part of the wire format and can be changed
freely. Only types and their order matter.

Some type substitutions are compatible because they have the same wire format:
- `Vec<T>` ↔ `VecDeque<T>` ↔ `HashSet<T>` ↔ `BTreeSet<T>` (all are sequences)
- `HashMap<K, V>` ↔ `BTreeMap<K, V>` ↔ `Vec<(K, V)>` (all are maps)

## Error Scoping

Errors in Rapace have different scopes, from narrowest to widest:

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
> MUST use a monotonically increasing counter starting at 1.

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
    payload: Vec<u8>,  // [POSTCARD]-encoded Result<T, RapaceError<E>>
}
```

Where `T` is the method's success type and `E` is the method's error type
(if the method returns `Result<T, E>`).

## Response Encoding

> r[unary.response.encoding]
>
> The response payload MUST be the [POSTCARD] encoding of `Result<T, RapaceError<E>>`,
> where `T` and `E` come from the method signature.

For a method declared as:

```rust
async fn get_user(&self, id: UserId) -> Result<User, UserError>;
```

The response payload is `Result<User, RapaceError<UserError>>`.

For a method that cannot fail at the application level:

```rust
async fn ping(&self) -> Pong;
```

The response payload is `Result<Pong, RapaceError<Infallible>>` (or an
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
> Metadata keys are case-sensitive strings.

> r[unary.metadata.unknown]
>
> Unknown metadata keys MUST be ignored.

> r[unary.metadata.limits]
>
> A Request or Response MUST contain at most 128 metadata keys. Each
> metadata value MUST be at most 16 KB (16,384 bytes). If a peer
> receives a message exceeding these limits, it MUST send a Goodbye
> message (reason: `unary.metadata.limits`) and close the connection.

### Example Uses

Metadata is application-defined. Common uses include:

- **Deadlines**: Absolute timestamp after which the caller no longer cares
- **Distributed tracing**: W3C traceparent/tracestate, or other trace IDs
- **Authentication**: Bearer tokens, API keys, signatures
- **Priority**: Scheduling hints for request processing order
- **Compression**: Indicating payload compression scheme

## RapaceError

> r[unary.error.rapace-error]
>
> `RapaceError<E>` distinguishes application errors from protocol errors.
> The variant order defines wire discriminants ([POSTCARD] varint encoding):

| Discriminant | Variant | Payload | Meaning |
|--------------|---------|---------|---------|
| 0 | `User` | `E` | Application returned an error |
| 1 | `UnknownMethod` | none | No handler for this `method_id` |
| 2 | `InvalidPayload` | none | Could not deserialize request arguments |
| 3 | `Cancelled` | none | Caller cancelled the request |

In Rust syntax (for clarity):

```rust
enum RapaceError<E> {
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
> it MUST send a Response with `Err(RapaceError::UnknownMethod)`. The
> connection remains open.

> r[unary.error.invalid-payload]
>
> If a callee cannot deserialize the Request payload, it MUST send a
> Response with `Err(RapaceError::InvalidPayload)`. The connection
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

Streaming methods have `Stream<T>` in argument or return position. Unlike
unary RPC, data flows continuously over dedicated streams.

## Stream Type

> r[streaming.type]
>
> `Stream<T>` is a Rapace-provided type recognized by the `#[rapace::service]`
> macro. On the wire, a `Stream<T>` serializes as a `u64` stream ID.

The number of streams in a call is not always obvious from the method
signature — streams may appear inside enums, so the actual stream IDs
present depend on which variant is passed or returned.

## Stream ID Allocation

> r[streaming.allocation.caller]
>
> The caller allocates stream IDs for streams that appear in **argument**
> position. These IDs are serialized in the Request payload.

> r[streaming.allocation.callee]
>
> The callee allocates stream IDs for streams that appear in **return**
> position. These IDs are serialized in the Response payload.

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
> For peer-to-peer transports, the initiator (who opened the connection)
> MUST allocate odd stream IDs (1, 3, 5, ...). The acceptor MUST allocate
> even stream IDs (2, 4, 6, ...). This prevents collisions without
> coordination.

Note: "Initiator" and "acceptor" refer to who opened the connection, not
who is calling whom. Other transports (e.g., shared memory) may use
different allocation schemes as specified in their transport binding.

## Call Lifecycle with Streams

```aasvg
.---------.                                                    .---------.
| Caller  |                                                    | Callee  |
'----+----'                                                    '----+----'
     |                                                              |
     |                                                              |
     |-------- Request(method, payload with stream_id=3) ---------->|
     |                                                              |
     |                                                              |
     |                            [accept call, allocate stream_id=4]
     |                                                              |
     |                                                              |
     |<------- Response(Ok, payload with stream_id=4) --------------|
     |                                                              |
     |                                                              |
     +=================== streams are now open =====================+
     |                                                              |
     |                                                              |
     |-------- Data(stream_id=3, chunk) --------------------------->|
     |                                                              |
     |-------- Data(stream_id=3, chunk) --------------------------->|
     |                                                              |
     |<------- Data(stream_id=4, result) ---------------------------|
     |                                                              |
     |-------- Close(stream_id=3) ---------------------------------->|
     |                                                              |
     |<------- Data(stream_id=4, result) ---------------------------|
     |                                                              |
     |<------- Close(stream_id=4) -----------------------------------|
     |                                                              |
     |                                                              |
```

> r[streaming.lifecycle.request]
>
> The caller sends a Request with stream IDs for argument streams
> embedded in the payload. The caller MUST NOT send Data on these
> streams until the Response arrives.

> r[streaming.lifecycle.response-success]
>
> If the callee accepts the call, the Response contains stream IDs for
> return streams. Upon receiving a successful Response, all streams
> (argument and return) are considered open.

> r[streaming.lifecycle.response-error]
>
> If the callee rejects the call (Response contains `Err(RapaceError::UnknownMethod)`,
> `Err(RapaceError::InvalidPayload)`, or `Err(RapaceError::Cancelled)`), no streams
> are opened. The stream IDs in the Request payload are "burned" — they were never
> opened and MUST NOT be reused.

> r[streaming.error-no-streams]
>
> `Stream<T>` MUST NOT appear inside error types. A method's error type `E` in
> `Result<T, E>` MUST NOT contain `Stream<T>` at any nesting level. This ensures
> that `Err(RapaceError::User(e))` never carries stream IDs.

## Stream Data Flow

> r[streaming.data]
>
> Once a stream is open, the sending peer MAY send Data messages.
> Each Data message contains exactly one [POSTCARD]-encoded value of
> the stream's element type `T`.

> r[streaming.data.invalid]
>
> If a peer receives a Data message that cannot be deserialized as the
> stream's element type, it MUST send a Goodbye message (reason:
> `streaming.data.invalid`) and close the connection.

> r[streaming.close]
>
> When the sender has no more data, it MUST send a Close message.
> After Close, the stream is ended — no more Data will arrive.

> r[streaming.data-after-close]
>
> If a peer receives a Data message on a stream after having received
> Close on that stream, it MUST send a Goodbye message (reason:
> `streaming.data-after-close`) and close the connection.

Note: Streams are unidirectional (see `r[core.stream]`). There is no
"half-close" — Close ends the one direction of data flow. For bidirectional
communication, use two streams.

## Resetting a Stream

> r[streaming.reset]
>
> Either peer MAY send Reset to forcefully terminate a stream. The sender
> uses Reset to abandon a stream early; the receiver uses Reset to signal
> it no longer wants data.

> r[streaming.reset.effect]
>
> Upon receiving Reset, the peer MUST consider the stream terminated.
> Any further Data, Close, or Credit messages for that stream MUST be
> ignored (they may arrive due to race conditions).

> r[streaming.reset.credit]
>
> When a stream is reset, any outstanding credit for that stream is lost.

> r[streaming.unknown]
>
> If a peer receives a stream message (Data, Close, Reset, Credit) with a
> `stream_id` that was never opened, it MUST send a Goodbye message
> (reason: `streaming.unknown`) and close the connection.

## Streams and Call Completion

> r[streaming.call-complete]
>
> The RPC call itself completes when the Response is received. Streams
> have their own lifecycle independent of the call.

This means:
- The request ID is no longer in-flight once the Response arrives
- Streams may remain open indefinitely after the call completes
- Cancelling the call (before Response) does not affect already-opened streams

# Flow Control

Flow control prevents fast senders from overwhelming slow receivers.
Rapace uses credit-based flow control for streams on all transports.

## Stream Flow Control

> r[flow.stream.credit-based]
>
> Streams use credit-based flow control. A sender MUST NOT send a
> stream element if doing so would exceed the remaining credit for
> that stream — even if the underlying transport would accept the data.

> r[flow.stream.all-transports]
>
> Credit-based flow control applies to all transports. On multi-stream
> transports (QUIC, WebTransport), Rapace credit operates independently
> of any transport-level flow control. The transport may additionally
> block writes, but that is transparent to the Rapace layer.

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
> The initial stream credit MUST be negotiated during handshake. All
> streams start with this amount of credit in each direction.

Both peers advertise their `initial_stream_credit` in Hello. The effective
initial credit is the minimum of both values.

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

Everything Rapace does — method calls, streams, control signals — is
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
> connection error (not `RapaceError` — the connection itself is gone).
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

Message transports (like WebSocket) deliver discrete messages. Each transport
message contains exactly one Rapace message, [POSTCARD]-encoded.

No additional framing is needed. All messages (control, RPC, stream data)
flow through the same transport connection.

## Multi-stream Transports

Multi-stream transports (like QUIC, WebTransport) provide multiple independent
streams, which can eliminate head-of-line blocking.

> r[transport.multistream.control]
>
> Implementations MUST use transport stream 0 for control messages
> (Hello, Goodbye, Request, Response, Cancel, Credit). These are
> [COBS]-framed [POSTCARD]-encoded Message values.

> r[transport.multistream.streams]
>
> Implementations MUST map each Rapace stream to a dedicated unidirectional
> transport stream. Rapace streams are unidirectional (see `r[core.stream]`).

> r[transport.multistream.stream-id-mapping]
>
> The stream allocator (caller for argument streams, callee for return
> streams) opens a transport stream and communicates the mapping to
> the other peer. The `stream_id` in Request/Response payloads serves
> as the identifier; implementations maintain a local mapping from
> Rapace `stream_id` to transport stream handle.

Note: Transport stream IDs (e.g., QUIC stream IDs) are transport-specific
and may not be directly usable as Rapace stream IDs. The Rapace `stream_id`
is allocated according to the binding's scheme (e.g., `r[streaming.id.parity]`
for peer-to-peer); the transport stream is an implementation detail.

> r[transport.multistream.stream-data]
>
> On dedicated transport streams, data is sent as [COBS]-framed [POSTCARD]-
> encoded values of the stream's element type `T`. No Message wrapper or
> `stream_id` field is needed — the transport stream identity is implicit.

> r[transport.multistream.stream-close]
>
> Closing a Rapace stream is signaled by closing the transport stream
> (e.g., QUIC FIN). The Close message is not used on multi-stream transports.

> r[transport.multistream.stream-reset]
>
> Resetting a Rapace stream is signaled by resetting the transport stream
> (e.g., QUIC RESET_STREAM). The Reset message is not used on multi-stream
> transports.

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

# Introspection

Peers MAY implement introspection services to help debug method mismatches
and explore available services. See the
[rapace-discovery](https://crates.io/crates/rapace-discovery) crate for
the standard introspection service definition and types.

# References

- **[POSTCARD]** Postcard Wire Format Specification  
  <https://postcard.jamesmunns.com/wire-format>

- **[RUST-SPEC]** Rapace Rust Implementation Specification  
  <@/rust-spec/_index.md>

- **[SHM-SPEC]** Rapace Shared Memory Transport Specification  
  <@/shm-spec/_index.md>

- **[COBS]** Consistent Overhead Byte Stuffing  
  <https://en.wikipedia.org/wiki/Consistent_Overhead_Byte_Stuffing>
