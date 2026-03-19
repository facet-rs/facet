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

This specification describes the current roam v9 protocol model. The v9 line
introduces a transport prologue below the conduit/session layers so conduit
mode is selected on the wire before session establishment.

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

To "handle" a call (i.e. send a response to an incoming request), or to "make" a
call (i.e. send a request to the peer, expecting a response), one needs a connection.

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
| Conduit                |  serialization, replay, session-facing continuity
+------------------------+
| Transport Prologue     |  conduit mode request / accept / reject
+------------------------+
| Link                   |  TCP, SHM, WebSocket, etc.
+------------------------+
```

The layers have distinct continuity boundaries:

- A **Link** is one concrete transport attachment.
- A **Transport Prologue** selects which conduit protocol, if any, will run on
  that link attachment.
- A **Conduit** may hide some link failures and replacement internally.
- A **Session** is above any one conduit instance and may survive conduit
  replacement.
- A **Connection** is scoped to a session, not to an individual conduit.

# Terminology: call, request attempt, response, and operation

roam uses several related terms that refer to different layers of the system.
This specification uses them consistently as follows.

A **call** is the application-level RPC invocation as seen by the programmer.
Calling a generated client method creates one call. Handling an incoming RPC
in a service implementation handles one call. A call has one terminal outcome
from the application's point of view.

A **request attempt** is one concrete wire-level delivery attempt for a call.
A request attempt is carried by a `RequestCall`, identified by a `RequestId`,
and sent on one connection. A request attempt may succeed, fail, be cancelled,
or be abandoned by attachment loss.

A **response** is the terminal reply to one request attempt. On the wire, a
response is carried by `RequestResponse` and is matched to a prior request
attempt by `RequestId`.

An **operation** is the logical RPC action across retries. An operation is
identified by `operation_id`. One call corresponds to exactly one logical
operation. That operation may be represented by one request attempt or by
multiple request attempts if retry or session recovery creates later delivery
attempts for the same operation.

In summary:

- one **call** corresponds to one **operation**
- one **operation** may have one or more **request attempts**
- each **request attempt** has at most one terminal **response**

This distinction matters for continuity:

- conduit continuity preserves **request-attempt continuity**
- session resumption preserves **session-scoped state**
- retry preserves **operation continuity**

Session resumption does not preserve in-flight request or response attempts on
the failed attachment. If an unresolved operation continues after session
resumption, it does so by creating a new request attempt for the same
operation.
