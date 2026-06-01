// The typed (memory) side of the IR: a `MemProgram` moves bytes between the wire
// and a value's in-memory layout, at offsets the descriptor supplies. Where
// `Program`/`Op` builds a dynamic `Value` on a stack, a `MemProgram` reads and
// writes the value's memory directly.
//
// Mirrors the `MemOp`/`MemProgram` half of `rust/phon-ir/src/ir.rs`. This first
// cut models fixed scalars (and, via folded offsets, in-place records — a whole
// nested fixed struct dissolves into a flat run of `scalar` copies). Owned
// sequences, options, enums, and maps grow this enum as the engine learns them.

/// A lowered typed program: base-relative memory copies, in wire order.
public typealias MemProgram = [MemOp]

/// One typed step. The base pointer is supplied at run time; `offset` is relative
/// to it.
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
    public var witness: SeqWitness

    public init(offset: Int, element: MemProgram, stride: Int, elemAlign: Int, minWire: Int, witness: SeqWitness) {
        self.offset = offset
        self.element = element
        self.stride = stride
        self.elemAlign = elemAlign
        self.minWire = minWire
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

    public init(
        offset: Int,
        tag: @escaping (UnsafeRawPointer) -> Int,
        projectPayload: @escaping (UnsafeRawPointer, Int, UnsafeMutableRawPointer) -> Void,
        destroyPayload: @escaping (UnsafeMutableRawPointer, Int) -> Void,
        inject: @escaping (UnsafeMutableRawPointer, Int, UnsafeMutableRawPointer) -> Void,
        variants: [EnumVariantOp]
    ) {
        self.offset = offset
        self.tag = tag
        self.projectPayload = projectPayload
        self.destroyPayload = destroyPayload
        self.inject = inject
        self.variants = variants
    }
}

/// One enum variant in a `MemOp.enumeration`: its wire index, the payload program
/// (offsets into the variant scratch), and the scratch layout.
public struct EnumVariantOp {
    public var wireIndex: UInt32
    public var payload: MemProgram
    public var payloadSize: Int
    public var payloadAlign: Int

    public init(wireIndex: UInt32, payload: MemProgram, payloadSize: Int, payloadAlign: Int) {
        self.wireIndex = wireIndex
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
