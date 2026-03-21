import Foundation

struct SessionHandshakeResult {
    let negotiated: Negotiated
    let peerSupportsRetry: Bool
    let sessionResumeKey: [UInt8]?
    let localRootSettings: ConnectionSettingsV7
    let peerRootSettings: ConnectionSettingsV7
}

func oppositeParity(_ parity: ParityV7) -> ParityV7 {
    switch parity {
    case .odd:
        return .even
    case .even:
        return .odd
    }
}

func sendHandshakeSorry(_ link: any Link, reason: String) async {
    try? await sendHandshake(link, .sorry(HandshakeSorry(reason: reason)))
}

func requireIdentityMessageSchema(
    _ peerMessageSchemaCbor: [UInt8],
    on link: any Link
) async throws {
    guard handshakeMessageSchemasMatch(peerMessageSchemaCbor) else {
        let reason = "unsupported message schema translation"
        await sendHandshakeSorry(link, reason: reason)
        throw ConnectionError.handshakeFailed(reason)
    }
}

func performInitiatorHandshake(
    link: any Link,
    maxPayloadSize: UInt32,
    maxConcurrentRequests: UInt32,
    resumable: Bool,
    resumeKey: [UInt8]? = nil
) async throws -> SessionHandshakeResult {
    let ourSettings = ConnectionSettingsV7(parity: .odd, maxConcurrentRequests: maxConcurrentRequests)
    let hello = HandshakeHello(
        parity: ourSettings.parity,
        connectionSettings: ourSettings,
        messagePayloadSchemaCbor: wireMessageSchemasCbor,
        supportsRetry: true,
        resumeKey: resumeKey.map(ResumeKeyBytes.init(bytes:))
    )
    try await sendHandshake(link, .hello(hello))

    let peerHello: HandshakeHelloYourself
    switch try await recvHandshake(link) {
    case .helloYourself(let helloYourself):
        peerHello = helloYourself
    case .sorry(let sorry):
        throw ConnectionError.handshakeFailed(sorry.reason)
    default:
        await sendHandshakeSorry(link, reason: "expected HelloYourself or Sorry")
        throw ConnectionError.handshakeFailed("expected HelloYourself")
    }

    try await requireIdentityMessageSchema(peerHello.messagePayloadSchemaCbor, on: link)

    let sessionResumeKey = peerHello.resumeKey?.bytes
    if resumable && sessionResumeKey == nil {
        await sendHandshakeSorry(link, reason: "peer did not advertise session resumption")
        throw ConnectionError.handshakeFailed("peer did not advertise session resumption")
    }

    try await sendHandshake(link, .letsGo(HandshakeLetsGo()))

    let negotiated = Negotiated(
        maxPayloadSize: maxPayloadSize,
        initialCredit: 64 * 1024,
        maxConcurrentRequests: min(
            ourSettings.maxConcurrentRequests,
            peerHello.connectionSettings.maxConcurrentRequests
        )
    )
    debugLog(
        "handshake complete: maxPayloadSize=\(negotiated.maxPayloadSize), "
            + "initialCredit=\(negotiated.initialCredit), "
            + "maxConcurrentRequests=\(negotiated.maxConcurrentRequests)"
    )

    return SessionHandshakeResult(
        negotiated: negotiated,
        peerSupportsRetry: peerHello.supportsRetry,
        sessionResumeKey: sessionResumeKey,
        localRootSettings: ourSettings,
        peerRootSettings: peerHello.connectionSettings
    )
}

func performAcceptorHandshake(
    link: any Link,
    maxPayloadSize: UInt32,
    maxConcurrentRequests: UInt32,
    resumable: Bool,
    expectedResumeKey: [UInt8]? = nil
) async throws -> SessionHandshakeResult {
    let peerHello: HandshakeHello
    switch try await recvHandshake(link) {
    case .hello(let hello):
        peerHello = hello
    default:
        throw ConnectionError.handshakeFailed("expected Hello")
    }

    try await requireIdentityMessageSchema(peerHello.messagePayloadSchemaCbor, on: link)

    if let expectedResumeKey {
        guard let actualResumeKey = peerHello.resumeKey?.bytes,
            sessionResumeKeysEqual(actualResumeKey, expectedResumeKey)
        else {
            let reason = "session resume key mismatch"
            await sendHandshakeSorry(link, reason: reason)
            throw ConnectionError.protocolViolation(rule: reason)
        }
    }

    let ourSettings = ConnectionSettingsV7(
        parity: oppositeParity(peerHello.parity),
        maxConcurrentRequests: maxConcurrentRequests
    )
    let sessionResumeKey = resumable ? freshSessionResumeKey() : nil
    let helloYourself = HandshakeHelloYourself(
        connectionSettings: ourSettings,
        messagePayloadSchemaCbor: wireMessageSchemasCbor,
        supportsRetry: true,
        resumeKey: sessionResumeKey.map(ResumeKeyBytes.init(bytes:))
    )
    try await sendHandshake(link, .helloYourself(helloYourself))

    switch try await recvHandshake(link) {
    case .letsGo:
        break
    case .sorry(let sorry):
        throw ConnectionError.handshakeFailed(sorry.reason)
    default:
        throw ConnectionError.handshakeFailed("expected LetsGo")
    }

    let negotiated = Negotiated(
        maxPayloadSize: maxPayloadSize,
        initialCredit: 64 * 1024,
        maxConcurrentRequests: min(
            ourSettings.maxConcurrentRequests,
            peerHello.connectionSettings.maxConcurrentRequests
        )
    )
    debugLog(
        "handshake complete: maxPayloadSize=\(negotiated.maxPayloadSize), "
            + "initialCredit=\(negotiated.initialCredit), "
            + "maxConcurrentRequests=\(negotiated.maxConcurrentRequests)"
    )

    return SessionHandshakeResult(
        negotiated: negotiated,
        peerSupportsRetry: peerHello.supportsRetry,
        sessionResumeKey: sessionResumeKey,
        localRootSettings: ourSettings,
        peerRootSettings: peerHello.connectionSettings
    )
}

func buildEstablishedConduit(
    transport: TransportConduitKind,
    attachment: LinkAttachment,
    recoverAttachment: (@Sendable () async throws -> LinkAttachment)? = nil
) async throws -> any Conduit {
    switch transport {
    case .bare:
        return BareConduit(link: attachment.link)
    case .stable:
        if let recoverAttachment {
            return try await StableConduit.connect(
                source: PrefetchedLinkSource(
                    first: attachment,
                    base: AnyLinkSource(recoverAttachment)
                )
            )
        }
        return try await StableConduit.connect(source: singleAttachmentSource(attachment))
    }
}

/// Establish a SHM guest connection as an initiator.
public func establishShmGuest<D: ServiceDispatcher>(
    transport: ShmGuestTransport,
    dispatcher: D,
    role: Role = .initiator,
    conduit: TransportConduitKind = .bare,
    acceptConnections: Bool = false,
    keepalive: DriverKeepaliveConfig? = nil,
    resumable: Bool = false
) async throws -> (Connection, Driver, SessionHandle, [UInt8]?) {
    switch role {
    case .initiator:
        try await performInitiatorTransportPrologue(transport: transport, conduit: conduit)
        return try await establishInitiator(
            attachment: .initiator(transport),
            transport: conduit,
            dispatcher: dispatcher,
            acceptConnections: acceptConnections,
            maxPayloadSize: transport.negotiated.maxPayloadSize,
            keepalive: keepalive,
            resumable: resumable
        )
    case .acceptor:
        _ = try await performAcceptorTransportPrologue(transport: transport, supportedConduit: .bare)
        return try await establishAcceptor(
            attachment: .init(link: transport),
            transport: .bare,
            dispatcher: dispatcher,
            acceptConnections: acceptConnections,
            keepalive: keepalive,
            resumable: resumable
        )
    }
}

public func establishInitiator(
    attachment: LinkAttachment,
    transport: TransportConduitKind = .bare,
    dispatcher: any ServiceDispatcher,
    acceptConnections: Bool = false,
    maxPayloadSize: UInt32? = nil,
    keepalive: DriverKeepaliveConfig? = nil,
    resumable: Bool = false,
    recoverAttachment: (@Sendable () async throws -> LinkAttachment)? = nil
) async throws -> (Connection, Driver, SessionHandle, [UInt8]?) {
    let ourMaxPayload = maxPayloadSize ?? (1024 * 1024)
    let handshake = try await performInitiatorHandshake(
        link: attachment.link,
        maxPayloadSize: ourMaxPayload,
        maxConcurrentRequests: 64,
        resumable: resumable
    )

    let conduit = try await buildEstablishedConduit(
        transport: transport,
        attachment: attachment,
        recoverAttachment: recoverAttachment
    )
    try await conduit.setMaxFrameSize(Int(handshake.negotiated.maxPayloadSize) + 64)

    let (connection, driver, handle) = makeSessionDriverAndConnection(
        conduit: conduit,
        dispatcher: dispatcher,
        role: .initiator,
        negotiated: handshake.negotiated,
        peerSupportsRetry: handshake.peerSupportsRetry,
        acceptConnections: acceptConnections,
        keepalive: keepalive,
        resumable: resumable,
        sessionResumeKey: handshake.sessionResumeKey,
        localRootSettings: handshake.localRootSettings,
        peerRootSettings: handshake.peerRootSettings,
        transport: transport,
        recoverAttachment: recoverAttachment
    )
    return (connection, driver, handle, handshake.sessionResumeKey)
}

public func establishInitiator(
    link: any Link,
    transport: TransportConduitKind = .bare,
    dispatcher: any ServiceDispatcher,
    acceptConnections: Bool = false,
    maxPayloadSize: UInt32? = nil,
    keepalive: DriverKeepaliveConfig? = nil,
    resumable: Bool = false,
    recoverAttachment: (@Sendable () async throws -> LinkAttachment)? = nil
) async throws -> (Connection, Driver, SessionHandle, [UInt8]?) {
    try await establishInitiator(
        attachment: .initiator(link),
        transport: transport,
        dispatcher: dispatcher,
        acceptConnections: acceptConnections,
        maxPayloadSize: maxPayloadSize,
        keepalive: keepalive,
        resumable: resumable,
        recoverAttachment: recoverAttachment
    )
}

public func establishInitiator(
    conduit: any Link,
    transport: TransportConduitKind = .bare,
    dispatcher: any ServiceDispatcher,
    acceptConnections: Bool = false,
    maxPayloadSize: UInt32? = nil,
    keepalive: DriverKeepaliveConfig? = nil,
    resumable: Bool = false,
    recoverAttachment: (@Sendable () async throws -> LinkAttachment)? = nil
) async throws -> (Connection, Driver, SessionHandle, [UInt8]?) {
    try await establishInitiator(
        link: conduit,
        transport: transport,
        dispatcher: dispatcher,
        acceptConnections: acceptConnections,
        maxPayloadSize: maxPayloadSize,
        keepalive: keepalive,
        resumable: resumable,
        recoverAttachment: recoverAttachment
    )
}

public func establishAcceptor(
    attachment: LinkAttachment,
    transport: TransportConduitKind = .bare,
    dispatcher: any ServiceDispatcher,
    acceptConnections: Bool = false,
    maxPayloadSize: UInt32? = nil,
    keepalive: DriverKeepaliveConfig? = nil,
    resumable: Bool = false
) async throws -> (Connection, Driver, SessionHandle, [UInt8]?) {
    let ourMaxPayload = maxPayloadSize ?? (1024 * 1024)
    let handshake = try await performAcceptorHandshake(
        link: attachment.link,
        maxPayloadSize: ourMaxPayload,
        maxConcurrentRequests: 64,
        resumable: resumable
    )

    let conduit = try await buildEstablishedConduit(
        transport: transport,
        attachment: attachment
    )
    try await conduit.setMaxFrameSize(Int(handshake.negotiated.maxPayloadSize) + 64)

    let (connection, driver, handle) = makeSessionDriverAndConnection(
        conduit: conduit,
        dispatcher: dispatcher,
        role: .acceptor,
        negotiated: handshake.negotiated,
        peerSupportsRetry: handshake.peerSupportsRetry,
        acceptConnections: acceptConnections,
        keepalive: keepalive,
        resumable: resumable,
        sessionResumeKey: handshake.sessionResumeKey,
        localRootSettings: handshake.localRootSettings,
        peerRootSettings: handshake.peerRootSettings,
        transport: transport,
        recoverAttachment: nil
    )
    return (connection, driver, handle, handshake.sessionResumeKey)
}

public func establishAcceptor(
    link: any Link,
    transport: TransportConduitKind = .bare,
    dispatcher: any ServiceDispatcher,
    acceptConnections: Bool = false,
    maxPayloadSize: UInt32? = nil,
    keepalive: DriverKeepaliveConfig? = nil,
    resumable: Bool = false
) async throws -> (Connection, Driver, SessionHandle, [UInt8]?) {
    try await establishAcceptor(
        attachment: .init(link: link),
        transport: transport,
        dispatcher: dispatcher,
        acceptConnections: acceptConnections,
        maxPayloadSize: maxPayloadSize,
        keepalive: keepalive,
        resumable: resumable
    )
}

public func establishAcceptor(
    conduit: any Link,
    transport: TransportConduitKind = .bare,
    dispatcher: any ServiceDispatcher,
    acceptConnections: Bool = false,
    maxPayloadSize: UInt32? = nil,
    keepalive: DriverKeepaliveConfig? = nil,
    resumable: Bool = false
) async throws -> (Connection, Driver, SessionHandle, [UInt8]?) {
    try await establishAcceptor(
        link: conduit,
        transport: transport,
        dispatcher: dispatcher,
        acceptConnections: acceptConnections,
        maxPayloadSize: maxPayloadSize,
        keepalive: keepalive,
        resumable: resumable
    )
}
