+++
title = "Rust Guide"
description = "How to define services, run sessions, and choose transport crates in Rust."
weight = 21
+++

Rust is the source-of-truth implementation for Roam. Service traits in Rust define the schema used for generated clients in other languages.

## Layer mapping

- Service surface: `roam`
- Runtime/session/driver: `roam-core`
- Protocol types: `roam-types`
- Transports: `roam-stream`, `roam-websocket`, `roam-shm`, `roam-local`

## Typical crate layout

- `*-proto` crate: service traits and shared types
- service crate: handlers and dispatchers
- binary crate: transport setup and runtime wiring

## Minimal flow

1. Define a service trait with `#[roam::service]`.
2. Implement the generated service trait on your handler type.
3. Build a session via `roam_core::session::{initiator,acceptor}`.
4. Create a `Driver` and use `driver.caller()` for outbound clients.

## Choosing a transport

- Use `roam-stream` for TCP/stdio/Unix stream-like links.
- Use `roam-websocket` for browser and websocket-native environments.
- Use `roam-shm` for local high-throughput shared-memory IPC.

## Upgrade notes

If you are moving from v6, read [Migrating from v6 to v7](/v6-to-v7/).
