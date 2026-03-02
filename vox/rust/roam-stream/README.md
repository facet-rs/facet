# roam-stream

Byte-stream transport binding for Roam over `AsyncRead`/`AsyncWrite`.

## Role in the Roam stack

`roam-stream` implements the `Link` layer using length-prefixed framing on stream transports.

## What this crate provides

- Framing and link adapters for TCP/Unix/stdio style byte streams
- Runtime-compatible transport glue for session establishment

## Fits with

- `roam-core` session and driver runtime
- `roam-local` for local IPC sockets and named pipes
- `roam-types` for transport message payloads

Part of the Roam workspace: <https://github.com/bearcove/roam>
