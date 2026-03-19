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

> r[schema.type-id]
>
> A type ID is a `u64` content hash — a deterministic structural hash
> of the type's postcard-level definition. The same type always produces
> the same hash, regardless of which connection, session, process, or
> language produced it. On the wire (CBOR), a type ID is encoded as a
> CBOR unsigned integer.

> r[schema.type-id.hash]
>
> The content hash of a type is computed by feeding a canonical byte
> sequence into blake3, then taking the first 8 bytes of the output as
> a little-endian `u64`. The canonical byte sequence is constructed by
> updating the hasher with the components described below.
>
>   * **Strings** (field names, variant names, tag strings) are fed as
>     their byte length as a `u32` in little-endian order, followed by
>     the raw UTF-8 bytes. The length prefix ensures the encoding is
>     injective — no two different type structures produce the same
>     byte sequence.
>   * **`u64` values** (child type hashes, array lengths) are fed as 8
>     bytes in little-endian order.
>   * **`u32` values** (variant indices) are fed as 4 bytes in
>     little-endian order.
>
> Implementations MUST produce identical hashes for structurally
> identical types regardless of the source language.
>
> For recursive types, see `r[schema.hash.recursive]`.

## Primitive type hashes

The hash input for a primitive type is a single tag string. Because the
hash operates at the postcard encoding level — not at the source-language
type level — Rust newtypes, TypeScript type aliases, and other
language-level wrappers over the same underlying type all produce the
same hash. For example, `struct UserId(u64)` and `struct PostId(u64)`
both hash as `u64`.

> r[schema.type-id.hash.primitives]
>
> The hash of a primitive type is `blake3(tag)[0..8]` where `tag` is one
> of the following UTF-8 strings:
>
> | Postcard type | Tag string |
> |---------------|------------|
> | bool          | `"bool"`   |
> | u8            | `"u8"`     |
> | u16           | `"u16"`    |
> | u32           | `"u32"`    |
> | u64           | `"u64"`    |
> | u128          | `"u128"`   |
> | i8            | `"i8"`     |
> | i16           | `"i16"`    |
> | i32           | `"i32"`    |
> | i64           | `"i64"`    |
> | i128          | `"i128"`   |
> | f32           | `"f32"`    |
> | f64           | `"f64"`    |
> | char          | `"char"`   |
> | string        | `"string"` |
> | unit          | `"unit"`   |
> | bytes         | `"bytes"`   |
> | payload       | `"payload"` |
>
> These 18 hashes are constants. Implementations MAY precompute them.

## Struct hashes

> r[schema.type-id.hash.struct]
>
> To hash a struct, update the hasher with:
>
>   1. The tag `"struct"`
>   2. For each field, in declaration order:
>      a. The field name (length-prefixed UTF-8 string)
>      b. The content hash of the field's type (8 bytes, little-endian)

## Enum hashes

> r[schema.type-id.hash.enum]
>
> To hash an enum, update the hasher with:
>
>   1. The tag `"enum"`
>   2. For each variant, in declaration order:
>      a. The variant name (length-prefixed UTF-8 string)
>      b. The variant index as a `u32` (4 bytes, little-endian)
>      c. The payload tag: `"unit"`, `"newtype"`, `"tuple"`, or `"struct"`
>      d. For newtype payloads: the content hash of the inner type
>         (8 bytes, little-endian)
>      e. For tuple payloads: the content hash of each element type,
>         in order (8 bytes each, little-endian)
>      f. For struct payloads: each field as in `r[schema.type-id.hash.struct]`
>         step 2 (name then type hash, in order)

## Container hashes

> r[schema.type-id.hash.container]
>
> To hash a container type, update the hasher with:
>
>   * **List:** `"list"` then the element type hash
>   * **Option:** `"option"` then the element type hash
>   * **Array:** `"array"` then the element type hash, then the length
>     as a `u64` (8 bytes, little-endian)
>   * **Map:** `"map"` then the key type hash, then the value type hash

## Tuple hashes

> r[schema.type-id.hash.tuple]
>
> To hash a tuple, update the hasher with:
>
>   1. The tag `"tuple"`
>   2. The content hash of each element type, in order (8 bytes each,
>      little-endian)

Content hashes give type IDs a universal meaning. A peer that receives
a schema tagged with a content hash it has already seen — from this
connection, a previous connection, or even a persistent store — knows
it already has that schema. This is critical for operation stores
(see `r[schema.interaction.retry]`) and for efficient schema tracking
across connection resumes.

> r[schema.type-id.per-connection]
>
> Every connection starts with zero schema knowledge. A peer MUST NOT
> assume that schemas sent on one connection are available on another,
> even within the same session. Each connection half has its own
> sent/received tracking. However, because type IDs are content hashes,
> a peer MAY use a previously received schema (from another connection
> or a persistent cache) to build a translation plan without waiting for
> the schema to be resent — as long as it does not send data until the
> remote peer has confirmed (by sending its own schemas) that it can
> read it.

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

# Hashing recursive types

Non-recursive types have straightforward content hashes — hash the
structure, reference child types by their hashes. Recursive types
create a cycle: the hash of `TreeNode` depends on the hash of
`Vec<TreeNode>`, which depends on the hash of `TreeNode`.

The solution is a three-step algorithm that computes preliminary
hashes to establish a canonical ordering, then derives final hashes
from that ordering.

> r[schema.hash.recursive]
>
> To compute content hashes for a mutually recursive group of types:
>
>   1. **Preliminary hashes.** Hash each type in the group using the
>      normal rules (see `r[schema.type-id.hash]`), except that any
>      reference to another type in the same recursive group is replaced
>      with 8 zero bytes (the **sentinel**). References to types outside
>      the group use their real content hashes as normal. The result is
>      one preliminary hash per type.
>
>   2. **Canonical ordering.** Sort the types by their preliminary hash
>      (ascending, unsigned integer comparison). In the unlikely event
>      that two types have the same preliminary hash (a 64-bit collision),
>      break the tie by lexicographic comparison of their full canonical
>      byte sequences (the input to blake3 before truncation, as computed
>      in step 1).
>
>   3. **Final hashes.** Compute the **group hash** as
>      `blake3(preliminary_hash_0 || preliminary_hash_1 || ...)[0..8]`
>      where the preliminary hashes are concatenated in canonical order.
>      Then each type's final content hash is
>      `blake3(group_hash || index)[0..8]` where `index` is the type's
>      position in the canonical order, encoded as a `u64` in
>      little-endian order.
>
> These final hashes are the types' `TypeId`s — plain `u64` values,
> indistinguishable from non-recursive type hashes. No special
> representation is needed on the wire or in data structures.

> r[schema.hash.recursive.non-recursive]
>
> A non-recursive type does not participate in this algorithm. Its
> content hash is computed directly from its structure as described
> in `r[schema.type-id.hash]`.

Example: a recursive tree type.

```
// Step 1: preliminary hash
//   TreeNode: blake3("struct" || "label" || hash(string)
//                    || "children" || hash_of(list, SENTINEL))
//   → preliminary_hash = 0xABCD...
//
// Step 2: canonical order (only one type, so trivial)
//   [TreeNode]
//
// Step 3: final hash
//   group_hash = blake3(preliminary_hash)[0..8]
//   TreeNode.type_id = blake3(group_hash || 0u64)[0..8]
```

Example: mutually recursive types.

```
// Expr { body: ExprBody }
// ExprBody { Literal(u64), Add(Expr, Expr) }
//
// Step 1: preliminary hashes (recursive refs → sentinel)
//   Expr:     blake3("struct" || "body" || SENTINEL)     → 0x1111...
//   ExprBody: blake3("enum" || "Literal" || 0u32 || "newtype" || hash(u64)
//                    || "Add" || 1u32 || "struct" || "left" || SENTINEL
//                    || "right" || SENTINEL)              → 0x2222...
//
// Step 2: canonical order (sort by preliminary hash)
//   [Expr (0x1111), ExprBody (0x2222)]
//
// Step 3: final hashes
//   group_hash = blake3(0x1111... || 0x2222...)[0..8]
//   Expr.type_id     = blake3(group_hash || 0u64)[0..8]
//   ExprBody.type_id = blake3(group_hash || 1u64)[0..8]
```

# Schema format

A schema describes a single type. Schemas are CBOR-encoded and
self-contained — every type referenced by a schema is either a primitive
or is referenced by its type ID (a content hash or bundle-local index).

The following Rust declarations define the schema data model. Other
language implementations must produce equivalent CBOR encodings.

```rust
/// A content hash that uniquely identifies a type's postcard-level
/// structure. Computed via blake3, truncated to 64 bits.
///
/// The same type always produces the same TypeId regardless of
/// connection, session, process, or language. On the wire (CBOR),
/// a TypeId is encoded as a CBOR unsigned integer.
struct TypeId(u64);

/// The primitive types of the postcard encoding.
///
/// These are leaves in the type graph — they have no child types.
/// Language-level wrappers (Rust newtypes, TypeScript type aliases)
/// are transparent: `struct UserId(u64)` has the same schema as `u64`.
enum PrimitiveType {
    Bool,
    U8, U16, U32, U64, U128,
    I8, I16, I32, I64, I128,
    F32, F64,
    Char,
    String,
    /// The unit type — zero bytes on the wire. This is the canonical
    /// representation of "nothing." A zero-element tuple is not valid;
    /// use Unit instead.
    Unit,
    /// A raw byte sequence (`Vec<u8>`, `&[u8]`).
    Bytes,
    /// An opaque payload — a length-prefixed byte sequence whose
    /// length prefix is a little-endian u32 (not a varint like other
    /// postcard sequences). Used for protocol extensions where the
    /// sender must reserve space before writing the payload.
    Payload,
}

/// The structural description of a type.
enum SchemaKind {
    /// A leaf type with no child types.
    Primitive { primitive_type: PrimitiveType },

    /// An ordered collection of named fields.
    Struct { fields: Vec<FieldSchema> },

    /// A tagged union of named variants.
    Enum { variants: Vec<VariantSchema> },

    /// An ordered, fixed-arity product type. Must have 1 or more
    /// elements — use `PrimitiveType::Unit` for the zero case.
    Tuple { elements: Vec<TypeId> },

    /// A variable-length homogeneous sequence (`Vec<T>`, `HashSet<T>`,
    /// etc.). Sets and lists have the same postcard encoding and are
    /// not distinguished at the schema level.
    List { element: TypeId },

    /// A variable-length collection of key-value pairs.
    Map { key: TypeId, value: TypeId },

    /// A fixed-length homogeneous sequence (`[T; N]`).
    Array { element: TypeId, length: u64 },

    /// A value that may be absent (`Option<T>`).
    Option { element: TypeId },
}

/// A field in a struct or struct-variant.
struct FieldSchema {
    /// The field name. Used for matching across schema versions —
    /// renaming a field is a breaking change.
    name: String,
    /// The type of this field.
    type_id: TypeId,
}

/// A variant in an enum.
struct VariantSchema {
    /// The variant name. Used for matching across schema versions.
    name: String,
    /// The postcard variant index (varint ordinal on the wire).
    index: u32,
    /// The variant's payload shape.
    payload: VariantPayload,
}

/// The payload of an enum variant.
enum VariantPayload {
    /// No payload (e.g. `None`, `Disconnected`).
    Unit,
    /// A single unnamed value (e.g. `Some(T)`, `Ok(T)`).
    Newtype { type_id: TypeId },
    /// A tuple of unnamed values (e.g. `Pair(u32, String)`).
    Tuple { types: Vec<TypeId> },
    /// Named fields, like a struct (e.g. `Move { x: i32, y: i32 }`).
    Struct { fields: Vec<FieldSchema> },
}

/// A complete schema: the type ID, a name, and the structural
/// description.
struct Schema {
    /// The content hash that identifies this type.
    type_id: TypeId,
    /// The type's name (e.g. "Point", "Vec<String>"). Required and
    /// MUST NOT be empty. Used for matching across schema versions
    /// and for diagnostics.
    name: String,
    /// The structural description of this type.
    kind: SchemaKind,
}
```

The normative rules below define the CBOR encoding of these types.

> r[schema.format]
>
> A schema MUST be a CBOR map containing:
>
>   * `type_id` — a CBOR unsigned integer (the type's content hash)
>   * `name` — the type's name (UTF-8 string, required, MUST NOT be empty)
>   * `kind` — one of: `"struct"`, `"enum"`, `"tuple"`, `"list"`, `"map"`,
>     `"array"`, `"option"`, `"primitive"`
>   * Kind-specific fields as defined below

> r[schema.format.type-id]
>
> A `TypeId` MUST be encoded as a CBOR unsigned integer.

> r[schema.format.primitive]
>
> A primitive schema MUST contain:
>
>   * `kind`: `"primitive"`
>   * `primitive_type`: one of `"bool"`, `"u8"`, `"u16"`, `"u32"`,
>     `"u64"`, `"u128"`, `"i8"`, `"i16"`, `"i32"`, `"i64"`, `"i128"`,
>     `"f32"`, `"f64"`, `"char"`, `"string"`, `"unit"`, `"bytes"`,
>     `"payload"`

> r[schema.format.struct]
>
> A struct schema MUST contain:
>
>   * `kind`: `"struct"`
>   * `fields`: a CBOR array of field descriptors, each a map with:
>     - `name`: field name (UTF-8 string)
>     - `type_id`: a `TypeId` (CBOR unsigned integer)
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
>     - `index`: the postcard variant index (`u32`)
>     - `payload`: one of:
>       - `"unit"` — no payload
>       - `{"newtype": type_id}` — single value
>       - `{"tuple": [type_id, ...]}` — tuple of unnamed values
>       - `{"struct": [field_descriptors...]}` — struct variant
>         (field descriptors follow `r[schema.format.struct]`)

> r[schema.format.container]
>
> Container schemas MUST contain:
>
>   * `kind`: `"list"`, `"map"`, `"array"`, or `"option"`
>   * `element`: a `TypeId` (for list, array, option)
>   * `key` and `value`: `TypeId`s (for map)
>   * `length`: a `u64` (for array only)
>
> Sets (`HashSet<T>`, `BTreeSet<T>`, etc.) have the same postcard encoding
> as lists and MUST use `kind: "list"`. The schema does not distinguish
> between ordered and unordered sequences.

> r[schema.format.tuple]
>
> A tuple schema MUST contain:
>
>   * `kind`: `"tuple"`
>   * `elements`: a CBOR array of `TypeId`s, one per element, in order
>
> The `elements` array MUST contain at least one element. A zero-element
> tuple is not valid — use `PrimitiveType::Unit` instead.

## Recursive types on the wire

Recursive types reference each other by their final `TypeId` — the
same plain `u64` content hash as any other type. There is no special
wire representation for recursive references. The schemas for all
types in a recursive group simply reference each other by hash.

> r[schema.format.recursive]
>
> When sending schemas for a recursive group, the sender MUST include
> all schemas in the group that have not already been sent on this
> connection. The receiver MUST be able to resolve every `TypeId`
> referenced in the schemas using either the current batch of schemas
> or schemas previously received on this connection.

## Schema delivery

Schemas are not sent as standalone messages. They are bundled with
the `Request` or `Response` that needs them, along with a method
binding that tells the receiver which type is the root for this
method's arguments or response.

```rust
/// A method binding maps a method ID to the root TypeId for its
/// arguments or response type.
struct MethodSchemaBinding {
    method_id: MethodId,
    /// The TypeId of the root type (e.g. the args struct for a
    /// request, or `Result<T, RoamError<E>>` for a response).
    root_type_id: TypeId,
    direction: BindingDirection,
}

enum BindingDirection {
    /// This binding is for the method's argument type.
    Args,
    /// This binding is for the method's response type.
    Response,
}

/// The CBOR-encoded payload attached to a Request or Response.
struct SchemaPayload {
    /// All schemas needed by the receiver that have not been
    /// previously sent on this connection.
    schemas: Vec<Schema>,
    /// Method bindings that map method ID + direction to a root
    /// TypeId in the schema set. Tells the receiver which schema
    /// describes the postcard payload it is about to deserialize.
    method_bindings: Vec<MethodSchemaBinding>,
}
```

> r[schema.format.self-contained]
>
> When a `Request` or `Response` includes schemas, the set of schemas
> MUST be self-contained. Every `TypeId` referenced by any schema in
> the set MUST either be defined in the same set or have been previously
> sent on this connection. The receiver MUST be able to build translation
> plans for all included types before deserializing the payload.

> r[schema.format.delivery]
>
> Schemas are delivered as a CBOR-encoded `SchemaPayload` attached to
> a `Request` or `Response`. The payload MUST include:
>
>   * All schemas needed for the method's types that have not been
>     previously sent on this connection
>   * A `MethodSchemaBinding` that maps the method ID and direction
>     (`Args` for requests, `Response` for responses) to the root
>     `TypeId` of the payload being sent
>
> The root type for a response is always the full
> `Result<T, RoamError<E>>` wire type, regardless of whether the
> handler succeeded or failed.
>
> If all schemas for a method's types have already been sent on this
> connection, the schemas array MAY be empty — but the method binding
> MUST still be included if this is the first time this (method_id,
> direction) pair has been sent on this connection. The receiver needs
> the binding to know which previously-sent TypeId is the root for
> this method. Sending a schema whose `TypeId` has already been sent
> on this connection is a protocol error.

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
> are also marked as sent. A schema payload is self-contained
> (see `r[schema.format.self-contained]`), so sending a struct schema
> implicitly sends the schemas of all its field types, their field types,
> and so on.

> r[schema.tracking.bindings]
>
> Each peer MUST track the set of (method_id, direction) pairs for which
> it has sent method bindings on this connection. A binding MUST be sent
> the first time a method's schemas are delivered for a given direction,
> even if all the schemas themselves were already sent by a previous call
> to a different method.

# Two levels of schema exchange

Schema exchange operates at two levels:

1. **Protocol level (per-session):** The `MessagePayload` schema is
   exchanged during the CBOR handshake (see `r[session.handshake]`).
   This allows the protocol framing itself to evolve without breaking
   changes.

2. **Application level (per-connection):** Method argument and response
   schemas are exchanged lazily, bundled with `Request` and `Response`
   payloads, scoped to each connection. This allows service types to
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
> the method's argument types have been sent to this peer on this connection.
> If any have not, the caller MUST include all unsent schemas in the
> `Request` (see `r[schema.format.delivery]`).

> r[schema.exchange.callee]
>
> Before sending any `Response` for a method, the callee MUST check whether
> the schemas for the method's **statically-known response type** have been
> sent to this peer on this connection. If any have not, the callee MUST
> include all unsent schemas in the `Response`.
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
> in the caller's schemas. If the response contains channel types,
> their element schemas MUST be included in the callee's schemas.

> r[schema.exchange.required]
>
> Application-level schema exchange is mandatory. If a peer receives a
> `Request` or `Response` for a method whose schemas have not been
> received on that connection, this is a protocol error and the
> connection MUST be torn down. There is no fallback to identity
> deserialization — the sender is always responsible for including schemas
> with the data that needs them.

> r[schema.exchange.idempotent]
>
> If the caller has already sent schemas for a method's argument types
> (from a previous call to the same or different method using the same
> types), no schemas need to be included. The `r[schema.principles.once-per-type]`
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
>   * They are both tuples and tuple matching succeeds
>     (see `r[schema.translation.tuple]`)

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
> newtype with a compatible inner type, tuple matches tuple with
> compatible elements (see `r[schema.translation.tuple]`), struct matches
> struct with compatible fields (same rules as top-level struct matching).

# Tuple evolution

Tuples are positional — elements are matched by index, not by name.
This means tuple evolution is more restricted than struct evolution.

> r[schema.translation.tuple]
>
> Tuple types MUST have the same arity (number of elements) in both
> the remote and local types. For each position, the remote element
> type and local element type MUST be compatible (per
> `r[schema.translation.type-compat]`). Adding, removing, or
> reordering tuple elements is a breaking change.

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

> r[schema.errors.call-level]
>
> A translation plan failure is a **call-level error**, not a connection-level
> fault. The connection remains open and other method calls are unaffected.
> This is distinct from missing schemas entirely (a protocol error per
> `r[schema.exchange.required]`), which tears down the connection.

> r[schema.errors.call-level.callee]
>
> If the callee cannot build a translation plan for incoming request
> arguments, it MUST respond with an error describing the incompatibility
> (including a diff of the remote schema versus the local type).

> r[schema.errors.call-level.caller]
>
> If the caller cannot build a translation plan for an incoming response,
> the failure is local — the call's result resolves to an error. There is
> no further message to send; the response has already been received.

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
> Operation stores MUST store schemas alongside serialized payloads.
> A sealed operation contains postcard-encoded bytes that are only
> meaningful together with the schemas that describe them. When replaying
> a sealed response, the replaying peer MUST send schemas for the
> response types on the current connection if they have not already been
> sent, just as it would for a live response.
>
> Because type IDs are content hashes, the operation store does not need
> a per-connection schema ID namespace. The stored schemas use the same
> content hashes regardless of which connection originally produced them
> or which connection replays them. A disk-backed operation store that
> survives process restarts can use content hashes as stable keys for
> its schema cache.

> r[schema.interaction.metadata]
>
> Metadata is unaffected by schema exchange. Metadata key-value pairs are
> not typed in the postcard sense and do not participate in schema exchange.
