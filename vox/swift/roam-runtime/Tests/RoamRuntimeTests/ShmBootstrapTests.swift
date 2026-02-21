#if os(macOS)
import Darwin
import Foundation
import Testing
import CRoamShm

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

private func sendPassedFds(sock: Int32, fds: [Int32]) -> Bool {
    guard !fds.isEmpty else { return true }
    return fds.withUnsafeBufferPointer { fdsBuf in
        guard let base = fdsBuf.baseAddress else {
            return false
        }
        let rc = roam_send_fds(sock, base, Int32(fdsBuf.count))
        return rc > 0
    }
}

private func cmsgAlign(_ n: Int) -> Int {
    let align = MemoryLayout<Int>.size
    return (n + align - 1) & ~(align - 1)
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
    sendDoorbellFd: Int32?,
    sendShmFd: Int32?
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

        var client: Int32
        while true {
            client = accept(listener, nil, nil)
            if client >= 0 { break }
            if errno == EINTR { continue }
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

        var fdsToSend: [Int32] = []
        if let fd = sendDoorbellFd {
            fdsToSend.append(fd)
        }
        if let fd = sendShmFd {
            fdsToSend.append(fd)
        }
        if !fdsToSend.isEmpty {
            _ = sendPassedFds(sock: client, fds: fdsToSend)
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
        for _ in 0..<100 {
            let sid = "123e4567-e89b-12d3-a456-426614174000"

            let file = open("/dev/null", O_RDONLY)
            #expect(file >= 0)
            defer { close(file) }
            let shmFile = open("/dev/null", O_RDONLY)
            #expect(shmFile >= 0)
            defer { close(shmFile) }

            let server = try startBootstrapServer(
                expectedSid: sid,
                responseStatus: 0,
                responsePeerId: 1,
                responsePayload: "/tmp/test.shm",
                sendDoorbellFd: file,
                sendShmFd: shmFile
            )
            defer { server.thread.cancel() }

            let ticket = try requestShmBootstrapTicket(controlSocketPath: server.socketPath, sid: sid)

            #expect(ticket.peerId == 1)
            #expect(ticket.hubPath == "/tmp/test.shm")

            let flags = fcntl(ticket.doorbellFd, F_GETFD)
            #expect(flags != -1)
            let shmFlags = fcntl(ticket.shmFd, F_GETFD)
            #expect(shmFlags != -1)
            close(ticket.doorbellFd)
            close(ticket.shmFd)
        }
    }

    @Test func failsWhenNoFdIsPassed() throws {
        let sid = "123e4567-e89b-12d3-a456-426614174000"

        let server = try startBootstrapServer(
            expectedSid: sid,
            responseStatus: 0,
            responsePeerId: 1,
            responsePayload: "/tmp/test.shm",
            sendDoorbellFd: nil,
            sendShmFd: nil
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
            sendDoorbellFd: nil,
            sendShmFd: nil
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
