# rapace Implementor's Guide

Rules and invariants for contributors. Violating these breaks the design.

---

## Crate Structure

**Core crates:**
- `rapace` — Main crate, re-exports, `#[rapace::service]` proc macro
- `rapace-core` — Shared types: `Frame`, `FrameView`, `MsgDescHot`, `MsgDescCold`, `MsgHeader`, `ControlPayload`, `ErrorCode`, `FrameFlags`, validation, limits
- `rapace-macros` — Proc macro implementation for `#[rapace::service]`

**Transport crates:**
- `rapace-transport-mem` — In-proc transport (semantic reference)
- `rapace-transport-shm` — Shared memory transport (performance reference)
- `rapace-transport-stream` — TCP/Unix socket transport
- `rapace-transport-websocket` — WebSocket transport

**Supporting crates:**
- `rapace-codec` — Facet-driven encoding/decoding, `EncodeCtx`/`DecodeCtx` traits
- `rapace-registry` — Service registry, schemas, introspection

**Optional/utility crates:**
- `rapace-bus` — Broker pattern for N-way topologies (future)
- `rapace-testing` — Fault injection, conformance tests

**External dependencies:**
- `facet` — Reflection system (from git)
- `facet-postcard` — Postcard encoding via facet (from git)
- `tokio` — Async runtime
- `bitflags` — For `FrameFlags`

---

## Hard Constraints

### NO SERDE

rapace does not use serde. All serialization goes through facet.

```rust
// WRONG: serde derives
#[derive(Serialize, Deserialize)]
struct MyType { ... }

// RIGHT: facet derives
#[derive(facet::Facet)]
struct MyType { ... }
```

This is non-negotiable. If a dependency requires serde, find an alternative or write a facet-based solution.

---

## Core Invariants

These are non-negotiable. If you find yourself wanting to break one, stop and discuss first.

### 1. No transport-specific types in user-visible APIs

```rust
// GOOD: Pure Rust trait
trait Hasher {
    async fn hash(&self, data: &[u8]) -> [u8; 32];
}

// BAD: Transport leaks into signature
trait Hasher {
    async fn hash(&self, data: ShmSlice<'_>) -> [u8; 32];
}
```

The whole point is that plugin authors don't know (or care) what transport they're on.

### 2. No borrow outlives a frame (except in-proc)

```rust
// GOOD: View lifetime tied to frame processing
fn handle_frame<'a>(&self, frame: FrameView<'a>) {
    let data: &'a [u8] = frame.payload();
    self.process(data);  // OK: within frame lifetime
}

// BAD: Storing reference beyond frame
fn handle_frame(&self, frame: FrameView<'_>) {
    self.cached_data = frame.payload();  // WRONG: escapes frame lifetime
}
```

Exception: In-proc transport uses real Rust borrows with real lifetimes.

### 3. Never manufacture `'static` from SHM

SHM mappings can be unmapped. Any `'static` reference to SHM data is unsound.

```rust
// NEVER DO THIS
unsafe fn get_static_ref(slot: &ShmSlot) -> &'static [u8] {
    std::mem::transmute(slot.data())  // UNSOUND
}
```

### 4. Control frames must always be deliverable

Control channel (channel 0) is exempt from per-channel credit flow control.
CANCEL, PING, and credit grants must never be blocked by data backpressure.

```rust
// GOOD: Control frames bypass per-channel credits
if frame.channel_id == 0 {
    // Always process, don't check credits
}

// BAD: Blocking control on credits
if self.credits[channel_id] < frame.len() {
    return Err(NoCredits);  // WRONG if channel_id == 0
}
```

Global resource limits still apply (to prevent DoS), but control frames get priority.

### 5. Validation happens before fault injection

```rust
// GOOD: Validate first, then maybe inject faults
fn process_frame(&self, desc: &MsgDescHot) -> Result<(), Error> {
    validate_descriptor(desc)?;           // Step 1: Always validate
    if self.fault_injector.should_drop() { // Step 2: Then fault inject
        return Ok(());
    }
    self.dispatch(desc)                   // Step 3: Normal processing
}

// BAD: Fault injection masking validation bugs
fn process_frame(&self, desc: &MsgDescHot) -> Result<(), Error> {
    if self.fault_injector.should_drop() {
        return Ok(());  // WRONG: Skipped validation
    }
    validate_descriptor(desc)?;
    self.dispatch(desc)
}
```

### 6. Hot-path structs fit in a single cache line

`MsgDescHot` is 64 bytes. Don't add fields that push it over.

```rust
#[repr(C, align(64))]
struct MsgDescHot {
    // ... 64 bytes total, no more
}

const _: () = assert!(std::mem::size_of::<MsgDescHot>() == 64);
```

If you need more data, use the cold path (`MsgDescCold`) or separate storage.

### 7. SHM is the reference transport; others degrade gracefully

When adding features:
- Design for SHM first (zero-copy, slot references, generations)
- Stream/WebSocket implementations may copy more or skip optimizations
- In-proc may bypass entire subsystems

Don't design features that only work on one transport.

---

## Ordering Guarantees

### What we guarantee

- Frames are delivered **in-order per channel**
- Control channel is ordered relative to itself, not data channels
- There is **no global total order** across channels

### What we don't guarantee

- Order between channels (channel 1 frame may arrive before channel 2 frame sent earlier)
- Order between control and data (cancel may arrive after data it tried to cancel)

### Implication for cancellation

Cancellation is advisory. Late frames after CANCEL are normal and must be handled:

```rust
fn on_frame(&self, channel_id: u32, desc: &MsgDescHot) {
    if self.is_cancelled(channel_id) {
        // Late frame after cancellation - drop silently, free slot
        if desc.payload_slot != u32::MAX {
            self.data_segment.free(desc.payload_slot, desc.payload_generation);
        }
        self.metrics.frames_dropped_after_cancel.inc();
        return;
    }
    // Normal processing...
}
```

---

## Safety Boundary

rapace assumes **memory-safe but potentially buggy** peers.

| Threat | Protection |
|--------|------------|
| Buggy peer sends malformed descriptor | Validation rejects it |
| Buggy peer crashes | Generation counters, heartbeats, cleanup |
| Buggy peer sends garbage payload | Application-level validation |
| Malicious peer tries DoS | Resource limits (channels, frames, payload size) |
| Malicious peer tries memory corruption | **Not defended** — same-machine trust assumed |

We validate everything to prevent UB, but DoS from a malicious peer is possible.

---

## Frame Semantics

A **frame** is the smallest unit of:
- Ordering (frames within a channel are ordered)
- Cancellation (you cancel a channel, affecting all its frames)
- Accounting (credits, telemetry events)

A frame may contain zero bytes (CONTROL, METADATA_ONLY flags).

### Frame vs Message vs Payload

```
Frame = MsgDescHot + optional payload
Payload = MsgHeader + body
Body = encoded arguments/results (postcard, JSON, raw)
```

### Method ID vs Encoding

- `method_id` determines **semantics** (which method to call)
- `encoding` determines **representation** (how the body is serialized)

A method MUST be able to reject unsupported encodings:

```rust
fn dispatch(&self, method_id: u32, encoding: Encoding, body: &[u8]) -> Result<...> {
    if encoding != Encoding::Postcard {
        return Err(RpcError::UnsupportedEncoding);
    }
    // ...
}
```

---

## Transport Implementation Checklist

When implementing a new transport:

### Required

- [ ] Implement `Transport` trait (send_frame, recv_frame, encoder, close)
- [ ] Implement transport-specific `EncodeCtx`
- [ ] Handle connection setup / teardown
- [ ] Respect frame ordering within channels
- [ ] Pass validation for all received frames

### Recommended

- [ ] Implement `DynTransport` wrapper for dynamic dispatch
- [ ] Support deadlines (reject frames past deadline)
- [ ] Emit telemetry events
- [ ] Handle graceful shutdown

### Transport-Specific

| Transport | Special Considerations |
|-----------|----------------------|
| In-proc | May bypass encoding entirely; use real Rust lifetimes |
| SHM | Check if data is already in SHM for zero-copy; validate generations |
| Stream | Length-prefix frames; handle partial reads |
| WebSocket | One WS message = one frame; handle text vs binary |

---

## EncodeCtx Implementation Rules

### encode_bytes behavior by transport

```rust
// In-proc: just reference
fn encode_bytes(&mut self, bytes: &[u8]) {
    self.ref = bytes;  // Zero-copy, real borrow
}

// SHM: check location, then either reference or copy
fn encode_bytes(&mut self, bytes: &[u8]) {
    if let Some((slot, offset)) = self.session.find_shm_location(bytes) {
        self.builder.encode_slot_ref(slot, offset, bytes.len());
    } else {
        let slot = self.session.alloc_slot(bytes.len())?;
        self.session.copy_to_slot(slot, bytes);
        self.builder.encode_slot_ref(slot, 0, bytes.len());
    }
}

// Stream: always copy
fn encode_bytes(&mut self, bytes: &[u8]) {
    self.buf.extend_from_slice(bytes);
}
```

---

## Common Mistakes

### Mistake: Assuming total order

```rust
// WRONG: Assumes channel 1 frame arrives before channel 2
send_on_channel(1, setup_data);
send_on_channel(2, depends_on_setup);  // May arrive first!
```

### Mistake: Holding frame references

```rust
// WRONG: Reference escapes frame lifetime
let saved: &[u8];
transport.recv_frame(|frame| {
    saved = frame.payload();  // Borrow escapes!
});
use(saved);  // Use after free
```

### Mistake: Forgetting to free slots

```rust
// WRONG: Slot leak
fn handle(&self, desc: &MsgDescHot) {
    if desc.channel_id == BORING_CHANNEL {
        return;  // Forgot to free payload_slot!
    }
    // ...
}

// RIGHT: Always free
fn handle(&self, desc: &MsgDescHot) {
    let _guard = SlotGuard::new(&self.data_segment, desc);  // RAII
    if desc.channel_id == BORING_CHANNEL {
        return;  // Guard frees on drop
    }
    // ...
}
```

### Mistake: Blocking control channel

```rust
// WRONG: Control blocked on data credits
async fn send(&self, frame: Frame) {
    self.wait_for_credits(frame.channel_id).await;  // Blocks channel 0!
    // ...
}

// RIGHT: Control bypasses credits
async fn send(&self, frame: Frame) {
    if frame.channel_id != 0 {
        self.wait_for_credits(frame.channel_id).await;
    }
    // ...
}
```

---

## Testing Requirements

### For new features

- [ ] Works on SHM transport
- [ ] Works on stream transport (or gracefully degrades)
- [ ] Works on in-proc transport
- [ ] Handles cancellation mid-operation
- [ ] Handles peer death mid-operation
- [ ] Doesn't leak slots/channels on error paths

### For transport implementations

- [ ] Passes shared conformance tests
- [ ] Handles frame ordering correctly
- [ ] Cleans up on close
- [ ] Respects resource limits

---

## Design Smells

If a change requires any of the following, the design is probably wrong:

- **Higher-ranked lifetimes in user traits** — user traits should be simple
- **New public type parameter for transports** — transport should be invisible to users
- **Assuming total ordering across channels** — only per-channel order is guaranteed
- **Assuming borrows survive async suspension** — they don't (except in-proc)
- **Checking transport type at runtime in user code** — behavior should be uniform
- **Blocking the control channel on data flow** — control must always be deliverable

If you find yourself needing one of these, step back and reconsider.

---

## When in Doubt

1. Check the Design notes in `docs/content/guide/design.md`
2. Ask: "Does this work on all transports?"
3. Ask: "What happens if the peer crashes right now?"
4. Ask: "What happens if this is cancelled right now?"
5. Ask: "Does any borrow escape a frame?"

If you're still unsure, discuss before implementing.
