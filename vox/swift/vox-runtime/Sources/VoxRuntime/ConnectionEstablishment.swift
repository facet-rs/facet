import Foundation

struct ConnectionHandshakeResult {
    let negotiated: Negotiated
    let localControlSettings: ConnectionSettings
    let peerControlSettings: ConnectionSettings
    let peerMetadata: Metadata
    let peerEvidence: PeerEvidence
    let peerIdentity: PeerIdentity
    /// The peer's advertised Message schema closure, used to build the conduit's
    /// compatibility decoder.
    let peerMessageSchema: [UInt8]
}

// r[impl connection.role]
// r[impl connection.lane-id-parity]
func oppositeParity(_ parity: Parity) -> Parity {
    switch parity {
    case .odd:
        return .even
    case .even:
        return .odd
    }
}

// r[impl connection.handshake.sorry]
func sendHandshakeSorry(_ link: any Link, reason: String) async {
    try? await sendHandshake(link, .sorry(Sorry(reason: reason)))
}

// r[impl connection.handshake.decline]
// r[impl connection.policy.establishment.rejection]
// r[impl rejection.reason.taxonomy]
// r[impl rpc.metadata.records]
func sendHandshakeDecline(_ link: any Link, decline: Decline) async {
    try? await sendHandshake(link, .decline(decline))
}

// r[impl connection.identity.resolver]
// r[impl connection.policy.establishment]
func resolvePeerIdentity(
    role: VoxEstablishmentRole,
    evidence: PeerEvidence,
    claims: Metadata,
    identityResolver: IdentityResolver?,
    on link: any Link
) async throws -> PeerIdentity {
    let resolver = identityResolver ?? { _ in PeerIdentity.anonymous }
    let context = IdentityResolutionContext(role: role, evidence: evidence, claims: claims)
    do {
        return try await withObservedEstablishment(
            VoxEstablishmentContext(role: role, phase: .identityResolution)
        ) {
            try await withObservedEstablishment(
                VoxEstablishmentContext(role: role, phase: .connectionPolicy)
            ) {
                try await resolver(context)
            }
        }
    } catch let decline as ConnectionDeclinedError {
        await sendHandshakeDecline(link, decline: decline.decline)
        throw decline
    }
}

// r[impl connection.handshake.protocol-schema]
// r[impl connection.handshake.protocol-schema.connection-scoped]
func requireIdentityMessageSchema(
    _ peerMessageSchema: [UInt8],
    on link: any Link
) async throws {
    guard handshakeMessageSchemasMatch(peerMessageSchema) else {
        let reason = "unsupported message compatibility plan"
        await sendHandshakeSorry(link, reason: reason)
        throw ConnectionError.handshakeFailed(reason)
    }
}

/// The local Message schema closure, advertised in the handshake.
/// r[impl connection.handshake.protocol-schema]
private var localMessagePayloadSchema: [UInt8] { MessageSchemaClosure }

func validateInitialChannelCredit(_ initialChannelCredit: UInt32) throws {
    // r[impl rpc.flow-control.credit.initial.zero]
    guard initialChannelCredit > 0 else {
        throw ConnectionError.protocolViolation(rule: "rpc.flow-control.credit.initial.zero")
    }
}

func makeConnectionSettings(
    parity: Parity,
    maxConcurrentRequests: UInt32,
    initialChannelCredit: UInt32
) throws -> ConnectionSettings {
    // r[impl lane.settings]
    // r[impl rpc.flow-control.max-concurrent-requests.default]
    // r[impl rpc.flow-control.credit.initial.high-level]
    try validateInitialChannelCredit(initialChannelCredit)
    return ConnectionSettings(
        parity: parity,
        maxConcurrentRequests: maxConcurrentRequests,
        initialChannelCredit: initialChannelCredit
    )
}

// r[impl connection.protocol]
// r[impl connection.handshake]
// r[impl connection.handshake.metadata]
// r[impl rpc.metadata.records]
// r[impl connection.handshake.phon]
// r[impl connection.handshake.protocol-schema]
// r[impl connection.handshake.protocol-schema.connection-scoped]
// r[impl connection.handshake.unversioned]
// r[impl lane.settings]
// r[impl connection.peer]
// r[impl connection.role]
// r[impl connection.symmetry]
func performInitiatorHandshake(
    link: any Link,
    maxPayloadSize: UInt32,
    maxConcurrentRequests: UInt32,
    initialChannelCredit: UInt32 = 16,
    metadata: Metadata = .null,
    peerEvidence: PeerEvidence = .none,
    identityResolver: IdentityResolver? = nil
) async throws -> ConnectionHandshakeResult {
    traceLog(.handshake, "initiator sending Hello")
    let ourSettings = try makeConnectionSettings(
        parity: .odd,
        maxConcurrentRequests: maxConcurrentRequests,
        initialChannelCredit: initialChannelCredit
    )
    let hello = Hello(
        parity: ourSettings.parity,
        connectionSettings: ourSettings,
        messagePayloadSchema: Data(localMessagePayloadSchema),
        metadata: metadata
    )
    try await sendHandshake(link, .hello(hello))

    let peerHello: HelloYourself
    switch try await recvHandshake(link) {
    case .helloYourself(let helloYourself):
        traceLog(.handshake, "initiator received HelloYourself")
        peerHello = helloYourself
    case .decline(let decline):
        throw ConnectionDeclinedError(decline: decline, receivedFromPeer: true)
    case .sorry(let sorry):
        throw ConnectionError.handshakeFailed(sorry.reason)
    default:
        await sendHandshakeSorry(link, reason: "expected HelloYourself or Sorry")
        throw ConnectionError.handshakeFailed("expected HelloYourself")
    }

    try validateInitialChannelCredit(peerHello.connectionSettings.initialChannelCredit)

    let peerSchema = [UInt8](peerHello.messagePayloadSchema)
    let peerIdentity = try await resolvePeerIdentity(
        role: .initiator,
        evidence: peerEvidence,
        claims: peerHello.metadata,
        identityResolver: identityResolver,
        on: link
    )
    try await requireIdentityMessageSchema(peerSchema, on: link)

    try await sendHandshake(link, .letsGo(LetsGo()))
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

    return ConnectionHandshakeResult(
        negotiated: negotiated,
        localControlSettings: ourSettings,
        peerControlSettings: peerHello.connectionSettings,
        peerMetadata: peerHello.metadata,
        peerEvidence: peerEvidence,
        peerIdentity: peerIdentity,
        peerMessageSchema: peerSchema
    )
}

// r[impl connection.protocol]
// r[impl connection.handshake]
// r[impl connection.handshake.metadata]
// r[impl rpc.metadata.records]
// r[impl connection.handshake.phon]
// r[impl connection.handshake.protocol-schema]
// r[impl connection.handshake.protocol-schema.connection-scoped]
// r[impl connection.handshake.unversioned]
// r[impl lane.settings]
// r[impl connection.peer]
// r[impl connection.role]
// r[impl connection.symmetry]
func performAcceptorHandshake(
    link: any Link,
    maxPayloadSize: UInt32,
    maxConcurrentRequests: UInt32,
    initialChannelCredit: UInt32 = 16,
    metadata: Metadata = .null,
    peerEvidence: PeerEvidence = .none,
    identityResolver: IdentityResolver? = nil
) async throws -> ConnectionHandshakeResult {
    let peerHello: Hello
    switch try await recvHandshake(link) {
    case .hello(let hello):
        traceLog(.handshake, "acceptor received Hello")
        peerHello = hello
    default:
        throw ConnectionError.handshakeFailed("expected Hello")
    }

    let peerSchema = [UInt8](peerHello.messagePayloadSchema)
    try validateInitialChannelCredit(peerHello.connectionSettings.initialChannelCredit)
    let peerIdentity = try await resolvePeerIdentity(
        role: .acceptor,
        evidence: peerEvidence,
        claims: peerHello.metadata,
        identityResolver: identityResolver,
        on: link
    )
    try await requireIdentityMessageSchema(peerSchema, on: link)

    let ourSettings = try makeConnectionSettings(
        parity: oppositeParity(peerHello.parity),
        maxConcurrentRequests: maxConcurrentRequests,
        initialChannelCredit: initialChannelCredit
    )
    let helloYourself = HelloYourself(
        connectionSettings: ourSettings,
        messagePayloadSchema: Data(localMessagePayloadSchema),
        metadata: metadata
    )
    try await sendHandshake(link, .helloYourself(helloYourself))
    traceLog(.handshake, "acceptor sent HelloYourself")

    switch try await recvHandshake(link) {
    case .letsGo:
        traceLog(.handshake, "acceptor received LetsGo")
        break
    case .decline(let decline):
        throw ConnectionDeclinedError(decline: decline, receivedFromPeer: true)
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

    return ConnectionHandshakeResult(
        negotiated: negotiated,
        localControlSettings: ourSettings,
        peerControlSettings: peerHello.connectionSettings,
        peerMetadata: peerHello.metadata,
        peerEvidence: peerEvidence,
        peerIdentity: peerIdentity,
        peerMessageSchema: peerSchema
    )
}

// r[impl transport.prologue.post-accept]
// r[impl connection.message.payloads]
func buildEstablishedConduit(
    role: Role,
    attachment: LinkAttachment,
    peerMessageSchema: [UInt8]
) async throws -> any Conduit {
    let context = VoxEstablishmentContext(
        role: voxEstablishmentRole(role),
        phase: .schemaDecodePlan
    )
    return try await withObservedEstablishment(context) {
        BareConduit(link: attachment.link, peerMessageSchema: peerMessageSchema)
    }
}

// r[impl rpc.connection-setup]
func establishInitiator(
    attachment: LinkAttachment,
    dispatcher: any ServiceDispatcher,
    laneAcceptor: (any LaneAcceptor)? = nil,
    maxPayloadSize: UInt32? = nil,
    maxConcurrentRequests: UInt32 = 64,
    initialChannelCredit: UInt32 = 16,
    keepalive: ConnectionKeepaliveConfig? = nil,
    metadata: Metadata = .null,
    identityResolver: IdentityResolver? = nil
) async throws -> (Lane, Driver, ConnectionHandle, Metadata) {
    warnLog("[vox-establish] initiator: starting handshake")
    let ourMaxPayload = maxPayloadSize ?? (1024 * 1024)
    let handshake = try await withObservedEstablishment(
        VoxEstablishmentContext(role: .initiator, phase: .connectionHandshake)
    ) {
        try await performInitiatorHandshake(
            link: attachment.link,
            maxPayloadSize: ourMaxPayload,
            maxConcurrentRequests: maxConcurrentRequests,
            initialChannelCredit: initialChannelCredit,
            metadata: metadata,
            peerEvidence: attachment.peerEvidence,
            identityResolver: identityResolver
        )
    }
    warnLog("[vox-establish] initiator: handshake done")

    let conduit = try await buildEstablishedConduit(
        role: .initiator,
        attachment: attachment,
        peerMessageSchema: handshake.peerMessageSchema
    )
    try await conduit.setMaxFrameSize(Int(handshake.negotiated.maxPayloadSize) + 64)

    let (connection, driver, handle) = makeConnectionDriverAndControlLane(
        conduit: conduit,
        dispatcher: dispatcher,
        role: .initiator,
        negotiated: handshake.negotiated,
        laneAcceptor: laneAcceptor,
        keepalive: keepalive,
        localControlSettings: handshake.localControlSettings,
        peerControlSettings: handshake.peerControlSettings,
        peerMessageSchema: handshake.peerMessageSchema,
        peerEvidence: handshake.peerEvidence,
        peerIdentity: handshake.peerIdentity
    )
    return (connection, driver, handle, handshake.peerMetadata)
}

// r[impl rpc.connection-setup]
func establishInitiator(
    link: any Link,
    dispatcher: any ServiceDispatcher,
    laneAcceptor: (any LaneAcceptor)? = nil,
    maxPayloadSize: UInt32? = nil,
    maxConcurrentRequests: UInt32 = 64,
    initialChannelCredit: UInt32 = 16,
    keepalive: ConnectionKeepaliveConfig? = nil,
    metadata: Metadata = .null,
    identityResolver: IdentityResolver? = nil
) async throws -> (Lane, Driver, ConnectionHandle, Metadata) {
    try await establishInitiator(
        attachment: .initiator(link),
        dispatcher: dispatcher,
        laneAcceptor: laneAcceptor,
        maxPayloadSize: maxPayloadSize,
        maxConcurrentRequests: maxConcurrentRequests,
        initialChannelCredit: initialChannelCredit,
        keepalive: keepalive,
        metadata: metadata,
        identityResolver: identityResolver
    )
}

// r[impl rpc.connection-setup]
func establishInitiator(
    conduit: any Link,
    dispatcher: any ServiceDispatcher,
    laneAcceptor: (any LaneAcceptor)? = nil,
    maxPayloadSize: UInt32? = nil,
    maxConcurrentRequests: UInt32 = 64,
    initialChannelCredit: UInt32 = 16,
    keepalive: ConnectionKeepaliveConfig? = nil,
    metadata: Metadata = .null,
    identityResolver: IdentityResolver? = nil
) async throws -> (Lane, Driver, ConnectionHandle, Metadata) {
    try await establishInitiator(
        link: conduit,
        dispatcher: dispatcher,
        laneAcceptor: laneAcceptor,
        maxPayloadSize: maxPayloadSize,
        maxConcurrentRequests: maxConcurrentRequests,
        initialChannelCredit: initialChannelCredit,
        keepalive: keepalive,
        metadata: metadata,
        identityResolver: identityResolver
    )
}

// r[impl transport.prologue.first-payload]
// r[impl transport.prologue.post-accept]
// r[impl rpc.connection-setup]
func establishAcceptor(
    attachment: LinkAttachment,
    dispatcher: any ServiceDispatcher,
    laneAcceptor: (any LaneAcceptor)? = nil,
    maxPayloadSize: UInt32? = nil,
    maxConcurrentRequests: UInt32 = 64,
    initialChannelCredit: UInt32 = 16,
    keepalive: ConnectionKeepaliveConfig? = nil,
    metadata: Metadata = .null,
    identityResolver: IdentityResolver? = nil
) async throws -> (Lane, Driver, ConnectionHandle, Metadata) {
    warnLog("[vox-establish] acceptor: prologueComplete=\(attachment.hasCompletedPrologue)")
    if !attachment.hasCompletedPrologue {
        warnLog("[vox-establish] acceptor: running link prologue")
        try await performAcceptorLinkPrologue(link: attachment.link)
        warnLog("[vox-establish] acceptor: prologue done")
    }

    let ourMaxPayload = maxPayloadSize ?? (1024 * 1024)
    warnLog("[vox-establish] acceptor: starting handshake")
    let handshake = try await withObservedEstablishment(
        VoxEstablishmentContext(role: .acceptor, phase: .connectionHandshake)
    ) {
        try await performAcceptorHandshake(
            link: attachment.link,
            maxPayloadSize: ourMaxPayload,
            maxConcurrentRequests: maxConcurrentRequests,
            initialChannelCredit: initialChannelCredit,
            metadata: metadata,
            peerEvidence: attachment.peerEvidence,
            identityResolver: identityResolver
        )
    }

    let conduit = try await buildEstablishedConduit(
        role: .acceptor,
        attachment: attachment,
        peerMessageSchema: handshake.peerMessageSchema
    )
    try await conduit.setMaxFrameSize(Int(handshake.negotiated.maxPayloadSize) + 64)

    let (connection, driver, handle) = makeConnectionDriverAndControlLane(
        conduit: conduit,
        dispatcher: dispatcher,
        role: .acceptor,
        negotiated: handshake.negotiated,
        laneAcceptor: laneAcceptor,
        keepalive: keepalive,
        localControlSettings: handshake.localControlSettings,
        peerControlSettings: handshake.peerControlSettings,
        peerMessageSchema: handshake.peerMessageSchema,
        peerEvidence: handshake.peerEvidence,
        peerIdentity: handshake.peerIdentity
    )
    return (connection, driver, handle, handshake.peerMetadata)
}

// r[impl rpc.connection-setup]
func establishAcceptor(
    link: any Link,
    dispatcher: any ServiceDispatcher,
    laneAcceptor: (any LaneAcceptor)? = nil,
    maxPayloadSize: UInt32? = nil,
    maxConcurrentRequests: UInt32 = 64,
    initialChannelCredit: UInt32 = 16,
    keepalive: ConnectionKeepaliveConfig? = nil,
    metadata: Metadata = .null,
    identityResolver: IdentityResolver? = nil
) async throws -> (Lane, Driver, ConnectionHandle, Metadata) {
    try await establishAcceptor(
        attachment: .init(link: link),
        dispatcher: dispatcher,
        laneAcceptor: laneAcceptor,
        maxPayloadSize: maxPayloadSize,
        maxConcurrentRequests: maxConcurrentRequests,
        initialChannelCredit: initialChannelCredit,
        keepalive: keepalive,
        metadata: metadata,
        identityResolver: identityResolver
    )
}

// r[impl rpc.connection-setup]
func establishAcceptor(
    conduit: any Link,
    dispatcher: any ServiceDispatcher,
    laneAcceptor: (any LaneAcceptor)? = nil,
    maxPayloadSize: UInt32? = nil,
    maxConcurrentRequests: UInt32 = 64,
    initialChannelCredit: UInt32 = 16,
    keepalive: ConnectionKeepaliveConfig? = nil,
    metadata: Metadata = .null,
    identityResolver: IdentityResolver? = nil
) async throws -> (Lane, Driver, ConnectionHandle, Metadata) {
    try await establishAcceptor(
        link: conduit,
        dispatcher: dispatcher,
        laneAcceptor: laneAcceptor,
        maxPayloadSize: maxPayloadSize,
        maxConcurrentRequests: maxConcurrentRequests,
        initialChannelCredit: initialChannelCredit,
        keepalive: keepalive,
        metadata: metadata,
        identityResolver: identityResolver
    )
}
