let transportHelloMagic = Array("ROTH".utf8)
let transportAcceptMagic = Array("ROTA".utf8)
let transportRejectMagic = Array("ROTR".utf8)
let transportVersion: UInt8 = 9
let rejectUnsupportedMode: UInt8 = 1
let defaultTransportPrologueTimeoutNs: UInt64 = 5_000_000_000

func encodeTransportHello(_ conduit: TransportConduitKind) -> [UInt8] {
    [
        transportHelloMagic[0], transportHelloMagic[1], transportHelloMagic[2], transportHelloMagic[3],
        transportVersion,
        conduit == .stable ? 1 : 0,
        0,
        0,
    ]
}

public func encodeTransportAccept(_ conduit: TransportConduitKind) -> [UInt8] {
    [
        transportAcceptMagic[0], transportAcceptMagic[1], transportAcceptMagic[2], transportAcceptMagic[3],
        transportVersion,
        conduit == .stable ? 1 : 0,
        0,
        0,
    ]
}

func encodeTransportRejectUnsupported() -> [UInt8] {
    [
        transportRejectMagic[0], transportRejectMagic[1], transportRejectMagic[2], transportRejectMagic[3],
        transportVersion,
        rejectUnsupportedMode,
        0,
        0,
    ]
}

public func decodeTransportHello(_ bytes: [UInt8]) throws -> TransportConduitKind {
    guard bytes.count == 8 else {
        throw TransportError.protocolViolation("invalid transport hello size")
    }
    guard Array(bytes[0..<4]) == transportHelloMagic else {
        throw TransportError.protocolViolation("expected TransportHello")
    }
    guard bytes[4] == transportVersion else {
        throw TransportError.protocolViolation("unsupported transport version \(bytes[4])")
    }
    switch bytes[5] {
    case 0:
        return .bare
    case 1:
        return .stable
    default:
        throw TransportError.protocolViolation("unknown conduit mode \(bytes[5])")
    }
}

func validateTransportAccept(_ bytes: [UInt8], requested: TransportConduitKind) throws {
    guard bytes.count == 8 else {
        throw TransportError.protocolViolation("invalid transport prologue response size")
    }
    if Array(bytes[0..<4]) == transportAcceptMagic {
        guard bytes[4] == transportVersion else {
            throw TransportError.protocolViolation("unsupported transport version \(bytes[4])")
        }
        let selected = bytes[5] == 1 ? TransportConduitKind.stable : TransportConduitKind.bare
        guard selected == requested else {
            throw TransportError.protocolViolation("transport selected \(selected) for requested \(requested)")
        }
        return
    }
    if Array(bytes[0..<4]) == transportRejectMagic {
        if bytes[5] == rejectUnsupportedMode {
            throw TransportError.protocolViolation("transport rejected unsupported conduit mode")
        }
        throw TransportError.protocolViolation("transport rejected with reason \(bytes[5])")
    }
    throw TransportError.protocolViolation("expected TransportAccept or TransportReject")
}

public func performInitiatorTransportPrologue(
    transport: some Link,
    conduit: TransportConduitKind
) async throws {
    try await transport.sendRawPrologue(encodeTransportHello(conduit))
    guard let response = try await transport.recvRawPrologue() else {
        throw TransportError.connectionClosed
    }
    try validateTransportAccept(response, requested: conduit)
}

public func performAcceptorTransportPrologue(
    transport: some Link,
    supportedConduit: TransportConduitKind = .bare
) async throws -> TransportConduitKind {
    guard let request = try await transport.recvRawPrologue() else {
        throw TransportError.connectionClosed
    }
    let requested = try decodeTransportHello(request)
    guard requested == supportedConduit else {
        try await transport.sendRawPrologue(encodeTransportRejectUnsupported())
        throw TransportError.protocolViolation("transport rejected unsupported conduit mode")
    }
    try await transport.sendRawPrologue(encodeTransportAccept(requested))
    return requested
}
