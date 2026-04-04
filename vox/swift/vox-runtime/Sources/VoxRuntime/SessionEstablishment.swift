import Foundation

struct SessionHandshakeResult {
    let negotiated: Negotiated
    let peerSupportsRetry: Bool
    let sessionResumeKey: [UInt8]?
    let localRootSettings: ConnectionSettings
    let peerRootSettings: ConnectionSettings
    let peerMetadata: [MetadataEntry]
}

func oppositeParity(_ parity: Parity) -> Parity {
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
    resumeKey: [UInt8]? = nil,
    metadata: [MetadataEntry] = []
) async throws -> SessionHandshakeResult {
    traceLog(.handshake, "initiator sending Hello resumable=\(resumable)")
    let ourSettings = ConnectionSettings(parity: .odd, maxConcurrentRequests: maxConcurrentRequests)
    let hello = HandshakeHello(
        parity: ourSettings.parity,
        connectionSettings: ourSettings,
        messagePayloadSchemaCbor: wireMessageSchemasCbor,
        supportsRetry: true,
        resumeKey: resumeKey.map(ResumeKeyBytes.init(bytes:)),
        metadata: metadata
    )
    try await sendHandshake(link, .hello(hello))

    let peerHello: HandshakeHelloYourself
    switch try await recvHandshake(link) {
    case .helloYourself(let helloYourself):
        traceLog(.handshake, "initiator received HelloYourself")
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
    traceLog(.handshake, "initiator sent LetsGo")

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
        peerRootSettings: peerHello.connectionSettings,
        peerMetadata: peerHello.metadata
    )
}

func performAcceptorHandshake(
    link: any Link,
    maxPayloadSize: UInt32,
    maxConcurrentRequests: UInt32,
    resumable: Bool,
    expectedResumeKey: [UInt8]? = nil,
    metadata: [MetadataEntry] = []
) async throws -> SessionHandshakeResult {
    let peerHello: HandshakeHello
    switch try await recvHandshake(link) {
    case .hello(let hello):
        traceLog(.handshake, "acceptor received Hello resumable=\(hello.resumeKey != nil)")
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

    let ourSettings = ConnectionSettings(
        parity: oppositeParity(peerHello.parity),
        maxConcurrentRequests: maxConcurrentRequests
    )
    let sessionResumeKey = expectedResumeKey ?? (resumable ? freshSessionResumeKey() : nil)
    let helloYourself = HandshakeHelloYourself(
        connectionSettings: ourSettings,
        messagePayloadSchemaCbor: wireMessageSchemasCbor,
        supportsRetry: true,
        resumeKey: sessionResumeKey.map(ResumeKeyBytes.init(bytes:)),
        metadata: metadata
    )
    try await sendHandshake(link, .helloYourself(helloYourself))
    traceLog(.handshake, "acceptor sent HelloYourself resumable=\(sessionResumeKey != nil)")

    switch try await recvHandshake(link) {
    case .letsGo:
        traceLog(.handshake, "acceptor received LetsGo")
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
        peerRootSettings: peerHello.connectionSettings,
        peerMetadata: peerHello.metadata
    )
}

func buildEstablishedConduit(
    role: Role,
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
        if role == .acceptor && attachment.clientHello == nil {
            return try await StableConduit.connect(
                source: DeferredStableAcceptorAttachmentSource(link: attachment.link)
            )
        }
        return try await StableConduit.connect(source: singleAttachmentSource(attachment))
    }
}

private actor DeferredStableAcceptorAttachmentSource: LinkSource {
    private var link: (any Link)?

    init(link: any Link) {
        self.link = link
    }

    func nextLink() async throws -> LinkAttachment {
        guard let link else {
            throw TransportError.protocolViolation("single-use stable acceptor source exhausted")
        }
        self.link = nil
        return try await prepareStableAcceptorAttachment(link: link)
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
    recoverAttachment: (@Sendable () async throws -> LinkAttachment)? = nil,
    metadata: [MetadataEntry] = []
) async throws -> (Connection, Driver, SessionHandle, [UInt8]?, [MetadataEntry]) {
    let ourMaxPayload = maxPayloadSize ?? (1024 * 1024)
    let handshake = try await performInitiatorHandshake(
        link: attachment.link,
        maxPayloadSize: ourMaxPayload,
        maxConcurrentRequests: 64,
        resumable: resumable,
        metadata: metadata
    )

    let conduit = try await buildEstablishedConduit(
        role: .initiator,
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
    return (connection, driver, handle, handshake.sessionResumeKey, handshake.peerMetadata)
}

public func establishInitiator(
    link: any Link,
    transport: TransportConduitKind = .bare,
    dispatcher: any ServiceDispatcher,
    acceptConnections: Bool = false,
    maxPayloadSize: UInt32? = nil,
    keepalive: DriverKeepaliveConfig? = nil,
    resumable: Bool = false,
    recoverAttachment: (@Sendable () async throws -> LinkAttachment)? = nil,
    metadata: [MetadataEntry] = []
) async throws -> (Connection, Driver, SessionHandle, [UInt8]?, [MetadataEntry]) {
    try await establishInitiator(
        attachment: .initiator(link),
        transport: transport,
        dispatcher: dispatcher,
        acceptConnections: acceptConnections,
        maxPayloadSize: maxPayloadSize,
        keepalive: keepalive,
        resumable: resumable,
        recoverAttachment: recoverAttachment,
        metadata: metadata
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
    recoverAttachment: (@Sendable () async throws -> LinkAttachment)? = nil,
    metadata: [MetadataEntry] = []
) async throws -> (Connection, Driver, SessionHandle, [UInt8]?, [MetadataEntry]) {
    try await establishInitiator(
        link: conduit,
        transport: transport,
        dispatcher: dispatcher,
        acceptConnections: acceptConnections,
        maxPayloadSize: maxPayloadSize,
        keepalive: keepalive,
        resumable: resumable,
        recoverAttachment: recoverAttachment,
        metadata: metadata
    )
}

public func establishAcceptor(
    attachment: LinkAttachment,
    transport: TransportConduitKind = .bare,
    dispatcher: any ServiceDispatcher,
    acceptConnections: Bool = false,
    maxPayloadSize: UInt32? = nil,
    keepalive: DriverKeepaliveConfig? = nil,
    resumable: Bool = false,
    metadata: [MetadataEntry] = []
) async throws -> (Connection, Driver, SessionHandle, [UInt8]?, [MetadataEntry]) {
    if attachment.clientHello == nil {
        let negotiatedTransport = try await performAcceptorTransportPrologue(
            transport: attachment.link,
            supportedConduit: transport
        )
        guard negotiatedTransport == transport else {
            throw TransportError.protocolViolation(
                "transport negotiated \(negotiatedTransport) for requested \(transport)"
            )
        }
    }

    let ourMaxPayload = maxPayloadSize ?? (1024 * 1024)
    let handshake = try await performAcceptorHandshake(
        link: attachment.link,
        maxPayloadSize: ourMaxPayload,
        maxConcurrentRequests: 64,
        resumable: resumable,
        metadata: metadata
    )

    let conduit = try await buildEstablishedConduit(
        role: .acceptor,
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
    return (connection, driver, handle, handshake.sessionResumeKey, handshake.peerMetadata)
}

public func establishAcceptor(
    link: any Link,
    transport: TransportConduitKind = .bare,
    dispatcher: any ServiceDispatcher,
    acceptConnections: Bool = false,
    maxPayloadSize: UInt32? = nil,
    keepalive: DriverKeepaliveConfig? = nil,
    resumable: Bool = false,
    metadata: [MetadataEntry] = []
) async throws -> (Connection, Driver, SessionHandle, [UInt8]?, [MetadataEntry]) {
    try await establishAcceptor(
        attachment: .init(link: link),
        transport: transport,
        dispatcher: dispatcher,
        acceptConnections: acceptConnections,
        maxPayloadSize: maxPayloadSize,
        keepalive: keepalive,
        resumable: resumable,
        metadata: metadata
    )
}

public func establishAcceptor(
    conduit: any Link,
    transport: TransportConduitKind = .bare,
    dispatcher: any ServiceDispatcher,
    acceptConnections: Bool = false,
    maxPayloadSize: UInt32? = nil,
    keepalive: DriverKeepaliveConfig? = nil,
    resumable: Bool = false,
    metadata: [MetadataEntry] = []
) async throws -> (Connection, Driver, SessionHandle, [UInt8]?, [MetadataEntry]) {
    try await establishAcceptor(
        link: conduit,
        transport: transport,
        dispatcher: dispatcher,
        acceptConnections: acceptConnections,
        maxPayloadSize: maxPayloadSize,
        keepalive: keepalive,
        resumable: resumable,
        metadata: metadata
    )
}
