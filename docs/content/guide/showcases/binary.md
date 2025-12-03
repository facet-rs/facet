+++
title = "Binary Formats"
weight = 5
insert_anchor_links = "heading"
+++

Facet supports several binary serialization formats for compact, efficient data exchange.

## facet-msgpack

[MessagePack](https://msgpack.org/) is a compact binary format that's "like JSON but fast and small."

```rust
use facet::Facet;

#[derive(Facet)]
struct Message {
    id: u64,
    content: String,
    tags: Vec<String>,
}

let msg = Message {
    id: 42,
    content: "Hello, world!".into(),
    tags: vec!["greeting".into()],
};

// Serialize to bytes
let bytes = facet_msgpack::to_vec(&msg)?;

// Deserialize from bytes
let decoded: Message = facet_msgpack::from_slice(&bytes)?;
```

### When to use MessagePack

- **Network protocols**: More compact than JSON, widely supported
- **Cross-language data exchange**: Libraries exist for most languages
- **Caching**: Efficient binary storage for Redis, etc.
- **Logs and metrics**: Smaller than JSON, still human-debuggable with tools

### Features

- Full serialization and deserialization
- Streaming support
- Extension types for dates, bytes, etc.
- Compatible with other MessagePack implementations

## facet-postcard

[Postcard](https://postcard.jamesmunns.com/) is a `no_std`-friendly binary format designed for embedded systems and high-performance applications.

```rust
use facet::Facet;

#[derive(Facet)]
struct SensorReading {
    sensor_id: u16,
    temperature: f32,
    humidity: f32,
}

let reading = SensorReading {
    sensor_id: 1,
    temperature: 23.5,
    humidity: 65.0,
};

// Serialize to bytes
let bytes = facet_postcard::to_vec(&reading)?;

// Deserialize from bytes
let decoded: SensorReading = facet_postcard::from_slice(&bytes)?;
```

### When to use Postcard

- **Embedded systems**: `no_std` compatible, minimal overhead
- **High-performance**: Variable-length encoding, no schema overhead
- **Rust-to-Rust**: Native Rust types, no schema negotiation
- **Fixed-size buffers**: Can serialize directly into `[u8; N]`

### Features

- `no_std` support (with `alloc` feature)
- Variable-length integer encoding (varint)
- Zero-copy deserialization where possible
- Compact encoding for common patterns

### Fixed-size serialization

Postcard can serialize to fixed-size arrays:

```rust
use facet_postcard::to_slice;

let mut buf = [0u8; 64];
let used = to_slice(&reading, &mut buf)?;
// `used` contains the actual bytes written
```

## Comparison

| Feature | MessagePack | Postcard |
|---------|-------------|----------|
| Size | Very compact | Ultra compact |
| Speed | Fast | Very fast |
| `no_std` | No | Yes |
| Cross-language | Excellent | Rust-focused |
| Schema | Self-describing | Schema required |
| Debugging | Tooling available | Hex dump |

### Size comparison (approximate)

For a typical struct with a few fields:

| Format | Size (bytes) |
|--------|--------------|
| JSON | 100-150 |
| MessagePack | 40-60 |
| Postcard | 20-40 |

## Other Binary Formats

### facet-asn1

ASN.1 DER encoding, used in cryptography and X.509 certificates. Serialization only.

### facet-xdr

XDR (External Data Representation), used in NFS and some RPC protocols. Serialization only.

## Next Steps

- See [Format comparison matrix](@/reference/format-crate-matrix/) for feature support
- Check [Ecosystem](@/guide/ecosystem.md) for `no_std` support details
- Read the [postcard crate docs](https://docs.rs/postcard) for advanced usage
