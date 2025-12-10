# rapace

> **⚠️ EXPERIMENTAL - DO NOT USE ⚠️**
>
> rapace is a research project. APIs churn, transports change, and there are **zero stability or performance guarantees**.

rapace is a Rust-native RPC spine for host ↔ plugin systems on the same machine. You write async Rust traits, mark them with `#[rapace::service]`, and pick a transport (in-proc, sockets, SHM, WebSocket, wasm). Facet-driven reflection keeps schemas and encoders aligned, while the shared-memory transport moves large payloads without memcpy. It exists to power [dodeca](https://github.com/bearcove/dodeca), so host ↔ plugin calls stay fast even when plugins crash, restart, or stream multi-GB payloads.

## Fast facts
- Trait-first: generated clients/servers stay plain Rust.
- Facet-driven: every type carries schema + per-transport encoders.
- Channel-based: unary + streaming RPCs share the same multiplexed session.
- Crash-aware: a single `RpcSession` per transport handles flow control and recovery.
- Zero-copy local path: SHM transport allocates directly in shared memory.

## Where it fits
- ✅ You own both ends, want async traits, and care about throughput on one box.
- ❌ You need cross-language stability, public internet protocols, hardened sandboxes, or simple HTTP+JSON.

## Current state (December 2025)
| Area | Status |
|------|--------|
| Service macros & session | ✅ prototype-ready |
| In-proc / stream / WebSocket transports | ✅ conformance-tested |
| SHM transport & allocator | ✅ zero-copy, alpha polish |
| Facet integration | ✅ mandatory |
| Browser + wasm client | ✅ Playwright coverage |
| Tooling (registry/explorer) | ⚠️ in flux |
| API stability, prod readiness | ❌ breaking changes expected |

## What ships in this repo
Cargo workspace crates cover the public facade (`rapace`), core channel logic, transports (stream/mem/shm/websocket), registry, explorer UI, tracing/http integrations, fuzz/test harnesses, plus a `demos/` directory with runnable hosts ↔ plugins that double as diagnostics.

## Sponsors
CI and browser suites run on generously provisioned runners provided by Depot:

<p><a href="https://depot.dev?utm_source=rapace">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/bearcove/rapace/raw/main/static/depot-dark.svg">
<img src="https://github.com/bearcove/rapace/raw/main/static/depot-light.svg" height="40" alt="Depot">
</picture>
</a></p>

Their support keeps Miri, fuzzing, wasm, and browser tests running on every change.

## License
Dual-licensed under `LICENSE-APACHE` and `LICENSE-MIT`; pick whichever terms work for you.
