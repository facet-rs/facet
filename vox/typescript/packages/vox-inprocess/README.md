# @bearcove/vox-inprocess

In-process transport binding for Vox in TypeScript.

## Role in the Vox stack

`@bearcove/vox-inprocess` implements the `Link` layer without sockets, so two Vox peers can exchange binary frames inside the same JavaScript process.

## What this package provides

- `InProcessLink`, a `@bearcove/vox-core` `Link` implementation
- Message queueing and close propagation between paired in-process endpoints
- A transport useful for tests, browser/WASM integration, and embedded peer setups

## Fits with

- `@bearcove/vox-core` for connection/lane/call orchestration
- `@bearcove/vox-wire` for protocol payloads
- `@bearcove/phon-engine` / `@bearcove/phon-schema` for serialization

Part of the Facet workspace: <https://github.com/facet-rs/facet>
