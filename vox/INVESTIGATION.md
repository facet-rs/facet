# Investigation Log

- [2026-02-20 21:38:30 CET] Codex: Started investigation for failing `mixed_calls_with_slot_exhaustion` test.
- [2026-02-20 21:38:30 CET] Codex: Reproduced with `cargo nextest run -p roam-shm -E 'test(mixed_calls_with_slot_exhaustion)' --no-capture`.
- [2026-02-20 21:38:30 CET] Codex: Current failure mode is timeout at ~4.006s (no assertion panic observed).
- [2026-02-20 21:38:52 CET] Codex: Timeout reproduced (~4s) in mixed_calls_with_slot_exhaustion; no panic. Started tracing hang path through rust/roam-shm/tests/driver.rs plus slot lifecycle in rust/roam-shm/src/guest.rs and rust/roam-shm/src/var_slot_pool.rs.

---

## Claude (Opus 4.6) — 2026-02-20 ~22:00 CET

### Test Details

- Test: `roam-shm::driver::mixed_calls_with_slot_exhaustion` (`rust/roam-shm/tests/driver.rs:1135`)
- With `--profile loom` (30s timeout): FAIL at 20.248s — inner 10s per-round timeout fires
- Config: `bipbuf_capacity: 4096, max_guests: 4, ..default`

### Key Finding: VarSlotPool Is NOT the Bottleneck

The test name says "slot exhaustion" but with default `inline_threshold = 256` (frame header 24 bytes + payload ≤ 232 bytes → inline):
- Request payloads: ~135-140 bytes → **inline** (confirmed by debug: `encode_request_payload result_len=135`)
- DATA payloads: ~22 bytes (8 conn_id + ~14 string) → **inline**
- RESPONSE/CLOSE payloads: small → **inline**

**Nothing uses VarSlotPool at all.** The real bottleneck is the **H2G BipBuffer at only 4096 bytes**.

### Architecture (from reading code)

**Guest driver** (`ShmDriver` with `ShmGuestTransport`) runs `tokio::select! { biased; }`:
1. `incoming_response_rx.recv()` — Accept/Reject (highest priority)
2. `driver_rx.recv()` — outgoing G2H messages (Response, Data, Close)
3. `MessageTransport::recv(&mut self.io)` — incoming H2G (lowest priority)

Guest `recv()` (`transport.rs:773`): reads H2G ring → signals G2H doorbell
Guest `send()` (`transport.rs:531`): writes G2H ring → on RingFull/SlotExhausted, waits on `doorbell.wait()` (H2G signal!)

**Host driver** (`MultiPeerHostDriver`) runs `tokio::select!` (**not** biased):
1. `control_rx` — add peers
2. `incoming_response_rx` — Accept/Reject
3. `ring_rx` — G2H doorbell rang → poll + handle messages + `retry_all_pending_sends()`
4. `driver_msg_rx` — outgoing H2G calls from ConnectionHandle → `send_to_peer()` → on backpressure, queues to `pending_sends`

**Critical**: `pending_sends` is ONLY drained in the `ring_rx` arm via `retry_all_pending_sends()`.

### Potential Deadlock Mechanism

1. Host sends 10 concurrent requests (via `driver_msg_rx`). First few succeed (written to H2G ring, doorbell signaled). But H2G ring (4096 bytes) fills up → remaining messages queued in `pending_sends`.

2. Guest reads H2G messages via its `recv` arm → signals G2H doorbell after each read.

3. Host receives G2H doorbell → `ring_rx` arm fires → calls `host.poll()` + `retry_all_pending_sends()`. This sends more pending messages to H2G ring, signals doorbell.

4. But host `select!` is **not biased** — `driver_msg_rx` competes fairly with `ring_rx`. Drain tasks for streaming channels keep producing DATA messages via `driver_msg_rx`. If `driver_msg_rx` repeatedly wins the select race, the host keeps queuing more messages into `pending_sends` without ever running `retry_all_pending_sends()`.

5. Meanwhile, the guest's biased select prioritizes `driver_rx` (sending G2H) over `recv` (reading H2G). If the guest has responses/data to send, it tries those first. But if the G2H ring is... no, the G2H ring shouldn't be full (host reads it promptly).

**Most likely root cause**: The host `select!` starvation. `driver_msg_rx` keeps feeding messages faster than `ring_rx` gets a chance to retry pending sends. The H2G ring empties (guest reads) but host never retries because the doorbell notification gets starved by driver_msg_rx.

### Spec Rules (from `docs/content/shm-spec/_index.md`)

- `r[shm.backpressure.host-to-guest]`: Host MUST queue and retry when H2G buffer full or no slots. Guest signals doorbell after consuming; host uses that as cue.
- `r[shm.wakeup.producer-wait]`: Producer waiting for space → Consumer signals doorbell after releasing bytes.
- `r[shm.slot.exhaustion]`: Sender MUST wait. Poll with backoff. Not a protocol error.
- `r[shm.wakeup.slot-wait]`: Receiver signals after freeing a slot.

### TODO

- [ ] Verify the starvation theory: add tracing to count how often `ring_rx` vs `driver_msg_rx` wins the select
- [ ] Possible fix: make host select biased (prioritize `ring_rx` over `driver_msg_rx`) so pending drains happen promptly
- [ ] Alternative fix: also call `retry_all_pending_sends()` in the `driver_msg_rx` arm (attempt retry on every iteration)
- [ ] Check if guest biased select (driver_rx before recv) also contributes
