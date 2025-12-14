+++
title = "rapace"
description = "An RPC library used by dodeca and related tools"
+++

rapace is a small IPC/RPC library for Rust. It was originally written so dodeca could talk to plugins as separate processes instead of linking everything into one binary.

It provides:

- A [`#[rapace::service]`](https://docs.rs/rapace-macros/latest/rapace_macros/attr.service.html) proc macro for defining request/response interfaces
- Integration with [facet](https://facets.rs) for serialization, deserialization, and type introspection
- [postcard](https://postcard.jamesmunns.com/) as the primary binary wire format, with room for others
- A small set of [transports](https://docs.rs/rapace/latest/rapace/transport/index.html) with a common API
- Basic support for unary and streaming RPCs

Example service (see the [crate documentation](https://docs.rs/rapace) for more):

```rust,noexec
use rapace::prelude::*;

#[rapace::service]
pub trait Calculator {
    async fn add(&self, a: i32, b: i32) -> i32;
}
```

This generates client and server types for `Calculator`. The same trait can be used over in-memory, shared-memory, WebSocket, or stream-based transports.

## Transports

Today rapace ships with:

- Shared memory transport (used by dodeca for host↔plugin)
- WebSocket transport (used by browser-based tools)
- In-memory transport (mainly for tests and experiments)
- Stream transport (TCP/Unix); present but not currently used here

## Related projects

- [dodeca](https://dodeca.bearcove.eu/) – static site generator that motivated rapace
- [`rapace-plugin`](https://docs.rs/rapace-plugin) – high-level plugin runtime for building SHM-based plugins (see [Plugins guide](/guide/plugins/))
- [`rapace-tracing`](https://docs.rs/rapace-tracing) – forwards tracing data over rapace
- [`rapace-registry`](https://docs.rs/rapace-registry) – local service/metadata registry used by codegen and explorer
