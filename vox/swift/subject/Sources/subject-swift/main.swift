import Foundation

enum Fatal: Error {
    case message(String)
}

func fatal(_ message: String) -> Never {
    fputs(message + "\n", stderr)
    exit(1)
}

// MARK: - Varint (LEB128)

func encodeVarint(_ value: UInt64) -> [UInt8] {
    var result: [UInt8] = []
    var remaining = value
    repeat {
        var byte = UInt8(remaining & 0x7F)
        remaining >>= 7
        if remaining != 0 {
            byte |= 0x80
        }
        result.append(byte)
    } while remaining != 0
    return result
}

func decodeVarint(from data: Data, offset: inout Int) throws -> UInt64 {
    var result: UInt64 = 0
    var shift: UInt64 = 0
    while true {
        guard offset < data.count else { throw Fatal.message("varint: eof") }
        let byte = data[offset]
        offset += 1
        if shift >= 64 { throw Fatal.message("varint: overflow") }
        result |= UInt64(byte & 0x7F) << shift
        if (byte & 0x80) == 0 { return result }
        shift += 7
    }
}

func decodeVarintU32(from data: Data, offset: inout Int) throws -> UInt32 {
    let v = try decodeVarint(from: data, offset: &offset)
    if v > UInt64(UInt32.max) { throw Fatal.message("u32 overflow") }
    return UInt32(v)
}

// MARK: - Postcard helpers (subset)

func encodeString(_ s: String) -> [UInt8] {
    let bytes = Array(s.utf8)
    return encodeVarint(UInt64(bytes.count)) + bytes
}

func encodeBytes(_ bytes: [UInt8]) -> [UInt8] {
    return encodeVarint(UInt64(bytes.count)) + bytes
}

// MARK: - COBS

func cobsEncode(_ input: [UInt8]) -> [UInt8] {
    var out: [UInt8] = []
    out.reserveCapacity(input.count + 2)

    var codeIndex = 0
    var code: UInt8 = 1
    out.append(0) // placeholder

    for b in input {
        if b == 0 {
            out[codeIndex] = code
            codeIndex = out.count
            out.append(0) // placeholder
            code = 1
        } else {
            out.append(b)
            code &+= 1
            if code == 0xFF {
                out[codeIndex] = code
                codeIndex = out.count
                out.append(0)
                code = 1
            }
        }
    }

    out[codeIndex] = code
    return out
}

func cobsDecode(_ input: [UInt8]) throws -> [UInt8] {
    var out: [UInt8] = []
    out.reserveCapacity(input.count)

    var i = 0
    while i < input.count {
        let code = input[i]
        i += 1
        if code == 0 { throw Fatal.message("cobs: zero code") }
        let n = Int(code) - 1
        if i + n > input.count { throw Fatal.message("cobs: overrun") }
        if n > 0 {
            out.append(contentsOf: input[i..<(i + n)])
            i += n
        }
        if code != 0xFF && i < input.count {
            out.append(0)
        }
    }

    return out
}

// MARK: - Socket

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

// MARK: - Protocol subset

let peerAddr = ProcessInfo.processInfo.environment["PEER_ADDR"]
guard let peerAddr else { fatal("PEER_ADDR is not set") }
guard let idx = peerAddr.lastIndex(of: ":") else { fatal("Invalid PEER_ADDR \(peerAddr)") }
let host = String(peerAddr[..<idx])
let portStr = String(peerAddr[peerAddr.index(after: idx)...])
guard let portNum = UInt16(portStr) else { fatal("Invalid port in PEER_ADDR \(peerAddr)") }

let fd = connectTcp(host: host, port: portNum)

let localMaxPayload: UInt32 = 1024 * 1024
let localInitialCredit: UInt32 = 64 * 1024
var negotiatedMaxPayload: UInt32 = localMaxPayload
var haveReceivedHello = false

func sendHello() throws {
    // Message::Hello (0), Hello::V1 (0)
    var payload: [UInt8] = []
    payload += encodeVarint(0)
    payload += encodeVarint(0)
    payload += encodeVarint(UInt64(localMaxPayload))
    payload += encodeVarint(UInt64(localInitialCredit))

    var framed = cobsEncode(payload)
    framed.append(0)
    try writeAll(fd: fd, bytes: framed)
}

func sendGoodbye(reason: String) -> Never {
    do {
        var payload: [UInt8] = []
        payload += encodeVarint(1) // Message::Goodbye
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

try sendHello()

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
            sendGoodbye(reason: "message.decode-error")
        }

        do {
            let payload = Data(decoded)
            var o = 0
            let msgDisc = try decodeVarint(from: payload, offset: &o)

            if msgDisc == 0 {
                // Hello
                let helloDisc = try decodeVarint(from: payload, offset: &o)
                if helloDisc != 0 {
                    sendGoodbye(reason: "message.hello.unknown-version")
                }
                let remoteMax = try decodeVarintU32(from: payload, offset: &o)
                _ = try decodeVarintU32(from: payload, offset: &o) // initial_stream_credit
                negotiatedMaxPayload = min(localMaxPayload, remoteMax)
                haveReceivedHello = true
                continue
            }

            if !haveReceivedHello {
                continue
            }

            if msgDisc == 2 {
                // Request { request_id, method_id, metadata, payload }
                _ = try decodeVarint(from: payload, offset: &o) // request_id
                _ = try decodeVarint(from: payload, offset: &o) // method_id

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
                    sendGoodbye(reason: "flow.unary.payload-limit")
                }
                continue
            }

            if msgDisc == 3 {
                // Response { request_id, metadata, payload }
                _ = try decodeVarint(from: payload, offset: &o) // request_id

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
                    sendGoodbye(reason: "flow.unary.payload-limit")
                }
                continue
            }

            if msgDisc == 6 || msgDisc == 7 {
                // Close/Reset { stream_id }
                let streamId = try decodeVarint(from: payload, offset: &o)
                if streamId == 0 {
                    sendGoodbye(reason: "streaming.id.zero-reserved")
                }
                continue
            }
        } catch {
            sendGoodbye(reason: "message.decode-error")
        }
    }
}

close(fd)
exit(0)
