import Foundation
#if os(macOS)
import Darwin
#endif

public struct ShmBootstrapTicket: Sendable {
    public let peerId: UInt8
    public let hubPath: String
    public let doorbellFd: Int32

    public init(peerId: UInt8, hubPath: String, doorbellFd: Int32) {
        self.peerId = peerId
        self.hubPath = hubPath
        self.doorbellFd = doorbellFd
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

            let passedFd = try recvPassedFd(fd: fd)
            close(fd)
            return ShmBootstrapTicket(peerId: response.peerId, hubPath: hubPath, doorbellFd: passedFd)

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
            write(fd, rawBuf.baseAddress!.advanced(by: offset), bytes.count - offset)
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

private func recvPassedFd(fd: Int32) throws -> Int32 {
    var byte = [UInt8](repeating: 0, count: 1)
    var iov = iovec(iov_base: nil, iov_len: 1)

    let controlLen = cmsgSpace(MemoryLayout<Int32>.size)
    var control = [UInt8](repeating: 0, count: controlLen)
    let receivedFd: Int32? = byte.withUnsafeMutableBytes { byteBuf in
        iov.iov_base = byteBuf.baseAddress
        return withUnsafeMutablePointer(to: &iov) { iovPtr in
            control.withUnsafeMutableBytes { controlBuf in
                var msg = msghdr()
                msg.msg_iov = iovPtr
                msg.msg_iovlen = 1
                msg.msg_control = controlBuf.baseAddress
                msg.msg_controllen = socklen_t(controlBuf.count)

                while true {
                    let n = recvmsg(fd, &msg, 0)
                    if n < 0 {
                        if errno == EINTR {
                            continue
                        }
                        return nil
                    }
                    if n == 0 {
                        return nil
                    }
                    break
                }

                guard msg.msg_controllen >= MemoryLayout<cmsghdr>.size else {
                    return nil
                }
                guard let base = controlBuf.baseAddress else {
                    return nil
                }

                let cmsg = base.assumingMemoryBound(to: cmsghdr.self)
                if cmsg.pointee.cmsg_level != SOL_SOCKET || cmsg.pointee.cmsg_type != SCM_RIGHTS {
                    return nil
                }

                let minLen = cmsgLen(MemoryLayout<Int32>.size)
                if Int(cmsg.pointee.cmsg_len) < minLen {
                    return nil
                }

                let dataOffset = cmsgDataOffset()
                guard dataOffset + MemoryLayout<Int32>.size <= controlBuf.count else {
                    return nil
                }

                return base
                    .advanced(by: dataOffset)
                    .assumingMemoryBound(to: Int32.self)
                    .pointee
            }
        }
    }

    guard let receivedFd else {
        throw ShmBootstrapError.missingFileDescriptor
    }

    return receivedFd
}

private func cmsgAlign(_ n: Int) -> Int {
    (n + 3) & ~3
}

private func cmsgLen(_ dataLen: Int) -> Int {
    cmsgAlign(MemoryLayout<cmsghdr>.size) + dataLen
}

private func cmsgSpace(_ dataLen: Int) -> Int {
    cmsgAlign(MemoryLayout<cmsghdr>.size) + cmsgAlign(dataLen)
}

private func cmsgDataOffset() -> Int {
    cmsgAlign(MemoryLayout<cmsghdr>.size)
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
