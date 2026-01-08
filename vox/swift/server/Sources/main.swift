// Swift TCP server for cross-language testing.
//
// Listens on a TCP port and handles Echo service requests.
// Used to test clients in other languages against a Swift server.

import Foundation
import RoamRuntime

// MARK: - Error types

enum ServerError: Error {
    case listenFailed(String)
    case acceptFailed(String)
    case protocolError(String)
}

// MARK: - Echo Handler

struct EchoService: EchoHandler {
    func echo(message: String) async throws -> String {
        return message
    }

    func reverse(message: String) async throws -> String {
        return String(message.reversed())
    }
}

// MARK: - Connection Handler

func handleConnection(fd: Int32) async {
    defer { close(fd) }

    let localMaxPayload: UInt32 = 1024 * 1024
    let localInitialCredit: UInt32 = 64 * 1024
    var negotiatedMaxPayload: UInt32 = localMaxPayload
    var haveReceivedHello = false

    let dispatcher = createEchoDispatcher(handler: EchoService())

    // Send Hello
    do {
        var payload: [UInt8] = []
        payload += encodeVarint(0)  // Message::Hello
        payload += encodeVarint(0)  // Hello::V1
        payload += encodeVarint(UInt64(localMaxPayload))
        payload += encodeVarint(UInt64(localInitialCredit))
        var framed = cobsEncode(payload)
        framed.append(0)
        try writeAll(fd: fd, bytes: framed)
    } catch {
        fputs("Failed to send hello: \(error)\n", stderr)
        return
    }

    var recvBuf = Data()
    var tmp = [UInt8](repeating: 0, count: 4096)

    while true {
        let n = read(fd, &tmp, tmp.count)
        if n <= 0 {
            break
        }
        recvBuf.append(contentsOf: tmp[0..<n])

        while let zeroIdx = recvBuf.firstIndex(of: 0) {
            let frame = recvBuf.prefix(upTo: zeroIdx)
            recvBuf.removeSubrange(..<recvBuf.index(after: zeroIdx))

            if frame.isEmpty { continue }

            let decoded: [UInt8]
            do {
                decoded = try cobsDecode(Array(frame))
            } catch {
                _ = sendGoodbye(fd: fd, reason: "message.decode-error")
                return
            }

            do {
                let shouldContinue = try await processMessage(
                    fd: fd,
                    payload: Data(decoded),
                    dispatcher: dispatcher,
                    negotiatedMaxPayload: &negotiatedMaxPayload,
                    haveReceivedHello: &haveReceivedHello
                )
                if !shouldContinue {
                    return
                }
            } catch {
                _ = sendGoodbye(fd: fd, reason: "message.decode-error")
                return
            }
        }
    }
}

func processMessage(
    fd: Int32,
    payload: Data,
    dispatcher: @escaping (UInt64, Data) async throws -> Data,
    negotiatedMaxPayload: inout UInt32,
    haveReceivedHello: inout Bool
) async throws -> Bool {
    var o = 0
    let msgDisc = try decodeVarint(from: payload, offset: &o)

    switch msgDisc {
    case 0: // Hello
        let helloDisc = try decodeVarint(from: payload, offset: &o)
        if helloDisc != 0 {
            _ = sendGoodbye(fd: fd, reason: "message.hello.unknown-version")
            return false
        }
        let remoteMax = try decodeVarintU32(from: payload, offset: &o)
        _ = try decodeVarintU32(from: payload, offset: &o)  // initial_stream_credit
        negotiatedMaxPayload = min(negotiatedMaxPayload, remoteMax)
        haveReceivedHello = true
        return true

    case 1: // Goodbye
        return false

    case 2: // Request
        if !haveReceivedHello {
            return true
        }

        let requestId = try decodeVarint(from: payload, offset: &o)
        let methodId = try decodeVarint(from: payload, offset: &o)

        // Skip metadata
        let mdLen = try decodeVarint(from: payload, offset: &o)
        for _ in 0..<mdLen {
            let kLen = Int(try decodeVarint(from: payload, offset: &o))
            o += kLen
            let vDisc = try decodeVarint(from: payload, offset: &o)
            if vDisc == 0 {
                let sLen = Int(try decodeVarint(from: payload, offset: &o))
                o += sLen
            } else if vDisc == 1 {
                let bLen = Int(try decodeVarint(from: payload, offset: &o))
                o += bLen
            } else if vDisc == 2 {
                _ = try decodeVarint(from: payload, offset: &o)
            }
        }

        let pLen = try decodeVarint(from: payload, offset: &o)
        if pLen > UInt64(negotiatedMaxPayload) {
            _ = sendGoodbye(fd: fd, reason: "flow.unary.payload-limit")
            return false
        }

        let requestPayload = payload.subdata(in: o..<payload.count)

        // Dispatch the call
        let responsePayload = try await dispatcher(methodId, requestPayload)

        // Send Response
        var respMsg: [UInt8] = []
        respMsg += encodeVarint(3)  // Message::Response
        respMsg += encodeVarint(requestId)
        respMsg += encodeVarint(0)  // metadata length = 0
        respMsg += encodeBytes(Array(responsePayload))
        var framedResp = cobsEncode(respMsg)
        framedResp.append(0)
        try writeAll(fd: fd, bytes: framedResp)
        return true

    case 5, 6, 7, 8: // Data, Close, Reset, Credit
        if !haveReceivedHello {
            return true
        }
        let streamId = try decodeVarint(from: payload, offset: &o)
        if streamId == 0 {
            _ = sendGoodbye(fd: fd, reason: "streaming.id.zero-reserved")
            return false
        }
        _ = sendGoodbye(fd: fd, reason: "streaming.unknown")
        return false

    default:
        return true
    }
}

func writeAll(fd: Int32, bytes: [UInt8]) throws {
    var sent = 0
    while sent < bytes.count {
        let rc = bytes.withUnsafeBytes { raw in
            write(fd, raw.baseAddress!.advanced(by: sent), bytes.count - sent)
        }
        if rc <= 0 {
            throw ServerError.listenFailed("write failed")
        }
        sent += rc
    }
}

func sendGoodbye(fd: Int32, reason: String) -> Bool {
    do {
        var payload: [UInt8] = []
        payload += encodeVarint(1)  // Message::Goodbye
        payload += encodeString(reason)
        var framed = cobsEncode(payload)
        framed.append(0)
        try writeAll(fd: fd, bytes: framed)
    } catch {
        // ignore
    }
    return false
}

// MARK: - Main

let portStr = ProcessInfo.processInfo.environment["TCP_PORT"] ?? "9030"
guard let port = UInt16(portStr) else {
    fputs("Invalid TCP_PORT: \(portStr)\n", stderr)
    exit(1)
}

let serverFd = socket(AF_INET, SOCK_STREAM, 0)
guard serverFd >= 0 else {
    fputs("socket() failed\n", stderr)
    exit(1)
}

var opt: Int32 = 1
setsockopt(serverFd, SOL_SOCKET, SO_REUSEADDR, &opt, socklen_t(MemoryLayout<Int32>.size))

var addr = sockaddr_in()
addr.sin_family = sa_family_t(AF_INET)
addr.sin_port = port.bigEndian
addr.sin_addr = in_addr(s_addr: inet_addr("127.0.0.1"))

var sa = sockaddr()
memcpy(&sa, &addr, MemoryLayout<sockaddr_in>.size)
let bindRc = withUnsafePointer(to: &sa) {
    $0.withMemoryRebound(to: sockaddr.self, capacity: 1) { ptr in
        bind(serverFd, ptr, socklen_t(MemoryLayout<sockaddr_in>.size))
    }
}
guard bindRc == 0 else {
    fputs("bind() failed\n", stderr)
    exit(1)
}

guard listen(serverFd, 5) == 0 else {
    fputs("listen() failed\n", stderr)
    exit(1)
}

fputs("Swift TCP server listening on 127.0.0.1:\(port)\n", stderr)
print(port)  // For test harness
fflush(stdout)

// Use a RunLoop to allow async tasks to run
let semaphore = DispatchSemaphore(value: 0)

DispatchQueue.global().async {
    while true {
        var clientAddr = sockaddr()
        var clientAddrLen = socklen_t(MemoryLayout<sockaddr>.size)
        let clientFd = accept(serverFd, &clientAddr, &clientAddrLen)
        guard clientFd >= 0 else {
            fputs("accept() failed\n", stderr)
            continue
        }

        fputs("New connection\n", stderr)

        // Handle connection synchronously for simplicity
        let group = DispatchGroup()
        group.enter()
        Task {
            await handleConnection(fd: clientFd)
            fputs("Connection closed\n", stderr)
            group.leave()
        }
        group.wait()
    }
}

// Keep main thread alive
dispatchMain()
