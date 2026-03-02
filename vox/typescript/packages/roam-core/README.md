# @bearcove/roam-core

Core TypeScript runtime abstractions for Roam connections, calls, and channeling.

## Role in the Roam stack

`@bearcove/roam-core` implements TypeScript-side runtime behavior at the `Requests / Channels`, `Connections`, and `Session` layers.

## What this package provides

- Caller/connection abstractions for generated and hand-written clients
- Call-building and middleware-style runtime plumbing
- Channeling primitives used by higher-level transports and generated bindings

## Fits with

- `@bearcove/roam-wire` for wire message types/codecs
- `@bearcove/roam-postcard` for serialization
- `@bearcove/roam-tcp` and `@bearcove/roam-ws` for concrete transports

Part of the Roam workspace: <https://github.com/bearcove/roam>
