# Bench baselines

Each entry: commit SHA, branch, what just landed, the median rpc bench numbers.
Run with `cargo bench --bench rpc -- 'rpc::mem::jit::echo_gnarly'` (no
DIVAN_MIN_TIME, no piping into tail). Median is what we track.

## 2e70ea22 — pre-resolved conduit Tx/Rx + leaked descriptors

Branch: cranelift. Just landed: pre-resolved encoders/decoders in
BareConduit/StableConduit, plus the latent CalibrationRegistry UAF fix
(descriptors leaked at register-time so JIT-embedded `*const` stays valid).

```
mem::jit::echo_gnarly
  n=1   median 12.80 µs   mean 14.52 µs
  n=4   median 20.02 µs   mean 23.43 µs
  n=16  median 47.57 µs   mean 53.19 µs
```

Profile (n=4, 12s nperf capture, ~12k samples):
- ~9% kernel (tokio channel syscalls)
- ~6% in JIT'd encode/decode itself (the actual work)
- **~8% memcpy** — biggest non-JIT user-space cost. Of which:
  - ~2% `Core::set_stage` (storing handler future output in tokio task slot)
  - ~2% `Cell::new` / Box::new (spawning the handler task)
  - ~1% `tokio::task::spawn` registration path
  - ~1% scattered (DynConduitTx::prepare_msg Box::pin, etc.)
- ~6% malloc + free
- ~3% parking_lot mutex lock/unlock
- ~1-2% hashing
- ~1% `vox_jit::encode_with` itself (JIT entry overhead, down from ~16%
  before pre-resolve)

## (TBD SHA) — handler future hosted on `FuturesUnordered`, no `tokio::spawn`

Driver's `run` loop owns a `FuturesUnordered<Abortable<Pin<Box<…>>>>`.
Each incoming request pushes its handler future onto that collection
instead of going through `tokio::spawn`. Cancel still works via
`AbortHandle` stored alongside the in-flight entry. The
`HandlerCompleted` local-control message and the join-handle bookkeeping
both go away — completion is observed directly when the future yields
its `RequestId` from the FuturesUnordered stream.

```
mem::jit::echo_gnarly                  prev      now      Δ
  n=1   median                       12.80 µs  11.58 µs  -9.5%
  n=4   median                       20.02 µs  17.27 µs  -13.7%
  n=16  median                       47.57 µs  45.39 µs  -4.6%
```

Expected wins came from (roughly): killing the `Stage<F, Output>`
`set_stage` memcpy on completion, dropping the tokio scheduler
registration path, and shrinking per-request alloc to one `Box` + one
`Arc<Task>` (no `Cell<T, S>` overhead).

## Next planned change

Run a fresh nperf profile to see what the new top hotspot is. Allocator
+ memcpy was 14% combined before; expect both to drop. If allocator is
still meaningful, escalate to a typed task slab (option (2) from the
sketch). Otherwise look at the next thing in the residual.
