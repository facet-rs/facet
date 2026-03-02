#if os(macOS)
import Darwin
import Foundation
import Testing

@testable import RoamRuntime

private func loadShmFixture(_ name: String) throws -> [UInt8] {
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

private func makeTempPath(_ suffix: String) -> String {
    let id = UUID().uuidString
    return "/tmp/roam-swift-shm-\(id)-\(suffix)"
}

struct ShmFoundationFixtureParityTests {
    // r[verify shm.segment.header]
    // r[verify shm.segment.magic.v7]
    // r[verify shm.segment.config]
    @Test func segmentHeaderFixtureParses() throws {
        let bytes = try loadShmFixture("segment_header")
        let header = try ShmSegmentHeader.decode(from: bytes)
        try header.validateV2()

        #expect(header.magic == shmSegmentMagic)
        #expect(header.version == shmSegmentVersion)
        #expect(header.headerSize == 128)
        #expect(header.maxGuests == 2)
        #expect(header.bipbufCapacity == 128)
        #expect(header.varSlotPoolOffset > 0)
    }

    // r[verify shm.segment]
    // r[verify shm.peer-table]
    // r[verify shm.peer-table.states]
    @Test func segmentLayoutPeerViewMatchesFixture() throws {
        let bytes = try loadShmFixture("segment_layout")
        let path = makeTempPath("segment.bin")
        defer { try? FileManager.default.removeItem(atPath: path) }

        FileManager.default.createFile(atPath: path, contents: Data(bytes), attributes: nil)
        let region = try ShmRegion.attach(path: path)
        let view = try ShmSegmentView(region: region)

        let peer = try view.peerEntry(peerId: 1)
        #expect(peer.state == 1)
        #expect(peer.epoch == 7)
        #expect(peer.lastHeartbeat == 12_345_678)

    }

    // r[verify shm.framing.header]
    // r[verify shm.framing.inline]
    // r[verify shm.framing.alignment]
    // r[verify shm.framing.slot-ref]
    @Test func frameAndSlotRefFixtureParity() throws {
        let headerBytes = try loadShmFixture("frame_header")
        let header = try #require(ShmFrameHeader.read(from: headerBytes))
        #expect(header.totalLen == 20)
        #expect(header.flags == shmFlagSlotRef)

        let slotRefBytes = try loadShmFixture("slot_ref")
        let slotRef = try #require(ShmSlotRef.read(from: slotRefBytes))
        #expect(slotRef.classIdx == 2)
        #expect(slotRef.extentIdx == 1)
        #expect(slotRef.slotIdx == 42)
        #expect(slotRef.slotGeneration == 7)

        let inline = try loadShmFixture("frame_inline")
        let decodedInline = try decodeShmFrame(inline)
        guard case .inline(_, let payload) = decodedInline else {
            Issue.record("expected inline frame")
            return
        }
        #expect(payload.starts(with: Array("swift-shm".utf8)))

        let slotRefFrame = try loadShmFixture("frame_slot_ref")
        let decodedSlotRef = try decodeShmFrame(slotRefFrame)
        guard case .slotRef(let slotRefHeader, let parsedSlotRef) = decodedSlotRef else {
            Issue.record("expected slot-ref frame")
            return
        }
        #expect(slotRefHeader.totalLen == 20)
        #expect(parsedSlotRef.slotIdx == 42)
    }
}

struct ShmHeaderValidationTests {
    // r[verify shm.segment.header]
    // r[verify shm.segment.magic.v7]
    @Test func rejectsInvalidHeaderInvariants() throws {
        var bytes = try loadShmFixture("segment_header")

        bytes[0] = 0
        #expect(throws: ShmLayoutError.self) {
            let header = try ShmSegmentHeader.decode(from: bytes)
            try header.validateV2()
        }

        bytes = try loadShmFixture("segment_header")
        bytes[8] = 1
        #expect(throws: ShmLayoutError.unsupportedVersion(1)) {
            let header = try ShmSegmentHeader.decode(from: bytes)
            try header.validateV2()
        }

        bytes = try loadShmFixture("segment_header")
        bytes[48] = 0
        bytes[49] = 0
        bytes[50] = 0
        bytes[51] = 0
        bytes[52] = 0
        bytes[53] = 0
        bytes[54] = 0
        bytes[55] = 0
        #expect(throws: ShmLayoutError.missingVarSlotPool) {
            let header = try ShmSegmentHeader.decode(from: bytes)
            try header.validateV2()
        }
    }

    // r[verify shm.framing]
    // r[verify shm.framing.header]
    // r[verify shm.framing.slot-ref]
    @Test func rejectsMalformedFrames() throws {
        #expect(throws: ShmFrameDecodeError.shortHeader) {
            _ = try decodeShmFrame([1, 2, 3])
        }

        var frame = encodeShmInlineFrame(payload: [1, 2, 3])
        frame[0] = 0
        frame[1] = 0
        frame[2] = 0
        frame[3] = 0
        #expect(throws: ShmFrameDecodeError.invalidTotalLength(0)) {
            _ = try decodeShmFrame(frame)
        }

        frame = encodeShmSlotRefFrame(slotRef: ShmSlotRef(classIdx: 1, extentIdx: 0, slotIdx: 2, slotGeneration: 3))
        frame[0] = 40
        #expect(throws: ShmFrameDecodeError.shortFrame(required: 40, available: 20)) {
            _ = try decodeShmFrame(frame)
        }
    }
}

struct ShmBipBufferCorrectnessTests {
    private enum StressError: Error {
        case mismatch(expected: UInt32, actual: UInt32, read: UInt32, write: UInt32, watermark: UInt32)
        case timeout
    }

    // r[verify shm.bipbuf]
    // r[verify shm.bipbuf.header]
    // r[verify shm.bipbuf.init]
    // r[verify shm.bipbuf.layout]
    // r[verify shm.file.create]
    // r[verify shm.file.attach]
    // r[verify shm.bipbuf.read]
    // r[verify shm.bipbuf.release]
    @Test func contiguousReadWrite() throws {
        let path = makeTempPath("bipbuf.bin")
        defer { try? FileManager.default.removeItem(atPath: path) }

        let region = try ShmRegion.create(path: path, size: shmBipbufHeaderSize + 256, cleanup: .manual)
        let buf = try ShmBipBuffer.initialize(region: region, headerOffset: 0, capacity: 256)

        let grant = try #require(try buf.tryGrant(10))
        grant.copyBytes(from: Array("helloworld".utf8))
        try buf.commit(10)

        let data = try #require(buf.tryRead())
        #expect(Array(data) == Array("helloworld".utf8))
        try buf.release(10)
        #expect(buf.tryRead() == nil)
        #expect(buf.isEmpty())
    }

    // r[verify shm.bipbuf]
    // r[verify shm.bipbuf.backpressure]
    @Test func wrapAndWatermarkBehavior() throws {
        let path = makeTempPath("bipbuf-wrap.bin")
        defer { try? FileManager.default.removeItem(atPath: path) }

        let region = try ShmRegion.create(path: path, size: shmBipbufHeaderSize + 32, cleanup: .manual)
        let buf = try ShmBipBuffer.initialize(region: region, headerOffset: 0, capacity: 32)

        let first = try #require(try buf.tryGrant(24))
        for i in 0..<24 {
            first[i] = UInt8(i)
        }
        try buf.commit(24)

        let firstRead = try #require(buf.tryRead())
        #expect(firstRead.count == 24)
        try buf.release(20)

        let wrapped = try #require(try buf.tryGrant(16))
        for i in 0..<16 {
            wrapped[i] = UInt8(100 + i)
        }
        try buf.commit(16)

        let tail = try #require(buf.tryRead())
        #expect(Array(tail) == [20, 21, 22, 23])
        try buf.release(4)

        let wrappedRead = try #require(buf.tryRead())
        #expect(Array(wrappedRead) == Array((100..<116).map(UInt8.init)))
        try buf.release(16)
        #expect(buf.tryRead() == nil)
    }

    // r[verify shm.bipbuf.backpressure]
    @Test func fullAndEmptyBoundaries() throws {
        let path = makeTempPath("bipbuf-boundary.bin")
        defer { try? FileManager.default.removeItem(atPath: path) }

        let region = try ShmRegion.create(path: path, size: shmBipbufHeaderSize + 16, cleanup: .manual)
        let buf = try ShmBipBuffer.initialize(region: region, headerOffset: 0, capacity: 16)

        let full = try #require(try buf.tryGrant(16))
        for i in 0..<16 {
            full[i] = UInt8(i)
        }
        try buf.commit(16)

        #expect(try buf.tryGrant(1) == nil)
        let read = try #require(buf.tryRead())
        #expect(read.count == 16)
        try buf.release(16)
        #expect(buf.isEmpty())
    }

    // r[verify shm.bipbuf]
    // r[verify shm.bipbuf.grant]
    // r[verify shm.bipbuf.commit]
    // r[verify shm.bipbuf.read]
    // r[verify shm.bipbuf.release]
    @Test func randomizedModel() throws {
        let path = makeTempPath("bipbuf-model.bin")
        defer { try? FileManager.default.removeItem(atPath: path) }

        let capacity = 128
        let region = try ShmRegion.create(path: path, size: shmBipbufHeaderSize + capacity, cleanup: .manual)
        let buf = try ShmBipBuffer.initialize(region: region, headerOffset: 0, capacity: UInt32(capacity))

        var rng = UInt64(0xC0FFEE)
        var model: [UInt8] = []

        for _ in 0..<2000 {
            rng = rng &* 6364136223846793005 &+ 1
            let op = Int((rng >> 33) % 3)

            if op == 0 {
                let len = Int((rng >> 12) % 24)
                if len <= capacity - model.count {
                    if let grant = try buf.tryGrant(UInt32(len)) {
                        var bytes = [UInt8](repeating: 0, count: len)
                        for i in 0..<len {
                            rng = rng &* 2862933555777941757 &+ 3037000493
                            bytes[i] = UInt8(truncatingIfNeeded: rng >> 56)
                            grant[i] = bytes[i]
                        }
                        try buf.commit(UInt32(len))
                        model.append(contentsOf: bytes)
                    }
                }
            } else {
                if let readable = buf.tryRead() {
                    let take = min(readable.count, Int((rng >> 17) % 24) + 1)
                    let got = Array(readable.prefix(take))
                    let expected = Array(model.prefix(take))
                    #expect(got == expected)
                    try buf.release(UInt32(take))
                    model.removeFirst(take)
                }
            }
        }

        while let readable = buf.tryRead() {
            let got = Array(readable)
            let expected = Array(model.prefix(got.count))
            #expect(got == expected)
            try buf.release(UInt32(got.count))
            model.removeFirst(got.count)
        }
        #expect(model.isEmpty)
    }

    // r[verify shm.bipbuf]
    // r[verify shm.bipbuf.grant]
    // r[verify shm.bipbuf.commit]
    // r[verify shm.bipbuf.read]
    // r[verify shm.bipbuf.release]
    @Test func boundedConcurrentStress() async throws {
        let path = makeTempPath("bipbuf-stress.bin")
        defer { try? FileManager.default.removeItem(atPath: path) }

        let capacity = 4096
        let region = try ShmRegion.create(path: path, size: shmBipbufHeaderSize + capacity, cleanup: .manual)
        let buf = try ShmBipBuffer.initialize(region: region, headerOffset: 0, capacity: UInt32(capacity))

        let iterations = 10_000

        func producerLoop() async throws {
            var value: UInt32 = 0
            while Int(value) < iterations {
                if let grant = try buf.tryGrant(4) {
                    let le = value.littleEndian
                    grant.storeBytes(of: le, as: UInt32.self)
                    try buf.commit(4)
                    value += 1
                } else {
                    await Task.yield()
                }
            }
        }

        func consumerLoop() async throws {
            var expected: UInt32 = 0
            while Int(expected) < iterations {
                guard let readable = buf.tryRead() else {
                    await Task.yield()
                    continue
                }

                let chunks = readable.count / 4
                if chunks == 0 {
                    await Task.yield()
                    continue
                }

                for i in 0..<chunks {
                    let off = i * 4
                    let b0 = UInt32(readable[off])
                    let b1 = UInt32(readable[off + 1]) << 8
                    let b2 = UInt32(readable[off + 2]) << 16
                    let b3 = UInt32(readable[off + 3]) << 24
                    let value = b0 | b1 | b2 | b3
                    if value != expected {
                        let state = buf.debugState()
                        throw StressError.mismatch(
                            expected: expected,
                            actual: value,
                            read: state.read,
                            write: state.write,
                            watermark: state.watermark
                        )
                    }
                    expected += 1
                }
                try buf.release(UInt32(chunks * 4))
            }
        }

        try await withThrowingTaskGroup(of: Void.self) { group in
            group.addTask {
                try await producerLoop()
            }
            group.addTask {
                try await consumerLoop()
            }
            group.addTask {
                try await Task.sleep(nanoseconds: 5_000_000_000)
                throw StressError.timeout
            }

            var finished = 0
            while let _ = try await group.next() {
                finished += 1
                if finished == 2 {
                    group.cancelAll()
                    break
                }
            }
        }

        #expect(buf.isEmpty())
    }
}

struct ShmFileRegionTests {
    // r[verify shm.file]
    // r[verify shm.file.create]
    // r[verify shm.file.attach]
    // r[verify shm.file.permissions]
    @Test func createManualSetsExpectedPermissionsAndAllowsAttach() throws {
        let path = makeTempPath("region-manual.bin")
        defer { try? FileManager.default.removeItem(atPath: path) }

        let region = try ShmRegion.create(path: path, size: 4096, cleanup: .manual)
        #expect(FileManager.default.fileExists(atPath: path))
        #expect(region.length == 4096)

        var st = stat()
        #expect(stat(path, &st) == 0)
        #expect((st.st_mode & S_IRWXU) == (S_IRUSR | S_IWUSR))

        let attached = try ShmRegion.attach(path: path)
        #expect(attached.length == 4096)
        _ = attached
    }

    // r[verify shm.file.cleanup]
    @Test func createAutoUnlinksBackingFile() throws {
        let path = makeTempPath("region-auto.bin")
        let region = try ShmRegion.create(path: path, size: 4096, cleanup: .auto)
        #expect(region.length == 4096)
        #expect(!FileManager.default.fileExists(atPath: path))
    }
}
#endif
