+++
title = "Schema Exchange"
description = "Backwards-compatible type evolution without changing the wire format"
weight = 14
+++

Postcard is roam's data wire format. It is compact and fast, but positional —
fields are identified by their order, not by name. This means that adding,
removing, or reordering fields changes the byte layout, and a peer reading
with a different type definition silently gets garbage.

Schema exchange solves this without replacing postcard. The data bytes stay
the same. What changes is that peers describe their types to each other
using self-describing schemas, and the receiving side builds a **translation
plan** that maps remote field positions to local field positions before
deserializing.

The result: postcard remains the fast path for serialization and
deserialization, but peers with different versions of the same types can
communicate safely. Incompatibilities are detected early — when the
translation plan is built — not mid-stream when a field has the wrong value.

# Design principles

> r[schema.principles.no-roundtrips]
>
> Schema exchange MUST NOT require request-response negotiation. The sender
> proactively includes schemas before data when the receiver has not seen
> them. No round trips, no handshake, no "do you have this schema?" queries.

> r[schema.principles.sender-driven]
>
> Each peer tracks which schemas it has sent to the other side. When a peer
> is about to send data of a type the other side has not seen, it sends the
> schema first. The receiver never requests schemas — the sender pushes them.

> r[schema.principles.cbor]
>
> Schemas MUST be encoded using CBOR (RFC 8949). CBOR is self-describing
> and does not require a schema to parse — avoiding the chicken-and-egg
> problem of needing a schema to read a schema. Postcard is used for data;
> CBOR is used for metadata about data.

> r[schema.principles.once-per-type]
>
> A schema for a given type ID MUST be sent at most once per connection.
> Once a peer has sent a schema, it records the type ID as "sent" and does
> not send it again for the lifetime of the connection.

# Type identity

Schemas reference other types by type ID. A type ID is an opaque `u32`
handle assigned by the sender.

> r[schema.type-id]
>
> A type ID is a `u32` assigned by the sender. It MUST be unique within
> the sender's half of a connection — no two distinct types sent by the
> same peer may share a type ID. Each peer assigns its own type IDs
> independently; the two peers' ID spaces do not interact. Type IDs do
> not need to be stable across connections, sessions, or compiles.
> Incrementing integers are valid. (If your service has more than 2³²
> distinct types, please get in touch — we'd love to hear about it.)

> r[schema.type-id.per-connection]
>
> Every connection starts with zero schema knowledge. A peer MUST NOT
> assume that schemas sent on one connection are available on another,
> even within the same session. Each connection half has its own
> independent namespace of type IDs and its own sent/received tracking.

Per-connection tracking is required because connections within a session
may terminate at different peers. Consider this topology:

```
     B  → C  (Conn 0 aka root connection)
A ← (B) ← C  (Conn 1)
```

Connection 0 (root) between B and C serves one set of services. C
requests a virtual connection (ID 1), which B forwards to A. B routes
`MessagePayload`s for connection 1 between A and C without inspecting
their content — B does not know what services A and C are speaking on
that connection, and does not need to.

If schema knowledge leaked across connections — for example, if a peer
assumed "I already sent `String`'s schema on connection 0, so I don't
need to send it on connection 1" — the proxy would break. A never saw
connection 0's schemas; it only sees connection 1. Each connection is
an independent communication channel that may reach a different peer,
so schema state must be tracked independently per connection.

Type IDs exist so schemas can reference each other (a struct field points
to its field type's schema by type ID) and so the sender can track which
types it has already sent. They are bookkeeping, not identity.

# Schema format

A schema describes a single type. Schemas are CBOR-encoded and
self-contained — every type referenced by a schema is either a primitive
or is referenced by its type ID.

> r[schema.format]
>
> A schema MUST be a CBOR map containing:
>
>   * `kind` — one of: `"struct"`, `"enum"`, `"tuple"`, `"list"`, `"map"`,
>     `"set"`, `"array"`, `"option"`, `"primitive"`
>   * Kind-specific fields as defined below

> r[schema.format.struct]
>
> A struct schema MUST contain:
>
>   * `kind`: `"struct"`
>   * `fields`: a CBOR array of field descriptors, each a map with:
>     - `name`: field name (UTF-8 string)
>     - `type_id`: the 16-byte type ID of the field's type
>     - `required`: boolean — `true` if the field has no default value
>
> Fields MUST be listed in declaration order (which is also postcard
> serialization order).

> r[schema.format.enum]
>
> An enum schema MUST contain:
>
>   * `kind`: `"enum"`
>   * `variants`: a CBOR array of variant descriptors, each a map with:
>     - `name`: variant name (UTF-8 string)
>     - `index`: the postcard variant index (varint ordinal)
>     - `payload`: one of:
>       - `"unit"` — no payload
>       - `{"newtype": type_id}` — single value
>       - `{"struct": [field_descriptors...]}` — struct variant

> r[schema.format.container]
>
> Container schemas MUST contain:
>
>   * `kind`: `"list"`, `"set"`, `"map"`, `"array"`, or `"option"`
>   * `element`: type ID of the element type (for list, set, array, option)
>   * `key` and `value`: type IDs (for map)
>   * `length`: fixed length (for array only)

> r[schema.format.primitive]
>
> A primitive schema MUST contain:
>
>   * `kind`: `"primitive"`
>   * `type`: one of `"bool"`, `"u8"`, `"u16"`, `"u32"`, `"u64"`, `"u128"`,
>     `"i8"`, `"i16"`, `"i32"`, `"i64"`, `"i128"`, `"f32"`, `"f64"`,
>     `"char"`, `"string"`, `"unit"`, `"bytes"`

> r[schema.format.tuple]
>
> A tuple schema MUST contain:
>
>   * `kind`: `"tuple"`
>   * `elements`: a CBOR array of type IDs, one per element, in order

## Recursive types

Types can reference themselves — a tree node contains child tree nodes.
Since type IDs are opaque handles (not derived from the schema content),
recursive types are straightforward: the recursive field simply references
the type ID of the containing type.

> r[schema.format.recursive]
>
> Recursive type references MUST use the type ID of the referenced type.
> Since the schema for the recursive type is included in the same batch,
> the receiver can resolve the reference. No special backreference markers
> are needed.

## Self-contained schema messages

> r[schema.format.self-contained]
>
> A schema message sent over the wire MUST be self-contained. If a struct
> schema references a field whose type ID has not been previously sent to
> this peer, the field type's schema MUST be included in the same schema
> message. The receiver MUST be able to fully interpret every type ID
> referenced in the message using only the schemas in that message plus
> schemas previously received on this connection.

> r[schema.format.batch]
>
> A schema message is a CBOR-encoded payload containing a list of schemas
> (each with its type ID) and a list of method bindings (mapping method ID
> to root type ID). Schemas SHOULD be ordered so that dependencies appear
> before dependents, but receivers MUST handle any order.

# Schema tracking

Each peer maintains two sets per connection:

> r[schema.tracking.sent]
>
> Each peer MUST track the set of type IDs for which it has sent schemas to
> the other peer. This set starts empty and grows monotonically over the
> connection lifetime.

> r[schema.tracking.received]
>
> Each peer MUST track the set of type IDs for which it has received schemas
> from the other peer. This set starts empty and grows monotonically over
> the connection lifetime.

> r[schema.tracking.transitive]
>
> When a schema is sent, all type IDs transitively referenced by that schema
> are also marked as sent. A schema message is self-contained
> (see `r[schema.format.self-contained]`), so sending a struct schema
> implicitly sends the schemas of all its field types, their field types,
> and so on.

# Two levels of schema exchange

Schema exchange operates at two levels:

1. **Protocol level (per-session):** The `MessagePayload` schema is
   exchanged during the CBOR handshake (see `r[session.handshake]`).
   This allows the protocol framing itself to evolve without breaking
   changes.

2. **Application level (per-connection):** Method argument and response
   schemas are exchanged lazily via `SchemaMessage` payloads, scoped to
   each connection. This allows service types to evolve independently.

The rest of this section describes application-level schema exchange.

# When schemas are exchanged

Schema exchange is triggered by method invocation. The caller sends schemas
for its argument types; the callee sends schemas for its response types. This
is lazy — schemas are only exchanged for types actually used in calls, not
for the entire service interface up front.

> r[schema.exchange.caller]
>
> Before sending a `Request`, the caller MUST check whether the schemas for
> the method's argument types have been sent to this peer on this connection.
> If any have not, the caller MUST send a `SchemaMessage` containing all
> unsent schemas before sending the `Request`.

> r[schema.exchange.callee]
>
> Before sending any `Response` for a method, the callee MUST check whether
> the schemas for the method's **statically-known response type** have been
> sent to this peer on this connection. If any have not, the callee MUST
> send a `SchemaMessage` before the `Response`.
>
> The response schema is determined by the method signature — it is the
> full `Result<T, RoamError<E>>` wire type. It MUST NOT vary based on
> whether the handler succeeded or failed. Sending schemas for a different
> type (e.g. `Result<(), RoamError<E>>` when the method returns
> `Result<T, RoamError<E>>`) is a protocol error.

> r[schema.exchange.channels]
>
> Channel element types are included in schema exchange. If a method's
> arguments contain `Tx<T>` or `Rx<T>`, the schema for `T` MUST be included
> in the caller's schema message. If the response contains channel types,
> their element schemas MUST be included in the callee's schema message.

> r[schema.exchange.ordering]
>
> `SchemaMessage` MUST arrive before the `Request` or `Response` that
> references those types on the same connection. The receiver MUST be
> able to build a translation plan for the incoming data before
> deserializing it.

> r[schema.exchange.required]
>
> Application-level schema exchange is mandatory. If a peer receives a
> `Request` or `Response` for a method whose schemas have not been
> received on that connection, this is a protocol error and the
> connection MUST be torn down. There is no fallback to identity
> deserialization — the sender is always responsible for sending schemas
> before data.

> r[schema.exchange.idempotent]
>
> If the caller has already sent schemas for a method's argument types
> (from a previous call to the same or different method using the same
> types), no schema message is needed. The `r[schema.principles.once-per-type]`
> rule applies — each type ID is sent at most once.

# Method identity without signatures

Schema exchange is mandatory (see `r[session.handshake]`). Since peers
always have each other's type metadata, method identity no longer needs
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
> The signature hash (`sig_bytes` from `r[signature.hash.algorithm]`) is
> excluded. Only the service name and method name contribute to the method
> ID.

Renaming a method is still a breaking change (the method ID changes),
but changing argument or return types is no longer automatically
breaking — it depends on whether the translation plan can bridge the
difference.

# Translation plans

When a peer receives a schema for a remote type that it will deserialize
into a local type, it builds a **translation plan**. The translation plan
is a recipe for reading postcard bytes written by the remote type and
populating the fields of the local type.

Translation plans are built once per (remote type ID, local type) pair
and cached for the connection lifetime.

> r[schema.translation.field-matching]
>
> Fields MUST be matched by name, not by position. For each field in the
> local type, the translation plan looks up the corresponding field in the
> remote schema by name. If found, the plan records the remote field's
> position so the deserializer knows which postcard field to read.

> r[schema.translation.skip-unknown]
>
> If the remote schema contains fields that do not exist in the local type,
> those fields MUST be skipped during deserialization. The translation plan
> records how many bytes to skip for each unknown remote field, based on
> the remote field's type schema.

> r[schema.translation.fill-defaults]
>
> If the local type contains fields that do not exist in the remote schema,
> those fields MUST be filled with their default values. Fields without
> default values that are missing from the remote schema cause a
> translation plan error (see `r[schema.errors.missing-required]`).

> r[schema.translation.reorder]
>
> If fields exist in both the local and remote types but in different order,
> the translation plan MUST handle the reordering. The deserializer reads
> postcard bytes in remote field order but writes values into local field
> positions.

> r[schema.translation.type-compat]
>
> For each matched field, the remote field type and local field type MUST be
> compatible. Two types are compatible if:
>
>   * They are the same primitive type
>   * They are both containers of the same kind with compatible element types
>   * They are both structs and a nested translation plan can be built
>   * They are both enums and variant matching succeeds
>     (see `r[schema.translation.enum]`)

> r[schema.translation.serialization-unchanged]
>
> Schema exchange does NOT affect serialization. A peer always serializes
> using its own local type definition and postcard. The translation plan
> applies only on the deserialization side — the receiver adapts to the
> sender's layout.

# Enum evolution

Enums follow the same principle as structs — match by name, not by position.
This allows adding variants to an enum without breaking existing peers.

> r[schema.translation.enum]
>
> Enum variants MUST be matched by name, not by variant index. The
> translation plan maps remote variant names to local variant indices and
> records how to deserialize each variant's payload.

> r[schema.translation.enum.unknown-variant]
>
> If a remote enum has variants that the local type does not, those variants
> are skippable in the schema but cause an error at runtime if actually
> received. The translation plan records that these variants exist in the
> remote schema; if a message arrives with an unknown variant, the
> deserializer MUST return an error.

> r[schema.translation.enum.missing-variant]
>
> If the local enum has variants that the remote schema does not, this is
> fine — those variants will never appear in data from that remote peer.
> No error is needed. The local peer can still use those variants when
> sending data.

> r[schema.translation.enum.payload-compat]
>
> For each variant that exists in both the remote and local types, the
> variant payloads MUST be compatible: unit matches unit, newtype matches
> newtype with a compatible inner type, struct matches struct with
> compatible fields (same rules as top-level struct matching).

# Error reporting

Schema exchange detects incompatibilities early — when building the
translation plan — rather than failing mid-stream on corrupt data.

> r[schema.errors.early-detection]
>
> Type incompatibilities MUST be detected at translation-plan construction
> time, not during deserialization of individual messages. When a peer
> receives a schema and attempts to build a translation plan against a
> local type, all structural incompatibilities MUST be reported before
> any data of that type is processed.

> r[schema.errors.missing-required]
>
> If a local struct has a required field (no default value) that is not
> present in the remote schema, the translation plan MUST fail with an
> error identifying the missing field by name and type.

> r[schema.errors.type-mismatch]
>
> If a field exists in both the remote and local types but the types are
> incompatible (e.g., remote has `u32`, local has `String`), the
> translation plan MUST fail with an error identifying the field, the
> remote type, and the local type.

> r[schema.errors.unknown-variant-runtime]
>
> If a message arrives containing an enum variant that exists in the
> remote schema but not in the local type, the deserializer MUST return
> an error for that specific message. This is a runtime error because
> the translation plan cannot predict which variant a given message
> will contain.

> r[schema.errors.content]
>
> All schema-related errors MUST include:
>
>   * The remote type ID
>   * The local type name (for diagnostics)
>   * The specific incompatibility (missing field, type mismatch, etc.)
>   * For field-level errors: the field name and both the remote and local
>     field types

# Compatibility checking

Schema exchange handles runtime differences gracefully, but it is still
valuable to know about compatibility issues before deployment. Tooling
can snapshot schemas and check changes as part of the development workflow.

> r[schema.compat.snapshot]
>
> Implementations SHOULD provide tooling to snapshot the schemas of a
> service's types. A snapshot captures the full schemas for every type
> used in the service's method signatures.

> r[schema.compat.check]
>
> Implementations SHOULD provide tooling to compare two snapshots and
> report:
>
>   * **Compatible changes** — changes where a translation plan can be
>     built in both directions (e.g., adding an optional field)
>   * **One-way compatible changes** — changes where old can read new but
>     not vice versa (e.g., adding a required field with a default)
>   * **Breaking changes** — changes where no translation plan can be
>     built (e.g., removing a required field, changing a field's type
>     incompatibly)

> r[schema.compat.ci]
>
> Schema compatibility checks SHOULD be integrated into CI pipelines.
> Breaking changes should fail the build unless explicitly acknowledged.

> r[schema.compat.policy]
>
> A breaking change is one where a translation plan cannot be built between
> the old and new versions. Whether a breaking change is acceptable depends
> on the project's deployment model (rolling updates vs. coordinated
> releases). The tooling reports facts; policy is up to the project.

# Interaction with other spec areas

Schema exchange is designed to be transparent to the rest of the protocol.

> r[schema.interaction.channels]
>
> Channels are unaffected by schema exchange beyond their element types.
> Channel semantics (creation, flow control, close, reset) are unchanged.
> The element type's schema is exchanged as part of the method's argument
> or response schemas (see `r[schema.exchange.channels]`), and translation
> plans apply to channel items the same way they apply to request/response
> payloads.

> r[schema.interaction.retry]
>
> Retry semantics are unaffected by schema exchange. Operation IDs, the
> commit point, and sealed replay all work identically. The translation
> plan for a sealed response is the same one built when the type's schema
> was first received — replayed responses use the same deserialization
> path as live ones.

> r[schema.interaction.metadata]
>
> Metadata is unaffected by schema exchange. Metadata key-value pairs are
> not typed in the postcard sense and do not participate in schema exchange.
