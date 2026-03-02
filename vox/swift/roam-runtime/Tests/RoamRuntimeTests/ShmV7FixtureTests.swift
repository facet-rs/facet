#if os(macOS)
import Foundation
import Testing

@testable import RoamRuntime

private func loadShmV7Fixture(_ name: String) throws -> [UInt8] {
    let testFile = URL(fileURLWithPath: #filePath)
    let projectRoot =
        testFile
        .deletingLastPathComponent()
        .deletingLastPathComponent()
        .deletingLastPathComponent()
        .deletingLastPathComponent()
        .deletingLastPathComponent()
    let path = projectRoot.appendingPathComponent("test-fixtures/golden-vectors/shm-v7/\(name).bin")
    return Array(try Data(contentsOf: path))
}

@inline(__always)
private func readU32LE(_ bytes: [UInt8], _ at: Int) -> UInt32 {
    UInt32(bytes[at])
        | (UInt32(bytes[at + 1]) << 8)
        | (UInt32(bytes[at + 2]) << 16)
        | (UInt32(bytes[at + 3]) << 24)
}

@inline(__always)
private func readU64LE(_ bytes: [UInt8], _ at: Int) -> UInt64 {
    let b0 = UInt64(bytes[at])
    let b1 = UInt64(bytes[at + 1]) << 8
    let b2 = UInt64(bytes[at + 2]) << 16
    let b3 = UInt64(bytes[at + 3]) << 24
    let b4 = UInt64(bytes[at + 4]) << 32
    let b5 = UInt64(bytes[at + 5]) << 40
    let b6 = UInt64(bytes[at + 6]) << 48
    let b7 = UInt64(bytes[at + 7]) << 56
    return b0 | b1 | b2 | b3 | b4 | b5 | b6 | b7
}

struct ShmV7FixtureTests {
    // r[verify shm.segment.magic.v7]
    // r[verify shm.segment.header]
    @Test func segmentHeaderHasV7MagicAndVersion() throws {
        let bytes = try loadShmV7Fixture("segment_header")
        #expect(bytes.count == 128)
        #expect(Array(bytes[0..<8]) == Array("ROAMHUB\u{07}".utf8))
        #expect(readU32LE(bytes, 8) == 7)
        #expect(readU32LE(bytes, 12) == 128)
    }

    // r[verify shm.framing.header]
    // r[verify shm.framing.flags]
    @Test func frameHeaderIsV7EightByteHeader() throws {
        let bytes = try loadShmV7Fixture("frame_header")
        #expect(bytes.count == 8)
        #expect(readU32LE(bytes, 0) == 20)
        #expect(bytes[4] == 0x01) // SLOT_REF
        #expect(bytes[5] == 0)
        #expect(bytes[6] == 0)
        #expect(bytes[7] == 0)
    }

    // r[verify shm.framing.inline]
    // r[verify shm.framing.slot-ref]
    // r[verify shm.framing.mmap-ref]
    @Test func inlineAndReferenceFixturesHaveExpectedSizes() throws {
        let inline = try loadShmV7Fixture("frame_inline")
        #expect(inline.count == 20)
        #expect(readU32LE(inline, 0) == 20)
        #expect(inline[4] == 0)
        #expect(Array(inline[8..<17]) == Array("swift-shm".utf8))

        let slotRef = try loadShmV7Fixture("frame_slot_ref")
        #expect(slotRef.count == 20)
        #expect(readU32LE(slotRef, 0) == 20)
        #expect(slotRef[4] == 0x01) // SLOT_REF

        let mmapRef = try loadShmV7Fixture("frame_mmap_ref")
        #expect(mmapRef.count == 32)
        #expect(readU32LE(mmapRef, 0) == 32)
        #expect(mmapRef[4] == 0x02) // MMAP_REF
        #expect(readU32LE(mmapRef, 8) == 9) // map_id
        #expect(readU32LE(mmapRef, 12) == 3) // map_generation
        #expect(readU64LE(mmapRef, 16) == 4096) // map_offset
        #expect(readU32LE(mmapRef, 24) == 8192) // payload_len
    }
}
#endif
