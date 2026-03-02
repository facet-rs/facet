# roam-websocket

WebSocket transport binding for Roam links.

## Role in the Roam stack

`roam-websocket` implements the `Link` layer on top of WebSocket binary frames.

## What this crate provides

- Client/server link adapters over WebSocket transports
- Native and wasm-friendly runtime integration points

## Fits with

- `roam-core` for connection/session orchestration
- `roam-types` for protocol payloads and control messages
- `roam` for generated clients and service dispatchers

Part of the Roam workspace: <https://github.com/bearcove/roam>
