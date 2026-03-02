# @bearcove/roam-wire

Wire-format types and codecs for the Roam protocol in TypeScript.

## Role in the Roam stack

`@bearcove/roam-wire` sits at the protocol boundary between runtime logic and transport links.

## What this package provides

- Message schemas and protocol type definitions
- Wire-level encode/decode helpers
- Shared wire constants and error representations

## Fits with

- `@bearcove/roam-postcard` for low-level serialization
- `@bearcove/roam-core` for runtime behavior on decoded messages
- Transport packages that carry encoded wire frames

Part of the Roam workspace: <https://github.com/bearcove/roam>
