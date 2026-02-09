import Foundation
#if os(macOS)
import CRoamShm
import Darwin
#endif

public struct ShmBootstrapTicket: Sendable {
    public let peerId: UInt8
    public let hubPath: String
    public let doorbellFd: Int32
    public let shmFd: Int32

    public init(peerId: UInt8, hubPath: String, doorbellFd: Int32, shmFd: Int32 = -1) {
        self.peerId = peerId
        self.hubPath = hubPath
        self.doorbellFd = doorbellFd
        self.shmFd = shmFd
    }
}

public enum ShmBootstrapError: Error {
    case invalidSid
    case unsupportedPlatform
    case invalidSocketPath
    case socketCreateFailed(errno: Int32)
    case connectFailed(errno: Int32)
    case writeFailed(errno: Int32)
    case readFailed(errno: Int32)
    case eof
    case invalidRequestEncoding
    case invalidResponseMagic
    case protocolError(String)
    case missingFileDescriptor
}

#if os(macOS)
private let shmBootstrapRequestMagic = [UInt8]("RSH0".utf8)
private let shmBootstrapResponseMagic = [UInt8]("RSP0".utf8)

private let shmBootstrapStatusOK: UInt8 = 0
private let shmBootstrapStatusError: UInt8 = 1

public func requestShmBootstrapTicket(controlSocketPath: String, sid: String) throws ->
    ShmBootstrapTicket
{
    guard isValidSid(sid) else {
        throw ShmBootstrapError.invalidSid
    }

    let fd = socket(AF_UNIX, SOCK_STREAM, 0)
    guard fd >= 0 else {
        throw ShmBootstrapError.socketCreateFailed(errno: errno)
    }

    do {
        try connectUnixSocket(fd: fd, path: controlSocketPath)
        try writeBootstrapRequest(fd: fd, sid: sid)
        let response = try readBootstrapResponse(fd: fd)

        switch response.status {
        case shmBootstrapStatusOK:
            guard response.peerId != 0 else {
                throw ShmBootstrapError.protocolError("invalid peer_id 0")
            }
            guard let hubPath = String(bytes: response.payload, encoding: .utf8) else {
                throw ShmBootstrapError.protocolError("hub path not utf-8")
            }

            _ = try? writeFdAck(fd: fd)
            let fds = try recvPassedFds(fd: fd, expected: 2)
            let doorbellFd = fds[0]
            let shmFd = fds[1]
            close(fd)
            return ShmBootstrapTicket(
                peerId: response.peerId,
                hubPath: hubPath,
                doorbellFd: doorbellFd,
                shmFd: shmFd
            )

        case shmBootstrapStatusError:
            let msg = String(bytes: response.payload, encoding: .utf8) ?? "bootstrap error"
            throw ShmBootstrapError.protocolError(msg)

        default:
            throw ShmBootstrapError.protocolError("unknown bootstrap status \(response.status)")
        }
    } catch {
        close(fd)
        throw error
    }
}

private struct BootstrapResponse {
    let status: UInt8
    let peerId: UInt8
    let payload: [UInt8]
}

private func writeBootstrapRequest(fd: Int32, sid: String) throws {
    let sidBytes = [UInt8](sid.utf8)
    guard let sidLen = UInt16(exactly: sidBytes.count) else {
        throw ShmBootstrapError.invalidRequestEncoding
    }

    var out: [UInt8] = []
    out.reserveCapacity(4 + 2 + sidBytes.count)
    out.append(contentsOf: shmBootstrapRequestMagic)
    out.append(UInt8(truncatingIfNeeded: sidLen & 0x00FF))
    out.append(UInt8(truncatingIfNeeded: (sidLen >> 8) & 0x00FF))
    out.append(contentsOf: sidBytes)

    try writeAll(fd: fd, bytes: out)
}

private func readBootstrapResponse(fd: Int32) throws -> BootstrapResponse {
    let magic = try readExactly(fd: fd, count: 4)
    guard magic == shmBootstrapResponseMagic else {
        throw ShmBootstrapError.invalidResponseMagic
    }

    let status = try readExactly(fd: fd, count: 1)[0]
    let peerId = try readExactly(fd: fd, count: 1)[0]
    let lenBytes = try readExactly(fd: fd, count: 2)
    let payloadLen = Int(UInt16(lenBytes[0]) | (UInt16(lenBytes[1]) << 8))
    let payload = try readExactly(fd: fd, count: payloadLen)

    return BootstrapResponse(status: status, peerId: peerId, payload: payload)
}

private func connectUnixSocket(fd: Int32, path: String) throws {
    let one: Int32 = 1
    _ = withUnsafePointer(to: one) { ptr in
        setsockopt(fd, SOL_SOCKET, SO_NOSIGPIPE, ptr, socklen_t(MemoryLayout<Int32>.size))
    }

    var addr = sockaddr_un()
    addr.sun_family = sa_family_t(AF_UNIX)

    let pathBytes = [UInt8](path.utf8)
    let maxPathLen = MemoryLayout.size(ofValue: addr.sun_path)
    guard pathBytes.count < maxPathLen else {
        throw ShmBootstrapError.invalidSocketPath
    }

    withUnsafeMutablePointer(to: &addr.sun_path) { sunPathPtr in
        let raw = UnsafeMutableRawPointer(sunPathPtr)
        raw.initializeMemory(as: UInt8.self, repeating: 0, count: maxPathLen)
        raw.copyMemory(from: pathBytes, byteCount: pathBytes.count)
    }

    let result = withUnsafePointer(to: &addr) { ptr in
        ptr.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockPtr in
            connect(fd, sockPtr, socklen_t(MemoryLayout<sockaddr_un>.size))
        }
    }

    if result != 0 {
        throw ShmBootstrapError.connectFailed(errno: errno)
    }
}

private func writeAll(fd: Int32, bytes: [UInt8]) throws {
    var offset = 0
    while offset < bytes.count {
        let written = bytes.withUnsafeBytes { rawBuf in
            send(fd, rawBuf.baseAddress!.advanced(by: offset), bytes.count - offset, 0)
        }
        if written < 0 {
            if errno == EINTR {
                continue
            }
            throw ShmBootstrapError.writeFailed(errno: errno)
        }
        if written == 0 {
            throw ShmBootstrapError.eof
        }
        offset += written
    }
}

private func readExactly(fd: Int32, count: Int) throws -> [UInt8] {
    if count == 0 {
        return []
    }

    var out = [UInt8](repeating: 0, count: count)
    var offset = 0

    while offset < count {
        let readCount = out.withUnsafeMutableBytes { rawBuf in
            read(fd, rawBuf.baseAddress!.advanced(by: offset), count - offset)
        }
        if readCount < 0 {
            if errno == EINTR {
                continue
            }
            throw ShmBootstrapError.readFailed(errno: errno)
        }
        if readCount == 0 {
            throw ShmBootstrapError.eof
        }
        offset += readCount
    }

    return out
}

private func recvPassedFds(fd: Int32, expected: Int) throws -> [Int32] {
    var out = [Int32](repeating: -1, count: max(expected, 1))
    while true {
        let rc = roam_recv_fds(fd, &out, Int32(out.count))
        if rc > 0 {
            if rc < expected {
                throw ShmBootstrapError.missingFileDescriptor
            }
            return Array(out.prefix(Int(rc)))
        }
        if rc == 0 {
            throw ShmBootstrapError.missingFileDescriptor
        }
        if errno == EINTR {
            continue
        }
        throw ShmBootstrapError.missingFileDescriptor
    }
}

private func writeFdAck(fd: Int32) throws {
    let ack: [UInt8] = [0xA5]
    try writeAll(fd: fd, bytes: ack)
}

private func isValidSid(_ sid: String) -> Bool {
    if sid.count == 32 {
        return sid.unicodeScalars.allSatisfy { CharacterSet(charactersIn: "0123456789abcdefABCDEF").contains($0) }
    }

    if sid.count == 36 {
        let chars = Array(sid)
        let hyphenPositions: Set<Int> = [8, 13, 18, 23]
        for i in 0..<chars.count {
            if hyphenPositions.contains(i) {
                if chars[i] != "-" {
                    return false
                }
                continue
            }
            if !chars[i].isHexDigit {
                return false
            }
        }
        return true
    }

    return false
}
#else
public func requestShmBootstrapTicket(controlSocketPath: String, sid: String) throws -> ShmBootstrapTicket {
    _ = controlSocketPath
    _ = sid
    throw ShmBootstrapError.unsupportedPlatform
}
#endif
