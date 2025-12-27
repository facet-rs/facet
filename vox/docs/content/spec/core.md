+++
title = "Core Protocol"
description = "Frames, channels, and control messages"
weight = 40
+++

This document defines the Rapace core protocol: channel lifecycle, message flow, control messages, and the rules that govern communication between peers.

## Design Principles

The protocol is designed for **no v2 ever**:

- Wire framing is frozen forever
- Evolution happens through reserved bits, extensible metadata, and capability negotiation
- Old peers can safely ignore unknown optional features
- Unknown must-have features are rejected cleanly at handshake

## Channels

A **channel** is a logical stream of related messages within a connection. Channels provide multiplexing: multiple concurrent operations share one transport connection.

### Channel Kinds

Every channel has a **kind** established at open time:

| Kind | Purpose |
|------|---------|
| `CONTROL` | Channel 0 only. Carries control messages (OpenChannel, CloseChannel, etc.) |
| `CALL` | Carries exactly one RPC call (one request, one response) |
| `STREAM` | Carries a typed sequence of items (application-level streaming) |
| `TUNNEL` | Carries raw bytes (TCP-like byte stream) |

### Channel ID Allocation

Channel IDs are 32-bit unsigned integers allocated without coordination:

- **Channel 0**: Reserved for control messages (never allocated by either peer)
- **Initiator** (connection opener): Uses odd IDs (1, 3, 5, 7, ...)
- **Acceptor** (connection listener): Uses even IDs (2, 4, 6, 8, ...)

Formula:
- Initiator: `channel_id = 2*n + 1` for n = 0, 1, 2, ... (yields 1, 3, 5, ...)
- Acceptor: `channel_id = 2*n + 2` for n = 0, 1, 2, ... (yields 2, 4, 6, ...)

This deterministic scheme prevents collisions in bidirectional RPC without any coordination.

**Channel ID reuse**: Channel IDs MUST NOT be reused within the lifetime of a connection. This prevents late or duplicated frames from being misinterpreted as belonging to a new channel.

### Channel Lifecycle

```
                    ┌─────────────────────────────────────────┐
                    │              CONTROL (ch 0)             │
                    │  OpenChannel, CloseChannel, Ping, etc.  │
                    └─────────────────────────────────────────┘
                                        │
            ┌───────────────────────────┼───────────────────────────┐
            │                           │                           │
            ▼                           ▼                           ▼
    ┌───────────────┐           ┌───────────────┐           ┌───────────────┐
    │  CALL channel │           │ STREAM channel│           │TUNNEL channel │
    │               │           │               │           │               │
    │ 1 req → 1 resp│           │ N items → EOS │           │ bytes → EOS   │
    └───────────────┘           └───────────────┘           └───────────────┘
```

Channels are opened explicitly via `OpenChannel` on the control channel, used for their designated purpose, and closed when both sides have reached terminal state.

## Channel Opening

Channels are opened by sending an `OpenChannel` control message on channel 0.

### OpenChannel Message

```rust
OpenChannel {
    channel_id: u32,          // The new channel's ID (odd/even per role)
    kind: ChannelKind,        // CALL, STREAM, or TUNNEL
    attach: Option<AttachTo>, // For STREAM/TUNNEL: which call this belongs to
    metadata: Vec<(String, Vec<u8>)>,  // Tracing, auth, etc.
    initial_credits: u32,     // Initial flow control credits (0 = no grant)
}

AttachTo {
    call_channel_id: u32,     // The parent CALL channel
    port_id: u32,             // Port identifier from method signature
    direction: Direction,     // CLIENT_TO_SERVER, SERVER_TO_CLIENT, or BIDIR
}

enum ChannelKind {
    Call = 1,
    Stream = 2,
    Tunnel = 3,
}

enum Direction {
    ClientToServer = 1,
    ServerToClient = 2,
    Bidir = 3,
}
```

### Validation Rules

| Channel Kind | `attach` field |
|--------------|----------------|
| `CALL` | MUST be `None` |
| `STREAM` | MUST be `Some(AttachTo)` |
| `TUNNEL` | MUST be `Some(AttachTo)` |

When receiving `OpenChannel` with `attach`:

| Condition | Response |
|-----------|----------|
| `call_channel_id` does not exist | `CancelChannel { channel_id, reason: ProtocolViolation }` |
| `port_id` not declared by method | `CancelChannel { channel_id, reason: ProtocolViolation }` |
| `kind` mismatches port's declared kind | `CancelChannel { channel_id, reason: ProtocolViolation }` |
| `direction` mismatches expected direction | `CancelChannel { channel_id, reason: ProtocolViolation }` |

When receiving `OpenChannel` without `attach` (for CALL channels):

| Condition | Response |
|-----------|----------|
| `kind` is STREAM or TUNNEL (attach required) | `CancelChannel { channel_id, reason: ProtocolViolation }` |
| `max_channels` limit exceeded | `CancelChannel { channel_id, reason: ResourceExhausted }` |
| Channel ID parity wrong (e.g., acceptor using odd ID) | `CancelChannel { channel_id, reason: ProtocolViolation }` |

All `CancelChannel` responses are sent on channel 0. The connection remains open unless the violation indicates a fundamentally broken peer (repeated violations may warrant `GoAway`).

### Who Opens Which Channels

- **Client** opens:
  - CALL channels (to initiate RPC calls)
  - Request stream/tunnel ports (client→server attachments)
- **Server** opens:
  - Response stream/tunnel ports (server→client attachments)

Pre-opening channels for the other side is not permitted.

## CALL Channels

A CALL channel carries exactly **one request** and **one response**. This is the primary RPC primitive.

### Message Flow

```
Client                                          Server
   │                                               │
   │  DATA | EOS                                   │
   │  method_id = <method hash>                    │
   │  payload = request args + port refs           │
   ├──────────────────────────────────────────────▶│
   │                                               │
   │                         DATA | EOS | RESPONSE │
   │                         method_id = <same>    │
   │                         payload = CallResult  │
   │◀──────────────────────────────────────────────┤
   │                                               │
```

### Request Frame

- **Flags**: `DATA | EOS` (MUST include EOS)
- **method_id**: Method identifier (see [Method ID Computation](#method-id-computation))
- **msg_id**: Unique message ID for correlation
- **payload**: Postcard-encoded request arguments, including `PortRef` values for any stream/tunnel parameters

### Response Frame

- **Flags**: `DATA | EOS | RESPONSE` (MUST include all three)
- **method_id**: Same as request (for logging/metrics/debugging)
- **msg_id**: MUST be the same as the request's `msg_id` (for correlation). See [Frame Format: msg_id Semantics](@/spec/frame-format.md#msg_id-semantics).
- **payload**: `CallResult` envelope (see below)

### CallResult Envelope

Every response uses a fixed envelope structure:

```rust
struct CallResult {
    status: Status,
    trailers: Vec<(String, Vec<u8>)>,
    body: Option<Vec<u8>>,  // Present on success, absent on error
}

struct Status {
    code: u32,              // 0 = OK, non-zero = error
    message: String,        // Human-readable description
    details: Vec<u8>,       // Opaque, for structured error details
}
```

This envelope provides:

- **Status**: Success or structured error (code + message + details)
- **Trailers**: Key-value metadata available after processing completes
- **Body**: The actual response data (only present when `status.code == 0`)

### Error Responses

Errors are signaled within the `CallResult` envelope:

- **Flags**: `DATA | EOS | RESPONSE | ERROR`
- **payload**: `CallResult` with `status.code != 0` and `body = None`

The `ERROR` flag is a fast-path hint that mirrors the envelope status. Implementations MUST ensure the flag matches the envelope (ERROR flag set iff `status.code != 0`).

### Why One Frame Each Direction?

- **Simplicity**: No multi-frame state machines for call channels
- **Determinism**: EOS is always present, no ambiguity about "more messages coming"
- **Trailers without extra frames**: Status and trailers are in the response envelope
- **Streams do the heavy lifting**: Multi-message flows use attached STREAM channels

## STREAM Channels

A STREAM channel carries a **typed sequence of items** as an attachment to a CALL.

### Attachment Model

STREAM channels are always attached to a parent CALL via a **port**:

```rust
// Method signature with stream arguments/returns
fn upload(meta: UploadMeta, data: Stream<Chunk>) -> UploadResult;
fn fetch(req: FetchReq) -> (meta: FetchMeta, body: Stream<Chunk>);
```

Each `Stream<T>` in the signature is assigned a **port_id** by codegen:

- **Request ports** (client→server): 1, 2, 3, ... (in declaration order)
- **Response ports** (server→client): 101, 102, 103, ... (in declaration order)

### Port References

In the request/response payload, stream positions are represented as port IDs:

- **Required stream**: `u32` (the port_id)
- **Optional stream**: `Option<u32>` (None if stream not used)

The actual channel binding happens via `OpenChannel { attach: AttachTo { call_channel_id, port_id, direction } }`.

### Message Flow

```
Client                                          Server
   │                                               │
   │  OpenChannel(ch=3, STREAM, attach=(1, 1, C→S))│
   ├──────────────────────────────────────────────▶│
   │                                               │
   │  [call request on ch=1 with port_id=1]        │
   ├──────────────────────────────────────────────▶│
   │                                               │
   │  DATA (stream item) on ch=3                   │
   ├──────────────────────────────────────────────▶│
   │  DATA (stream item) on ch=3                   │
   ├──────────────────────────────────────────────▶│
   │  DATA | EOS on ch=3                           │
   ├──────────────────────────────────────────────▶│
   │                                               │
   │                    [call response on ch=1]    │
   │◀──────────────────────────────────────────────┤
```

### Stream Frame Format

- **Flags**: `DATA` for items, `DATA | EOS` for final item (or EOS-only for empty stream)
- **method_id**: MUST be 0 (streams are not methods)
- **payload**: Postcard-encoded item of type `T`

### Empty Streams

An empty stream (zero items) is represented by a single frame with `EOS` flag and `payload_len = 0`. The `DATA` flag MAY be omitted for empty streams.

### Type Enforcement

The receiver knows the expected item type `T` from:
1. The method signature (identified by `method_id` on the parent CALL channel)
2. The port binding (identified by `port_id` in the `AttachTo` attachment)

If payload decoding fails (type mismatch, malformed data):
- The receiver SHOULD send `CancelChannel` for the stream channel with reason `ProtocolViolation`
- The parent call SHOULD fail with an appropriate error status

### Ordering

- `OpenChannel` for required ports MUST be sent before or concurrently with the root request message
- Receiver MUST accept either order (network reordering happens)
- If request arrives but required port not yet opened, receiver waits until port opens or deadline hits

### Bidirectional Streams

For `direction = BIDIR`:

- One channel, both sides can send items
- Each side sends `EOS` independently (half-close semantics)
- Channel is fully closed when both sides have sent `EOS`

## TUNNEL Channels

A TUNNEL channel carries **raw bytes** (TCP-like stream) as an attachment to a CALL.

### Semantics

- Same attachment model as STREAM (port_id, OpenChannel, etc.)
- **method_id**: MUST be 0
- `EOS` is half-close (exactly like TCP FIN)

### Raw Bytes (Not Postcard)

TUNNEL payloads are **uninterpreted raw bytes**, NOT Postcard-encoded. This is the only exception to Postcard payload encoding in Rapace.

- Each frame's `payload` field contains raw bytes
- Frame boundaries are transport artifacts, NOT message boundaries
- Implementations MUST reassemble frames into a continuous byte stream for the application
- Applications MUST NOT rely on frame boundaries for message framing
- Payload size per frame is still bounded by negotiated `max_payload_size`

### Ordering and Reliability

- TUNNEL frames are ordered within the channel (as per transport ordering)
- TUNNEL channels MUST be reliable; implementations MUST NOT use WebTransport datagrams for TUNNEL
- Lost or reordered frames would corrupt the byte stream

### Flow Control

Credits for TUNNEL channels count raw payload bytes (`payload_len`):
- A frame with `payload_len = 1000` consumes 1000 credits
- EOS-only frames (no payload) consume 0 credits

### Use Cases

- Proxying TCP connections
- File transfer (when you don't want item framing overhead)
- Embedding existing protocols (HTTP/1.1, database wire protocols, etc.)

## Half-Close and Termination

### EOS (End of Stream)

`EOS` flag means: "I will not send more DATA on this channel, but I can still receive."

After sending `EOS`:

- Sender MUST NOT send more DATA frames on that channel (in that direction)
- Receiver continues processing until it also sends `EOS`

### Channel States

For each direction (send/receive) on a channel:

| State | Meaning |
|-------|---------|
| `OPEN` | Can send/receive DATA |
| `EOS_SENT` | Sent EOS, waiting for peer's EOS |
| `EOS_RECEIVED` | Received peer's EOS |
| `CLOSED` | Both directions have EOS (or canceled) |

### Full Close

A channel is fully closed when:

- Both sides have sent `EOS`, OR
- `CancelChannel` was sent/received

After full close, implementations MAY free channel state. Channel IDs MUST NOT be reused within the lifetime of a connection.

### CloseChannel

`CloseChannel` is an explicit teardown hint:

```rust
CloseChannel {
    channel_id: u32,
    reason: CloseReason,
}

enum CloseReason {
    Normal,
    Error(String),
}
```

- Sent on channel 0
- Signals that the sender has freed its state for this channel
- **Unilateral**: No acknowledgment is required or defined
- **Optional**: EOS from both sides is sufficient for graceful close; `CloseChannel` is an optimization to free state earlier

### CancelChannel (Abort)

`CancelChannel` is an immediate abort:

```rust
CancelChannel {
    channel_id: u32,
    reason: CancelReason,
}

enum CancelReason {
    ClientCancel,
    DeadlineExceeded,
    ResourceExhausted,
    ProtocolViolation,
}
```

- Stops work immediately
- Discards pending data
- Wakes waiters with error
- Idempotent (multiple cancels are fine)

**Cancel propagation**:

- `CancelChannel(call_channel)` cancels all attached stream/tunnel channels
- `CancelChannel(attached_channel)` cancels only that port; call may still complete if port is optional

## Call Completion

A call is **complete** when:

1. Client has sent request `DATA | EOS` on the CALL channel
2. Server has sent response `DATA | EOS | RESPONSE` on the CALL channel
3. All **required** attached ports have reached terminal state:
   - For client→server required streams: server received `EOS` (or cancel)
   - For server→client required streams: client received `EOS` (or cancel)

### Optional Ports

- If a stream port is optional (`Option<Stream<T>>`) and not used, no channel is opened
- The port_id field in the payload is `None`
- Call can complete without that port reaching terminal state

### Required Ports Not Opened

If a required port is never opened before deadline:

- Server treats as `FAILED_PRECONDITION` error
- Response envelope contains appropriate error status

## Control Channel (Channel 0)

Channel 0 is reserved for control messages.

**Canonical rule**: Control messages are identified by `channel_id == 0`. The `CONTROL` flag MUST be set on all frames with `channel_id == 0`, and MUST NOT be set on any other channel. This redundancy allows fast filtering without inspecting `channel_id`.

Control frames use `method_id` as a verb selector.

### Control Verbs

| method_id | Verb | Payload |
|-----------|------|---------|
| 0 | `Hello` | `{ protocol_version, role, required_features, ... }` |
| 1 | `OpenChannel` | `{ channel_id, kind, attach, metadata, initial_credits }` |
| 2 | `CloseChannel` | `{ channel_id, reason }` |
| 3 | `CancelChannel` | `{ channel_id, reason }` |
| 4 | `GrantCredits` | `{ channel_id, bytes }` |
| 5 | `Ping` | `{ payload: [u8; 8] }` |
| 6 | `Pong` | `{ payload: [u8; 8] }` |
| 7 | `GoAway` | `{ reason, last_channel_id, message, metadata }` |

Control payloads are postcard-encoded like regular messages.

See [Handshake & Capabilities](@/spec/handshake.md) for the full `Hello` message format.

### GoAway

The `GoAway` control message signals graceful shutdown:

```rust
GoAway {
    reason: GoAwayReason,
    last_channel_id: u32,  // Last channel ID the sender will process
    message: String,        // Human-readable reason
    metadata: Vec<(String, Vec<u8>)>,  // Extension data
}

enum GoAwayReason {
    Shutdown = 1,           // Planned shutdown
    Maintenance = 2,        // Maintenance window
    Overload = 3,           // Server overloaded
    ProtocolError = 4,      // Peer misbehaving
}
```

**`last_channel_id` semantics**:
- This is the highest channel ID **opened by the peer** (not by the sender) that the sender will still process
- Initiator (client) uses odd IDs; acceptor (server) uses even IDs
- The sender commits to completing work on channels with `channel_id <= last_channel_id` that were opened by the peer

After sending `GoAway`:
- Sender continues processing existing calls on channels opened by peer with `channel_id <= last_channel_id`
- Sender rejects new `OpenChannel` from peer with `channel_id > last_channel_id` using `CancelChannel { reason: ResourceExhausted }`
- Sender will not open new channels itself
- Sender closes connection after a grace period (recommended: 30 seconds or until all accepted channels complete)

See [Overload & Draining](@/spec/overload.md) for detailed shutdown semantics.

### Unknown Control Verbs

When a peer receives a control message with an unknown `method_id`:

**Reserved range (0-99)**:
- The receiver MUST send `GoAway { reason: ProtocolError, message: "unknown control verb", last_channel_id: <current_max> }`
- The receiver SHOULD close the connection after a short grace period
- This indicates a protocol version mismatch or buggy peer

**Extension range (100+)**:
- The receiver MUST ignore the message silently (no response)
- The connection continues normally
- This allows future extensions without breaking older peers

### Ping/Pong

- **Purpose**: Application-level liveness check
- **Semantics**: Receiver MUST respond to `Ping` with `Pong` containing the same payload
- **Timing**: Implementation-defined (not mandatory keepalive)
- **Negotiation**: Peers may negotiate ping support via handshake capabilities

Note: Some transports (WebSocket) have their own ping/pong. Rapace-level ping is for "Rapace liveness" across intermediaries that might not forward transport-level pings.

## Method ID Computation

Method IDs are 32-bit identifiers used to dispatch RPC calls. They are computed as a hash of the fully-qualified method name.

### Hash Algorithm

Method IDs use FNV-1a (Fowler-Noll-Vo) hash, folded to 32 bits:

```rust
fn compute_method_id(service_name: &str, method_name: &str) -> u32 {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    
    let mut hash: u64 = FNV_OFFSET;
    
    // Hash service name
    for byte in service_name.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    
    // Hash separator
    hash ^= b'.' as u64;
    hash = hash.wrapping_mul(FNV_PRIME);
    
    // Hash method name
    for byte in method_name.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    
    // Fold to 32 bits
    ((hash >> 32) ^ hash) as u32
}
```

### Input String Format

The hash input is: `"{ServiceName}.{MethodName}"` where:
- `ServiceName` is the unqualified service trait name (e.g., `"Calculator"`)
- `MethodName` is the method name (e.g., `"add"`)

Example: `"Calculator.add"` → method_id

### Reserved Method IDs

- `method_id = 0`: Reserved for control messages (on channel 0) and for STREAM/TUNNEL channel frames (which are not method calls).

**Enforcement**:
- Code generators MUST check if `compute_method_id(service, method)` returns 0
- If it does, code generation MUST fail with an error instructing the developer to rename the method
- Handshake MUST reject any method registry entry with `method_id = 0`

This reservation ensures unambiguous routing: `method_id = 0` always means "not an RPC method."

### Collision Handling

Method ID collisions (different methods hashing to the same ID) MUST be detected at build time. If a collision is detected:
- Code generation MUST fail with an error
- The developer must rename one of the conflicting methods

At runtime, if a peer receives a method_id it doesn't recognize, it MUST respond with error code `UNIMPLEMENTED`.

## Flow Control

Rapace uses **credit-based flow control** per channel.

### Semantics

- Every channel has an **inbound credit window** (bytes the peer may send)
- Sender MUST NOT exceed granted credit
- Receiver grants credits via:
  - `GrantCredits` control message, OR
  - `credit_grant` field in `MsgDescHot` (fast path, with `CREDITS` flag)

### Credit Grant

```rust
GrantCredits {
    channel_id: u32,
    bytes: u32,
}
```

Credits are additive: if you grant 1000, then grant 500, the peer has 1500 bytes available.

### Default Mode

Implementations MAY run in "infinite credit" mode:

- Grant a very large initial window (e.g., `u32::MAX`)
- Periodically top up as needed
- Protocol semantics remain the same

This allows simple implementations while preserving the ability to add real backpressure later.

### Credit Overrun (Protocol Error)

If a receiver sees a frame whose `payload_len` exceeds the remaining credits for that channel:

1. This is a **protocol error** (the sender violated flow control)
2. Receiver SHOULD send `GoAway { reason: ProtocolError, message: "credit overrun" }`
3. Receiver MUST close the transport connection
4. Receiver MUST discard any frames that arrive after initiating close

Credit overrun is a serious violation because it indicates a buggy or malicious peer.

### EOS Frames and Credits

Frames with only the `EOS` flag (no `DATA` flag, `payload_len = 0`) do **not** consume credits. This ensures half-close can always be signaled regardless of credit state.

### Why Per-Channel Credits?

- Different streams can have different flow characteristics
- File upload stream can be heavily windowed
- Small "progress/log" stream stays responsive
- Enables priority-based scheduling (grant credits to important channels first)

## Frame Reference

### MsgDescHot (64 bytes)

```rust
#[repr(C, align(64))]
pub struct MsgDescHot {
    // Identity (16 bytes)
    pub msg_id: u64,
    pub channel_id: u32,
    pub method_id: u32,

    // Payload location (16 bytes)
    pub payload_slot: u32,
    pub payload_generation: u32,
    pub payload_offset: u32,
    pub payload_len: u32,

    // Flow control & timing (16 bytes)
    pub flags: FrameFlags,
    pub credit_grant: u32,
    pub deadline_ns: u64,

    // Inline payload (16 bytes)
    pub inline_payload: [u8; 16],
}
```

### FrameFlags

```rust
bitflags! {
    pub struct FrameFlags: u32 {
        const DATA          = 0b0000_0001;  // Regular data frame
        const CONTROL       = 0b0000_0010;  // Control message (channel 0)
        const EOS           = 0b0000_0100;  // End of stream (half-close)
        const _RESERVED_08  = 0b0000_1000;  // Reserved (do not use)
        const ERROR         = 0b0001_0000;  // Error response
        const HIGH_PRIORITY = 0b0010_0000;  // Priority hint (maps to priority 192)
        const CREDITS       = 0b0100_0000;  // Contains credit grant
        const _RESERVED_80  = 0b1000_0000;  // Reserved (do not use)
        const NO_REPLY      = 0b0001_0000_0000;  // Reserved: fire-and-forget
        const RESPONSE      = 0b0010_0000_0000;  // This is a response frame
    }
}
```

**Flag semantics**:

- `DATA`: Frame carries payload data
- `CONTROL`: Frame is a control message (MUST only be set when `channel_id == 0`)
- `EOS`: End of stream (half-close); sender will not send more data on this channel
- `ERROR`: Response indicates an error (MUST match `status.code != 0` in envelope)
- `CREDITS`: The `credit_grant` field contains a valid credit grant
- `RESPONSE`: Frame is a response to a request (not a request itself)

**Reserved flags**: Flags marked "Reserved" (prefixed with `_RESERVED`) are allocated but not yet defined. Implementations MUST NOT set reserved flags. Receivers MUST ignore unknown flags unless the feature is negotiated as "must-understand" in handshake.

**HIGH_PRIORITY flag**: This flag is a fast-path hint for binary high/normal priority. When set, it maps to priority level 192. When not set, priority defaults to 128 (or the per-call/connection priority if specified). See [Prioritization & QoS](@/spec/prioritization.md) for details. Receivers MAY ignore this flag if they don't implement priority-based scheduling.

### Flag Combinations by Channel Kind

| Channel Kind | Request | Response | Stream Item | Final Item | Error |
|--------------|---------|----------|-------------|------------|-------|
| CALL | `DATA \| EOS` | `DATA \| EOS \| RESPONSE` | N/A | N/A | `DATA \| EOS \| RESPONSE \| ERROR` |
| STREAM | N/A | N/A | `DATA` | `DATA \| EOS` | N/A |
| TUNNEL | N/A | N/A | `DATA` | `DATA \| EOS` | N/A |
| CONTROL | `CONTROL` | N/A | N/A | N/A | N/A |

## Extension Points

The protocol includes extension points for forward compatibility:

1. **Reserved flag bits**: Unused bits in `FrameFlags` for future features
2. **Metadata fields**: `OpenChannel.metadata` and `CallResult.trailers` for key-value extensions
3. **Capability negotiation**: Handshake exchange of supported features (see [Handshake & Capabilities](@/spec/handshake.md))
4. **Status details**: `Status.details` for structured error information

New features are added by:

- Defining new capability flags
- Using metadata/trailers for optional data
- Reserving new control verbs (method_id on channel 0)

Old peers safely ignore unknown optional features. Unknown must-have features are rejected at handshake.

## Next Steps

- [Frame Format](@/spec/frame-format.md) – How frames are structured
- [Transport Bindings](@/spec/transport-bindings.md) – How frames are sent over different transports
- [Handshake & Capabilities](@/spec/handshake.md) – Connection establishment and feature negotiation
- [Cancellation & Deadlines](@/spec/cancellation.md) – Request cancellation and timeout semantics
- [Error Handling & Retries](@/spec/errors.md) – Error codes and retry semantics
