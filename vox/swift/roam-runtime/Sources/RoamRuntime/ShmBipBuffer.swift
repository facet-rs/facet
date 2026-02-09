import Foundation
import CRoamShm

public enum ShmBipBufferError: Error, Equatable {
    case invalidCapacity
    case invalidHeaderOffset
    case regionTooSmall
    case grantTooLarge
    case commitOverflow
    case releaseOverflow
}

public struct ShmBipBufferHeaderView {
    public static let writeOffset = 0
    public static let watermarkOffset = 4
    public static let capacityOffset = 8
    public static let readOffset = 64
}

public final class ShmBipBuffer: @unchecked Sendable {
    public let capacity: UInt32

    private let headerPointer: UnsafeMutablePointer<roam_bipbuf_header_t>
    private let dataPointer: UnsafeMutableRawPointer

    public static func initialize(region: ShmRegion, headerOffset: Int, capacity: UInt32) throws -> ShmBipBuffer {
        guard capacity > 0 else {
            throw ShmBipBufferError.invalidCapacity
        }
        if headerOffset < 0 || !headerOffset.isMultiple(of: 64) {
            throw ShmBipBufferError.invalidHeaderOffset
        }

        let required = headerOffset + shmBipbufHeaderSize + Int(capacity)
        if required > region.length {
            throw ShmBipBufferError.regionTooSmall
        }

        let headerRaw = try region.pointer(at: headerOffset)
        memset(headerRaw, 0, shmBipbufHeaderSize)
        let header = headerRaw.assumingMemoryBound(to: roam_bipbuf_header_t.self)
        roam_bipbuf_init(header, capacity)

        return try attach(region: region, headerOffset: headerOffset)
    }

    public static func attach(region: ShmRegion, headerOffset: Int) throws -> ShmBipBuffer {
        if headerOffset < 0 || !headerOffset.isMultiple(of: 64) {
            throw ShmBipBufferError.invalidHeaderOffset
        }

        let headerRaw = try region.pointer(at: headerOffset)
        let header = headerRaw.assumingMemoryBound(to: roam_bipbuf_header_t.self)
        let capacity = roam_bipbuf_capacity(header)
        guard capacity > 0 else {
            throw ShmBipBufferError.invalidCapacity
        }

        let required = headerOffset + shmBipbufHeaderSize + Int(capacity)
        if required > region.length {
            throw ShmBipBufferError.regionTooSmall
        }

        return ShmBipBuffer(
            headerPointer: header,
            dataPointer: headerRaw.advanced(by: shmBipbufHeaderSize),
            capacity: capacity
        )
    }

    private init(
        headerPointer: UnsafeMutablePointer<roam_bipbuf_header_t>,
        dataPointer: UnsafeMutableRawPointer,
        capacity: UInt32
    ) {
        self.headerPointer = headerPointer
        self.dataPointer = dataPointer
        self.capacity = capacity
    }

    func debugState() -> (read: UInt32, write: UInt32, watermark: UInt32) {
        (
            read: roam_bipbuf_load_read_acquire(headerPointer),
            write: roam_bipbuf_load_write_acquire(headerPointer),
            watermark: roam_bipbuf_load_watermark_acquire(headerPointer)
        )
    }

    public func tryGrant(_ len: UInt32) throws -> UnsafeMutableRawBufferPointer? {
        if len == 0 {
            return UnsafeMutableRawBufferPointer(start: dataPointer, count: 0)
        }
        if len > capacity {
            throw ShmBipBufferError.grantTooLarge
        }

        var offset: UInt32 = 0
        let result = roam_bipbuf_try_grant(headerPointer, len, &offset)
        if result < 0 {
            throw ShmBipBufferError.grantTooLarge
        }
        if result == 0 {
            return nil
        }
        let ptr = dataPointer.advanced(by: Int(offset))
        return UnsafeMutableRawBufferPointer(start: ptr, count: Int(len))
    }

    public func commit(_ len: UInt32) throws {
        if roam_bipbuf_commit(headerPointer, len) != 0 {
            throw ShmBipBufferError.commitOverflow
        }
    }

    public func tryRead() -> UnsafeRawBufferPointer? {
        var offset: UInt32 = 0
        var length: UInt32 = 0
        let result = roam_bipbuf_try_read(headerPointer, &offset, &length)
        if result == 1 {
            let ptr = UnsafeRawPointer(dataPointer.advanced(by: Int(offset)))
            return UnsafeRawBufferPointer(start: ptr, count: Int(length))
        }
        return nil
    }

    public func release(_ len: UInt32) throws {
        if roam_bipbuf_release(headerPointer, len) != 0 {
            throw ShmBipBufferError.releaseOverflow
        }
    }

    public func isEmpty() -> Bool {
        let read = roam_bipbuf_load_read_acquire(headerPointer)
        let write = roam_bipbuf_load_write_acquire(headerPointer)
        let watermark = roam_bipbuf_load_watermark_acquire(headerPointer)
        return read == write && watermark == 0
    }
}
