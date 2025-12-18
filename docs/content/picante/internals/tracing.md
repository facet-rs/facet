+++
title = "Tracing and Observability"
weight = 15
+++

picante uses the `tracing` crate for structured instrumentation.

This page documents what picante emits today and how to use it.

## Where instrumentation lives

Primary instrumentation sites:

- `InputIngredient`: `crates/picante/src/ingredient/input.rs`
  - `set`: `#[tracing::instrument(level = "debug", skip_all, fields(kind = self.kind.0))]`
  - `remove`: same
  - `get`: `level = "trace"` and records deps when a frame is active
- `InternedIngredient`: `crates/picante/src/ingredient/interned.rs`
  - `intern`: `debug`
  - `get`: `trace`
- `DerivedIngredient`: `crates/picante/src/ingredient/derived.rs`
  - emits `trace!/debug!` around dependency recording, waiting, revalidation, compute start/ok/err/panic, and in-flight adoption
- `frame`: `crates/picante/src/frame.rs`
  - emits `trace` for dep recording and cycle detection helpers
- persistence: `crates/picante/src/persist.rs`
  - `debug` for load/save start, `info` for completion, `warn` for corruption policy actions / unknown sections / record dropping

## Recommended setup

picante does not install a subscriber. In binaries/tests, use `tracing-subscriber` (or your app’s logging setup).

Common patterns:

- Enable broad traces when debugging invalidation:
  - `RUST_LOG=picante=trace`
- Focus on persistence:
  - `RUST_LOG=picante::persist=debug`

Exact target/module names depend on how your subscriber maps crate/module paths; the simplest is usually `picante=trace`.

### Minimal subscriber

```rust,noexec
use tracing_subscriber::EnvFilter;

tracing_subscriber::fmt()
    .with_env_filter(EnvFilter::from_default_env())
    .init();
```

## How to read traces

Useful mental model:

- `input dep` / `derived dep` traces tell you what dependencies are being recorded
- `revalidate: start` indicates a cached derived value is being checked against its stored dep list
- `compute: start` / `compute: ok|err|panic` indicates the system decided it must recompute
- “wait on running cell” indicates local single-flight contention
- “inflight: follower/leader …” indicates cross-snapshot coalescing

To correlate events, look for the common fields:

- `kind` (u32 kind id)
- `key_hash` (hex string formatting of the deterministic key hash)
- `rev` / `started_at`

## Related runtime events

If you’re building live reload / diagnostics tooling, consider pairing traces with the structured runtime event stream:

- [Runtime Events](../events/)
