import Foundation
#if os(macOS) || os(Linux)
#if os(macOS)
import Darwin
#else
import Glibc
#endif
#endif

public struct ShmBootstrapTicket: Sendable {
    public let peerId: UInt8
    public let hubPath: String
    public let doorbellFd: Int32
    public let shmFd: Int32
    public let mmapControlFd: Int32

    public init(
        peerId: UInt8,
        hubPath: String,
        doorbellFd: Int32,
        shmFd: Int32 = -1,
        mmapControlFd: Int32 = -1
    ) {
        self.peerId = peerId
        self.hubPath = hubPath
        self.doorbellFd = doorbellFd
        self.shmFd = shmFd
        self.mmapControlFd = mmapControlFd
    }
}

// r[impl shm.spawn]
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

#if os(macOS) || os(Linux)
private let shmBootstrapStatusOK: UInt8 = 0
private let shmBootstrapStatusError: UInt8 = 1

// r[impl shm.spawn]
// r[impl shm.spawn.fd-inheritance]
public func requestShmBootstrapTicket(controlSocketPath: String, sid: String) throws ->
    ShmBootstrapTicket
{
    guard isValidSid(sid) else {
        throw ShmBootstrapError.invalidSid
    }

    // Use the current Rust wire bootstrap (`VSH1`/`VSP1` + SCM_RIGHTS).
    if let ticket = try requestShmBootstrapTicketRust(controlSocketPath: controlSocketPath) {
        return ticket
    }

    throw ShmBootstrapError.protocolError("peer did not complete VSH1 bootstrap")
}

private struct RustBootstrapRecv {
    let status: UInt8
    let peerId: UInt32
    let payload: [UInt8]
    let fds: [Int32]
}

private func cmsgAlign(_ value: Int) -> Int {
    #if os(macOS)
    let alignment = 4
    #else
    let alignment = MemoryLayout<size_t>.size
    #endif
    return (value + alignment - 1) & ~(alignment - 1)
}

private func cmsgSpace(_ dataLen: Int) -> Int {
    cmsgAlign(MemoryLayout<cmsghdr>.size) + cmsgAlign(dataLen)
}

private func cmsgLen(_ dataLen: Int) -> Int {
    cmsgAlign(MemoryLayout<cmsghdr>.size) + dataLen
}

private func requestShmBootstrapTicketRust(controlSocketPath: String) throws -> ShmBootstrapTicket? {
    #if os(Linux)
    let fd = socket(AF_UNIX, Int32(SOCK_STREAM.rawValue), 0)
    #else
    let fd = socket(AF_UNIX, SOCK_STREAM, 0)
    #endif
    guard fd >= 0 else {
        throw ShmBootstrapError.socketCreateFailed(errno: errno)
    }

    do {
        try connectUnixSocket(fd: fd, path: controlSocketPath)
        try writeAll(fd: fd, bytes: [UInt8]("VSH1".utf8))

        let recv: RustBootstrapRecv
        do {
            recv = try recvRustBootstrapResponse(fd: fd)
        } catch {
            close(fd)
            return nil
        }

        if recv.status == shmBootstrapStatusError {
            let msg = String(bytes: recv.payload, encoding: .utf8) ?? "bootstrap error"
            closeRustFds(recv.fds)
            throw ShmBootstrapError.protocolError(msg)
        }
        if recv.status != shmBootstrapStatusOK {
            closeRustFds(recv.fds)
            throw ShmBootstrapError.protocolError("unknown bootstrap status \(recv.status)")
        }
        guard recv.peerId != 0, recv.peerId <= UInt32(UInt8.max) else {
            closeRustFds(recv.fds)
            throw ShmBootstrapError.protocolError("invalid peer_id \(recv.peerId)")
        }
        guard let hubPath = String(bytes: recv.payload, encoding: .utf8) else {
            closeRustFds(recv.fds)
            throw ShmBootstrapError.protocolError("hub path not utf-8")
        }
        guard recv.fds.count == 4 || recv.fds.count == 3 else {
            closeRustFds(recv.fds)
            throw ShmBootstrapError.protocolError("invalid fd count \(recv.fds.count)")
        }

        let doorbellFd = recv.fds[0]
        let shmFd = recv.fds[1]
        let mmapControlFd = recv.fds[2]
        if recv.fds.count > 3 {
            // Swift runtime currently uses one mmap control fd.
            close(recv.fds[3])
        }

        close(fd)
        return ShmBootstrapTicket(
            peerId: UInt8(recv.peerId),
            hubPath: hubPath,
            doorbellFd: doorbellFd,
            shmFd: shmFd,
            mmapControlFd: mmapControlFd
        )
    } catch {
        close(fd)
        throw error
    }
}

private func recvRustBootstrapResponse(fd: Int32) throws -> RustBootstrapRecv {
    let maxPayload = Int(UInt16.max)
    let maxFrameLen = 11 + maxPayload
    var frame = [UInt8](repeating: 0, count: maxFrameLen)

    let fdBytes = 4 * MemoryLayout<Int32>.size
    var control = [UInt8](repeating: 0, count: cmsgSpace(fdBytes))

    var msg = msghdr()
    var iov = iovec()

    let readCount = frame.withUnsafeMutableBytes { frameBuf in
        control.withUnsafeMutableBytes { controlBuf in
            iov.iov_base = frameBuf.baseAddress
            iov.iov_len = frameBuf.count
            msg.msg_name = nil
            msg.msg_namelen = 0
            msg.msg_iov = withUnsafeMutablePointer(to: &iov) { $0 }
            msg.msg_iovlen = 1
            msg.msg_control = controlBuf.baseAddress
            msg.msg_controllen = socklen_t(controlBuf.count)
            msg.msg_flags = 0
            return recvmsg(fd, &msg, 0)
        }
    }

    if readCount < 0 {
        throw ShmBootstrapError.readFailed(errno: errno)
    }
    if readCount == 0 {
        throw ShmBootstrapError.eof
    }

    let n = Int(readCount)
    let received = Array(frame.prefix(n))
    if received.count < 11 {
        throw ShmBootstrapError.protocolError("bootstrap response too short")
    }
    if received[0] != UInt8(ascii: "V"),
        received[1] != UInt8(ascii: "S"),
        received[2] != UInt8(ascii: "P"),
        received[3] != UInt8(ascii: "1")
    {
        throw ShmBootstrapError.protocolError("invalid bootstrap response magic")
    }

    let status = received[4]
    let p0 = UInt32(received[5])
    let p1 = UInt32(received[6]) << 8
    let p2 = UInt32(received[7]) << 16
    let p3 = UInt32(received[8]) << 24
    let peerId = p0 | p1 | p2 | p3
    let payloadLen = Int(UInt16(received[9]) | (UInt16(received[10]) << 8))
    let frameLen = 11 + payloadLen
    guard received.count == frameLen else {
        throw ShmBootstrapError.protocolError("bootstrap response length mismatch")
    }

    var fds: [Int32] = []
    if (msg.msg_flags & Int32(MSG_CTRUNC)) != 0 {
        throw ShmBootstrapError.protocolError("truncated bootstrap control message")
    }
    let msgControlLen = Int(msg.msg_controllen)
    let headerAlignedLen = cmsgAlign(MemoryLayout<cmsghdr>.size)
    var offset = 0
    while offset + MemoryLayout<cmsghdr>.size <= msgControlLen {
        let header = control.withUnsafeBytes { raw in
            raw.load(fromByteOffset: offset, as: cmsghdr.self)
        }
        let headerLen = Int(header.cmsg_len)
        if headerLen < cmsgLen(0) || offset + headerLen > msgControlLen {
            break
        }
        if header.cmsg_level == SOL_SOCKET && header.cmsg_type == SCM_RIGHTS {
            let dataStart = offset + headerAlignedLen
            let dataLen = headerLen - headerAlignedLen
            if dataLen >= MemoryLayout<Int32>.size && dataStart + dataLen <= control.count {
                let fdCount = dataLen / MemoryLayout<Int32>.size
                control.withUnsafeBytes { raw in
                    let dataPtr = raw.baseAddress!.advanced(by: dataStart).assumingMemoryBound(
                        to: Int32.self
                    )
                    for i in 0..<fdCount {
                        fds.append(dataPtr.advanced(by: i).pointee)
                    }
                }
            }
        }
        offset += cmsgAlign(headerLen)
    }

    return RustBootstrapRecv(
        status: status,
        peerId: peerId,
        payload: Array(received[11..<frameLen]),
        fds: fds
    )
}

private func closeRustFds(_ fds: [Int32]) {
    for fd in fds where fd >= 0 {
        close(fd)
    }
}

private func connectUnixSocket(fd: Int32, path: String) throws {
    #if os(macOS)
    let one: Int32 = 1
    let setNoSigPipeRc = withUnsafePointer(to: one) { ptr in
        setsockopt(fd, SOL_SOCKET, SO_NOSIGPIPE, ptr, socklen_t(MemoryLayout<Int32>.size))
    }
    if setNoSigPipeRc != 0 {
        throw ShmBootstrapError.connectFailed(errno: errno)
    }
    #endif

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

    var lastErr: Int32 = 0
    for attempt in 0..<100 {
        let result = withUnsafePointer(to: &addr) { ptr in
            ptr.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockPtr in
                connect(fd, sockPtr, socklen_t(MemoryLayout<sockaddr_un>.size))
            }
        }

        if result == 0 {
            return
        }

        lastErr = errno
        let shouldRetry = lastErr == ECONNREFUSED || lastErr == ENOENT
        if !shouldRetry || attempt == 99 {
            throw ShmBootstrapError.connectFailed(errno: lastErr)
        }

        usleep(5_000)
    }
}

private func writeAll(fd: Int32, bytes: [UInt8]) throws {
    var offset = 0
    while offset < bytes.count {
        let written = bytes.withUnsafeBytes { rawBuf in
            send(fd, rawBuf.baseAddress!.advanced(by: offset), bytes.count - offset, Int32(MSG_NOSIGNAL))
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

private func isValidSid(_ sid: String) -> Bool {
    if sid.count == 32 {
        return sid.unicodeScalars.allSatisfy {
            CharacterSet(charactersIn: "0123456789abcdefABCDEF").contains($0)
        }
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
