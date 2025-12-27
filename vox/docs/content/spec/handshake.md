+++
title = "Handshake & Capabilities"
description = "Connection establishment and feature negotiation"
weight = 50
+++

This document defines the Rapace handshake protocol: how connections are established, capabilities negotiated, and method registries exchanged.

## Overview

When a transport connection is established, both peers MUST exchange a `Hello` message before any other communication. The handshake:

1. **Validates compatibility**: Protocol version, required features
2. **Negotiates capabilities**: What optional features both peers support
3. **Exchanges limits**: Maximum payload sizes, channel counts
4. **Registers methods**: Method IDs and signature hashes for compatibility checking

## Handshake Flow

```
Initiator                                      Acceptor
    │                                              │
    │  [transport connection established]          │
    │                                              │
    │  HELLO (role=INITIATOR, ...)                 │
    ├─────────────────────────────────────────────▶│
    │                                              │
    │                    HELLO (role=ACCEPTOR, ...)│
    │◀─────────────────────────────────────────────┤
    │                                              │
    │  [validate & negotiate]                      │
    │                                              │
    │  [connection ready for RPC]                  │
    │                                              │
```

## Hello Message

The `Hello` message is sent on channel 0 as a control message with `method_id = 0` (reserved for handshake).

### Wire Format

```rust
Hello {
    protocol_version: u32,      // Protocol version (see below)
    role: Role,                 // INITIATOR or ACCEPTOR
    required_features: u64,     // Features the peer MUST support
    supported_features: u64,    // Features the peer supports
    limits: Limits,             // Advertised limits
    methods: Vec<MethodInfo>,   // Method registry
    params: Vec<(String, Vec<u8>)>,  // Extension parameters
}

enum Role {
    Initiator = 1,
    Acceptor = 2,
}

struct Limits {
    max_payload_size: u32,      // Maximum payload bytes per frame
    max_channels: u32,          // Maximum concurrent channels (0 = unlimited)
    max_pending_calls: u32,     // Maximum pending RPC calls (0 = unlimited)
}

struct MethodInfo {
    method_id: u32,             // Method identifier (see Core Protocol)
    sig_hash: [u8; 32],         // Structural signature hash (BLAKE3)
    name: Option<String>,       // Human-readable "Service.method" (for debugging)
}
```

### Frame Structure

```
channel_id = 0
method_id = 0 (HELLO verb)
flags = CONTROL
payload = postcard-encoded Hello
```

## Protocol Version

The `protocol_version` field uses semantic versioning packed into a u32:

```
protocol_version = (major << 16) | minor
```

For this specification: `protocol_version = 0x00010000` (v1.0)

### Compatibility Rules

- **Major version mismatch**: Connection MUST be rejected
- **Minor version mismatch**: Connection proceeds; peers use the lower minor version's feature set

## Role Validation

Both peers declare their role explicitly:

- **Initiator**: The peer that opened the transport connection (client)
- **Acceptor**: The peer that accepted the transport connection (server)

Roles determine channel ID allocation:
- Initiator uses odd channel IDs (1, 3, 5, ...)
- Acceptor uses even channel IDs (2, 4, 6, ...)

If both peers claim the same role, or roles don't match the transport semantics (e.g., the accepting side claims INITIATOR), the connection MUST be rejected with a protocol error.

## Feature Negotiation

Features are represented as bits in a 64-bit field. There are two categories:

### Required Features (`required_features`)

Features the sender requires the peer to support. If `(required_features & peer.supported_features) != required_features`, the connection MUST be rejected.

### Supported Features (`supported_features`)

Features the sender supports but doesn't require. The effective feature set for the connection is:

```
effective_features = my.supported_features & peer.supported_features
```

### Defined Feature Bits

| Bit | Name | Description |
|-----|------|-------------|
| 0 | `ATTACHED_STREAMS` | Supports STREAM/TUNNEL channels attached to calls |
| 1 | `CALL_ENVELOPE` | Uses CallResult envelope with status + trailers |
| 2 | `CREDIT_FLOW_CONTROL` | Enforces credit-based flow control |
| 3 | `RAPACE_PING` | Supports Rapace-level Ping/Pong |
| 4 | `WEBTRANSPORT_MULTI_STREAM` | WebTransport: map Rapace channels to QUIC streams |
| 5 | `WEBTRANSPORT_DATAGRAMS` | WebTransport: support unreliable datagrams |
| 6-63 | Reserved | Reserved for future use |

**WebTransport features**: These bits are only relevant for WebTransport connections. When `WEBTRANSPORT_MULTI_STREAM` is negotiated, each Rapace channel maps to a separate QUIC stream (see [Transport Bindings: Multi-Stream Mode](@/spec/transport-bindings.md#multi-stream-mode-advanced)). When `WEBTRANSPORT_DATAGRAMS` is negotiated, STREAM channels may be marked as `unreliable` via the `rapace.unreliable` metadata key on `OpenChannel`.

For protocol v1.0, bits 0-1 (`ATTACHED_STREAMS`, `CALL_ENVELOPE`) SHOULD be set as required by conforming implementations.

### Feature Negotiation Example

```
Initiator:
  required_features  = 0b0011  (ATTACHED_STREAMS, CALL_ENVELOPE)
  supported_features = 0b1111  (all four features)

Acceptor:
  required_features  = 0b0001  (ATTACHED_STREAMS only)
  supported_features = 0b0111  (not RAPACE_PING)

Negotiation:
  Initiator requires CALL_ENVELOPE, Acceptor supports it → OK
  Acceptor requires ATTACHED_STREAMS, Initiator supports it → OK
  effective_features = 0b0111 & 0b1111 = 0b0111
  (RAPACE_PING not in effective set)
```

## Limits Negotiation

Both peers advertise their limits. The effective limit is the **minimum** of both:

```
effective_max_payload = min(my.limits.max_payload_size, peer.limits.max_payload_size)
effective_max_channels = min(my.limits.max_channels, peer.limits.max_channels)
```

A value of 0 means "unlimited" (implementation-defined maximum).

### Transport-Specific Limits

Different transports have different inherent limits:

| Transport | Typical max_payload_size |
|-----------|-------------------------|
| Stream (TCP) | Limited by memory, often 1-16 MB |
| WebSocket | Limited by implementation, often 16 MB |
| SHM (Pair) | `slot_size` (default 4KB) |
| SHM (Hub) | 16 MB (largest size class) |

Peers SHOULD advertise realistic limits based on their transport.

## Method Registry

The `methods` array contains information about every method the peer can handle (for acceptors) or intends to call (for initiators).

### MethodInfo Fields

- **method_id**: Method identifier per [Core Protocol: Method ID Computation](@/spec/core.md#method-id-computation). The method_id already encodes "Service.method", so a separate service_id is not needed.
- **sig_hash**: 32-byte BLAKE3 hash of the method signature (arguments + return type)
- **name**: Optional human-readable name for debugging (e.g., `"Calculator.add"`)

### Registry Validation

The method registry MUST be validated during handshake:

1. **No reserved method_id**: If any entry has `method_id = 0`, handshake MUST fail
2. **No duplicates**: If any two entries have the same `method_id`, handshake MUST fail
3. **Cross-service collisions**: Different services with methods that hash to the same `method_id` are collisions and MUST be rejected

These rules apply even if methods come from different services. Runtime dispatch is by `method_id` only; there is no separate service routing layer.

**Failure behavior**: If validation fails, send `CloseChannel { channel_id: 0, reason: Error("duplicate method_id") }` and close the transport.

### Signature Hash

The `sig_hash` is a BLAKE3 hash of the method's structural signature, computed from the [facet](https://facets.rs) `Shape` of argument and return types. See [Schema Evolution](@/spec/schema-evolution.md) for the complete algorithm.

Hash inputs include:
- Field **identifiers** (names) and order
- Field types (recursively)
- Enum variant names, discriminants, and payloads
- Container types (Vec, Option, HashMap, etc.)

Hash excludes:
- Type names (struct/enum names can be renamed freely)
- Module paths
- Documentation

Two methods with the same `sig_hash` are wire-compatible.

**Note**: BLAKE3 is the only supported hash algorithm. This is not negotiable.

### Compatibility Checking

After exchanging registries, each peer can determine:

1. **Compatible methods**: Same `method_id` and same `sig_hash`
2. **Incompatible methods**: Same `method_id` but different `sig_hash`
3. **Unknown methods**: `method_id` present on one side but not the other

For **incompatible methods**, implementations SHOULD:
- Reject calls to that method immediately (before encoding)
- Optionally provide detailed schema diff for debugging

For **unknown methods**, the peer that doesn't have the method will return `UNIMPLEMENTED` at call time.

## Extension Parameters

The `params` field is a key-value map for future extensions:

```rust
params: Vec<(String, Vec<u8>)>
```

Known parameter keys (see [Metadata Conventions: Connection Parameters](@/spec/metadata.md#connection-parameters) for encoding details):
- `rapace.ping_interval_ms`: Suggested ping interval in milliseconds
- `rapace.compression`: Supported compression algorithms
- `rapace.default_priority`: Default priority for calls on this connection

**Default priority**: If `rapace.default_priority` is present, all calls on this connection use that priority level unless overridden by per-call `rapace.priority` metadata or the `HIGH_PRIORITY` frame flag. If absent, the default is 128 (middle of Normal range). See [Prioritization & QoS](@/spec/prioritization.md) for priority semantics.

Unknown parameters MUST be ignored.

## Handshake Timing

### Ordering

1. After transport connection is established, each peer sends `Hello` immediately
2. Peers MUST NOT send any other frames until they have both sent and received `Hello`
3. After successful handshake, the connection is ready for RPC

### Timeout

Implementations SHOULD impose a handshake timeout (e.g., 30 seconds). If `Hello` is not received within the timeout, the connection MUST be closed.

### Failure

If handshake fails (version mismatch, required feature not supported, role conflict, non-Hello first frame), the peer that detects the failure:

1. MAY send a `CloseChannel` on channel 0 with an error reason string
2. MUST close the transport connection immediately after
3. MUST NOT process any further frames

**First frame not Hello**: If the first frame received on a new connection is not a `Hello` (i.e., `channel_id != 0` or `method_id != 0`), this is a handshake failure. The receiver:
- SHOULD send `CloseChannel { channel_id: 0, reason: Error("expected Hello") }` if possible
- MUST close the transport connection
- MUST NOT attempt to process the non-Hello frame

This is a hard requirement for all compliance levels. There is no implicit handshake mode.

## Transport-Specific Considerations

### Shared Memory Transport

For SHM transports, the handshake serves additional purposes:

- Validates segment magic and protocol version (done before Hello)
- Exchanges peer IDs for channel ID allocation
- Negotiates slot sizes and counts (via limits or transport-specific params)

The segment header (`SegmentHeader` or `HubHeader`) contains version information that is validated when mapping the segment. The `Hello` exchange happens after segment mapping succeeds.

### WebSocket Transport

For WebSocket, the HTTP upgrade handshake happens first (at the transport layer). The Rapace `Hello` exchange happens immediately after the WebSocket connection is established.

Rapace-level ping/pong is separate from WebSocket ping/pong frames. Use `RAPACE_PING` feature negotiation to determine which to use.

### Stream Transport (TCP, Unix Socket)

No transport-level handshake. The first bytes on the connection are the length-prefixed Rapace `Hello` frame.

## Security Considerations

The handshake does not provide authentication or encryption. Those concerns are delegated to:

- Transport-level security (TLS, Unix socket permissions)
- Application-level authentication (tokens in `Hello.params` or `OpenChannel.metadata`)

Implementations SHOULD use secure transports in production.

For detailed security requirements and deployment profiles, see [Security Profiles](@/spec/security.md).

## Summary

| Aspect | Rule |
|--------|------|
| **Handshake required** | Yes, explicit `Hello` exchange before any other frames |
| **Protocol version** | Major version must match; minor version uses lower |
| **Role** | Explicit; determines channel ID allocation |
| **Features** | Bitfield; required must be supported; effective = intersection |
| **Limits** | Effective = minimum of both peers |
| **Method registry** | Full exchange; sig_hash for compatibility |
| **Timeout** | Implementation-defined, recommended 30s |

## Next Steps

- [Core Protocol](@/spec/core.md) – Channel lifecycle and control messages
- [Schema Evolution](@/spec/schema-evolution.md) – How signature hashes are computed
- [Error Handling](@/spec/errors.md) – Error codes for handshake failures
