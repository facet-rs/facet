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

# Transport prologue

> r[transport.prologue]
>
> Every fresh link attachment begins with a **transport prologue** before any
> conduit-specific traffic is sent.

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
>   * the requested conduit mode

> r[transport.prologue.requested-mode]
>
> The requested conduit mode is an exact request, not a preference list. This
> spec defines two conduit modes:
>
>   * `bare`
>   * `stable`

> r[transport.prologue.accept]
>
> The acceptor MUST reply with either:
>
>   * `TransportAccept`, acknowledging the requested conduit mode, or
>   * `TransportReject`, refusing the request

> r[transport.prologue.no-fallback]
>
> If the acceptor does not support the requested conduit mode, it MUST reject
> the transport prologue. It MUST NOT silently fall back to a different conduit
> mode.

> r[transport.prologue.post-accept]
>
> After `TransportAccept`, all subsequent payloads on that link attachment are
> interpreted according to the selected conduit mode.

> r[transport.prologue.reject-close]
>
> After `TransportReject`, the link attachment is unusable for vox traffic and
> MUST be closed or abandoned by the peers.

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
> `BareConduit` does not provide any feature on top of
> serialization/deserialization. It begins immediately after the transport
> prologue has accepted `bare`.

> r[conduit.stable]
>
> `StableConduit` provides automatic reconnection (over fresh links) and replay of
> missed messages. It comes with its own Packet framing.

`StableConduit` begins only after the transport prologue has accepted `stable`.
Its own stable-conduit handshake is separate from, and ordered after, the
transport prologue.

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

The transport prologue selects the conduit mode first. Session establishment
starts only after that conduit has been selected and initialized.

> r[session.outlives-conduit]
>
> A session is not owned by any one conduit attachment. A session MAY survive
> conduit failure and continue on a replacement conduit.

> r[session.resumption.runtime-managed]
>
> Session resumption is managed by the runtime. It MUST NOT require
> application-level handlers, callers, or peer-specific user code to
> collaborate in the resume protocol.

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
> Schemas are not a standalone message type. They are delivered inline
> with `Request` and `Response` payloads (see `r[schema.format.delivery]`).
>
> `Hello`, `HelloYourself`, `LetsGo`, and `Sorry` are NOT message payloads.
> They are CBOR-encoded handshake structs exchanged before the postcard
> `MessagePayload` enum is used (see `r[session.handshake]`).

> r[session.handshake]
>
> To establish a session on top of an existing conduit, a three-step CBOR
> handshake MUST be performed. The handshake messages are CBOR-encoded
> structs, NOT postcard-encoded `MessagePayload` variants. This is the
> bootstrap: it establishes the schemas needed to interpret the postcard
> `MessagePayload` enum that follows.
>
> 1. The initiator sends a **`Hello`** containing:
>    - `parity`: the identifier partition desired by the initiator
>    - `connection_settings`: limits for the root connection
>    - `message_payload_schemas`: a self-contained set of schemas describing
>      the initiator's `MessagePayload` enum and all types it references
>      (the postcard enum used for all subsequent communication)
>
> 2. The acceptor adopts the opposite parity, compares the `MessagePayload`
>    schemas, and replies with one of:
>    - **`HelloYourself`** containing:
>      - `connection_settings`: limits for the root connection
>      - `message_payload_schemas`: a self-contained set of schemas describing
>        the acceptor's `MessagePayload` enum and all types it references
>    - **`Sorry`** if the schemas are incompatible (see `r[session.handshake.sorry]`)
>
> 3. The initiator compares schemas and replies with one of:
>    - **`LetsGo`**: confirms compatibility; the session is established
>    - **`Sorry`**: rejects the session

> r[session.handshake.cbor]
>
> All handshake messages (`Hello`, `HelloYourself`, `LetsGo`, `Sorry`) MUST
> be CBOR-encoded. CBOR is self-describing and does not require a schema to
> parse, avoiding the chicken-and-egg problem of needing a schema to read a
> schema. After `LetsGo`, all subsequent communication is postcard-encoded
> `MessagePayload` values, deserialized using translation plans built from
> the schemas exchanged in the handshake.

> r[session.handshake.sorry]
>
> `Sorry` MUST contain a structured CBOR description of the incompatibility:
> which variants or fields the rejecting peer requires that the other peer's
> schema does not provide. After sending or receiving `Sorry`, the session
> MUST NOT proceed and the conduit SHOULD be closed.

> r[session.handshake.protocol-schema]
>
> The `message_payload_schemas` exchanged during the handshake are a
> self-contained set of schemas describing the `MessagePayload` enum and
> all types it references — the top-level type for all post-handshake
> communication. Each peer builds a translation plan for the other's
> `MessagePayload` schema. This allows the protocol to evolve: peers with
> different versions of `MessagePayload` can communicate as long as a
> translation plan can be built.
>
> The sender MUST NOT send a `MessagePayload` variant that the receiver's
> schema does not include. If a peer's schema is missing a variant the other
> peer requires, the handshake MUST fail with `Sorry`.

> r[session.handshake.protocol-schema.session-scoped]
>
> Protocol schemas are exchanged once per session during the handshake. They
> are immutable for the session lifetime. Transparent reconnection (via
> `StableConduit`) does not re-exchange protocol schemas. Session resumption
> (new handshake) does.

> r[session.handshake.unversioned]
>
> There is no version field in `Hello`. Protocol evolution is handled entirely
> through schema exchange: each peer describes its `MessagePayload` enum and
> peers build translation plans from the schemas. If a peer's schema is
> missing a variant the other peer requires, the handshake fails with `Sorry`.

> r[session.handshake.resume]
>
> After initial establishment, the runtime MAY bind a replacement conduit onto
> the same session. Resumption preserves session-scoped state, including the
> session's connection namespace and any operation records attached to that
> session. Protocol schemas are re-exchanged on resumption (new handshake).
>
> Session resumption preserves session-scoped state, but does not preserve
> in-flight request attempts or in-flight response deliveries on the failed
> attachment. If an unresolved operation continues after session resumption, it
> does so by creating a new request attempt for the same operation.

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

# Initial connect waiting

> r[session.initial-connect-waiting]
>
> A caller that has just spawned a local daemon or is waiting for a remote
> service to become reachable MAY request initial-connect waiting from the
> runtime. In this mode, the runtime retries failed initial connection
> attempts until a session is established or the waiting timeout expires.
>
> Initial connect waiting is distinct from session recovery. Session
> recovery applies after a session exists and its conduit fails. Initial
> connect waiting applies before any session has been established.

> r[session.initial-connect-waiting.retryable]
>
> During initial connect waiting, only transient failures MUST be retried:
> I/O errors and connect timeouts. These indicate that the service is not
> yet reachable.

> r[session.initial-connect-waiting.non-retryable]
>
> During initial connect waiting, permanent failures MUST NOT be retried and
> MUST surface immediately. Protocol errors, payload/schema incompatibilities,
> and explicit rejections indicate a fundamental mismatch that retrying will
> not resolve.

> r[session.initial-connect-waiting.backoff]
>
> The runtime MUST apply exponential backoff between retry attempts, starting
> from a small initial interval and capping at a maximum interval. This
> prevents a busy loop when the service is slow to start.

> r[session.initial-connect-waiting.timeout]
>
> Initial connect waiting is bounded by a caller-supplied timeout. If the
> timeout expires before a session is established, the runtime MUST surface
> the last retryable failure.

> r[session.initial-connect-waiting.no-session]
>
> A failed initial connect waiting attempt that never establishes a session
> MUST NOT be treated as session recovery.

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
.----------------.   vox/SHM   .----------------.   vox/WebSocket   .----------------.
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
