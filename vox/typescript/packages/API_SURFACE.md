# TypeScript Package API Surface

This document defines the intended public entrypoints for TypeScript workspace packages.

## `@bearcove/roam-core`

`@bearcove/roam-core` is the runtime/client package. Its root export is intentionally limited to:

- Connection/handshake runtime (`Connection`, `ConnectionError`, `helloExchange*`, `defaultHello`)
- Dispatcher and call plumbing (`ServiceDispatcher`, `ChannelingDispatcher`, `Caller`, `CallBuilder`, middleware types)
- Generated-code channeling surface (`Tx`, `Rx`, `channel`, `bindChannels`, descriptor types)
- Metadata helpers (`ClientMetadata`, conversions)
- RPC error helpers (`RpcError`, `RpcErrorCode`, `decodeUserError`)

Low-level channel/schema internals are not part of the curated root API.

## `@bearcove/roam-tcp`

`@bearcove/roam-tcp` is transport-focused. Its root export is intentionally limited to:

- TCP framing/transport (`LengthPrefixedFramed`, `Server`, `ConnectOptions`)
- Minimal connection surface needed by transport consumers (`Connection`, `ConnectionError`, `Negotiated`, `HelloExchangeOptions`)

Convenience re-exports of channel internals are intentionally excluded.

## Boundary Rules

Allowed cross-package imports:

- Package root: `@bearcove/<pkg>`
- Public subpaths (if added in package `exports`): `@bearcove/<pkg>/<public-subpath>`

Forbidden cross-package imports:

- Any `src` deep import, for example:
  - `@bearcove/roam-core/src/...`
  - `../roam-core/src/...`

Enforcement:

- `pnpm check` runs `scripts/check-ts-package-boundaries.mjs`, which fails on forbidden `src` imports in `typescript/packages/*`.
