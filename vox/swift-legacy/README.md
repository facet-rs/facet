# rapace-swift

Swift client implementation for the rapace RPC framework.

See also: [`rust/`](../rust/) (new implementation), [`rust-legacy/`](../rust-legacy/) (legacy implementation), [`typescript/`](../typescript/) (TypeScript client)

## Status

🚧 **Work in Progress** - De-risking experiments complete, implementation in progress.

## What is Rapace?

Rapace is a high-performance RPC framework for Rust with:
- Type-safe RPC with compile-time method ID hashing (FNV-1a)
- Zero-copy deserialization with facet-format-postcard
- Streaming support (server and client)
- Multiple transports (TCP, Unix sockets, WebSocket, shared memory)

## Project Goals

1. **Postcard serialization in Swift** - Binary format compatible with Rust
2. **Swift code generator** - Generate Swift clients from rapace proto crates
3. **Async TCP client** - Modern Swift concurrency with actors

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    Swift Client                          │
├─────────────────────────────────────────────────────────┤
│  Generated Client Stubs (BrowserDemoClient, etc.)       │
├─────────────────────────────────────────────────────────┤
│  RapaceClient (frame send/recv, channel demux)          │
├─────────────────────────────────────────────────────────┤
│  TCPConnection (async/await, actor-based)               │
├─────────────────────────────────────────────────────────┤
│  Postcard (varint, zigzag, struct serialization)        │
└─────────────────────────────────────────────────────────┘
                          │
                          │ TCP
                          ▼
┌─────────────────────────────────────────────────────────┐
│                    Rust Server                           │
│         (rapace with any transport backend)              │
└─────────────────────────────────────────────────────────┘
```

## Code Generation Strategy

```
xxx-proto (Rust)              xxx-swift-gen (Rust binary)
    │                                │
    │ defines types +                │ uses facet reflection
    │ service traits                 │ to introspect types
    │                                │
    └────────────┬───────────────────┘
                 │
                 ▼
           xxx.swift (generated)
                 │
                 ▼
           Swift client library
```

The codegen binary:
1. Depends on the proto crate
2. Uses `ServiceRegistry` to access service/method metadata
3. Uses facet's `Shape` API to introspect request/response types
4. Outputs Swift source code

## Building

### Swift

```bash
swift build
```

### Rust Test Harnesses

```bash
# Varint test vectors
cd test-harness/varint-test && cargo run

# MsgDescHot binary layout
cd test-harness/descriptor-test && cargo run -- hex

# Codegen proof-of-concept
cd test-harness/codegen-poc && cargo run
```

## Wire Protocol

### Frame Format (TCP)

```
[4 bytes: frame_len (u32 LE)]
[64 bytes: MsgDescHot]
[0..N bytes: payload]
```

### MsgDescHot (64 bytes)

| Offset | Size | Field |
|--------|------|-------|
| 0 | 8 | msg_id |
| 8 | 4 | channel_id |
| 12 | 4 | method_id |
| 16 | 4 | payload_slot |
| 20 | 4 | payload_generation |
| 24 | 4 | payload_offset |
| 28 | 4 | payload_len |
| 32 | 4 | flags |
| 36 | 4 | credit_grant |
| 40 | 8 | deadline_ns |
| 48 | 16 | inline_payload |

### Postcard Format

| Type | Encoding |
|------|----------|
| `bool` | 1 byte (0x00 or 0x01) |
| `u8`/`i8` | 1 byte raw |
| `u16`–`u64` | Varint (LEB128) |
| `i16`–`i64` | Zigzag + Varint |
| `f32`/`f64` | Little-endian IEEE 754 |
| `String` | Varint length + UTF-8 |
| `Vec<T>` | Varint length + elements |
| `Option<T>` | 0x00 (None) or 0x01 + value |
| `struct` | Fields in order, no delimiters |
| `enum` | Varint discriminant + payload |

## License

Same as rapace - see [rapace repository](https://github.com/bearcove/rapace) for details.
