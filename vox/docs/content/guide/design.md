+++
title = "Design notes"
description = "Invariants and internal design constraints"
+++

This page collects internal design constraints and invariants. It used to live at the repository root as `DESIGN.md`.

rapace’s public surface is intentionally small:

- a `Transport` enum that moves `Frame`s between two peers
- an `RpcSession` that owns a `Transport` and multiplexes channels
- `#[rapace::service]` codegen that turns trait methods into framed RPC calls

## Key invariants

- **Single reader**: only `RpcSession::run()` calls `Transport::recv_frame()`.
- **Frames only at transport boundary**: transports do not know about methods, only `Frame`s.
- **No transport generics in user code**: user-facing APIs should not grow transport type parameters.
- **Payload is transport-owned**: payload storage is an enum (`Payload`), not an associated type.

## Transport API shape

The transport boundary is:

- `Transport::send_frame(Frame) -> Result<(), TransportError>`
- `Transport::recv_frame() -> Result<Frame, TransportError>`

`Frame` contains a fixed-size header (`MsgDescHot`) and a `Payload` enum. Payload variants include:

- inline bytes (stored in the descriptor)
- owned heap bytes (`Vec<u8>`)
- ref-counted bytes (`bytes::Bytes`)
- pooled buffers (reserved; see issue #46)
- SHM slot guards (feature-gated; frees the slot on drop)

## Session dispatch

Server-side dispatch is installed with:

```rust,noexec
use std::sync::Arc;
use rapace::{Frame, RpcError, RpcSession, Transport};

let (client_transport, server_transport) = Transport::mem_pair();
let server_session = Arc::new(RpcSession::with_channel_start(server_transport, 1));

server_session.set_dispatcher(move |request: Frame| {
    Box::pin(async move {
        // Decode + handle + encode here (typically via a generated `FooServer`)
        let mut response = request; // placeholder
        response.desc.flags = rapace::FrameFlags::DATA | rapace::FrameFlags::EOS;
        Ok(response)
    })
});
```

The dispatcher takes the full request `Frame` and returns a response `Frame`. `RpcSession` is responsible for:

- routing responses to pending callers by `channel_id`
- routing streaming `DATA` frames to tunnels
- enforcing the “single reader” invariant

## SHM layout source of truth

The canonical SHM `repr(C)` layout is defined in code in `rust/rapace-core/src/transport/shm/layout.rs`. The Architecture guide has an overview and diagrams.

