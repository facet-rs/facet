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
//! The generic memory-descriptor vocabulary lives in `weavy`; PHON specializes it
//! with `phon_schema::SchemaRef` so existing callers keep the same PHON-facing
//! names while other bytecode consumers can reuse the same model.
//!
//! Spec: "The descriptor model" (`r[descriptors.*]`).

use phon_schema::SchemaRef;

pub use weavy::mem::{
    ByteOwner, ByteRange, Construct, FieldDefault, Layout, MapStorage, Presence,
    RecordByteOwnership, SequenceStorage, SetStorage, Tag, Thunk,
};

/// A node of the PHON descriptor tree.
// r[impl descriptors.separate-implementations]
// r[impl descriptors.fact-driven]
pub type Descriptor = weavy::mem::Descriptor<SchemaRef>;

/// How a PHON value's bytes are read and constructed.
// r[impl descriptors.encode-decode-asymmetry]
pub type Access = weavy::mem::Access<SchemaRef>;

/// A PHON struct or tuple: its fields at offsets, with how to construct it.
pub type RecordAccess = weavy::mem::RecordAccess<SchemaRef>;

/// One PHON field: its byte offset within the record, and its descriptor.
pub type FieldAccess = weavy::mem::FieldAccess<SchemaRef>;

/// A PHON sum type: a tag selecting the active variant, and the per-variant payloads.
pub type EnumAccess = weavy::mem::EnumAccess<SchemaRef>;

/// One PHON variant: its schema index, local tag selector, and payload fields.
pub type VariantAccess = weavy::mem::VariantAccess<SchemaRef>;

/// An optional PHON value: how presence is read/written, and the some-payload.
pub type OptionAccess = weavy::mem::OptionAccess<SchemaRef>;

/// A dynamic homogeneous PHON sequence or byte sequence.
pub type SequenceAccess = weavy::mem::SequenceAccess<SchemaRef>;

/// A PHON set: its element descriptor and storage strategy.
pub type SetAccess = weavy::mem::SetAccess<SchemaRef>;

/// A PHON result: the `Ok` and `Err` payload descriptors plus thunks.
pub type ResultAccess = weavy::mem::ResultAccess<SchemaRef>;

/// A PHON owning pointer: its pointee descriptor and thunks.
pub type PointerAccess = weavy::mem::PointerAccess<SchemaRef>;

/// PHON key/value pairs: key and value descriptors plus map storage.
pub type MapAccess = weavy::mem::MapAccess<SchemaRef>;

/// A PHON runtime-shape tensor.
pub type TensorAccess = weavy::mem::TensorAccess<SchemaRef>;
