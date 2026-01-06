import XCTest

@testable import RoamRuntime

final class COBSTests: XCTestCase {
    func testCOBSRoundtrip() throws {
        let testCases: [[UInt8]] = [
            [],
            [1, 2, 3],
            [0],
            [0, 0],
            [1, 0, 2],
            [0, 1, 2, 3],
            [1, 2, 3, 0],
            Array(repeating: 0, count: 10),
            Array(1...254),
        ]

        for original in testCases {
            let encoded = cobsEncode(original)
            let decoded = try cobsDecode(encoded)
            XCTAssertEqual(decoded, original, "Failed for \(original.prefix(10))...")
        }
    }

    func testCOBSNoZeroBytesInEncoded() {
        let input: [UInt8] = [0, 1, 0, 2, 0]
        let encoded = cobsEncode(input)
        XCTAssertFalse(encoded.contains(0), "COBS encoded data should not contain zero bytes")
    }
}
