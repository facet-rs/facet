+++
title = "Connectivity"
description = "Links, conduits, sessions, and connections"
weight = 11
+++

# Links and transports

> r[link]
>
> A link provides a reliable way to send and receive payloads (byte buffers)
> between two peers.
>
> A kind of link is called a "transport". If you use the TCP transport, then you
> establish TCP links between your peers.

> r[transport.memory]
>
> Roam provides an in-memory transport via `MemoryLink`, based on tokio MPSC
> channels.

> r[transport.stream]
>
> Roam provides a stream transport via `StreamLink`, which prefixes each payload
> with its length: a 32-bit LE unsigned integer.

> r[transport.stream.kinds]
>
> `StreamLink` must be constructible from arbitrary tokio `AsyncRead`/`AsyncWrite`
> pairs. Convenience constructors are provided for:
>
>   * TCP sockets
>   * Stdio
>   * Unix sockets
>   * Named pipes on Windows

> r[transport.stream.local]
>
> Roam provides a `LocalLink` abstraction that uses named pipes on Windows and
> Unix sockets on Linux & macOS. Endpoints/addresses are a `String` internally.

> r[transport.websocket]
>
> Roam provides a Websocket link, which sends payloads via Websocket binary
> frames.

> r[transport.websocket.platforms]
>
> The Websocket link MUST work on platforms where tokio works
> (e.g. `x86_64-unknown-linux-gnu`) and on `wasm32-unknown-unknown`.

> r[transport.inprocess]
>
> Roam provides an in-process link for WASM ↔ JS communication within the
> same browser tab. The Rust side sends via a `js_sys::Function` callback and
> receives via an MPSC channel fed by JS. No network is involved.

> r[transport.inprocess.platforms]
>
> The in-process link is available only on `wasm32-unknown-unknown`.

> r[transport.shm]
>
> Roam provides a shared memory transport. It is designed for high-performance
> IPC on a single machine.

> r[link.split]
>
> A `Link` MUST be splittable into independent transmit and receive halves.
> The halves MUST be safe to move to different tasks/threads.

> r[link.message]
>
> A link is *message-oriented*: each send corresponds to exactly one received
> payload (a byte buffer). Transports MUST preserve payload boundaries (no
> implicit concatenation or splitting of payloads).

> r[link.order]
>
> Links MUST deliver payloads reliably and in order: if payload A is committed
> before payload B on the sender, then A MUST be observed before B by the
> receiver, with no duplication.

> r[link.tx.reserve]
>
> Sending MUST be a three-step operation:
>
> 1. `reserve()` awaits until the transport can accept one more payload and
>    yields a send permit.
> 2. `permit.alloc(len)` allocates a writable slot of exactly `len` bytes backed
>    by transport-owned storage.
> 3. The caller writes into the slot and then commits it.
>
> `reserve()` is the backpressure point: it MUST wait until the transport can
> accept a payload (or error).

> r[link.tx.permit.drop]
>
> Dropping a send permit without allocating/committing MUST release the
> reservation and MUST NOT publish any payload.

> r[link.message.empty]
>
> Links MUST support empty payloads (`len = 0`).

> r[link.tx.alloc.limits]
>
> If a transport has a maximum payload size, `permit.alloc(len)` MUST return an
> error when `len` exceeds that maximum.

> r[link.tx.slot.len]
>
> A write slot returned by `permit.alloc(len)` MUST expose a writable byte slice
> of exactly length `len`.

> r[link.tx.discard]
>
> Dropping a write slot without committing MUST discard it (no bytes become
> visible to the peer) and MUST release any reserved capacity.

> r[link.tx.commit]
>
> Committing a write slot MUST publish exactly one payload whose bytes are the
> contents of the slot at the time of commit. Commit MUST be synchronous (it
> only makes already-written bytes visible to the transport/receiver).

> r[link.tx.cancel-safe]
>
> `reserve()` MUST be cancellation-safe: canceling/dropping the `reserve` future
> MUST NOT publish a partial payload and MUST NOT leak reserved capacity.

> r[link.tx.close]
>
> Links MUST support graceful close of the outbound direction. After a graceful
> close completes, the peer MUST eventually observe end-of-stream (`Ok(None)`)
> after it has received all payloads committed before the close began. A graceful
> close MUST NOT cause loss or reordering of previously committed payloads.

> r[link.rx.recv]
>
> Receiving MUST yield exactly one payload per `recv` call, as an owned backing
> buffer/handle. The received bytes MUST remain valid and immutable until the
> backing is dropped.

> r[link.rx.error]
>
> If `recv` returns an error, the link MUST be treated as dead: the receiver
> MUST NOT yield any further payloads after an error.

> r[link.rx.eof]
>
> When the peer has closed the link, `recv` MUST return `Ok(None)`. After `Ok(None)`
> is returned once, all subsequent `recv` calls MUST return `Ok(None)` as well.

# Conduits

> r[conduit]
>
> Conduits provide Postcard serialization/deserialization on top of links.
> Like links, they use a permit system for sending.

> r[conduit.typeplan]
>
> Conduits are built to serialize and deserialize _one_ type (typically an enum).
> For deserialization, conduits MUST use a `TypePlan` to avoid re-planning on every
> item.

> r[conduit.bare]
>
> `BareConduit` does not provide any feature on top of serialization/deserialization.

> r[conduit.stable]
>
> `StableConduit` provides automatic reconnection (over fresh links) and replay of
> missed messages. It comes with its own Packet framing.

`StableConduit` continuity does not, by itself, answer what happens to an RPC
whose outcome is now ambiguous. Operation-level retry and session resumption
semantics are defined in [Retry](./retry/).

> r[conduit.split]
>
> Conduits can be passed around whole, but before use, they MUST be split into
> a Sender and a Receiver.

> r[conduit.permit]
>
> A conduit's Sender MUST expose an async `reserve()` that returns a Permit.
>
> `reserve()` may "block" (be "Pending") for a while, this is how a conduit can
> apply backpressure. This method may also error out, as conduits can die.

> r[conduit.permit.send]
>
> The returned permit MUST have a synchronous `send()` function, which consumes
> the permit and enqueues the item for sending.
>
> The permit guarantees that one item can be sent — it is consumed synchronously.
> Dropping the permit returns capacity to the conduit.

Crucially, separating `.send()` from `Permit::send()` avoids losing items
in-transit by accidentally cancelling (dropping) Futures blocked in a dual-purpose
`send()`.

See tokio's [Sender::reserve](https://docs.rs/tokio/latest/tokio/sync/mpsc/struct.Sender.html#method.reserve)
documentation for more information.

# Sessions

> r[session]
>
> Sessions are established between two peers on top of a conduit. They keep track of
> any number of connections, on which calls (requests) can be made, and data can be
> exchanged over channels.

> r[session.peer]
>
> When talking about peers, the local peer is simply called "peer" and the remote
> peer is called "counterpart".

> r[session.role]
>
> Even though a session is established over an existing conduit, and therefore doesn't
> have to worry about "connecting" or "accepting connections", each peer plays a "role":
> initiator, or acceptor.

> r[session.symmetry]
>
> The role a peer plays in a session does not dictate whether they make or
> handle requests, or whether they send or receive items over channels.
> All sessions are fully bidirectional.

> r[session.message]
>
> Every operation that can be done during a session's lifecycle is done by
> sending and receiving `Message` values.

> r[session.message.connection-id]
>
> Every message is composed of a connection identifier and a payload. The
> connection ID is meaningful for every message type except for the handshake
> (`Hello` and `HelloYourself`), `ProtocolError`, and keepalive
> (`Ping`/`Pong`), all of which MUST use connection ID 0.

> r[session.message.payloads]
>
> Here are all the kinds of message payloads:
>
>   * Hello
>   * HelloYourself
>   * ProtocolError
>   * Ping
>   * Pong
>   * OpenConnection
>   * AcceptConnection
>   * RejectConnection
>   * CloseConnection
>   * Request
>   * Response
>   * CancelRequest
>   * ChannelItem
>   * CloseChannel
>   * ResetChannel
>   * GrantCredit

> r[session.handshake]
>
> To establish a session on top of an existing conduit, a handshake MUST be
> performed. The initiator sends a `Hello` message, with the version field
> set to `7`, and the parity field set to the identifier partition desired by
> the initiator.
>
> The counterpart MUST assert that the version is set to 7, adopt the opposite
> parity, and send back a `HelloYourself` message.

> r[session.parity]
>
> Parity plays a role on two different levels:
>
>   * sessions (for connection IDs)
>   * connections (for request IDs and channel IDs)
>
> The idea is to partition the identifier space so that either peer can allocate
> new identifiers without coordinating.
>
> For example, if peer Alice initiates a session with `parity` set to `Odd`,
> Alice may later open virtual connections with ID 1, 3, 5, 7, etc. whereas
> Bob may open virtual connections with ID 2, 4, 6, 8, etc.

> r[session.connection-settings]
>
> `ConnectionSettings` is a struct embedded in `Hello` (for the root
> connection) and `OpenConnection` (for virtual connections). It carries
> per-connection limits advertised by the peer:
>
>   * `max_concurrent_requests` — the maximum number of in-flight requests
>     the peer is willing to accept on this connection (u32).

> r[session.connection-settings.hello]
>
> `Hello` and `HelloYourself` each carry a `ConnectionSettings` that
> applies to the root connection. Each peer advertises its own limits.

> r[session.connection-settings.open]
>
> `OpenConnection` carries a `ConnectionSettings` from the opener.
> `AcceptConnection` carries a `ConnectionSettings` from the accepter.
> Together, they establish the limits for the virtual connection.

> r[session.protocol-error]
>
> When their counterpart does something that violates the roam spec, a peer MUST
> send a `ProtocolError` message describing the violation, and MUST tear down
> the entire session, including its underlying conduit and link.
>
> `ProtocolError` is always sent on connection ID 0. Sending it on another connection
> ID is itself, a protocol error.
>
> Any pending request MUST be resolved with an error indicating that there's been
> a protocol error. Any live channel MUST be put in a state where any attempt to
> send or recv from them MUST return an error indicating that there's been a protocol
> error.

> r[session.keepalive]
>
> Peers MAY use protocol keepalive on connection ID 0 to detect half-open or
> otherwise dead peers. Keepalive uses:
>
>   * `Ping { nonce: u64 }`
>   * `Pong { nonce: u64 }`
>
> A peer receiving `Ping` MUST reply with `Pong` carrying the same nonce.
> Implementations MAY periodically send `Ping` and treat missing `Pong` as a
> connection failure, in which case they MUST tear down the session and fail all
> pending requests with a connection-closed style error.

# Connections

> r[connection]
>
> A connection is a namespace for requests and channels inside of a session.

> r[connection.root]
>
> A session can hold many connections: it starts with one, the root connection,
> with ID 0. Trying to close the root connection is a protocol error.
>
> Each peer's parity on the root connection matches their session parity.

> r[connection.virtual]
>
> Connections that are dynamically opened in a session with identifiers strictly
> greater than 0 are called "virtual connections".

> r[connection.open]
>
> Either peer may allocate a new connection ID using its parity, and send a
> `OpenConnection` message on the desired connection ID, then wait until the
> counterpart replies with either `AcceptConnection` or `RejectConnection`. Only
> once `AcceptConnection` has been received may the peer send other messages on
> that connection.
>
> Sending `OpenConnection` with an ID that does not match the sender's session
> parity is a protocol error. Sending `OpenConnection` with an ID that is already
> in use is a protocol error.

> r[connection.open.rejection]
>
> There is no negotiated protocol-level limit on the maximum number of virtual
> connections a session may hold. Instead, peers MUST protect their own resources
> by enforcing local limits. If a counterpart attempts to open too many connections
> or if the peer lacks the resources to handle a new connection, the peer MUST
> reply with a `RejectConnection` message.

> r[connection.parity]
>
> When opening a virtual connection, a peer requests a certain parity, which impacts
> which IDs a peer may allocate for requests and channels, without coordination.
> Request IDs and channel IDs have separate namespaces within a connection.
>
> The parity of virtual connections needn't be the same as the session parity.
>
> For example, peer Alice may have session parity Odd: she might open a new
> connection with ID 13 (odd), with parity Even. Within that connection, Alice
> will send requests with ID 2, 4, 6 and channels with ID 2, 4, 6 (in their
> respective namespaces), etc.

> r[connection.close]
>
> Either peer may gracefully terminate a virtual connection by sending a
> `CloseConnection` message. After sending `CloseConnection`, a peer MUST NOT
> send any further requests, responses, or channel messages on that connection ID.

> r[connection.close.semantics]
>
> Upon receiving a `CloseConnection` message, a peer MUST treat the connection as
> immediately terminated and release its associated resources. The receiving peer
> SHOULD behave as if all in-flight requests on that connection received a
> `CancelRequest`, and it MUST treat all active channels bound to that connection
> as implicitly closed or reset. Sending any message on a connection ID after
> receiving `CloseConnection` for it is a protocol error.

The design objective is to allow proxies to map existing connections without
having to translate request IDs or channel IDs.

Case study: [dodeca](https://github.com/bearcove/dodeca) is a static site
generator. It's using roam RPC over the shared memory transport to communicate
the host (main binary) and cells, which implement basic functionality.

Dodeca's HTTP server is implemented as a cell: on top of serving HTML, it also
accepts new roam sessions over WebSocket connections, to serve the DevTools
service (which allows inspecting the template variables and patching the page
live when new changes are made to the Markdown, etc.).

The HTTP server cell finds itself in the middle of the host and the browser, and
has to forward calls somehow:

```aasvg
.----------------.   roam/SHM   .----------------.   roam/WebSocket   .----------------.
| Host           |<------------>| HTTP Server    |<------------------>| Browser        |
| (main binary)  |              | Cell           |                    | (DevTools)     |
'----------------'              '----------------'                    '----------------'
```

Instead of manually forwarding calls back to the host, the HTTP server cell can
simply open a virtual connection on its existing host session, matching the
parity that the browser peer picked when connecting over WS.

## Rust runtime API for virtual connections

The Rust v7 runtime (`roam-core`) exposes virtual connections as first-class
session operations:

1. Create a session and keep `session.run()` running.
2. Open outbound virtual connections via `SessionHandle::open_connection(...)`.
3. Accept inbound virtual connections by registering `.on_connection(...)` on
   the session builder.

Example (open outbound):

```rust
let (mut session, root_handle, session_handle) = roam_core::session::initiator(conduit)
    .establish()
    .await?;

let mut root_driver = roam_core::Driver::new(root_handle, root_dispatcher, roam_types::Parity::Odd);
let root_caller = root_driver.caller();

let vconn_handle = session_handle
    .open_connection(
        roam_types::ConnectionSettings {
            parity: roam_types::Parity::Odd,
            max_concurrent_requests: 64,
        },
        vec![],
    )
    .await?;

let mut vconn_driver = roam_core::Driver::new(vconn_handle, vconn_dispatcher, roam_types::Parity::Odd);
let vconn_caller = vconn_driver.caller();
```

Each `ConnectionHandle` gets its own driver state, request/channel ID allocators,
and caller. This means a virtual connection can run a different dispatcher and
caller context than the root connection.

Example (accept inbound):

```rust
let (mut session, root_handle, _session_handle) = roam_core::session::acceptor(conduit)
    .on_connection(my_connection_acceptor)
    .establish()
    .await?;
```

If `.on_connection(...)` is not configured, inbound `OpenConnection` messages
are rejected.
