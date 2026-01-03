+++
title = "Cancellation & Deadlines"
description = "Request cancellation and timeout semantics"
weight = 55
+++

This document defines how Rapace handles request cancellation and deadline enforcement.

## Overview

Rapace supports two mechanisms for limiting request lifetime:

1. **Deadlines**: Absolute time after which a request should be abandoned
2. **Cancellation**: Explicit signal to stop processing a request

Both mechanisms propagate to attached streams/tunnels and trigger cleanup of resources (including SHM slot guards).

## Deadlines

### Representation

r[cancel.deadline.field]
Deadlines MUST be carried in the `deadline_ns` field of `MsgDescHot`:

```rust
pub deadline_ns: u64,  // Absolute deadline in nanoseconds
```

| Value | Meaning |
|-------|---------|
| `0xFFFFFFFFFFFFFFFF` | No deadline (wait forever) |
| Any other value | Absolute deadline timestamp |

### Time Base

r[cancel.deadline.clock]
Deadlines MUST use monotonic clock nanoseconds since an epoch. The epoch is established per-connection.

r[cancel.deadline.shm]
For SHM transports, implementations MUST use the system monotonic clock directly, as both processes share the same clock.

r[cancel.deadline.stream]
For stream/WebSocket transports, the sender MUST compute `remaining_ns = deadline_ns - now()` and the receiver MUST compute `deadline_ns = now() + remaining_ns`. This handles clock skew but introduces round-trip latency into the deadline.

r[cancel.deadline.rounding]
When converting between deadline formats: nanoseconds to milliseconds (for `rapace.deadline_remaining_ms`) MUST use floor division (round down) to ensure the receiver never waits longer than intended. Milliseconds to nanoseconds MUST multiply exactly (`ms * 1_000_000`).

r[cancel.deadline.expired]
If `remaining_ns` is negative or zero, the deadline has already passed. Senders SHOULD fail the request locally rather than transmitting it; receivers MUST immediately return `DEADLINE_EXCEEDED`.

### Deadline Propagation

When a call has a deadline:

1. The deadline is set on the CALL channel's request frame
2. All attached STREAM/TUNNEL channels inherit the same deadline
3. If any operation on the call exceeds the deadline, the entire call fails

### Deadline Exceeded Behavior

r[cancel.deadline.exceeded]
When `now() > deadline_ns`:
- Senders SHOULD fail the request immediately rather than sending it
- Receivers MUST stop processing and return `DEADLINE_EXCEEDED`
- Attached channels MUST be canceled
- SHM slot guards MUST be dropped to free slots

r[cancel.deadline.terminal]
The `DEADLINE_EXCEEDED` error MUST be treated as terminal. The call cannot succeed after this point.

## Cancellation

### CancelChannel Control Message

Cancellation is signaled via the `CancelChannel` control message on channel 0:

```rust
CancelChannel {
    channel_id: u32,
    reason: CancelReason,
}

enum CancelReason {
    ClientCancel = 1,      // Client explicitly canceled
    DeadlineExceeded = 2,  // Deadline passed
    ResourceExhausted = 3, // Out of memory, slots, etc.
    ProtocolViolation = 4, // Malformed frames, invalid state
    Unauthenticated = 5,   // Missing or invalid authentication
    PermissionDenied = 6,  // Valid credentials, not authorized
}
```

### Cancellation Scope

| Target | Effect |
|--------|--------|
| `CancelChannel(call_channel)` | Cancels the call AND all attached streams/tunnels |
| `CancelChannel(stream_channel)` | Cancels only that stream; call may continue if stream was optional |

### Propagation Rules

When a CALL channel is canceled:

1. **All required attached channels** are implicitly canceled
2. **All optional attached channels** are implicitly canceled
3. **Server MUST stop processing** the request
4. **Server SHOULD send** a response with error status (if not already sent)
5. **Client MUST stop waiting** for the response

When a STREAM/TUNNEL channel is canceled:

1. **That channel only** is canceled
2. **If the port was required**: The parent call fails with an error
3. **If the port was optional**: The parent call may still complete successfully

### Idempotency

r[cancel.idempotent]
`CancelChannel` MUST be idempotent. Sending it multiple times for the same channel has no additional effect. Implementations MUST handle duplicate cancellations gracefully.

### Cancel vs EOS

| Signal | Meaning | Data |
|--------|---------|------|
| `EOS` | Graceful end of stream | All data was sent successfully |
| `CancelChannel` | Abort | Discard pending data, stop immediately |

r[cancel.precedence]
A stream may receive `EOS` from one side and `CancelChannel` from the other. `CancelChannel` takes precedence—any pending data MUST be discarded.

## Cleanup Semantics

### SHM Slot Reclamation

r[cancel.shm.reclaim]
When a channel is canceled (or deadline exceeded):
- The receiver MUST drop all `SlotGuard`s for that channel
- Slots are freed back to the sender's pool
- Pending frames in the ring for that channel MAY be discarded without processing

This prevents slot leaks when requests are abandoned.

### Pending Writes

When cancellation is received:

| Transport | Behavior |
|-----------|----------|
| **Stream/WebSocket** | MAY continue to drain pending writes, or close immediately |
| **SHM** | MUST free slots; MAY leave descriptors in ring (receiver ignores them) |

The key invariant: **slot resources MUST be freed promptly on cancellation**.

### Ordering

r[cancel.ordering]
Cancellation is asynchronous. Implementations MUST handle the lack of ordering guarantees between `CancelChannel` on channel 0 and data frames on the canceled channel.

r[cancel.ordering.handle]
Implementations MUST handle all of the following cases:
- Data frames arriving after `CancelChannel`: these MUST be ignored
- `CancelChannel` arriving after `EOS`: this is a no-op (already closed)
- Multiple `CancelChannel` for the same channel: handling MUST be idempotent

## Client-Side Cancellation

Clients cancel requests by:

1. Sending `CancelChannel(call_channel, ClientCancel)` on channel 0
2. Optionally waiting for a response (server may send error status)
3. Dropping local state for the call

### Fire-and-Forget Cancellation

Clients MAY send `CancelChannel` without waiting for acknowledgment. The server will eventually process it and clean up.

### Cancellation Before Response

If the client cancels before receiving a response:
- Server MAY still send a response (client ignores it)
- Server MAY send an error response acknowledging cancellation
- Server MAY send nothing (client already moved on)

## Server-Side Cancellation

Servers cancel requests when:

1. **Deadline exceeded**: Server notices `now() > deadline_ns`
2. **Resource exhaustion**: Server is overloaded
3. **Shutdown**: Server is draining connections

### Graceful Shutdown Cancellation

During server shutdown:

1. Server sends `GoAway` to stop new calls
2. Server waits for grace period (or until all calls complete)
3. After grace period, server sends `CancelChannel` for remaining calls with reason `DeadlineExceeded`
4. Server closes the connection

The `DeadlineExceeded` reason is used because the grace period acts as a deadline. Clients receiving this error MAY retry on a different server.

See [Overload & Draining](@/spec/overload.md) for details.

## Error Status on Cancellation

When a call is canceled, the response (if sent) uses these status codes:

| Reason | Status Code | Retryable? |
|--------|-------------|------------|
| `ClientCancel` | `CANCELLED` | No (client chose to cancel) |
| `DeadlineExceeded` | `DEADLINE_EXCEEDED` | Maybe (with new deadline) |
| `ResourceExhausted` | `RESOURCE_EXHAUSTED` | Yes (after backoff) |
| `ProtocolViolation` | `INTERNAL` | No (bug) |

See [Error Handling](@/spec/errors.md) for the full error code table.

## Implementation Requirements

r[cancel.impl.support]
Implementations MUST support the `CancelChannel` control message.

r[cancel.impl.propagate]
Implementations MUST propagate cancellation to attached channels.

r[cancel.impl.shm-free]
Implementations MUST free SHM slots promptly on cancellation.

r[cancel.impl.idempotent]
Implementations MUST handle `CancelChannel` idempotently.

r[cancel.impl.check-deadline]
Implementations SHOULD check deadlines before sending requests.

r[cancel.impl.error-response]
Implementations SHOULD send error responses when canceling server-side and SHOULD drain pending writes gracefully when possible.

r[cancel.impl.ignore-data]
Implementations MAY ignore data frames after `CancelChannel`. Implementations MAY close the connection on repeated protocol violations.

## Summary

| Aspect | Rule |
|--------|------|
| **Deadline field** | `deadline_ns` in `MsgDescHot`, `0xFFFF...` = none |
| **Time base** | Monotonic nanoseconds; duration-based for cross-machine |
| **Cancel message** | `CancelChannel` on channel 0 |
| **Cancel scope** | Call cancels all attached; stream cancels only itself |
| **Propagation** | Automatic to attached channels |
| **Cleanup** | Drop slot guards, free resources promptly |
| **Idempotency** | Multiple cancels are safe |

## Next Steps

- [Core Protocol](@/spec/core.md) – Channel lifecycle
- [Error Handling](@/spec/errors.md) – Error codes and status
- [Overload & Draining](@/spec/overload.md) – Graceful shutdown
