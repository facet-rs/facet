+++
title = "Compliance & Testing"
description = "Conformance requirements and test suite"
weight = 100
+++

This document defines compliance requirements for Rapace implementations and describes the conformance test suite.

## Compliance Levels

Rapace implementations are classified into three compliance levels:

| Level | Requirements | Use Case |
|-------|--------------|----------|
| **Core** | Basic protocol, single transport | Embedded, minimal clients |
| **Standard** | Core + handshake + flow control | General purpose |
| **Full** | Standard + all transports + streaming | Production systems |

### Core Compliance

Minimum requirements:

- [ ] Parse and emit `MsgDescHot` descriptors
- [ ] Postcard encoding/decoding of payloads (CALL and STREAM channels)
- [ ] Hello handshake exchange (explicit handshake is mandatory)
- [ ] CALL channel lifecycle (request → response)
- [ ] Control channel (channel 0) message parsing
- [ ] At least one transport (typically TCP or WebSocket)
- [ ] Error response handling

**Note**: TUNNEL channel payloads are raw bytes, not Postcard-encoded. See [Core Protocol: TUNNEL Channels](@/spec/core.md#tunnel-channels).

### Standard Compliance

Core requirements plus:

- [ ] Feature negotiation
- [ ] Credit-based flow control
- [ ] OpenChannel/CloseChannel/CancelChannel handling
- [ ] Ping/Pong liveness
- [ ] GoAway graceful shutdown
- [ ] Deadline propagation and enforcement
- [ ] Metadata handling (at least `rapace.priority`, `rapace.deadline*`)

### Full Compliance

Standard requirements plus:

- [ ] All transport bindings (TCP, WebSocket, WebTransport, SHM)
- [ ] STREAM channels with attachment
- [ ] TUNNEL channels
- [ ] Bidirectional streaming
- [ ] Method registry and signature hash validation
- [ ] All standard metadata keys
- [ ] Priority-based scheduling
- [ ] Comprehensive observability hooks

## Test Categories

### Wire Format Tests

Verify correct encoding/decoding of protocol elements.

#### Descriptor Tests

| Test ID | Description |
|---------|-------------|
| `wire.desc.01` | MsgDescHot round-trip (all fields) |
| `wire.desc.02` | Inline payload (≤16 bytes) |
| `wire.desc.03` | External payload reference |
| `wire.desc.04` | All flag combinations |
| `wire.desc.05` | Channel ID edge cases (0, max) |
| `wire.desc.06` | Method ID edge cases |
| `wire.desc.07` | Deadline encoding (nanoseconds) |

#### Payload Tests

| Test ID | Description |
|---------|-------------|
| `wire.payload.01` | Empty payload |
| `wire.payload.02` | All scalar types round-trip |
| `wire.payload.03` | Nested structs |
| `wire.payload.04` | Enums (unit, newtype, struct variants) |
| `wire.payload.05` | Option (None/Some) |
| `wire.payload.06` | Vec (empty, small, large) |
| `wire.payload.07` | HashMap (including non-deterministic order) |
| `wire.payload.08` | Maximum payload size |
| `wire.payload.09` | Malformed payload rejection |
| `wire.payload.10` | NaN canonicalization |

#### Varint Tests

| Test ID | Description |
|---------|-------------|
| `wire.varint.01` | Single byte values (0-127) |
| `wire.varint.02` | Two byte values |
| `wire.varint.03` | Maximum varint (10 bytes) |
| `wire.varint.04` | Overflow rejection (>10 bytes) |
| `wire.varint.05` | Non-canonical encoding rejection |

### Transport Tests

Verify correct behavior on each transport.

#### TCP Transport

| Test ID | Description |
|---------|-------------|
| `transport.tcp.01` | Connection establishment |
| `transport.tcp.02` | Frame round-trip |
| `transport.tcp.03` | Length prefix validation |
| `transport.tcp.04` | Oversized frame rejection |
| `transport.tcp.05` | Graceful close |
| `transport.tcp.06` | Connection drop handling |
| `transport.tcp.07` | Concurrent send/recv |
| `transport.tcp.08` | Backpressure |

#### WebSocket Transport

| Test ID | Description |
|---------|-------------|
| `transport.ws.01` | Connection upgrade |
| `transport.ws.02` | Binary message framing |
| `transport.ws.03` | Large message handling |
| `transport.ws.04` | WebSocket ping/pong |
| `transport.ws.05` | Close frame handling |
| `transport.ws.06` | Text message rejection |

#### WebTransport

| Test ID | Description |
|---------|-------------|
| `transport.wt.01` | QUIC connection establishment |
| `transport.wt.02` | Bidirectional stream creation |
| `transport.wt.03` | Single-stream mode |
| `transport.wt.04` | Multi-stream mode (if supported) |
| `transport.wt.05` | Datagram support (if supported) |
| `transport.wt.06` | Connection migration |
| `transport.wt.07` | 0-RTT reconnection |

#### SHM Transport

| Test ID | Description |
|---------|-------------|
| `transport.shm.01` | Segment creation and mapping |
| `transport.shm.02` | Peer handshake |
| `transport.shm.03` | Inline payload (≤16 bytes) |
| `transport.shm.04` | Slot allocation/free |
| `transport.shm.05` | Generation counter |
| `transport.shm.06` | Futex signaling |
| `transport.shm.07` | Slot exhaustion handling |
| `transport.shm.08` | Zero-copy receive |
| `transport.shm.09` | Peer crash detection |

### Protocol Tests

Verify correct protocol behavior.

#### Handshake Tests

| Test ID | Description |
|---------|-------------|
| `proto.handshake.01` | Hello exchange |
| `proto.handshake.02` | Version negotiation |
| `proto.handshake.03` | Required feature validation |
| `proto.handshake.04` | Optional feature intersection |
| `proto.handshake.05` | Limit negotiation |
| `proto.handshake.06` | Method registry exchange |
| `proto.handshake.07` | Signature hash validation |
| `proto.handshake.08` | Handshake timeout |
| `proto.handshake.09` | Invalid Hello rejection |

#### Channel Tests

| Test ID | Description |
|---------|-------------|
| `proto.channel.01` | CALL channel lifecycle |
| `proto.channel.02` | STREAM channel lifecycle |
| `proto.channel.03` | TUNNEL channel lifecycle |
| `proto.channel.04` | Channel ID allocation (odd/even) |
| `proto.channel.05` | OpenChannel validation |
| `proto.channel.06` | CloseChannel handling |
| `proto.channel.07` | CancelChannel handling |
| `proto.channel.08` | Attached channel binding |
| `proto.channel.09` | Required port validation |
| `proto.channel.10` | Optional port handling |

#### Flow Control Tests

| Test ID | Description |
|---------|-------------|
| `proto.flow.01` | Initial credit grant |
| `proto.flow.02` | GrantCredits message |
| `proto.flow.03` | Credit exhaustion blocking |
| `proto.flow.04` | Credit stall timeout |
| `proto.flow.05` | Per-channel credits |

#### Control Message Tests

| Test ID | Description |
|---------|-------------|
| `proto.control.01` | Ping/Pong round-trip |
| `proto.control.02` | GoAway handling |
| `proto.control.03` | Unknown control verb (reserved range) |
| `proto.control.04` | Unknown control verb (extension range) |

### RPC Tests

Verify correct RPC semantics.

#### Call Tests

| Test ID | Description |
|---------|-------------|
| `rpc.call.01` | Simple unary call |
| `rpc.call.02` | Call with multiple arguments |
| `rpc.call.03` | Call with complex types |
| `rpc.call.04` | Error response |
| `rpc.call.05` | Unknown method (UNIMPLEMENTED) |
| `rpc.call.06` | CallResult envelope parsing |
| `rpc.call.07` | Trailers extraction |

#### Streaming Tests

| Test ID | Description |
|---------|-------------|
| `rpc.stream.01` | Server streaming (basic) |
| `rpc.stream.02` | Server streaming (many items) |
| `rpc.stream.03` | Server streaming (empty) |
| `rpc.stream.04` | Server streaming (error mid-stream) |
| `rpc.stream.05` | Client streaming |
| `rpc.stream.06` | Bidirectional streaming |
| `rpc.stream.07` | Stream cancellation |
| `rpc.stream.08` | Stream backpressure |

#### Cancellation Tests

| Test ID | Description |
|---------|-------------|
| `rpc.cancel.01` | Client cancellation before response |
| `rpc.cancel.02` | Client cancellation during streaming |
| `rpc.cancel.03` | Server cancellation |
| `rpc.cancel.04` | Deadline exceeded |
| `rpc.cancel.05` | Cascading cancellation |

### Interoperability Tests

Verify cross-implementation compatibility.

| Test ID | Description |
|---------|-------------|
| `interop.01` | Rust client → Rust server |
| `interop.02` | TypeScript client → Rust server |
| `interop.03` | Swift client → Rust server |
| `interop.04` | Go client → Rust server (when available) |
| `interop.05` | Java client → Rust server (when available) |
| `interop.06` | Cross-transport (e.g., WS client → TCP server via proxy) |
| `interop.07` | Version compatibility (old client, new server) |

### Stress Tests

Verify behavior under load.

| Test ID | Description |
|---------|-------------|
| `stress.01` | High throughput (requests/sec) |
| `stress.02` | High concurrency (parallel calls) |
| `stress.03` | Large payloads (near max size) |
| `stress.04` | Many channels (near max) |
| `stress.05` | Memory stability (no leaks under load) |
| `stress.06` | Connection churn (rapid connect/disconnect) |
| `stress.07` | Overload behavior (graceful degradation) |

### Security Tests

Verify security properties.

| Test ID | Description |
|---------|-------------|
| `security.01` | Oversized varint rejection |
| `security.02` | Oversized frame rejection |
| `security.03` | Malformed payload rejection |
| `security.04` | Resource exhaustion limits |
| `security.05` | Invalid channel ID handling |
| `security.06` | Invalid method ID handling |

## Test Vectors

### Descriptor Test Vector

```
# MsgDescHot for a simple CALL request
# All values little-endian

00000000: 01 00 00 00 00 00 00 00  # msg_id = 1
00000008: 01 00 00 00              # channel_id = 1
0000000c: 78 56 34 12              # method_id = 0x12345678
00000010: ff ff ff ff              # payload_slot = INLINE
00000014: 00 00 00 00              # payload_generation = 0
00000018: 00 00 00 00              # payload_offset = 0
0000001c: 05 00 00 00              # payload_len = 5
00000020: 05 00 00 00              # flags = DATA | EOS
00000024: 00 00 00 00              # credit_grant = 0
00000028: 00 00 00 00 00 00 00 00  # deadline_ns = 0
00000030: 48 65 6c 6c 6f 00 00 00  # inline_payload = "Hello"
00000038: 00 00 00 00 00 00 00 00  # (padding)
```

### Payload Test Vectors

All integers (except u8/i8) use LEB128 varint encoding per [Payload Encoding](@/spec/payload-encoding.md).

```
# Empty struct
00

# Simple struct { a: u32, b: String }
# a = 42 (varint), b = "hello" (length-prefixed)
2a 05 68 65 6c 6c 6f

# Option<u32> = None
00

# Option<u32> = Some(42)
# 0x01 = Some, 0x2a = 42 as varint
01 2a

# Vec<u8> = [1, 2, 3]
# 0x03 = length, then raw bytes (u8 is not varint)
03 01 02 03

# Enum variant 0 (unit)
00

# Enum variant 1 with u32 payload = 42
# 0x01 = variant, 0x2a = 42 as varint
01 2a

# u32 = 300 (requires 2-byte varint)
# 300 = 0x12c = 0b100101100 → LEB128: 0xac 0x02
ac 02

# i32 = -1 (zigzag encoding)
# -1 → zigzag → 1 → varint 0x01
01

# i32 = -64 (zigzag encoding)
# -64 → zigzag → 127 → varint 0x7f
7f
```

### Handshake Test Vector

```
# Hello message (minimal)
{
    "protocol_version": 65536,  # v1.0
    "role": 1,                  # INITIATOR
    "required_features": 3,     # ATTACHED_STREAMS | CALL_ENVELOPE
    "supported_features": 15,   # All features
    "limits": {
        "max_payload_size": 16777216,
        "max_channels": 65536,
        "max_pending_calls": 1024
    },
    "methods": [],
    "params": []
}
```

## Conformance Test Harness

### Test Runner Interface

Implementations provide a test harness:

```rust
pub trait TestHarness {
    /// Create a server listening on the given address.
    async fn start_server(&self, addr: &str) -> Result<ServerHandle, Error>;
    
    /// Create a client connected to the given address.
    async fn create_client(&self, addr: &str) -> Result<ClientHandle, Error>;
    
    /// Get implementation metadata.
    fn implementation_info(&self) -> ImplementationInfo;
}

pub struct ImplementationInfo {
    pub name: String,
    pub version: String,
    pub language: String,
    pub compliance_level: ComplianceLevel,
    pub supported_transports: Vec<String>,
}
```

### Running Tests

```bash
# Run all tests for an implementation
rapace-conformance --harness ./my-harness --level full

# Run specific test category
rapace-conformance --harness ./my-harness --category wire

# Run specific test
rapace-conformance --harness ./my-harness --test wire.desc.01

# Generate compliance report
rapace-conformance --harness ./my-harness --report compliance.json
```

### Compliance Report Format

```json
{
  "implementation": {
    "name": "my-rapace",
    "version": "1.0.0",
    "language": "Go",
    "compliance_level": "standard"
  },
  "test_run": {
    "timestamp": "2024-01-15T10:30:00Z",
    "duration_seconds": 42.5
  },
  "results": {
    "passed": 156,
    "failed": 3,
    "skipped": 12
  },
  "tests": [
    {
      "id": "wire.desc.01",
      "status": "passed",
      "duration_ms": 5
    },
    {
      "id": "transport.shm.01",
      "status": "skipped",
      "reason": "SHM transport not implemented"
    },
    {
      "id": "rpc.stream.06",
      "status": "failed",
      "error": "Bidirectional stream closed prematurely"
    }
  ]
}
```

## Certification

### Self-Certification

Implementations can self-certify by:

1. Running the full test suite
2. Generating a compliance report
3. Publishing the report with the implementation

### Official Certification

For official Rapace certification:

1. Submit compliance report to the Rapace project
2. Provide test harness for verification
3. Pass review of edge case handling
4. Receive certification badge for the compliance level

### Certification Badge

```markdown
[![Rapace Certified: Standard](https://rapace.dev/badges/standard.svg)](https://rapace.dev/certified/my-impl)
```

## Continuous Compliance

### CI Integration

```yaml
# GitHub Actions example
name: Rapace Compliance
on: [push, pull_request]

jobs:
  compliance:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: rapace/conformance-action@v1
        with:
          harness: ./test-harness
          level: standard
          report: compliance.json
      - uses: actions/upload-artifact@v4
        with:
          name: compliance-report
          path: compliance.json
```

### Regression Prevention

- Run compliance tests on every commit
- Fail CI if any previously-passing tests fail
- Track compliance coverage over time

## Next Steps

- [Core Protocol](@/spec/core.md) – Protocol requirements
- [Transport Bindings](@/spec/transport-bindings.md) – Transport specifications
- [Schema Evolution](@/spec/schema-evolution.md) – Compatibility requirements
