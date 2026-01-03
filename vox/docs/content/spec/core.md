+++
title = "Core Protocol"
description = "Frames, channels, and control messages"
weight = 40
+++

This document defines the Rapace core protocol: channel lifecycle, message flow, control messages, and the rules that govern communication between peers.

## Channels

A **channel** is a logical stream of related messages within a connection. Channels provide multiplexing: multiple concurrent operations share one transport connection.

### Channel Kinds

r[core.channel.kind]
Every channel MUST have a **kind** established at open time. The kind MUST remain constant for the lifetime of the channel.

| Kind | Purpose |
|------|---------|
| `CONTROL` | Channel 0 only. Carries control messages (OpenChannel, CloseChannel, etc.) |
| `CALL` | Carries exactly one RPC call (one request, one response) |
| `STREAM` | Carries a typed sequence of items (application-level streaming) |
| `TUNNEL` | Carries raw bytes (TCP-like byte stream) |

### Channel ID Allocation

r[core.channel.id.allocation]
Channel IDs MUST be 32-bit unsigned integers.

r[core.channel.id.zero-reserved]
Channel 0 MUST be reserved for control messages. Neither peer SHALL allocate channel 0 for application use.

r[core.channel.id.parity.initiator]
The initiator (connection opener) MUST use odd channel IDs (1, 3, 5, 7, ...).

r[core.channel.id.parity.acceptor]
The acceptor (connection listener) MUST use even channel IDs (2, 4, 6, 8, ...).

Formula:
- Initiator: `channel_id = 2*n + 1` for n = 0, 1, 2, ... (yields 1, 3, 5, ...)
- Acceptor: `channel_id = 2*n + 2` for n = 0, 1, 2, ... (yields 2, 4, 6, ...)

This deterministic scheme prevents collisions in bidirectional RPC without any coordination.

r[core.channel.id.no-reuse]
**Channel ID reuse**: Each channel ID MUST be used at most once within the lifetime of a connection. This prevents late or duplicated frames from being misinterpreted as belonging to a new channel.

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

r[core.channel.lifecycle]
Channels MUST be opened explicitly via `OpenChannel` on the control channel before any data frames are sent on that channel. A channel is closed when both sides have reached terminal state (both sent EOS, or CancelChannel received).

## Channel Opening

r[core.channel.open]
Channels MUST be opened by sending an `OpenChannel` control message on channel 0. A peer MUST wait until it has sent (as opener) or received (as acceptor) the corresponding `OpenChannel` before sending data frames on that channel.

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

r[core.channel.open.attach-required]
| Channel Kind | `attach` field |
|--------------|----------------|
| `CALL` | MUST be `None` |
| `STREAM` | MUST be `Some(AttachTo)` |
| `TUNNEL` | MUST be `Some(AttachTo)` |

r[core.channel.open.attach-validation]
When receiving `OpenChannel` with `attach`, implementations MUST respond as follows:

| Condition | Response |
|-----------|----------|
| `call_channel_id` does not exist | `CancelChannel { channel_id, reason: ProtocolViolation }` |
| `port_id` not declared by method | `CancelChannel { channel_id, reason: ProtocolViolation }` |
| `kind` mismatches port's declared kind | `CancelChannel { channel_id, reason: ProtocolViolation }` |
| `direction` mismatches expected direction | `CancelChannel { channel_id, reason: ProtocolViolation }` |

r[core.channel.open.call-validation]
When receiving `OpenChannel` without `attach` (for CALL channels), implementations MUST respond as follows:

| Condition | Response |
|-----------|----------|
| `kind` is STREAM or TUNNEL (attach required) | `CancelChannel { channel_id, reason: ProtocolViolation }` |
| `max_channels` limit exceeded | `CancelChannel { channel_id, reason: ResourceExhausted }` |
| Channel ID parity wrong (e.g., acceptor using odd ID) | `CancelChannel { channel_id, reason: ProtocolViolation }` |

r[core.channel.open.cancel-on-violation]
All `CancelChannel` responses MUST be sent on channel 0. The connection SHOULD remain open unless the violation indicates a fundamentally broken peer (repeated violations may warrant `GoAway`).

### Who Opens Which Channels

r[core.channel.open.ownership]
The client MUST open CALL channels and request-direction stream/tunnel ports (client→server attachments). The server MUST open response-direction stream/tunnel ports (server→client attachments).

r[core.channel.open.no-pre-open]
Each peer MUST open only the channels it will send data on.

## CALL Channels

r[core.call.one-req-one-resp]
A CALL channel MUST carry exactly one request and one response. The client MUST send exactly one request frame with the `EOS` flag. The server MUST send exactly one response frame with the `EOS | RESPONSE` flags.

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

r[core.call.request.flags]
Request frames MUST have the `DATA | EOS` flags set.

r[core.call.request.method-id]
The `method_id` field MUST contain the method identifier computed as specified in [Method ID Computation](#method-id-computation).

r[core.call.request.payload]
The payload MUST be Postcard-encoded request arguments, including `PortRef` values for any stream/tunnel parameters.

### Response Frame

r[core.call.response.flags]
Response frames MUST have the `DATA | EOS | RESPONSE` flags set.

r[core.call.response.method-id]
The `method_id` MUST be the same as the request for correlation and routing.

r[core.call.response.msg-id]
The `msg_id` MUST be the same as the request's `msg_id` for correlation. See [Frame Format: msg_id Semantics](@/spec/frame-format.md#msg_id-semantics).

r[core.call.response.payload]
The payload MUST be a Postcard-encoded `CallResult` envelope (see below).

### CallResult Envelope

r[core.call.result.envelope]
Every response MUST use the following envelope structure:

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

r[core.call.error.flags]
Error responses MUST use the following format:

- **Flags**: `DATA | EOS | RESPONSE | ERROR`
- **payload**: `CallResult` with `status.code != 0` and `body = None`

r[core.call.error.flag-match]
The `ERROR` flag is a fast-path hint that mirrors the envelope status. Implementations MUST ensure the flag matches the envelope (ERROR flag set iff `status.code != 0`).

### Why One Frame Each Direction?

- **Simplicity**: No multi-frame state machines for call channels
- **Determinism**: EOS is always present, no ambiguity about "more messages coming"
- **Trailers without extra frames**: Status and trailers are in the response envelope
- **Streams do the heavy lifting**: Multi-message flows use attached STREAM channels

## STREAM Channels

r[core.stream.intro]
A STREAM channel MUST carry a typed sequence of items and MUST be attached to a parent CALL channel.

### Attachment Model

r[core.stream.attachment]
STREAM channels MUST always be attached to a parent CALL via a **port**. A STREAM channel without an attachment is a protocol violation.

```rust
// Method signature with stream arguments/returns
fn upload(meta: UploadMeta, data: Stream<Chunk>) -> UploadResult;
fn fetch(req: FetchReq) -> (meta: FetchMeta, body: Stream<Chunk>);
```

r[core.stream.port-id-assignment]
Each `Stream<T>` in the method signature MUST be assigned a **port_id** by codegen:
- Request ports (client→server) MUST use IDs 1, 2, 3, ... (in declaration order)
- Response ports (server→client) MUST use IDs 101, 102, 103, ... (in declaration order)

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

r[core.stream.frame.flags]
Stream item frames MUST have the `DATA` flag. The final item frame MUST have `DATA | EOS`. An empty stream MAY be represented by an EOS-only frame (no `DATA` flag).

r[core.stream.frame.method-id-zero]
The `method_id` field MUST be 0 for all STREAM channel frames.

r[core.stream.frame.payload]
The payload MUST be a Postcard-encoded item of the stream's declared type `T`.

### Empty Streams

r[core.stream.empty]
An empty stream (zero items) is represented by a single frame with `EOS` flag and `payload_len = 0`. The `DATA` flag MAY be omitted for empty streams.

### Type Enforcement

r[core.stream.type-enforcement]
The receiver MUST determine the expected item type `T` from:
1. The method signature (identified by `method_id` on the parent CALL channel)
2. The port binding (identified by `port_id` in the `AttachTo` attachment)

r[core.stream.decode-failure]
If payload decoding fails (type mismatch, malformed data):
- The receiver MUST send `CancelChannel` for the stream channel with reason `ProtocolViolation`
- The parent call MUST fail with an appropriate error status

### Ordering

r[core.stream.ordering]
- `OpenChannel` for required ports MUST be sent before or concurrently with the root request message
- Receiver MUST accept either order (network reordering happens)
- If request arrives but required port not yet opened, receiver waits until port opens or deadline hits

### Bidirectional Streams

r[core.stream.bidir]
For `direction = BIDIR`: both sides MAY send items on the same channel. Each side MUST send `EOS` independently (half-close semantics). The channel is fully closed when both sides have sent `EOS`.

## TUNNEL Channels

r[core.tunnel.intro]
A TUNNEL channel MUST carry raw bytes (TCP-like stream) and MUST be attached to a parent CALL channel.

### Semantics

r[core.tunnel.semantics]
TUNNEL channels use the same attachment model as STREAM (port_id, OpenChannel, etc.). The `method_id` field MUST be 0 for all TUNNEL channel frames. The `EOS` flag indicates half-close (exactly like TCP FIN).

### Raw Bytes (Not Postcard)

r[core.tunnel.raw-bytes]
TUNNEL payloads MUST be uninterpreted raw bytes, NOT Postcard-encoded. This is the only exception to Postcard payload encoding in Rapace.

r[core.tunnel.frame-boundaries]
Frame boundaries in TUNNEL channels are transport artifacts, not message boundaries. Implementations MUST reassemble frames into a continuous byte stream for the application. Applications MUST treat the byte stream as boundary-agnostic and implement their own message framing if needed. Payload size per frame is still bounded by the negotiated `max_payload_size`.

### Ordering and Reliability

r[core.tunnel.ordering]
TUNNEL frames MUST be ordered within the channel.

r[core.tunnel.reliability]
TUNNEL channels MUST use reliable, ordered delivery (e.g., WebTransport streams or equivalent). Lost or reordered frames would corrupt the byte stream.

### Flow Control

r[core.tunnel.credits]
Credits for TUNNEL channels MUST count raw payload bytes (`payload_len`):
- A frame with `payload_len = 1000` consumes 1000 credits
- EOS-only frames (no payload) consume 0 credits

### Use Cases

- Proxying TCP connections
- File transfer (when you don't want item framing overhead)
- Embedding existing protocols (HTTP/1.1, database wire protocols, etc.)

## Half-Close and Termination

### EOS (End of Stream)

The `EOS` flag means: "I will not send more DATA on this channel, but I can still receive."

r[core.eos.after-send]
The `EOS` frame MUST be the final DATA frame sent on that channel in that direction. The receiver continues processing until it also sends `EOS`.

### Channel States

For each direction (send/receive) on a channel:

| State | Meaning |
|-------|---------|
| `OPEN` | Can send/receive DATA |
| `EOS_SENT` | Sent EOS, waiting for peer's EOS |
| `EOS_RECEIVED` | Received peer's EOS |
| `CLOSED` | Both directions have EOS (or canceled) |

### Full Close

r[core.close.full]
A channel MUST be considered fully closed when:

- Both sides have sent `EOS`, OR
- `CancelChannel` was sent/received

r[core.close.state-free]
After full close, implementations MAY free channel state. Each channel ID MUST be used at most once within the lifetime of a connection (see r[core.channel.id.no-reuse]).

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

r[core.close.close-channel-semantics]
`CloseChannel` MUST be sent on channel 0 and has the following semantics:
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
    ClientCancel = 1,
    DeadlineExceeded = 2,
    ResourceExhausted = 3,
    ProtocolViolation = 4,
    Unauthenticated = 5,
    PermissionDenied = 6,
}
```

r[core.cancel.behavior]
Upon receiving `CancelChannel`, implementations MUST stop work immediately. They MUST discard pending data. They MUST wake waiters with an error.

r[core.cancel.idempotent]
`CancelChannel` MUST be idempotent (multiple cancels for the same channel are harmless).

r[core.cancel.propagation]
When a CALL channel is canceled, implementations MUST also cancel all attached stream/tunnel channels. When an attached channel is canceled, only that port is affected; the parent call MAY still complete if the port is optional.

## Call Completion

r[core.call.complete]
A call MUST be considered **complete** when all of the following are true:

1. Client has sent request `DATA | EOS` on the CALL channel
2. Server has sent response `DATA | EOS | RESPONSE` on the CALL channel
3. All **required** attached ports have reached terminal state:
   - For client→server required streams: server received `EOS` (or cancel)
   - For server→client required streams: client received `EOS` (or cancel)

### Optional Ports

r[core.call.optional-ports]
Optional ports MUST be handled as follows:
- If a stream port is optional (`Option<Stream<T>>`) and not used, no channel is opened
- The port_id field in the payload is `None`
- Call MAY complete without that port reaching terminal state

### Required Ports Not Opened

r[core.call.required-port-missing]
If a required port is never opened before deadline, the server MUST respond with a `FAILED_PRECONDITION` error status.

## Control Channel (Channel 0)

r[core.control.reserved]
Channel 0 MUST be reserved for control messages.

r[core.control.flag-set]
The `CONTROL` flag MUST be set on all frames with `channel_id == 0`.

r[core.control.flag-clear]
The `CONTROL` flag MUST be set only on channel 0. This redundancy allows fast filtering without inspecting `channel_id`.

r[core.control.verb-selector]
Control frames MUST use the `method_id` field as a verb selector from the table below.

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

r[core.control.payload-encoding]
Control payloads MUST be Postcard-encoded like regular messages.

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

r[core.goaway.last-channel-id]
The `last_channel_id` field MUST represent the highest channel ID **opened by the peer** (not by the sender) that the sender will still process. The sender MUST complete work on channels with `channel_id <= last_channel_id` that were opened by the peer.

r[core.goaway.after-send]
After sending `GoAway`, the sender MUST:
- Continue processing existing calls on channels opened by peer with `channel_id <= last_channel_id`
- Reject new `OpenChannel` from peer with `channel_id > last_channel_id` using `CancelChannel { reason: ResourceExhausted }`
- Refrain from opening new channels itself
- Close the connection after a grace period (recommended: 30 seconds or until all accepted channels complete)

See [Overload & Draining](@/spec/overload.md) for detailed shutdown semantics.

### Unknown Control Verbs

When a peer receives a control message with an unknown `method_id`:

r[core.control.unknown-reserved]
**Reserved range (0-99)**:
- The receiver MUST send `GoAway { reason: ProtocolError, message: "unknown control verb", last_channel_id: <current_max> }`
- The receiver MUST close the connection immediately after sending GoAway (no grace period for draining)
- This indicates a protocol version mismatch or buggy peer; continued operation is unsafe

r[core.control.unknown-extension]
**Extension range (100+)**:
- The receiver MUST ignore the message silently (no response)
- The connection continues normally
- This allows future extensions without breaking older peers

### Ping/Pong

r[core.ping.semantics]
Upon receiving a `Ping` control message, the receiver MUST respond with a `Pong` containing the same payload. The timing of ping initiation is implementation-defined (not mandatory keepalive).

Some transports (WebSocket) have their own ping/pong. Rapace-level ping provides application-level liveness checking across intermediaries that might not forward transport-level pings.

## Method ID Computation

r[core.method-id.intro]
Method IDs MUST be 32-bit identifiers computed as a hash of the fully-qualified method name using the algorithm specified below.

### Hash Algorithm

r[core.method-id.algorithm]
Implementations MUST use FNV-1a (Fowler-Noll-Vo) hash, folded to 32 bits:

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

r[core.method-id.input-format]
The hash input MUST be `"{ServiceName}.{MethodName}"` where `ServiceName` is the unqualified service trait name (e.g., `"Calculator"`) and `MethodName` is the method name (e.g., `"add"`).

Example: `"Calculator.add"` → method_id

### Reserved Method IDs

r[core.method-id.zero-reserved]
The value `method_id = 0` MUST be reserved for control messages (on channel 0) and for STREAM/TUNNEL channel frames (which are not method calls).

r[core.method-id.zero-enforcement]
**Enforcement**:
- Code generators MUST check if `compute_method_id(service, method)` returns 0
- If it does, code generation MUST fail with an error instructing the developer to rename the method
- Handshake MUST reject any method registry entry with `method_id = 0`

This reservation ensures unambiguous routing: `method_id = 0` always means "not an RPC method."

### Collision Handling

r[core.method-id.collision-detection]
Method ID collisions (different methods hashing to the same ID) MUST be detected at build time. If a collision is detected:
- Code generation MUST fail with an error
- The developer must rename one of the conflicting methods

r[core.method-id.unknown-method]
At runtime, if a peer receives a method_id it doesn't recognize, it MUST respond with error code `UNIMPLEMENTED`.

## Flow Control

r[core.flow.intro]
Rapace MUST use credit-based flow control per channel.

### Semantics

r[core.flow.credit-semantics]
Every channel has an inbound credit window (bytes the peer may send). The sender MUST limit payload bytes to at most the granted credit. The receiver grants credits via `GrantCredits` control message OR the `credit_grant` field in `MsgDescHot` (fast path, with `CREDITS` flag).

### Credit Grant

```rust
GrantCredits {
    channel_id: u32,
    bytes: u32,
}
```

r[core.flow.credit-additive]
Credits MUST be additive: if you grant 1000, then grant 500, the peer has 1500 bytes available.

### Default Mode

r[core.flow.infinite-credit]
Implementations MAY run in "infinite credit" mode:

- Grant a very large initial window (e.g., `u32::MAX`)
- Periodically top up as needed
- Protocol semantics remain the same

This allows simple implementations while preserving the ability to add real backpressure later.

### Credit Overrun (Protocol Error)

r[core.flow.credit-overrun]
If a receiver sees a frame whose `payload_len` exceeds the remaining credits for that channel, this MUST be treated as a protocol error:

1. Receiver SHOULD send `GoAway { reason: ProtocolError, message: "credit overrun" }`
2. Receiver MUST close the transport connection
3. Receiver MUST discard any frames that arrive after initiating close

Credit overrun is a serious violation because it indicates a buggy or malicious peer.

### EOS Frames and Credits

r[core.flow.eos-no-credits]
Frames with only the `EOS` flag (no `DATA` flag, `payload_len = 0`) MUST be exempt from credit accounting. This ensures half-close can always be signaled regardless of credit state.

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

r[core.flags.reserved]
**Reserved flags**: Flags marked "Reserved" (prefixed with `_RESERVED`) are allocated but not yet defined. Implementations MUST leave reserved flags clear (unset). Receivers MUST ignore unknown flags unless the feature is negotiated as "must-understand" in handshake.

r[core.flags.high-priority]
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
