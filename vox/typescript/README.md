# @bearcove/rapace

TypeScript client for the [Rapace](https://github.com/bearcove/rapace) RPC protocol — high-performance binary RPC over WebSocket.

[![npm version](https://img.shields.io/npm/v/@bearcove/rapace.svg)](https://www.npmjs.com/package/@bearcove/rapace)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE-MIT)

## Features

- **WebSocket transport** — Works in both browsers and Node.js (v20+)
- **Postcard serialization** — Compatible with Rust's [postcard](https://github.com/jamesmunns/postcard) format
- **Zero dependencies** — Pure TypeScript, no runtime dependencies
- **Type-safe** — Full TypeScript support with generated client types
- **Code generation** — Generate TypeScript clients from Rust service definitions

## Installation

```bash
npm install @bearcove/rapace
```

## Quick Start

```typescript
import { RapaceClient, PostcardEncoder, PostcardDecoder, computeMethodId } from '@bearcove/rapace';

// Connect to a Rapace server over WebSocket
const client = await RapaceClient.connect('ws://localhost:8080');

// Make an RPC call
const encoder = new PostcardEncoder();
encoder.u64(123n);        // item_id
encoder.u64(0n);          // offset
encoder.u64(1024n);       // length

const methodId = computeMethodId('Vfs', 'read');
const response = await client.call(methodId, encoder.bytes);

// Decode the response
const decoder = new PostcardDecoder(response);
const data = decoder.bytes();      // Uint8Array
const error = decoder.i32();       // error code

// Close when done
client.close();
```

## API Reference

### RapaceClient

The main client for making RPC calls.

```typescript
class RapaceClient {
  // Connect to a Rapace server over WebSocket
  static async connect(url: string): Promise<RapaceClient>;

  // Make a raw RPC call
  async call(methodId: number, requestPayload: Uint8Array): Promise<Uint8Array>;

  // Make a typed RPC call with custom encoding/decoding
  async callTyped<Req, Res>(
    methodId: number,
    request: Req,
    encode: (encoder: PostcardEncoder, req: Req) => void,
    decode: (decoder: PostcardDecoder) => Res
  ): Promise<Res>;

  // Close the connection
  close(): void;

  // Check if the client is closed
  readonly isClosed: boolean;
}
```

### PostcardEncoder

Encoder for the postcard binary format.

```typescript
class PostcardEncoder {
  // Get the encoded bytes
  readonly bytes: Uint8Array;

  // Reset the encoder for reuse
  reset(): void;

  // Primitive types
  bool(value: boolean): this;
  u8(value: number): this;
  i8(value: number): this;
  u16(value: number): this;
  i16(value: number): this;
  u32(value: number): this;
  i32(value: number): this;
  u64(value: bigint | number): this;
  i64(value: bigint | number): this;
  f32(value: number): this;
  f64(value: number): this;

  // Strings and bytes
  string(value: string): this;
  byteArray(value: Uint8Array): this;
  rawBytes(value: Uint8Array): this;

  // Containers
  option<T>(value: T | null, encode: (enc: this, v: T) => void): this;
  array<T>(values: T[], encode: (enc: this, v: T) => void): this;
  stringArray(values: string[]): this;

  // Enums
  enumDiscriminant(discriminant: number): this;
}
```

### PostcardDecoder

Decoder for the postcard binary format.

```typescript
class PostcardDecoder {
  constructor(data: Uint8Array);

  // Primitive types
  bool(): boolean;
  u8(): number;
  i8(): number;
  u16(): number;
  i16(): number;
  u32(): number;
  i32(): number;
  u64(): bigint;
  i64(): bigint;
  f32(): number;
  f64(): number;

  // Strings and bytes
  string(): string;
  bytes(): Uint8Array;
  rawBytes(count: number): Uint8Array;

  // Containers
  option<T>(decode: (dec: this) => T): T | null;
  array<T>(decode: (dec: this) => T): T[];
  stringArray(): string[];

  // Enums
  enumDiscriminant(): number;

  // Position
  readonly remaining: number;
  readonly position: number;
  hasRemaining(): boolean;
}
```

### Method ID Computation

```typescript
// Compute method ID from service and method names
function computeMethodId(service: string, method: string): number;

// Compute from full name (e.g., "Vfs.read")
function computeMethodIdFromFullName(fullName: string): number;
```

Method IDs are computed using 64-bit FNV-1a hash, folded to 32 bits. This matches the Rust implementation exactly.

### Frame Types

For advanced use cases, you can work directly with frames:

```typescript
import { Frame, MsgDescHot, FrameFlags } from '@bearcove/rapace';

// Create a data frame
const frame = Frame.data(msgId, channelId, methodId, payload);

// Check frame properties
frame.desc.isInline;    // payload stored inline (≤16 bytes)
frame.desc.isError;     // error response
frame.desc.isEos;       // end of stream

// Serialize for transmission
const bytes = frame.serialize();

// Parse from bytes
const parsed = Frame.parse(bytes);
```

## Wire Protocol

Rapace uses a binary wire format optimized for performance:

```
┌─────────────────────────────────────────────────────────────┐
│ Frame Format                                                │
├─────────────────────────────────────────────────────────────┤
│ [4 bytes]  Frame length (little-endian u32)                │
│ [64 bytes] MsgDescHot descriptor                           │
│ [N bytes]  Payload (if not inline)                         │
└─────────────────────────────────────────────────────────────┘
```

The 64-byte `MsgDescHot` descriptor is cache-line aligned for performance:

| Offset | Size | Field |
|--------|------|-------|
| 0 | 8 | `msgId` — Unique message ID |
| 8 | 4 | `channelId` — Logical stream ID |
| 12 | 4 | `methodId` — RPC method (FNV-1a hash) |
| 16 | 4 | `payloadSlot` — 0xFFFFFFFF = inline |
| 20 | 4 | `payloadGeneration` — ABA safety |
| 24 | 4 | `payloadOffset` — Offset in slot |
| 28 | 4 | `payloadLen` — Payload length |
| 32 | 4 | `flags` — Frame flags |
| 36 | 4 | `creditGrant` — Flow control |
| 40 | 8 | `deadlineNs` — Deadline (nanoseconds) |
| 48 | 16 | `inlinePayload` — Small payload storage |

Small payloads (≤16 bytes) are stored inline within the descriptor itself.

## Code Generation

For production use, generate TypeScript clients from your Rust service definitions using `rapace-typescript-codegen`:

```rust
// In your codegen binary (Rust)
use rapace_typescript_codegen::TypeScriptCodegen;
use rapace::registry::ServiceRegistry;

fn main() {
    // Register your services
    ServiceRegistry::with_global_mut(|registry| {
        my_proto::vfs_register(registry);
    });

    // Generate TypeScript code
    let mut codegen = TypeScriptCodegen::new();
    ServiceRegistry::with_global(|registry| {
        codegen.generate_from_registry(registry);
    });

    std::fs::write("src/generated/vfs-client.ts", codegen.into_output()).unwrap();
}
```

The generated code includes:

- TypeScript interfaces for all request/response types
- Encoder/decoder functions for each type
- Type-safe client classes with async methods

Example generated client:

```typescript
// Generated from #[rapace::service] trait Vfs { ... }

export interface ReadResult {
  data: Uint8Array;
  error: number;
}

export class VfsClient {
  static async connect(url: string): Promise<VfsClient>;

  async read(itemId: bigint, offset: bigint, len: bigint): Promise<ReadResult>;
  async write(itemId: bigint, offset: bigint, data: Uint8Array): Promise<WriteResult>;
  // ... other methods

  close(): void;
}
```

## Browser Support

This library works in modern browsers with WebSocket support. No polyfills required.

```typescript
// Works in browser
const client = await RapaceClient.connect('wss://api.example.com/rpc');
```

## Node.js Support

Requires Node.js 20+ (for native WebSocket support) or Node.js 18+ with a WebSocket polyfill.

```typescript
// Works in Node.js 20+
const client = await RapaceClient.connect('ws://localhost:8080');
```

## Related

- [`rust/`](../rust/) — Rust implementation (core framework)
- [`swift/`](../swift/) — Swift client implementation
- [facet](https://github.com/facet-rs/facet) — Rust reflection library used by rapace

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.
