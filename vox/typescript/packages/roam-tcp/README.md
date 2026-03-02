# @bearcove/roam-tcp

TCP transport binding for Roam in TypeScript.

## Role in the Roam stack

`@bearcove/roam-tcp` implements the `Link` layer over Node.js TCP streams.

## What this package provides

- Framing and transport adapter logic for TCP links
- Integration with `@bearcove/roam-core` connection/runtime abstractions

## Fits with

- `@bearcove/roam-core` for session/call orchestration
- `@bearcove/roam-wire` and `@bearcove/roam-postcard` for protocol payloads

Part of the Roam workspace: <https://github.com/bearcove/roam>
