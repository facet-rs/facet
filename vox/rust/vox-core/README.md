# vox-core

Core runtime for connections, lanes, drivers, and conduits.

## Role in the Vox stack

`vox-core` primarily implements the `Connection`, `Lane`, and `Conduit` layers.

## What this crate provides

- Connection builders (`initiator`, `acceptor`) and handles
- Driver runtime for dispatching inbound calls and issuing outbound calls
- Connection lifecycle primitives and runtime glue

## Fits with

- `vox` for service-facing APIs
- Link/transport crates (`vox-stream`, `vox-websocket`, `vox-local`)
- `vox-types` for protocol state and message types

Part of the Vox workspace: <https://github.com/bearcove/vox>
