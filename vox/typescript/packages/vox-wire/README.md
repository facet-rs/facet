# @bearcove/vox-wire

Wire-format types and codecs for the Vox protocol in TypeScript.

## Role in the Vox stack

`@bearcove/vox-wire` sits at the protocol boundary between runtime logic and transport links.

## What this package provides

- Message schemas and protocol type definitions
- Wire-level encode/decode helpers
- Shared wire constants and error representations

## Fits with

- `@bearcove/vox-postcard` for low-level serialization
- `@bearcove/vox-core` for runtime behavior on decoded messages
- Transport packages that carry encoded wire frames

Part of the Vox workspace: <https://github.com/bearcove/vox>
