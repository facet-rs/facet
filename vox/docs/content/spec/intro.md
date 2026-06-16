+++
title = "Introduction"
description = "Formal vox RPC protocol specification"
weight = 10
+++

> r[service-macro.is-source-of-truth]
>
> vox is a **Rust-native** RPC protocol. There is no independent schema language;
> Rust traits *are* the schema. Implementations for other languages (Swift,
> TypeScript, etc.) are generated from Rust definitions.

This specification describes the current protocol model. Every fresh link
begins with a transport prologue below the conduit/connection layers so an
incompatible peer is rejected before Vox connection establishment.

## Defining a service

An application named `fantastic` would typically define services in `*-proto`
crates. If it has only one, the `fantastic-proto` crate would contain something
like:

```rust
#[vox::service]
pub trait Adder {
    /// Load a template by name.
    async fn add(&self, l: u32, r: u32) -> u32;
}
```

Proto crates are meant to only contain types and trait definitions (as much as
possible, modulo orphan rules) so that they may be joined with vox codegen to
generate client and server code for Swift and TypeScript.

All types that occur as arguments or in return position must implement the
`Facet` trait, from the [facet](https://docs.rs/facet) crate.

## Implementing a service

Given an `Adder` trait, the `vox::service` proc macro generates an
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

vox supports various transports, like memory, TCP and other sockets, and
WebSocket; but a vox connection sits several layers above a "TCP connection".

```aasvg
+------------------------+
| Requests / Channels    |  RPC calls and streaming data
+------------------------+
| Service Lanes          |  service-local request/channel namespace
+------------------------+
| Connection             |  peer identity, handshake, lanes, keepalive
+------------------------+
| Conduit                |  phon serialization over a link
+------------------------+
| Transport Prologue     |  vox protocol/version gate
+------------------------+
| Link                   |  TCP, WebSocket, etc.
+------------------------+
```

The layers have distinct failure boundaries:

- A **Link** is one concrete transport connection.
- A **Transport Prologue** validates that the peer is speaking a compatible
  vox transport protocol on that link.
- A **Conduit** is a `BareConduit` bound to one link. It does not hide link
  failure, reconnect, replay, or preserve in-flight request attempts.
- A **Connection** runs above one `BareConduit` and ends when that conduit
  fails. The historical implementation term for this protocol envelope is
  "session".
- A **Lane** is a service-local request/channel namespace inside one
  connection.

# Terminology: call, operation, request scope, request attempt, and response

vox uses several related terms that refer to different layers of the system.
This specification uses them consistently as follows.

A **call** is the application-level RPC invocation as seen by the programmer.
Calling a generated client method creates one call. Handling an incoming RPC
in a service implementation handles one call. A call has one terminal outcome
from the application's point of view.

An **operation** is an optional logical unit of work above request scopes. It
can group replacement request attempts for retry, resume, idempotency, or
distributed tracing. In the common case, one call creates one operation and one
request scope.

A **request scope** is the runtime owner of one request attempt, the response
when present, channels introduced by that request, request-local progress, and
observer/debug context. Raw channels introduced by a request are live only while
that request scope is still in flight. Delivering the response makes the request
scope terminal, so a method that wants to keep raw channels live must keep the
request open until those channels are terminal.

A **request attempt** is one concrete wire-level delivery attempt for a call.
A request attempt is carried by a `RequestCall`, identified by a `RequestId`,
and sent on one service lane. A request attempt may succeed, fail, be
cancelled, or be abandoned by lane or connection failure.

A **response** is the terminal reply to one request attempt and the terminal
event for its request scope. On the wire, a response is carried by
`RequestResponse` and is matched to a prior request attempt by `RequestId`.

In summary:

- one **call** usually creates one **operation**
- one **operation** may create one or more **request scopes**
- each **request scope** owns one **request attempt**
- each **request attempt** has at most one **response**

This distinction matters for failure handling:

- raw channels are scoped to the **request scope** that introduced them
- lane or connection failure abandons in-flight **request attempts**
- the conduit and connection layers never reconnect, retry, or replay a request
  attempt

A caller that wants to retry after failure does so through operation-level
policy with a fresh request scope and request attempt.
