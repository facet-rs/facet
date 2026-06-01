import Testing

@testable import PhonSchema

// The cross-language golden: every primitive's content-hash id, as produced by
// the Rust source of truth (and matched by TypeScript). If BLAKE3, the canonical
// encoding (length-prefixed tag string), or the LE truncation drifts, one of
// these fails immediately.
@Test
func primitiveIdsMatchGolden() {
    let golden: [(Primitive, UInt64)] = [
        (.bool, 0x178367a87f66fb46),
        (.u8, 0x2c8d54f2314d0f20),
        (.u16, 0x1be6c8d0625ea876),
        (.u32, 0x281c5be4f2ee63b4),
        (.u64, 0xd9356298b81639ac),
        (.u128, 0x767c691472231d95),
        (.i8, 0x3bd6a76856978968),
        (.i16, 0x269c2efb67f8a4c7),
        (.i32, 0x361f4536eee9f991),
        (.i64, 0xc6eb8c46f1e17fba),
        (.i128, 0xe935ee7d4b9fe594),
        (.f32, 0x8e02f623d1b2310c),
        (.f64, 0x3f2e589db81e95bf),
        (.char, 0x18937b725e2e911b),
        (.string, 0x6d7dce914ee150e8),
        (.bytes, 0xba8125876d6388b4),
        (.datetime, 0x2df96deecf87538d),
        (.uuid, 0x228b7a9a7c76c62c),
        (.qname, 0x18b4e7af90ad4c0f),
        (.unit, 0xbc5c33249a2dc720),
        (.never, 0x5db70a394660f3e6),
    ]
    for (p, expected) in golden {
        #expect(primitiveId(p).raw == expected, "primitiveId(.\(p.rawValue)) mismatch")
    }
    // Every primitive is covered.
    #expect(golden.count == Primitive.allCases.count)
}
