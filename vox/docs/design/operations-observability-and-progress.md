# Operations, Observability, and Progress

This document captures the direction for making Vox easier to reason about
when a request introduces channels, when a server is slow or dead, and when a
client wants enough telemetry to explain what happened.

It is a design note, not yet a normative spec chapter. Once the shape settles,
the relevant pieces should move into `docs/content/spec/rpc.md` as traced
requirements.

## Current Problem

Vox currently has a clear wire-level request identity, but no first-class
operation identity.

Today:

- `RequestId` identifies one request attempt on one connection.
- `ChannelId` identifies one channel on one connection for wire routing.
- request metadata can carry tracing and deadline information, but no
  operation metadata keys are currently specified as protocol semantics.

The previous channel model gave a channel-bearing method call two competing
interpretations:

- the request/response exchange is done once the response arrives;
- the request still feels active because the channels introduced by that
  request are still active.

That was too easy to misuse. Users and agents tended to keep the method handler
pending to preserve channels, because the call looked like the natural owner of
the streaming work.

We should make the natural model explicit: channels are request-scoped, and the
response is the request lifetime boundary.

## Direction

Make channels request-scoped.

A request owns the channels introduced by that request. The method handler may
produce its response only after its raw channels and request-local progress are
terminal.

Response delivery is the successful request lifetime boundary. Terminal request
failure, cancellation, lane closure, connection loss, and protocol errors are
failure boundaries. In all cases, associated channels should observe that scope
termination instead of silently drifting into an unowned connection-level state.
If Vox ever supports detached channels, detachment should be explicit in the
protocol and API.

This lets devtools show the request as the lane that owns the channel traffic
while making an open handler an honest sign of ongoing request-scoped work.

An operation identity can still exist above request attempts when retry,
resume, or cross-peer tracing needs it. In the common case, the operation and
the request are the same thing. With explicit retry/resume, an operation may
group multiple request scopes, but channels are still owned by the request
scope that introduced them.

## Protocol Metadata

Operation identity can live in Vox metadata, as long as it is not just an
application convention. It should be a well-known protocol metadata namespace
with specified semantics. Request-scoped channel ownership does not require an
operation ID, but retries, resume, and distributed tracing probably do.

Vox metadata is a self-describing value map, so these entries can be structured
values rather than string-only HTTP-style headers. Implementations can validate
their shape and decide whether incompatibility is fatal, warning-level, or
ignored for a particular key.

Candidate reserved keys:

| Key | Purpose |
| --- | --- |
| `vox-operation-id` | Stable logical operation identity. |
| `vox-attempt` | Attempt number or structured attempt descriptor. |
| `vox-parent-operation-id` | Parent operation for nested/client-originated work. |
| `vox-idempotency-key` | Application-level key for safe explicit replacement calls. |
| `vox-deadline` | Absolute or structured deadline for operation policy. |
| `vox-idle-timeout` | Maximum time without request or operation progress. |
| `traceparent` | W3C/OpenTelemetry interop when useful. |
| `tracestate` | W3C/OpenTelemetry vendor state when useful. |

The exact names are not final. The important property is that `vox-*` keys are
reserved for Vox-defined semantics, while ordinary application metadata remains
free-form.

## Request, Channel, and Operation Association

Runtimes should associate a request scope with:

- the request attempt;
- the response;
- all channels listed by that request;
- observer/debug events for those channels;
- server spans and progress events emitted while handling that request.

When a request carries `vox-operation-id`, runtimes should also associate the
request scope with that operation.

Raw channels and request-local progress remain part of the in-flight request
scope. A response terminates that request scope, so tooling should show channel
traffic, progress signals, and request spans under the request that introduced
them while the request is still live, and preserve that association after the
scope is terminal.

This gives devtools the intuitive picture:

- progress-only methods keep the request open while progress channels are
  active;
- response delivery is the terminal transition for the request scope;
- channel traffic still appears under the request that introduced it, including
  in post-terminal debug snapshots.

## Progress and Idle Timeouts

The timeout model should distinguish lack of progress from lack of response.

For channel-bearing requests, a default "response did not arrive in N
seconds" timeout is often the wrong primitive. The more useful default is an
idle timeout for the live request scope:

> If no request-associated activity happens for the configured interval, treat
> the request scope as stuck.

Activity can include:

- request accepted;
- response sent or received;
- channel item sent or received;
- channel close/reset;
- channel credit that demonstrates receiver-side consumption;
- explicit progress event;
- span start/end or structured log event for the request.

Connection keepalive is not request progress. A peer can answer pings while a
particular request scope is deadlocked.

Idle timeout behavior should be policy-driven. Some clients may cancel the
request scope, some may only warn, and some may keep waiting while exposing the
idle state to tooling.

## Observability Side Channel

Vox should support a permissioned observability stream between peers.

The side channel is not a user RPC method and should not consume the request
scope it is observing. It is session-level or connection-level
control/telemetry.

A client may ask the server for observability detail. The server may accept,
reject, redact, or downsample. When enabled, the server can emit events such as:

- link/transport establishment started, succeeded, or failed;
- DNS lookup, TCP connect, TLS handshake, WebSocket upgrade, or platform
  equivalent transport span;
- Vox transport prologue started, accepted, or rejected;
- Vox session handshake started, accepted, rejected, or failed;
- request scope created;
- request attempt started;
- request attempt responded, failed, or cancelled;
- channel associated with request;
- channel item activity;
- channel closed, reset, or failed due to connection loss;
- span started/ended;
- structured progress heartbeat;
- outgoing HTTP/database/process span linked to the request;
- retry/resume attempt started.

The same event stream can feed local devtools. A Vox application should be able
to expose a small local UI that shows client spans, server spans, channel lanes,
and request attempts together.

## Relation to OpenTelemetry

The goal is not to reject OpenTelemetry. The goal is to make the Vox-native
request-scope model precise enough that it can be displayed locally and bridged
outward.

Vox operations can carry or derive OpenTelemetry trace context through
`traceparent` and `tracestate`. Vox-specific events can be exported to OTLP
when desired, while Vox devtools can keep richer protocol-specific structure:
request scopes, request attempts, channel IDs, operation IDs, flow control, and
connection failure boundaries.

## Retries and Durable Delivery

Raw Vox request attempts should still not be replayed implicitly by the conduit
or session layer.

Retries need an operation-level policy above request scopes and request
attempts. A replacement call can carry the same `vox-operation-id` and a new
`RequestId`, with an updated `vox-attempt` value. Whether that is safe depends
on idempotency, application semantics, and any resume protocol involved.

Raw channels are request-scoped ordered streams with flow control. Their wire
IDs may be connection-local, but their ownership and validity come from the
request scope that introduced them. They are not durable message queues.
Important delivery across server death needs a separate layer with explicit
sequence numbers, acknowledgements, commit points, retention policy, and resume
semantics.

Progress reporting is not delivery proof. It can keep an idle detector from
firing and improve debugging, but it cannot replace application-level
acknowledgement when data matters.

## Transport Establishment Observability

Transport establishment should be observable separately from request scopes.

The current spec names links, transports, the transport prologue, and the Vox
session handshake, but observability only requires broad connection lifecycle
and receive-error diagnostics. That is not enough to explain latency before the
first Vox request can run.

Vox should expose establishment spans for the layers that exist on a given
platform:

- endpoint resolution, such as DNS or name-service lookup;
- TCP, Unix socket, named-pipe, stdio, in-process, or WebSocket link creation;
- TLS or other security handshake when present;
- WebSocket HTTP upgrade when present;
- Vox transport prologue;
- Vox session handshake;
- schema decode-plan construction from the peer's session schema;
- virtual connection open/accept/reject.

Not every transport has every step. A Unix socket has no TCP or TLS span unless
wrapped by something else. An in-process link has no network span. The event
model should describe the actual stack rather than forcing every transport into
a fake TCP/TLS shape.

Transport spans are not request progress. They happen before a request scope
exists, or beside it when opening virtual connections. They should attach to a
session/link identity and, once a request exists, request scopes can reference
the established session that carried them.

Transport establishment failures should be visible as structured diagnostics,
not only as "connection failed". A client should be able to distinguish at
least:

- endpoint resolution failed;
- transport connect failed;
- TLS/security handshake failed;
- WebSocket upgrade failed;
- Vox transport prologue was rejected;
- Vox session handshake returned `Sorry`;
- schema compatibility failed during session establishment;
- post-establishment receive/send failed.

For OpenTelemetry export, these should become spans under the client operation
or under a session-establishment trace. For Vox devtools, they should be shown
as the pre-request part of the session timeline, so a slow cache hit can be
split into transport setup, Vox setup, request queueing, server work, channel
activity, and response delivery.

## Observed Current Behavior

This is a snapshot of the current checkout. It is not desired semantics.

| Area | Rust | TypeScript | Swift |
| --- | --- | --- | --- |
| Response timeout | No default per-call response timer is visible in `rust/vox-core/src/driver.rs::call_inner`; timeout behavior may live in higher-level callers or harnesses. | `typescript/packages/vox-core/src/session.ts` has `DEFAULT_TIMEOUT_MS = 30_000` and rejects with `timeout waiting for response`. | Generated Swift clients default `timeout` to `30.0` seconds and pass it through `ConnectionHandle.callRaw`. |
| Successful response with channel args | Generated clients finish call bindings on call/decode/application error paths, not on successful response paths. | Successful response clears pending state with `finalizeChannels: false`. | `ConnectionHandle.callRaw` calls `finalizeChannels?()` for any result, including success. |
| Error response or local timeout | Generated clients finish `Tx` call bindings on errors. | Timeout and most error cleanup paths finalize channel bindings locally. | `finishCallBinding()` for `UnboundTx` calls `close()` and finalizes the paired receive binding. |
| Connection closure observed by channel receivers | The spec requires channel receivers to observe connection closure as an error. | `channelRegistry.closeAll()` has no error argument at the session boundary, so receiver terminal reason is not represented there. | `closeAllChannels()` delivers reset-like terminal state to receivers rather than a distinct connection-loss error. |
| Existing production-like scenarios | `post_reply_generate` and `post_reply_sum` exercise channel traffic after a unary response through generated clients. | The same spec scenarios exist through the matrix, but timeout/error semantics differ. | The same spec scenarios exist through the matrix, but success finalization conflicts with the target model. |

## Roadmap

Each phase should land the spec shape first, with Tracey references added to
the implementation and verification work that follows. The goal is not to keep
old behavior alive under new names. The goal is to remove the misleading model
where channel lifetime appears to be connection-owned or handler-task-owned.

### Phase 0: Inventory the Current Runtime Matrix

Record the current behavior before changing it, so regressions and intended
breaks are distinguishable.

Work:

- document current Rust, TypeScript, and Swift behavior for response timeout,
  channel lifetime after success, channel lifetime after errors, cancellation,
  connection closure, transport establishment, and observer events;
- identify generated-client behavior separately from low-level channel tests;
- name production-like spec-test scenarios that should survive the migration.

Exit criteria:

- a short behavior matrix exists in this design note or a linked design note;
- the matrix separates current behavior from desired behavior;
- known non-parity is explicit, especially TypeScript and Swift timeout/error
  behavior.

### Phase 1: Specify Request Scopes

Make the core vocabulary unambiguous before touching runtime state.

Spec work:

- introduce "request scope" as the owner of a request attempt, response,
  channels introduced by the request, progress signals, and request-local
  spans;
- distinguish "response delivered" from "request scope terminal";
- specify the request-scope state machine:
  `pending`, `responded-live`, `succeeded`, `failed`, `cancelled`,
  `connection-lost`;
- specify that a successful response may move a scope to `responded-live`
  while channels continue;
- specify that terminal request failure fails associated channels unless an
  explicit future detach/resume primitive says otherwise;
- decide and specify cancellation as a request-scope transition, not just a
  response-interest flag;
- update `rpc.channel.lifecycle`, `rpc.cancel.channels`,
  `rpc.observability.channel.context`, and `rpc.debug.snapshot`.

Implementation order:

1. update `docs/content/spec/rpc.md`;
2. run `tracey_validate`;
3. let Tracey show stale references after the spec text changes;
4. update implementations and tests against the new rule versions.

Exit criteria:

- the spec no longer relies on "channels may outlive the request/response
  exchange" as the primary model;
- a reader can tell exactly when a request scope is still live after a
  response;
- a reader can tell exactly what happens to request-scoped channels on request
  failure, cancellation, and connection loss.

### Phase 2: Define Protocol Metadata and Scope Identity

Keep raw channel ownership request-scoped, while giving retries, resume, and
distributed tracing a grouping identity.

Spec work:

- reserve well-known `vox-*` metadata keys and their value shapes;
- define `vox-operation-id` as an optional grouping key, not the owner of raw
  channels;
- define `vox-attempt` for replacement request attempts under the same
  operation;
- define `vox-idempotency-key` for application-approved replacement calls;
- define `vox-idle-timeout` and `vox-deadline` as policy metadata;
- specify validation behavior for malformed reserved metadata.

Implementation order:

1. add typed helpers around metadata in Rust;
2. add equivalent TypeScript and Swift helpers;
3. update generated clients only after the helpers exist;
4. keep unknown application metadata behavior unchanged.

Exit criteria:

- no runtime has to parse ad-hoc strings to understand Vox-owned metadata;
- operation metadata can group request scopes without changing channel
  ownership;
- malformed reserved metadata has specified error or warning behavior.

### Phase 3: Rework Rust Core Request-Scope State

Rust should become the reference implementation for the new semantics.

Runtime work:

- add explicit request-scope state in `rust/vox-core/src/driver.rs`;
- attach every channel created by a request to that request scope;
- keep request-scope debug context after the response has been delivered;
- transition a successful response with live channels to `responded-live`;
- transition to `succeeded` only when the response is delivered and all
  associated channels/progress/spans are terminal;
- transition terminal failures to `failed`, `cancelled`, or `connection-lost`
  and fail associated channels with a request-scope error;
- ensure connection loss still surfaces as an error to live receivers;
- make concurrency accounting count live request scopes, or explicitly specify
  a separate limit for handler attempts if those diverge.

Verification work:

- add production-like Rust tests where the handler returns success promptly and
  channel traffic continues under the same request scope;
- add tests for request error, cancellation, idle timeout, and connection loss
  terminating request-scoped channels;
- verify observer/debug snapshots preserve request/service/method context after
  response delivery.

Exit criteria:

- Rust no longer needs handler tasks to stay pending to preserve channels;
- request-scope state is visible to observers and snapshots;
- `rpc.channel.connection-closure` is still satisfied.

### Phase 4: Update Rust Codegen and Public Policy Surface

Generated clients should make the correct lifetime model hard to misuse.

API work:

- introduce an explicit request policy surface for generated calls, for example
  `client.request(policy).method(args...).await`;
- let policy carry metadata, idle timeout, deadline, tracing preferences, and
  retry policy when that exists;
- keep the plain generated method as the default policy path;
- make channel-bearing calls return once the response is successful while the
  request scope remains internally tracked;
- ensure generated error paths fail request-scoped channels rather than leaving
  them connection-owned.

Exit criteria:

- users can configure request policy without manually building metadata maps;
- generated code cannot accidentally detach channels from their request scope;
- old response-timeout behavior is not the default policy.

### Phase 5: Bring TypeScript and Swift to Parity

Do not layer observability or retries on runtimes that still disagree about
basic channel terminal behavior.

TypeScript work:

- replace the fixed response timeout in
  `typescript/packages/vox-core/src/session.ts` with request-scope idle policy;
- send cancellation or scope-failure frames where required instead of only
  rejecting local promises;
- give channel receivers an error terminal state rather than `null` for all
  endings;
- update generated clients to expose the same request policy shape as Rust.

Swift work:

- remove successful-response channel finalization in
  `swift/vox-runtime/Sources/VoxRuntime/ConnectionHandle.swift`;
- make timeout/cancel semantics request-scope semantics;
- give channel receivers an error terminal state rather than `nil` for all
  endings;
- update generated clients to expose the same request policy shape as Rust.

Exit criteria:

- Rust, TypeScript, and Swift agree on success, error, cancel, idle timeout,
  and connection-loss outcomes;
- spec tests cover at least one channel-outlives-success scenario through each
  generated-client path;
- connection loss is never reported as graceful channel EOF.

### Phase 6: Add Local Observability Events

Build local observability before remote observability.

Spec work:

- define request-scope observer events;
- define channel events as request-scoped events;
- define transport/session establishment events;
- define progress events that reset idle timers;
- define low-cardinality metric guidance for request scopes and operation
  groups.

Implementation work:

- add a structured event enum in Rust using Facet-compatible types;
- emit link/transport establishment, transport prologue, session handshake,
  schema-plan, and virtual-connection events where those phases exist;
- emit events for request scope created, response delivered, channel
  associated, channel activity, progress, scope terminal, and connection loss;
- add mechanically friendly dump output for request/channel state;
- mirror the event vocabulary in TypeScript and Swift.

Exit criteria:

- idle timeout implementation can consume the same activity events devtools
  consumes;
- transport setup latency can be separated from Vox request latency;
- local dumps can explain why a request scope is still live;
- request IDs, channel IDs, and operation IDs are usable for debugging but not
  default metric labels.

### Phase 7: Replace Response Timeout with Idle Timeout

Timeout should mean "this request scope made no progress", not "the response
has not arrived yet".

Policy work:

- define which events count as request progress;
- define which events do not count, especially connection keepalive;
- decide default idle timeout behavior: cancel, warn, or keep waiting with
  visible idle state;
- define deadline behavior separately from idle timeout.

Runtime work:

- implement idle timers in Rust using request-scope observer activity;
- remove or demote TypeScript and Swift response timeouts;
- expose per-call policy through generated clients;
- make timeout terminalization fail associated request-scoped channels with a
  specific error.

Exit criteria:

- a channel-bearing request can run for a long time while it is active;
- a silent deadlocked request is detectable;
- timeout errors are attributable in observers and channel receiver errors.

### Phase 8: Add Permissioned Remote Observability

Only after local events are stable should peers send observability data to each
other.

Spec work:

- decide whether remote observability is core wire protocol or a standard Vox
  service with privileged semantics;
- define negotiation, authorization, redaction, and downsampling;
- define event schemas for remote spans, progress, request scopes, request
  attempts, channels, and transport/session establishment;
- define how remote observability composes with OpenTelemetry `traceparent`
  and `tracestate`.

Implementation work:

- implement Rust server-side event export first;
- implement Rust client-side consumption and dump/devtools ingestion;
- mirror TypeScript and Swift after the event schema stabilizes.

Exit criteria:

- a client can see server-side request progress when permitted;
- server spans can be correlated with client request scopes;
- the observability path does not itself depend on the request scope it is
  observing.

### Phase 9: Build Request/Channel Devtools

Devtools should validate the model by making the right mental picture obvious.

Work:

- show request lanes that remain alive after successful responses when channels
  are active;
- show transport establishment spans before the first request on a session;
- show operation groups only when metadata groups multiple request scopes;
- show channel activity on the request lane that introduced the channel;
- show idle state, deadlines, cancellation, connection loss, and terminal
  errors;
- show server spans and progress events when remote observability is enabled.

Exit criteria:

- a developer can tell whether a call is waiting for response, responded but
  still streaming, idle, failed, cancelled, or connection-lost;
- there is no visual incentive to keep handler tasks pending just to explain
  channel lifetime.

### Phase 10: Add Explicit Retry Policy

Retries are replacement request scopes under an operation policy. They are not
conduit/session replay.

Spec work:

- keep the rule that Vox runtimes do not automatically replay `VoxError`;
- define retry policy as generated-client/application policy above request
  attempts;
- require a fresh `RequestId` for each replacement attempt;
- carry the same `vox-operation-id` and incremented `vox-attempt`;
- require idempotency metadata or explicit application approval for automatic
  retry;
- specify that raw request-scoped channels do not transfer to a replacement
  request.

Rust API sketch:

```rust
let value = client
    .request(
        RequestPolicy::new()
            .idempotency_key(cache_key)
            .retry(RetryPolicy::exponential().max_attempts(3))
            .idle_timeout(Duration::from_secs(30)),
    )
    .get_cached_artifact(req)
    .await?;
```

Implementation work:

- build retries around generated methods so each attempt serializes fresh args
  and channel bindings;
- disable automatic retry by default for raw channel-bearing calls;
- allow retry for channel-bearing calls only when a higher-level resume
  protocol owns the channel semantics;
- expose retry attempt events through observability.

Exit criteria:

- retry never reuses a `RequestCall` or `RequestId`;
- retry never pretends a failed raw channel is still live;
- retry attempts are visible as separate request scopes in one operation group.

### Phase 11: Add Reliable Delivery as a New Primitive

Reliable delivery is not "retry, but harder". It needs its own stream/log
contract.

Spec work:

- define delivery modes by name: at-most-once, at-least-once, and
  effectively-once with receiver-side idempotent commit;
- define stream identity, sequence numbers, acknowledgements, commit points,
  retention, and resume cursors;
- define what `send` means for reliable streams: accepted into local volatile
  queue, accepted into durable local log, or acknowledged by remote peer;
- define receiver commit semantics using `(operation_id, stream_id, sequence)`;
- define resume as a new request scope under the same operation, not as
  resurrection of the old failed request scope.

Rust API sketch:

```rust
let mut chunks = client
    .request(RequestPolicy::new().idempotency_key(upload_id))
    .reliable_stream::<Chunk>("chunks", ReliableOptions::persistent(store))
    .await?;

chunks.send(chunk).await?;
chunks.flush_acked().await?;
chunks.finish().await?;
```

Receiver sketch:

```rust
async fn upload(&self, mut chunks: ReliableRx<Chunk>) -> Result<Receipt, Error> {
    while let Some(chunk) = chunks.recv().await? {
        write_chunk(chunk).await?;
        chunks.commit().await?;
    }

    Ok(receipt)
}
```

Implementation order:

1. volatile at-least-once stream with sequence numbers and ACKs;
2. persistent sender log;
3. receiver commit store;
4. resume handshake under `vox-operation-id`;
5. cross-runtime conformance tests;
6. devtools visualization of stream cursor, ACK, commit, and replay state.

Exit criteria:

- raw `Tx<T>` and `Rx<T>` remain simple request-scoped live channels;
- reliable streams have visibly different types and guarantees;
- server death can be handled by an explicit resume protocol instead of hidden
  replay.

### Phase 12: Remove Old Semantics and Lock Conformance

After the new model exists everywhere, remove the old footguns.

Cleanup work:

- remove fixed response-timeout defaults from generated clients;
- remove code paths that finalize channels solely because a successful response
  arrived;
- remove wording that says channels simply outlive requests without defining
  request scope;
- update old spec-test references that still use inactive channel lifecycle
  rule names;
- keep the broad workspace test command as the status source when reporting
  readiness.

Exit criteria:

- Tracey validates cleanly;
- Rust, TypeScript, and Swift have matching requirement coverage for the new
  rules;
- production-like spec tests exercise request-scoped channels, idle timeout,
  connection loss, retries, and reliable stream resume.

## Open Questions

- Should runtimes always create `vox-operation-id`, or only when observability,
  timeout policy, or retries need it?
- What exact `Value` shape should operation IDs use: string, bytes, tuple,
  UUID-like object, or runtime-selected opaque value?
- Should `vox-attempt` be a number, an object with connection/request fields,
  or both through a versioned shape?
- Which metadata shape incompatibilities are protocol errors, and which are
  observability warnings?
- How much of the observability stream belongs in the core wire protocol versus
  a standard service layered over Vox?
- How should redaction and authorization work for server-side spans sent back
  to a client?
