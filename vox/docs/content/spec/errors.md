+++
title = "Error Handling"
description = "Error codes, status, and retry semantics"
weight = 60
+++

This document defines how Rapace represents and handles errors.

## Overview

Rapace distinguishes between:

1. **Protocol errors**: Malformed frames, invalid state, connection issues
2. **RPC errors**: Call failed with a status code (but protocol is fine)
3. **Application errors**: Business logic failures encoded in the return type

## Status Type

Every RPC response includes a `Status` in the `CallResult` envelope:

```rust
struct Status {
    code: u32,        // Error code (0 = OK)
    message: String,  // Human-readable description
    details: Vec<u8>, // Opaque structured details (optional)
}
```

### Status in CallResult

```rust
struct CallResult {
    status: Status,
    trailers: Vec<(String, Vec<u8>)>,
    body: Option<Vec<u8>>,  // Present iff status.code == 0
}
```

r[error.status.success]
On success, `status.code` MUST be 0 and `body` MUST contain the response.

r[error.status.error]
On error, `status.code` MUST NOT be 0 and `body` MUST be `None`.

## Error Codes

### Code Ranges

| Range | Category |
|-------|----------|
| 0 | OK (success) |
| 1-49 | Standard RPC errors (gRPC-compatible) |
| 50-99 | Protocol/transport errors |
| 100-399 | Reserved for future use |
| 400+ | Application-defined |

### Standard RPC Error Codes

These codes are compatible with gRPC status codes for interoperability:

| Code | Name | Description | Retryable |
|------|------|-------------|-----------|
| 0 | `OK` | Success | N/A |
| 1 | `CANCELLED` | Request was canceled by client | No |
| 2 | `UNKNOWN` | Unknown error | Maybe |
| 3 | `INVALID_ARGUMENT` | Client sent invalid arguments | No |
| 4 | `DEADLINE_EXCEEDED` | Deadline passed before completion | Maybe |
| 5 | `NOT_FOUND` | Requested entity not found | No |
| 6 | `ALREADY_EXISTS` | Entity already exists | No |
| 7 | `PERMISSION_DENIED` | Caller lacks permission | No |
| 8 | `RESOURCE_EXHAUSTED` | Out of resources (memory, slots, quota) | Yes |
| 9 | `FAILED_PRECONDITION` | System not in required state | No |
| 10 | `ABORTED` | Operation aborted (e.g., concurrency conflict) | Yes |
| 11 | `OUT_OF_RANGE` | Value out of valid range | No |
| 12 | `UNIMPLEMENTED` | Method not implemented | No |
| 13 | `INTERNAL` | Internal server error | Maybe |
| 14 | `UNAVAILABLE` | Service temporarily unavailable | Yes |
| 15 | `DATA_LOSS` | Unrecoverable data loss | No |
| 16 | `UNAUTHENTICATED` | Missing or invalid authentication | No |
| 17 | `INCOMPATIBLE_SCHEMA` | Schema hash mismatch | No |

### Protocol Error Codes

| Code | Name | Description |
|------|------|-------------|
| 50 | `PROTOCOL_ERROR` | Generic protocol violation |
| 51 | `INVALID_FRAME` | Malformed frame |
| 52 | `INVALID_CHANNEL` | Unknown or invalid channel ID |
| 53 | `INVALID_METHOD` | Unknown method ID |
| 54 | `DECODE_ERROR` | Failed to decode payload |
| 55 | `ENCODE_ERROR` | Failed to encode payload |

## Error Categories

### Protocol Errors

Protocol errors indicate a bug or misconfiguration. They typically close the connection.

**Examples**:
- Malformed frame (wrong size, invalid flags)
- Unknown channel ID in data frame
- Payload decode failure

**Response**: Close the connection. No retry.

### Transport Errors

Transport errors indicate network or I/O issues.

**Examples**:
- Connection closed unexpectedly
- Read/write timeout
- TLS handshake failure

**Response**: Reconnect and retry (if idempotent).

### RPC Errors

RPC errors indicate the call was processed but failed.

**Examples**:
- `NOT_FOUND`: Requested resource doesn't exist
- `PERMISSION_DENIED`: Caller lacks access
- `INVALID_ARGUMENT`: Bad request data

**Response**: Depends on error code (see retryability).

### Application Errors

Application errors are encoded in the response type, not the status.

```rust
// Application error in return type
async fn get_user(&self, id: u64) -> Result<User, GetUserError>;

enum GetUserError {
    NotFound,
    Suspended { reason: String },
    RateLimited { retry_after_secs: u32 },
}
```

The RPC status is `OK` (code 0). The error is in the response body.

**When to use application errors vs status codes**:
- Use **status codes** for infrastructure concerns (auth, rate limiting, unavailable)
- Use **application errors** for business logic (user suspended, invalid state)

## Retryability

### Retry Decision

| Code | Retryable | Notes |
|------|-----------|-------|
| `OK` | N/A | Success |
| `CANCELLED` | No | Client chose to cancel |
| `INVALID_ARGUMENT` | No | Fix the request |
| `DEADLINE_EXCEEDED` | Maybe | Retry with new deadline |
| `NOT_FOUND` | No | Entity doesn't exist |
| `PERMISSION_DENIED` | No | Need different credentials |
| `RESOURCE_EXHAUSTED` | Yes | Backoff and retry |
| `ABORTED` | Yes | Retry immediately or with backoff |
| `UNAVAILABLE` | Yes | Backoff and retry |
| `INTERNAL` | Maybe | Depends on cause |
| `INCOMPATIBLE_SCHEMA` | No | Schema mismatch is permanent |

### Retry Strategy

For retryable errors:

1. **Exponential backoff**: Start with 100ms, double each retry, cap at 30s
2. **Jitter**: Add random jitter (±25%) to prevent thundering herd
3. **Max retries**: Limit total retries (e.g., 5)
4. **Deadline**: Respect the overall deadline

```
retry_delay = min(base_delay * 2^attempt, max_delay) * (0.75 + random(0, 0.5))
```

### Idempotency

**Idempotent operations** can be safely retried:
- Read operations
- Operations with client-provided idempotency keys
- Operations that check-and-set

**Non-idempotent operations** should NOT be retried blindly:
- Create operations (may create duplicates)
- Increment operations (may double-count)

Rapace does not enforce idempotency. Applications must design for it.

## Error Details

The `Status.details` field carries structured error information as an opaque byte vector.

### Normative Status

The `details` field is **application-defined**. The structure below is a **recommended convention**, not a normative requirement:

```rust
struct Status {
    code: u32,
    message: String,
    details: Vec<u8>,  // Opaque bytes; recommended: Postcard-encoded ErrorDetails
}

// RECOMMENDED structure (not normative)
struct ErrorDetails {
    // Retry information
    retry_after_ms: Option<u64>,
    
    // Debugging
    debug_info: Option<String>,
    stack_trace: Option<String>,
    
    // Structured cause
    cause: Option<ErrorCause>,
}

enum ErrorCause {
    QuotaExceeded { quota_name: String, limit: u64, used: u64 },
    RateLimited { requests_per_second: u32 },
    FieldViolation { field: String, description: String },
    // ... extensible
}
```

**Interoperability note**: If you use a different `details` format, receivers that expect the recommended format will fail to decode. For cross-organization APIs, document your error details schema explicitly.

r[error.details.populate]
Implementations SHOULD populate `details` for actionable errors.

r[error.details.unknown-format]
Implementations MUST NOT fail if `details` is empty or contains an unknown format.

## Error Propagation

### From Server to Client

1. Server catches error during processing
2. Server encodes `Status` with appropriate code
3. Server sends response frame with `ERROR` flag
4. Client decodes `CallResult`, extracts `Status`
5. Client maps to language-appropriate error type

### From Attached Channels

If an attached STREAM/TUNNEL channel fails:

1. The channel is canceled
2. If the port was **required**: The call fails with an error
3. If the port was **optional**: The call may succeed without that port's data

### Cancel vs Error

| Situation | Status Code |
|-----------|-------------|
| Client canceled | `CANCELLED` |
| Deadline exceeded | `DEADLINE_EXCEEDED` |
| Server overloaded | `RESOURCE_EXHAUSTED` or `UNAVAILABLE` |
| Server shutting down | `UNAVAILABLE` |
| Stream decode failed | `INTERNAL` or `INVALID_ARGUMENT` |

## ERROR Flag

The `ERROR` flag in `FrameFlags` is a fast-path hint:

```rust
const ERROR = 0b0001_0000;
```

r[error.flag.match]
The `ERROR` flag MUST be set if and only if `status.code != 0`.

r[error.flag.parse]
Receivers MAY use the flag for fast error detection but MUST still parse `CallResult` for the actual status.

## Connection-Level Errors

Some errors affect the entire connection:

| Error | Action |
|-------|--------|
| Handshake failure | Close connection |
| Version mismatch | Close connection |
| Repeated protocol violations | Close connection |
| Authentication failure | Close connection (after error response) |

For these, send an error response (if possible) then close.

## Implementation Requirements

r[error.impl.standard-codes]
Implementations MUST use the standard error codes for standard conditions.

r[error.impl.error-flag]
Implementations MUST set the `ERROR` flag correctly (matching `status.code != 0`).

r[error.impl.status-required]
Implementations MUST include `Status` in all error responses.

r[error.impl.unknown-codes]
Implementations MUST handle unknown error codes gracefully.

r[error.impl.details]
Implementations SHOULD populate `details` for actionable errors and SHOULD include `message` for debugging.

r[error.impl.backoff]
Implementations SHOULD implement exponential backoff for retries.

r[error.impl.custom-codes]
Implementations MAY define application-specific error codes in the 400+ range. Implementations MAY include stack traces in `details` (debug builds only).

## Summary

| Aspect | Rule |
|--------|------|
| **Success** | `status.code == 0`, body present |
| **Error** | `status.code != 0`, body absent |
| **ERROR flag** | Must match `status.code != 0` |
| **Retryable** | `RESOURCE_EXHAUSTED`, `UNAVAILABLE`, `ABORTED` |
| **Not retryable** | `CANCELLED`, `INVALID_ARGUMENT`, `PERMISSION_DENIED` |
| **Application errors** | Encoded in response type, not status |
| **Details** | Optional structured info in `Status.details` |

## Next Steps

- [Core Protocol](@/spec/core.md) – CallResult envelope
- [Cancellation & Deadlines](@/spec/cancellation.md) – Timeout handling
- [Handshake & Capabilities](@/spec/handshake.md) – Connection establishment errors
