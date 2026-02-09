#if os(macOS)
import Darwin
import Foundation

public enum ShmFileCleanup: Sendable {
    case manual
    case auto
}

public enum ShmRegionError: Error, Equatable {
    case invalidSize
    case invalidOffset
    case createFailed(errno: Int32)
    case openFailed(errno: Int32)
    case statFailed(errno: Int32)
    case mmapFailed(errno: Int32)
    case munmapFailed(errno: Int32)
    case ftruncateFailed(errno: Int32)
    case fchmodFailed(errno: Int32)
    case resizeShrinkUnsupported
    case emptyFile
}

public final class ShmRegion: @unchecked Sendable {
    public let path: String
    public private(set) var length: Int

    private var pointer: UnsafeMutableRawPointer
    private let fd: Int32
    private var ownsFile: Bool

    private init(
        path: String,
        length: Int,
        pointer: UnsafeMutableRawPointer,
        fd: Int32,
        ownsFile: Bool
    ) {
        self.path = path
        self.length = length
        self.pointer = pointer
        self.fd = fd
        self.ownsFile = ownsFile
    }

    deinit {
        if munmap(pointer, length) != 0 {
            _ = errno
        }
        close(fd)
        if ownsFile {
            unlink(path)
        }
    }

    public static func create(path: String, size: Int, cleanup: ShmFileCleanup) throws -> ShmRegion {
        guard size > 0 else {
            throw ShmRegionError.invalidSize
        }

        let fd = open(path, O_RDWR | O_CREAT | O_TRUNC, S_IRUSR | S_IWUSR)
        guard fd >= 0 else {
            throw ShmRegionError.createFailed(errno: errno)
        }

        if fchmod(fd, mode_t(S_IRUSR | S_IWUSR)) != 0 {
            let err = errno
            close(fd)
            throw ShmRegionError.fchmodFailed(errno: err)
        }

        if ftruncate(fd, off_t(size)) != 0 {
            let err = errno
            close(fd)
            throw ShmRegionError.ftruncateFailed(errno: err)
        }

        guard let mapped = mmap(nil, size, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0),
            mapped != MAP_FAILED
        else {
            let err = errno
            close(fd)
            throw ShmRegionError.mmapFailed(errno: err)
        }

        if cleanup == .auto {
            unlink(path)
        }

        return ShmRegion(
            path: path,
            length: size,
            pointer: mapped,
            fd: fd,
            ownsFile: cleanup == .manual
        )
    }

    public static func attach(path: String) throws -> ShmRegion {
        let fd = open(path, O_RDWR)
        guard fd >= 0 else {
            throw ShmRegionError.openFailed(errno: errno)
        }

        var statBuf = stat()
        guard fstat(fd, &statBuf) == 0 else {
            let err = errno
            close(fd)
            throw ShmRegionError.statFailed(errno: err)
        }

        let size = Int(statBuf.st_size)
        guard size > 0 else {
            close(fd)
            throw ShmRegionError.emptyFile
        }

        guard let mapped = mmap(nil, size, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0),
            mapped != MAP_FAILED
        else {
            let err = errno
            close(fd)
            throw ShmRegionError.mmapFailed(errno: err)
        }

        return ShmRegion(path: path, length: size, pointer: mapped, fd: fd, ownsFile: false)
    }

    public func takeOwnership() {
        ownsFile = true
    }

    public func releaseOwnership() {
        ownsFile = false
    }

    public func resize(newSize: Int) throws {
        if newSize < length {
            throw ShmRegionError.resizeShrinkUnsupported
        }
        if newSize == length {
            return
        }

        if ftruncate(fd, off_t(newSize)) != 0 {
            throw ShmRegionError.ftruncateFailed(errno: errno)
        }

        if munmap(pointer, length) != 0 {
            throw ShmRegionError.munmapFailed(errno: errno)
        }

        guard let mapped = mmap(nil, newSize, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0),
            mapped != MAP_FAILED
        else {
            throw ShmRegionError.mmapFailed(errno: errno)
        }

        pointer = mapped
        length = newSize
    }

    @inline(__always)
    public func basePointer() -> UnsafeMutableRawPointer {
        pointer
    }

    @inline(__always)
    public func pointer(at offset: Int) throws -> UnsafeMutableRawPointer {
        guard offset >= 0, offset < length else {
            throw ShmRegionError.invalidOffset
        }
        return pointer.advanced(by: offset)
    }

    @inline(__always)
    public func mutableBytes(at offset: Int, count: Int) throws -> UnsafeMutableRawBufferPointer {
        guard offset >= 0, count >= 0, offset + count <= length else {
            throw ShmRegionError.invalidOffset
        }
        return UnsafeMutableRawBufferPointer(start: pointer.advanced(by: offset), count: count)
    }
}

#else
import Foundation

public enum ShmFileCleanup: Sendable {
    case manual
    case auto
}

public enum ShmRegionError: Error, Equatable {
    case unsupportedPlatform
}

public final class ShmRegion: @unchecked Sendable {
    public let path: String = ""
    public let length: Int = 0

    public static func create(path: String, size: Int, cleanup: ShmFileCleanup) throws -> ShmRegion {
        throw ShmRegionError.unsupportedPlatform
    }

    public static func attach(path: String) throws -> ShmRegion {
        throw ShmRegionError.unsupportedPlatform
    }

    public func takeOwnership() {}
    public func releaseOwnership() {}
    public func resize(newSize: Int) throws {
        throw ShmRegionError.unsupportedPlatform
    }
    public func basePointer() -> UnsafeMutableRawPointer {
        UnsafeMutableRawPointer(bitPattern: 1)!
    }
    public func pointer(at offset: Int) throws -> UnsafeMutableRawPointer {
        throw ShmRegionError.unsupportedPlatform
    }
    public func mutableBytes(at offset: Int, count: Int) throws -> UnsafeMutableRawBufferPointer {
        throw ShmRegionError.unsupportedPlatform
    }
}
#endif
