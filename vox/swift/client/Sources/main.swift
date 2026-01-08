// Swift TCP client for cross-language testing.
//
// Connects to a TCP server, performs Hello exchange, and makes RPC calls.
// Used to test Swift client against servers implemented in other languages.

import Foundation
import RoamRuntime

// MARK: - Error types

enum ClientError: Error {
    case connectionFailed(String)
    case protocolError(String)
    case timeout
}

// MARK: - TCP Connection

class TcpConnection {
    private let fd: Int32
    private var recvBuf = Data()
    private var nextRequestId: UInt64 = 1
    private var negotiatedMaxPayload: UInt32 = 1024 * 1024

    init(host: String, port: UInt16) throws {
        fd = socket(AF_INET, SOCK_STREAM, 0)
        guard fd >= 0 else {
            throw ClientError.connectionFailed("socket() failed")
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
        guard rc == 0 else {
            close(fd)
            throw ClientError.connectionFailed("connect() failed to \(host):\(port)")
        }

        // Do Hello exchange
        try doHello()
    }

    deinit {
        close(fd)
    }

    private func writeAll(_ bytes: [UInt8]) throws {
        var sent = 0
        while sent < bytes.count {
            let rc = bytes.withUnsafeBytes { raw in
                write(fd, raw.baseAddress!.advanced(by: sent), bytes.count - sent)
            }
            guard rc > 0 else {
                throw ClientError.connectionFailed("write failed")
            }
            sent += rc
        }
    }

    private func sendFrame(_ payload: [UInt8]) throws {
        var framed = cobsEncode(payload)
        framed.append(0)
        try writeAll(framed)
    }

    private func recvFrame() throws -> [UInt8] {
        var tmp = [UInt8](repeating: 0, count: 4096)

        while true {
            if let zeroIdx = recvBuf.firstIndex(of: 0) {
                let frame = recvBuf.prefix(upTo: zeroIdx)
                recvBuf.removeSubrange(..<recvBuf.index(after: zeroIdx))

                if frame.isEmpty { continue }
                return try cobsDecode(Array(frame))
            }

            let n = read(fd, &tmp, tmp.count)
            guard n > 0 else {
                throw ClientError.connectionFailed("connection closed")
            }
            recvBuf.append(contentsOf: tmp[0..<n])
        }
    }

    private func doHello() throws {
        let localMaxPayload: UInt32 = 1024 * 1024
        let localInitialCredit: UInt32 = 64 * 1024

        // Send our Hello
        var payload: [UInt8] = []
        payload += encodeVarint(0)  // Message::Hello
        payload += encodeVarint(0)  // Hello::V1
        payload += encodeVarint(UInt64(localMaxPayload))
        payload += encodeVarint(UInt64(localInitialCredit))
        try sendFrame(payload)

        // Receive Hello from server
        let msg = try recvFrame()
        let data = Data(msg)
        var o = 0
        let msgDisc = try decodeVarint(from: data, offset: &o)
        guard msgDisc == 0 else {
            throw ClientError.protocolError("expected Hello message")
        }

        let helloDisc = try decodeVarint(from: data, offset: &o)
        guard helloDisc == 0 else {
            throw ClientError.protocolError("unsupported Hello version")
        }

        let remoteMax = try decodeVarintU32(from: data, offset: &o)
        negotiatedMaxPayload = min(localMaxPayload, remoteMax)
    }

    func call(methodId: UInt64, payload: [UInt8]) throws -> Data {
        let reqId = nextRequestId
        nextRequestId += 1

        // Build Request message
        var msg: [UInt8] = []
        msg += encodeVarint(2)  // Message::Request
        msg += encodeVarint(reqId)
        msg += encodeVarint(methodId)
        msg += encodeVarint(0)  // metadata length = 0
        msg += encodeBytes(payload)

        try sendFrame(msg)

        // Read Response
        while true {
            let resp = try recvFrame()
            let data = Data(resp)
            var o = 0

            let msgDisc = try decodeVarint(from: data, offset: &o)

            guard msgDisc == 3 else {
                // Not a Response, keep reading
                continue
            }

            let respId = try decodeVarint(from: data, offset: &o)
            guard respId == reqId else {
                // Not our response
                continue
            }

            // Skip metadata
            let mdLen = try decodeVarint(from: data, offset: &o)
            for _ in 0..<mdLen {
                let kLen = Int(try decodeVarint(from: data, offset: &o))
                o += kLen
                let vDisc = try decodeVarint(from: data, offset: &o)
                if vDisc == 0 {
                    let sLen = Int(try decodeVarint(from: data, offset: &o))
                    o += sLen
                } else if vDisc == 1 {
                    let bLen = Int(try decodeVarint(from: data, offset: &o))
                    o += bLen
                } else if vDisc == 2 {
                    _ = try decodeVarint(from: data, offset: &o)
                }
            }

            // Read payload
            let pLen = Int(try decodeVarint(from: data, offset: &o))
            return data.subdata(in: o..<(o + pLen))
        }
    }
}

// MARK: - Echo Client

struct EchoClientImpl {
    let conn: TcpConnection

    func echo(message: String) throws -> String {
        let payload = encodeString(message)
        let response = try conn.call(methodId: 0x3d66_dd9e_e36b_4240, payload: payload)

        var o = 0
        let resultTag = try decodeVarint(from: response, offset: &o)
        guard resultTag == 0 else {
            throw ClientError.protocolError("RPC returned error")
        }
        return try decodeString(from: response, offset: &o)
    }

    func reverse(message: String) throws -> String {
        let payload = encodeString(message)
        let response = try conn.call(methodId: 0x2682_46d3_2195_03fb, payload: payload)

        var o = 0
        let resultTag = try decodeVarint(from: response, offset: &o)
        guard resultTag == 0 else {
            throw ClientError.protocolError("RPC returned error")
        }
        return try decodeString(from: response, offset: &o)
    }
}

// MARK: - Main

func main() throws {
    let serverAddr = ProcessInfo.processInfo.environment["SERVER_ADDR"] ?? "127.0.0.1:9001"
    guard let idx = serverAddr.lastIndex(of: ":") else {
        fputs("Invalid SERVER_ADDR: \(serverAddr)\n", stderr)
        exit(1)
    }
    let host = String(serverAddr[..<idx])
    let portStr = String(serverAddr[serverAddr.index(after: idx)...])
    guard let port = UInt16(portStr) else {
        fputs("Invalid port in SERVER_ADDR: \(serverAddr)\n", stderr)
        exit(1)
    }

    fputs("Connecting to \(serverAddr)...\n", stderr)

    let conn = try TcpConnection(host: host, port: port)

    fputs("Connected! Running tests...\n", stderr)

    let client = EchoClientImpl(conn: conn)

    // Test Echo
    var result = try client.echo(message: "Hello, World!")
    guard result == "Hello, World!" else {
        fputs("Echo mismatch: got \"\(result)\", want \"Hello, World!\"\n", stderr)
        exit(1)
    }
    fputs("Echo: PASS\n", stderr)

    // Test Reverse
    result = try client.reverse(message: "Hello")
    guard result == "olleH" else {
        fputs("Reverse mismatch: got \"\(result)\", want \"olleH\"\n", stderr)
        exit(1)
    }
    fputs("Reverse: PASS\n", stderr)

    // Test with unicode
    result = try client.echo(message: "Hello, World! 🎉")
    guard result == "Hello, World! 🎉" else {
        fputs("Echo unicode mismatch: got \"\(result)\", want \"Hello, World! 🎉\"\n", stderr)
        exit(1)
    }
    fputs("Echo unicode: PASS\n", stderr)

    // Test Reverse with unicode
    result = try client.reverse(message: "日本語")
    guard result == "語本日" else {
        fputs("Reverse unicode mismatch: got \"\(result)\", want \"語本日\"\n", stderr)
        exit(1)
    }
    fputs("Reverse unicode: PASS\n", stderr)

    print("All tests passed!")
}

do {
    try main()
} catch {
    fputs("Error: \(error)\n", stderr)
    exit(1)
}
