# roam-core

Core runtime for sessions, drivers, conduits, and connection orchestration.

## Role in the Roam stack

`roam-core` primarily implements the `Session`, `Connections`, and `Conduit` layers.

## What this crate provides

- Session builders (`session::initiator`, `session::acceptor`) and handles
- Driver runtime for dispatching inbound calls and issuing outbound calls
- Connection lifecycle primitives and runtime glue

## Fits with

- `roam` for service-facing APIs
- Link/transport crates (`roam-stream`, `roam-websocket`, `roam-shm`, `roam-local`)
- `roam-types` for protocol state and message types

Part of the Roam workspace: <https://github.com/bearcove/roam>
