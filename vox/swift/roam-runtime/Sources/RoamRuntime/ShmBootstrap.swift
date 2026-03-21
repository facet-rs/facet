import Foundation
#if os(macOS) || os(Linux)
import CRoamShmFfi
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

        let sidBytes = [UInt8](sid.utf8)
        var request = [UInt8](
            repeating: 0,
            count: Int(roam_shm_bootstrap_request_header_size()) + sidBytes.count
        )
        var requestWritten: UInt = 0
        let encodeRc = request.withUnsafeMutableBufferPointer { reqBuf in
            sidBytes.withUnsafeBufferPointer { sidBuf in
                roam_shm_bootstrap_request_encode(
                    sidBuf.baseAddress,
                    UInt(sidBuf.count),
                    reqBuf.baseAddress,
                    UInt(reqBuf.count),
                    &requestWritten
                )
            }
        }
        guard encodeRc == 0 else {
            throw ShmBootstrapError.invalidRequestEncoding
        }

        try writeAll(fd: fd, bytes: Array(request.prefix(Int(requestWritten))))

        let recv = try recvBootstrapResponse(fd: fd)
        let status = recv.info.status

        switch status {
        case shmBootstrapStatusOK:
            guard recv.info.peer_id != 0 else {
                closeReceivedFds(recv)
                throw ShmBootstrapError.protocolError("invalid peer_id 0")
            }
            guard recv.info.peer_id <= UInt32(UInt8.max) else {
                closeReceivedFds(recv)
                throw ShmBootstrapError.protocolError("peer_id out of range")
            }
            guard let hubPath = String(bytes: recv.payload, encoding: .utf8) else {
                closeReceivedFds(recv)
                throw ShmBootstrapError.protocolError("hub path not utf-8")
            }
            guard recv.doorbellFd >= 0, recv.shmFd >= 0, recv.mmapControlFd >= 0 else {
                closeReceivedFds(recv)
                throw ShmBootstrapError.missingFileDescriptor
            }

            close(fd)
            return ShmBootstrapTicket(
                peerId: UInt8(recv.info.peer_id),
                hubPath: hubPath,
                doorbellFd: recv.doorbellFd,
                shmFd: recv.shmFd,
                mmapControlFd: recv.mmapControlFd
            )

        case shmBootstrapStatusError:
            closeReceivedFds(recv)
            let msg = String(bytes: recv.payload, encoding: .utf8) ?? "bootstrap error"
            throw ShmBootstrapError.protocolError(msg)

        default:
            closeReceivedFds(recv)
            throw ShmBootstrapError.protocolError("unknown bootstrap status \(status)")
        }
    } catch {
        close(fd)
        throw error
    }
}

private struct BootstrapRecv {
    let info: RoamShmBootstrapResponseInfo
    let payload: [UInt8]
    let doorbellFd: Int32
    let shmFd: Int32
    let mmapControlFd: Int32
}

private func recvBootstrapResponse(fd: Int32) throws -> BootstrapRecv {
    var payload = [UInt8](repeating: 0, count: Int(UInt16.max))
    var info = RoamShmBootstrapResponseInfo(status: 0, peer_id: 0, payload_len: 0)
    var doorbellFd: Int32 = -1
    var shmFd: Int32 = -1
    var mmapControlFd: Int32 = -1

    let rc = payload.withUnsafeMutableBufferPointer { buf in
        withUnsafeMutablePointer(to: &info) { infoPtr in
            roam_shm_bootstrap_response_recv_unix(
                fd,
                buf.baseAddress,
                UInt(buf.count),
                infoPtr,
                &doorbellFd,
                &shmFd,
                &mmapControlFd
            )
        }
    }

    switch rc {
    case 0:
        let payloadLen = Int(info.payload_len)
        if payloadLen > payload.count {
            throw ShmBootstrapError.protocolError("invalid payload length")
        }
        return BootstrapRecv(
            info: info,
            payload: Array(payload.prefix(payloadLen)),
            doorbellFd: doorbellFd,
            shmFd: shmFd,
            mmapControlFd: mmapControlFd
        )
    case -2:
        throw ShmBootstrapError.protocolError("bootstrap payload too large")
    default:
        throw ShmBootstrapError.protocolError("failed to receive bootstrap response")
    }
}

private func closeReceivedFds(_ recv: BootstrapRecv) {
    if recv.doorbellFd >= 0 {
        close(recv.doorbellFd)
    }
    if recv.shmFd >= 0 {
        close(recv.shmFd)
    }
    if recv.mmapControlFd >= 0 {
        close(recv.mmapControlFd)
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
