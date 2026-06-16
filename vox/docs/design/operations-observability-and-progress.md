# Request Scopes, Observability, and Progress

This document captures the current design direction for channel-bearing Vox
requests, idle detection, and observability. It is a design note, not a
normative spec chapter. Normative requirements live in `docs/content/spec/*.md`.

## Current Decision

Raw Vox channels are request-scoped sidebands.

A request scope owns:

- the request attempt;
- the response, when one is delivered;
- raw channels introduced by that request;
- request-local progress;
- observer/debug context for that work.

Response delivery is the successful terminal transition for the request scope.
Raw channels introduced by the request must be terminal before, or as part of,
that response delivery. A handler that wants to keep using a raw channel keeps
the request in flight until the raw channel is done.

This round does not specify retry, resume, durable streams, or operation
identity. Those may be useful later, but they are not Vox-core semantics for raw
channels.

## Why

The previous model let a successful response arrive while request-introduced
channels kept running. That made a channel-bearing method look like it had two
owners:

- the call had returned from the API user's point of view;
- the channels still visually belonged to the call that introduced them.

Users and agents tended to preserve channels by keeping handlers pending
forever. That intuition was not accidental: if a request introduced the
channels, tooling naturally wants to show the channel activity under that
request. The simpler rule is to make that visual model true.

## Important Streams

Raw `Tx<T>` and `Rx<T>` are not durable streams. They do not survive response
delivery, lane closure, connection loss, reconnect, or process death.

Important streams should be modeled as service-level protocols. Vixen's
`Producing::force(PartKey) -> Part` shape is the current example: a produced
tree crosses as a handle, and each demanded part is a normal request/response
call. If Vixen later needs recovery after a link break, that recovery belongs
to the producing service's handle, retention, authentication, and part-demand
protocol, not to raw Vox channels.

## Idle Timeouts

Timeouts should detect lack of request progress, not merely lack of response.

While a request scope is in flight, request-associated activity can reset an
idle detector:

- request accepted;
- channel item sent or received;
- channel close/reset;
- channel credit that proves receiver-side consumption;
- explicit request progress;
- cancellation, drain, or retire transition;
- response delivery.

Connection keepalive, unrelated logs, and spans not attached to the request
scope are not request progress.

## Observability

Vox should expose local observer events first. Remote observability can be
layered after the event vocabulary is stable.

Useful local events include:

- link/transport establishment started, succeeded, or failed;
- TCP, Unix socket, named-pipe, WebSocket, in-process, TLS, or platform
  security phases when they exist;
- Vox transport prologue and connection handshake;
- schema decode-plan construction;
- service-lane open/accept/reject;
- request scope created;
- request response delivered, failed, cancelled, or abandoned by lane or
  connection loss;
- raw channel associated with request;
- channel item, credit, close, reset, and connection-loss activity;
- explicit request progress.

The observer path must not depend on the request/channel path it is trying to
explain. It may use the same codec and schema machinery, but it must not block
behind ordinary user-request flow control.

For metrics, request IDs, lane IDs, connection IDs, channel IDs, peer
addresses, and metadata values are high-cardinality debug context, not default
labels.

## Transport Establishment

Transport establishment should be visible separately from request scopes.

Not every transport has every phase. A Unix socket has no TCP or TLS span
unless it is wrapped by another layer. An in-process link has no network span.
The event model should describe the actual stack rather than forcing every
transport into fake TCP/TLS milestones.

A client should be able to distinguish at least:

- endpoint resolution failed;
- transport connect failed;
- TLS or platform security handshake failed;
- WebSocket upgrade failed;
- Vox transport prologue was rejected;
- Vox connection handshake returned `Sorry`;
- schema compatibility failed during establishment;
- post-establishment receive/send failed.

## Roadmap

### Phase 1: Lock the Spec Vocabulary

- Keep `request scope` as the owner of request attempt, response, raw channels,
  request-local progress, and debug context.
- Keep response delivery as terminal for successful request scopes.
- Keep raw channels non-durable and request-scoped.
- Keep retry, resume, durable streams, and operation identity out of the
  current Vox-core spec.

### Phase 2: Audit Real Consumers

- Classify every raw `Tx<T>`/`Rx<T>` consumer as progress/event sideband or
  important stream.
- For progress/event sidebands, keep the request in flight until the channel is
  terminal.
- For important streams, move or keep them as service-level protocols like
  Vixen's `Producing::force`.

### Phase 3: Bring Runtime Implementations to the Spec

- Update Rust first.
- Update TypeScript and Swift after the Rust behavior is mechanically clear.
- Ensure generated clients cannot leave raw channels alive after response
  delivery.
- Ensure failure, cancellation, lane closure, and connection loss terminalize
  request-associated raw channels with distinguishable reasons.

### Phase 4: Replace Response Timeout with Idle Detection

- Define and emit the request-associated activity used by idle timers.
- Make silent stuck requests visible.
- Avoid treating active progress/channel traffic as a stuck request just
  because no response has been delivered yet.

### Phase 5: Build Observability Events and Devtools

- Emit local observer events for transports, connection establishment, lanes,
  request scopes, raw channels, and progress.
- Add mechanically friendly debug snapshots.
- Add permissioned remote observability only after local events are stable.

### Phase 6: Consider Service-Level Durable Protocols

Durable streams, resumable production handles, and replay/commit protocols are
future higher-level work. They should have visibly different types and
guarantees from raw `Tx<T>` and `Rx<T>`.

## Open Questions

- What exact terminal reason vocabulary should channel receivers expose for
  response delivery, cancellation, lane closure, connection loss, and protocol
  error?
- Should idle timeout default behavior cancel the request, warn, or only expose
  idle state to tooling?
- How much of remote observability belongs in core wire protocol versus a
  privileged standard service layered over Vox?
- How should redaction and authorization work for server-side spans sent back
  to a client?
