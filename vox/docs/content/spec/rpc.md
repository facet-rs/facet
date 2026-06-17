+++
title = "RPC Concepts"
description = "Services, method identity, handlers, and callers"
weight = 12
+++

> r[rpc]
>
> The RPC layer sits on top of connections. It defines how requests are made,
> how responses are returned, and how data flows over channels.

> r[rpc.service]
>
> A service is a set of methods. In Rust, a service is defined as a trait
> annotated with `#[vox::service]`. Methods only take `&self` — a service
> does not carry mutable state. Any state must be managed externally
> (e.g. behind an `Arc<Mutex<_>>` or similar).

> r[rpc.service.methods]
>
> Each method in a service is an async function. Its arguments and return
> type must implement `Facet`. The `#[vox::service]` macro generates a
> `{ServiceName}` trait. Method shape depends on whether the success return
> type borrows with explicit `'vox`:
>
>   * Owned success return: method returns the value (`T` or `Result<T, E>`)
>   * Borrowed `'vox` success return: method receives
>     `call: impl vox::Call<Ok, Err>` and replies explicitly
>
> This preserves borrowed-reply support while keeping owned-return handlers
> ergonomic.

> r[rpc.method-id]
>
> Every method has a unique 64-bit identifier derived from its service name
> and method name (see `r[schema.method-id]`). The signature is not included —
> schema exchange handles type evolution. This is what gets sent on the wire
> in `Request` messages.

> r[rpc.method-id.no-collisions]
>
> The method ID ensures that different services can have methods with the same
> name without collision.

> r[rpc.method-id.algorithm]
>
> The exact algorithm for computing method IDs is defined by
> `r[schema.method-id]`. Other language implementations receive
> pre-computed method IDs from code generation.

> r[rpc.schema-evolution]
>
> Adding new methods to a service is always safe — peers that don't know about
> a method simply report it as unknown.
>
> Renaming a service or method is a breaking change (the method ID changes).
>
> Changing argument types, return types, or the structure of types used in
> method signatures may or may not be breaking, depending on whether the
> phon compatibility decode plan can bridge the difference (see the
> [schema exchange specification](../schemas/) for details on decode plans
> and compatibility rules).

> r[rpc.handler]
>
> A handler handles incoming requests on a service lane. It is a user-provided
> implementation of a service trait. The vox runtime takes care of deserializing
> arguments, routing to the right method, and sending back responses.

> r[rpc.caller]
>
> A caller makes outgoing requests on a service lane. It is a generated struct
> (e.g. `AdderClient`) that provides async methods matching the source trait
> signature, and takes care of serialization and response handling internally.
> Successful responses include both return data and response metadata.

> r[rpc.caller.liveness.refcounted]
>
> Runtime caller handles for a given service lane MAY share refcounted local
> state. Cloning a caller keeps that local state reachable; dropping a caller
> releases only that local reference. Caller handle liveness is not a protocol
> signal and MUST NOT be the mechanism that performs graceful lane close,
> connection drain, or peer notification.

> r[rpc.caller.liveness.last-drop-closes-connection]
>
> Despite this historical requirement name, dropping the last public caller for
> a nonzero lane MUST NOT automatically close that lane as if the local peer
> requested a graceful close. Dropping the last caller releases local resources
> for that handle graph; graceful lane close requires an explicit async close,
> retire, or drain operation.

> r[rpc.caller.liveness.root-internal-close]
>
> There is no public root caller in the rootless model. Dropping public handles
> MUST NOT internally close ID 0. ID 0 is connection-control traffic and is
> closed only by explicit connection shutdown, connection drain completion,
> protocol error, or connection failure.

> r[rpc.caller.liveness.root-teardown-condition]
>
> Connection teardown is driven by the connection driver, explicit shutdown or
> drain policy, protocol error, or underlying transport/link failure. It MUST NOT
> be driven by generated caller liveness.

> r[rpc.session-setup]
>
> When establishing a Vox connection, the runtime MUST make the connection
> driver explicit enough that user code can keep it running and observe its
> terminal outcome. Service handlers and generated clients attach to service
> lanes, not to a public root caller on ID 0.

> r[rpc.virtual-connection.accept]
>
> When a counterpart requests a service lane, the accepting peer receives the
> lane metadata, decides whether to accept it, and receives a lane handle in its
> acceptance callback. A generated client for that lane is created from that
> lane's caller context.

> r[rpc.virtual-connection.open]
>
> A peer may open a service lane on an existing Vox connection, receiving a lane
> handle when the counterpart accepts it. Compatibility APIs may still spell this
> `SessionHandle::open_connection(...)` and return a `ConnectionHandle`; those
> names refer to service-lane state in the rootless model.
>
> Inbound service lanes are accepted only when a lane acceptor is configured;
> otherwise they are rejected.

# Requests and responses

> r[rpc.request]
>
> A `Request` message carries:
>
>   * A request ID, unique within the service lane, allocated by the caller
>     using the lane's parity
>   * A method ID (see `r[rpc.method-id]`)
>   * Serialized arguments
>   * A list of channel IDs for channels that appear in the arguments,
>     allocated by the caller
>   * Metadata (key-value pairs for tracing, auth, deadlines, etc.)

> r[rpc.response]
>
> A `Response` message carries:
>
>   * The request ID of the request being responded to
>   * The serialized return value
>   * Metadata

> r[rpc.request.scope]
>
> A request scope is the runtime owner of one request attempt. It includes the
> request message, response message when present, every raw channel introduced
> by that request, cancellation state, request-local progress events, and
> observer/debug context for that work.
>
> Raw channel and request-local progress activity are part of the in-flight
> request scope. A response MUST NOT be delivered while raw channels introduced
> by that request, or explicit request-local progress state, are still live.
> Delivering a response makes the request scope terminal.

> r[rpc.request.scope.terminal]
>
> A request scope becomes terminal when either:
>
>   * it fails before a successful response is delivered;
>   * it is cancelled;
>   * its lane or connection is lost; or
>   * its response has been delivered.
>
> Once a request scope is terminal, no further request, response, raw channel,
> cancellation, credit, or progress event may be associated with it.

> r[rpc.request.scope.channels]
>
> Raw channels MUST NOT outlive the request scope that introduced them and MUST
> NOT remain live after the request response is delivered. If a request scope
> becomes terminal because of response delivery, failure, cancellation, lane
> closure, connection loss, or protocol error, implementations MUST make every
> associated raw channel terminal with a reason that preserves the terminal
> condition.
>
> A handler that needs to keep sending or receiving on a raw channel must keep
> the request scope in flight until that raw channel is terminal. A handler that
> needs an important value stream after a method result MUST expose that stream
> as an explicit service-level resource, handle, or demand protocol rather than
> as a raw Vox channel.
>
> Retry, resume, durable delivery, and detached streams are outside raw channel
> semantics. A higher-level service may define those protocols explicitly, but
> raw `Tx<T>` and `Rx<T>` endpoints are owned only by the request scope that
> introduced them.

> r[rpc.timeout.idle-progress]
>
> Idle timeout policy applies to request scopes. While a request scope is in
> flight, request-associated activity that may reset an idle timer includes
> request acceptance, channel item delivery, channel close/reset, channel credit
> that proves receiver-side consumption, explicit request progress,
> cancellation, drain/retire transitions, and response delivery.
>
> Connection keepalive, unrelated logs, and spans that are not associated with
> the request scope MUST NOT count as request progress.

> r[rpc.request.id-allocation]
>
> Request IDs are allocated by the caller using the lane's parity. Sending a
> `Request` with an ID that does not match the caller's parity, or reusing an
> ID that is still in flight on that lane, is a protocol error.

> r[rpc.unknown-method]
>
> If a handler receives a request with a method ID it does not recognize,
> it MUST send an error response indicating the method is unknown.
> This is a call-level error, not a protocol error: the lane and connection
> remain open.

# Fallible methods

> r[rpc.fallible]
>
> A service method may return `T` (infallible) or `Result<T, E>` (fallible),
> where both `T` and `E` implement `Facet`.

> r[rpc.fallible.caller-signature]
>
> On the Rust caller side, generated client methods return `Result<_, VoxError<E>>`
> and do not expose response metadata:
>
>   * Infallible `fn foo() -> T` becomes
>     `fn foo() -> Result<R, VoxError>`
>   * Fallible `fn foo() -> Result<T, E>` becomes
>     `fn foo() -> Result<R, VoxError<E>>`
>
> Where `R` depends on whether return payload `T` borrows from response bytes:
>
>   * If `T` uses explicit `'vox` borrows, `R = SelfRef<T>`
>   * Otherwise, `R = T`
>
> Borrowed return payloads MUST use explicit `'vox`. Other lifetimes in return
> payloads are rejected by the Rust service macro.
>
> For `Result<T, E>`, `E` MUST be owned (no lifetimes) in Rust generated clients.

> r[rpc.fallible.vox-error]
>
> `VoxError<E>` distinguishes application errors from protocol-level errors:
>
>   * `User(E)` — the handler ran and returned an application error
>   * `UnknownMethod` — no handler recognized the method ID
>   * `InvalidPayload` — the arguments could not be deserialized
>   * `Cancelled` — the call was cancelled before completion
>   * `Indeterminate` — the runtime could not safely determine whether the
>     request attempt reached a terminal outcome

> r[rpc.fallible.vox-error.outcome]
>
> `VoxError` variants distinguish terminal call outcomes from connection
> interruptions:
>
>   * **Terminal call outcome** — the handler or protocol reached a definite
>     outcome for this call: `User`, `UnknownMethod`, `InvalidPayload`,
>     `Cancelled`
>   * **Connection interruption** — the connection ended before this call
>     received a terminal response. Current APIs may spell this as
>     `ConnectionClosed`, `SessionShutdown`, or `SendFailed` while migration is
>     in progress.
>   * **Indeterminate** — the runtime cannot safely determine whether the
>     request attempt reached a terminal outcome
>
> Vox runtimes MUST NOT automatically replay or resume RPC calls after any
> `VoxError`. Applications that recover after a connection interruption or
> indeterminate outcome must establish the recovery policy themselves and issue
> any replacement call explicitly.

> r[rpc.error.scope]
>
> Call errors affect only that call. The lane and connection remain open and
> other in-flight requests are unaffected.

# Channels

> r[rpc.channel]
>
> A channel is a unidirectional, ordered sequence of typed values between
> two peers. At the type level, `Tx<T>` and `Rx<T>` indicate direction and
> element type. `T` is the element type. Initial credit and buffering are
> runtime channel settings (see `r[rpc.flow-control.credit.initial]`), not
> type parameters. Each channel has exactly one sender and one receiver.

> r[rpc.channel.direction]
>
> `Tx<T>` means "I send" and `Rx<T>` means "I receive", where "I" is
> whoever holds the handle. Position determines who holds it:
>
>   * In arg position (handler holds): `Tx<T>` = handler sends → caller,
>     `Rx<T>` = handler receives ← caller.

> r[rpc.channel.placement]
>
> `Tx<T>` and `Rx<T>` may appear only as direct arguments of service
> methods. They MUST NOT appear in method return types or in the error
> variant of a `Result` return type.

> r[rpc.channel.direct-args]
>
> `Tx<T>` and `Rx<T>` MUST NOT be nested inside structs, enums, tuples,
> `Option`, `Result`, pointers, or other container/wrapper types used as
> method arguments.

> r[rpc.channel.no-collections]
>
> `Tx<T>` and `Rx<T>` MUST NOT appear inside collections (lists,
> arrays, maps, sets).

> r[rpc.channel.allocation]
>
> Channel IDs are allocated using the lane's parity. The caller allocates IDs
> for channels that appear in the request arguments.

> r[rpc.channel.lifecycle]
>
> Channels are created as part of a request. The request is the channel's
> allocation and association scope: it supplies the channel IDs and the
> request/service/method context used for observability and diagnostics.
>
> Endpoint ownership controls channel lifetime only while the request scope is
> live. Associated channels must be terminal before, or as part of, response
> delivery. Returning a terminal request failure, cancelling the request scope,
> closing the lane, losing the connection, hitting a protocol error, or
> delivering the response MUST make associated raw channels terminal; they do
> not become connection-owned resources.
>
> Raw channels are request sidebands, not durable streams. Protocols that need
> durable, resumable, or independently demanded stream values MUST model those
> values above raw channels, for example as service-level handles plus requests
> for particular items or byte ranges.
>
> If the runtime allocates or binds channel state for a request but fails
> before handing the corresponding endpoint to user code, it MUST tear down
> that local channel state instead of leaving an orphaned channel.

> r[rpc.channel.item]
>
> A `ChannelItem` message carries a channel ID and a serialized value of
> the channel's element type.

> r[rpc.channel.delivery.reliable]
>
> Once a `ChannelItem` has been accepted by a reliable `Tx::send`, the local
> runtime MUST NOT drop it because an internal receive queue is full. Internal
> queue capacity is backpressure: the receiving runtime MUST preserve accepted
> items and terminal channel messages in order, or report channel/lane/connection
> closure. Lossy application policy belongs above this layer through APIs such
> as `Tx::try_send`.
>
> This is an in-scope delivery guarantee only. It does not make raw channels
> stable across request response, lane close, connection loss, reconnect, or
> process death.

> r[rpc.channel.connection-closure]
>
> If the underlying connection terminates while a channel receiver is still
> live and no channel `Close` or `Reset` has been delivered, the receiver MUST
> observe connection closure as an error rather than a graceful channel EOF. If
> only the service lane terminates, the receiver MUST observe lane closure as an
> error rather than graceful EOF.

> r[rpc.channel.close]
>
> The sender of a channel sends `CloseChannel` when it is done sending.
> After sending `CloseChannel`, the sender MUST NOT send any more
> `ChannelItem` messages on that channel.

> r[rpc.channel.reset]
>
> The receiver of a channel sends `ResetChannel` to ask the sender to
> stop sending. After receiving `ResetChannel`, the sender MUST stop
> sending `ChannelItem` messages on that channel.

# Flow control

> r[rpc.flow-control]
>
> Vox provides backpressure at two levels: request pipelining limits and
> per-channel credit-based flow control.

## Request limits

> r[rpc.flow-control.max-concurrent-requests]
>
> Each service lane has two independent directional request limits: one for
> request attempts sent by the local peer, and one for request attempts
> sent by the counterpart. Each peer advertises the maximum number of
> concurrent request attempts it is willing to accept on that lane.
>
> A peer's advertised `max_concurrent_requests` limits how many concurrent
> request attempts the other peer may send on that lane.

> r[rpc.flow-control.max-concurrent-requests.outbound]
>
> A peer MUST NOT send a new request attempt if doing so would exceed the
> counterpart's advertised `max_concurrent_requests` for that lane.

> r[rpc.flow-control.max-concurrent-requests.inbound]
>
> If a peer receives a request attempt that exceeds its own advertised
> `max_concurrent_requests` for that lane, it MUST treat that as a
> protocol violation.

> r[rpc.flow-control.max-concurrent-requests.counting]
>
> `max_concurrent_requests` counts live request attempts. A later call issued
> after an earlier request attempt failed consumes its own unit of request
> concurrency while the later request attempt is live.

> r[rpc.flow-control.max-concurrent-requests.session-failure]
>
> Request-attempt accounting is lane-local. When the conduit, link, or Vox
> connection fails, in-flight request attempts on every lane of that connection
> are no longer live. The conduit layer MUST NOT reconnect, preserve, replay, or
> retransmit those attempts. A later call requires a new connection or a
> still-live existing connection and consumes its own fresh lane request
> concurrency while that request attempt is live.

> r[rpc.flow-control.max-concurrent-requests.default]
>
> The default limit is carried in `ConnectionSettings`, which is embedded
> in `Hello` (for connection defaults and compatibility control/root lane
> settings) and `OpenConnection` (for service lanes). See
> `r[session.connection-settings]`.

## Channel credit

> r[rpc.flow-control.credit]
>
> Channels use item-based credit for flow control. The receiver of a
> channel controls how many items the sender may send by granting credit.
> The sender MUST NOT send a `ChannelItem` if it has zero credit. Each
> sent `ChannelItem` consumes one unit of credit.

> r[rpc.flow-control.credit.initial]
>
> Initial credit is negotiated/configured per lane as
> `ConnectionSettings.initial_channel_credit`. When a channel is created
> (as part of a request), the sender starts with the receiver's advertised
> initial credit for that lane, so the sender can transmit immediately
> without waiting one RTT for the first `GrantCredit`. The receiver provisions
> a bounded inbound queue of the same size for that channel. The default
> initial channel credit is 16 items. Implementations MAY expose configuration
> for this value.

> r[rpc.flow-control.credit.initial.high-level]
>
> High-level connection and serving APIs SHOULD expose the same initial channel
> credit / channel capacity configuration as lower-level connection builders.
> The configured value MUST be applied to the connection-default lane settings
> advertised during the connection handshake and to service-lane settings unless
> a lane explicitly overrides it.

> r[rpc.flow-control.credit.initial.zero]
>
> Zero initial channel credit is invalid for negotiated connection settings and
> public channel-capacity configuration. Implementations MUST reject zero before
> advertising it in a handshake or accepting it from a peer, because a zero
> receive queue also leaves the sender with no initial credit and no item can be
> sent to trigger credit replenishment.

> r[rpc.flow-control.credit.grant]
>
> The receiver of a channel sends a `GrantCredit` message to add credit.
> `GrantCredit` carries a lane ID (historically named connection ID on the
> wire), a channel ID, and an `additional` count (u32). The sender's available
> credit increases by `additional`.
> The receiver MAY send `GrantCredit` at any time after the channel exists.

> r[rpc.flow-control.credit.grant.additive]
>
> Credit is strictly additive. There is no mechanism to revoke granted
> credit. The receiver controls flow by choosing when and how much credit
> to grant.

> r[rpc.flow-control.credit.exhaustion]
>
> When the sender's credit reaches zero, it MUST stop sending `ChannelItem`
> messages on that channel until more credit is granted. The sender SHOULD
> apply backpressure to the producing code (e.g. by blocking a `send()`
> call) rather than buffering unboundedly.

> r[rpc.flow-control.credit.try-send]
>
> A nonblocking channel send MUST NOT wait for credit or transport queue
> capacity. If sending would block because no credit or local queue capacity is
> available, it MUST fail with `Full(value)` and return ownership of the value.
> If the channel is terminal or its lane or underlying connection is closed, it
> MUST fail with `Closed(value)` and return ownership of the value.

# Runtime observability

> r[rpc.observability.runtime]
>
> Implementations SHOULD expose a runtime observer interface that can receive
> local introspection events without imposing a dependency on any metrics,
> tracing, or telemetry backend. Observer events are not wire protocol and MUST
> NOT affect interoperability.

> r[rpc.observability.channel]
>
> Channel observers SHOULD report channel open, send, try-send, credit,
> receive, consume, close, and reset events with local channel IDs and
> directions. These IDs are suitable for logs and debug snapshots, but MUST
> NOT be used as default metric labels.

> r[rpc.observability.channel.context]
>
> Channel observer events and debug snapshots SHOULD include the lane ID
> (historically named connection ID on the wire) and best-effort local debug
> context for each channel when available. This context SHOULD include the
> request ID, service, and method that introduced the channel, and SHOULD remain
> available after the request scope becomes terminal. Rust implementations
> SHOULD capture source location and payload type context for locally created
> channel pairs.

> r[rpc.debug.snapshot]
>
> Implementations SHOULD expose a local runtime debug snapshot API that inspects
> in-process runtime state directly rather than sending requests over the
> flow-controlled Vox channel path. Snapshots SHOULD include connection,
> lane, request, channel, flow-control, and runtime queue/task state when
> available. Channel snapshots SHOULD preserve the request/service/method
> association that introduced the channel, plus whether that request scope is
> waiting for a response, succeeded, failed, cancelled, lane-closed, or
> connection-lost when known.

> r[rpc.transport.stream.cancel-safe-recv]
>
> Stream transport receive operations exposed to connection runtime loops MUST
> be cancellation-safe. If a connection driver stops polling `LinkRx::recv`
> because another runtime branch wins selection, any partially-read
> length-prefixed frame MUST continue to completion in transport-owned state
> rather than corrupting the next receive attempt.

> r[rpc.observability.channel.try-send-detail]
>
> Channel try-send observer outcomes SHOULD distinguish credit exhaustion,
> runtime queue saturation, unbound handles, and closed channels even when the
> public API collapses blocking cases into `TrySendError::Full(value)`.

> r[rpc.observability.driver]
>
> Driver observers SHOULD report connection lifecycle, lane lifecycle, request
> lifecycle, outbound runtime queue saturation/closure, encode/decode failures,
> and protocol violations. Lane IDs, connection IDs, and request IDs are
> suitable for local debugging but MUST NOT be used as default metric labels.

> r[rpc.observability.session-errors]
>
> Connection receive errors from the conduit or transport MUST be surfaced as
> runtime diagnostics and connection close reasons. Implementations MUST NOT
> collapse decode, protocol, or transport receive failures into an ordinary
> graceful shutdown.

> r[rpc.observability.establishment]
>
> Runtime observers SHOULD report establishment events for the layers that
> exist on a given transport path: endpoint resolution, TCP or Unix socket or
> named-pipe or in-process link creation, TLS or platform security handshake,
> WebSocket upgrade, Vox transport prologue, connection handshake, schema
> decode-plan construction, and service-lane open/accept/reject. Observers MUST
> NOT invent TCP, TLS, or WebSocket phases for transports that do not have them.

> r[rpc.observability.low-cardinality]
>
> Metrics derived from observer events MUST use low-cardinality labels such as
> service, method, side, outcome, error kind, and channel direction. Request
> IDs, lane IDs, connection IDs, channel IDs, peer addresses, and metadata
> values MUST NOT be used as metric labels by default.

# Cancellation

> r[rpc.cancel]
>
> A caller may send `CancelRequest` to indicate it is no longer interested
> in the response. The handler SHOULD stop processing the request, but
> a response may still arrive — the caller MUST be prepared to ignore it.

> r[rpc.cancel.channels]
>
> Cancelling a request transitions its request scope to a terminal cancelled
> state. Implementations MUST make every raw channel introduced by that request
> terminal with a cancellation reason. Cancellation MUST NOT be reported to
> channel receivers as graceful EOF.

# Pipelining

> r[rpc.pipelining]
>
> Multiple requests MAY be in flight simultaneously on a service lane. Each
> request is independent; a slow or failed request MUST NOT block other
> requests on the same lane.

# Metadata

> r[rpc.metadata]
>
> Requests and Responses carry **metadata**: a self-describing phon `Value` map
> from UTF-8 string keys to arbitrary `Value`s, for out-of-band information such
> as tracing context, authentication tokens, or deadlines. Because metadata is a
> self-describing `Value`, it is not nominally typed and does not participate in
> schema exchange (see `r[schema.interaction.metadata]`).

> r[rpc.metadata.value]
>
> A metadata value is any phon self-describing `Value` (`phon r[value]`) —
> commonly a `String`, a `Bytes` buffer, or a `U64`, but lists and nested maps
> are also valid. A peer carrying no metadata MAY encode it as the unit/null
> `Value` rather than an empty map.

> r[rpc.metadata.keys]
>
> Metadata keys are case-sensitive UTF-8 strings. By convention, application
> keys use lowercase kebab-case (e.g. `authorization`, `trace-parent`,
> `request-deadline`).

> r[rpc.metadata.sigils]
>
> A metadata key MAY carry handling conventions directly in the key string:
> `#key` marks the value sensitive for log/trace rendering, `-key` marks the
> entry as no-propagate for code that intentionally forwards metadata, and
> `-#key` applies both conventions. Implementations MUST preserve the full key
> string on the wire; there is no separate flag map or metadata-specific wire
> type.

> r[rpc.metadata.duplicates]
>
> Metadata is a map: each key appears at most once. To associate multiple values
> with a single key, use a list `Value`.

> r[rpc.metadata.unknown]
>
> Unknown metadata keys MUST be ignored — they MUST NOT cause errors
> or protocol violations.

### Examples

Build a metadata map and mark an authentication token sensitive (and
non-propagating) with the `-#` key sigils:

```rust
let metadata = vox_types::metadata()
    .str("trace-id", "abc123")
    .u64("attempt", 2)
    .str("-#authorization", "Bearer sk-...")
    .build();
```

On the wire this is a `Value` map `{ "trace-id": "abc123", "attempt": 2,
"-#authorization": "Bearer sk-..." }`.

# Channel binding

> r[rpc.channel.discovery]
>
> Channel IDs in `Request.channels` MUST be listed in the order produced by a
> left-to-right scan of the direct method arguments. Since channels are
> rejected anywhere below a direct argument position (see
> `r[rpc.channel.direct-args]`), implementations do not perform recursive
> channel discovery over user structs, enum variants, options, or collections.

> r[rpc.channel.payload-encoding]
>
> `Tx<T>` and `Rx<T>` values in the serialized payload MUST be encoded as a
> `u32` *index* into the `channels` list of the `Request` message (in encode
> walk-order). The actual channel IDs are carried out-of-band in that
> `channels` field. Carrying an explicit index — rather than relying on
> position — keeps re-association correct under the same field reordering and
> skipping the compatibility path already allows for the rest of the payload.

> r[rpc.channel.binding]
>
> On the callee side, implementations MUST resolve each decoded `Tx<T>`/`Rx<T>`
> handle's channel ID by looking up its encoded index in `Request.channels`,
> and use that ID as authoritative when binding the stream. The channel IDs in
> `Request.channels` are authoritative over any value implied by payload position.

## Channel pairs and shared state

> r[rpc.channel.pair]
>
> `channel<T>()` returns a linked `(Tx<T>, Rx<T>)` pair for one logical
> unidirectional channel. Before binding, neither endpoint has a channel ID,
> element codec, or transport binding. Runtime channel capacity defaults to
> 16 items unless the connection or lane is configured otherwise.

> r[rpc.channel.pair.binding-propagation]
>
> When the framework binds a channel handle that is part of a pair
> (created via `channel()`), it MUST propagate the channel ID, element codec,
> and send/receive binding needed by the paired handle that the caller or
> callee kept. This allows the framework to bind both ends by touching only
> the handle that appears in the args.

## Caller-side binding (args)

> r[rpc.channel.binding.caller-args]
>
> When the caller sends a request containing channel handles in the
> arguments, the framework iterates direct channel arguments in method
> declaration order, allocates a channel ID for each, and binds the handle in
> the args tuple. Channel IDs are collected into `Request.channels`.

> r[rpc.channel.binding.caller-args.rx]
>
> For an `Rx<T>` in arg position: the handler will receive, so the
> caller must send. The framework allocates a channel ID and creates
> a sink (via `ChannelBinder::create_tx`). The sink is stored in the
> shared core so the caller's paired `Tx<T>` can send through it.

> r[rpc.channel.binding.caller-args.tx]
>
> For a `Tx<T>` in arg position: the handler will send, so the caller
> must receive. The framework allocates a channel ID and creates a
> receiver (via `ChannelBinder::create_rx`). The receiver is stored
> in the shared core so the caller's paired `Rx<T>` can receive from it.

## Callee-side binding (args)

> r[rpc.channel.binding.callee-args]
>
> When the callee receives a request, channel handles in the deserialized
> arguments are standalone (not part of a pair). The framework iterates direct
> channel arguments in method declaration order and binds each handle directly
> using the channel IDs from `Request.channels`.

> r[rpc.channel.binding.callee-args.rx]
>
> For an `Rx<T>` in arg position: the handler receives. The framework
> calls `ChannelBinder::register_rx` with the channel ID to register
> the channel for routing and stores the receiver directly in the
> `Rx`'s receiver slot.

> r[rpc.channel.binding.callee-args.tx]
>
> For a `Tx<T>` in arg position: the handler sends. The framework
> calls `ChannelBinder::bind_tx` with the channel ID and stores the
> sink directly in the `Tx`'s sink slot.

## Handle hot path

> r[rpc.channel.pair.tx-read]
>
> Sending through a `Tx<T>` MUST use the send binding currently associated
> with that handle. If the `Tx` was created standalone (deserialized or
> server-side), the binding is local to that handle. If it was created via
> `channel()`, the binding is installed by pair binding propagation when the
> paired `Rx<T>` is bound.

> r[rpc.channel.pair.rx-take]
>
> Receiving through an `Rx<T>` MUST use the receive binding currently
> associated with that handle. If the `Rx` was created standalone
> (deserialized or server-side), the binding is local to that handle. If it
> was created via `channel()`, the binding is installed by pair binding
> propagation when the paired `Tx<T>` is bound.
