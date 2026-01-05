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
- The [Rust Implementation Specification](@/rust-spec/_index.md) is essential
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
with [facet-postcard](https://crates.io/crates/facet-postcard).

Bindings for other languages (Swift, TypeScript) are generated using
a Rust codegen package which is linked together with the "proto" crate to
output Swift/TypeScript packages.

This specification exists to ensure that various implementations are compatible, and
to ensure that those implementations are specified — that their code corresponds to
natural-language requirements, rather than just floating out there.

# Protocol concepts

## Protocol Concepts

A **connection** is a transport-level link between two peers (e.g. a TCP
connection, a WebSocket session).

A **message** is the unit of communication. Messages are exchanged between
peers over a connection.

A **call** is a request/response exchange. One peer sends a Request, the
other sends a Response. Calls are identified by a `request_id`.

A **stream** is a bidirectional byte channel for ordered data transfer.
Either side can send Data messages until they send Eos (end-of-stream).
Streams are identified by a `stream_id`.

## Topologies

The transports covered in this spec are peer-to-peer: there's no inherent
"client" or "server" distinction. Either peer can call methods on the other.
One peer is the **initiator** (opened the connection) and the other is the
**acceptor** (accepted it), but this only affects stream ID allocation —
not who can call whom.

The [shared memory transport](@/shm-spec/_index.md) has a different topology
and is specified separately.

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
[Rust Implementation Specification](@/rust-spec/_index.md). Other language
implementations receive pre-computed method IDs from code generation.

## Schema Evolution

Adding new methods to a service is always safe — peers that don't know about
a method will simply report it as unknown.

Most other changes are breaking:
- Renaming a service
- Renaming a method
- Renaming an argument
- Adding, removing, or reordering arguments
- Changing an argument's type
- Changing the return type
- Adding, removing, or reordering fields in a struct
- Renaming a field in a struct
- Adding, removing, or reordering variants in an enum
- Renaming a variant in an enum
- Changing a variant's payload type

Some type substitutions are compatible because they have the same wire format
and produce the same signature hash:
- `Vec<T>` ↔ `VecDeque<T>` ↔ `HashSet<T>` ↔ `BTreeSet<T>` (all are sequences)
- `HashMap<K, V>` ↔ `BTreeMap<K, V>` ↔ `Vec<(K, V)>` (all are maps / list of pairs)

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
CloseStream.

**Connection errors** are protocol violations. The peer sends a Goodbye
message (citing the violated rule) and closes the connection. Everything
on this connection is torn down. Examples:
  * Data/Eos/CloseStream on an unknown stream ID
  * Data after CloseStream
  * Duplicate in-flight request ID

# Unary RPC

A unary RPC is the simplest form of method call: one request, one response.
This section specifies the complete lifecycle.

## Request IDs

> r[unary.request-id.uniqueness]
>
> A request ID (u64) MUST be unique among in-flight requests. Once a
> Response is received, that request ID MAY be reused for a new request.

> r[unary.request-id.duplicate-detection]
>
> If a peer receives a Request with a `request_id` that matches an
> existing in-flight request, it MUST send a Goodbye message citing
> this rule, then close the connection.

> r[unary.request-id.in-flight]
>
> A request is "in-flight" from when the Request message is sent until
> the corresponding Response message is received. Once the Response
> arrives, the request ID is no longer in-flight — even if streams
> established by the call are still active.

> r[unary.request-id.cancel-still-in-flight]
>
> Sending a Cancel message does NOT remove a request from in-flight status.
> The request remains in-flight until a Response is received (which may be
> a `Cancelled` error, a completed result, or any other response). The
> caller MUST NOT reuse the request ID until the Response arrives.

### Request State Diagram

```aasvg
                                         .-----------.
                                         |           |
                                 .------>|   Idle    |<------.
                                 |       |           |       |
                                 |       '-----+-----'       |
                                 |             |             |
                                 |             | send        |
                      recv       |             | Request     | recv
                      Response   |             v             | Response
                      (success)  |       .-----------.       | (error)
                                 |  .--->|           |----.  |
                                 |  |    | In-Flight |    |  |
                                 '--|    |           |    |--'
                                    |    '-----------'    |
                                    |                     |
                                    '---------------------'
                                          send Cancel
                                       (no state change)
```

The key insight: **Cancel is not a state transition**. It's a hint sent to the
callee, but the request remains in-flight until Response arrives. This prevents
request ID reuse races.

For streaming methods, the Request/Response exchange negotiates streams,
but those streams have their own lifecycle independent of the call. See
[Streaming RPC](#streaming-rpc) for details.

## Initiating a Call

> r[unary.initiate]
>
> A call is initiated by sending a Request message.

A Request contains a `request_id` (for correlation), a `method_id` (identifying
which method to call), and a `payload` (the Postcard-encoded arguments).

> r[unary.request.payload-encoding]
>
> The payload MUST be the Postcard encoding of a tuple containing all
> method arguments in declaration order.

For example, a method `fn add(a: i32, b: i32) -> i64` with arguments `(3, 5)`
would have a payload that is the Postcard encoding of the tuple `(3i32, 5i32)`.

## Completing a Call

> r[unary.complete]
>
> A call is completed by sending a Response message with the same
> `request_id` as the original Request.

A Response contains the `request_id` (echoed from the Request) and either
a success payload or a CallError.

## Call Errors

> r[unary.error.variants]
>
> A CallError indicates why a method call failed at the RPC level (not
> application level). Defined variants:
>
> | Variant | Meaning |
> |---------|---------|
> | `UnknownMethod` | No handler registered for this `method_id` |
> | `InvalidPayload` | Could not deserialize the request payload |
> | `Timeout` | Handler did not respond in time |
> | `Cancelled` | Caller cancelled the request |
> | `Internal` | Handler encountered an internal error |

Note: Application-level errors (e.g., "user not found") are NOT CallErrors.
They are part of the method's return type and encoded in the success payload.
A method returning `Result<User, UserError>::Err(NotFound)` is a successful
RPC — the method ran and returned a value.

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

The number of streams in a call can be dynamic — streams may appear inside
enums, so the actual stream IDs depend on which variant is sent/returned.

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
     |-------- Eos(stream_id=3) ----------------------------------->|
     |                                                              |
     |<------- Data(stream_id=4, result) ---------------------------|
     |                                                              |
     |<------- Eos(stream_id=4) ------------------------------------|
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
> If the callee rejects the call (returns a CallError), no streams are
> opened. The stream IDs in the Request payload are "burned" — they
> were never opened and MUST NOT be reused.

## Stream Data Flow

> r[streaming.data]
>
> Once a stream is open, the sending peer MAY send Data messages
> containing Postcard-encoded values of the stream's element type.

> r[streaming.data.invalid]
>
> If a peer receives a Data message that cannot be deserialized as the
> stream's element type, it MUST send a Goodbye message citing this rule,
> then close the connection.

> r[streaming.eos]
>
> When a peer has no more data to send on a stream, it MUST send an Eos
> message. After sending Eos, the peer MUST NOT send any more Data on
> that stream.

> r[streaming.half-close]
>
> Eos is a half-close. The other direction remains open until the other
> peer sends Eos. A stream is fully closed when both peers have sent Eos.

## Aborting a Stream

> r[streaming.abort]
>
> A peer MAY send CloseStream to signal it does not want to receive more
> data on a stream. The other peer SHOULD stop sending promptly.

> r[streaming.abort.violation]
>
> If a peer continues sending Data after receiving CloseStream, it is a
> protocol error. The receiving peer MUST send Goodbye citing this rule,
> then close the connection.

> r[streaming.unknown]
>
> If a peer receives a stream message (Data, Eos, CloseStream) with a
> `stream_id` that was never opened, it MUST send a Goodbye message
> citing this rule, then close the connection.

## Streams and Call Completion

> r[streaming.call-complete]
>
> The RPC call itself completes when the Response is received. Streams
> have their own lifecycle independent of the call.

This means:
- The request ID is no longer in-flight once the Response arrives
- Streams may remain open indefinitely after the call completes
- Cancelling the call (before Response) does not affect already-opened streams

# Messages

Everything Rapace does — method calls, streams, control signals — is
built on messages exchanged between peers.

```rust
enum Message {
    // Control
    Hello { /* handshake data */ },
    Goodbye { reason: String },
    Ping { token: u64 },
    Pong { token: u64 },
    
    // RPC
    Request { request_id: u64, method_id: u64, payload: Vec<u8> },
    Response { request_id: u64, result: Result<Vec<u8>, CallError> },
    Cancel { request_id: u64 },
    
    // Streams
    Data { stream_id: u64, payload: Vec<u8> },
    Eos { stream_id: u64 },
    CloseStream { stream_id: u64 },
}
```

Messages are Postcard-encoded. The enum discriminant identifies the message
type, and each variant contains only the fields it needs.

## Message Types

### Hello

Sent by both peers immediately after connection establishment. Contains
protocol version, supported features, and method registry for compatibility
checking.

### Goodbye

> r[message.goodbye]
>
> A peer MUST send a Goodbye message before closing the connection due to
> a protocol error. The `reason` field MUST contain a human-readable
> explanation of the violation.

After sending Goodbye, the peer SHOULD close the connection promptly. The
peer receiving Goodbye SHOULD log the reason and close gracefully — no
further messages should be expected.

### Request / Response / Cancel

`Request` initiates an RPC call. `Response` returns the result. `Cancel`
requests that the callee stop processing a request.

The `request_id` correlates requests with responses, enabling multiple
calls to be in flight simultaneously (pipelining).

### Data / Eos / CloseStream

`Data` carries payload bytes on a stream, identified by `stream_id`.
`Eos` signals end-of-stream (half-close). `CloseStream` signals the
sender doesn't want more data on this stream.

### Ping / Pong

Liveness checking. `Ping` requests a `Pong` response with the same token.

# Transports

Different transports require different handling:

| Kind | Example | Framing | Streams |
|------|---------|---------|---------|
| Message | WebSocket | Transport provides | All in one |
| Multi-stream | QUIC | Per stream | Can map to transport streams |
| Byte stream | TCP | COBS | All in one |

## Message Transports

Message transports (like WebSocket) deliver discrete messages. Each transport
message contains exactly one Rapace message, Postcard-encoded.

No additional framing is needed. All messages (control, RPC, stream data)
flow through the same transport connection.

## Multi-stream Transports

Multi-stream transports (like QUIC, WebTransport) provide multiple independent
streams, which can eliminate head-of-line blocking.

> r[transport.multistream.control]
>
> Implementations SHOULD use transport stream 0 for control and RPC messages
> (Hello, Goodbye, Ping, Pong, Request, Response, Cancel).

> r[transport.multistream.streams]
>
> Implementations MAY map each Rapace stream to a dedicated transport stream.
> When doing so, the `stream_id` in Data/Eos/CloseStream messages MAY be
> omitted (the transport stream provides identity).

This is an optimization — implementations can also send all messages through
a single transport stream, just like byte stream transports.

## Byte Stream Transports

Byte stream transports (like TCP) provide a single ordered byte stream.

> r[transport.bytestream.cobs]
>
> Messages MUST be framed using COBS (Consistent Overhead Byte Stuffing).
> Each message MUST be followed by a 0x00 delimiter byte.
> 
> ```
> [COBS-encoded message][0x00][COBS-encoded message][0x00]...
> ```

All messages flow through the single byte stream. The `stream_id` field
in stream messages provides multiplexing.

# Introspection

Peers MAY implement the `Diagnostic` service to help debug method mismatches
and explore available services. This is optional — if a peer doesn't implement
it, calls to `Diagnostic` methods will simply return "unknown method".

```rust
#[rapace::service]
pub trait Diagnostic {
    /// Explain why a method call failed
    async fn explain_mismatch(&self, attempted: MethodDetail) -> MismatchExplanation;
    
    /// List all services this peer implements
    async fn list_services(&self) -> Vec<ServiceSummary>;
    
    /// List methods for a service
    async fn list_methods(&self, service_name: String) -> Vec<MethodSummary>;
    
    /// Get full details for a method
    async fn describe_method(&self, service_name: String, method_name: String) -> MethodDetail;
}
```

The types used by this service (`MethodDetail`, `MismatchExplanation`, etc.)
are defined in the Rust implementation and code-generated for other languages.

When a method call fails with "unknown method", clients can optionally call
`Diagnostic.explain_mismatch` with full details of what they tried to call.
The response indicates whether it was an unknown service, unknown method,
or signature mismatch — enabling tooling to show helpful diffs.

The `list_services`, `list_methods`, and `describe_method` calls allow
exploring what a peer offers, useful for debugging and generic tooling.
