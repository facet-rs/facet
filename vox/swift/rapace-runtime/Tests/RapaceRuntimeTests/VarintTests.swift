import XCTest
@testable import RapaceRuntime

final class VarintTests: XCTestCase {
    func testEncodeDecodeVarint() throws {
        let testCases: [UInt64] = [
            0,
            1,
            127,
            128,
            255,
            256,
            0xFFFF,
            0xFFFFFFFF,
            UInt64.max
        ]

        for value in testCases {
            let encoded = encodeVarint(value)
            var offset = 0
            let decoded = try decodeVarint(from: Data(encoded), offset: &offset)
            XCTAssertEqual(decoded, value, "Failed for value \(value)")
            XCTAssertEqual(offset, encoded.count, "Offset should advance to end")
        }
    }

    func testDecodeVarintU32() throws {
        let encoded = encodeVarint(42)
        var offset = 0
        let decoded = try decodeVarintU32(from: Data(encoded), offset: &offset)
        XCTAssertEqual(decoded, 42)
    }

    func testDecodeVarintU32Overflow() throws {
        let tooLarge: UInt64 = UInt64(UInt32.max) + 1
        let encoded = encodeVarint(tooLarge)
        var offset = 0
        XCTAssertThrowsError(try decodeVarintU32(from: Data(encoded), offset: &offset))
    }
}
