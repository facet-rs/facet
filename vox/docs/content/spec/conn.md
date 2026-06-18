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

Vox implementations may provide package-specific transports beyond the core
link contract. Rust exposes an in-memory `MemoryLink` for tests and local
wiring. Rust and TypeScript expose WebSocket links. Rust and TypeScript also
expose an in-process browser transport for WASM ↔ JS communication within the
same tab. These package surfaces are useful, but they are not part of the core
Connection/Lane/RequestScope runtime contract.

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

# Hosted compliance subjects

> r[hosted.subject.lifecycle]
>
> A hosted Vox compliance subject is a child process owned by the spec harness.
> The subject MUST exit promptly when its peer disconnects or the connection is
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
> each direction. Connection handshake `Hello` / `HelloYourself` messages MUST
> NOT appear before the transport prologue has completed successfully.

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
> After `TransportAccept`, the link is eligible for Vox connection
> establishment. The next payloads are the phon self-describing connection
> handshake. After that handshake succeeds, subsequent connection traffic is
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
> serialization/deserialization. It carries post-handshake connection traffic on
> an accepted link.

# Connection and lane model

> r[connection.model]
>
> A Vox connection is the authenticated, observable protocol envelope
> established over one accepted link. It owns handshake state, peer identity,
> transport evidence, keepalive, observability state, and the lifecycle of
> service lanes carried by that link.
>
> Public APIs and normative specification text use `Connection` for this
> object. Compatibility module names that still contain older vocabulary are not
> part of the public protocol model.

# Connection identity, evidence, and policy

Vox separates peer-authored metadata from facts asserted by the local transport
or embedding runtime. This lets the same connection policy model apply to mTLS,
Unix sockets, named pipes, XPC, in-process transports, and memory transports
without pretending that all of those paths have the same security evidence.

> r[connection.evidence]
>
> **Transport evidence** is locally asserted information about the counterpart
> or about the path used to create the accepted link. Examples include a
> verified TLS/mTLS certificate, an ALPN result, Unix peer credentials,
> platform process identity, an XPC audit token, an in-process component
> identity, or a synthetic identity supplied by a test transport.
>
> Transport evidence is not ordinary request, response, or lane metadata. A
> value sent by the counterpart MUST NOT become transport evidence merely
> because it appears in metadata or in a payload.
>
> A value surfaced by a transport is evidence only to the extent that the local
> transport, kernel, cryptographic stack, or embedding runtime asserted or
> verified it. Peer-authored values that arrive through a transport, such as
> SNI, Host, Origin, path, query parameters, WebSocket upgrade headers, cookies,
> bearer tokens, or forwarded identity headers, are claims rather than
> transport evidence. An ALPN result is evidence for the locally observed
> negotiated protocol; the peer-influenced values riding beside it are not.
>
> Evidence MUST be constructible only by trusted transport, platform, runtime,
> embedding, or test harness code. Application payloads, metadata, service
> handlers, and policy callbacks MUST NOT be able to fabricate locally asserted
> evidence.

> r[connection.identity.inputs]
>
> Identity resolution consumes distinct input classes:
>
>   * transport/runtime evidence asserted locally;
>   * counterpart-authored claims carried in handshake metadata, lane metadata,
>     request metadata, or payloads;
>   * local configuration and verifier output.
>
> Implementations MUST keep the trust boundary between these input classes
> visible. A bearer token, username, service name, socket address, or other
> counterpart-authored value is a claim until local policy verifies it. Verifier
> output may contribute to peer identity, but the original claim MUST NOT be
> exposed as locally asserted transport evidence.

> r[connection.identity.resolver]
>
> The **identity resolver** runs at the connection establishment policy
> boundary, after transport/runtime evidence for the accepted link is available
> and before the connection is established. Vox core provides the pipeline and
> typed policy context; the application, embedding runtime, or transport
> integration supplies the actual verifier logic.
>
> The resolver returns either a connection-scoped peer identity or a connection
> policy rejection. Identity resolution and establishment policy MUST complete
> during the handshake: the acceptor resolves the initiator before sending
> `HelloYourself`, and the initiator resolves the acceptor before sending
> `LetsGo`. `Decline` is the structured carrier for establishment policy
> rejection. The resolver MUST NOT be hidden inside ordinary metadata handling,
> generated service dispatch, or individual method handlers.
>
> The resolver MUST NOT depend on opening an application service lane on the
> connection being established. Resolver work is on the connection
> establishment path, so implementations SHOULD make resolver diagnostics and
> time spent in the resolver observable.
>
> This handshake provides no multi-round authentication exchange. Challenge
> response, credential refresh, and re-authentication are not connection
> establishment semantics in this version; they require a future explicit
> protocol or service-level authorization state.

> r[connection.identity]
>
> A connection's **peer identity** is the local peer's policy-resolved view of
> its counterpart for this Vox connection. Peer identity is computed from
> transport evidence, verified counterpart claims, local configuration, and
> policy. Lane and request authorization consume this resolved identity rather
> than interpreting raw transport details directly.
>
> A connection MUST have an identity before application service lanes are
> accepted. If no identity resolver is configured, the default identity is
> anonymous/unauthenticated, optionally accompanied by whatever transport
> evidence the link supplied.

> r[connection.identity.late-claims]
>
> Claims that first appear after connection identity has been resolved, such as
> credentials in lane metadata or request metadata, MAY be verified by lane or
> request authorization policy. Such verifier output can affect that lane or
> request, but it never becomes transport evidence. Connection identity is
> immutable for the lifetime of that identity epoch. A connection has exactly
> one identity epoch in this version. A future re-authentication protocol, if
> defined, MUST create an explicit new identity epoch and MUST NOT
> retroactively reinterpret lanes, requests, channels, logs, or observer events
> that belonged to an earlier epoch.
>
> Expiring or revoked credentials presented after establishment are handled by
> lane authorization, request authorization, lane-grant revocation, or
> connection teardown. They MUST NOT mutate or downgrade the connection
> identity in place.

> r[connection.identity.forms]
>
> Public APIs SHOULD preserve enough structure for policy code to distinguish
> common identity forms without string parsing:
>
>   * anonymous or unauthenticated counterpart;
>   * synthetic test identity;
>   * local process or platform identity;
>   * certificate-backed identity;
>   * application/user identity produced by a local verifier;
>   * composite identity that records more than one verified basis.
>
> The exact platform-specific payloads are implementation-defined, but their
> trust boundary MUST remain visible.
> Composite identities MUST preserve per-basis provenance so policy can
> distinguish evidence-backed identity bases from verified-claim-backed identity
> bases. Synthetic and test identities are distinguishable identity forms and
> are not implicitly privileged.

> r[connection.identity.use-cases]
>
> The identity model MUST be able to represent the following cases without
> treating peer-authored metadata as transport evidence:
>
>   * a WAN service authenticated by TLS or mTLS evidence;
>   * a server-authenticated TLS connection whose client identity is established
>     by a locally verified token claim;
>   * a WebSocket or TCP connection whose application user is established by a
>     locally verified token or credential claim available during connection
>     establishment;
>   * a deployment behind a trusted local frontend, load balancer, sidecar, or
>     proxy where forwarded peer data is a claim verified against the frontend's
>     own evidence-backed identity and local policy;
>   * a gateway or on-behalf-of service where the connection identity is the
>     gateway and downstream users, tenants, or capabilities are represented as
>     lane-scoped or request-scoped claims;
>   * a Unix-socket peer identified by local peer credentials such as UID, GID,
>     PID, or platform-provided process information;
>   * a macOS XPC peer identified by audit-token and code-signing evidence;
>   * an in-process, FFI, or shared-library peer identified by the embedding
>     runtime;
>   * a memory/test peer identified by synthetic test evidence.

> r[connection.identity.local]
>
> Peer identity is local to one side of the connection. Each peer resolves its
> counterpart independently. A counterpart's self-description, socket address,
> service name, lane metadata, or connection role MUST NOT be treated as
> authoritative identity unless local policy explicitly verifies and accepts it.

> r[connection.identity.scope]
>
> Peer identity is connection-scoped by default. Implementations MUST NOT assume
> that two identities observed on different connections refer to the same
> principal unless the identity form includes a stable verified principal and
> policy defines equality for that form.

> r[connection.identity.redaction]
>
> Evidence and identity may contain secrets, personal data, host paths,
> certificate material, process identifiers, or other sensitive values.
> Observers and logs MUST expose redacted/debug forms by default. Raw evidence
> MAY be available to policy code, but it MUST NOT be used as a default metric
> label or emitted into ordinary logs without an explicit opt-in.

> r[connection.policy.establishment]
>
> Connection policy MAY reject a connection after transport evidence is
> available and before the connection is established. When rejection happens
> after Vox can send handshake messages, the rejecting peer MUST send `Decline`
> before abandoning the link. The outward reason MAY be redacted or less
> specific than the local policy decision.
>
> A rejected connection MUST also surface a local diagnostic that distinguishes
> policy rejection, missing or unconfigured policy, transport failure, protocol
> failure, and ordinary graceful shutdown. If the transport or platform security
> layer fails before Vox can send any protocol response, a local diagnostic is
> sufficient.

> r[connection.policy.establishment.rejection]
>
> Structured establishment rejection MUST carry a typed reason, not a free-form
> string. It MUST represent at least unauthenticated, forbidden, not ready,
> draining, unsupported, and policy rejected. It MAY carry metadata or a
> human-readable diagnostic message subject to redaction. Authentication and
> authorization failures MUST NOT rely on untyped string matching for
> programmatic behavior.
>
> `TransportReject` remains available for transport-prologue refusal before the
> Vox handshake can be interpreted. `Sorry` is reserved for handshake schema or
> protocol compatibility failure (see `r[connection.handshake.sorry]`) and
> SHOULD NOT be used for policy rejection once a structured establishment
> rejection carrier is available.
>
> When an acceptor rejects establishment policy, it sends `Decline` in place of
> `HelloYourself`. When an initiator rejects establishment policy, it sends
> `Decline` in place of `LetsGo`. If policy rejection and compatibility failure
> are both applicable, the rejecting peer SHOULD choose the less revealing
> outward reason while preserving precise local diagnostics.

> r[rejection.reason.taxonomy]
>
> Vox rejection carriers use typed reasons so callers and diagnostics do not
> depend on message text:
>
> | Layer | Carrier | Required reason space |
> | --- | --- | --- |
> | Transport prologue | `TransportReject` | Transport-prologue reasons such as unsupported prologue. |
> | Connection establishment | `Decline` | `Unauthenticated`, `Forbidden`, `NotReady`, `Draining`, `Unsupported`, `PolicyRejected`. |
> | Service lane open | `LaneReject` | `UnknownService`, `Forbidden`, `NotReady`, `Draining`, `SchemaIncompatible`, `PolicyRejected`. |
>
> Implementations MAY add more specific reasons, but every reason MUST map to a
> stable typed value and to one of the required reason categories for
> cross-language behavior.

> r[lane.authorization]
>
> Opening a service lane is an authorization point. The accepting peer decides
> whether to accept or reject the lane-open request using the resolved
> connection identity, available evidence, requested service namespace, lane
> metadata, local readiness, resource limits, and policy.
>
> While a lane-open request is pending, the lane is not usable for application
> requests. If policy refuses the lane, the rejecting peer MUST send a
> structured lane-open rejection.
>
> Either peer may open lanes, regardless of which side initiated the underlying
> link. Lane authorization always uses the accepting peer's local view of its
> counterpart identity and the lane-open metadata it received.

> r[lane.authorization.context]
>
> Lane authorization MAY produce a lane-scoped authorization context, also
> called a lane grant. The lane grant records the authority, tenant, scope,
> readiness, resource limits, or verifier output that the accepting peer wants
> subsequent requests on that lane to inherit.
>
> A lane grant is bound to the accepted lane and remains associated with that
> lane until the lane closes, the connection closes, or local policy explicitly
> revokes it. If policy revokes a lane grant, the implementation MUST make the
> revocation observable and MUST either proactively close the lane with a
> structured reason, or leave the lane open but reject subsequent requests with
> an authorization error. Revocation MUST NOT retroactively reinterpret requests
> that were already authorized under the previous grant.
>
> Closing a lane to enforce grant revocation may terminate in-flight requests
> according to the lane-close semantics. Closing a granted lane is an
> observable lane-grant revocation for that lane. That is operational
> termination, not a retroactive authorization reversal.

> r[lane.authorization.filtered]
>
> Discovery and rejection details are subject to authorization. The protocol
> MUST be able to represent unknown service, forbidden, not ready, draining,
> schema incompatible, and policy rejected, but local policy MAY choose a less
> revealing outward reason when exposing the more precise reason would disclose
> service existence or sensitive policy details.

> r[request.authorization]
>
> Implementations MAY authorize individual requests after a lane has opened.
> Request authorization consumes the connection identity, lane/service identity,
> lane authorization context if any, method identity, request metadata, lane
> policy state, and local readiness. Request metadata can carry credentials or
> policy inputs, but those values are peer-authored until a local verifier
> validates them.

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
> exposed by public APIs as an application service lane, generated caller, or
> liveness-only `Noop` client.

> r[lane.open]
>
> Either peer MAY request a service lane by allocating a nonzero lane ID using
> its connection parity and sending a lane-open request for the desired service.
> The lane is usable for requests only after the counterpart accepts it. Sending
> a request, response, or channel message on a lane before acceptance is a
> protocol error.

> r[lane.open.result]
>
> Lane-open rejection MUST be structured enough to represent at least: unknown
> service, forbidden, not ready, draining, schema incompatible, and policy
> rejected. Discovery and rejection details SHOULD be filtered by authorization;
> a peer MUST NOT learn every implemented service merely because it can open a
> connection.

> r[lane.wire]
>
> Lane open, accept, reject, and close are connection message payloads:
> `LaneOpen`, `LaneAccept`, `LaneReject`, and `LaneClose`. These messages have
> lane semantics, not connection-establishment semantics. ID 0 remains the
> private control lane.

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

# Connection Handshake And Lane Wire Terms

> r[connection.protocol]
>
> A Vox connection is established between two peers on top of a conduit. The
> connection keeps track of service lanes, on which calls (requests) can be made
> and data can be exchanged over channels.

The transport prologue completes first. Connection establishment exchanges phon
self-describing handshake messages on the accepted link. After the handshake
succeeds, the `BareConduit` carries connection `Message` traffic.

> r[connection.peer]
>
> When talking about peers, the local peer is simply called "peer" and the remote
> peer is called "counterpart".

> r[connection.role]
>
> Even though a Vox connection is established over an existing conduit, each peer
> still plays a connection-establishment role: initiator or acceptor.

> r[connection.symmetry]
>
> The role a peer plays during connection establishment does not dictate whether
> they make or
> handle requests, or whether they send or receive items over channels.
> Vox connections are fully bidirectional.

> r[connection.message]
>
> Every connection-level protocol action is done by sending and receiving
> `Message` values.

> r[connection.message.lane-id]
>
> Every message is composed of a lane identifier and a payload. Nonzero lane IDs
> identify service lanes. Connection-control payloads such as `ProtocolError`
> and keepalive (`Ping`/`Pong`) MUST use control lane ID 0.

> r[connection.message.payloads]
>
> Here are all the kinds of connection message payloads:
>
>   * ProtocolError
>   * LaneOpen
>   * LaneAccept
>   * LaneReject
>   * LaneClose
>   * RequestMessage
>   * SchemaMessage
>   * ChannelMessage
>   * Ping
>   * Pong
>
> `RequestMessage` contains request call, response, and cancellation bodies.
> `ChannelMessage` contains channel item, close, reset, and credit-grant
> bodies. Schemas may be delivered inline with request/response bodies or via a
> standalone `SchemaMessage` binding (see `r[schema.format.delivery]`).
>
> `Hello`, `HelloYourself`, `LetsGo`, `Decline`, and `Sorry` are NOT message
> payloads. They are phon self-describing handshake messages exchanged before
> the phon-encoded `MessagePayload` enum is used (see
> `r[connection.handshake]`).

> r[connection.handshake]
>
> To establish a Vox connection on an accepted link, a three-step phon
> self-describing handshake MUST be performed. The handshake messages are phon
> self-describing values, NOT phon-compact `MessagePayload` variants. This is
> the bootstrap: phon's self-describing mode needs no prior schema to read
> (`phon r[self-describing.bootstraps-schemas]`), and it establishes the schema
> needed to interpret the phon-compact `MessagePayload` enum that follows.
>
> 1. The initiator sends a **`Hello`** containing:
>    - `parity`: the identifier partition desired by the initiator
>    - `connection_settings`: default lane limits and control lane ID 0 limits
>    - `message_payload_schema`: the phon schema-closure bytes describing the
>      initiator's `Message` envelope and all types it references (the enum used
>      for all subsequent communication)
>    - `metadata`: early peer-authored metadata for connection-establishment
>      claims and connection extensions; a peer presenting no early metadata MAY
>      encode it as unit/null
>
> 2. The acceptor adopts the opposite parity, builds a phon decode plan for the
>    initiator's `Message` schema, and replies with one of:
>    - **`HelloYourself`** containing:
>      - `connection_settings`: default lane limits and control lane ID 0
>        limits
>      - `message_payload_schema`: the phon schema-closure bytes describing the
>        acceptor's `Message` envelope and all types it references
>      - `metadata`: early peer-authored metadata for connection-establishment
>        claims and connection extensions; a peer presenting no early metadata
>        MAY encode it as unit/null
>    - **`Decline`** if the acceptor's identity resolver or connection policy
>      refuses establishment
>    - **`Sorry`** if the schemas are incompatible (see `r[connection.handshake.sorry]`)
>
> 3. The initiator builds a phon decode plan for the acceptor's `Message` schema
>    and replies with one of:
>    - **`LetsGo`**: confirms compatibility; the connection is established
>    - **`Decline`**: refuses establishment due to connection policy, for
>      example because the initiator's identity resolver rejected the acceptor's
>      identity or claims
>    - **`Sorry`**: rejects the connection due to schema compatibility

> r[connection.handshake.metadata]
>
> Handshake metadata is peer-authored metadata carried by `Hello` and
> `HelloYourself` before application service lanes are accepted. It may carry
> early credential claims, routing hints, tracing context, or extension data for
> connection establishment. It is not transport evidence. Authentication
> claims in handshake metadata are claims until the local identity resolver
> verifies them.
>
> Observers and logs MUST treat handshake metadata as sensitive by default and
> MUST NOT render its values without redaction, independent of whether keys use
> the sensitive-key sigil from `r[rpc.metadata.sigils]`. Metadata sigils remain
> useful for values that are safe to render or forward under local policy, but
> handshake metadata starts from a default-sensitive posture because it is the
> early-claims carrier.

> r[connection.handshake.phon]
>
> All handshake messages (`Hello`, `HelloYourself`, `LetsGo`, `Decline`,
> `Sorry`) MUST be phon self-describing values. phon's self-describing mode is
> tag-led and needs no prior schema to parse (`phon r[self-describing.tag-led]`),
> avoiding the chicken-and-egg problem of needing a schema to read a schema.
> After `LetsGo`, all subsequent communication is phon-compact `MessagePayload`
> values, decoded using phon decode plans built from the
> `message_payload_schema` closures exchanged in the handshake.

> r[connection.handshake.decline]
>
> `Decline` is the structured handshake carrier for connection-establishment
> policy rejection. It MUST contain a structured reason, not a free-form string,
> and MAY contain metadata or a human-readable diagnostic message subject to
> redaction. It is used when Vox can speak the handshake but local policy
> refuses establishment, such as unauthenticated, forbidden, not ready,
> draining, unsupported, or policy rejected cases.
> After sending or receiving `Decline`, the connection MUST NOT proceed and the
> conduit SHOULD be closed.

> r[connection.handshake.sorry]
>
> `Sorry` MUST contain a structured description of the incompatibility:
> which variants or fields the rejecting peer requires that the other peer's
> schema does not provide. After sending or receiving `Sorry`, the connection
> MUST NOT proceed and the conduit SHOULD be closed.
> `Sorry` is for schema or handshake protocol compatibility failure, not
> authentication or authorization policy rejection.

> r[connection.handshake.protocol-schema]
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

> r[connection.handshake.protocol-schema.connection-scoped]
>
> Protocol schemas are exchanged once per connection during the handshake. They
> are immutable for the connection lifetime.

> r[connection.handshake.unversioned]
>
> There is no version field in `Hello`. Protocol evolution is handled entirely
> through schema exchange: each peer describes its `Message` envelope and peers
> build phon decode plans from the schema closures. If a peer's schema is
> missing a variant the other peer requires, the handshake fails with `Sorry`.

> r[connection.lane-id-parity]
>
> Parity plays a role on two different levels:
>
>   * the connection (for lane IDs)
>   * lanes (for request IDs and channel IDs)
>
> The idea is to partition the identifier space so that either peer can allocate
> new identifiers without coordinating.
>
> For example, if peer Alice initiates a connection with `parity` set to `Odd`,
> Alice may later open service lanes with ID 1, 3, 5, 7, etc. whereas Bob may
> open service lanes with ID 2, 4, 6, 8, etc.

> r[lane.settings]
>
> `ConnectionSettings` is embedded in `Hello` and `HelloYourself` for
> connection-default lane limits and control lane ID 0 limits, and in
> `LaneOpen` and `LaneAccept` for service lanes. It carries per-lane limits
> advertised by the peer:
>
>   * `max_concurrent_requests` — the maximum number of in-flight requests
>     the peer is willing to accept on this lane (u32).
>   * `initial_channel_credit` — the number of items the peer grants up
>     front for each newly created channel it receives on this lane
>     (u32). This value also bounds the peer's inbound per-channel queue.

> r[connection.handshake.lane-settings]
>
> `Hello` and `HelloYourself` each carry a `ConnectionSettings` that
> supplies connection-default lane limits and control lane ID 0 limits. Each
> peer advertises its own limits.

> r[lane.open.settings]
>
> `LaneOpen` carries a `ConnectionSettings` from the lane opener.
> `LaneAccept` carries a `ConnectionSettings` from the accepter. Together,
> they establish the limits for the service lane.

> r[connection.protocol-error]
>
> When their counterpart does something that violates the vox spec, a peer MUST
> send a `ProtocolError` message describing the violation, and MUST tear down
> the entire connection, including its underlying conduit and link.
>
> `ProtocolError` is always sent on control lane ID 0. Sending it on another
> lane ID is itself a protocol error.
>
> Any pending request MUST be resolved with an error indicating a protocol
> error. Any live channel MUST be put in a state where any attempt to
> send or receive returns an error indicating a protocol error.

> r[connection.keepalive]
>
> Peers MAY use protocol keepalive on control lane ID 0 to detect half-open or
> otherwise dead peers. Keepalive uses:
>
>   * `Ping { nonce: u64 }`
>   * `Pong { nonce: u64 }`
>
> A peer receiving `Ping` MUST reply with `Pong` carrying the same nonce.
> Implementations MAY periodically send `Ping` and treat missing `Pong` as a
> connection failure, in which case they MUST tear down the connection and fail
> all pending request scopes with a connection-closed style error.

# Lanes And Wire IDs

> r[lane.id]
>
> Each nonzero lane ID identifies a service lane with its own request and
> channel namespaces. Lane ID 0 is reserved for connection-control traffic.

> r[lane.control]
>
> ID 0 is reserved for connection-control traffic. Trying to close ID 0 as an
> application lane is a protocol error. ID 0 MUST NOT be exposed as a public
> service-bearing lane or generated caller.

> r[lane.service]
>
> IDs strictly greater than 0 identify service lanes.

> r[lane.open.wire]
>
> Either peer may allocate a new nonzero lane ID using its connection parity and
> send a `LaneOpen` message on the desired lane ID, then wait until the
> counterpart replies with either `LaneAccept` or `LaneReject`. Only once
> `LaneAccept` has been received may the
> peer send request, response, or channel messages on that lane.
>
> Sending `LaneOpen` with an ID that does not match the sender's
> connection parity is a protocol error. Sending `LaneOpen` with an ID
> that is already in use is a protocol error.

> r[lane.open.wire.rejection]
>
> There is no negotiated protocol-level limit on the maximum number of service
> lanes a connection may hold. Instead, peers MUST protect their own resources by
> enforcing local limits. If a counterpart attempts to open too many lanes, lacks
> authorization, requests an unavailable service, or if the peer lacks the
> resources to handle a new lane, the peer MUST reply with a `LaneReject`
> message.

> r[lane.request-channel-parity]
>
> When opening a service lane, a peer requests a request/channel parity for that
> lane. Request IDs and channel IDs have separate namespaces within a lane.
>
> The parity of service lanes need not be the same as the connection parity.
>
> For example, peer Alice may have connection parity Odd: she might open a new
> lane with ID 13 (odd), with lane parity Even. Within that lane, Alice will send
> requests with ID 2, 4, 6 and channels with ID 2, 4, 6 (in their respective
> namespaces), etc.

> r[lane.close]
>
> Either peer may gracefully terminate a nonzero service lane by sending a
> `LaneClose` message. After sending `LaneClose`, a
> peer MUST NOT send any further requests, responses, or channel messages on
> that lane ID.

> r[lane.close.semantics]
>
> Upon receiving a `LaneClose` message, a peer MUST treat the lane as
> immediately terminated and release its associated resources. The receiving peer
> SHOULD behave as if all in-flight request scopes on that lane received a
> `CancelRequest`, and it MUST make all active raw channels bound to that lane
> terminal with a lane-closed reason. Sending any message on a lane ID after
> receiving `LaneClose` for it is a protocol error.

The design objective is to allow lane-aware proxies to route service-lane
traffic without having to translate request IDs or channel IDs. The
connection/lane model keeps the useful namespace separation while treating
lanes as scoped service contexts inside one Vox connection.

Case study: [dodeca](https://github.com/bearcove/dodeca) is a static site
generator. It uses vox RPC to communicate the host (main binary) and cells,
which implement basic functionality.

Dodeca's HTTP server is implemented as a cell: on top of serving HTML, it also
accepts new Vox connections over WebSocket links, to serve the DevTools service
(which allows inspecting the template variables and patching the page live when
new changes are made to the Markdown, etc.).

The HTTP server cell finds itself in the middle of the host and the browser, and
has to forward calls somehow:

```aasvg
.----------------.   vox/Local .----------------.   vox/WebSocket   .----------------.
| Host           |<------------>| HTTP Server    |<------------------>| Browser        |
| (main binary)  |              | Cell           |                    | (DevTools)     |
'----------------'              '----------------'                    '----------------'
```

This is the forwarding requirement that service lanes preserve: the HTTP server
cell can ask the host-side connection to create another request/channel
namespace, then route browser traffic through that namespace without translating
request IDs or channel IDs.

That topology should be described in terms of service lanes or in terms of a
lower-level transport/topology that creates another Vox connection. Vox core
represents service traffic with explicit lanes: ID 0 remains connection
control, and every public service endpoint lives on an explicit lane.

## Current Rust runtime API

1. Establish a `Connection` and keep its driver future running.
2. Open outbound service lanes via `ConnectionHandle::open_lane(...)` or
   `ConnectionHandle::open_lane_handle(...)`.
3. Accept inbound service lanes by registering an inbound lane acceptor on the
   connection builder.

Each `LaneHandle` is a service-lane handle: it gets its own driver state,
request/channel ID allocators, dispatcher, and caller context.
The public API teaches this as "open or accept a service lane on an
explicitly driven Vox connection".

If no inbound lane acceptor is configured, inbound `LaneOpen` messages are
rejected.
