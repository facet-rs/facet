#if os(macOS)
import Darwin
import Foundation
import Testing

@testable import RoamRuntime

private struct BootstrapServerHandle {
    let socketPath: String
    let thread: Thread
}

private func makeUnixListener(path: String) throws -> Int32 {
    unlink(path)

    let fd = socket(AF_UNIX, SOCK_STREAM, 0)
    guard fd >= 0 else {
        throw ShmBootstrapError.socketCreateFailed(errno: errno)
    }

    var addr = sockaddr_un()
    addr.sun_family = sa_family_t(AF_UNIX)

    let pathBytes = [UInt8](path.utf8)
    let maxPathLen = MemoryLayout.size(ofValue: addr.sun_path)
    guard pathBytes.count < maxPathLen else {
        close(fd)
        throw ShmBootstrapError.invalidSocketPath
    }

    withUnsafeMutablePointer(to: &addr.sun_path) { sunPathPtr in
        let raw = UnsafeMutableRawPointer(sunPathPtr)
        raw.initializeMemory(as: UInt8.self, repeating: 0, count: maxPathLen)
        raw.copyMemory(from: pathBytes, byteCount: pathBytes.count)
    }

    let bindResult = withUnsafePointer(to: &addr) { ptr in
        ptr.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockPtr in
            bind(fd, sockPtr, socklen_t(MemoryLayout<sockaddr_un>.size))
        }
    }
    guard bindResult == 0 else {
        let err = errno
        close(fd)
        throw ShmBootstrapError.connectFailed(errno: err)
    }

    guard listen(fd, 1) == 0 else {
        let err = errno
        close(fd)
        throw ShmBootstrapError.connectFailed(errno: err)
    }

    return fd
}

private func sendPassedFd(sock: Int32, fd: Int32) -> Bool {
    var payload = [UInt8](repeating: 1, count: 1)
    var iov = iovec(iov_base: nil, iov_len: 1)

    let controlLen = cmsgSpace(MemoryLayout<Int32>.size)
    var control = [UInt8](repeating: 0, count: controlLen)

    return payload.withUnsafeMutableBytes { payloadBuf in
        iov.iov_base = payloadBuf.baseAddress
        return withUnsafeMutablePointer(to: &iov) { iovPtr in
            control.withUnsafeMutableBytes { raw in
                guard let base = raw.baseAddress else {
                    return false
                }

                let cmsg = base.assumingMemoryBound(to: cmsghdr.self)
                cmsg.pointee.cmsg_level = SOL_SOCKET
                cmsg.pointee.cmsg_type = SCM_RIGHTS
                cmsg.pointee.cmsg_len = socklen_t(cmsgLen(MemoryLayout<Int32>.size))

                base
                    .advanced(by: cmsgDataOffset())
                    .assumingMemoryBound(to: Int32.self)
                    .pointee = fd

                var msg = msghdr()
                msg.msg_iov = iovPtr
                msg.msg_iovlen = 1
                msg.msg_control = base
                msg.msg_controllen = socklen_t(raw.count)

                let sent = sendmsg(sock, &msg, 0)
                return sent == 1
            }
        }
    }
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

private func writeAll(fd: Int32, bytes: [UInt8]) -> Bool {
    var offset = 0
    while offset < bytes.count {
        let written = bytes.withUnsafeBytes { raw in
            write(fd, raw.baseAddress!.advanced(by: offset), bytes.count - offset)
        }
        if written < 0 {
            if errno == EINTR {
                continue
            }
            return false
        }
        if written == 0 {
            return false
        }
        offset += written
    }
    return true
}

private func readExactly(fd: Int32, count: Int) -> [UInt8]? {
    if count == 0 {
        return []
    }

    var out = [UInt8](repeating: 0, count: count)
    var offset = 0
    while offset < count {
        let n = out.withUnsafeMutableBytes { raw in
            read(fd, raw.baseAddress!.advanced(by: offset), count - offset)
        }
        if n < 0 {
            if errno == EINTR {
                continue
            }
            return nil
        }
        if n == 0 {
            return nil
        }
        offset += n
    }
    return out
}

private func startBootstrapServer(
    expectedSid: String,
    responseStatus: UInt8,
    responsePeerId: UInt8,
    responsePayload: String,
    sendFd: Int32?
) throws -> BootstrapServerHandle {
    let tmp = try XCTUnwrap(tempfile())
    let socketPath = tmp + "/control.sock"

    let listener = try makeUnixListener(path: socketPath)

    let thread = Thread {
        defer {
            close(listener)
            unlink(socketPath)
            removeTempDir(tmp)
        }

        let client = accept(listener, nil, nil)
        guard client >= 0 else {
            return
        }
        defer { close(client) }

        guard let magic = readExactly(fd: client, count: 4), magic == [UInt8]("RSH0".utf8) else {
            return
        }
        guard let sidLenBytes = readExactly(fd: client, count: 2) else { return }
        let sidLen = Int(UInt16(sidLenBytes[0]) | (UInt16(sidLenBytes[1]) << 8))
        guard let sidBytes = readExactly(fd: client, count: sidLen),
            let sid = String(bytes: sidBytes, encoding: .utf8), sid == expectedSid
        else {
            return
        }

        let payloadBytes = [UInt8](responsePayload.utf8)
        var response: [UInt8] = []
        response.append(contentsOf: [UInt8]("RSP0".utf8))
        response.append(responseStatus)
        response.append(responsePeerId)
        response.append(UInt8(truncatingIfNeeded: payloadBytes.count & 0x00FF))
        response.append(UInt8(truncatingIfNeeded: (payloadBytes.count >> 8) & 0x00FF))
        response.append(contentsOf: payloadBytes)

        guard writeAll(fd: client, bytes: response) else {
            return
        }

        if let fd = sendFd {
            _ = sendPassedFd(sock: client, fd: fd)
        }
    }
    thread.start()

    return BootstrapServerHandle(socketPath: socketPath, thread: thread)
}

private func tempfile() throws -> String {
    let template = "/tmp/roam-swift-bootstrap.XXXXXX"
    var bytes = Array(template.utf8CString)
    let fd = mkdtemp(&bytes)
    guard fd != nil else {
        throw POSIXError(.EIO)
    }
    let trimmed = bytes.prefix { $0 != 0 }.map(UInt8.init(bitPattern:))
    return String(decoding: trimmed, as: UTF8.self)
}

private func removeTempDir(_ path: String) {
    let fm = FileManager.default
    try? fm.removeItem(atPath: path)
}

private func XCTUnwrap<T>(_ value: T?) throws -> T {
    if let value {
        return value
    }
    throw POSIXError(.EINVAL)
}

struct ShmBootstrapTests {
    @Test func receivesDoorbellFdViaScmRights() throws {
        let sid = "123e4567-e89b-12d3-a456-426614174000"

        let file = open("/dev/null", O_RDONLY)
        #expect(file >= 0)
        defer { close(file) }

        let server = try startBootstrapServer(
            expectedSid: sid,
            responseStatus: 0,
            responsePeerId: 1,
            responsePayload: "/tmp/test.shm",
            sendFd: file
        )
        defer { server.thread.cancel() }

        let ticket = try requestShmBootstrapTicket(controlSocketPath: server.socketPath, sid: sid)

        #expect(ticket.peerId == 1)
        #expect(ticket.hubPath == "/tmp/test.shm")

        let flags = fcntl(ticket.doorbellFd, F_GETFD)
        #expect(flags != -1)
        close(ticket.doorbellFd)
    }

    @Test func failsWhenNoFdIsPassed() throws {
        let sid = "123e4567-e89b-12d3-a456-426614174000"

        let server = try startBootstrapServer(
            expectedSid: sid,
            responseStatus: 0,
            responsePeerId: 1,
            responsePayload: "/tmp/test.shm",
            sendFd: nil
        )
        defer { server.thread.cancel() }

        do {
            _ = try requestShmBootstrapTicket(controlSocketPath: server.socketPath, sid: sid)
            Issue.record("Expected missingFileDescriptor error")
        } catch let error as ShmBootstrapError {
            switch error {
            case .missingFileDescriptor:
                break
            default:
                Issue.record("Unexpected error: \(error)")
            }
        }
    }

    @Test func surfacesServerErrorPayload() throws {
        let sid = "123e4567-e89b-12d3-a456-426614174000"

        let server = try startBootstrapServer(
            expectedSid: sid,
            responseStatus: 1,
            responsePeerId: 0,
            responsePayload: "sid mismatch",
            sendFd: nil
        )
        defer { server.thread.cancel() }

        do {
            _ = try requestShmBootstrapTicket(controlSocketPath: server.socketPath, sid: sid)
            Issue.record("Expected protocolError")
        } catch let error as ShmBootstrapError {
            switch error {
            case .protocolError(let msg):
                #expect(msg == "sid mismatch")
            default:
                Issue.record("Unexpected error: \(error)")
            }
        }
    }
}
#endif
