import CBlake3

// BLAKE3 as a `Sink`: the identity canonical encoding is fed straight into the
// hasher. A `SchemaId` is the first 8 bytes of the digest read as a little-endian
// `u64` — matching `rust/phon-schema/src/identity.rs`.

/// An incremental BLAKE3 hasher.
public struct Blake3: Sink {
    private var hasher = blake3_hasher()

    public init() { blake3_hasher_init(&hasher) }

    public mutating func put(_ bytes: UnsafeRawBufferPointer) {
        guard let base = bytes.baseAddress, bytes.count > 0 else { return }
        blake3_hasher_update(&hasher, base, bytes.count)
    }

    /// The full 32-byte digest. `finalize` is non-destructive (it reads a const
    /// hasher in C); a local copy keeps this method non-`mutating`.
    public func digest() -> [UInt8] {
        var h = hasher
        var out = [UInt8](repeating: 0, count: 32)
        blake3_hasher_finalize(&h, &out, 32)
        return out
    }

    /// The schema id: the first 8 digest bytes as a little-endian `u64`.
    public func finalizeId() -> SchemaId {
        var h = hasher
        var out = [UInt8](repeating: 0, count: 8)
        blake3_hasher_finalize(&h, &out, 8)
        var v: UInt64 = 0
        for i in 0..<8 { v |= UInt64(out[i]) << (8 * i) }
        return SchemaId(v)
    }
}
