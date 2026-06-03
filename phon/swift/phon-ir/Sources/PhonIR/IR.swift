// The intermediate representation: a decode plan lowered to a straight,
// pre-sequenced run of `Op`s.
//
// Compatibility planning (in PhonEngine) translates a writer schema with a reader
// schema into a value-shaped tree; lowering flattens that tree into a `Program`.
// Every type-directed decision — which primitive, which field order, which fields
// to skip or default, how enum variants map — is made once, during lowering, and
// frozen into the op sequence. What remains is only data-directed control flow:
// the element count of a sequence, the active variant of an enum, the presence
// bit of an option.
//
// Two consumers run the same `Program`: the interpreter (a stack machine, in
// PhonEngine) and, later, the JIT (copy-and-patch, in PhonJIT).
//
// Mirrors `rust/phon-ir/src/ir.rs` (the decode, dynamic-Value path). The memory
// side (MemOp) and descriptors land with the typed/JIT path.
//
// Invariant: running a complete `Program` against a reader leaves exactly one
// value on the interpreter's stack — the decoded result.

import PhonSchema

/// A lowered decode program: a straight run of `Op`s executed start to finish.
public typealias Program = [Op]

/// One lowered decode step. Each reads from the wire and adjusts the
/// interpreter's value stack; the net stack effect of a complete lowered subtree
/// is always `+1`.
public indirect enum Op {
    /// Decode a primitive from the wire and push its value. Net `+1`.
    case scalar(Primitive)
    /// Decode a self-describing dynamic value and push it. Net `+1`.
    case dynamic
    /// Push a null — a reader-only field's default, or a unit variant payload.
    case null
    /// Decode a value by this writer schema reference and discard it: a
    /// writer-only field the reader does not have. Net `0`.
    case skip(SchemaRef)
    /// Pop `keys.count` values and assemble an object pairing each key with its
    /// value (in `keys` order); push it. Net `+1`.
    case object(keys: [String])
    /// Pop `count` values into an array; push it. Tuples and tuple variant
    /// payloads, whose heterogeneous elements were lowered inline. Net `+1`.
    case array(count: Int)
    /// Read a `u32` length `n`; run `body` `n` times; collect into an array,
    /// rejecting duplicates when `set`. Push it. `minWire` is the element's
    /// minimum wire size for the length guard. Net `+1`.
    case seq(set: Bool, minWire: Int, body: Program)
    /// Read a `u32` length `n`; run `key` then `value` `n` times; assemble an
    /// object (string keys), rejecting duplicate keys. Push it. Net `+1`.
    case map(key: Program, value: Program)
    /// Run `body` `product(dimensions)` times (a fixed-shape array); collect into
    /// an array; push it. `minWire` bounds the product. Net `+1`.
    case fixedArray(dimensions: [UInt64], minWire: Int, body: Program)
    /// Read a presence byte; on `1` run `some`, on `0` push null. Net `+1`.
    case option(some: Program)
    /// Read a `u32` writer variant index; dispatch to the matching arm, run its
    /// payload, and wrap the result as a single-key object under the reader's
    /// variant name. An index with no arm is a writer-only variant: a decode
    /// error. Net `+1`.
    case enumeration(arms: [EnumArm])
}

/// One enum arm: the writer's variant index it matches, the reader's name for
/// that variant, and the lowered payload program.
public struct EnumArm {
    public var writerIndex: UInt32
    public var readerName: String
    public var payload: Program

    public init(writerIndex: UInt32, readerName: String, payload: Program) {
        self.writerIndex = writerIndex
        self.readerName = readerName
        self.payload = payload
    }
}
