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
    Tensor { element: SchemaRef, rank: Option<u32> },
    Option { element: SchemaRef },
    Channel { direction: ChannelDirection, element: SchemaRef },
    Dynamic,
    External { kind: String, metadata: Value },
}

pub enum ChannelDirection {
    Tx,  // the sending end
    Rx,  // the receiving end
}
```

`Struct`, `Enum`, `Tuple`, `List`, `Set`, `Map`, and `Option` are the shapes
you'd expect. `Array`, `Tensor`, `Channel`, `Dynamic`, and `External` deserve a
note:

> r[type-system.array]
>
> An `Array` has a fixed shape known at the schema level: `dimensions` lists the
> size of each axis. `[u8; 32]` is `Array { element: u8, dimensions: [32] }`;
> `[[f32; 256]; 256]` is `dimensions: [256, 256]`. The shape is part of schema
> identity, so two arrays of different shape are different types. On the wire the
> elements are a flat row-major run — the dimensions are not repeated, because
> the schema already has them — which makes a scalar-element array borrowable as
> one contiguous slice.

> r[type-system.tensor]
>
> A `Tensor` has a runtime shape carried on the wire, for n-dimensional data
> whose dimensions vary per value — `ndarray`'s arrays, audio buffers, model
> activations. `rank` is `Some(r)` for a fixed number of axes (a 2-D `Array2`
> is `rank: Some(2)`) or `None` for fully dynamic rank (`ArrayD`). The
> dimension *sizes* are never in the schema or its identity; only `element` and
> `rank` are. On the wire a tensor writes its dimension sizes, then its elements
> as a flat row-major run. The distinction from `Array` is exactly fixed shape
> (in the schema, in identity) versus runtime shape (on the wire, per value).

> r[type-system.channel]
>
> A `Channel` is a stream of values of its `element` type in the given
> `direction` — `Tx` sending, `Rx` receiving. It comes from a streaming RPC
> signature (vox's `Tx<T>` / `Rx<T>`). The *type* lives in the schema so codegen
> and dispatch know a stream of `element` flows here; the *value* on the wire is
> a transport-assigned handle (a `u64`). The stream's items, lifecycle, and
> backpressure are the transport's, carried as separate messages. phon encodes
> the handle and knows the element type; it does not run the stream. Channel
> values appear only in compact, schema-known contexts, never in self-describing
> form. (How an implementation obtains the handle — turning a local endpoint
> into one and back — is a descriptor-model detail, covered in [Language
> implementations](#language-implementations), not part of this wire contract.)

> r[type-system.dynamic]
>
> A `Dynamic` value carries any phon value, encoded in self-describing form,
> regardless of what schema produced it. It is how a `facet_value::Value` — a
> value whose type isn't known at the schema level — goes on the wire, and the
> escape hatch anywhere a receiver shouldn't need the type ahead of time, at the
> cost of the bandwidth and validation compact form gives the rest of the
> message.

> r[type-system.external]
>
> An `External` value's payload bytes don't appear in the message. In their
> place the wire carries a transport-assigned `u64` handle; the actual payload
> travels through the transport's external-attachment channel. The `kind` field
> names which channel; `metadata` (a `Value`, carried in-band) describes the
> payload to the transport without inlining it. See [External
> payloads](#external-payloads) for the handle, validation, and borrow rules.


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

> r[type-system.generic-resolution]
>
> To use a parametric schema — to decode a value or to compare it for
> compatibility — the engine resolves the reference: it looks up the schema by
> id, zips its `type_params` with the `args` from the `Concrete` reference, and
> substitutes each `Var(name)` with the bound type. Resolution is recursive (an
> arg may itself be parametric) and happens at plan-build time, so the resulting
> plan is fully concrete and the per-value decode path never encounters a `Var`.

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

A struct's fields each have a name, a schema (which may be parametric), and a
`required` flag. `required` means the field has no default and must be present;
a non-required field is one the source type can default, so a reader can supply
it when a writer doesn't (see [Compatibility](#compatibility)). The default
*value* is reader-side — the schema records only the boolean. `required` is part
of schema identity, because required-versus-optional is a real contract
difference. Optionality of the value itself — none-or-some — is a separate thing,
expressed by the field's schema being an `Option`.

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
> Each variant carries exactly one payload shape: no payload (unit), a
> single-schema newtype, a positional tuple of schemas, or a named-field struct.
> These four shapes are what an enum variant can hold; a language with tagged
> unions or sum types maps onto them directly.

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

# Decoding

phon decodes a complete message from one contiguous buffer. It is not an
incremental, feed-me-bytes-as-they-arrive decoder: the framing layer hands it a
whole message and it walks it start to finish.

This is a deliberate choice, and it rests on three other parts of the design
carrying the weight that an incremental decoder would otherwise carry:

- **Large payloads are never inline.** Anything big — file contents, blobs — is
  an `External` value: a handle in the message, bytes out of band. So there is
  no such thing as a hundred-megabyte inline message to stream through a decoder.
- **Streams are not single values.** A `Channel` is a sequence of separate
  per-item messages, not one ever-growing value. Streaming is many small
  messages, each decoded whole.
- **Messages are size-bounded.** The transport negotiates a maximum message size
  (see [Framing](#framing)) — a couple of megabytes over a network, more over a
  local socket. Anything that would exceed it goes `External` instead. So a
  message always fits in a bounded buffer.

Head-of-line blocking — the reason an incremental decoder looks tempting — is
handled one layer down, by framing, not by the decoder. The transport
interleaves frames across streams and reassembles each message in its own
bounded buffer; a small message completes and decodes while a large one is still
arriving on another stream. The decoder never has to be suspendable to avoid
blocking, because it only ever runs on a message that has already fully arrived.

> r[decode.whole-message]
>
> A phon decoder runs on a complete message buffer and returns
> `Done(value, consumed)` or `Error(reason)`. `consumed` is the number of bytes
> the value occupied. There is no parked/needs-more state: the bytes are all
> present before the decoder runs.

> r[decode.chained]
>
> Several independent values may sit back to back in one buffer, and a caller
> may decode them in sequence: after a decode returns `Done(value, consumed)`,
> the next value starts at `consumed`. This is how a reader handles a message
> that is more than one value — most importantly RPC dispatch, where an envelope
> (method id, metadata) is decoded first, its method id selects the schema for
> what follows, and the remainder of the buffer is decoded as the arguments
> against that schema. phon does not bundle the two into one schema; it decodes
> one value, hands back the offset, and lets the caller decode the next.

The decoder knows where a value ends because the schema bounds it: primitives
have fixed widths, lists/maps/options/strings carry length prefixes, structs and
enums are walked field by field. So `consumed` falls out of the walk; the caller
doesn't have to be told where the value ended.

Encoders write to a sink (a buffer, or the transport's framer). The encoder
walks the schema and value and emits bytes as it goes.

The JIT benefits directly from this. Because a decoder always has all its input,
copy-and-patch generates **straight-line** code — no suspend/resume state
machine, no "try to read N bytes, park if not available" at every field
boundary. A read is just a read. This is a large simplification over a
streaming decoder, and it is the main reason whole-message decoding is worth the
size cap.

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

> r[schema-identity.closure]
>
> A schema references other schemas by `SchemaId`. Transmitting a schema means
> transmitting the transitive closure of those references — every schema
> reachable through its `SchemaRef`s — so the receiver can resolve all of them.
> A receiver holding the full closure can decode any value of the root schema;
> one missing a referenced schema cannot.

> r[schema-identity.unknown-is-error]
>
> Referencing a `SchemaId` the receiver does not hold is a decode error, full
> stop. The decoder does not pause to wait for the schema to arrive. Schema
> delivery is a protocol-layer concern that happens before a value is decoded —
> by the time a decoder runs, every schema it needs must already be resolved. An
> unknown id at decode time means delivery failed upstream, and that is an error,
> not a wait state.

> r[schema-identity.canonical-encoding]
>
> The bytes fed to BLAKE3 follow this exact recipe, so every implementation
> produces the same bytes. Building blocks: `u32`/`u64` are little-endian; a
> *string* is a `u32` LE byte length then its UTF-8 bytes; a `bool` is one byte,
> `0` or `1`. A schema's `id` field is never fed into its own hash.
>
> A schema's kind is its tag string followed by its body:
>
> - **primitive**: the primitive tag — one of `bool`, `u8`, `u16`, `u32`, `u64`,
>   `u128`, `i8`, `i16`, `i32`, `i64`, `i128`, `f32`, `f64`, `char`, `string`,
>   `bytes`, `unit`, `never`.
> - **struct**: `struct`; the name; the type-parameter list (a `u32` count then
>   each parameter name); a `u32` field count; then per field, in declaration
>   order: the field name, the `required` bool, the field's reference.
> - **enum**: `enum`; the name; the type-parameter list; a `u32` variant count;
>   then per variant, in declaration order: the variant name, its index (`u32`),
>   and its payload — `unit`; or `newtype` then a reference; or `tuple` then a
>   `u32` count then references; or `struct` then a `u32` field count then fields
>   encoded as above.
> - **tuple**: `tuple`; a `u32` element count; then each element reference.
> - **list** / **set** / **option**: the tag (`list` / `set` / `option`) then
>   the element reference.
> - **map**: `map`; the key reference; the value reference.
> - **array**: `array`; the element reference; a `u32` dimension count; then each
>   dimension as `u64`.
> - **tensor**: `tensor`; the element reference; then rank — one byte `0` for
>   `None`, or `1` then the rank as `u32`.
> - **channel**: `channel`; the direction tag (`tx` or `rx`); the element
>   reference.
> - **dynamic**: `dynamic`.
> - **external**: `external`; the `kind` string; then the `metadata`, encoded as
>   a `u32` length followed by its self-describing-form bytes.
>
> A *reference* is encoded as `concrete` then the referenced `SchemaId` (8 bytes
> LE) then a `u32` argument count then each argument reference; or, for a type
> parameter, `var` then the parameter name. The argument count is always written
> — zero for a non-generic concrete reference — with no conditional marker.

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

> r[schema-identity.computation]
>
> Ids are assigned in dependency order. Partition the schemas into the
> strongly-connected components of the reference graph (edges follow `concrete`
> references and their arguments) and process the components dependencies-first,
> so a component's outward references all resolve to already-assigned ids. The
> SCC *partition* is canonical — it does not depend on traversal order — and a
> component's id depends only on its own structure and its dependencies' ids, so
> the order among independent components doesn't matter.
>
> A component that is a single schema with no reference back to itself is encoded
> per `r[schema-identity.canonical-encoding]`, every reference feeding an
> already-assigned id, and its id is the truncated BLAKE3 of that encoding.
>
> A component that is a cycle is resolved by **structural unfolding with
> depth-indexed back-references**. Each member's id is the truncated BLAKE3 of a
> walk rooted at that member, where every reference encountered (including a
> concrete reference's arguments) is resolved against the component:
>
> - a reference whose target is outside the component feeds the target's
>   already-assigned id (`concrete` + id + args, as usual);
> - a reference whose target is another member of the component and is *not* yet
>   on the current walk path is inlined — emit `inline`, then walk that member's
>   kind with the path extended by it, then its arguments;
> - a reference whose target is a member already on the path is emitted as
>   `backref` then that ancestor's depth on the path (`u32`, the root being
>   depth 0), then its arguments.
>
> The back-reference is what terminates the walk. Because the walk is a pure
> function of the member's structure — not of discovery order, with no sorting,
> no group hash, no positional heuristic — every implementation computes the same
> id. Two members collapse to one id only when they are genuinely the same type
> (same names, same structure), which is correct. This deliberately does not
> canonicalize cycles up to isomorphism; phon doesn't need it, because schema
> names already distinguish members.

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
| 0x14 | array         | u32 LE rank, then `rank` u64 LE dimensions, then the elements   |
| 0x15 | tuple         | u32 LE element count, then that many values                     |
| 0x16 | struct        | name string, u32 LE field count, then that many `name` `value`  |
| 0x17 | enum          | variant name string, then the variant's payload value(s)       |
| 0x18 | option-none   | none                                                            |
| 0x19 | option-some   | one value                                                       |
| 0x1A | tensor        | u32 LE rank, then `rank` u64 LE dimensions, then the elements   |

A few things worth calling out:

- Integers and floats are little-endian, matching every host phon targets, so
  a decoder on a little-endian machine reads them with no transformation.
- A struct carries its field names inline, exactly like JSON. That repetition
  is the cost of self-description — paid here because this is the bootstrap and
  inspection path, not the hot path.
- An enum carries the variant *name*, not its index. A self-describing decoder
  has no schema to map an index back to a name, so the name is the only
  meaningful identifier.
- `array` carries its shape in the body, because without a schema there is
  nowhere else for it to live. Compact mode, which has the schema, omits it.
  `tensor` always carries its shape — its shape is runtime even in compact mode
  — so the two have identical bodies here and differ only by tag and by where
  the shape comes from in compact mode.
- there is no tag for `channel`: channel values appear only in compact,
  schema-known contexts, never in self-describing form (see
  `r[type-system.channel]`).

> r[self-describing.no-extra-kinds]
>
> `Dynamic` and `External` have no self-describing tags of their own. A
> `Dynamic` value simply *is* a self-describing value — it carries whatever tag
> its actual kind calls for. An `External` value's in-band form is its
> transport-assigned `u64` handle (encoded as a `u64`); its payload bytes travel
> out of band per `r[type-system.external]`.

> r[self-describing.bootstraps-schemas]
>
> Schemas are transmitted in self-describing mode. A schema is an ordinary phon
> value — an instance of the `Schema` type from the type system — so encoding
> it self-describing needs no pre-shared schema. That is what makes
> bootstrapping possible: the first thing two peers exchange is schemas, and
> schemas ride the one mode that requires nothing agreed in advance.

## Value

`Value` is phon's dynamic value: any phon value held in memory without reference
to a schema. It is what the self-describing codec produces and consumes, what a
`Dynamic` field carries, and what `External`'s `metadata` is.

> r[value]
>
> A `Value` has one case per self-describing kind in the tag table above — unit,
> bool, each integer and float width, char, string, bytes, list, set, map,
> array, tuple, struct (named fields), enum (named variant plus payload),
> option, and tensor. (There is no `channel` case: channel values never appear
> in self-describing form.) A `Value` is exactly the information a
> self-describing decode recovers, and exactly what a self-describing encode
> needs — the two are inverses over `Value`.

Each implementation maps `Value` onto its native dynamic-value type. In Rust
that type is `facet_value::Value`; the `phon` binding crate provides a total
conversion in both directions. A native dynamic type may carry cases phon's tag
table doesn't — `facet_value::Value`, for instance, has a null and a date/time
case — and the binding is responsible for mapping those onto phon kinds (null
to an option's none; a date/time to an agreed struct or integer, since phon has
no date/time primitive). The wire stays language-neutral; the reconciliation
lives in the binding, not the format.

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
  the name is not needed and not sent), then the payload. `Never` — an enum
  with zero variants — is uninhabited: it has no valid index, can never be
  encoded, and any bytes presented for it are a decode error.
- `set`, `map`: as above, with one constraint — duplicate set elements or map
  keys are a decode error (`r[validate.uniqueness]`). Element/pair order on the
  wire is the encoder's; the decoder preserves it but no canonical ordering is
  imposed.
- `array`: nothing extra — the shape is in the schema; just the elements, a
  flat row-major run of `product(dimensions)` of them (`product(dimensions)` is
  checked for overflow, `r[validate.dimensions]`)
- `tensor`: the dimension sizes (each u64 LE), then the elements as a flat
  row-major run. If the schema fixes `rank` to `Some(r)`, exactly `r` sizes are
  written; if `rank` is `None`, a u32 LE rank precedes the sizes
- `channel`: a `u64` LE transport handle (see `r[type-system.channel]`)
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

This is where compact mode enables zero-copy reads — a receiver borrowing a
primitive array straight out of the input buffer instead of copying it out.

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

This bites at the length prefix. A `[u64]`/`[f64]`/`[u128]` list writes its
u32 count, then padding to the element's 8- or 16-byte alignment, then the run —
the count desyncs the offset, so 4 (or up to 12) padding bytes follow it on
every such list. The layout is: count, then pad-to-element-alignment, then the
aligned run. A fixed `Array` of the same element pays no such padding, because
its shape is in the schema and there is no on-wire count before the run — so a
schema author who wants a borrowable `[f64; N]` with no per-message padding
reaches for `Array`, while a dynamic `List<f64>` accepts the count+padding. That
asymmetry is a real reason to prefer fixed arrays for hot numeric data.

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

Because phon decodes a whole message from one buffer (see
[Decoding](#decoding)), every value is contiguous by construction — there is no
fragmentation to defeat a borrow. So a borrow succeeds whenever the buffer is
aligned; the only fallback is the copy-into-aligned-storage case above, for a
buffer the framing layer couldn't start at an aligned address. Alignment padding
is always written, so an aligned buffer is always borrowable.

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

# Framing

phon decodes a complete message from one buffer (see [Decoding](#decoding)). It
does not define how those buffers are produced — where one message ends and the
next begins, which bytes belong to which concurrent exchange, how a large
message is split for transmission and put back together. That is framing, and
framing belongs to the transport.

The contract between phon and the framing layer has four load-bearing parts.

> r[framing.not-defined]
>
> phon does not define a wire framing format. Delimiting messages, multiplexing
> concurrent exchanges, fragmenting messages for transmission, and reassembling
> them are the transport's responsibility. A transport may length-prefix its
> frames, chunk-delimit them, multiplex them with stream ids, or carry a single
> message per connection — phon is indifferent, subject to the requirements
> below.

> r[framing.whole-messages]
>
> The framing layer delivers a complete message as one buffer. phon decodes the
> whole buffer; it cannot find a message boundary itself, because it runs only
> after the message has fully arrived and been reassembled. Finding boundaries
> is therefore the framing layer's job (its own length prefix, delimiter, or
> per-stream end-of-message flag). phon reports how many bytes each value
> consumed (`r[decode.whole-message]`) so a caller can chain several values
> within one delivered message, not so framing can discover where the message
> ends.

> r[framing.max-size]
>
> The transport negotiates a maximum message size — on the order of a couple of
> megabytes over a network, more over a local socket. A value too large to fit
> goes out of band as an `External` instead. The bound is what lets a transport
> reassemble each message in a buffer of bounded size and lets phon assume it
> always receives a complete message. The exact limit is the transport's to
> negotiate; phon only relies on one existing.
>
> The max-message size bounds *one* message, not the aggregate. Per-stream
> reassembly means total in-flight reassembly memory is roughly
> `max_message_size × concurrent partial messages`, which is unbounded unless
> the transport also caps how many partial reassemblies it will hold at once.
> That cap is part of the transport's flow control (alongside its stream-count
> and credit limits); phon names the requirement but the limit is the
> transport's. Without it, a peer opening many streams and dribbling a large
> message on each is a memory-exhaustion vector.

> r[framing.alignment]
>
> For a receiver to borrow values out of the buffer (per
> `r[compact.aligned-buffer]`), the framing layer must deliver each message
> starting at an address aligned to the largest alignment the message uses. A
> framing layer that does not is still correct — the receiver copies instead of
> borrowing — but it forfeits zero-copy for its receivers. A framing layer that
> wants zero-copy aligns message starts.

Head-of-line blocking is the framing layer's to avoid, and it does so by
interleaving frames across streams and reassembling each message in its own
per-stream buffer. A small message's frames complete and decode while a large
message is still arriving on another stream — they never share a buffer, so the
small one doesn't wait. This is why phon's decoder doesn't need to be
incremental: the interleaving that prevents blocking happens below it, and by
the time phon runs, its one message is whole. The reference shape is HTTP/2 —
independent streams multiplexed over one connection, messages split into frames,
distinct frame kinds for the cheap dispatch header versus the bulk payload — and
it is the shape vox's transport targets. It is guidance; phon's actual
requirements are the four rules above.

phon models a channel's *type* but not its *operation*. The `Channel` schema
kind (see `r[type-system.channel]`) records direction and element type, so
codegen and dispatch know a stream of `Event` flows here — but phon does not run
the stream. A channel value on the wire is a transport-assigned `u64` handle;
the items flow as separate phon messages the framing layer associates with that
handle, and the stream's identity, lifecycle, and backpressure are the framing
layer's. phon encodes the handle and knows the element type; everything dynamic
about the stream lives above phon.

# Compatibility

Two peers drift. The reader receives bytes written against the writer's schema
and has to produce a value of its own type. Compatibility is whether that's
possible; translation is how it's done.

> r[compat.plan-first]
>
> Before decoding any compact bytes, the reader builds a translation plan from
> the writer schema to its own schema. If the plan cannot be built, the schemas
> are incompatible and decoding must not begin. You find out before touching
> the payload, not partway through it.

> r[compat.field-matching]
>
> Struct fields are matched by name, not by declaration position. The plan maps
> each writer field position to a reader field position before any bytes are
> read. Reordering fields between versions is transparent.

> r[compat.skip-writer-only]
>
> A field present in the writer schema but absent from the reader schema is
> skipped: the reader walks the writer schema to step over that field's bytes.
> Compact mode has no per-field length wrappers, so skipping means decoding the
> field by its schema and discarding it, not jumping a stored length.

> r[compat.reader-only-fields]
>
> A field present in the reader schema but absent from the writer schema is
> governed by its `required` flag. A `required` reader field absent from the
> writer makes the plan fail — the schemas are incompatible. A non-required
> reader field absent from the writer is filled with the reader's default value.

> r[compat.defaults-are-reader-side]
>
> The schema carries *whether* a field has a default — that is the `required`
> flag, and it is part of schema identity, because required-versus-optional is a
> real contract difference. The schema does not carry the default *value*: that
> is the reader's business, living in the reader's language mapping (Rust's
> `Default`, a codegen-emitted initializer), and applied when a non-required
> field is absent from the writer. Two readers of the same schema may fill
> different values; both are correct. Tooling may track the value for
> cross-language analysis, but the schema records only the boolean.

> r[compat.type-match]
>
> Matched fields are compatible only when a rule says so. The same primitive is
> compatible with itself. The same container kind (list, set, map, option) is
> compatible when its element types are compatible. A tuple is compatible with a
> tuple of the same arity and pairwise-compatible elements. An array is
> compatible with an array of compatible element type and identical
> `dimensions` (shape is part of an array's contract). A tensor is compatible
> with a tensor of compatible element type and identical `rank` — the dimension
> sizes are runtime, so they are not a schema-compatibility question (a decoder
> may still validate them per value). A channel is compatible with a channel of
> the same `direction`; its element compatibility is enforced when the stream's
> items are decoded, each as its own message, not at the channel itself. A
> struct is compatible when its field plan builds. Numeric widening is not
> implicit: `u32` and `u64` are different types, and a value written as one is
> not readable as the other unless a future rule adds an explicit conversion.
> These rules nest: `Option<Option<T>>` is compatible with `Option<Option<U>>`
> exactly when `T` and `U` are. `Dynamic` is compatible only with `Dynamic` —
> its bytes are self-describing, a form a compact reader of a concrete type
> cannot consume, so a concrete writer and a `Dynamic` reader (or the reverse)
> are incompatible.
>
> Parametric references are resolved (per `r[type-system.generic-resolution]`)
> before matching; compatibility is decided on the resolved forms.

> r[compat.enum]
>
> Enum variants are matched by name, and the plan maps writer variant indices
> to reader variant indices. A reader variant the writer lacks is fine — the
> writer can't produce it. A writer variant the reader lacks is structurally
> skippable, but actually receiving that variant at runtime is a decode error
> for that value. Variants present in both must have matching payload shapes:
> unit with unit, newtype with newtype, tuple with tuple, struct with struct.

> r[compat.direction]
>
> The runtime builds one-directional plans on demand — writer schema to reader
> schema — and fails if one can't be built; that is all decoding needs.
> Separately, tooling may offer a direction report between two schema versions,
> built by planning both ways: backward (the newer reads the older), forward
> (the older reads the newer), bidirectional, or incompatible. This is a
> schema-evolution aid (a CI check before deploy), not part of the decode path.
> Such a report should name the schema path and the reason for each
> incompatibility.

# External payloads

Putting a large payload through the wire format means copying it at every
buffer boundary it crosses. For an RPC between two processes on the same
machine, a 100 MB blob serialized inline gets copied out of the sender's heap,
into a kernel buffer, into the receiver's read buffer, into a decode buffer,
and out into whatever the receiver hands the application — several 100 MB
memcpys for data that never had to leave physical memory. That is the cost
`External` exists to avoid.

> r[external.handle-in-band]
>
> An `External` value's payload bytes are not in the message. In their place the
> wire carries a *handle*: a `u64` the transport assigns, plus the schema's
> `kind` naming the side channel. The handle is what lets one message carry many
> externals of the same kind and keep them apart — a unit placeholder couldn't,
> since two same-kind externals would be indistinguishable. The `metadata` is
> in-band (it rides the message as a self-describing `Value`, so it is sized and
> not free); it describes the payload to the transport without inlining the
> payload itself.

> r[external.transport-channel]
>
> The side channel is the transport's choice: shared memory between
> same-machine peers (map a region once, the handle names it, both sides see
> the bytes), file-descriptor passing over a Unix socket (the handle indexes the
> message's fd table), a content-addressed blob store, anything that beats
> copying. phon defines the in-band handle and defers the channel to the
> transport, the same way it defers framing.

> r[external.handle-is-validated]
>
> A handle is a capability the transport issued, and the decoder treats it as
> untrusted: a handle that names no channel the transport actually provided is a
> decode error, never a dereference. The transport validates the handle before
> the receiver borrows through it. This closes the confused-deputy path where a
> peer names a buffer or descriptor it was never given.

> r[external.borrow-on-receive]
>
> On the receiving side, a validated `External` handle yields a borrow — a
> pointer and a length — into wherever the side channel placed the bytes,
> exactly as an inline byte field yields a borrow into the wire buffer. The
> receiver cannot tell the difference and pays a copy only if it asks for an
> owned value.

# Decoding untrusted input

A phon decoder reads bytes from a peer that may be malicious. A crafted message
must never crash the decoder, hang it, make it over-allocate, or let it escape
memory safety. The format is shaped to make every check cheap — whole-message
decoding means the decoder always knows how many bytes remain, fixed-u32 lengths
mean no varint tricks, and the negotiated size cap bounds the buffer — but the
checks are mandatory. This section is normative: a conforming decoder performs
all of them.

> r[validate.lengths]
>
> Every length or count read from the wire is checked against the bytes
> remaining in the buffer before any allocation or iteration. Because every
> element has a nonzero minimum wire size, a count may never drive a
> pre-allocation larger than `bytes_remaining / min_element_size` — that ratio
> is the true ceiling, regardless of what the count claims. A length or count
> that exceeds what remains is a decode error. (A u32 can claim four billion
> elements in a twelve-byte message; this rule is what stops the resulting
> allocation bomb.)

> r[validate.dimensions]
>
> A tensor's or array's element count is `product(dimensions)`, computed with
> checked multiplication — overflow is a decode error, never a silent wrap. The
> product times the element's wire size must not exceed the bytes remaining. A
> tensor `rank` read from the wire is bounded; an absurd rank is rejected before
> reading that many dimension sizes.

> r[validate.depth]
>
> Decoding, and schema-structure traversal including the identity hash of
> recursive schemas, are bounded by a maximum nesting depth. A message or schema
> that nests deeper is a decode error, not a stack overflow. The exact bound is
> an implementation's to choose; it must exist. (A self-describing list-of-one
> repeated thousands of times is cheap to author and would otherwise blow the
> stack.)

> r[validate.tags]
>
> A self-describing tag byte outside the defined table is a decode error. The
> decoder never skips an unknown tag — without a known kind it cannot know the
> body's length, so there is nothing safe to skip.

> r[validate.text]
>
> A `string` is validated as UTF-8, and a `char` as a Unicode scalar value
> (surrogates and values above U+10FFFF rejected), before the value is exposed —
> in particular before any borrowed `&str` is handed out.

> r[validate.uniqueness]
>
> Duplicate keys in a `map`, or duplicate elements in a `set`, are a decode
> error. The schema claims uniqueness; a message that violates it is malformed,
> and accepting it would make decode ambiguous and invite hash-flooding. The
> seen-set this requires is bounded by `r[validate.lengths]`, so the check is
> cheap.

> r[validate.bundles]
>
> A schema bundle received over the wire is verified before use: each member's
> stated `SchemaId` must equal its recomputed content hash, and the transitive
> closure must be complete — no referenced `SchemaId` left unresolved. A bundle
> failing either check is rejected. (`r[schema-identity.unknown-is-error]` is the
> runtime counterpart, for an id referenced by a value but never delivered.)

Two safety contracts stated elsewhere are part of this discipline:
`r[external.handle-is-validated]` (a handle is an untrusted capability; an
unissued one is a decode error, never a dereference) and `r[descriptors.borrowed]`
(a borrowed value's lifetime is bound to the input buffer). The borrow contract
is a hard requirement, not advice: an implementation must tie a borrowed value's
lifetime to the buffer it points into — in a language without lifetimes, by
copying instead of borrowing — so a freed buffer can never leave a dangling
view.

# Codegen

phon schemas come from Rust types via facet (see [Base concepts](#base-concepts)).
Codegen turns those schemas into source for the other languages a system
speaks.

> r[codegen.emits]
>
> For each target language, codegen emits two things per schema: the type
> definitions a programmer writes against, and the schema itself as a constant
> — the self-describing phon bytes of the `Schema` value, which the peer's own
> phon implementation parses into a `Schema` at startup or on first use. The
> peer ships with its schemas baked in and never derives or fetches them at
> runtime.

> r[codegen.schema-is-source-of-truth]
>
> A non-Rust peer never re-derives a schema from its generated types. The
> schema bytes emitted from the Rust-side schema are the source of truth; the
> generated types exist for the programmer's convenience. This guarantees the
> peer's `SchemaId` matches the Rust origin exactly — a peer that re-derived
> from, say, TypeScript types might hash to something different, because the
> mapping from phon types to a given language is not always one-to-one.

# Crates and packages

This section is implementation organization, not wire contract — like the
language sections below, it's how each implementation is built, and two
implementations packaged differently still interoperate. It's in the spec
because the boundaries are load-bearing: a package with no dependency on a
language's reflection *cannot* leak that reflection into the engine, which is
how phon keeps execution backend-blind structurally rather than by discipline.

Every implementation separates the same four concerns, and each boundary is a
real package, not merely a module:

- **Contract** — the schema model, identity, `Value`, and the self-describing
  codec. The most widely depended-on piece; no engine, no language binding.
- **Engine** — compact codec, compatibility planning, the IR, the interpreter,
  the descriptor model. Backend-blind: it consumes descriptors and an IR and
  reaches for no reflection.
- **JIT** — the optional accelerator, behind an opt-in so the baseline pays
  nothing for it and restricted platforms omit it entirely.
- **Binding** — the language-specific bridge that produces descriptors and
  offers the ergonomic typed API. The only part that touches facet, the Swift
  runtime, or JS objects.

> r[crates.concern-separation]
>
> An implementation is split into at least four packages: a contract package, an
> engine package, an optional JIT package, and a binding package, as described
> above. The separations are package boundaries, not module boundaries, so that
> dependency edges — not convention — enforce what each layer may touch.

> r[crates.engine-is-binding-free]
>
> The engine and JIT packages do not depend on the language's reflection or
> derive machinery; only the binding package does. This is the structural
> guarantee behind `r[descriptors.fact-driven]`: the engine consumes a descriptor
> and an IR and physically cannot call facet (Rust), probe the Swift runtime, or
> reflect over JS objects, because those crates are not in its dependency graph.

> r[crates.jit-opt-in]
>
> The JIT is its own package, reached only through an opt-in — in Rust, a `jit`
> Cargo feature on the front-door crate. With it off, the JIT's machinery is not
> compiled and the engine runs the interpreter; with it on, the typed API routes
> through the JIT. A platform that cannot allocate executable memory simply never
> enables it.

## Rust — six crates

```
phon-schema    Schema, SchemaKind, SchemaRef, Primitive, ChannelDirection,
               Field, Variant, SchemaId + identity hash, Value, the
               self-describing codec.
               deps: blake3, facet_value.   (no facet-derive, no engine)

phon-ir        the IR both backends run, the Descriptor model, and thunk
               bindings — the shared vocabulary of execution.
               deps: phon-schema.

phon-engine    compact codec, compatibility planning (schema + descriptor ->
               IR), and the interpreter (runs IR). the backend-blind baseline.
               deps: phon-schema, phon-ir.   (no facet)

phon-jit       copy-and-patch JIT: IR -> machine code via rustc/LLVM stencils.
               deps: phon-ir.   (no facet; opt-in)

phon           the front door. facet -> schema + descriptor, typed
               encode::<T> / decode::<T>. `jit` feature pulls phon-jit and
               routes the typed API through it.
               deps: phon-engine, phon-schema, facet [+ phon-jit if `jit`].

phon-codegen   the codegen tool: reads Rust types (facet) or schema bundles,
               emits target-language source + schema-bytes constants.
               deps: phon-schema, facet.
```

Only `phon` and `phon-codegen` depend on facet. `phon-ir`, `phon-engine`, and
`phon-jit` cannot, which is `r[crates.engine-is-binding-free]` made concrete.
The `jit` feature on `phon` is the "enable a feature, get the JIT" ergonomics:
the machinery is absent unless the feature is on, and the feature is simply not
enabled on targets that forbid executable memory. Because `phon-ir` defines the
IR up front and the interpreter in `phon-engine` is its first consumer, the JIT
is a second consumer of an IR that exists from the first commit — JIT-ready by
construction, not retrofitted.

## Swift — the same four concerns

As Swift Package Manager modules: `PhonSchema`, `PhonIR`, `PhonEngine`,
`PhonJIT` (copy-and-patch via swiftc/LLVM), and `Phon` (the binding: probes the
Swift runtime for descriptors, offers the typed API). The split mirrors Rust;
only the binding differs (runtime probing instead of facet). Swift consumes
codegen output, so there is no Swift codegen module.

## TypeScript — where it diverges

As npm packages: `@phon/schema`, `@phon/engine`, and `@phon/core` (the front
door). TypeScript has no descriptor model — values are GC'd objects with no
offsets — so its binding is codegen-emitted property accessors, and the engine
consumes accessor functions rather than descriptor data. Its JIT is generated
JavaScript passed to `new Function()`, light enough to live inside
`@phon/engine` rather than a separate package. TypeScript consumes codegen
output too.

## Where vox sits

phon is the format-and-engine layer; [vox](https://github.com/bearcove/vox) is
the RPC layer above it. Migrating vox onto phon means vox drops its
`vox-schema` / `vox-postcard` / `vox-jit` crates and depends on `phon-schema` /
`phon-engine` / `phon-jit`, keeping its own RPC core, transports, fd-passing,
and FFI. phon owns the wire and the engine; vox owns sessions, dispatch,
channels, and everything stateful.

# Language implementations

Everything above is the portable contract: the wire format, schema identity,
compatibility, framing requirements, the behaviors every implementation shares.
What follows is per-language — how a given implementation realizes the contract
in its own runtime. None of it affects the wire; two implementations that
disagree here still interoperate, because they agree on everything above.

Two concepts are inherently per-language and live here rather than in the
portable contract:

- **Descriptors** — how an implementation reads and writes its own language's
  in-memory values for a given schema. The wire says what bytes to produce; the
  descriptor says where the value's pieces are in this process's memory.
- **Execution** — how an implementation turns a translation plan into running
  code.

Execution shares a shape across all implementations:

> r[exec.interpreter-baseline]
>
> Every implementation has an interpreter that handles any schema the
> compatibility rules accept. It is the baseline — it always works, including
> on platforms where a JIT cannot run.

> r[exec.jit-optional]
>
> A JIT is optional. When present it produces results identical to the
> interpreter, differing only in speed. The technique is the implementation's
> choice: copy-and-patch machine code, generated source, or none at all.

> r[exec.strict-recording]
>
> An implementation may offer a strict mode that records every subtree its JIT
> could not compile and fell back on, as a development aid for finding what to
> teach the JIT next. Strict mode is a diagnostic, not a production execution
> mode.

## The descriptor model

A schema says what bytes go on the wire. It says nothing about where a value's
pieces live in memory — and they have to live somewhere for an encoder to read
them or a decoder to build them. That "where" is the descriptor.

A descriptor is a tree shaped like the schema, each node annotated with the
process-local facts needed to read that part of the value (encode) and
construct it (decode). It is never transmitted, never hashed, never part of
schema identity. It is true for exactly one type, in one language, in one
build, in one process.

The descriptor model is a shared design, realized separately by each
memory-layout implementation. Rust has its own descriptor types describing Rust
memory; Swift has its own describing Swift memory. They never cross — like the
`Schema` type, the *shape* is shared and documented once, here, but each
implementation has its own. TypeScript has no descriptors at all; its values are
objects accessed by property, with no offsets to describe.

Every node carries its facts in one of two forms:

- **Direct facts** — offsets, strides, tag locations, niche patterns —
  concrete enough that the engine reads or writes memory itself. A plain
  struct's fields sit at known offsets; the engine reads them directly.
- **Thunks** — named functions the implementation provides, in its own
  language, for everything direct facts can't express. A Rust `HashMap`'s
  internal layout is opaque, so the Rust engine iterates it to encode and
  inserts to decode through Rust functions; a Swift existential is opaque, so
  the Swift engine goes through Swift functions. Thunks are same-language
  helpers, not cross-language bridges.

> r[descriptors.separate-implementations]
>
> Each memory-layout implementation has its own descriptors and its own engine.
> The Rust engine consumes Rust descriptors and calls Rust thunks; the Swift
> engine consumes Swift descriptors and calls Swift thunks. Nothing crosses:
> there is no shared engine, and no descriptor is ever handed across a language
> boundary. Rust and Swift peers interoperate only through the wire. What is
> shared is this model's shape, documented once and realized in each
> implementation — the same way the `Schema` type is.

> r[descriptors.fact-driven]
>
> Within an implementation, the engine works from descriptor facts and thunk
> bindings, never from the source type directly. There is no hand-written
> per-type encode/decode path beside the engine — the descriptor is the single
> input that tells the engine how to read and build any value of any type, and
> the JIT specializes it per `(schema, descriptor)` pair at runtime.

Reading a value and constructing one are not symmetric, and the descriptor
reflects that. Reading is usually direct: the value already exists in memory,
and offsets are enough to walk it. Constructing often is not — allocating a
vector's backing buffer, inserting into a hash map, initializing a type with
internal invariants — and needs the language runtime's cooperation. So a node
commonly has direct read facts and a thunked construct path.

> r[descriptors.encode-decode-asymmetry]
>
> A descriptor node may provide direct facts for one direction and a thunk for
> the other. Encoding — reading an existing value — is commonly direct.
> Decoding — constructing a value — commonly needs a thunk for allocation,
> insertion, or controlled initialization. The engine uses whatever the node
> provides for the direction it is running.

The model, in the same Rust notation the type system uses. These are not wire
types: no `SchemaId`, never serialized, process-local only.

```rust
pub struct Descriptor {
    pub schema: SchemaRef,   // the schema this realizes
    pub layout: Layout,      // process-local size and alignment
    pub access: Access,      // how to read and construct it
}

pub struct Layout {
    pub size: usize,
    pub align: usize,
}

pub enum Access {
    /// Fixed-width scalar whose in-memory bytes equal its wire bytes: bool,
    /// the integer and float primitives, char. Copy `layout.size` bytes either
    /// direction. (Assumes the host matches wire endianness, which every phon
    /// target does; a host that didn't would thunk or byteswap instead.)
    Scalar,

    /// A struct or tuple: fields at fixed offsets.
    Record(RecordAccess),

    /// A sum type: an active variant chosen by a tag, a payload per variant.
    Enum(EnumAccess),

    /// none / some.
    Option(OptionAccess),

    /// A fixed-shape array: `count` elements inline (the product of the
    /// schema's dimensions), `stride` apart, no allocation, direct both ways.
    Array { element: Box<Descriptor>, count: usize, stride: usize },

    /// A runtime-shape tensor (ndarray and friends).
    Tensor(TensorAccess),

    /// A dynamic homogeneous sequence (list, set) or byte sequence
    /// (string, bytes).
    Sequence(SequenceAccess),

    /// Key / value pairs.
    Map(MapAccess),

    /// A `Dynamic` value: no layout to describe. The engine decodes/encodes a
    /// `Value` through the self-describing codec and hands it over as-is.
    Dynamic,

    /// The whole subtree is handled by thunks: no direct facts apply. This is
    /// how `Channel` and `External` are accessed — the binding turns a local
    /// endpoint or external buffer into a handle on encode and back on decode —
    /// and the fallback for any kind a producer can't reduce to layout facts.
    Opaque { encode: Thunk, decode: Thunk },
}

pub struct RecordAccess {
    pub fields: Vec<FieldAccess>,
    pub construct: Construct,
}

pub struct FieldAccess {
    pub offset: usize,
    pub descriptor: Descriptor,
}

pub enum Construct {
    /// Decode writes each field into its offset in uninitialized storage; the
    /// value is valid once all fields are written. Plain structs and tuples.
    InPlace,
    /// Decode fills a scratch buffer, then a thunk builds the real value from
    /// it. Types with construction invariants, languages that can't be poked
    /// field by field.
    Thunk(Thunk),
}

pub struct EnumAccess {
    pub tag: Tag,
    pub variants: Vec<VariantAccess>,
}

pub enum Tag {
    /// An integer discriminant `width` bytes wide at `offset`. The value read
    /// there matches one variant's `selector`.
    Direct { offset: usize, width: usize },
    /// A niche: the discriminating region overlaps the payload (Option<&T> is
    /// null, niche-optimized enums). Read like `Direct`, but writing it only
    /// applies to variants that don't otherwise occupy the region.
    Niche { offset: usize, width: usize },
    /// The implementation determines and sets the active variant via thunks.
    Thunk { read: Thunk, write: Thunk },
}

pub struct VariantAccess {
    pub index: u32,                 // schema variant index
    pub selector: u64,              // tag value that identifies this variant in memory
    pub payload: RecordAccess,      // payload fields at offsets, with their own construct
}

pub struct OptionAccess {
    pub presence: Presence,
    pub some: Box<Descriptor>,
}

pub enum Presence {
    /// A dedicated tag region; `none_value` distinguishes none from some.
    Tag { offset: usize, width: usize, none_value: u64 },
    /// The some-payload's own bytes encode none at a pattern (null pointer,
    /// zero of a non-zero type).
    Niche { offset: usize, width: usize, none_pattern: Vec<u8> },
    /// Backend presence and construction.
    Thunk { is_some: Thunk, set_none: Thunk, set_some: Thunk },
}

pub struct SequenceAccess {
    pub element: Box<Descriptor>,
    pub storage: SequenceStorage,
}

pub enum SequenceStorage {
    /// Owned contiguous run: (ptr, len, capacity) at offsets, elements
    /// `element.layout` stride apart. Encode reads ptr+len and walks. Decode
    /// calls `allocate`, writes the elements, then writes the triple.
    /// `Vec<T>`, `String`.
    Owned {
        ptr_offset: usize,
        len_offset: usize,
        cap_offset: Option<usize>,   // None for owned-without-capacity, e.g. Box<[T]>
        allocate: Thunk,
    },
    /// Borrowed contiguous run: (ptr, len) at offsets, no capacity, no
    /// allocation. Decode points `ptr` into the input (or a decode-scoped
    /// arena) and writes `len`. Encode reads ptr+len and walks, same as Owned.
    /// `&str`, `&[u8]`, `&[T]` for scalar `T`.
    Borrowed {
        ptr_offset: usize,
        len_offset: usize,
    },
    /// Non-flat storage: length and per-element access go through thunks
    /// (linked lists, copy-on-write buffers, anything not a contiguous run).
    Thunk { len: Thunk, get: Thunk, push: Thunk },
}

pub struct MapAccess {
    pub key: Box<Descriptor>,
    pub value: Box<Descriptor>,
    pub len: Thunk,         // encode: entry count
    pub iterate: Thunk,     // encode: yield (key, value) pairs
    pub insert: Thunk,      // decode: insert a decoded pair
}

pub struct TensorAccess {
    pub element: Box<Descriptor>,
    pub shape: Thunk,           // encode: read the dimension sizes
    pub data: SequenceStorage,  // the flat row-major elements; Borrowed when contiguous
    pub reshape: Thunk,         // decode: give the filled flat data its shape
}

pub struct Thunk {
    pub name: String,   // resolved to a function pointer by the binding
}
```

> r[descriptors.thunk-binding]
>
> A thunk names a function; it does not carry one. Before building an encoder or
> decoder, the caller supplies a binding from thunk names to process-local
> function pointers. The engine resolves names through that binding. A thunk
> with no binding is a build-time error — there is no implicit fallback, no
> default behavior an unbound name silently falls into.

> r[descriptors.facts-are-optional]
>
> Direct-fact variants — niche tags, direct offsets, `Contiguous` storage — are
> producer-optional. A producer emits them when it can prove the layout and
> falls back to a thunk otherwise; an engine must accept a descriptor that uses
> only thunks for any given subtree. The engine never requires a particular fact
> to be present. (Niche packing, for instance, is a Rust/Swift compiler concern
> with no TypeScript analog — a producer that has no niche to expose simply uses
> a `Tag` or a thunk.)

> r[descriptors.borrowed]
>
> A `Borrowed` sequence decodes without allocating: the engine points its
> pointer into the input buffer and writes its length. The decoded value is
> valid only as long as that input buffer — and any decode-scoped arena (below)
> — lives; the caller must keep it alive for the value's lifetime. Because phon
> decodes a whole message from one buffer, the run is always contiguous, so
> borrowing in place needs two things: the run is aligned (for primitive-array
> elements) per `r[compact.alignment]`, and the host's byte order matches the
> wire's. The wire is little-endian and every phon target is little-endian, so
> on a target host this always holds; a hypothetical big-endian host would
> byteswap multi-byte elements on the copy path and could not borrow them. When
> a precondition fails — a misaligned array, or (off-target) a byte-order
> mismatch — the engine copies the run into a decode-scoped arena that shares
> the value's lifetime, still without allocating a per-value owned container.
> Borrowing applies only where the wire bytes equal the memory bytes: scalars,
> `bytes`, `string`, arrays of those, and the element data of a contiguous
> (standard-layout) tensor. A `&[T]` of non-scalar `T` cannot borrow, because
> the wire and memory layouts differ; it is `Owned` or `Thunk` instead, and a
> strided or non-contiguous tensor view falls back the same way.

Borrowed storage is the heart of zero-copy decoding. A wire format that always
allocated would make the alignment work in `r[compact.alignment]` pointless;
`Borrowed` is what turns an aligned, contiguous run on the wire into a slice the
caller reads in place. An implementation built for throughput — a virtual
filesystem moving file contents, an RPC layer relaying large responses — leans
on it heavily, and the model has to express it as a first-class storage mode,
not as an afterthought to allocation.

This is exactly where the execution model's "direct codegen or helper call"
split comes from. A node with direct facts is one the JIT lowers to inline
memory operations. A node with a thunk is one the JIT lowers to a call. Strict
mode (`r[exec.strict-recording]`) records the thunked nodes, because those are
the ones a sharper producer — exposing a layout fact it currently hides — or a
sharper engine could someday turn into direct facts.

## Rust

A Rust implementation produces descriptors (per [The descriptor
model](#the-descriptor-model)) from [facet](https://crates.io/crates/facet)
metadata — field offsets, enum discriminants, niche optimizations — produced at
compile time by `#[derive(Facet)]`. Most Rust descriptors are direct: structs
become `Record { InPlace }` with field offsets, fixed primitives become
`Scalar`, `Vec<T>`/`String` become `Sequence { Owned }` reading the
`(ptr, len, cap)` triple directly on encode and allocating via a thunk on
decode, borrowed `&str`/`&[u8]`/`&[T]` become `Sequence { Borrowed }` decoded as
pointers into the input rather than allocations — the zero-copy path a hot Rust
service leans on — and niche-optimized `Option`s become `Option { Niche }`. The
thunked paths appear where Rust construction needs the allocator or a
container's own insert — `Vec` allocation, `HashMap`/`HashSet` insertion,
`BTreeMap`.

Execution: the interpreter walks the translation plan against descriptor-typed
memory. The JIT, behind a Cargo feature, compiles the plan to machine code via
copy-and-patch — ops written as small Rust functions, lowered through
rustc/LLVM at build time, with their machine code and relocations extracted,
then stamped out and patched at runtime.

## Swift

A Swift implementation produces descriptors of the same shape, sourced
differently: by probing the Swift runtime — reflection over stored properties,
enum-case layout, the runtime's own type metadata — and validating the facts
against live values before trusting them. Where Rust reads a struct's field
offsets from facet, Swift reads them from runtime metadata; the resulting
`Record` has the same shape, and Swift's engine consumes it the way Rust's
engine consumes Rust's. Swift leans on thunks more than Rust does — copy-on-
write `Array`, `String`'s inline-or-heap representation, and existentials are
opaque enough to go through `Sequence { Thunk }` or `Opaque` — but each is the
same `Access` shape, handled by Swift's own engine. The design transfers; the
code doesn't.

Execution: interpreter baseline; copy-and-patch JIT through swiftc/LLVM — the
same technique as Rust, on the same compiler substrate, so implementation
experience transfers directly between the two. The JIT runs where the platform
permits allocating executable memory (macOS does); the interpreter alone covers
platforms that don't.

## TypeScript

A TypeScript implementation has no manual memory layout — values are
garbage-collected objects — so it has no descriptor in the Rust/Swift sense.
"Layout" is just property access on objects.

Execution: interpreter baseline; the JIT is generated JavaScript source handed
to `new Function()`. Same engine model, language-native realization — where
Rust and Swift emit machine code, TypeScript emits a specialized JavaScript
function and lets the host JS engine compile it.
