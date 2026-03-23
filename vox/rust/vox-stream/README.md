# vox-stream

Byte-stream transport binding for Vox over `AsyncRead`/`AsyncWrite`.

## Role in the Vox stack

`vox-stream` implements the `Link` layer using length-prefixed framing on stream transports.

## What this crate provides

- Framing and link adapters for TCP/Unix/stdio style byte streams
- Runtime-compatible transport glue for session establishment

## Fits with

- `vox-core` session and driver runtime
- `vox-local` for local IPC sockets and named pipes
- `vox-types` for transport message payloads

Part of the Vox workspace: <https://github.com/bearcove/vox>
