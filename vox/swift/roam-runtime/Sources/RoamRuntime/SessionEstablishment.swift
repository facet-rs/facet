import Foundation

private let sessionEstablishmentMetadataFlagsNone: UInt64 = 0
private let sessionEstablishmentConnectionCorrelationKey = "moire.connection_correlation_id"

private func establishmentMetadataString(_ metadata: [MetadataEntryV7], key: String) -> String? {
    for entry in metadata where entry.key == key {
        if entry.key == key, case .string(let value) = entry.value {
            return value
        }
    }
    return nil
}

private func establishmentHelloCorrelationId(_ hello: HelloV7) -> String? {
    establishmentMetadataString(hello.metadata, key: sessionEstablishmentConnectionCorrelationKey)
}

private func establishmentNextConnectionCorrelationId() -> String {
    "swift.\(UUID().uuidString.lowercased())"
}

/// Establish a SHM guest connection as an initiator.
///
/// SHM is a transport bootstrap; session establishment still performs the v7
/// Hello/HelloYourself exchange over the selected conduit.
public func establishShmGuest<D: ServiceDispatcher>(
    transport: ShmGuestTransport,
    dispatcher: D,
    role: Role = .initiator,
    conduit: TransportConduitKind = .bare,
    acceptConnections: Bool = false,
    keepalive: DriverKeepaliveConfig? = nil
) async throws -> (Connection, Driver) {
    switch role {
    case .initiator:
        try await performInitiatorTransportPrologue(transport: transport, conduit: conduit)
        let selectedConduit = BareConduit(link: transport)
        return try await establishInitiator(
            conduit: selectedConduit,
            dispatcher: dispatcher,
            acceptConnections: acceptConnections,
            maxPayloadSize: transport.negotiated.maxPayloadSize,
            keepalive: keepalive
        )
    case .acceptor:
        _ = try await performAcceptorTransportPrologue(transport: transport, supportedConduit: .bare)
        let selectedConduit = BareConduit(link: transport)
        return try await establishAcceptor(
            conduit: selectedConduit,
            dispatcher: dispatcher,
            acceptConnections: acceptConnections,
            keepalive: keepalive
        )
    }
}

/// Establish a connection as initiator.
public func establishInitiator(
    conduit: any Conduit,
    dispatcher: any ServiceDispatcher,
    acceptConnections: Bool = false,
    maxPayloadSize: UInt32? = nil,
    keepalive: DriverKeepaliveConfig? = nil
) async throws -> (Connection, Driver) {
    let ourMaxPayload = maxPayloadSize ?? (1024 * 1024)
    let ourInitialCredit: UInt32 = 64 * 1024
    let ourCorrelationId = establishmentNextConnectionCorrelationId()
    let ourHello = HelloV7(
        version: 7,
        connectionSettings: ConnectionSettingsV7(parity: .odd, maxConcurrentRequests: 64),
        metadata: appendRetrySupportMetadata([
            MetadataEntryV7(
                key: sessionEstablishmentConnectionCorrelationKey,
                value: .string(ourCorrelationId),
                flags: sessionEstablishmentMetadataFlagsNone
            )
        ])
    )
    try await conduit.send(.hello(ourHello))

    guard let peerMsg = try await conduit.recv() else {
        try? await conduit.send(.protocolError(description: "handshake.expected-hello-yourself"))
        throw ConnectionError.handshakeFailed("expected HelloYourself")
    }
    guard case .helloYourself(let peerHello) = peerMsg.payload else {
        try? await conduit.send(.protocolError(description: "handshake.expected-hello-yourself"))
        throw ConnectionError.handshakeFailed("expected HelloYourself")
    }

    let peerCorrelationId = establishmentMetadataString(
        peerHello.metadata,
        key: sessionEstablishmentConnectionCorrelationKey
    )
    let peerSupportsRetry = metadataSupportsRetry(peerHello.metadata)
    let canonicalCorrelationId = ourCorrelationId.isEmpty ? peerCorrelationId : ourCorrelationId
    _ = canonicalCorrelationId

    let negotiated = Negotiated(
        maxPayloadSize: ourMaxPayload,
        initialCredit: ourInitialCredit,
        maxConcurrentRequests: min(
            ourHello.connectionSettings.maxConcurrentRequests,
            peerHello.connectionSettings.maxConcurrentRequests
        )
    )
    debugLog(
        "handshake complete: maxPayloadSize=\(negotiated.maxPayloadSize), initialCredit=\(negotiated.initialCredit), maxConcurrentRequests=\(negotiated.maxConcurrentRequests)"
    )

    try await conduit.setMaxFrameSize(Int(negotiated.maxPayloadSize) + 64)

    return makeDriverAndConnection(
        conduit: conduit,
        dispatcher: dispatcher,
        role: .initiator,
        negotiated: negotiated,
        peerSupportsRetry: peerSupportsRetry,
        acceptConnections: acceptConnections,
        keepalive: keepalive
    )
}

/// Establish a connection as acceptor.
public func establishAcceptor(
    conduit: any Conduit,
    dispatcher: any ServiceDispatcher,
    acceptConnections: Bool = false,
    maxPayloadSize: UInt32? = nil,
    keepalive: DriverKeepaliveConfig? = nil
) async throws -> (Connection, Driver) {
    let ourMaxPayload = maxPayloadSize ?? (1024 * 1024)
    let ourInitialCredit: UInt32 = 64 * 1024
    let ourCorrelationId = establishmentNextConnectionCorrelationId()
    guard let peerMsg = try await conduit.recv() else {
        throw ConnectionError.handshakeFailed("expected Hello")
    }
    guard case .hello(let peerHello) = peerMsg.payload else {
        try? await conduit.send(.protocolError(description: "handshake.expected-hello"))
        throw ConnectionError.handshakeFailed("expected Hello")
    }
    if peerHello.version != 7 {
        try? await conduit.send(.protocolError(description: "message.hello.unknown-version"))
        throw ConnectionError.handshakeFailed("message.hello.unknown-version")
    }

    let ourHello = HelloYourselfV7(
        connectionSettings: ConnectionSettingsV7(parity: .even, maxConcurrentRequests: 64),
        metadata: appendRetrySupportMetadata([
            MetadataEntryV7(
                key: sessionEstablishmentConnectionCorrelationKey,
                value: .string(ourCorrelationId),
                flags: sessionEstablishmentMetadataFlagsNone
            )
        ])
    )
    try await conduit.send(.helloYourself(ourHello))

    let peerCorrelationId = establishmentHelloCorrelationId(peerHello)
    let peerSupportsRetry = metadataSupportsRetry(peerHello.metadata)
    let canonicalCorrelationId = peerCorrelationId ?? ourCorrelationId
    _ = canonicalCorrelationId

    let negotiated = Negotiated(
        maxPayloadSize: ourMaxPayload,
        initialCredit: ourInitialCredit,
        maxConcurrentRequests: min(
            ourHello.connectionSettings.maxConcurrentRequests,
            peerHello.connectionSettings.maxConcurrentRequests
        )
    )
    debugLog(
        "handshake complete: maxPayloadSize=\(negotiated.maxPayloadSize), initialCredit=\(negotiated.initialCredit), maxConcurrentRequests=\(negotiated.maxConcurrentRequests)"
    )

    try await conduit.setMaxFrameSize(Int(negotiated.maxPayloadSize) + 64)

    return makeDriverAndConnection(
        conduit: conduit,
        dispatcher: dispatcher,
        role: .acceptor,
        negotiated: negotiated,
        peerSupportsRetry: peerSupportsRetry,
        acceptConnections: acceptConnections,
        keepalive: keepalive
    )
}
