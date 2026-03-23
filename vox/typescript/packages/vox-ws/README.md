# @bearcove/vox-ws

WebSocket transport binding for Vox in TypeScript.

## Role in the Vox stack

`@bearcove/vox-ws` implements the `Link` layer over WebSocket binary frames.

## What this package provides

- WebSocket transport adapters with reconnecting utilities
- Integration with `@bearcove/vox-core` runtime abstractions

## Fits with

- `@bearcove/vox-core` for connection/session runtime behavior
- `@bearcove/vox-tcp` where shared transport logic is reused
- `@bearcove/vox-wire` for protocol message payloads

Part of the Vox workspace: <https://github.com/bearcove/vox>
