# vox-core

Core runtime for sessions, drivers, conduits, and connection orchestration.

## Role in the Vox stack

`vox-core` primarily implements the `Session`, `Connections`, and `Conduit` layers.

## What this crate provides

- Session builders (`session::initiator`, `session::acceptor`) and handles
- Driver runtime for dispatching inbound calls and issuing outbound calls
- Connection lifecycle primitives and runtime glue

## Fits with

- `vox` for service-facing APIs
- Link/transport crates (`vox-stream`, `vox-websocket`, `vox-shm`, `vox-local`)
- `vox-types` for protocol state and message types

Part of the Vox workspace: <https://github.com/bearcove/vox>
