let transportHelloMagic = Array("VOTH".utf8)
let transportAcceptMagic = Array("VOTA".utf8)
let transportRejectMagic = Array("VOTR".utf8)
let transportVersion: UInt8 = 9
let rejectUnsupportedMode: UInt8 = 1
let defaultTransportPrologueTimeoutNs: UInt64 = 5_000_000_000

func encodeTransportHello(_ conduit: ConduitKind) -> [UInt8] {
    [
        transportHelloMagic[0], transportHelloMagic[1], transportHelloMagic[2],
        transportHelloMagic[3],
        transportVersion,
        conduit == .stable ? 1 : 0,
        0,
        0,
    ]
}

public func encodeTransportAccept(_ conduit: ConduitKind) -> [UInt8] {
    [
        transportAcceptMagic[0], transportAcceptMagic[1], transportAcceptMagic[2],
        transportAcceptMagic[3],
        transportVersion,
        conduit == .stable ? 1 : 0,
        0,
        0,
    ]
}

func encodeTransportRejectUnsupported() -> [UInt8] {
    [
        transportRejectMagic[0], transportRejectMagic[1], transportRejectMagic[2],
        transportRejectMagic[3],
        transportVersion,
        rejectUnsupportedMode,
        0,
        0,
    ]
}

public func decodeTransportHello(_ bytes: [UInt8]) throws -> ConduitKind {
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

func validateTransportAccept(_ bytes: [UInt8], requested: ConduitKind) throws {
    guard bytes.count == 8 else {
        throw TransportError.protocolViolation("invalid transport prologue response size")
    }
    if Array(bytes[0..<4]) == transportAcceptMagic {
        guard bytes[4] == transportVersion else {
            throw TransportError.protocolViolation("unsupported transport version \(bytes[4])")
        }
        let selected = bytes[5] == 1 ? ConduitKind.stable : ConduitKind.bare
        guard selected == requested else {
            throw TransportError.protocolViolation(
                "transport selected \(selected) for requested \(requested)")
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

public func performInitiatorLinkPrologue(
    link: some Link,
    conduit: ConduitKind
) async throws {
    warnLog("[vox-prologue] initiator: sending TransportHello conduit=\(conduit)")
    try await link.sendRawPrologue(encodeTransportHello(conduit))
    warnLog("[vox-prologue] initiator: waiting for TransportAccept")
    guard let response = try await link.recvRawPrologue() else {
        warnLog("[vox-prologue] initiator: recvRawPrologue returned nil (connection closed)")
        throw TransportError.connectionClosed
    }
    warnLog("[vox-prologue] initiator: got response (\(response.count) bytes)")
    try validateTransportAccept(response, requested: conduit)
    warnLog("[vox-prologue] initiator: transport accepted")
}

public func performAcceptorLinkPrologue(
    link: some Link,
    supportedConduit: ConduitKind = .bare
) async throws -> ConduitKind {
    warnLog("[vox-prologue] acceptor: waiting for TransportHello")
    guard let request = try await link.recvRawPrologue() else {
        warnLog("[vox-prologue] acceptor: recvRawPrologue returned nil (connection closed)")
        throw TransportError.connectionClosed
    }
    warnLog("[vox-prologue] acceptor: got request (\(request.count) bytes)")
    let requested = try decodeTransportHello(request)
    warnLog("[vox-prologue] acceptor: peer wants conduit=\(requested), we support=\(supportedConduit)")
    guard requested == supportedConduit else {
        try await link.sendRawPrologue(encodeTransportRejectUnsupported())
        throw TransportError.protocolViolation("transport rejected unsupported conduit mode")
    }
    try await link.sendRawPrologue(encodeTransportAccept(requested))
    warnLog("[vox-prologue] acceptor: transport accepted")
    return requested
}
