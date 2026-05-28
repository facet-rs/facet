+++
title = "phon"
description = "Typed binary format and execution engine"
+++

# Base concepts

phon is a binary exchange format, allowing programs written in various languages
to serialize and deserialize values, and supporting schema evolution over time.

Its primary use case and the reasoning for most of its design is RPC (remote
procedure calling), where a request is sent over the wire (TCP, WebSocket, etc.)
to a peer, who sends back a response.

In JSON-RPC, for example, a request for method `ping` would look like this:

```json
{
  "jsonrpc": "2.0",
  "method": "ping",
  "params": { "data": "foobar" },
  "id": 1
}
```

...with a response that looks like this:

```json
{
  "jsonrpc": "2.0",
  "result": { "data": "foobar" },
  "id": 1
}
```

Both the request and the response are self-describing.

If a peer includes an extra field in one of the objects, the other peer is free
to ignore it, because JSON is a self-describing format, just like CBOR, MsgPack,
etc.

The upside to this is observability (you can intercept part of an exchange, and
know what they're talking about), and a certain amount of compatibility that you
get for free — along with better diagnostics, when two things end up being
incompatible. 

On the flip side, self-describing formats waste a lot of bytes, repeating the
same information over and over, when making frequent exchanges between two peers
who know very well which protocol they are speaking. 

Indeed, some folks encode Rust structs, for example, not as JSON objects, but as
JSON arrays:

```json
["2.0", "ping", ["foobar"], 1]
```

```json
["2.0", ["foobar"], 1]
```

This is much less wasteful, but we've lost observability and compatibility. Adding
or removing fields, reordering fields, changing the type of fields, are all
transforms that could lead to silent misinterpretation during decoding.

Some formats attempt to get the best of both worlds by prefixing records with a schema.

```json
{
  "type": "schema",
  "schema": {
    "type": "struct",
    "name": "Request",
    "fields": [
      { "name": "jsonrpc", "type": "string" },
      { "name": "method", "type": "string" },
      {
        "name": "params",
        "type": {
          "type": "struct",
          "name": "PingParams",
          "fields": [
            { "name": "data", "type": "string" }
          ]
        }
      },
      { "name": "id", "type": "u64" }
    ]
  }
}
```

```json
{
  "type": "value",
  "schema": "Request",
  "value": ["2.0", "ping", ["foobar"], 1]
}
```

The schema fully describes what appears on the wire, why, and in which order.

In the context of an RPC system, this lets a peer know ahead of time if the
message that is sent to them is going to be compatible with their conception of
what this message should be.

A system in which a peer has knowledge of its own schema and of the remote
schema enables forwards and backwards compatibility for a large number of schema
mutations without having to explicitly number fields.

It also makes implementing such a scheme challenging in terms of both
correctness and performance. 

First, there is a bootstrapping problem when it comes to schemas. Before we can
use the format, we have to send a schema. The schema itself must be serialized
using some format. It cannot be serialized using the format we're sending the
schema for, because that format is not defined yet. 

This creates the need for two separate formats, or at the very least, two
different modes for a format: a self-describing mode and a compact mode.

Second, there is an important distinction to be made between the representation
of a value of a certain type in memory for a given process in an application
coded with a given programming language, and the representation of that same
value on the wire: sent over TCP to appear, or over WebSocket, or shared between
two processes, over memory mappings. 

Because different languages want to represent types such as structs and classes,
and arrays and vectors, and maps and dictionaries, and sets and tuples, and
different things in memory, the temptation to borrow, as a struct, from the
buffer, for example, is entirely removed. 

Not only must we assume that the wire representation is completely different
from the runtime representation, we must also assume that the remote schema is
different from the local schema. By only implementing and only attempting to
optimize for the worst possible case, we ensure that performance is consistent
throughout, no matter the language pair, and amount of drift between two peers. 

Thirdly, the time and frequency at which schemas are sent matter. 

Sending all schemas ahead of time, upon connection establishment slash
handshake, would result in a huge spike in terms of bandwidth used and latency
at the beginning of a new connection.

Sending schemas, along with every message, would be redundant and largely negate
the benefits of using schemas at all. 

One possible strategy is to send schemas right before any message that would
actually need it. Aiming to send schemas at most once, but tolerating duplicates
in the case of concurrent calls. 

Lastly, one must consider how to recover performance in the face of
non-negotiable data mapping: once again, the wire representation will never
equal the runtime representation. Therefore, there is a deserialization step.

The wire representation is also unpredictable from one peer to the next.
Therefore, deserialization code cannot be compiled and optimized ahead of time.
This is once again non-negotiable and a fundamental consequence of phon's design

A naive implementation would compare the remote schema and the local schema
every time it needs to decode a value. 

A smarter implementation would generate a "decoder program" using the remote
schema, and a local descriptor (containing layout, offset, alignment information
for the runtime representation of a value in a given language in a given
process).

A smarter implementation still would translate that decoder program to machine
code using whatever just-in-time compilation technique feels appropriate.

# Type system

phon's type system is defined here as Rust type declarations. These declarations
are the canonical spec for what a phon schema can describe. There is no separate
IDL, no `.phon` files. The Rust definitions below are the spec.

Other implementations have native data types that mirror these. A phon Schema in
Swift is a Swift `enum` with the same variants, carrying the same data. In
TypeScript, it's a discriminated union with matching fields. Whatever
implementation language, the requirement is that values produce and consume the
same self-describing phon bytes the Rust definitions do.

> r[type-system.canonical-form]
>
> The type system is specified as Rust type declarations. Implementations in
> other languages provide data types that produce and consume the same
> self-describing phon bytes the Rust definitions do.

## Schema

```rust
pub struct Schema {
    pub id: SchemaId,
    pub type_params: Vec<String>,
    pub kind: SchemaKind,
}
```

Every schema has a content-derived identifier (covered in [Schema
identity](#schema-identity)), an optional list of type parameter names if the
schema is parametric, and a kind that says what it represents.

## Schema kinds

```rust
pub enum SchemaKind {
    Primitive(Primitive),
    Struct { name: String, fields: Vec<Field> },
    Enum { name: String, variants: Vec<Variant> },
    Tuple { elements: Vec<SchemaRef> },
    List { element: SchemaRef },
    Set { element: SchemaRef },
    Map { key: SchemaRef, value: SchemaRef },
    Array { element: SchemaRef, dimensions: Vec<u64> },
    Option { element: SchemaRef },
    Dynamic,
    External { kind: String, metadata: Value },
}
```

`Struct`, `Enum`, `Tuple`, `List`, `Set`, `Map`, `Array`, and `Option` are the
shapes you'd expect. Three deserve a note:

> r[type-system.dynamic]
>
> A `Dynamic` value carries any phon value, encoded in self-describing form,
> regardless of what schema produced it. It's the escape hatch when the
> receiver shouldn't have to know the value's type ahead of time — at the cost
> of the bandwidth and validation that compact form provides for that subtree.

> r[type-system.external]
>
> An `External` value's bytes don't appear in the message at all. Its in-band
> representation is the unit value; the actual payload travels through the
> transport's external-attachment channel. The `kind` field names which
> channel; `metadata` describes the payload to the transport without revealing
> its content.

`Array` differs from `List` by having fixed dimensions known at the schema
level. A `[[u32; 4]; 3]` is an `Array { element: u32, dimensions: [3, 4] }`,
not a `List<List<u32>>`.

## Schema references

```rust
pub enum SchemaRef {
    Concrete { id: SchemaId, args: Vec<SchemaRef> },
    Var { name: String },
}
```

A schema's fields, variants, and container elements don't inline the full
nested Schema — they reference it. `Concrete` names a schema by its identifier
and supplies arguments for its type parameters if any. `Var` names a type
parameter declared by an enclosing schema's `type_params`.

> r[type-system.generics]
>
> A schema is parametric when it declares one or more `type_params`. Inside the
> schema's structure, those parameters appear as `SchemaRef::Var { name }`.
> Concrete uses of the schema bind the parameters by supplying `args` in the
> `Concrete` reference.

## Primitives

```rust
pub enum Primitive {
    Bool,
    U8, U16, U32, U64, U128,
    I8, I16, I32, I64, I128,
    F32, F64,
    Char,
    String,
    Bytes,
    Payload,
    Unit,
    Never,
}
```

`String` is UTF-8 encoded text. `Bytes` is an arbitrary byte sequence the
schema treats as data. `Unit` is the value-less type (Rust's `()`, Swift's
`Void`); `Never` is the type with no inhabitants, useful in shapes like
`Result<T, Never>`.

> r[type-system.payload]
>
> `Payload` carries opaque bytes whose meaning is defined by something other
> than phon — typically another protocol layered on top. Unlike `Bytes`, which
> is data the schema describes, `Payload` is bytes the schema explicitly
> doesn't introspect.

## Fields

```rust
pub struct Field {
    pub name: String,
    pub schema: SchemaRef,
    pub required: bool,
}
```

A struct's fields name a schema (possibly parametric) and declare whether the
field is required. Non-required fields can be omitted by a writer that doesn't
have one to send; required fields can't.

## Variants

```rust
pub struct Variant {
    pub name: String,
    pub index: u32,
    pub payload: VariantPayload,
}

pub enum VariantPayload {
    Unit,
    Newtype(SchemaRef),
    Tuple(Vec<SchemaRef>),
    Struct(Vec<Field>),
}
```

> r[type-system.variant-payloads]
>
> Each variant carries exactly one payload shape: no payload, a single-schema
> newtype, a positional tuple of schemas, or a named-field struct. These map
> directly to Rust enum variant forms and to Swift enum cases with associated
> values.

The `index` field gives each variant a stable position separate from its
declaration order. Reordering variants in source doesn't change wire
compatibility as long as indices stay assigned to the same variant names.

## The Rust subset

Not every Rust construct is part of the spec's notation. The definitions above
use only:

- Plain `enum` and `struct` declarations with owned data
- The specific generic containers `Vec<T>` and `Option<T>`
- Primitive types `String`, `u32`, `u64`
- The phon-defined types `SchemaId`, `SchemaRef`, `Schema`, `SchemaKind`,
  `Primitive`, `Field`, `Variant`, `VariantPayload`, `Value`

> r[type-system.rust-subset]
>
> The Rust used in the type system spec is the subset listed here. No
> lifetimes. No traits. No references. No free generic parameters outside the
> listed containers. The subset is intentionally narrow so that mechanical
> translation to other implementation languages has no judgment calls left
> to make.

# Schema identity

(TODO: describe how schema hashing works, to identify common schemas,
what goes into a schema identity etc.)

# Self-describing mode

(TODO: specify self-describing mode for all types)

# Compact mode

(TODO: don't forget alignment so we can borrow &[u32] etc. — maybe we should specify
alignment of entire messages? mhh.)
