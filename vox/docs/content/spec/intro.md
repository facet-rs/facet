+++
title = "Introduction"
description = "Formal roam RPC protocol specification"
weight = 10
+++

> r[service-macro.is-source-of-truth]
>
> roam is a **Rust-native** RPC protocol. There is no independent schema language;
> Rust traits *are* the schema. Implementations for other languages (Swift,
> TypeScript, etc.) are generated from Rust definitions.

If you're upgrading an existing codebase, read the
[v6 -> v7 migration guide](../v6-to-v7/).

## Defining a service

An application named `fantastic` would typically define services in `*-proto`
crates. If it has only one, the `fantastic-proto` crate would contain something
like:

```rust
#[roam::service]
pub trait Adder {
    /// Load a template by name.
    async fn add(&self, l: u32, r: u32) -> u32;
}
```

Proto crates are meant to only contain types and trait definitions (as much as
possible, modulo orphan rules) so that they may be joined with roam codegen to
generate client and server code for Swift and TypeScript.

All types that occur as arguments or in return position must implement the
`Facet` trait, from the [facet](https://docs.rs/facet) crate.

## Implementing a service

Given an `Adder` trait, the `roam::service` proc macro generates an
`Adder` trait:

```rust
#[derive(Clone)]
struct AdderHandler;

impl Adder for AdderHandler {
    /// Add two numbers.
    async fn add(&self, l: u32, r: u32) -> u32 {
        l + r
    }
}
```

## Consuming a service

The proc macro also generates a `{ServiceName}Client` struct, which provides the
same async methods:

```rust
// Make a call
let response = client.add(3, 5).await.unwrap();
assert_eq!(response, 8);
```

But how do you obtain a client?

# The connectivity stack

To "handle" a call (ie. send a response to an incoming request), or to "make" a
call (ie. send a request to the peer, expecting a response), one needs a connection.

roam supports various transports, like memory, TCP and other sockets, WebSocket,
shared memory; but a roam connection sits several layers above a "TCP connection".

```aasvg
+------------------------+
| Requests / Channels    |  RPC calls and streaming data
+------------------------+
| Connections            |  request/channel ID namespace
+------------------------+
| Session                |  set of connections over a conduit
+------------------------+
| Conduit                |  serialization, reconnection
+------------------------+
| Link                   |  TCP, SHM, WebSocket, etc.
+------------------------+
```

The layers have distinct continuity boundaries:

- A **Link** is one concrete transport attachment.
- A **Conduit** may hide some link failures and replacement internally.
- A **Session** is above any one conduit instance and may survive conduit
  replacement.
- A **Connection** is scoped to a session, not to an individual conduit.
