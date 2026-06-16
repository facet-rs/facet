+++
title = "Connectivity"
description = "Links, conduits, connections, and lanes"
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
> Vox provides an in-memory transport via `MemoryLink`, based on tokio MPSC
> channels.

> r[transport.stream]
>
> Vox provides a stream transport via `StreamLink`, which prefixes each payload
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
> Vox provides a `LocalLink` abstraction that uses named pipes on Windows and
> Unix sockets on Linux & macOS. Endpoints/addresses are a `String` internally.

> r[transport.fd.capability]
>
> `vox::Fd` values are transport capabilities, not ordinary payload bytes. They
> may travel only over transports that explicitly support descriptor passing
> (`FdStreamLink` / Unix-domain local transports on Unix). Transports that cannot
> carry descriptors MUST reject descriptor-bearing frames with a diagnostic
> error. Generated non-Rust bindings MUST reject service surfaces containing
> `vox::Fd` instead of lowering them to generic bytes or unknown values.

> r[transport.websocket]
>
> Vox provides a WebSocket link, which sends payloads via WebSocket binary
> frames.

> r[transport.websocket.platforms]
>
> The WebSocket link MUST work on platforms where tokio works
> (e.g. `x86_64-unknown-linux-gnu`) and on `wasm32-unknown-unknown`.

> r[transport.inprocess]
>
> Vox provides an in-process link for WASM ↔ JS communication within the
> same browser tab. The Rust side sends via a `js_sys::Function` callback and
> receives via an MPSC channel fed by JS. No network is involved.

> r[transport.inprocess.platforms]
>
> The in-process link is available only on `wasm32-unknown-unknown`.

# Hosted compliance subjects

> r[hosted.subject.lifecycle]
>
> A hosted Vox compliance subject is a child process owned by the spec harness.
> The subject MUST exit promptly when its peer disconnects or the session is
> shut down. It MUST also enforce an inactivity timeout so a stalled harness
> cannot leave subject processes behind indefinitely. The harness MUST spawn
> subjects with process ownership that prevents child accumulation if a test
> exits before normal protocol shutdown completes.

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

> r[link.tx.send]
>
> Sending MUST accept one fully-owned payload buffer and enqueue exactly one
> link-level message. Backpressure is applied at the send/enqueue boundary:
> if the transport cannot currently accept another payload, the send future
> MUST wait (or error) before the payload becomes visible to the receiver.

> r[link.message.empty]
>
> Links MUST support empty payloads (`len = 0`).

> r[link.tx.alloc.limits]
>
> If a transport has a maximum payload size, sending a payload whose length
> exceeds that maximum MUST return an error.

> r[link.tx.cancel-safe]
>
> Link send MUST be cancellation-safe: canceling/dropping the send future
> MUST NOT publish a partial payload and MUST NOT leak queued capacity.

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

# Transport prologue

> r[transport.prologue]
>
> Every fresh link begins with a **transport prologue** before any
> conduit traffic is sent.

> r[transport.prologue.first-payload]
>
> The transport prologue MUST be the first payload observed on a fresh link in
> each direction. Session `Hello` / `HelloYourself` messages MUST NOT appear
> before the transport prologue has completed successfully.

> r[transport.prologue.request]
>
> The initiator sends a `TransportHello` that includes:
>
>   * a transport-prologue magic number
>   * a transport-prologue version

> r[transport.prologue.accept]
>
> The acceptor MUST reply with either:
>
>   * `TransportAccept`, acknowledging a supported transport prologue, or
>   * `TransportReject`, refusing the request

> r[transport.prologue.post-accept]
>
> After `TransportAccept`, the link is eligible for vox session
> establishment. The next payloads are the phon self-describing session
> handshake. After that handshake succeeds, subsequent session traffic is
> interpreted as `BareConduit` payloads.

> r[transport.prologue.reject-close]
>
> After `TransportReject`, the link is unusable for vox traffic and
> MUST be closed or abandoned by the peers.

# Conduits

> r[conduit]
>
> Conduits provide [phon](https://github.com/bearcove/phon)
> serialization/deserialization on top of links.

> r[conduit.typeplan]
>
> Conduits are built to serialize and deserialize _one_ type (typically an enum).
> For deserialization, conduits MUST reuse a single phon decode plan
> (`phon r[compat.plan-first]`) across items rather than re-planning on every
> message.

> r[conduit.bare]
>
> `BareConduit` does not provide any feature on top of
> serialization/deserialization. It carries post-handshake session traffic on an
> accepted link.

# Connection and lane model

> r[connection.model]
>
> A Vox connection is the authenticated, observable protocol envelope
> established over one accepted link. It owns handshake state, peer identity,
> transport evidence, keepalive, observability state, and the lifecycle of
> service lanes carried by that link.
>
> The historical implementation term `Session` refers to this same protocol
> envelope. Public APIs SHOULD use `Connection` for this object. Compatibility
> layers MAY keep internal `Session` names while migrating existing code.

> r[lane]
>
> A lane is a request, response, and channel namespace inside one connection.
> Application RPC traffic runs on lanes. Each lane has its own request ID and
> channel ID allocation state, request limits, channel credit configuration,
> schema tracking, and service-local observer context.

> r[lane.service]
>
> A service lane is bound to exactly one service namespace. If a peer needs to
> call multiple services over one connection, it MUST open multiple lanes, one
> per service. A service lane MUST NOT mix unrelated service namespaces in one
> request/channel ID namespace.

> r[lane.control]
>
> Lane ID 0 is reserved for connection-control traffic such as protocol errors,
> keepalive, and connection-level lifecycle messages. Lane ID 0 MUST NOT be
> exposed by public APIs as an application service lane, root service, generated
> caller, or liveness-only `Noop` client.

> r[lane.open]
>
> Either peer MAY request a service lane by allocating a nonzero lane ID using
> its connection parity and sending a lane-open request for the desired service.
> The lane is usable for requests only after the counterpart accepts it. Sending
> a request, response, or channel message on a lane before acceptance is a
> protocol error.

> r[lane.open.result]
>
> Lane-open rejection MUST be structured enough for callers and diagnostics to
> distinguish at least: unknown service, forbidden, not ready, draining, schema
> incompatible, and policy rejected. Discovery and rejection details SHOULD be
> filtered by authorization; a peer MUST NOT learn every implemented service
> merely because it can open a connection.

> r[lane.wire.compat]
>
> During the rootless-lane migration, implementations MAY encode lane
> open/accept/reject/close using the historical `OpenConnection`,
> `AcceptConnection`, `RejectConnection`, and `CloseConnection` message
> variants. When used this way, those messages have lane semantics, not public
> root-service semantics, and ID 0 remains the private control lane.

> r[connection.lifecycle.driven]
>
> Listening, accepted connections, and manually established connections are
> driven by explicit futures or tasks. Dropping an ordinary generated client,
> lane handle, or connection handle MUST NOT be the protocol action that stops a
> listener or accepted peer. If a connection stops because its driver future was
> never run, cancelled, or completed, diagnostics SHOULD identify the driver
> lifecycle as the cause.

> r[connection.shutdown.explicit]
>
> Graceful connection shutdown is an explicit async operation or shutdown
> signal. Dropping a public handle may release a local reference and MAY trigger
> best-effort cleanup, but it MUST NOT be the only way to perform graceful
> drain, retire, close, or peer notification.

# Sessions

> r[session]
>
> Sessions are established between two peers on top of a conduit. They keep track of
> any number of connections, on which calls (requests) can be made, and data can be
> exchanged over channels.

The transport prologue completes first. Session establishment exchanges phon
self-describing handshake messages on the accepted link. After the handshake
succeeds, the `BareConduit` carries session `Message` traffic.

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
> Every session-level protocol action is done by sending and receiving
> `Message` values.

> r[session.message.connection-id]
>
> Every message is composed of a connection identifier and a payload. The
> connection ID is meaningful for every message type except `ProtocolError`
> and keepalive (`Ping`/`Pong`), which MUST use connection ID 0.

> r[session.message.payloads]
>
> Here are all the kinds of message payloads:
>
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
>
> Schemas may be delivered inline with `Request` and `Response` payloads or
> via a standalone `SchemaMessage` binding (see `r[schema.format.delivery]`).
>
> `Hello`, `HelloYourself`, `LetsGo`, and `Sorry` are NOT message payloads.
> They are phon self-describing handshake messages exchanged before the
> phon-encoded `MessagePayload` enum is used (see `r[session.handshake]`).

> r[session.handshake]
>
> To establish a session on an accepted link, a three-step phon
> self-describing handshake MUST be performed. The handshake messages are phon
> self-describing values, NOT phon-compact `MessagePayload` variants. This is
> the bootstrap: phon's self-describing mode needs no prior schema to read
> (`phon r[self-describing.bootstraps-schemas]`), and it establishes the schema
> needed to interpret the phon-compact `MessagePayload` enum that follows.
>
> 1. The initiator sends a **`Hello`** containing:
>    - `parity`: the identifier partition desired by the initiator
>    - `connection_settings`: limits for the root connection
>    - `message_payload_schema`: the phon schema-closure bytes describing the
>      initiator's `Message` envelope and all types it references (the enum used
>      for all subsequent communication)
>
> 2. The acceptor adopts the opposite parity, builds a phon decode plan for the
>    initiator's `Message` schema, and replies with one of:
>    - **`HelloYourself`** containing:
>      - `connection_settings`: limits for the root connection
>      - `message_payload_schema`: the phon schema-closure bytes describing the
>        acceptor's `Message` envelope and all types it references
>    - **`Sorry`** if the schemas are incompatible (see `r[session.handshake.sorry]`)
>
> 3. The initiator builds a phon decode plan for the acceptor's `Message` schema
>    and replies with one of:
>    - **`LetsGo`**: confirms compatibility; the session is established
>    - **`Sorry`**: rejects the session

> r[session.handshake.phon]
>
> All handshake messages (`Hello`, `HelloYourself`, `LetsGo`, `Sorry`) MUST
> be phon self-describing values. phon's self-describing mode is tag-led and
> needs no prior schema to parse (`phon r[self-describing.tag-led]`), avoiding
> the chicken-and-egg problem of needing a schema to read a schema. After
> `LetsGo`, all subsequent communication is phon-compact `MessagePayload`
> values, decoded using phon decode plans built from the `message_payload_schema`
> closures exchanged in the handshake.

> r[session.handshake.sorry]
>
> `Sorry` MUST contain a structured description of the incompatibility:
> which variants or fields the rejecting peer requires that the other peer's
> schema does not provide. After sending or receiving `Sorry`, the session
> MUST NOT proceed and the conduit SHOULD be closed.

> r[session.handshake.protocol-schema]
>
> The `message_payload_schema` exchanged during the handshake is the phon
> schema closure for the `Message` envelope and all types it references — the
> top-level type for all post-handshake communication. Each peer builds a phon
> decode plan for the other's `Message` schema. This allows the protocol to
> evolve: peers with different versions of `Message` can communicate as long as
> phon can build a decode plan.
>
> The sender MUST NOT send a `MessagePayload` variant that the receiver's
> schema does not include. If a peer's schema is missing a variant the other
> peer requires, the handshake MUST fail with `Sorry`.

> r[session.handshake.protocol-schema.session-scoped]
>
> Protocol schemas are exchanged once per session during the handshake. They
> are immutable for the session lifetime.

> r[session.handshake.unversioned]
>
> There is no version field in `Hello`. Protocol evolution is handled entirely
> through schema exchange: each peer describes its `Message` envelope and peers
> build phon decode plans from the schema closures. If a peer's schema is
> missing a variant the other peer requires, the handshake fails with `Sorry`.

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
>   * `initial_channel_credit` — the number of items the peer grants up
>     front for each newly created channel it receives on this connection
>     (u32). This value also bounds the peer's inbound per-channel queue.

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
> When their counterpart does something that violates the vox spec, a peer MUST
> send a `ProtocolError` message describing the violation, and MUST tear down
> the entire session, including its underlying conduit and link.
>
> `ProtocolError` is always sent on connection ID 0. Sending it on another connection
> ID is itself, a protocol error.
>
> Any pending request MUST be resolved with an error indicating a protocol
> error. Any live channel MUST be put in a state where any attempt to
> send or receive returns an error indicating a protocol error.

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
> The parity of virtual connections need not be the same as the session parity.
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
generator. It uses vox RPC to communicate the host (main binary) and cells,
which implement basic functionality.

Dodeca's HTTP server is implemented as a cell: on top of serving HTML, it also
accepts new vox sessions over WebSocket connections, to serve the DevTools
service (which allows inspecting the template variables and patching the page
live when new changes are made to the Markdown, etc.).

The HTTP server cell finds itself in the middle of the host and the browser, and
has to forward calls somehow:

```aasvg
.----------------.   vox/Local .----------------.   vox/WebSocket   .----------------.
| Host           |<------------>| HTTP Server    |<------------------>| Browser        |
| (main binary)  |              | Cell           |                    | (DevTools)     |
'----------------'              '----------------'                    '----------------'
```

Instead of manually forwarding calls back to the host, the HTTP server cell can
simply open a virtual connection on its existing host session, matching the
parity that the browser peer picked when connecting over WS.

## Rust runtime API for virtual connections

The Rust runtime (`vox-core`) exposes virtual connections as first-class
session operations:

1. Create a session and keep `session.run()` running.
2. Open outbound virtual connections via `SessionHandle::open_connection(...)`.
3. Accept inbound virtual connections by registering `.on_connection(...)` on
   the session builder.

Example (open outbound):

```rust
let (mut session, root_handle, session_handle) = vox_core::session::initiator(conduit)
    .establish()
    .await?;

let mut root_driver = vox_core::Driver::new(root_handle, root_dispatcher, vox_types::Parity::Odd);
let root_caller = root_driver.caller();

let vconn_handle = session_handle
    .open_connection(
        vox_types::ConnectionSettings {
            parity: vox_types::Parity::Odd,
            max_concurrent_requests: 64,
        },
        vec![],
    )
    .await?;

let mut vconn_driver = vox_core::Driver::new(vconn_handle, vconn_dispatcher, vox_types::Parity::Odd);
let vconn_caller = vconn_driver.caller();
```

Each `ConnectionHandle` gets its own driver state, request/channel ID allocators,
and caller. This means a virtual connection can run a different dispatcher and
caller context than the root connection.

Example (accept inbound):

```rust
let (mut session, root_handle, _session_handle) = vox_core::session::acceptor(conduit)
    .on_connection(my_connection_acceptor)
    .establish()
    .await?;
```

If `.on_connection(...)` is not configured, inbound `OpenConnection` messages
are rejected.
