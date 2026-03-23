# vox-websocket

WebSocket transport binding for Vox links.

## Role in the Vox stack

`vox-websocket` implements the `Link` layer on top of WebSocket binary frames.

## What this crate provides

- Client/server link adapters over WebSocket transports
- Native and wasm-friendly runtime integration points

## Fits with

- `vox-core` for connection/session orchestration
- `vox-types` for protocol payloads and control messages
- `vox` for generated clients and service dispatchers

Part of the Vox workspace: <https://github.com/bearcove/vox>
