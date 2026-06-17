+++
title = "Schema Exchange"
description = "Backwards-compatible type evolution without changing the wire format"
weight = 14
+++

vox's wire format and type model are [phon](https://github.com/bearcove/phon).
phon is a schema-driven binary codec: values are encoded in a compact,
positional layout (fields by order, not name), and a **schema** describes a
type's structure so that two peers with different versions of the same type can
still communicate. Everything about the type model — schema kinds, content-hash
type identity, the self-describing and compact encodings, and the
**plan-first** compatibility decoder that bridges a writer's layout onto a
reader's type — is defined by phon's spec and is **not** restated here.

This section specifies the layer vox builds on top: the **schema exchange
protocol** — how and when peers send each other the phon schemas they need
before sending data that depends on them. The data bytes are phon; what schema
exchange adds is the discipline of describing types ahead of the data, scoped to
each connection, so service types can evolve independently.

The result: phon stays the fast path for serialization and deserialization, but
peers with different versions of the same types communicate safely.
Incompatibilities surface early — when phon builds its decode plan
(`phon r[compat.plan-first]`) — not mid-stream when a field has the wrong value.

# Design principles

> r[schema.principles.no-roundtrips]
>
> Schema exchange MUST NOT require request-response negotiation. The sender
> proactively includes schemas before data when the receiver has not seen
> them. No round trips, no handshake, no "do you have this schema?" queries.

> r[schema.principles.sender-driven]
>
> Each peer tracks which schema bindings it has established with the other
> side. When a peer is about to send data for a method/direction binding the
> other side has not seen, it sends the binding first. The receiver never
> requests schemas — the sender pushes them.

> r[schema.principles.self-describing]
>
> Schemas themselves are exchanged as phon **schema-closure bytes**: the
> self-describing serialization of a root type plus every composite type it
> transitively references, framed so the receiver can rebuild a phon registry
> without already having a schema (`phon r[self-describing.bootstraps-schemas]`,
> `phon r[schema-identity.closure]`). phon's self-describing mode is the
> bootstrap that avoids the chicken-and-egg problem of needing a schema to read
> a schema; the data payloads it describes use phon's compact mode.

> r[schema.principles.once-per-type]
>
> Within one schema-closure carrier, a schema for a given type ID MUST appear
> at most once. Across the connection, deduplication is binding-scoped: the
> first carrier for each `(method_id, direction)` binding may repeat type
> definitions that appeared in earlier bindings, because the receiver needs a
> root type ID for this specific method/direction.

# Type identity

vox identifies types by their phon **schema ID**: a `u64` content hash of a
type declaration's phon-level structure (`phon r[schema-identity.content-hash]`,
`phon r[schema-identity.computation]`). The hash is structural and
language-independent — the same declaration produces the same ID regardless of
connection, lane, process, or source language, and language-level wrappers
(Rust newtypes, TypeScript aliases) over the same underlying type collapse to
the same ID. vox does not define its own hashing; it uses phon's.

> r[schema.type-id]
>
> A vox type ID is a phon schema ID — a `u64` content hash, computed by phon
> (`phon r[schema-identity.content-hash]`). Implementations MUST use phon's
> identity so that IDs match across languages and persist across connections
> and lanes.

Content hashes give type IDs a universal meaning. A peer that receives a schema
tagged with a content hash it has already seen — from this lane, another
connection, or a persistent store — knows it already has that schema. This
supports efficient schema tracking for later lanes and local schema caches.

> r[schema.type-id.per-connection]
>
> Every service lane starts with zero application-schema knowledge. A peer MUST
> NOT assume that schemas sent on one lane are available on another lane, even
> within the same Vox connection. Each lane half has its own
> sent/received tracking. However, because type IDs are content hashes,
> a peer MAY use a previously received schema (from another lane
> or a persistent cache) to build a decode plan without waiting for the
> schema to be resent — as long as it does not send data until the remote
> peer has confirmed (by sending its own schemas) that it can read it.

Per-lane tracking is required because service lanes can be routed or proxied
independently. Consider this topology:

```
     B  -> C  (lane 1: HTTP cell service)
A <- (B) <- C  (lane 3: host DevTools service, proxied by B)
```

Lane 1 between B and C serves one set of services. C requests another service
lane, which B routes to A. B routes `MessagePayload`s for that lane between A
and C without inspecting their content: B does not know what services A and C
are speaking on that lane, and does not need to.

If schema knowledge leaked across lanes — for example, if a peer assumed
"I already sent `String`'s schema on lane 1, so I don't need to send it on lane
3" — the proxy would break. A never saw lane 1's schemas; it only sees lane 3.
Each service lane is an independent application-schema scope that may reach a
different peer, so schema state must be tracked independently per lane.

# Schema delivery

Schemas reach a peer through one of two carriers, both of which hold phon
schema-closure bytes:

  * **Inline with the data.** A `RequestCall` and a `RequestResponse` each carry
    a `schemas` field (phon schema-closure bytes for that message's argument or
    response root). It is non-empty the first time a `(method_id, direction)`
    binding is used on a lane and empty thereafter.
  * **Standalone `SchemaMessage`.** A binding may also be advertised ahead of
    its first payload-bearing message, so a batch can establish all required
    bindings before their first use. A `SchemaMessage` carries the same
    schema-closure bytes plus the `(method_id, direction)` it binds.

```rust
enum BindingDirection {
    Args,
    Response,
}
```

A schema binding is self-describing and self-contained: it identifies the root
type IDs that matter for the binding and includes every composite schema the
receiver needs, so the receiver can rebuild a phon registry and decode plans
from the binding alone (plus any composites already received on this
lane).

> r[schema.format.self-contained]
>
> When a carrier includes schemas, the set of schemas MUST be self-contained:
> every type ID referenced by any schema in the closure MUST either be defined
> in the same closure or have been previously received on this lane. The
> receiver MUST be able to build phon decode plans for all included types before
> deserializing the payload.

> r[schema.format.delivery]
>
> A binding carrier (the `schemas` field of a `RequestCall`/`RequestResponse`,
> or a standalone `SchemaMessage`) MUST carry phon schema-binding bytes that
> include:
>
>   * All schemas for the method's types that have not been previously sent on
>     this lane, and
>   * The primary root type ID for one `(method_id, direction)` binding.
>
> The root type for a response is always the full `Result<T, VoxError<E>>` wire
> type, regardless of whether the handler succeeded or failed.
>
> A carrier binds exactly one `(method_id, direction)` pair. If all schemas for
> that method's types have already been sent on this lane, the closure MAY
> contain only the root (no new composite schemas) — but the binding MUST still
> be established the first time this `(method_id, direction)` pair is introduced
> on the lane, so the receiver knows which type ID is the root for this method.

> r[schema.format.binding-roots]
>
> A schema binding MUST identify exactly one **primary** root for the
> `(method_id, direction)` pair and MAY identify auxiliary roots used by
> payload-adjacent values that are not reachable from the primary wire shape.
> Auxiliary roots are part of the same binding, not independent method
> bindings. Each auxiliary root MUST be keyed by a stable semantic role so the
> receiver can choose the correct writer root when building a decode plan.
>
> For request arguments, the primary root is the argument tuple's wire shape.
> For responses, the primary root is the full response wire shape
> `Result<T, VoxError<E>>`. Channel element roots (see
> `r[schema.exchange.channels]`) are auxiliary roots because `Tx<T>` and
> `Rx<T>` encode as opaque channel indices on the wire.
>
> The schema-binding byte framing is:
>
> ```text
> u64 primary_root
> u32 schema_count
> repeated schema_count:
>   u32 schema_len
>   bytes schema
> optional:
>   u32 auxiliary_root_count
>   repeated auxiliary_root_count:
>     u32 role_len
>     utf8 role
>     u64 root
> ```
>
> The auxiliary-root section is absent when the count would be zero, preserving
> the compact single-root closure used by bindings without auxiliary roots.

# Schema tracking

Each peer maintains, per lane:

> r[schema.tracking.sent]
>
> Each peer MUST track the set of `(method_id, direction)` bindings for which
> it has sent schema-binding bytes to the other peer. This set starts empty
> and grows monotonically over the lane lifetime.

> r[schema.tracking.received]
>
> Each peer MUST track the schema-binding bytes it has received for each
> `(method_id, direction)` binding from the other peer. Receiving the same
> binding more than once is not an error — the receiver overwrites
> idempotently.

> r[schema.tracking.transitive]
>
> When a schema binding is sent, its schema closure MUST include every type
> transitively referenced by the root and auxiliary roots in that binding.
> A schema closure is self-contained (see `r[schema.format.self-contained]`),
> so sending a struct schema implicitly includes the schemas of all its field
> types, their field types, and so on.

> r[schema.tracking.bindings]
>
> Each peer MUST track the set of (method_id, direction) pairs for which
> it has established bindings on this lane. A binding MUST be sent
> the first time a method's schemas are delivered for a given direction,
> even if all the schemas themselves were already sent by a previous call
> to a different method.

# Two levels of schema exchange

Schema exchange operates at two levels:

1. **Protocol level (per-connection):** The `Message` envelope schema is
   exchanged during the connection handshake (see `r[connection.handshake]`). This
   lets the protocol framing itself evolve without breaking changes.

2. **Application level (per-lane):** Method argument and response schemas
   are exchanged lazily, scoped to each service lane. This lets service types
   evolve independently.

The rest of this section describes application-level schema exchange.

# When schemas are exchanged

Schema exchange is triggered by method invocation. The caller sends schemas
for its argument types; the callee sends schemas for its response types. This
is lazy — schemas are only exchanged for types actually used in calls, not
for the entire service interface up front.

> r[schema.exchange.caller]
>
> Before sending a `Request`, the caller MUST check whether the schemas for
> the method's argument types have been sent to this peer on this lane.
> If any have not, the caller MUST include the unsent schema closure (and the
> method binding) with — or ahead of — the `Request`
> (see `r[schema.format.delivery]`).

> r[schema.exchange.callee]
>
> Before sending any `Response` for a method, the callee MUST check whether
> the schemas for the method's **statically-known response type** have been
> sent to this peer on this lane. If any have not, the callee MUST
> include the unsent schema closure and the method binding with — or ahead of —
> the `Response`.
>
> The response schema is determined by the method signature — it is the
> full `Result<T, VoxError<E>>` wire type. It MUST NOT vary based on
> whether the handler succeeded or failed. Sending schemas for a different
> type (e.g. `Result<(), VoxError<E>>` when the method returns
> `Result<T, VoxError<E>>`) is a protocol error.

> r[schema.exchange.channels]
>
> Channel element types are included in schema exchange. If a method's
> arguments contain top-level `Tx<T>` or `Rx<T>` handles, the schema for each
> element type `T` MUST be reachable from the caller's advertised argument
> schemas. On the wire a channel handle is opaque
> (`r[rpc.channel.payload-encoding]`); its element schema therefore travels as
> an auxiliary root of the method's argument schema binding (see
> `r[schema.format.binding-roots]`), keyed by the generated per-method channel
> metadata:
>
>   * Argument index.
>   * Channel direction (`Tx` or `Rx` from the holder's point of view).
>   * Element root ID for that argument's `T`.
>
> The item receiver MUST store the applicable channel-element writer root
> alongside the bound channel handle so that each incoming item is decoded
> through phon's compatibility plan against the local element type. Channels
> MUST NOT appear in return types (see `r[rpc.channel.placement]`).

> r[schema.exchange.channels.rx-args]
>
> For an argument `Rx<T>`, the caller is the channel item writer and the callee
> is the channel item receiver. The callee MUST bind its `Rx<T>` with the
> caller's `channel.arg.N.rx.element` auxiliary root from the method's argument
> schema binding, and MUST decode every incoming item through a phon
> compatibility plan from that writer root to the callee's local `T`.

> r[schema.exchange.channels.tx-args]
>
> For an argument `Tx<T>`, the callee is the channel item writer and the caller
> is the channel item receiver. Before the callee sends the first item on that
> channel, the caller MUST have received the callee's
> `channel.arg.N.tx.element` writer root for the same method/argument role, so
> the caller's paired `Rx<T>` can decode incoming items through a phon
> compatibility plan from the callee's writer root to the caller's local `T`.

> r[schema.exchange.required]
>
> Application-level schema exchange is mandatory. If a peer receives a
> `Request` or `Response` and either (a) the schemas for any referenced
> type have not been received on that lane, or (b) no
> method binding for this `(method_id, direction)` pair has been
> received on this lane, this is a protocol error and the
> lane MUST be torn down. The sender is always responsible for
> sending both schemas and bindings before the data that needs them.

> r[schema.exchange.idempotent]
>
> If the caller has already sent schemas for a method's argument types
> (from a previous call to the same or different method using the same
> types), no schemas need to be included. The `r[schema.principles.once-per-type]`
> rule applies — each type ID is sent at most once. The binding for a new
> `(method_id, direction)` pair MUST still be established the first time it is
> introduced on the lane (see `r[schema.tracking.bindings]`).

# Method identity without signatures

Schema exchange is mandatory (see `r[connection.handshake]`). Since peers always
have each other's protocol and application type metadata before decoding data,
method identity no longer needs
to encode the full type signature. Two versions of a service may have
the same method with evolved argument types — including the signature
hash in the method ID would make these look like different methods,
which is exactly what schema exchange is designed to avoid.

> r[schema.method-id]
>
> The method ID MUST be computed as:
> ```
> method_id = blake3(kebab(ServiceName) + "." + kebab(methodName))[0..8]
> ```
> Only the service name and method name contribute to the method ID. Argument
> and return types do not contribute.

Renaming a method is still a breaking change (the method ID changes),
but changing argument or return types is no longer automatically
breaking — it depends on whether phon's compatibility plan can bridge the
difference.

# Decode plans and error reporting

When a peer must deserialize a remote type into a local type, it asks phon to
build a **compatibility decode plan** from the remote (writer) schema
against the local (reader) type — matching fields by name, reordering, skipping
writer-only fields, and defaulting reader-only fields. The mechanics of that
plan — field matching, enum-by-name, defaulting, the early-detection of
structural incompatibilities — are defined by phon
(`phon r[compat.plan-first]`, `phon r[compat.field-matching]`,
`phon r[compat.enum-by-name]`, `phon r[compat.unmatched-reader-field]`). vox
defines only how a plan failure is surfaced as an RPC outcome.

> r[schema.errors.call-level]
>
> A decode-plan failure is a **call-level error**, not a connection-level
> fault. The connection remains open and other method calls are unaffected.
> This is distinct from missing schemas entirely (a protocol error per
> `r[schema.exchange.required]`), which tears down the connection.

> r[schema.errors.call-level.callee]
>
> If the callee cannot build a decode plan for incoming request arguments,
> it MUST respond with an error describing the incompatibility (surfacing
> phon's plan error, which identifies the offending type and field).

> r[schema.errors.call-level.caller]
>
> If the caller cannot build a decode plan for an incoming response, the
> failure is local — the call's result resolves to an error. There is no
> further message to send; the response has already been received.

> r[schema.errors.same-peer-terminal]
>
> A decode-plan failure is terminal for that call against the current remote
> peer schema. The remote peer's schema for a given type does not change while
> the connection is open, so issuing the same call again against the same peer
> schema will reproduce the same failure.

# Compatibility checking

phon's plan handles runtime differences gracefully, but it is still valuable to
know about compatibility issues before deployment. Tooling can snapshot schemas
and check changes as part of the development workflow.

> r[schema.compat.snapshot]
>
> Implementations SHOULD provide tooling to snapshot the schemas of a
> service's types. A snapshot captures the full phon schemas for every type
> used in the service's method signatures.

> r[schema.compat.check]
>
> Implementations SHOULD provide tooling to compare two snapshots and
> report:
>
>   * **Compatible changes** — changes where a decode plan can be built in
>     both directions (e.g., adding an optional field)
>   * **One-way compatible changes** — changes where old can read new but
>     not vice versa (e.g., adding a required field with a default)
>   * **Breaking changes** — changes where no decode plan can be built
>     (e.g., removing a required field, changing a field's type incompatibly)

> r[schema.compat.ci]
>
> Schema compatibility checks SHOULD be integrated into CI pipelines.
> Breaking changes should fail the build unless explicitly acknowledged.

> r[schema.compat.policy]
>
> A breaking change is one where a decode plan cannot be built between
> the old and new versions. Whether a breaking change is acceptable depends
> on the project's deployment model (rolling updates vs. coordinated
> releases). The tooling reports facts; policy is up to the project.

# Interaction with other spec areas

Schema exchange is designed to be transparent to the rest of the protocol.

> r[schema.interaction.channels]
>
> Channels are unaffected by schema exchange beyond their element types.
> Channel semantics (creation, flow control, close, reset) are unchanged.
> Channel element writer roots are exchanged according to
> `r[schema.exchange.channels.rx-args]` and
> `r[schema.exchange.channels.tx-args]`, and decode plans apply to channel items
> the same way they apply to request/response payloads. The writer root for a
> channel item is the channel element auxiliary root recorded when the channel
> handle was bound, not the receiver's local element root.

> r[schema.interaction.metadata]
>
> Metadata is unaffected by schema exchange. Metadata is a self-describing
> phon `Value` map (see `r[rpc.metadata]`); its entries are not nominally
> typed and do not participate in schema exchange.
