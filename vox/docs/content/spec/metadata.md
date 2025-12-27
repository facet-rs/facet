+++
title = "Metadata Conventions"
description = "Standard metadata keys for auth, tracing, and priority"
weight = 65
+++

This document defines standard metadata keys used in Rapace protocol messages. Metadata enables extensibility without protocol changes.

## Metadata Locations

Metadata appears in three places in the protocol:

| Location | Wire Type | Purpose |
|----------|-----------|---------|
| `Hello.params` | `Vec<(String, Vec<u8>)>` | Connection-level parameters |
| `OpenChannel.metadata` | `Vec<(String, Vec<u8>)>` | Per-call request metadata (headers) |
| `CallResult.trailers` | `Vec<(String, Vec<u8>)>` | Per-call response metadata (trailers) |

## Key Naming Conventions

### Reserved Prefixes

Keys starting with `rapace.` are reserved for protocol-defined metadata. Applications MUST NOT define custom keys with this prefix.

| Prefix | Owner |
|--------|-------|
| `rapace.` | Rapace protocol specification |
| `x-` | Application-defined (not standardized) |
| (no prefix) | Application-defined |

### Key Format

Keys MUST:
- Be valid UTF-8 strings
- Be at most 256 bytes
- Contain only printable ASCII characters (0x21-0x7E), excluding `=` and whitespace
- Not start with a digit

Standard keys (prefixed `rapace.`) use only lowercase letters, digits, hyphens, underscores, and dots.

Recommended format for custom keys: `namespace.key_name` (e.g., `myapp.request_id`)

### Case Sensitivity

Keys are **case-sensitive**. `Trace-Id` and `trace-id` are different keys.

Implementations SHOULD use lowercase keys consistently for interoperability. Receivers MUST NOT normalize case (treat keys as opaque bytes).

### Duplicate Keys

If the same key appears multiple times in a metadata list:

- **Receivers MUST use the first occurrence** (first-wins semantics)
- Subsequent occurrences of the same key SHOULD be ignored
- Senders SHOULD NOT include duplicate keys

This rule applies to `Hello.params`, `OpenChannel.metadata`, and `CallResult.trailers`.

**Rationale**: First-wins is simple to implement and matches common key-value semantics. It also prevents injection attacks where a malicious intermediary appends keys to override earlier values.

## Value Encoding

Values are arbitrary byte sequences (`Vec<u8>`). The interpretation depends on the key:

| Value Type | Encoding |
|------------|----------|
| UTF-8 string | Raw UTF-8 bytes |
| Integer | Little-endian bytes (4 or 8 bytes) |
| Binary | Raw bytes |
| Structured | Postcard-encoded struct |

Standard keys (below) document their encoding explicitly.

## Standard Keys

### Distributed Tracing

These keys provide OpenTelemetry-compatible distributed tracing context.

#### `rapace.trace_id`

- **Location**: `OpenChannel.metadata`
- **Type**: 16 bytes (binary)
- **Purpose**: Unique identifier for an entire trace across services
- **Format**: 128-bit identifier, typically hex-encoded in logs
- **Propagation**: MUST be propagated to all downstream calls

#### `rapace.span_id`

- **Location**: `OpenChannel.metadata`
- **Type**: 8 bytes (binary)
- **Purpose**: Identifier for this specific span within a trace
- **Format**: 64-bit identifier
- **Propagation**: Each service creates its own span_id; parent span_id is `rapace.parent_span_id`

#### `rapace.parent_span_id`

- **Location**: `OpenChannel.metadata`
- **Type**: 8 bytes (binary)
- **Purpose**: Span ID of the calling service's span
- **Propagation**: When making downstream calls, set this to your current span_id

#### `rapace.trace_flags`

- **Location**: `OpenChannel.metadata`
- **Type**: 1 byte
- **Purpose**: OpenTelemetry trace flags (sampled, etc.)
- **Format**: Bit 0 = sampled (1 if trace should be recorded)

#### `rapace.trace_state`

- **Location**: `OpenChannel.metadata`
- **Type**: UTF-8 string
- **Purpose**: OpenTelemetry trace state (vendor-specific context)
- **Format**: Comma-separated key=value pairs

### Authentication

#### `rapace.auth_token`

- **Location**: `OpenChannel.metadata` or `Hello.params`
- **Type**: UTF-8 string (typically)
- **Purpose**: Bearer token or similar authentication credential
- **Format**: Application-defined (often JWT, API key, etc.)
- **Security**: SHOULD be used with encrypted transports

#### `rapace.auth_scheme`

- **Location**: `OpenChannel.metadata` or `Hello.params`
- **Type**: UTF-8 string
- **Purpose**: Indicates the authentication scheme
- **Values**: `bearer`, `basic`, `hmac`, application-defined

### Deadlines and Timeouts

Rapace has two deadline mechanisms with different scopes:

1. **`MsgDescHot.deadline_ns`** (frame field): Authoritative for protocol enforcement. Uses monotonic clock. See [Cancellation & Deadlines](@/spec/cancellation.md).

2. **Metadata keys** (below): For cross-service propagation through gateways/proxies.

#### `rapace.deadline_remaining_ms`

- **Location**: `OpenChannel.metadata`
- **Type**: 4 bytes (u32 little-endian)
- **Purpose**: Remaining time budget in milliseconds
- **Use**: Cross-service deadline propagation (preferred)
- **Semantics**: Receiver computes local `deadline_ns = monotonic_now() + remaining_ms * 1_000_000`

This is the **recommended** key for deadline propagation because it handles clock skew.

#### `rapace.deadline`

- **Location**: `OpenChannel.metadata`
- **Type**: 8 bytes (u64 little-endian)
- **Purpose**: Absolute deadline as Unix timestamp (nanoseconds since Unix epoch)
- **Use**: When wall-clock time matters (e.g., "complete by 3pm")
- **Semantics**: Server converts to monotonic for enforcement
- **Warning**: Subject to clock skew; prefer `deadline_remaining_ms`

#### Precedence

When multiple deadline sources exist:

1. `MsgDescHot.deadline_ns` (if set) is authoritative for local enforcement
2. `rapace.deadline_remaining_ms` is used for propagation to downstream calls
3. `rapace.deadline` is informational; implementation MAY ignore if `deadline_remaining_ms` is present

### Priority and Quality of Service

#### `rapace.priority`

- **Location**: `OpenChannel.metadata`
- **Type**: 1 byte (u8)
- **Purpose**: Request priority hint
- **Values**: 0 (lowest) to 255 (highest); default 128 (normal)
- **Semantics**: Hint only; server MAY ignore

| Value Range | Meaning |
|-------------|---------|
| 0-31 | Background/batch |
| 32-95 | Low priority |
| 96-159 | Normal (default: 128) |
| 160-223 | High priority |
| 224-255 | Critical/real-time |

#### `rapace.idempotency_key`

- **Location**: `OpenChannel.metadata`
- **Type**: UTF-8 string (max 128 bytes)
- **Purpose**: Client-generated key for request deduplication
- **Semantics**: Server MAY return cached response for duplicate key

### Transport Hints

#### `rapace.unreliable`

- **Location**: `OpenChannel.metadata` (for STREAM channels only)
- **Type**: 1 byte (bool)
- **Purpose**: Request unreliable delivery for this STREAM channel
- **Values**: 0 = reliable (default), 1 = unreliable
- **Semantics**: When set and `WEBTRANSPORT_DATAGRAMS` capability is negotiated, the transport MAY use WebTransport datagrams instead of streams
- **Constraints**: 
  - MUST NOT be set for CALL or TUNNEL channels (they require reliability)
  - Items may be lost or arrive out of order when enabled
- **See**: [Transport Bindings: Datagram Support](@/spec/transport-bindings.md#datagram-support)

### Server Information (Trailers)

#### `rapace.server_timing_ns`

- **Location**: `CallResult.trailers`
- **Type**: 8 bytes (u64 little-endian)
- **Purpose**: Server-side processing time in nanoseconds
- **Semantics**: Time from request receipt to response start

#### `rapace.retryable`

- **Location**: `CallResult.trailers`
- **Type**: 1 byte (bool)
- **Purpose**: Indicates if the failed request is safe to retry
- **Values**: 0 = not retryable, 1 = retryable
- **Use**: Set on error responses to guide client retry logic

#### `rapace.retry_after_ms`

- **Location**: `CallResult.trailers`
- **Type**: 4 bytes (u32 little-endian)
- **Purpose**: Suggested delay before retry (milliseconds)
- **Semantics**: Client SHOULD wait at least this long before retrying

### Connection Parameters

These keys appear in `Hello.params`:

#### `rapace.ping_interval_ms`

- **Location**: `Hello.params`
- **Type**: 4 bytes (u32 little-endian)
- **Purpose**: Suggested Rapace-level ping interval
- **Semantics**: Peer MAY send Ping at this interval; 0 = disabled

#### `rapace.compression`

- **Location**: `Hello.params`
- **Type**: UTF-8 string
- **Purpose**: Comma-separated list of supported compression algorithms
- **Values**: `none`, `zstd`, `lz4`, application-defined
- **Semantics**: Peers use intersection of supported algorithms

#### `rapace.default_priority`

- **Location**: `Hello.params`
- **Type**: 1 byte (u8)
- **Purpose**: Default priority level for all calls on this connection
- **Values**: 0 (lowest) to 255 (highest); if absent, defaults to 128
- **Semantics**: Applies to all calls unless overridden by per-call `rapace.priority` metadata or `HIGH_PRIORITY` frame flag
- **See**: [Prioritization & QoS](@/spec/prioritization.md)

## Propagation Rules

### Request Metadata (`OpenChannel.metadata`)

| Key Pattern | Propagation |
|-------------|-------------|
| `rapace.trace_*` | MUST propagate to downstream calls |
| `rapace.deadline*` | SHOULD propagate with adjustment for local processing |
| `rapace.priority` | MAY propagate (service can override) |
| `rapace.auth_*` | MUST NOT propagate (authenticate at each hop) |
| `rapace.idempotency_key` | MUST NOT propagate (per-service scope) |
| `x-*` | Application-defined |
| Other | Application-defined |

### Trailer Metadata (`CallResult.trailers`)

Trailers are per-call and do not propagate. They are consumed by the immediate caller.

### Connection Parameters (`Hello.params`)

Connection parameters are per-connection and do not propagate.

## Size Limits

To prevent abuse and ensure efficient processing:

| Limit | Value | Notes |
|-------|-------|-------|
| Max key length | 256 bytes | Per key |
| Max value length | 64 KiB | Per value |
| Max metadata entries | 128 | Per location |
| Max total metadata size | 1 MiB | All entries combined |

Implementations SHOULD reject messages exceeding these limits with error code `RESOURCE_EXHAUSTED`.

## Binary vs Text Keys

Some systems distinguish binary and text metadata (e.g., gRPC's `-bin` suffix). Rapace does not:

- All values are `Vec<u8>` (binary)
- Interpretation is key-dependent
- Standard keys document their encoding
- Custom keys SHOULD document their encoding

For interop with systems that require text-only headers (e.g., HTTP gateways):
- Use base64 encoding for binary values
- Append `.b64` to the key name (e.g., `myapp.token.b64`)

## Reserved Keys (Do Not Use)

The following keys are reserved for future use:

- `rapace.version`
- `rapace.encoding`
- `rapace.signature`
- `rapace.encryption`
- `rapace.compression` (in channel metadata)
- Any key starting with `rapace.internal.`

## Examples

### Request with Tracing and Auth

```rust
OpenChannel {
    channel_id: 1,
    kind: ChannelKind::Call,
    attach: None,
    metadata: vec![
        ("rapace.trace_id".into(), trace_id_bytes.to_vec()),
        ("rapace.span_id".into(), span_id_bytes.to_vec()),
        ("rapace.parent_span_id".into(), parent_span_bytes.to_vec()),
        ("rapace.trace_flags".into(), vec![0x01]),  // sampled
        ("rapace.deadline_remaining_ms".into(), 5000u32.to_le_bytes().to_vec()),
        ("rapace.priority".into(), vec![160]),  // high priority
        ("rapace.auth_token".into(), b"Bearer eyJ...".to_vec()),
    ],
    initial_credits: 65536,
}
```

### Response with Timing

```rust
CallResult {
    status: Status { code: 0, message: String::new(), details: vec![] },
    trailers: vec![
        ("rapace.server_timing_ns".into(), 1_234_567u64.to_le_bytes().to_vec()),
    ],
    body: Some(response_bytes),
}
```

### Error Response with Retry Hint

```rust
CallResult {
    status: Status {
        code: ErrorCode::Unavailable as u32,
        message: "service overloaded".into(),
        details: vec![],
    },
    trailers: vec![
        ("rapace.retryable".into(), vec![1]),
        ("rapace.retry_after_ms".into(), 100u32.to_le_bytes().to_vec()),
    ],
    body: None,
}
```

## Next Steps

- [Handshake & Capabilities](@/spec/handshake.md) – Connection-level parameters
- [Observability](@/spec/observability.md) – Tracing integration details
- [Errors](@/spec/errors.md) – Error codes and retry semantics
