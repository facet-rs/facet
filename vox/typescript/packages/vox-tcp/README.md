# @bearcove/vox-tcp

TCP transport binding for Vox in TypeScript.

## Role in the Vox stack

`@bearcove/vox-tcp` implements the `Link` layer over Node.js TCP streams.

## What this package provides

- Framing and transport adapter logic for TCP links
- Integration with `@bearcove/vox-core` connection/runtime abstractions

## Fits with

- `@bearcove/vox-core` for session/call orchestration
- `@bearcove/vox-wire` and `@bearcove/vox-postcard` for protocol payloads

Part of the Vox workspace: <https://github.com/bearcove/vox>
