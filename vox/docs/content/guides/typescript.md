+++
title = "TypeScript Guide"
description = "How TypeScript runtime packages and generated clients fit into the Roam protocol stack."
weight = 23
+++

TypeScript support combines generated service bindings with runtime packages under `typescript/packages/`.

## Layer mapping

- Core runtime: `@bearcove/roam-core`
- Wire model/codecs: `@bearcove/roam-wire`
- Serialization: `@bearcove/roam-postcard`
- Transports: `@bearcove/roam-tcp`, `@bearcove/roam-ws`

## Typical flow

1. Define services in Rust.
2. Generate TypeScript bindings from descriptors.
3. Wire generated clients to runtime transport packages.
4. Run TypeScript and cross-language compliance tests.

## Transport choice

- Node/server TCP use cases: prefer `@bearcove/roam-tcp`.
- Browser or websocket infrastructures: prefer `@bearcove/roam-ws`.

## Versioning guidance

Keep generated packages and runtime packages on aligned major versions with the Rust protocol release.
