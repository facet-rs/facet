+++
title = "Rapace specification"
description = "Formal Rapace RPC protocol specification"
+++

# Introduction

This is Rapace specification v1.0.0, last updated January 5, 2026. It canonically
lives at https://github.com/bearcove/rapace — where you can get the latest version.

Rapace is a Rust-native RPC protocol. Services are defined inside of Rust
"proto" crates, annotating traits with the `#[rapace::service]` proc macro
attribute:

```rust
#[rapace::service]
pub trait TemplateHost {
    /// Load a template by name.
    async fn load_template(&self, context_id: ContextId, name: String) -> LoadTemplateResult;

    /// Resolve a data value by path.
    async fn resolve_data(&self, context_id: ContextId, path: Vec<String>) -> ResolveDataResult;

    /// Get child keys at a data path.
    async fn keys_at(&self, context_id: ContextId, path: Vec<String>) -> KeysAtResult;

    /// Call a template function on the host.
    async fn call_function(
        &self,
        context_id: ContextId,
        name: String,
        args: Vec<Value>,
        kwargs: Vec<(String, Value)>,
    ) -> CallFunctionResult;
}
```

All types that occur as arguments or in return position must implement
[facet](https://facet.rs), so that they might be serialized and deserialized
with [facet-postcard](https://crates.io/crates/facet-postcard).

Clients/servers for other languages (Swift, TypeScript) are generated using
a Rust codegen package which is linked together with the "proto" crate to
output Swift/TypeScript packages.

This specification exists to ensure that various implementations are compatible, and
to ensure that those implementations are specified — that their code corresponds to
natural-language requirements, rather than just floating out there.

# Nomenclature

## Protocol Concepts

A **connection** is a transport-level link between two peers (e.g. a TCP
connection, a WebSocket session).

A **channel** is a logical multiplexed stream within a connection. Channels
have a kind (Call, Stream, or Tunnel) that determines their behavior.

A **message** is the unit of communication. Messages are sent on channels.
Different channel kinds accept different message types.

A **call** is a request/response exchange on a Call channel. One peer sends
a Request, the other sends a Response.

A **stream** is a channel for ordered data transfer. Either side can send
Data messages until they send Eos (end-of-stream).

A **tunnel** is like a stream, but carries raw bytes instead of
Postcard-encoded payloads.

## Topologies

The transports covered in this spec are peer-to-peer: there's no inherent
"client" or "server" distinction. Either peer can call methods on the other.
One peer is the **initiator** (opened the connection) and the other is the
**acceptor** (accepted it), but this only affects channel ID allocation —
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

Every method has a unique 64-bit identifier computed from its service name,
method name, and signature. This is what gets sent on the wire in `Request`
messages.

> r[method.identity.computation]
>
> The method ID MUST be computed as:
> ```
> method_id = blake3(kebab(ServiceName) + "." + kebab(methodName) + sig_bytes)[0..8]
> ```
> Where:
> - `kebab()` converts to kebab-case (e.g. `TemplateHost` → `template-host`)
> - `sig_bytes` is the BLAKE3 hash of the method's argument and return types
> - `[0..8]` takes the first 8 bytes as a u64

This means:
- Different services can have methods with the same name without collision
- Renaming a service or method changes the ID (breaking change)
- Changing the signature changes the ID (breaking change)
- Case variations normalize to the same ID (`loadTemplate` = `load_template`)

The exact algorithm for computing `sig_bytes` is defined in the
[Rust Implementation Specification](@/rust-spec/_index.md). Other language
implementations receive pre-computed method IDs from code generation.

When a peer receives a `Request` with an unknown `method_id`, it returns an
error. For debugging, peers MAY implement the `Diagnostic` service (see
[Introspection](#introspection)) to provide detailed mismatch information.

## Errors

There are different kinds of errors in Rapace and they have different severity:

**Protocol errors** mean someone messed up the wire format and there's nothing
we can do to help. Whenever possible we'll send a human-readable payload back
explaining why we're disconnecting, but... we're going to be disconnecting.

Examples of protocol errors include:

  * Invalid handshake format
  * Invalid postcard for a method's arguments or return types
  * Sending a message that's too big
  
**Call errors** mean a method call did not succeed, but it's not going to bring
down the entire connection.

Examples of call errors include:

  * The method call did not complete in a timely fashion
  * The peer hung up while we were waiting for a response
  * The method did complete but returned a user-defined error

# Channels

A connection multiplexes multiple **channels**.
Each channel has a kind that determines what messages can be sent on it:

| Kind | Purpose | Messages |
|------|---------|----------|
| Call | Request/response RPC | Request, Response |
| Stream | Ordered byte stream | Data, Eos |
| Tunnel | Raw bidirectional pipe | Data, Eos |

Channel IDs are `u32`. The initiator allocates odd IDs (1, 3, 5, ...), the
acceptor allocates even IDs (2, 4, 6, ...). Channel 0 is reserved for
connection-level control messages (Hello, Ping, Pong).

Channels must be explicitly opened with `OpenChannel` before use, and closed
with `CloseChannel` or implicitly when both sides have sent `Eos`.

## Call Channels

A Call channel supports request/response RPC. Multiple requests can be in
flight simultaneously (pipelining) — each request has a `request_id` scoped
to the channel, and the response echoes it for correlation.

## Stream Channels

A Stream channel carries an ordered sequence of `Data` messages. Either side
can send `Eos` to signal they're done sending (half-close). Payloads are
Postcard-encoded.

## Tunnel Channels

A Tunnel channel is like a Stream, but payloads are raw bytes (not
Postcard-encoded). Useful for proxying or embedding other protocols.

# Messages

Everything Rapace does — method calls, streams, tunnels, control signals — is
built on messages exchanged between peers.

```rust
struct Message {
    channel_id: u32,
    payload: MessagePayload,
}

enum MessagePayload {
    // Connection control (channel_id = 0)
    Hello { /* handshake data */ },
    Ping { token: u64 },
    Pong { token: u64 },
    
    // Channel lifecycle
    OpenChannel { kind: ChannelKind, /* ... */ },
    CloseChannel { reason: Option<String> },
    
    // CALL channels
    Request { request_id: u32, method_id: u32, payload: Vec<u8> },
    Response { request_id: u32, payload: Vec<u8> },
    
    // STREAM/TUNNEL channels
    Data { payload: Vec<u8> },
    Eos,
    
    // Flow control
    Credits { amount: u32 },
    
    // Cancellation
    Cancel,
}

enum ChannelKind {
    Call,
    Stream,
    Tunnel,
}
```

Every message has a `channel_id` identifying which channel it belongs to.
Channel 0 is reserved for connection-level control (Hello, Ping, Pong).

Messages are Postcard-encoded. The `MessagePayload` discriminant identifies
the message type, and each variant contains only the fields it needs.

## Message Types

### Hello

Sent by both peers immediately after connection establishment. Contains
protocol version, supported features, and method registry for compatibility
checking. See [Handshake](#handshake).

### OpenChannel / CloseChannel

Opens or closes a logical channel. Channels are multiplexed over a single
connection. The initiator uses odd channel IDs (1, 3, 5, ...), the acceptor
uses even channel IDs (2, 4, 6, ...).

### Request / Response

Used on CALL channels. `Request` initiates a method call; `Response` returns
the result. The `request_id` is scoped to the channel and used to correlate
responses with requests (allows pipelining multiple calls on one channel).

### Data / Eos

Used on STREAM and TUNNEL channels. `Data` carries payload bytes. `Eos`
signals end-of-stream (half-close).

### Credits

Grants flow control credits to the peer for a specific channel. The peer
may send up to `amount` additional bytes on that channel.

### Cancel

Requests cancellation of work on a channel. The peer should stop processing
and close the channel.

### Ping / Pong

Liveness checking. `Ping` requests a `Pong` response with the same token.

# Transports

Different transports require different handling:

| Kind | Example | Framing | Multiplexing |
|------|---------|---------|--------------|
| Message | WebSocket | Transport provides | Rapace channels |
| Multi-stream | QUIC | Per stream | Can map to transport streams |
| Byte stream | TCP | COBS | Rapace channels |

## Message Transports

Message transports (like WebSocket) deliver discrete messages. Each transport
message contains exactly one Rapace message, Postcard-encoded.

No additional framing is needed.

## Multi-stream Transports

Multi-stream transports (like QUIC) provide multiple independent streams.
Each stream carries Rapace messages with COBS framing.

> r[transport.multistream.channel-mapping]
>
> Implementations MUST map Rapace channels to transport streams, eliminating
> head-of-line blocking between channels.
>
> The `channel_id` field in messages MUST be set to `0xFFFFFFFF`. The
> transport stream provides the channel identity.

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

All multiplexing happens via Rapace channels.

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
