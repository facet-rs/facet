// The descriptor model — how an implementation reads and constructs its own
// language's in-memory values for a given schema.
//
// A descriptor is a tree shaped like the schema, each node annotated with the
// process-local facts the engine needs to read that part of a value (encode) and
// construct it (decode). It is never transmitted, never hashed, never part of
// schema identity. The shape is shared across implementations; each has its own
// descriptors describing its own memory.
//
// Mirrors `rust/phon-ir/src/descriptor.rs`. Where Rust names thunks (resolved to
// function pointers by a separate binding step), Swift carries the closures
// directly — Swift binds at descriptor-build time, so the name indirection is
// unnecessary.
//
// This first cut models what the typed engine handles now: scalars and in-place
// records. Enums, options, sequences, maps, result, dynamic, and opaque grow the
// `Access` enum as the engine learns them.

import PhonSchema

/// A node of the descriptor tree: the schema it realizes, its process-local
/// memory layout, and how to read and construct it.
public struct Descriptor {
    /// The schema this node realizes.
    public var schema: SchemaRef
    /// Process-local size and alignment.
    public var layout: Layout
    /// How to read and construct this value.
    public var access: Access

    public init(schema: SchemaRef, layout: Layout, access: Access) {
        self.schema = schema
        self.layout = layout
        self.access = access
    }
}

/// Process-local size and alignment, in bytes.
public struct Layout: Equatable {
    public var size: Int
    public var align: Int

    public init(size: Int, align: Int) {
        self.size = size
        self.align = align
    }
}

/// How a value's bytes are read and constructed.
public indirect enum Access {
    /// A fixed-width scalar whose in-memory bytes equal its wire bytes (bool, the
    /// integer and float primitives, char): copy `layout.size` bytes either
    /// direction. Assumes host byte order matches the wire's (little-endian).
    case scalar
    /// A struct or tuple: fields at fixed offsets.
    case record(RecordAccess)
}

/// A struct or tuple: its fields at offsets, with how to construct it.
public struct RecordAccess {
    public var fields: [FieldAccess]
    public var construct: Construct

    public init(fields: [FieldAccess], construct: Construct) {
        self.fields = fields
        self.construct = construct
    }
}

/// One field: its byte offset within the record, and its descriptor.
public struct FieldAccess {
    public var offset: Int
    public var descriptor: Descriptor
    /// How to write this field's default in place, for the decode-compat path
    /// when the field is reader-only (absent from the writer). `nil` for a
    /// required field, whose absence from the writer makes the schemas
    /// incompatible.
    public var defaultInit: ((_ slot: UnsafeMutableRawPointer) -> Void)?

    public init(
        offset: Int,
        descriptor: Descriptor,
        defaultInit: ((_ slot: UnsafeMutableRawPointer) -> Void)? = nil
    ) {
        self.offset = offset
        self.descriptor = descriptor
        self.defaultInit = defaultInit
    }
}

/// How a record is built on decode.
public enum Construct {
    /// Decode writes each field into its offset in uninitialized storage; the
    /// value is valid once all fields are written. Plain structs and tuples.
    case inPlace
    /// Decode fills a scratch buffer, then a closure builds the real value from
    /// it. Types with construction invariants, or that can't be poked field by
    /// field.
    case thunk((_ scratch: UnsafeRawPointer, _ slot: UnsafeMutableRawPointer) -> Void)
}
