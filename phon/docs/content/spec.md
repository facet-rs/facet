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
    Unit,
    Never,
}
```

`String` is UTF-8 encoded text. `Bytes` is an arbitrary byte sequence the
schema treats as data — phon never interprets its contents, so it doubles as
the carrier for "bytes whose meaning is defined by some protocol layered on
top of phon." `Unit` is the value-less type (Rust's `()`, Swift's `Void`);
`Never` is the type with no inhabitants, useful in shapes like
`Result<T, Never>`.

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

# Incremental decoding

phon decoders consume bytes as they arrive rather than requiring the whole
encoded value upfront.

The motivation is head-of-line blocking. Multiple RPC exchanges typically
share a transport, often with very different sizes. A peer receiving a 100 MB
response for one call interleaved with a 200-byte status reply for another
should not have to wait for the 100 MB to be reassembled in some
transport-side buffer before phon can decode the status reply. If the decoder
demands a complete encoded value as a single buffer, every byte that arrives
on the wire has to be sorted into per-stream reassembly buffers, and small
messages sit behind large ones' accumulation.

Letting phon consume bytes as they arrive — interleaved with bytes for other
streams, in whatever chunks the transport delivers — removes the buffering
layer entirely. Each stream's decoder advances when its bytes are available
and parks when they're not. A 100 MB response can be paused indefinitely
while small messages on other streams complete around it.

> r[incremental-decoding]
>
> A phon decoder consumes input incrementally. The caller feeds bytes in
> chunks; the decoder returns one of `Done(value)`, `NeedsMore`, or
> `Error(reason)` per call. `NeedsMore` means the decoder is parked; feeding
> additional bytes resumes it.

The decoder knows when it's done because the schema bounds the decode.
Primitives have fixed widths. Lists, maps, options, strings, and byte
sequences carry their own length prefixes. Structs and enums are walked
field-by-field. By the time the last field is consumed, the decoder has read
exactly the right number of bytes — it returns `Done` and any remaining
bytes in the input belong to whatever comes next.

The caller never tells phon where a message ends. Phon tells the caller it's
done. Framing — message boundaries, stream multiplexing, fragmentation —
belongs to whatever is layered above phon, and that layer consults phon's
`Done` signal to know when to start the next decode.

Encoders mirror this: they write to a sink that accepts bytes. The encoder
walks the schema and the value and emits bytes as it goes, never buffering
the whole result. The sink chunks or frames the output however its transport
needs.

The JIT implication is real. A decoder generated by copy-and-patch is a state
machine with explicit suspend/resume points wherever the input might run out
— every primitive read, every length prefix, every field boundary. The
stencils for "try to read N bytes; suspend if not yet available" become
first-class operations. Encoders stay straight-line because writing to a
sink either succeeds or fails terminally with a single branch. The
interpreter follows the same suspend/resume shape so it and the JIT remain
interchangeable.

# Schema identity

Two peers need to agree on which schema a run of bytes was written against,
and they need to agree without a central authority assigning numbers first. A
peer might be a Rust service, a Swift app, a TypeScript client — none of them
coordinate schema numbering ahead of time. So a schema's identity has to fall
out of the schema itself.

phon derives a schema's identity by hashing its structure. Same structure in,
same identity out, on every implementation, with no coordination.

> r[schema-identity.content-hash]
>
> A `SchemaId` is the BLAKE3 hash of a schema's canonical structural encoding,
> truncated to the first 8 bytes read as a little-endian `u64`. The same
> logical schema produces the same `SchemaId` in every implementation.

Because the id is derived rather than assigned, a Rust service that defines
`struct Point { x: u32, y: f64 }` and a Swift app whose codegen produced the
matching type compute the same `SchemaId` independently. Neither registers it
anywhere. When a peer receives a `SchemaId` it recognizes, it already has the
schema; when it receives one it doesn't recognize, that absence is the signal
that the schema needs to be sent before the value referencing it can be
decoded.

> r[schema-identity.canonical-encoding]
>
> The bytes fed to BLAKE3 are a deterministic encoding of the schema's
> structure: the kind discriminant, then per kind, the names (struct name,
> field names, variant names, type parameter names), variant indices, the
> primitive kind, array dimensions, and the nested `SchemaRef`s — all in
> declaration order, with strings length-prefixed and nested concrete
> references encoded by the referenced `SchemaId`. The `id` field is never fed
> into its own hash. Every implementation encodes this identically, which is
> what makes the hash reproducible across languages.

Everything structural is in the hash; there is nothing non-structural in a
phon schema to leave out (no doc comments, no annotations travel on the wire).

Names are part of identity on purpose. phon matches struct fields and enum
variants by name when reconciling two schema versions — that's how a field
survives being reordered. So two structs with the same field types but
different field names are genuinely different schemas and hash differently, and
a struct named `Request` is distinct from a structurally identical one named
`Response`.

Recursive schemas need care. A linked list references itself —
`struct Node { value: u32, next: Option<Node> }` — and you cannot hash `Node`
by recursing into `Node`, because you would never stop.

> r[schema-identity.recursive]
>
> When schemas reference each other in a cycle, the references that close the
> cycle are encoded as a fixed sentinel rather than by recursing into the
> referenced schema's id. An implementation identifies the cycle group, hashes
> each member with intra-group references replaced by the sentinel, combines
> those into a group hash, and derives each member's final `SchemaId` from the
> group hash plus the member's sorted position within the group. This makes
> recursive schema identity well-defined and identical across implementations.

A 64-bit id has a birthday bound around four billion distinct schemas before a
coincidental collision becomes likely. A realistic deployment has thousands of
schemas in flight, not billions, so the margin is enormous — but a collision
would be catastrophic (two different schemas mistaken for one, decoding
silently against the wrong structure), so it is worth stating the bound rather
than pretending it is zero. 64 bits is the trade phon makes: schema references
stay 8 bytes, which matters because they appear throughout every transmitted
schema.

# Self-describing mode

Self-describing mode is the bootstrap. Before two peers can speak compact mode
they have to exchange schemas — and a schema is itself a phon value, so it has
to be encoded without already knowing its schema. Self-describing mode is that
"without": every value carries enough tagging to be decoded with no schema at
all.

It also backs the `Dynamic` kind — a value whose type the surrounding schema
deliberately doesn't pin down — and any tooling that wants to read bytes off
the wire without the schema, the observability that JSON has and positional
encodings throw away.

> r[self-describing.tag-led]
>
> In self-describing mode every value begins with a one-byte tag identifying
> its kind. The tag determines how to read the body that follows. A decoder
> with no schema can walk a self-describing value from start to finish using
> the tags alone.

The tags and their bodies:

| tag  | kind          | body                                                            |
|------|---------------|-----------------------------------------------------------------|
| 0x00 | unit          | none                                                            |
| 0x01 | bool          | 1 byte: `0x00` false, `0x01` true                               |
| 0x02 | u8            | 1 byte                                                          |
| 0x03 | u16           | 2 bytes, little-endian                                          |
| 0x04 | u32           | 4 bytes, little-endian                                          |
| 0x05 | u64           | 8 bytes, little-endian                                          |
| 0x06 | u128          | 16 bytes, little-endian                                         |
| 0x07 | i8            | 1 byte, two's complement                                        |
| 0x08 | i16           | 2 bytes, little-endian two's complement                         |
| 0x09 | i32           | 4 bytes, little-endian two's complement                         |
| 0x0A | i64           | 8 bytes, little-endian two's complement                         |
| 0x0B | i128          | 16 bytes, little-endian two's complement                        |
| 0x0C | f32           | 4 bytes, IEEE 754 little-endian                                 |
| 0x0D | f64           | 8 bytes, IEEE 754 little-endian                                 |
| 0x0E | char          | 4 bytes, little-endian Unicode scalar value                     |
| 0x0F | string        | u32 LE byte length, then that many UTF-8 bytes                  |
| 0x10 | bytes         | u32 LE byte length, then that many raw bytes                    |
| 0x11 | list          | u32 LE element count, then that many values                     |
| 0x12 | set           | u32 LE element count, then that many values                     |
| 0x13 | map           | u32 LE entry count, then that many `key` `value` value pairs    |
| 0x14 | array         | u32 LE rank, then `rank` u32 LE dimensions, then the elements   |
| 0x15 | tuple         | u32 LE element count, then that many values                     |
| 0x16 | struct        | name string, u32 LE field count, then that many `name` `value`  |
| 0x17 | enum          | variant name string, then the variant's payload value(s)       |
| 0x18 | option-none   | none                                                            |
| 0x19 | option-some   | one value                                                       |

A few things worth calling out:

- Integers and floats are little-endian, matching every host phon targets, so
  a decoder on a little-endian machine reads them with no transformation.
- A struct carries its field names inline, exactly like JSON. That repetition
  is the cost of self-description — paid here because this is the bootstrap and
  inspection path, not the hot path.
- An enum carries the variant *name*, not its index. A self-describing decoder
  has no schema to map an index back to a name, so the name is the only
  meaningful identifier.
- `array` carries its rank and dimensions in the body, because without a
  schema there is nowhere else for them to live. Compact mode, which has the
  schema, omits them.

> r[self-describing.no-extra-kinds]
>
> `Dynamic` and `External` have no self-describing tags of their own. A
> `Dynamic` value simply *is* a self-describing value — it carries whatever tag
> its actual kind calls for. An `External` attachment's in-band value is
> `unit`; its bytes travel out of band per `r[type-system.external]`.

> r[self-describing.bootstraps-schemas]
>
> Schemas are transmitted in self-describing mode. A schema is an ordinary phon
> value — an instance of the `Schema` type from the type system — so encoding
> it self-describing needs no pre-shared schema. That is what makes
> bootstrapping possible: the first thing two peers exchange is schemas, and
> schemas ride the one mode that requires nothing agreed in advance.

# Compact mode

Compact mode is the format for when the schema is already known out of band.
No tags, no field names — the schema says what comes next, so the bytes are
just the values, back to back.

`Point { x: u32, y: f64 }` in self-describing mode costs a struct tag, the
field names `x` and `y`, and a tag per field. In compact mode it costs 4 bytes
for `x`, 8 bytes for `y`, plus alignment padding (below). That is the entire
point: when both peers know the schema, you stop paying to repeat it.

> r[compact.schema-driven]
>
> In compact mode values carry no tags and no names. The decoder walks the
> schema in lockstep with the bytes: the schema says "u32 next," the decoder
> reads four bytes. A compact value cannot be decoded without its schema.

Fixed-shape kinds are just their bytes. The variable-shape kinds still need
their counts and lengths on the wire, because the schema says "a list of u32"
but not how many:

- `string`, `bytes`: u32 LE byte length, then the bytes
- `list`, `set`: u32 LE element count, then the elements
- `map`: u32 LE entry count, then the key/value pairs
- `option`: one byte, `0x00` none or `0x01` some, then the value if some
- `enum`: the variant *index* as u32 LE (the schema lists variants by index;
  the name is not needed and not sent), then the payload
- `array`: nothing extra — dimensions are in the schema; just the elements
- `struct`, `tuple`: nothing extra — fields and arity are in the schema; just
  the values in declaration order

Lengths and counts are fixed-width u32, not varints. A varint is smaller for
short values but costs a decode loop and makes every following offset
unpredictable. Fixed u32 is a single load and keeps offsets computable, which
matters for the alignment that follows. The cost is four bytes per
variable-length value; the cap is four gigabytes per value, which is fine
because anything that large belongs out of band as an `External`. (This is a
real trade — flagging it as the most likely place to want varints later if
wire size beats decode speed for some deployment.)

## Alignment

This is where compact mode earns the zero-copy property from `r[zero-copy]`.

A receiver decoding a `[u32]` should be able to borrow it as a slice pointing
straight into the input buffer, no copy. That only works if those bytes sit at
a 4-byte-aligned address in memory. Whether they do depends on everything that
came before them on the wire — a single `bool` ahead of a `[u32]` would push it
to an odd offset, and the borrow becomes illegal.

So compact mode pads.

> r[compact.alignment]
>
> Before writing a value that requires N-byte alignment, the encoder inserts
> zero bytes until the current offset, measured from the start of the message,
> is a multiple of N. The decoder skips the same padding by the same rule.
> Alignment N is the natural alignment of the type: 2 for u16/i16, 4 for
> u32/i32/f32/char, 8 for u64/i64/f64, 16 for u128/i128. Primitive-array
> payloads align to their element's alignment.

Padding is relative to message start, which means absolute alignment only holds
if the message itself starts at an aligned address. That is the framing layer's
responsibility.

> r[compact.aligned-buffer]
>
> Borrowing requires both that the message start at an address aligned to the
> largest alignment the message uses (8 bytes covers everything up to
> u64/f64; 16 if the message contains u128/i128), and that internal padding per
> `r[compact.alignment]` be applied from there. A framing layer that wants its
> receivers to borrow must place message starts at aligned offsets. When the
> buffer is not suitably aligned, the decoder still works — it copies the
> affected scalars and arrays into aligned storage instead of borrowing. The
> decoded value is identical; only the zero-copy optimization is forfeited.

Padding is wasted bytes, so field order matters. phon writes fields in schema
declaration order and never reorders, because the wire order is part of what
the schema pins down. A schema author minimizes padding the same way they would
lay out a C struct: declare wider fields before narrower ones. phon does not do
this for you — declaration order is your lever, and it is also part of the
schema's identity, so two field orderings are two different (but mutually
compatible) schemas.

## Why both modes share almost everything

The two modes differ in exactly one axis: whether the kind is known from a tag
(self-describing) or from the schema (compact). The body grammars are otherwise
the same shapes — little-endian scalars, u32-length-prefixed sequences,
in-order struct fields. An implementation's two decoders share most of their
machinery; the compact decoder is the self-describing decoder with the tags
removed and the schema supplying the kinds instead.
