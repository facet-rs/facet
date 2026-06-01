// Content-derived schema identity. A `SchemaId` is the first 8 bytes (LE `u64`)
// of BLAKE3 over a schema's canonical structural encoding. Mirrors
// `rust/phon-schema/src/identity.rs`; the cross-language oracle is that every
// implementation reproduces the same id from the same structure.
//
// This file currently covers the primitive case; composite and recursive
// identity (SCC partitioning, depth-indexed back-references) land with the
// schema model.

/// A content-derived type id: BLAKE3 of the canonical encoding, first 8 bytes LE.
public struct SchemaId: Hashable, Sendable, CustomStringConvertible {
    public var raw: UInt64
    public init(_ raw: UInt64) { self.raw = raw }
    public var description: String { "0x" + String(raw, radix: 16) }
}

/// The id of a primitive type: BLAKE3 of its length-prefixed tag string.
public func primitiveId(_ p: Primitive) -> SchemaId {
    var h = Blake3()
    h.writeStr(p.tag)
    return h.finalizeId()
}
