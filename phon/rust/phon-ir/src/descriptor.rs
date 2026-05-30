//! The descriptor model — how an implementation reads and constructs its own
//! language's in-memory values for a given schema.
//!
//! A descriptor is a tree shaped like the schema, each node annotated with the
//! process-local facts the engine needs to read that part of a value (encode)
//! and construct it (decode). It is never transmitted, never hashed, never part
//! of schema identity. The *shape* is shared across implementations and
//! documented once in the spec; each implementation has its own descriptors
//! describing its own memory.
//!
//! Facts come in two forms: **direct facts** (offsets, strides, tags, niches)
//! the engine reads/writes itself, and **thunks** (named same-language functions)
//! for everything direct facts can't express. A node may give direct facts for
//! one direction and a thunk for the other (`r[descriptors.encode-decode-asymmetry]`).
//!
//! Spec: "The descriptor model" (`r[descriptors.*]`).

use phon_schema::SchemaRef;

/// A node of the descriptor tree: the schema it realizes, its process-local
/// memory layout, and how to read and construct it.
#[derive(Clone, Debug)]
pub struct Descriptor {
    /// The schema this node realizes.
    pub schema: SchemaRef,
    /// Process-local size and alignment.
    pub layout: Layout,
    /// How to read and construct this value.
    pub access: Access,
}

/// Process-local size and alignment, in bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    pub size: usize,
    pub align: usize,
}

/// How a value's bytes are read and constructed.
///
/// Direct-fact variants are producer-optional (`r[descriptors.facts-are-optional]`):
/// a producer emits them when it can prove the layout and falls back to a thunk
/// otherwise; an engine must accept a descriptor that uses only thunks.
#[derive(Clone, Debug)]
pub enum Access {
    /// A fixed-width scalar whose in-memory bytes equal its wire bytes (bool, the
    /// integer and float primitives, char): copy `layout.size` bytes either
    /// direction. Assumes host byte order matches the wire's (little-endian),
    /// which every phon target does.
    Scalar,
    /// A struct or tuple: fields at fixed offsets.
    Record(RecordAccess),
    /// A sum type: an active variant chosen by a tag, a payload per variant.
    Enum(EnumAccess),
    /// none / some.
    Option(OptionAccess),
    /// A fixed-shape array: `count` elements inline (the product of the schema's
    /// dimensions), `stride` apart, no allocation, direct both ways.
    Array {
        element: Box<Descriptor>,
        count: usize,
        stride: usize,
    },
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
    /// The whole subtree is handled by thunks: no direct facts apply. How
    /// `Channel` and `External` are accessed (the binding turns a local endpoint
    /// or external buffer into a handle on encode and back on decode), and the
    /// fallback for any kind a producer can't reduce to layout facts.
    Opaque { encode: Thunk, decode: Thunk },
}

/// A struct or tuple: its fields at offsets, with how to construct it.
#[derive(Clone, Debug)]
pub struct RecordAccess {
    pub fields: Vec<FieldAccess>,
    pub construct: Construct,
}

/// One field: its byte offset within the record, and its descriptor.
#[derive(Clone, Debug)]
pub struct FieldAccess {
    pub offset: usize,
    pub descriptor: Descriptor,
}

/// How a record is built on decode.
#[derive(Clone, Debug)]
pub enum Construct {
    /// Decode writes each field into its offset in uninitialized storage; the
    /// value is valid once all fields are written. Plain structs and tuples.
    InPlace,
    /// Decode fills a scratch buffer, then a thunk builds the real value from
    /// it. Types with construction invariants, or languages that can't be poked
    /// field by field.
    Thunk(Thunk),
}

/// A sum type: a tag selecting the active variant, and the per-variant payloads.
#[derive(Clone, Debug)]
pub struct EnumAccess {
    pub tag: Tag,
    pub variants: Vec<VariantAccess>,
}

/// How the active variant is read and set.
#[derive(Clone, Debug)]
pub enum Tag {
    /// An integer discriminant `width` bytes wide at `offset`; the value read
    /// there matches one variant's `selector`.
    Direct { offset: usize, width: usize },
    /// A niche: the discriminating region overlaps the payload (`Option<&T>` is
    /// null, niche-optimized enums). Read like `Direct`, but writing it only
    /// applies to variants that don't otherwise occupy the region.
    Niche { offset: usize, width: usize },
    /// The implementation determines and sets the active variant via thunks.
    Thunk { read: Thunk, write: Thunk },
}

/// One variant: its schema index, the in-memory tag value identifying it, and
/// its payload fields.
#[derive(Clone, Debug)]
pub struct VariantAccess {
    /// The schema variant index.
    pub index: u32,
    /// The tag value that identifies this variant in memory.
    pub selector: u64,
    /// The payload fields at offsets, with their own construction.
    pub payload: RecordAccess,
}

/// An optional value: how presence is read/written, and the some-payload.
#[derive(Clone, Debug)]
pub struct OptionAccess {
    pub presence: Presence,
    pub some: Box<Descriptor>,
}

/// How none-vs-some is encoded in memory.
#[derive(Clone, Debug)]
pub enum Presence {
    /// A dedicated tag region; `none_value` distinguishes none from some.
    Tag {
        offset: usize,
        width: usize,
        none_value: u64,
    },
    /// The some-payload's own bytes encode none at a pattern (null pointer, zero
    /// of a non-zero type).
    Niche {
        offset: usize,
        width: usize,
        none_pattern: Vec<u8>,
    },
    /// Backend presence and construction.
    Thunk {
        is_some: Thunk,
        set_none: Thunk,
        set_some: Thunk,
    },
}

/// A dynamic homogeneous sequence or byte sequence: its element and storage.
#[derive(Clone, Debug)]
pub struct SequenceAccess {
    pub element: Box<Descriptor>,
    pub storage: SequenceStorage,
}

/// How a sequence's elements are stored in memory.
#[derive(Clone, Debug)]
pub enum SequenceStorage {
    /// Owned contiguous run: `(ptr, len, capacity)` at offsets, elements
    /// `element.layout` stride apart. Encode reads ptr+len and walks. Decode
    /// calls `allocate`, writes the elements, then writes the triple.
    /// `Vec<T>`, `String`.
    Owned {
        ptr_offset: usize,
        len_offset: usize,
        /// `None` for owned-without-capacity, e.g. `Box<[T]>`.
        cap_offset: Option<usize>,
        allocate: Thunk,
    },
    /// Borrowed contiguous run: `(ptr, len)` at offsets, no capacity, no
    /// allocation. Decode points `ptr` into the input (or a decode-scoped arena)
    /// and writes `len`. `&str`, `&[u8]`, `&[T]` for scalar `T`.
    Borrowed { ptr_offset: usize, len_offset: usize },
    /// Non-flat storage: length and per-element access go through thunks (linked
    /// lists, copy-on-write buffers, anything not a contiguous run).
    Thunk { len: Thunk, get: Thunk, push: Thunk },
}

/// Key/value pairs accessed through thunks.
#[derive(Clone, Debug)]
pub struct MapAccess {
    pub key: Box<Descriptor>,
    pub value: Box<Descriptor>,
    /// Encode: entry count.
    pub len: Thunk,
    /// Encode: yield `(key, value)` pairs.
    pub iterate: Thunk,
    /// Decode: insert a decoded pair.
    pub insert: Thunk,
}

/// A runtime-shape tensor.
#[derive(Clone, Debug)]
pub struct TensorAccess {
    pub element: Box<Descriptor>,
    /// Encode: read the dimension sizes.
    pub shape: Thunk,
    /// The flat row-major elements; `Borrowed` when contiguous.
    pub data: SequenceStorage,
    /// Decode: give the filled flat data its shape.
    pub reshape: Thunk,
}

/// A named function the implementation provides. A thunk names a function; it
/// does not carry one. Before building an encoder or decoder, the caller supplies
/// a binding from thunk names to process-local function pointers
/// (`r[descriptors.thunk-binding]`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Thunk {
    /// Resolved to a function pointer by the binding.
    pub name: String,
}
