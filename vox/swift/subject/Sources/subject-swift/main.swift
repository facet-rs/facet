import Foundation
import RoamRuntime

// Import generated Echo service
// Note: This will be in the same module since we're compiling together

// MARK: - Echo Handler Implementation

struct EchoService: EchoHandler {
    func echo(message: String) async throws -> String {
        return message
    }

    func reverse(message: String) async throws -> String {
        return String(message.reversed())
    }
}

// MARK: - Main

enum Fatal: Error {
    case message(String)
}

func fatal(_ message: String) -> Never {
    fputs(message + "\n", stderr)
    exit(1)
}

func writeAll(fd: Int32, bytes: [UInt8]) throws {
    var sent = 0
    while sent < bytes.count {
        let rc = bytes.withUnsafeBytes { raw in
            write(fd, raw.baseAddress!.advanced(by: sent), bytes.count - sent)
        }
        if rc <= 0 {
            throw Fatal.message("write failed")
        }
        sent += rc
    }
}

func connectTcp(host: String, port: UInt16) -> Int32 {
    let fd = socket(AF_INET, SOCK_STREAM, 0)
    if fd < 0 {
        fatal("socket() failed")
    }

    var addr = sockaddr_in()
    addr.sin_family = sa_family_t(AF_INET)
    addr.sin_port = port.bigEndian
    addr.sin_addr = in_addr(s_addr: inet_addr(host))

    var sa = sockaddr()
    memcpy(&sa, &addr, MemoryLayout<sockaddr_in>.size)
    let rc = withUnsafePointer(to: &sa) {
        $0.withMemoryRebound(to: sockaddr.self, capacity: 1) { ptr in
            connect(fd, ptr, socklen_t(MemoryLayout<sockaddr_in>.size))
        }
    }
    if rc != 0 {
        close(fd)
        fatal("connect() failed to \(host):\(port)")
    }

    return fd
}

func sendGoodbye(fd: Int32, reason: String) -> Never {
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
    close(fd)
    exit(0)
}

// Parse PEER_ADDR environment variable
let peerAddr = ProcessInfo.processInfo.environment["PEER_ADDR"]
guard let peerAddr else { fatal("PEER_ADDR is not set") }
guard let idx = peerAddr.lastIndex(of: ":") else { fatal("Invalid PEER_ADDR \(peerAddr)") }
let host = String(peerAddr[..<idx])
let portStr = String(peerAddr[peerAddr.index(after: idx)...])
guard let portNum = UInt16(portStr) else { fatal("Invalid port in PEER_ADDR \(peerAddr)") }

let fd = connectTcp(host: host, port: portNum)

// Create Echo service and dispatcher
let echoService = EchoService()
let dispatcher = createEchoDispatcher(handler: echoService)

let localMaxPayload: UInt32 = 1024 * 1024
let localInitialCredit: UInt32 = 64 * 1024
var negotiatedMaxPayload: UInt32 = localMaxPayload
var haveReceivedHello = false

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
    fatal("failed to send hello")
}

var recvBuf = Data()
var tmp = [UInt8](repeating: 0, count: 4096)

// Main message loop
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
            sendGoodbye(fd: fd, reason: "message.decode-error")
        }

        do {
            let payload = Data(decoded)
            var o = 0
            let msgDisc = try decodeVarint(from: payload, offset: &o)

            // Handle Hello
            if msgDisc == 0 {
                let helloDisc = try decodeVarint(from: payload, offset: &o)
                if helloDisc != 0 {
                    sendGoodbye(fd: fd, reason: "message.hello.unknown-version")
                }
                let remoteMax = try decodeVarintU32(from: payload, offset: &o)
                _ = try decodeVarintU32(from: payload, offset: &o)  // initial_stream_credit
                negotiatedMaxPayload = min(localMaxPayload, remoteMax)
                haveReceivedHello = true
                continue
            }

            if !haveReceivedHello {
                continue
            }

            // Handle Request
            if msgDisc == 2 {
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
                    } else {
                        throw Fatal.message("unknown MetadataValue")
                    }
                }

                let pLen = try decodeVarint(from: payload, offset: &o)
                if pLen > UInt64(negotiatedMaxPayload) {
                    sendGoodbye(fd: fd, reason: "flow.unary.payload-limit")
                }

                // Extract request payload
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

                continue
            }

            // Handle stream messages (Data=5, Close=6, Reset=7, Credit=8)
            // r[impl streaming.id.zero-reserved] - stream_id 0 is reserved
            // r[impl streaming.unknown] - unknown stream_id triggers Goodbye
            if msgDisc == 5 || msgDisc == 6 || msgDisc == 7 || msgDisc == 8 {
                let streamId = try decodeVarint(from: payload, offset: &o)
                if streamId == 0 {
                    sendGoodbye(fd: fd, reason: "streaming.id.zero-reserved")
                }
                // For Echo service (unary only), all stream IDs are unknown
                // since we never open any streams
                sendGoodbye(fd: fd, reason: "streaming.unknown")
            }

        } catch {
            sendGoodbye(fd: fd, reason: "message.decode-error")
        }
    }
}

close(fd)
exit(0)
