# @bearcove/roam-ws

WebSocket transport binding for Roam in TypeScript.

## Role in the Roam stack

`@bearcove/roam-ws` implements the `Link` layer over WebSocket binary frames.

## What this package provides

- WebSocket transport adapters with reconnecting utilities
- Integration with `@bearcove/roam-core` runtime abstractions

## Fits with

- `@bearcove/roam-core` for connection/session runtime behavior
- `@bearcove/roam-tcp` where shared transport logic is reused
- `@bearcove/roam-wire` for protocol message payloads

Part of the Roam workspace: <https://github.com/bearcove/roam>
