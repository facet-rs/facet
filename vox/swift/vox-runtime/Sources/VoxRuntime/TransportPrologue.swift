let transportHelloMagic = Array("VOTH".utf8)
let transportAcceptMagic = Array("VOTA".utf8)
let transportRejectMagic = Array("VOTR".utf8)
let transportVersion: UInt8 = 9
let rejectUnsupportedPrologue: UInt8 = 1
let defaultTransportPrologueTimeoutNs: UInt64 = 5_000_000_000

func encodeTransportHello() -> [UInt8] {
    [
        transportHelloMagic[0], transportHelloMagic[1], transportHelloMagic[2],
        transportHelloMagic[3],
        transportVersion,
        0,
        0,
        0,
    ]
}

public func encodeTransportAccept() -> [UInt8] {
    [
        transportAcceptMagic[0], transportAcceptMagic[1], transportAcceptMagic[2],
        transportAcceptMagic[3],
        transportVersion,
        0,
        0,
        0,
    ]
}

func encodeTransportRejectUnsupported() -> [UInt8] {
    [
        transportRejectMagic[0], transportRejectMagic[1], transportRejectMagic[2],
        transportRejectMagic[3],
        transportVersion,
        rejectUnsupportedPrologue,
        0,
        0,
    ]
}

func transportReservedBytesAreZero(_ bytes: [UInt8]) -> Bool {
    bytes.count == 8 && bytes[5] == 0 && bytes[6] == 0 && bytes[7] == 0
}

public func validateTransportHello(_ bytes: [UInt8]) throws {
    guard bytes.count == 8 else {
        throw TransportError.protocolViolation("invalid transport hello size")
    }
    guard Array(bytes[0..<4]) == transportHelloMagic else {
        throw TransportError.protocolViolation("expected TransportHello")
    }
    guard bytes[4] == transportVersion else {
        throw TransportError.protocolViolation("unsupported transport version \(bytes[4])")
    }
    guard transportReservedBytesAreZero(bytes) else {
        throw TransportError.protocolViolation("transport hello reserved bytes must be zero")
    }
}

func validateTransportAccept(_ bytes: [UInt8]) throws {
    guard bytes.count == 8 else {
        throw TransportError.protocolViolation("invalid transport prologue response size")
    }
    if Array(bytes[0..<4]) == transportAcceptMagic {
        guard bytes[4] == transportVersion else {
            throw TransportError.protocolViolation("unsupported transport version \(bytes[4])")
        }
        guard transportReservedBytesAreZero(bytes) else {
            throw TransportError.protocolViolation("transport accept reserved bytes must be zero")
        }
        return
    }
    if Array(bytes[0..<4]) == transportRejectMagic {
        if bytes[5] == rejectUnsupportedPrologue {
            throw TransportError.protocolViolation("transport rejected unsupported prologue")
        }
        throw TransportError.protocolViolation("transport rejected with reason \(bytes[5])")
    }
    throw TransportError.protocolViolation("expected TransportAccept or TransportReject")
}

// r[impl transport.prologue]
// r[impl transport.prologue.request]
// r[impl transport.prologue.accept]
// r[impl transport.prologue.reject-close]
public func performInitiatorLinkPrologue(
    link: some Link
) async throws {
    let context = VoxEstablishmentContext(
        role: .initiator,
        phase: .transportPrologue
    )
    try await withObservedEstablishment(context) {
        warnLog("[vox-prologue] initiator: sending TransportHello")
        try await link.sendRawPrologue(encodeTransportHello())
        warnLog("[vox-prologue] initiator: waiting for TransportAccept")
        guard let response = try await link.recvRawPrologue() else {
            warnLog("[vox-prologue] initiator: recvRawPrologue returned nil (connection closed)")
            throw TransportError.connectionClosed
        }
        warnLog("[vox-prologue] initiator: got response (\(response.count) bytes)")
        try validateTransportAccept(response)
        warnLog("[vox-prologue] initiator: transport accepted")
    }
}

// r[impl transport.prologue]
// r[impl transport.prologue.first-payload]
// r[impl transport.prologue.request]
// r[impl transport.prologue.accept]
// r[impl transport.prologue.reject-close]
public func performAcceptorLinkPrologue(
    link: some Link
) async throws {
    let context = VoxEstablishmentContext(
        role: .acceptor,
        phase: .transportPrologue
    )
    try await withObservedEstablishment(context) {
        warnLog("[vox-prologue] acceptor: waiting for TransportHello")
        guard let request = try await link.recvRawPrologue() else {
            warnLog("[vox-prologue] acceptor: recvRawPrologue returned nil (connection closed)")
            throw TransportError.connectionClosed
        }
        warnLog("[vox-prologue] acceptor: got request (\(request.count) bytes)")
        do {
            try validateTransportHello(request)
        } catch {
            try await link.sendRawPrologue(encodeTransportRejectUnsupported())
            throw error
        }
        try await link.sendRawPrologue(encodeTransportAccept())
        warnLog("[vox-prologue] acceptor: transport accepted")
    }
}
