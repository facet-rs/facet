# @bearcove/vox-core

Core TypeScript runtime abstractions for Vox connections, calls, and channeling.

## Role in the Vox stack

`@bearcove/vox-core` implements TypeScript-side runtime behavior for links, connections, service lanes, requests, and channels.

## What this package provides

- Connection/lane abstractions for generated and hand-written clients
- Call-building and middleware-style runtime plumbing
- Channeling primitives used by higher-level transports and generated bindings

## Fits with

- `@bearcove/vox-wire` for wire message types/codecs
- `@bearcove/phon-engine` / `@bearcove/phon-schema` for serialization
- `@bearcove/vox-tcp` and `@bearcove/vox-ws` for concrete transports

Part of the Vox workspace: <https://github.com/bearcove/vox>
