// The typed (memory) side of the IR: a `MemProgram` moves bytes between the wire
// and a value's in-memory layout, at offsets the descriptor supplies. Where
// `Program`/`Op` builds a dynamic `Value` on a stack, a `MemProgram` reads and
// writes the value's memory directly.
//
// Mirrors the `MemOp`/`MemProgram` half of `rust/phon-ir/src/ir.rs`. This first
// cut models fixed scalars (and, via folded offsets, in-place records — a whole
// nested fixed struct dissolves into a flat run of `scalar` copies). Owned
// sequences, options, enums, and maps grow this enum as the engine learns them.

import PhonSchema

/// A lowered typed program: base-relative memory copies, in wire order.
public typealias MemProgram = [MemOp]

/// A lowered typed program: the root op stream plus the per-schema block programs
/// that `MemOp.callBlock` calls into. For a non-recursive type `blocks` is empty
/// and `program` is the familiar flat op stream; a recursive type lowers each of
/// its cyclic schemas to a block here, so `program` (and every block) stays finite.
/// Mirrors `rust/phon-ir/src/ir.rs::Lowered`.
public struct Lowered {
    public var program: MemProgram
    public var blocks: [SchemaId: MemProgram]

    public init(program: MemProgram, blocks: [SchemaId: MemProgram] = [:]) {
        self.program = program
        self.blocks = blocks
    }
}

/// One typed step. The base pointer is supplied at run time; `offset` is relative
/// to it.
// r[impl ir.one-vocabulary]
// r[impl ir.memory]
public indirect enum MemOp {
    /// Copy a run of `size` bytes between memory at `offset` and the wire, which
    /// is first padded to `align`. A single scalar, or a fused run of adjacent
    /// scalars. Encode reads memory and writes the wire; decode reads the wire and
    /// writes memory. Sound only where host byte order equals the wire's
    /// (little-endian), which every phon target is.
    case scalar(offset: Int, size: Int, align: Int)
    /// An `Option<T>` at `offset`: a `u8` presence byte then, only when present,
    /// the inner `T` by its own program. Presence and construction go through the
    /// witnesses; the engine never assumes the niche/tag layout.
    case option(OptionOp)
    /// A self-describing `Value` at `offset` (the in-memory field is a concrete
    /// `PhonSchema.Value`): encoded/decoded by the self-describing codec,
    /// self-delimiting on the wire (no length prefix).
    case dynamic(offset: Int)
    /// A bulk byte/scalar run at `offset`: a `u32` element count, padding to
    /// `elemAlign` when non-empty, then `count * stride` contiguous bytes — one
    /// block copy in each direction.
    case bytes(BytesOp)
    /// A sum type at `offset`: a `u32` wire variant index then the active
    /// variant's payload. Encode reads the active variant via `tag`, projects its
    /// payload into scratch, and encodes it; decode reads the index, decodes the
    /// payload into scratch, and injects the variant.
    case enumeration(EnumOp)
    /// An owned sequence of structured elements at `offset`: a `u32` count then
    /// each element by its own program.
    case sequence(SeqOp)
    /// An owned map at `offset`: a `u32` entry count then per-entry key+value
    /// programs. Encode emits entries in sorted-key order; decode rejects a
    /// repeated key.
    case map(MapOp)
    /// A writer-only value present on the wire but absent from the reader: consume
    /// its wire bytes (by the writer's skip skeleton) and write nothing to memory.
    /// Decode-only (`r[compat.skip-writer-only]`).
    case skipWire(SkipOp)
    /// A reader-only field absent from the writer: write its default into memory at
    /// `offset` with no wire read. Decode-only (`r[compat.reader-only-fields]`).
    case writeDefault(DefaultOp)
    /// A call into a recursive schema's block program, run at `base + offset` (the
    /// recursive value sits at `offset` from the enclosing base — a struct field, or
    /// `0` for a sequence element / option payload / map value reached at its own
    /// base). This is how a recursive type stays finite: the cyclic schema is lowered
    /// once into a block (resolved from `Lowered.blocks` by `schema`, with offsets
    /// relative to the recursive value's start), and every reference to it is a
    /// `callBlock` rather than an inlined subtree. Encode and decode both recurse into
    /// the block. (`r[ir.recursion]`)
    case callBlock(schema: SchemaId, offset: Int)
}

/// A reader-only default op: initialize the reader field at `offset` to its
/// default in place, reading no wire bytes.
public struct DefaultOp {
    public var offset: Int
    public var initFn: (_ slot: UnsafeMutableRawPointer) -> Void

    public init(offset: Int, initFn: @escaping (UnsafeMutableRawPointer) -> Void) {
        self.offset = offset
        self.initFn = initFn
    }
}

/// A pre-built skeleton of a writer value, used to advance the reader past a
/// writer-only field without touching reader memory (`r[compat.skip-writer-only]`).
/// Built once at lowering from the writer schema. Mirrors `phon-ir/ir.rs::SkipOp`.
public indirect enum SkipOp {
    /// A fixed scalar: pad to `align`, advance `size` bytes.
    case scalar(size: Int, align: Int)
    /// A bulk byte run: a `u32` count, pad to `elemAlign`, advance `count * stride`.
    case bytes(stride: Int, elemAlign: Int)
    /// An owned sequence of structured elements: a `u32` count, then skip the
    /// element that many times.
    case seq(SkipOp)
    /// An `Option<T>`: a `u8` presence byte; on `1` skip the inner.
    case option(SkipOp)
    /// An enum: a `u32` writer variant index, then skip that variant's field skips.
    case enumeration([(wireIndex: UInt32, fields: [SkipOp])])
    /// An owned map: a `u32` entry count, then skip key then value each time.
    case map(SkipOp, SkipOp)
    /// A struct/tuple: skip each field in wire order.
    case structure([SkipOp])
    /// A self-describing dynamic value: decode one value and discard it.
    case dynamic
}

/// An owned-map op's payload (in `MemOp.map`).
public struct MapOp {
    public var offset: Int
    public var key: MemProgram
    public var value: MemProgram
    public var keyStride: Int
    public var keyAlign: Int
    public var valueStride: Int
    public var valueAlign: Int
    public var witness: MapWitness

    public init(
        offset: Int, key: MemProgram, value: MemProgram,
        keyStride: Int, keyAlign: Int, valueStride: Int, valueAlign: Int,
        witness: MapWitness
    ) {
        self.offset = offset
        self.key = key
        self.value = value
        self.keyStride = keyStride
        self.keyAlign = keyAlign
        self.valueStride = valueStride
        self.valueAlign = valueAlign
        self.witness = witness
    }
}

/// An owned-sequence op's payload (in `MemOp.sequence`).
public struct SeqOp {
    public var offset: Int
    public var element: MemProgram
    public var stride: Int
    public var elemAlign: Int
    /// Minimum wire bytes one element occupies (`0` for a zero-sized element).
    public var minWire: Int
    /// Whether decode must reject duplicate elements after constructing the
    /// sequence handle. Set schemas use this; list schemas do not.
    public var unique: Bool
    public var witness: SeqWitness

    public init(
        offset: Int,
        element: MemProgram,
        stride: Int,
        elemAlign: Int,
        minWire: Int,
        unique: Bool = false,
        witness: SeqWitness
    ) {
        self.offset = offset
        self.element = element
        self.stride = stride
        self.elemAlign = elemAlign
        self.minWire = minWire
        self.unique = unique
        self.witness = witness
    }
}

/// A sum-type op's payload (in `MemOp.enumeration`).
public struct EnumOp {
    public var offset: Int
    public var tag: (_ value: UnsafeRawPointer) -> Int
    public var projectPayload: (_ value: UnsafeRawPointer, _ localIndex: Int, _ scratch: UnsafeMutableRawPointer) -> Void
    public var destroyPayload: (_ scratch: UnsafeMutableRawPointer, _ localIndex: Int) -> Void
    public var inject: (_ slot: UnsafeMutableRawPointer, _ localIndex: Int, _ scratch: UnsafeMutableRawPointer) -> Void
    public var variants: [EnumVariantOp]
    /// Writer variant indices the reader lacks: receiving one is a
    /// writer-only-variant decode error (empty for a single-schema lower).
    public var writerOnly: [UInt32]

    public init(
        offset: Int,
        tag: @escaping (UnsafeRawPointer) -> Int,
        projectPayload: @escaping (UnsafeRawPointer, Int, UnsafeMutableRawPointer) -> Void,
        destroyPayload: @escaping (UnsafeMutableRawPointer, Int) -> Void,
        inject: @escaping (UnsafeMutableRawPointer, Int, UnsafeMutableRawPointer) -> Void,
        variants: [EnumVariantOp],
        writerOnly: [UInt32] = []
    ) {
        self.offset = offset
        self.tag = tag
        self.projectPayload = projectPayload
        self.destroyPayload = destroyPayload
        self.inject = inject
        self.variants = variants
        self.writerOnly = writerOnly
    }
}

/// One enum variant in a `MemOp.enumeration`: the `u32` index read from the wire
/// (the *writer's* index), the reader's local variant index used to inject, the
/// payload program, and the scratch layout. On a single-schema lower the wire and
/// reader indices coincide.
public struct EnumVariantOp {
    public var wireIndex: UInt32
    public var readerLocalIndex: Int
    public var payload: MemProgram
    public var payloadSize: Int
    public var payloadAlign: Int

    public init(wireIndex: UInt32, readerLocalIndex: Int, payload: MemProgram, payloadSize: Int, payloadAlign: Int) {
        self.wireIndex = wireIndex
        self.readerLocalIndex = readerLocalIndex
        self.payload = payload
        self.payloadSize = payloadSize
        self.payloadAlign = payloadAlign
    }
}

/// A bulk byte-run op's payload (in `MemOp.bytes`).
public struct BytesOp {
    public var offset: Int
    public var stride: Int
    public var elemAlign: Int
    public var witness: BytesWitness

    public init(offset: Int, stride: Int, elemAlign: Int, witness: BytesWitness) {
        self.offset = offset
        self.stride = stride
        self.elemAlign = elemAlign
        self.witness = witness
    }
}

/// An optional op's payload (in `MemOp.option`). `innerSize`/`innerAlign` size
/// the scratch buffer the engine projects into (encode) / decodes into (decode).
public struct OptionOp {
    public var offset: Int
    public var some: MemProgram
    public var innerSize: Int
    public var innerAlign: Int
    public var witness: OptionWitness

    public init(offset: Int, some: MemProgram, innerSize: Int, innerAlign: Int, witness: OptionWitness) {
        self.offset = offset
        self.some = some
        self.innerSize = innerSize
        self.innerAlign = innerAlign
        self.witness = witness
    }
}
