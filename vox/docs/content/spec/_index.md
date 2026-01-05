+++
title = "Rapace specification"
description = "Formal Rapace RPC protocol specification"
+++

# Introduction

r[spec.normative]
The spec MUST be normative.

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

# Topologies

The transports covered in this spec are peer-to-peer: there's no inherent
"client" or "server" distinction. Either peer can call methods on the other.
One peer is the **initiator** (opened the connection) and the other is the
**acceptor** (accepted it), but this only affects channel ID allocation —
not who can call whom.

The [shared memory transport](@/shm-spec/_index.md) has a different topology
and is specified separately.

# Nomenclature

A "proto crate" contains multiple "services" (Rust async traits) which
themselves contain a bunch of "methods" (not functions), which have 
parameters and a return type.

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

# Messages

Everything Rapace does — method calls, streams, tunnels, control signals — is
built on messages exchanged between peers. Each message type carries only the
fields it needs.

```rust
enum Message {
    // Connection control
    Hello { /* handshake data */ },
    
    // Channel lifecycle
    OpenChannel { channel_id: u32, kind: ChannelKind, /* ... */ },
    CloseChannel { channel_id: u32, reason: Option<String> },
    
    // CALL channels
    Request { channel_id: u32, request_id: u32, method_id: u32, payload: Vec<u8> },
    Response { channel_id: u32, request_id: u32, payload: Vec<u8> },
    
    // STREAM/TUNNEL channels
    Data { channel_id: u32, payload: Vec<u8> },
    Eos { channel_id: u32 },
    
    // Flow control
    Credits { channel_id: u32, amount: u32 },
    
    // Cancellation
    Cancel { channel_id: u32 },
    
    // Liveness
    Ping { token: u64 },
    Pong { token: u64 },
}

enum ChannelKind {
    Call,
    Stream,
    Tunnel,
}
```

Messages are Postcard-encoded. The enum discriminant identifies the message
type, and each variant contains only the fields relevant to that message.

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

Implementations MAY map Rapace channels to transport streams (eliminating
head-of-line blocking between channels). In this case, `channel_id` in
messages can be omitted or set to a sentinel value — the transport stream
provides the channel identity.

## Byte Stream Transports

Byte stream transports (like TCP) provide a single ordered byte stream.
Messages are framed using COBS (Consistent Overhead Byte Stuffing), which
uses 0x00 as an unambiguous message delimiter.

```
[COBS-encoded message][0x00][COBS-encoded message][0x00]...
```

All multiplexing happens via Rapace channels.
