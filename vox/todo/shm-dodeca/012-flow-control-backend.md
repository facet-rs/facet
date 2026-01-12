# Phase 012: Real Flow Control (Move Off Infinite Credit)

## Goal

Implement real credit-based flow control for streaming (`Tx<T>`/`Rx<T>`) so that:

- Stream transports can grant/receive credit via `Message::Credit`.
- SHM can grant/consume credit via the channel table atomics (`ChannelEntry.granted_total`,
  sender-local `sent_total`) without `Message::Credit`.

This phase removes the current reliance on “infinite credit” (`u32::MAX`) which
works but provides no meaningful backpressure.

## Current State

- `roam_session::ChannelRegistry` enforces incoming credit (`CreditOverrun`) but
  the runtime does not yet replenish credit as streams are consumed.
- Both stream and SHM drivers currently lean on the “infinite credit” escape hatch
  (`ChannelRegistry::new()`), which is spec-permitted but not ideal for dodeca.

## Implementation Plan

### 1. Wire FlowControl Into Stream Consumption

- Extend `Rx<T>::recv()` (and any other “bytes consumed” points) so that after a
  payload is successfully received and deserialized, we notify the connection’s
  flow control mechanism to grant credit back to the sender.
- This requires a way for `Rx<T>` to reach a `FlowControl` implementation:
  - store an `Arc<...>` in the stream handle, or
  - have `ChannelRegistry` provide a per-channel hook that `Rx` can call.

### 2. Stream Transport Backend

- Implement a FlowControl backend that turns “grant credit” into a queued
  `Message::Credit` to send, and “wait for send credit” into a wait on the local
  `outgoing_credit` state.

### 3. SHM Backend

- Implement a FlowControl backend that:
  - grants credit by `ChannelEntry::granted_total.fetch_add(bytes)`
  - waits for send credit using `wait_for_credit` on `ChannelEntry.granted_total`
    (with futex on Linux, backoff elsewhere)
  - records sent bytes by incrementing sender-local `sent_total`

### 4. Tighten Defaults

- Switch SHM and stream drivers to use negotiated finite `initial_credit` once
  replenishment exists.
- Add tests that send > `initial_credit` bytes over a single stream without
  triggering `CreditOverrun`.

## Tasks

- [ ] Decide how `Rx<T>` reaches FlowControl
- [ ] Implement stream flow-control backend (`Message::Credit`)
- [ ] Implement SHM flow-control backend (channel table)
- [ ] Update drivers to use finite `initial_credit`
- [ ] Add tests for credit replenishment

