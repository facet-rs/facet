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

public extension MemoryLayout {
    /// The phon `Layout` (size + alignment) for `T` — a convenience for codegen.
    static var phonLayout: Layout { Layout(size: size, align: alignment) }
}

/// How a value's bytes are read and constructed.
public indirect enum Access {
    /// A fixed-width scalar whose in-memory bytes equal its wire bytes (bool, the
    /// integer and float primitives, char): copy `layout.size` bytes either
    /// direction. Assumes host byte order matches the wire's (little-endian).
    case scalar
    /// A struct or tuple: fields at fixed offsets.
    case record(RecordAccess)
    /// An optional value: presence/construction via witnesses, plus the
    /// some-payload descriptor.
    case option(OptionAccess)
    /// A `Dynamic` value: no layout to describe. The in-memory field IS a
    /// `PhonSchema.Value`; the engine reads/writes it through the self-describing
    /// codec at the field offset.
    case dynamic
    /// A bulk contiguous run of `stride`-byte elements: `String`, `[UInt8]`, or a
    /// `[scalar]` whose element copies as one block (`u32` count then
    /// `count * stride` contiguous bytes). The wire form is identical to a
    /// `list`/`set` of that scalar; the witnesses own the concrete handle.
    case bytes(BytesAccess)
    /// A sum type: a tag selecting the active variant, per-variant payloads, and
    /// the witnesses that read the active variant and project/inject its payload.
    case enumeration(EnumAccess)
    /// A dynamic homogeneous sequence (`list`/`set`) of *structured* elements
    /// (`[Struct]`): a `u32` count then each element by its own program. Scalar
    /// element sequences use the bulk `bytes` path instead.
    case sequence(SequenceAccess)
    /// An owned map (`[K: V]`): a `u32` entry count then, per entry, the key by
    /// its program then the value by its program. Entries are emitted in a
    /// deterministic (sorted-key) order so the bytes are stable.
    case map(MapAccess)
}

/// An owned map: key and value descriptors, their scratch layouts, and the
/// witnesses that enumerate/build the concrete `[K: V]`.
public struct MapAccess {
    public var key: Descriptor
    public var value: Descriptor
    public var keyStride: Int
    public var keyAlign: Int
    public var valueStride: Int
    public var valueAlign: Int
    public var witness: MapWitness

    public init(
        key: Descriptor, value: Descriptor,
        keyStride: Int, keyAlign: Int, valueStride: Int, valueAlign: Int,
        witness: MapWitness
    ) {
        self.key = key
        self.value = value
        self.keyStride = keyStride
        self.keyAlign = keyAlign
        self.valueStride = valueStride
        self.valueAlign = valueAlign
        self.witness = witness
    }
}

/// Witnesses over an owned map handle. Encode copies entries (in sorted-key
/// order) into engine key/value buffers as proper retained copies — so they
/// outlive the witness call — and `destroyEntries` deinitializes them after the
/// engine has read them. Decode inits the map then inserts each decoded entry,
/// moving key+value out of scratch; a repeated key collapses two entries into one
/// and is rejected via the final count.
public struct MapWitness {
    /// The entry count.
    public var count: (_ handle: UnsafeRawPointer) -> Int
    /// Copy the `count` entries into `keys`/`values` (each its stride apart), in a
    /// deterministic sorted-key order, as retained copies.
    public var projectEntries: (_ handle: UnsafeRawPointer, _ keys: UnsafeMutableRawPointer, _ values: UnsafeMutableRawPointer) -> Void
    /// Deinitialize the `count` projected entries (a no-op for trivial K/V).
    public var destroyEntries: (_ keys: UnsafeMutableRawPointer, _ values: UnsafeMutableRawPointer, _ count: Int) -> Void
    /// Initialize the uninitialized map at `handle` with room for `capacity`.
    public var initWithCapacity: (_ handle: UnsafeMutableRawPointer, _ capacity: Int) -> Void
    /// Insert one entry, moving key and value out of the engine scratch.
    public var insert: (_ handle: UnsafeMutableRawPointer, _ key: UnsafeMutableRawPointer, _ value: UnsafeMutableRawPointer) -> Void

    public init(
        count: @escaping (UnsafeRawPointer) -> Int,
        projectEntries: @escaping (UnsafeRawPointer, UnsafeMutableRawPointer, UnsafeMutableRawPointer) -> Void,
        destroyEntries: @escaping (UnsafeMutableRawPointer, UnsafeMutableRawPointer, Int) -> Void,
        initWithCapacity: @escaping (UnsafeMutableRawPointer, Int) -> Void,
        insert: @escaping (UnsafeMutableRawPointer, UnsafeMutableRawPointer, UnsafeMutableRawPointer) -> Void
    ) {
        self.count = count
        self.projectEntries = projectEntries
        self.destroyEntries = destroyEntries
        self.initWithCapacity = initWithCapacity
        self.insert = insert
    }
}

/// A homogeneous sequence of structured elements. The witnesses read the count
/// and the contiguous element storage (encode) and build the handle (decode);
/// the engine runs `element`'s program once per element slot.
///
/// First cut: trivially-copyable elements (the engine bitwise-copies element
/// storage to/from its scratch buffer). Elements with managed payloads
/// (`[String]`, `[[T]]`) come later.
public struct SequenceAccess {
    public var element: Descriptor
    /// Bytes between consecutive elements in contiguous storage (the element's
    /// stride).
    public var stride: Int
    /// Alignment of the element buffer.
    public var elemAlign: Int
    public var witness: SeqWitness

    public init(element: Descriptor, stride: Int, elemAlign: Int, witness: SeqWitness) {
        self.element = element
        self.stride = stride
        self.elemAlign = elemAlign
        self.witness = witness
    }
}

/// Witnesses over an owned sequence handle (`[T]`).
public struct SeqWitness {
    /// The element count.
    public var count: (_ handle: UnsafeRawPointer) -> Int
    /// Copy (bitwise-borrow) the `count * stride` contiguous element bytes into
    /// `dst` for reading. A non-trivial element (`String`) is copied without a
    /// retain — the handle still owns it — and the engine only reads `dst`.
    public var copyElements: (_ handle: UnsafeRawPointer, _ dst: UnsafeMutableRawPointer) -> Void
    /// Build the handle at `handle` by **moving** the `count` elements out of
    /// `src` (the engine's scratch, which it then frees without deinitializing).
    /// `moveInitialize` is correct for both trivial and managed elements.
    public var construct: (_ handle: UnsafeMutableRawPointer, _ src: UnsafeMutableRawPointer, _ count: Int) -> Void

    public init(
        count: @escaping (UnsafeRawPointer) -> Int,
        copyElements: @escaping (UnsafeRawPointer, UnsafeMutableRawPointer) -> Void,
        construct: @escaping (UnsafeMutableRawPointer, UnsafeMutableRawPointer, Int) -> Void
    ) {
        self.count = count
        self.copyElements = copyElements
        self.construct = construct
    }
}

/// A sum type whose in-memory layout the engine never assumes — it reads the
/// active variant and projects/injects payloads through witnesses (Swift's
/// generated `tag`/`project`/`inject`).
///
/// The `variants` array order defines the *local index*: `tag` returns the active
/// variant's position in this array, and `projectPayload`/`inject` take that same
/// local index. Each variant's payload is treated as a record laid out in an
/// engine-owned scratch buffer (`payloadLayout`); `payloadFields` give the field
/// offsets within it.
public struct EnumAccess {
    /// The active variant's local index (its position in `variants`).
    public var tag: (_ value: UnsafeRawPointer) -> Int
    /// Copy variant `localIndex`'s payload from `value` into `scratch`
    /// (`payloadLayout`-shaped), as retained copies. A no-op for a unit variant.
    public var projectPayload: (_ value: UnsafeRawPointer, _ localIndex: Int, _ scratch: UnsafeMutableRawPointer) -> Void
    /// Deinitialize the payload `projectPayload` wrote into `scratch` (after the
    /// engine has encoded it). A no-op for unit / trivial payloads.
    public var destroyPayload: (_ scratch: UnsafeMutableRawPointer, _ localIndex: Int) -> Void
    /// Construct the enum at `slot` for variant `localIndex`, **moving** the
    /// payload the engine decoded into `scratch`.
    public var inject: (_ slot: UnsafeMutableRawPointer, _ localIndex: Int, _ scratch: UnsafeMutableRawPointer) -> Void
    public var variants: [VariantAccess]

    public init(
        tag: @escaping (UnsafeRawPointer) -> Int,
        projectPayload: @escaping (UnsafeRawPointer, Int, UnsafeMutableRawPointer) -> Void,
        destroyPayload: @escaping (UnsafeMutableRawPointer, Int) -> Void = { _, _ in },
        inject: @escaping (UnsafeMutableRawPointer, Int, UnsafeMutableRawPointer) -> Void,
        variants: [VariantAccess]
    ) {
        self.tag = tag
        self.projectPayload = projectPayload
        self.destroyPayload = destroyPayload
        self.inject = inject
        self.variants = variants
    }
}

/// One variant: its schema wire index, its payload fields (at offsets within the
/// variant's scratch buffer; empty for a unit variant), and the scratch layout.
public struct VariantAccess {
    public var wireIndex: UInt32
    public var payloadFields: [FieldAccess]
    public var payloadLayout: Layout

    public init(wireIndex: UInt32, payloadFields: [FieldAccess], payloadLayout: Layout) {
        self.wireIndex = wireIndex
        self.payloadFields = payloadFields
        self.payloadLayout = payloadLayout
    }
}

/// A bulk byte/scalar run: its element stride/alignment and the witnesses that
/// read and build the concrete handle (`String`/`[UInt8]`/`[scalar]`).
public struct BytesAccess {
    /// Bytes per element: 1 for `String`/`[UInt8]`, the element size otherwise.
    public var stride: Int
    /// Alignment of the contiguous element buffer (pads before the run on the
    /// wire when non-empty).
    public var elemAlign: Int
    public var witness: BytesWitness

    public init(stride: Int, elemAlign: Int, witness: BytesWitness) {
        self.stride = stride
        self.elemAlign = elemAlign
        self.witness = witness
    }
}

/// Witnesses over a contiguous bulk handle. Encode copies the run into an
/// engine-owned scratch buffer (the inout-sink rule again); decode builds the
/// handle from the wire bytes, rejecting invalid content (non-UTF-8 for
/// `String`).
public struct BytesWitness {
    /// The element count (its UTF-8 byte length for `String`).
    public var count: (_ field: UnsafeRawPointer) -> Int
    /// Copy the `count * stride` contiguous element bytes into `dst`.
    public var copyInto: (_ field: UnsafeRawPointer, _ dst: UnsafeMutableRawPointer) -> Void
    /// Build the handle at `field` from `count` elements at `src`; return `false`
    /// on invalid content. `src` is valid for `count * stride` bytes (a dummy when
    /// `count == 0`).
    public var construct: (_ field: UnsafeMutableRawPointer, _ src: UnsafeRawPointer, _ count: Int) -> Bool

    public init(
        count: @escaping (UnsafeRawPointer) -> Int,
        copyInto: @escaping (UnsafeRawPointer, UnsafeMutableRawPointer) -> Void,
        construct: @escaping (UnsafeMutableRawPointer, UnsafeRawPointer, Int) -> Bool
    ) {
        self.count = count
        self.copyInto = copyInto
        self.construct = construct
    }
}

/// An optional value: how presence is read/written (witnesses), and the
/// some-payload descriptor.
public struct OptionAccess {
    public var witness: OptionWitness
    public var some: Descriptor

    public init(witness: OptionWitness, some: Descriptor) {
        self.witness = witness
        self.some = some
    }
}

/// Witnesses over an `Optional<T>` whose niche/tag layout the engine never
/// assumes.
///
/// Encode projects the inner value (when present) into an engine-owned scratch
/// buffer the engine then reads — a borrow-read, so a non-trivial inner (a
/// `String`, a `Vec`) is bitwise-copied without ownership transfer. (A closure
/// can't capture the `inout` byte sink, so the engine can't be called back
/// mid-projection; the scratch hop sidesteps that.) Decode builds none/some in
/// place.
public struct OptionWitness {
    /// Copy the inner value into `scratch` (the inner's layout) if present;
    /// return whether it was present.
    public var projectSome: (_ option: UnsafeRawPointer, _ scratch: UnsafeMutableRawPointer) -> Bool
    /// Initialize the uninitialized option at `option` to `Some`, **moving** the
    /// inner value the engine decoded into `value` (engine scratch).
    public var initSome: (_ option: UnsafeMutableRawPointer, _ value: UnsafeMutableRawPointer) -> Void
    /// Initialize the uninitialized option at `option` to `None`.
    public var initNone: (_ option: UnsafeMutableRawPointer) -> Void

    public init(
        projectSome: @escaping (UnsafeRawPointer, UnsafeMutableRawPointer) -> Bool,
        initSome: @escaping (UnsafeMutableRawPointer, UnsafeMutableRawPointer) -> Void,
        initNone: @escaping (UnsafeMutableRawPointer) -> Void
    ) {
        self.projectSome = projectSome
        self.initSome = initSome
        self.initNone = initNone
    }
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
