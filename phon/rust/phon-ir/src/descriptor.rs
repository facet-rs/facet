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

use crate::ir::{BorrowThunks, DefaultThunk, MapThunks, OpaqueThunks, OptionThunks, SeqThunks};

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
    /// An opaque field (`#[facet(opaque = ...)]`): no layout to describe, no inner
    /// schema the engine reads. The front-door-bound [`OpaqueThunks`] encode the
    /// inner value (appended after a backpatched `u32` length) and decode it from
    /// the borrowed span; on the wire it is a `Primitive::Bytes` run. This is how
    /// `Channel`/`External` bindings (a local endpoint or external buffer becoming a
    /// handle) and any kind a producer can't reduce to layout facts are carried.
    Opaque(OpaqueThunks),
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
    /// How to write this field's default in place, for the decode-compat path when
    /// the field is reader-only (absent from the writer). `Some` for a field the
    /// language marks defaultable (`#[facet(default)]`); `None` for a required
    /// field, whose absence from the writer makes the schemas incompatible
    /// (`r[compat.reader-only-fields]`). The `ctx` is the front-door-bound context
    /// the thunk understands (passed back untouched).
    pub default: Option<FieldDefault>,
}

/// A field's bound default-in-place operation: a [`DefaultThunk`] plus the opaque
/// `ctx` it is called with. Used only on the decode-compat path for a reader-only
/// field; ignored when the field is present on the wire.
#[derive(Clone, Copy, Debug)]
pub struct FieldDefault {
    /// Opaque per-field context the front door binds (passed to `thunk`).
    pub ctx: *const (),
    /// Initialize the uninitialized field at `slot` to its default.
    pub thunk: DefaultThunk,
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
    /// Front-door-bound presence via the inner type's option vtable — the typed
    /// path's representation, mirroring [`SequenceStorage::Vtable`]. The engine
    /// reads presence and builds none/some through these thunks, never assuming
    /// the in-memory niche/tag layout.
    Vtable(OptionThunks),
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
    /// An owned contiguous sequence reached through the front door's bound list
    /// vtable (`Vec<T>`, `String`), whose `(ptr, len, cap)` layout the engine does
    /// not assume. The typed path's owned-sequence representation
    /// (`r[descriptors.thunk-binding]`).
    Vtable(SeqThunks),
    /// A BORROWED, zero-copy contiguous byte run reached through the front door's
    /// bound borrow thunks (`&str`, `&[u8]`), whose fat-pointer layout the engine
    /// does not assume. Decode writes a fat pointer INTO the reader's input buffer
    /// (no allocation, no copy); the decoded value borrows the input. The typed
    /// path's borrowed-leaf representation, mirroring [`Vtable`](Self::Vtable) but
    /// thunk-built rather than allocated. Distinct from the layout-fact
    /// [`Borrowed`](Self::Borrowed), which carries raw `(ptr, len)` offsets
    /// (`r[descriptors.thunk-binding]`).
    BorrowedVtable(BorrowThunks),
}

/// Key/value pairs: the key and value descriptors and how the map is stored.
#[derive(Clone, Debug)]
pub struct MapAccess {
    pub key: Box<Descriptor>,
    pub value: Box<Descriptor>,
    pub storage: MapStorage,
}

/// How a map's entries are read and constructed in memory.
#[derive(Clone, Debug)]
pub enum MapStorage {
    /// Named same-language thunks: `len` (entry count, encode), `iterate` (yield
    /// `(key, value)` pairs, encode), `insert` (a decoded pair, decode). The
    /// spec'd, binding-resolved representation.
    Thunk {
        len: Thunk,
        iterate: Thunk,
        insert: Thunk,
    },
    /// An owned map reached through the front door's bound map vtable
    /// (`BTreeMap<K, V>`, `HashMap<K, V>`, …), whose in-memory layout the engine
    /// does not assume. The typed path's owned-map representation, mirroring
    /// [`SequenceStorage::Vtable`] (`r[descriptors.thunk-binding]`).
    Vtable(MapThunks),
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
