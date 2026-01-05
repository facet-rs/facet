import XCTest
@testable import Postcard

final class VarintTests: XCTestCase {
    // MARK: - Unsigned Varint Tests

    func testEncodeVarintZero() {
        let encoded = encodeVarint(0)
        XCTAssertEqual(encoded, [0x00])
    }

    func testEncodeVarintSmall() {
        // Values < 128 encode to single byte
        XCTAssertEqual(encodeVarint(1), [0x01])
        XCTAssertEqual(encodeVarint(127), [0x7F])
    }

    func testEncodeVarintTwoBytes() {
        // 128 = 0b10000000 -> 0x80 0x01
        XCTAssertEqual(encodeVarint(128), [0x80, 0x01])
        // 300 = 0b100101100 -> 0xAC 0x02
        XCTAssertEqual(encodeVarint(300), [0xAC, 0x02])
    }

    func testEncodeVarintLarge() {
        // Max u64 should encode to 10 bytes
        let maxU64 = UInt64.max
        let encoded = encodeVarint(maxU64)
        XCTAssertEqual(encoded.count, 10)
    }

    func testDecodeVarintRoundtrip() throws {
        let testValues: [UInt64] = [0, 1, 127, 128, 255, 256, 16383, 16384, 2097151, UInt64.max]

        for value in testValues {
            let encoded = encodeVarint(value)
            var data = Data(encoded)
            let decoded = try decodeVarint(from: &data)
            XCTAssertEqual(decoded, value, "Roundtrip failed for \(value)")
            XCTAssertTrue(data.isEmpty, "Data should be consumed for \(value)")
        }
    }

    func testDecodeVarintUnexpectedEnd() {
        // 0x80 has continuation bit set but no more data
        var data = Data([0x80])
        XCTAssertThrowsError(try decodeVarint(from: &data)) { error in
            XCTAssertEqual(error as? VarintError, .unexpectedEndOfData)
        }
    }

    // MARK: - Zigzag Tests

    func testZigzagEncodeZero() {
        XCTAssertEqual(zigzagEncode(0), 0)
    }

    func testZigzagEncodePositive() {
        XCTAssertEqual(zigzagEncode(1), 2)
        XCTAssertEqual(zigzagEncode(2), 4)
        XCTAssertEqual(zigzagEncode(100), 200)
    }

    func testZigzagEncodeNegative() {
        XCTAssertEqual(zigzagEncode(-1), 1)
        XCTAssertEqual(zigzagEncode(-2), 3)
        XCTAssertEqual(zigzagEncode(-100), 199)
    }

    func testZigzagRoundtrip() {
        let testValues: [Int64] = [0, 1, -1, 2, -2, 127, -128, 1000, -1000, Int64.max, Int64.min]

        for value in testValues {
            let encoded = zigzagEncode(value)
            let decoded = zigzagDecode(encoded)
            XCTAssertEqual(decoded, value, "Zigzag roundtrip failed for \(value)")
        }
    }

    // MARK: - Signed Varint Tests

    func testSignedVarintRoundtrip() throws {
        let testValues: [Int64] = [0, 1, -1, 127, -128, 1000, -1000, Int64.max, Int64.min]

        for value in testValues {
            let encoded = encodeSignedVarint(value)
            var data = Data(encoded)
            let decoded = try decodeSignedVarint(from: &data)
            XCTAssertEqual(decoded, value, "Signed varint roundtrip failed for \(value)")
        }
    }
}
