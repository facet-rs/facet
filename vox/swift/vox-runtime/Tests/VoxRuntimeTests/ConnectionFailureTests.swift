import Foundation
import PhonSchema
import Testing

@testable import VoxRuntime

// Test shims for the removed CBOR-era `Message` static factories + encode/decode: map
// them onto the phon `message*` free functions + the phon envelope codec, so the
// scripted-transport tests read unchanged. (Metadata is now a phon `Value`.)
extension Message {
    static func request(
        laneId: UInt64,
        requestId: UInt64,
        methodId: UInt64,
        metadata: Metadata,
        payload: [UInt8],
        channels: [UInt64] = []
    ) -> Message {
        messageRequest(
            requestId: requestId, methodId: methodId, payload: payload, metadata: metadata,
            channels: channels, laneId: laneId)
    }
    static func response(
        laneId: UInt64, requestId: UInt64, metadata: Metadata, payload: [UInt8]
    ) -> Message {
        messageResponse(
            requestId: requestId, payload: payload, metadata: metadata, laneId: laneId)
    }
    static func data(laneId: UInt64, channelId: UInt64, payload: [UInt8]) -> Message {
        messageData(channelId: channelId, item: payload, laneId: laneId)
    }
    static func pong(_ pong: Pong) -> Message { messagePong(nonce: pong.nonce) }

    func encode() -> [UInt8] { encodeMessage(self) }
    // Decode uses our own advertised Message schema (writer == reader here).
    static func decode(fromBytes bytes: [UInt8]) throws -> Message {
        try buildMessageDecoder(peerMessageSchema: MessageSchemaClosure)(bytes)
    }
}

/// Build a `Metadata` (phon `Value`) from key/value string pairs for scripted tests.
private func meta(_ pairs: [(String, String)]) -> Metadata {
    var m: Metadata = .null
    for (k, v) in pairs { m.metaSet(k, .string(v)) }
    return m
}

// The stub dispatchers below never originate a runtime error, so a no-op encoder
// satisfies the protocol requirement without a method/response context.
extension ServiceDispatcher {
    func encodeVoxError(_: VoxRuntimeError) -> [UInt8] { [] }
}

private enum TestTransportError: Error {
    case sendFailed
}

private enum AsyncTestError: Error {
    case timedOut
}

private let peepsMethodNameMetadataKey = "moire.method_name"
private let peepsRequestEntityIdMetadataKey = "moire.request_entity_id"
private let peepsConnectionCorrelationIdMetadataKey = "moire.connection_correlation_id"

private enum InboundEvent: Sendable {
    case frame([UInt8])
    case closed
}

private enum SentFrame: Sendable {
    case raw([UInt8])
    case handshake(HandshakeMessage)
    case message(Message)
}

private actor ScriptedTransport: Link {
    private var sentFrames: [SentFrame] = []
    private var sentMessages: [Message] = []
    private var sentHandshakes: [HandshakeMessage] = []
    private var inboundQueue: [InboundEvent] = []
    private var recvWaiters: [CheckedContinuation<InboundEvent, Never>] = []

    private var failNextRequestSend = false
    private let autoRespondRequestCount: Int
    private let dropAfterRequestCount: Int?
    private let autoRespondPing: Bool
    private var requestSends = 0
    private var didClose = false

    init(
        autoRespondRequestCount: Int = 0,
        dropAfterRequestCount: Int? = nil,
        autoRespondPing: Bool = false,
        initialHandshake: HandshakeMessage? = .helloYourself(
            HelloYourself(
                connectionSettings: ConnectionSettings(parity: .even, maxConcurrentRequests: 64, initialChannelCredit: 16),
                messagePayloadSchema: Data(MessageSchemaClosure),
                metadata: .null
            ))
    ) {
        self.autoRespondRequestCount = autoRespondRequestCount
        self.dropAfterRequestCount = dropAfterRequestCount
        self.autoRespondPing = autoRespondPing
        if let initialHandshake {
            if case .hello = initialHandshake {
                inboundQueue.append(.frame(encodeTransportHello()))
            }
            inboundQueue.append(.frame(encodeHandshakeFrame(initialHandshake)))
            if case .hello = initialHandshake {
                inboundQueue.append(.frame(encodeHandshakeFrame(.letsGo(LetsGo()))))
            }
        }
    }

    func setFailNextRequestSend() {
        failNextRequestSend = true
    }

    func enqueueMessage(_ message: Message) {
        enqueueInbound(.frame(message.encode()))
    }

    func enqueueRaw(_ bytes: [UInt8]) {
        enqueueInbound(.frame(bytes))
    }

    func enqueueHandshake(_ handshake: HandshakeMessage) {
        enqueueInbound(.frame(encodeHandshakeFrame(handshake)))
    }

    func sent() -> [Message] {
        sentMessages
    }

    func sentHandshakeMessages() -> [HandshakeMessage] {
        sentHandshakes
    }

    func sentRequestIds() -> [UInt64] {
        sentMessages.compactMap { message in
            if case .requestMessage(let request) = message.payload,
                case .call = request.body
            {
                return request.id
            }
            return nil
        }
    }

    func sendFrame(_ bytes: [UInt8]) async throws {
        if bytes.count == 8,
            Array(bytes[0..<4]) == Array("VOTH".utf8)
                || Array(bytes[0..<4]) == Array("VOTA".utf8)
                || Array(bytes[0..<4]) == Array("VOTR".utf8)
        {
            sentFrames.append(.raw(bytes))
            return
        }

        if let handshake = try? decodeHandshakeFrame(bytes) {
            sentFrames.append(.handshake(handshake))
            sentHandshakes.append(handshake)
            return
        }

        let message = try Message.decode(fromBytes: bytes)
        sentFrames.append(.message(message))
        sentMessages.append(message)

        if case .ping(let ping) = message.payload {
            if autoRespondPing {
                enqueueInbound(.frame(Message.pong(.init(nonce: ping.nonce)).encode()))
            }
            return
        }

        if case .requestMessage(let request) = message.payload,
            case .call = request.body
        {
            let requestId = request.id
            if failNextRequestSend {
                failNextRequestSend = false
                throw TestTransportError.sendFailed
            }

            requestSends += 1

            if requestSends <= autoRespondRequestCount {
                enqueueInbound(
                    .frame(
                        Message.response(
                            laneId: 0,
                            requestId: requestId,
                            metadata: .null,
                            payload: [0]
                        ).encode()
                    )
                )
            }

            if let dropAfterRequestCount, requestSends == dropAfterRequestCount {
                didClose = true
                enqueueInbound(.closed)
            }
        }
    }

    func recvFrame() async throws -> [UInt8]? {
        let event: InboundEvent
        if !inboundQueue.isEmpty {
            event = inboundQueue.removeFirst()
        } else {
            event = await withCheckedContinuation { continuation in
                recvWaiters.append(continuation)
            }
        }

        switch event {
        case .frame(let bytes):
            return bytes
        case .closed:
            return nil
        }
    }

    func setMaxFrameSize(_: Int) async throws {}

    func close() async throws {
        guard !didClose else {
            return
        }
        didClose = true
        enqueueInbound(.closed)
        for waiter in recvWaiters {
            waiter.resume(returning: .closed)
        }
        recvWaiters.removeAll()
    }

    private func enqueueInbound(_ event: InboundEvent) {
        if let waiter = recvWaiters.first {
            recvWaiters.removeFirst()
            waiter.resume(returning: event)
            return
        }
        inboundQueue.append(event)
    }
}

private struct EmptyServiceDispatcher: ServiceDispatcher {
    func preregister(
        methodId _: UInt64,
        payload _: [UInt8],
        channels _: [UInt64],
        registry _: ChannelRegistry
    ) async {}

    func dispatch(
        methodId _: UInt64,
        payload _: [UInt8],
        requestId _: UInt64,
        channels _: [UInt64],
        registry _: ChannelRegistry,
        schemaSendTracker _: SchemaSendTracker,
        schemaReceiveTracker _: SchemaTracker,
        context _: RequestContext,
        taskTx _: @escaping @Sendable (TaskMessage) -> Void
    ) async {}
}

private struct ScriptedConnector: ConnectionConnector {
    let transport: ScriptedTransport
    var peerEvidence: PeerEvidence = .none

    func openAttachment() async throws -> LinkAttachment {
        .initiator(transport, peerEvidence: peerEvidence)
    }
}

private final class RecordingRuntimeObserver: VoxRuntimeObserver, @unchecked Sendable {
    private let lock = NSLock()
    private var events: [VoxDriverObserverEvent] = []
    private var establishmentEvents: [VoxEstablishmentObserverEvent] = []

    func driverEvent(_ event: VoxDriverObserverEvent) {
        lock.lock()
        events.append(event)
        lock.unlock()
    }

    func establishmentEvent(_ event: VoxEstablishmentObserverEvent) {
        lock.lock()
        establishmentEvents.append(event)
        lock.unlock()
    }

    func snapshot() -> [VoxDriverObserverEvent] {
        lock.lock()
        defer { lock.unlock() }
        return events
    }

    func establishmentSnapshot() -> [VoxEstablishmentObserverEvent] {
        lock.lock()
        defer { lock.unlock() }
        return establishmentEvents
    }
}

private func establishmentLabels(_ events: [VoxEstablishmentObserverEvent]) -> [String] {
    events.map { event in
        switch event {
        case .started(let context):
            return
                "started:\(context.role.rawValue):\(context.phase.rawValue):"
                + "\(context.laneId.map(String.init) ?? "-"):-"
        case .finished(let context, let outcome, _, _):
            return
                "finished:\(context.role.rawValue):\(context.phase.rawValue):"
                + "\(context.laneId.map(String.init) ?? "-"):\(outcome.rawValue)"
        }
    }
}

@Test
// r[verify connection.protocol]
// r[verify connection.model]
// r[verify lane.control.compat]
// r[verify lane.control]
// r[verify rpc.connection-setup]
// r[verify connection.peer]
// r[verify connection.role]
// r[verify schema.interaction.metadata]
// r[verify rpc.metadata.records]
func acceptorConnectionExposesPeerHandshakeMetadata() async throws {
    let metadata = meta([("vixenfs-sid", "abc123")])
    let link = ScriptedTransport(
        initialHandshake: .hello(
            Hello(
                parity: .odd,
                connectionSettings: ConnectionSettings(parity: .odd, maxConcurrentRequests: 64, initialChannelCredit: 16),
                messagePayloadSchema: Data(MessageSchemaClosure),
                metadata: metadata
            ))
    )

    let connection = try await Connection.accept(
        freshLink: link,
        controlDispatcher: EmptyServiceDispatcher()
    )

    #expect(connection.controlLane.laneId == 0)
    #expect(connection.role == .acceptor)
    #expect(connection.driver.role == .acceptor)
    #expect(connection.driver.dispatcher is EmptyServiceDispatcher)
    #expect(connection.controlLane.handle === connection.driver.handle)
    #expect(connection.peerMetadata == metadata)
}

@Test
// r[verify connection.handshake.metadata]
// r[verify connection.evidence]
// r[verify connection.identity]
// r[verify connection.identity.forms]
// r[verify connection.identity.inputs]
// r[verify connection.identity.local]
// r[verify connection.identity.redaction]
// r[verify connection.identity.scope]
// r[verify connection.identity.use-cases]
// r[verify connection.policy.establishment]
func initiatorResolvesPeerIdentityFromEvidenceAndHandshakeMetadata() async throws {
    let metadata = meta([
        ("server-auth", "token-ok"),
        ("traceparent", "redacted-by-policy"),
    ])
    let link = ScriptedTransport(
        initialHandshake: .helloYourself(
            HelloYourself(
                connectionSettings: ConnectionSettings(
                    parity: .even,
                    maxConcurrentRequests: 64,
                    initialChannelCredit: 16
                ),
                messagePayloadSchema: Data(MessageSchemaClosure),
                metadata: metadata
            ))
    )

    let connection = try await Connection.connect(
        ScriptedConnector(transport: link, peerEvidence: .synthetic("memory-link")),
        controlDispatcher: EmptyServiceDispatcher(),
        identityResolver: { context in
            #expect(context.role == .initiator)
            #expect(context.claims.metaStr("server-auth") == "token-ok")
            if case .synthetic(label: let label) = context.evidence.items.first {
                #expect(label == "memory-link")
            } else {
                Issue.record("expected synthetic peer evidence")
            }
            return PeerIdentity.composite([
                IdentityBasis(
                    form: .synthetic,
                    provenance: .evidenceBacked,
                    redacted: "memory-link"
                ),
                IdentityBasis(
                    form: .applicationUser,
                    provenance: .verifiedClaimBacked,
                    redacted: "user:7"
                ),
            ])
        }
    )

    #expect(connection.peerMetadata.metaStr("server-auth") == "token-ok")
    if case .synthetic(label: let label) = connection.peerEvidence.items.first {
        #expect(label == "memory-link")
    } else {
        Issue.record("expected connection peer evidence")
    }
    #expect(connection.peerIdentity.epoch == 0)
    #expect(connection.peerIdentity.form == .composite)
    #expect(connection.peerIdentity.bases == [
        IdentityBasis(form: .synthetic, provenance: .evidenceBacked, redacted: "memory-link"),
        IdentityBasis(form: .applicationUser, provenance: .verifiedClaimBacked, redacted: "user:7"),
    ])
}

@Test
// r[verify connection.handshake.sorry]
func acceptorSendsSorryForInvalidPeerMessageSchema() async throws {
    let link = ScriptedTransport(
        initialHandshake: .hello(
            Hello(
                parity: .odd,
                connectionSettings: ConnectionSettings(
                    parity: .odd,
                    maxConcurrentRequests: 64,
                    initialChannelCredit: 16
                ),
                messagePayloadSchema: Data([0xFF, 0x00, 0xFF]),
                metadata: .null
            ))
    )

    do {
        _ = try await Connection.accept(
            freshLink: link,
            controlDispatcher: EmptyServiceDispatcher()
        )
        Issue.record("expected handshake rejection")
    } catch ConnectionError.handshakeFailed(let reason) {
        #expect(reason == "unsupported message compatibility plan")
    } catch {
        Issue.record("expected handshakeFailed, got \(String(describing: error))")
    }

    let sent = await link.sentHandshakeMessages()
    guard case .sorry(let sorry) = sent.first else {
        Issue.record("expected Sorry handshake")
        return
    }
    #expect(sorry.reason == "unsupported message compatibility plan")
}

@Test
// r[verify connection.handshake.decline]
// r[verify connection.policy.establishment.rejection]
// r[verify connection.identity.resolver]
// r[verify rejection.reason.taxonomy]
func acceptorSendsDeclineWhenIdentityResolverRejectsInitiatorClaims() async throws {
    let link = ScriptedTransport(
        initialHandshake: .hello(
            Hello(
                parity: .odd,
                connectionSettings: ConnectionSettings(
                    parity: .odd,
                    maxConcurrentRequests: 64,
                    initialChannelCredit: 16
                ),
                messagePayloadSchema: Data(MessageSchemaClosure),
                metadata: meta([("auth", "nope")])
            ))
    )

    do {
        _ = try await Connection.accept(
            .fresh(link, peerEvidence: .synthetic("memory-link")),
            controlDispatcher: EmptyServiceDispatcher(),
            identityResolver: { context in
                #expect(context.role == .acceptor)
                #expect(context.claims.metaStr("auth") == "nope")
                throw ConnectionDeclinedError(reason: .forbidden)
            }
        )
        Issue.record("expected connection decline")
    } catch let declined as ConnectionDeclinedError {
        #expect(declined.decline.reason == .forbidden)
        #expect(declined.receivedFromPeer == false)
    } catch {
        Issue.record("expected ConnectionDeclinedError, got \(String(describing: error))")
    }

    let sent = await link.sentHandshakeMessages()
    guard case .decline(let decline) = sent.first else {
        Issue.record("expected Decline handshake")
        return
    }
    #expect(decline.reason == .forbidden)
}

@Test
// r[verify connection.handshake.decline]
// r[verify connection.policy.establishment.rejection]
// r[verify connection.identity.resolver]
func initiatorSendsDeclineWhenIdentityResolverRejectsAcceptorClaims() async throws {
    let link = ScriptedTransport(
        initialHandshake: .helloYourself(
            HelloYourself(
                connectionSettings: ConnectionSettings(
                    parity: .even,
                    maxConcurrentRequests: 64,
                    initialChannelCredit: 16
                ),
                messagePayloadSchema: Data(MessageSchemaClosure),
                metadata: meta([("server-auth", "bad")])
            ))
    )

    do {
        _ = try await Connection.connect(
            ScriptedConnector(transport: link, peerEvidence: .synthetic("memory-link")),
            controlDispatcher: EmptyServiceDispatcher(),
            identityResolver: { context in
                #expect(context.role == .initiator)
                #expect(context.claims.metaStr("server-auth") == "bad")
                throw ConnectionDeclinedError(reason: .forbidden)
            }
        )
        Issue.record("expected connection decline")
    } catch let declined as ConnectionDeclinedError {
        #expect(declined.decline.reason == .forbidden)
        #expect(declined.receivedFromPeer == false)
    } catch {
        Issue.record("expected ConnectionDeclinedError, got \(String(describing: error))")
    }

    let sent = await link.sentHandshakeMessages()
    guard sent.contains(where: {
        if case .decline(let decline) = $0 {
            return decline.reason == .forbidden
        }
        return false
    }) else {
        Issue.record("expected Decline handshake")
        return
    }
}

// r[verify transport.prologue.first-payload]
// r[verify transport.prologue.post-accept]
@Test func acceptorConnectionConsumesTransportPrologueBeforeHandshake() async throws {
    let metadata = meta([("vixenfs-sid", "abc123")])
    let link = ScriptedTransport(initialHandshake: nil)
    await link.enqueueRaw(encodeTransportHello())
    await link.enqueueHandshake(
        .hello(
            Hello(
                parity: .odd,
                connectionSettings: ConnectionSettings(parity: .odd, maxConcurrentRequests: 64, initialChannelCredit: 16),
                messagePayloadSchema: Data(MessageSchemaClosure),
                metadata: metadata
            ))
    )
    await link.enqueueHandshake(.letsGo(LetsGo()))

    let connection = try await Connection.accept(
        freshLink: link,
        controlDispatcher: EmptyServiceDispatcher()
    )

    #expect(connection.peerMetadata == metadata)
}

private struct ImmediateResponseDispatcher: ServiceDispatcher {
    func preregister(
        methodId _: UInt64,
        payload _: [UInt8],
        channels _: [UInt64],
        registry _: ChannelRegistry
    ) async {}

    func dispatch(
        methodId _: UInt64,
        payload _: [UInt8],
        requestId: UInt64,
        channels _: [UInt64],
        registry _: ChannelRegistry,
        schemaSendTracker _: SchemaSendTracker,
        schemaReceiveTracker _: SchemaTracker,
        context _: RequestContext,
        taskTx: @escaping @Sendable (TaskMessage) -> Void
    ) async {
        taskTx(.response(requestId: requestId, payload: [0x01]))
    }
}

private struct AuthorizationProbeDispatcher: ServiceDispatcher {
    func preregister(
        methodId _: UInt64,
        payload _: [UInt8],
        channels _: [UInt64],
        registry _: ChannelRegistry
    ) async {}

    func dispatch(
        methodId _: UInt64,
        payload _: [UInt8],
        requestId: UInt64,
        channels _: [UInt64],
        registry _: ChannelRegistry,
        schemaSendTracker _: SchemaSendTracker,
        schemaReceiveTracker _: SchemaTracker,
        context: RequestContext,
        taskTx: @escaping @Sendable (TaskMessage) -> Void
    ) async {
        #expect(context.authorization.peerIdentity.form == .anonymous)
        #expect(context.authorization.peerEvidence.items.isEmpty)
        #expect(context.authorization.laneGrant.metadata.metaStr("grant-scope") == "swift-probe")
        taskTx(.response(requestId: requestId, payload: [0x01]))
    }
}

private struct GrantingLaneAcceptor: LaneAcceptor {
    let dispatcher: any ServiceDispatcher

    func accept(request: LaneRequest, lane: PendingLane) {
        #expect(request.service == "Noop")
        lane.handleWith(
            dispatcher,
            grant: LaneGrant(metadata: meta([("grant-scope", "swift-probe")]))
        )
    }
}

private struct PipeliningDispatcher: ServiceDispatcher {
    func preregister(
        methodId _: UInt64,
        payload _: [UInt8],
        channels _: [UInt64],
        registry _: ChannelRegistry
    ) async {}

    func dispatch(
        methodId: UInt64,
        payload _: [UInt8],
        requestId: UInt64,
        channels _: [UInt64],
        registry _: ChannelRegistry,
        schemaSendTracker _: SchemaSendTracker,
        schemaReceiveTracker _: SchemaTracker,
        context _: RequestContext,
        taskTx: @escaping @Sendable (TaskMessage) -> Void
    ) async {
        if methodId == 1 {
            try? await Task.sleep(nanoseconds: 200_000_000)
            taskTx(.response(requestId: requestId, payload: [0x01]))
        } else {
            taskTx(.response(requestId: requestId, payload: [0x02]))
        }
    }
}

private actor BlockingRequestGate {
    private var started = false
    private var released = false

    func markStarted() {
        started = true
    }

    func hasStarted() -> Bool {
        started
    }

    func release() {
        released = true
    }

    func waitUntilReleased() async {
        while !released {
            try? await Task.sleep(nanoseconds: 5_000_000)
        }
    }
}

private struct BlockingFlowControlDispatcher: ServiceDispatcher {
    let gate: BlockingRequestGate

    func preregister(
        methodId _: UInt64,
        payload _: [UInt8],
        channels _: [UInt64],
        registry _: ChannelRegistry
    ) async {}

    func dispatch(
        methodId _: UInt64,
        payload _: [UInt8],
        requestId: UInt64,
        channels _: [UInt64],
        registry _: ChannelRegistry,
        schemaSendTracker _: SchemaSendTracker,
        schemaReceiveTracker _: SchemaTracker,
        context _: RequestContext,
        taskTx: @escaping @Sendable (TaskMessage) -> Void
    ) async {
        await gate.markStarted()
        await gate.waitUntilReleased()
        taskTx(.response(requestId: requestId, payload: [0x01]))
    }
}

private actor ChannelReceiverCapture {
    private var receiver: ChannelReceiver?

    func set(_ receiver: ChannelReceiver) {
        self.receiver = receiver
    }

    func get() -> ChannelReceiver? {
        receiver
    }
}

private struct CancelChannelDispatcher: ServiceDispatcher {
    let capture: ChannelReceiverCapture

    func preregister(
        methodId _: UInt64,
        payload _: [UInt8],
        channels: [UInt64],
        registry: ChannelRegistry
    ) async {
        guard let channelId = channels.first else {
            return
        }
        let receiver = await registry.register(
            channelId,
            initialCredit: defaultInitialChannelCredit
        )
        await capture.set(receiver)
    }

    func dispatch(
        methodId _: UInt64,
        payload _: [UInt8],
        requestId _: UInt64,
        channels _: [UInt64],
        registry _: ChannelRegistry,
        schemaSendTracker _: SchemaSendTracker,
        schemaReceiveTracker _: SchemaTracker,
        context _: RequestContext,
        taskTx _: @escaping @Sendable (TaskMessage) -> Void
    ) async {}
}

private func metadataString(_ metadata: Metadata, key: String) -> String? {
    metadata.metaStr(key)
}

private func awaitHasCancel(
    _ transport: ScriptedTransport,
    timeoutMs: UInt64 = 500
) async -> Bool {
    let start = ContinuousClock.now
    let timeout = Duration.milliseconds(Int64(timeoutMs))
    while ContinuousClock.now - start < timeout {
        let sent = await transport.sent()
        if sent.contains(where: { message in
            if case .requestMessage(let request) = message.payload,
                case .cancel = request.body
            {
                return true
            }
            return false
        }) {
            return true
        }
        try? await Task.sleep(nanoseconds: 5_000_000)
    }
    return false
}

private func awaitRequestId(
    _ transport: ScriptedTransport,
    index: Int,
    timeoutMs: UInt64 = 1_000
) async -> UInt64? {
    let start = ContinuousClock.now
    let timeout = Duration.milliseconds(Int64(timeoutMs))
    while ContinuousClock.now - start < timeout {
        let requestIds = await transport.sentRequestIds()
        if requestIds.count > index {
            return requestIds[index]
        }
        try? await Task.sleep(nanoseconds: 5_000_000)
    }
    return nil
}

private func awaitLaneOpenId(
    _ transport: ScriptedTransport,
    timeoutMs: UInt64 = 1_000
) async -> UInt64? {
    let start = ContinuousClock.now
    let timeout = Duration.milliseconds(Int64(timeoutMs))
    while ContinuousClock.now - start < timeout {
        let sent = await transport.sent()
        for message in sent {
            if case .laneOpen = message.payload {
                return message.laneId
            }
        }
        try? await Task.sleep(nanoseconds: 5_000_000)
    }
    return nil
}

private func awaitLaneOpenId(
    _ transport: ScriptedTransport,
    excluding excluded: Set<UInt64>,
    timeoutMs: UInt64 = 1_000
) async -> UInt64? {
    let start = ContinuousClock.now
    let timeout = Duration.milliseconds(Int64(timeoutMs))
    while ContinuousClock.now - start < timeout {
        let sent = await transport.sent()
        for message in sent {
            if case .laneOpen = message.payload, !excluded.contains(message.laneId) {
                return message.laneId
            }
        }
        try? await Task.sleep(nanoseconds: 5_000_000)
    }
    return nil
}

private func awaitLaneAccept(
    _ transport: ScriptedTransport,
    laneId: UInt64,
    timeoutMs: UInt64 = 1_000
) async -> LaneAccept? {
    let start = ContinuousClock.now
    let timeout = Duration.milliseconds(Int64(timeoutMs))
    while ContinuousClock.now - start < timeout {
        let sent = await transport.sent()
        for message in sent where message.laneId == laneId {
            if case .laneAccept(let accept) = message.payload {
                return accept
            }
        }
        try? await Task.sleep(nanoseconds: 5_000_000)
    }
    return nil
}

private func awaitLaneReject(
    _ transport: ScriptedTransport,
    laneId: UInt64,
    timeoutMs: UInt64 = 1_000
) async -> LaneReject? {
    let start = ContinuousClock.now
    let timeout = Duration.milliseconds(Int64(timeoutMs))
    while ContinuousClock.now - start < timeout {
        let sent = await transport.sent()
        for message in sent where message.laneId == laneId {
            if case .laneReject(let reject) = message.payload {
                return reject
            }
        }
        try? await Task.sleep(nanoseconds: 5_000_000)
    }
    return nil
}

private func awaitLaneClose(
    _ transport: ScriptedTransport,
    laneId: UInt64,
    timeoutMs: UInt64 = 1_000
) async -> LaneClose? {
    let start = ContinuousClock.now
    let timeout = Duration.milliseconds(Int64(timeoutMs))
    while ContinuousClock.now - start < timeout {
        let sent = await transport.sent()
        for message in sent where message.laneId == laneId {
            if case .laneClose(let close) = message.payload {
                return close
            }
        }
        try? await Task.sleep(nanoseconds: 5_000_000)
    }
    return nil
}

private func awaitRequest(
    _ transport: ScriptedTransport,
    laneId: UInt64,
    timeoutMs: UInt64 = 1_000
) async -> RequestMessage? {
    let start = ContinuousClock.now
    let timeout = Duration.milliseconds(Int64(timeoutMs))
    while ContinuousClock.now - start < timeout {
        let sent = await transport.sent()
        for message in sent where message.laneId == laneId {
            if case .requestMessage(let request) = message.payload,
                case .call = request.body
            {
                return request
            }
        }
        try? await Task.sleep(nanoseconds: 5_000_000)
    }
    return nil
}

private func awaitResponsePayload(
    _ transport: ScriptedTransport,
    laneId: UInt64,
    requestId: UInt64,
    timeoutMs: UInt64 = 1_000
) async -> [UInt8]? {
    let start = ContinuousClock.now
    let timeout = Duration.milliseconds(Int64(timeoutMs))
    while ContinuousClock.now - start < timeout {
        let sent = await transport.sent()
        for message in sent where message.laneId == laneId {
            if case .requestMessage(let request) = message.payload,
                request.id == requestId,
                case .response(let response) = request.body
            {
                return [UInt8](response.ret)
            }
        }
        try? await Task.sleep(nanoseconds: 5_000_000)
    }
    return nil
}

private func awaitProtocolReason(
    _ transport: ScriptedTransport,
    timeoutMs: UInt64 = 1_000
) async -> String? {
    let start = ContinuousClock.now
    let timeout = Duration.milliseconds(Int64(timeoutMs))
    while ContinuousClock.now - start < timeout {
        let sent = await transport.sent()
        for msg in sent {
            if case .protocolError(let err) = msg.payload {
                return err.description
            }
        }
        try? await Task.sleep(nanoseconds: 5_000_000)
    }
    return nil
}

private func isConnectionClosed(_ error: Error) -> Bool {
    guard let connError = error as? ConnectionError else {
        return false
    }
    if case .connectionClosed = connError {
        return true
    }
    return false
}

private func isTransportError(_ error: Error) -> Bool {
    guard let connError = error as? ConnectionError else {
        return false
    }
    if case .transportError = connError {
        return true
    }
    return false
}

private func isTimeout(_ error: Error) -> Bool {
    guard let connError = error as? ConnectionError else {
        return false
    }
    if case .timeout = connError {
        return true
    }
    return false
}

private func laneRejection(_ error: Error) -> LaneRejection? {
    guard let connError = error as? ConnectionError else {
        return nil
    }
    if case .rejected(let rejection) = connError {
        return rejection
    }
    return nil
}

private func isProtocolViolation(_ error: Error, rule: String) -> Bool {
    guard let connError = error as? ConnectionError else {
        return false
    }
    if case .protocolViolation(let actualRule) = connError {
        return actualRule == rule
    }
    return false
}

private func awaitSentPingNonce(
    _ transport: ScriptedTransport,
    timeoutMs: UInt64 = 1_000
) async -> UInt64? {
    let start = ContinuousClock.now
    let timeout = Duration.milliseconds(Int64(timeoutMs))
    while ContinuousClock.now - start < timeout {
        let sent = await transport.sent()
        for message in sent {
            if case .ping(let ping) = message.payload {
                return ping.nonce
            }
        }
        try? await Task.sleep(nanoseconds: 5_000_000)
    }
    return nil
}

private func awaitTaskResult<T: Sendable>(
    _ task: Task<T, Error>,
    timeoutMs: UInt64 = 1_000
) async throws -> T {
    try await withThrowingTaskGroup(of: T.self) { group in
        group.addTask {
            try await task.value
        }
        group.addTask {
            try await Task.sleep(nanoseconds: timeoutMs * 1_000_000)
            throw AsyncTestError.timedOut
        }
        guard let result = try await group.next() else {
            throw AsyncTestError.timedOut
        }
        group.cancelAll()
        return result
    }
}

@Suite(.serialized)
struct ConnectionFailureTests {
    // r[verify rpc]
    // r[verify rpc.observability.runtime]
    // r[verify rpc.observability.driver]
    @Test func runtimeObserverReceivesDriverLifecycleEventsWithoutTelemetryBackend() async throws {
        let observer = RecordingRuntimeObserver()
        try await withVoxRuntimeObserverForTest(observer) {
            let transport = ScriptedTransport()
            let (_, driver, _, _) = try await establishInitiator(
                conduit: transport,
                dispatcher: EmptyServiceDispatcher()
            )
            #expect(voxRuntimeObserver() != nil)

            let driverTask: Task<Void, Error> = Task {
                try await driver.run()
            }
            try await withAsyncCleanup({
                try? await transport.close()
                await cancelAndDrain(driverTask)
            }) {
                try? await transport.close()
                try await awaitTaskResult(driverTask)

                let events = observer.snapshot()
                #expect(events.contains(.runStarted))
                #expect(events.contains(.readerClosed))
                #expect(events.contains(.runExited))
            }
        }
    }

    // r[verify rpc.observability.establishment]
    @Test func establishmentObserverReceivesDirectLinkHandshakeAndSchemaPhases() async throws {
        let observer = RecordingRuntimeObserver()
        try await withVoxRuntimeObserverForTest(observer) {
            let transport = ScriptedTransport()
            _ = try await establishInitiator(
                conduit: transport,
                dispatcher: EmptyServiceDispatcher()
            )

            let labels = establishmentLabels(observer.establishmentSnapshot())
            for expected in [
                "started:initiator:connection-handshake:-:-",
                "started:initiator:identity-resolution:-:-",
                "started:initiator:connection-policy:-:-",
                "finished:initiator:connection-policy:-:ok",
                "finished:initiator:identity-resolution:-:ok",
                "finished:initiator:connection-handshake:-:ok",
                "started:initiator:schema-decode-plan:-:-",
                "finished:initiator:schema-decode-plan:-:ok",
            ] {
                #expect(labels.contains(expected))
            }
        }
    }

    // r[verify rpc.observability.establishment]
    @Test func establishmentObserverReceivesAcceptorTransportPrologue() async throws {
        let observer = RecordingRuntimeObserver()
        try await withVoxRuntimeObserverForTest(observer) {
            let transport = ScriptedTransport(
                initialHandshake: .hello(
                    Hello(
                        parity: .odd,
                        connectionSettings: ConnectionSettings(
                            parity: .odd,
                            maxConcurrentRequests: 64,
                            initialChannelCredit: 16
                        ),
                        messagePayloadSchema: Data(MessageSchemaClosure),
                        metadata: .null
                    ))
            )
            _ = try await Connection.accept(
                freshLink: transport,
                controlDispatcher: EmptyServiceDispatcher()
            )

            #expect(establishmentLabels(observer.establishmentSnapshot()) == [
                "started:acceptor:transport-prologue:-:-",
                "finished:acceptor:transport-prologue:-:ok",
                "started:acceptor:connection-handshake:-:-",
                "started:acceptor:identity-resolution:-:-",
                "started:acceptor:connection-policy:-:-",
                "finished:acceptor:connection-policy:-:ok",
                "finished:acceptor:identity-resolution:-:ok",
                "finished:acceptor:connection-handshake:-:ok",
                "started:acceptor:schema-decode-plan:-:-",
                "finished:acceptor:schema-decode-plan:-:ok",
            ])
        }
    }

    // r[verify rpc.observability.establishment]
    @Test func establishmentObserverReceivesServiceLaneOpenOutcomes() async throws {
        let observer = RecordingRuntimeObserver()
        try await withVoxRuntimeObserverForTest(observer) {
            let transport = ScriptedTransport()
            let (_, driver, connectionHandle, _) = try await establishInitiator(
                conduit: transport,
                dispatcher: EmptyServiceDispatcher()
            )
            let driverTask: Task<Void, Error> = Task {
                try await driver.run()
            }
            try await withAsyncCleanup({
                try? await transport.close()
                await cancelAndDrain(driverTask)
            }) {
                let localSettings = ConnectionSettings(
                    parity: .even,
                    maxConcurrentRequests: 8,
                    initialChannelCredit: 16
                )
                let acceptedTask: Task<Lane, Error> = Task {
                    try await connectionHandle.openLane(
                        settings: localSettings,
                        metadata: meta([("vox-service", "Noop")])
                    )
                }
                guard let acceptedId = await awaitLaneOpenId(transport) else {
                    Issue.record("expected accepted LaneOpen")
                    return
                }
                await transport.enqueueMessage(
                    messageLaneAccept(
                        laneId: acceptedId,
                        settings: ConnectionSettings(
                            parity: .odd,
                            maxConcurrentRequests: 8,
                            initialChannelCredit: 16
                        ),
                        metadata: meta([("grant-scope", "observer")])
                    )
                )
                _ = try await awaitTaskResult(acceptedTask)
                try await connectionHandle.closeLane(acceptedId)
                guard await awaitLaneClose(transport, laneId: acceptedId) != nil else {
                    Issue.record("expected accepted lane close")
                    return
                }

                let rejectedTask: Task<Lane, Error> = Task {
                    try await connectionHandle.openLane(
                        settings: localSettings,
                        metadata: meta([("vox-service", "Noop")])
                    )
                }
                guard
                    let rejectedId = await awaitLaneOpenId(
                        transport,
                        excluding: [acceptedId]
                    )
                else {
                    Issue.record("expected rejected LaneOpen")
                    return
                }
                await transport.enqueueMessage(
                    messageLaneReject(
                        laneId: rejectedId,
                        metadata: LaneRejection.withMessage(.unknownService, "missing")
                            .toMetadata()
                    )
                )
                do {
                    _ = try await awaitTaskResult(rejectedTask)
                    Issue.record("expected service lane rejection")
                } catch {}

                let labels = establishmentLabels(
                    observer.establishmentSnapshot().filter { event in
                        switch event {
                        case .started(let context):
                            return context.phase == .serviceLaneOpen
                        case .finished(let context, _, _, _):
                            return context.phase == .serviceLaneOpen
                        }
                    }
                )
                #expect(labels == [
                    "started:initiator:service-lane-open:1:-",
                    "finished:initiator:service-lane-open:1:ok",
                    "started:initiator:service-lane-open:3:-",
                    "finished:initiator:service-lane-open:3:rejected",
                ])

                let grantLabels = establishmentLabels(
                    observer.establishmentSnapshot().filter { event in
                        switch event {
                        case .started(let context):
                            return context.phase == .laneGrant
                                || context.phase == .laneGrantRevocation
                        case .finished(let context, _, _, _):
                            return context.phase == .laneGrant
                                || context.phase == .laneGrantRevocation
                        }
                    }
                )
                #expect(grantLabels == [
                    "started:initiator:lane-grant:1:-",
                    "finished:initiator:lane-grant:1:ok",
                    "started:initiator:lane-grant-revocation:1:-",
                    "finished:initiator:lane-grant-revocation:1:ok",
                ])
            }
        }
    }

    // r[verify connection.handshake]
    // r[verify connection.handshake.phon]
    // r[verify connection.handshake.protocol-schema]
    // r[verify connection.handshake.protocol-schema.connection-scoped]
    // r[verify connection.handshake.unversioned]
    // r[verify lane.settings]
    // r[verify connection.handshake.lane-settings]
    // r[verify connection.lane-id-parity]
    // r[verify rpc.flow-control.max-concurrent-requests.default]
    @Test func initiatorHelloCarriesMessagePayloadSchema() async throws {
        let transport = ScriptedTransport()
        _ = try await establishInitiator(
            conduit: transport,
            dispatcher: EmptyServiceDispatcher()
        )

        let sent = await transport.sentHandshakeMessages()
        guard let first = sent.first else {
            Issue.record("expected hello to be sent")
            return
        }
        guard case .hello(let hello) = first else {
            Issue.record("expected first sent message to be hello")
            return
        }
        #expect(hello.parity == .odd)
        #expect(hello.connectionSettings.parity == .odd)
        #expect(hello.connectionSettings.maxConcurrentRequests == 64)
        #expect(hello.connectionSettings.initialChannelCredit == 16)
        #expect(Array(hello.messagePayloadSchema) == MessageSchemaClosure)
    }

    // r[verify rpc.connection-setup]
    @Test func connectDoesNotInjectServiceMetadataIntoHandshake() async throws {
        let transport = ScriptedTransport()
        let connection = try await Connection.connect(
            ScriptedConnector(transport: transport),
            controlDispatcher: EmptyServiceDispatcher()
        )
        #expect(connection.controlLane.laneId == 0)

        let sent = await transport.sentHandshakeMessages()
        guard let first = sent.first, case .hello(let hello) = first else {
            Issue.record("expected hello to be sent")
            return
        }
        #expect(hello.metadata.metaStr("vox-service") == nil)
    }

    // r[verify rpc.connection-setup]
    @Test func connectPreservesExplicitHandshakeMetadata() async throws {
        let transport = ScriptedTransport()
        let metadata = meta([("prefix", "app")])
        let connection = try await Connection.connect(
            ScriptedConnector(transport: transport),
            controlDispatcher: EmptyServiceDispatcher(),
            metadata: metadata
        )
        #expect(connection.controlLane.laneId == 0)

        let sent = await transport.sentHandshakeMessages()
        guard let first = sent.first, case .hello(let hello) = first else {
            Issue.record("expected hello to be sent")
            return
        }
        #expect(hello.metadata.metaStr("vox-service") == nil)
        #expect(hello.metadata.metaStr("prefix") == "app")
    }

    // r[verify rpc.connection-setup]
    @Test func freshLinkConnectDoesNotInjectServiceMetadataIntoHandshake() async throws {
        let transport = ScriptedTransport()
        let connection = try await Connection.connect(
            overFreshLink: transport,
            controlDispatcher: EmptyServiceDispatcher()
        )
        #expect(connection.controlLane.laneId == 0)

        let sent = await transport.sentHandshakeMessages()
        guard let first = sent.first, case .hello(let hello) = first else {
            Issue.record("expected hello to be sent")
            return
        }
        #expect(hello.metadata.metaStr("vox-service") == nil)
    }

    // r[verify rpc.connection-setup]
    @Test func freshLinkConnectPreservesExplicitHandshakeMetadata() async throws {
        let transport = ScriptedTransport()
        let metadata = meta([("prefix", "app")])
        let connection = try await Connection.connect(
            overFreshLink: transport,
            controlDispatcher: EmptyServiceDispatcher(),
            metadata: metadata
        )
        #expect(connection.controlLane.laneId == 0)

        let sent = await transport.sentHandshakeMessages()
        guard let first = sent.first, case .hello(let hello) = first else {
            Issue.record("expected hello to be sent")
            return
        }
        #expect(hello.metadata.metaStr("vox-service") == nil)
        #expect(hello.metadata.metaStr("prefix") == "app")
    }

    // r[verify conduit]
    // r[verify conduit.bare]
    // r[verify conduit.typeplan]
    // r[verify connection.message]
    // r[verify connection.message.lane-id]
    // r[verify connection.message.payloads]
    // r[verify rpc.request]
    // r[verify rpc.response]
    @Test func serverResponsePreservesPeepsRequestMetadata() async throws {
        let transport = ScriptedTransport(
            initialHandshake: .hello(
                Hello(
                    parity: .odd,
                    connectionSettings: ConnectionSettings(parity: .odd, maxConcurrentRequests: 64, initialChannelCredit: 16),
                    messagePayloadSchema: Data(MessageSchemaClosure),
                    metadata: .null
                )))
        let (controlLane, driver, _, _) = try await establishAcceptor(
            conduit: transport,
            dispatcher: ImmediateResponseDispatcher()
        )
        #expect(controlLane.laneId == 0)
        let driverTask: Task<Void, Error> = Task {
            try await driver.run()
        }
        await withAsyncCleanup({
            try? await transport.close()
            await cancelAndDrain(driverTask)
        }) {
            await transport.enqueueMessage(
                .request(
                    laneId: 0,
                    requestId: 77,
                    methodId: 42,
                    metadata: meta([
                        (peepsMethodNameMetadataKey, "DemoRpc.test"),
                        (peepsRequestEntityIdMetadataKey, "request:abc"),
                        ("unrelated", "keep_out"),
                    ]),
                    payload: []
                )
            )

            let start = ContinuousClock.now
            let timeout = Duration.milliseconds(1_000)
            var responseMetadata: Metadata? = nil
            while ContinuousClock.now - start < timeout {
                let sent = await transport.sent()
                for message in sent {
                    if case .requestMessage(let request) = message.payload,
                        case .response(let response) = request.body,
                        request.id == 77
                    {
                        responseMetadata = response.metadata
                        break
                    }
                }
                if responseMetadata != nil {
                    break
                }
                try? await Task.sleep(nanoseconds: 5_000_000)
            }

            guard let responseMetadata else {
                Issue.record("expected response to be sent")
                return
            }

            #expect(
                metadataString(responseMetadata, key: peepsMethodNameMetadataKey) == "DemoRpc.test")
            #expect(
                metadataString(responseMetadata, key: peepsRequestEntityIdMetadataKey)
                    == "request:abc")
            #expect(metadataString(responseMetadata, key: "unrelated") == nil)
        }
    }

    // r[verify lane.id.compat]
    // r[verify lane.control.compat]
    // r[verify rpc.request]
    // r[verify rpc.response]
    @Test func immediateResponseAfterSendStillCompletesCall() async throws {
        let transport = ScriptedTransport(autoRespondRequestCount: 1)
        let (handle, driver, _, _) = try await establishInitiator(
            conduit: transport,
            dispatcher: EmptyServiceDispatcher()
        )
        let driverTask = Task {
            try await driver.run()
        }
        try await withAsyncCleanup({
            try? await transport.close()
            await cancelAndDrain(driverTask)
        }) {
            #expect(handle.laneId == 0)
            let payload = try await handle.callRaw(methodId: 1, payload: [1, 2, 3], timeout: 2.0)
            #expect(payload == [0])
        }
    }

    // r[verify rpc.flow-control]
    // r[verify rpc.flow-control.max-concurrent-requests]
    // r[verify rpc.flow-control.max-concurrent-requests.outbound]
    // r[verify rpc.flow-control.max-concurrent-requests.counting]
    // r[verify connection.lane-id-parity]
    // r[verify rpc.request.id-allocation]
    @Test func outboundMaxConcurrentRequestsWaitsForPeerLimit() async throws {
        let transport = ScriptedTransport(
            initialHandshake: .helloYourself(
                HelloYourself(
                    connectionSettings: ConnectionSettings(
                        parity: .even,
                        maxConcurrentRequests: 1,
                        initialChannelCredit: 16
                    ),
                    messagePayloadSchema: Data(MessageSchemaClosure),
                    metadata: .null
                ))
        )
        let (handle, driver, _, _) = try await establishInitiator(
            conduit: transport,
            dispatcher: EmptyServiceDispatcher()
        )
        let driverTask = Task {
            try await driver.run()
        }

        try await withAsyncCleanup({
            try? await transport.close()
            await cancelAndDrain(driverTask)
        }) {
            let firstCall = Task {
                try await handle.callRaw(methodId: 1, payload: [1], timeout: 1.0)
            }
            guard let firstRequestId = await awaitRequestId(transport, index: 0) else {
                Issue.record("expected first request to be sent")
                return
            }
            #expect(firstRequestId == 1)

            let secondCall = Task {
                try await handle.callRaw(methodId: 2, payload: [2], timeout: 1.0)
            }
            try? await Task.sleep(nanoseconds: 50_000_000)
            #expect(await transport.sentRequestIds() == [1])

            await transport.enqueueMessage(
                .response(
                    laneId: 0,
                    requestId: firstRequestId,
                    metadata: .null,
                    payload: [0x11]
                )
            )
            let firstResponse = try await awaitTaskResult(firstCall)
            #expect(firstResponse == [0x11])

            guard let secondRequestId = await awaitRequestId(transport, index: 1) else {
                Issue.record("expected second request after first response releases capacity")
                return
            }
            #expect(secondRequestId == 3)

            await transport.enqueueMessage(
                .response(
                    laneId: 0,
                    requestId: secondRequestId,
                    metadata: .null,
                    payload: [0x22]
                )
            )
            let secondResponse = try await awaitTaskResult(secondCall)
            #expect(secondResponse == [0x22])
        }
    }

    // r[verify rpc.flow-control.max-concurrent-requests.inbound]
    @Test func inboundMaxConcurrentRequestsViolationClosesConnection() async throws {
        let rule = "rpc.flow-control.max-concurrent-requests.inbound"
        let gate = BlockingRequestGate()
        let transport = ScriptedTransport(
            initialHandshake: .hello(
                Hello(
                    parity: .odd,
                    connectionSettings: ConnectionSettings(
                        parity: .odd,
                        maxConcurrentRequests: 64,
                        initialChannelCredit: 16
                    ),
                    messagePayloadSchema: Data(MessageSchemaClosure),
                    metadata: .null
                ))
        )
        let (controlLane, driver, _, _) = try await establishAcceptor(
            conduit: transport,
            dispatcher: BlockingFlowControlDispatcher(gate: gate),
            maxConcurrentRequests: 1
        )
        #expect(controlLane.laneId == 0)
        let driverTask: Task<Void, Error> = Task {
            try await driver.run()
        }

        await withAsyncCleanup({
            await gate.release()
            try? await transport.close()
            await cancelAndDrain(driverTask)
        }) {
            await transport.enqueueMessage(
                .request(
                    laneId: 0,
                    requestId: 77,
                    methodId: 1,
                    metadata: .null,
                    payload: []
                )
            )

            let start = ContinuousClock.now
            let timeout = Duration.milliseconds(1_000)
            while ContinuousClock.now - start < timeout {
                if await gate.hasStarted() {
                    break
                }
                try? await Task.sleep(nanoseconds: 5_000_000)
            }
            #expect(await gate.hasStarted())

            await transport.enqueueMessage(
                .request(
                    laneId: 0,
                    requestId: 79,
                    methodId: 1,
                    metadata: .null,
                    payload: []
                )
            )

            do {
                try await awaitTaskResult(driverTask, timeoutMs: 1_000)
                Issue.record("expected inbound max-concurrent violation")
            } catch {
                #expect(isProtocolViolation(error, rule: rule))
            }

            let protocolReason = await awaitProtocolReason(transport)
            #expect(protocolReason == rule)
        }
    }

    // r[verify rpc.flow-control.max-concurrent-requests.connection-failure]
    @Test func queuedOutboundRequestFailsWhenLimitedConnectionCloses() async throws {
        let transport = ScriptedTransport(
            initialHandshake: .helloYourself(
                HelloYourself(
                    connectionSettings: ConnectionSettings(
                        parity: .even,
                        maxConcurrentRequests: 1,
                        initialChannelCredit: 16
                    ),
                    messagePayloadSchema: Data(MessageSchemaClosure),
                    metadata: .null
                ))
        )
        let (handle, driver, _, _) = try await establishInitiator(
            conduit: transport,
            dispatcher: EmptyServiceDispatcher()
        )
        let driverTask = Task {
            try await driver.run()
        }

        let firstCall = Task {
            try await handle.callRaw(methodId: 1, payload: [1], timeout: TimeInterval?.none)
        }
        guard let firstRequestId = await awaitRequestId(transport, index: 0) else {
            Issue.record("expected first request to be sent")
            return
        }
        #expect(firstRequestId == 1)

        let secondCall = Task {
            try await handle.callRaw(methodId: 2, payload: [2], timeout: TimeInterval?.none)
        }
        try? await Task.sleep(nanoseconds: 50_000_000)
        #expect(await transport.sentRequestIds() == [1])

        try? await transport.close()

        do {
            _ = try await awaitTaskResult(firstCall)
            Issue.record("expected in-flight request to fail after connection close")
        } catch {
            #expect(isConnectionClosed(error))
        }
        do {
            _ = try await awaitTaskResult(secondCall)
            Issue.record("expected queued request to fail after connection close")
        } catch {
            #expect(isConnectionClosed(error))
        }
        #expect(await transport.sentRequestIds() == [1])

        _ = try? await awaitTaskResult(driverTask)
    }

    @Test func callFailsFastAfterDriverExit() async throws {
        let transport = ScriptedTransport()
        let (handle, driver, _, _) = try await establishInitiator(
            conduit: transport,
            dispatcher: EmptyServiceDispatcher()
        )
        let driverTask = Task {
            try await driver.run()
        }

        try? await transport.close()
        _ = try? await driverTask.value

        let start = ContinuousClock.now
        do {
            _ = try await handle.callRaw(methodId: 123, payload: [1], timeout: 2.0)
            Issue.record("expected connection closed")
        } catch {
            #expect(isConnectionClosed(error))
        }
        let elapsed = ContinuousClock.now - start
        #expect(elapsed < .milliseconds(250))
    }

    @Test func zeroTimeoutDoesNotOrphanContinuation() async throws {
        let transport = ScriptedTransport()
        let (handle, driver, _, _) = try await establishInitiator(
            conduit: transport,
            dispatcher: EmptyServiceDispatcher()
        )
        let driverTask = Task {
            try await driver.run()
        }
        await withAsyncCleanup({
            try? await transport.close()
            await cancelAndDrain(driverTask)
        }) {
            do {
                _ = try await handle.callRaw(methodId: 1, payload: [], timeout: 0.0)
                Issue.record("expected timeout")
            } catch {
                #expect(isTimeout(error))
            }

            #expect(await awaitHasCancel(transport))
        }
    }

    // r[verify rpc.cancel]
    @Test func callTimesOutAndSendsCancel() async throws {
        let transport = ScriptedTransport()
        let (handle, driver, _, _) = try await establishInitiator(
            conduit: transport,
            dispatcher: EmptyServiceDispatcher()
        )
        let driverTask = Task {
            try await driver.run()
        }
        await withAsyncCleanup({
            try? await transport.close()
            await cancelAndDrain(driverTask)
        }) {
            do {
                _ = try await handle.callRaw(methodId: 1, payload: [], timeout: 0.05)
                Issue.record("expected timeout")
            } catch {
                #expect(isTimeout(error))
            }

            #expect(await awaitHasCancel(transport))
        }
    }

    // r[verify rpc.timeout.idle-progress]
    @Test func requestAssociatedChannelActivityExtendsIdleTimeout() async throws {
        let channelId: UInt64 = 1
        let transport = ScriptedTransport()
        let (handle, driver, _, _) = try await establishInitiator(
            conduit: transport,
            dispatcher: EmptyServiceDispatcher()
        )
        let receiver = await handle.incomingChannelRegistry.register(
            channelId,
            initialCredit: defaultInitialChannelCredit
        )
        let driverTask = Task {
            try await driver.run()
        }

        try await withAsyncCleanup({
            try? await transport.close()
            await cancelAndDrain(driverTask)
        }) {
            let callTask = Task {
                try await handle.callRaw(
                    methodId: 1,
                    payload: [],
                    channels: [channelId],
                    timeout: 0.12
                )
            }
            guard let requestId = await awaitRequestId(transport, index: 0) else {
                Issue.record("expected request to be sent")
                return
            }

            try await Task.sleep(nanoseconds: 70_000_000)
            await transport.enqueueMessage(
                .data(laneId: 0, channelId: channelId, payload: [0x01])
            )
            try await Task.sleep(nanoseconds: 80_000_000)
            await transport.enqueueMessage(
                .response(laneId: 0, requestId: requestId, metadata: .null, payload: [0xBB])
            )

            let response = try await awaitTaskResult(callTask, timeoutMs: 1_000)
            #expect(response == [0xBB])
            #expect(try await receiver.recv() == [0x01])
            #expect(await awaitProtocolReason(transport, timeoutMs: 100) == nil)
        }
    }

    // r[verify rpc.request.scope]
    // r[verify rpc.request.scope.channels]
    // r[verify rpc.request.scope.terminal]
    // r[verify rpc.timeout.idle-progress]
    @Test func requestTimeoutTerminalizesAssociatedChannels() async throws {
        let channelId: UInt64 = 1
        let transport = ScriptedTransport()
        let (handle, driver, _, _) = try await establishInitiator(
            conduit: transport,
            dispatcher: EmptyServiceDispatcher()
        )
        let receiver = await handle.incomingChannelRegistry.register(
            channelId,
            initialCredit: defaultInitialChannelCredit
        )
        let driverTask = Task {
            try await driver.run()
        }

        await withAsyncCleanup({
            try? await transport.close()
            await cancelAndDrain(driverTask)
        }) {
            let callTask = Task {
                try await handle.callRaw(
                    methodId: 1,
                    payload: [],
                    channels: [channelId],
                    timeout: 0.05
                )
            }
            guard await awaitRequestId(transport, index: 0) != nil else {
                Issue.record("expected request to be sent")
                return
            }

            do {
                _ = try await awaitTaskResult(callTask, timeoutMs: 1_000)
                Issue.record("expected timeout")
            } catch {
                #expect(isTimeout(error))
            }

            do {
                _ = try await receiver.recv()
                Issue.record("expected channel timeout error")
            } catch {
                #expect(error as? ChannelError == .timedOut)
            }
            #expect(await awaitHasCancel(transport))
        }
    }

    // r[verify rpc.cancel.channels]
    @Test func inboundCancelTerminalizesRequestChannels() async throws {
        let channelId: UInt64 = 1
        let capture = ChannelReceiverCapture()
        let transport = ScriptedTransport(
            initialHandshake: .hello(
                Hello(
                    parity: .odd,
                    connectionSettings: ConnectionSettings(
                        parity: .odd,
                        maxConcurrentRequests: 64,
                        initialChannelCredit: 16
                    ),
                    messagePayloadSchema: Data(MessageSchemaClosure),
                    metadata: .null
                ))
        )
        let (controlLane, driver, connectionHandle, _) = try await establishAcceptor(
            conduit: transport,
            dispatcher: CancelChannelDispatcher(capture: capture)
        )
        #expect(controlLane.laneId == 0)
        let driverTask: Task<Void, Error> = Task {
            try await driver.run()
        }

        await withAsyncCleanup({
            try? await transport.close()
            await cancelAndDrain(driverTask)
        }) {
            defer {
                _ = controlLane
                _ = connectionHandle
            }
            await transport.enqueueMessage(
                .request(
                    laneId: 0,
                    requestId: 77,
                    methodId: 1,
                    metadata: .null,
                    payload: [],
                    channels: [channelId]
                )
            )

            let start = ContinuousClock.now
            let timeout = Duration.milliseconds(1_000)
            var receiver: ChannelReceiver? = nil
            while ContinuousClock.now - start < timeout {
                if let captured = await capture.get() {
                    receiver = captured
                    break
                }
                try? await Task.sleep(nanoseconds: 5_000_000)
            }
            guard let receiver else {
                Issue.record("expected request channel to be registered")
                return
            }

            await transport.enqueueMessage(messageCancel(requestId: 77))

            let receivedTask = Task<[UInt8]?, Error> {
                try await receiver.recv()
            }
            do {
                _ = try await awaitTaskResult(receivedTask, timeoutMs: 1_000)
                Issue.record("expected request channel to observe cancellation")
            } catch {
                #expect(error as? VoxRuntime.ChannelError == .cancelled)
            }
            #expect(await awaitProtocolReason(transport, timeoutMs: 100) == nil)
        }
    }

    @Test func callFailsWhenRequestSendFails() async throws {
        let transport = ScriptedTransport()
        await transport.setFailNextRequestSend()

        let (handle, driver, _, _) = try await establishInitiator(
            conduit: transport,
            dispatcher: EmptyServiceDispatcher()
        )
        let driverTask = Task {
            try await driver.run()
        }

        do {
            _ = try await handle.callRaw(methodId: 1, payload: [], timeout: 2.0)
            Issue.record("expected transport error")
        } catch {
            #expect(isTransportError(error))
        }

        try? await transport.close()
        _ = try? await driverTask.value
    }

    // r[verify connection.protocol-error]
    // r[verify rpc.observability.connection-errors]
    @Test func unknownResponseRequestIdClosesConnectionAndFailsPendingCalls() async throws {
        let transport = ScriptedTransport()
        let (handle, driver, _, _) = try await establishInitiator(
            conduit: transport,
            dispatcher: EmptyServiceDispatcher()
        )
        let driverTask = Task {
            try await driver.run()
        }

        let callTask = Task { try await handle.callRaw(methodId: 1, payload: [], timeout: 2.0) }
        let requestId = await awaitRequestId(transport, index: 0)
        #expect(requestId != nil)
        await transport.enqueueMessage(
            .response(laneId: 0, requestId: 999, metadata: .null, payload: [7, 7, 7])
        )

        do {
            _ = try await callTask.value
            Issue.record("expected connection closed")
        } catch {
            #expect(isConnectionClosed(error))
        }

        do {
            try await driverTask.value
            Issue.record("expected protocol violation")
        } catch {
            #expect(isProtocolViolation(error, rule: "call.lifecycle.unknown-request-id"))
        }

        let protocolReason = await awaitProtocolReason(transport)
        #expect(protocolReason == "call.lifecycle.unknown-request-id")
    }

    // r[verify rpc.cancel]
    // r[verify rpc.error.scope]
    @Test func lateResponseAfterTimeoutIsIgnoredAndConnectionStaysUsable() async throws {
        let transport = ScriptedTransport()
        let (handle, driver, _, _) = try await establishInitiator(
            conduit: transport,
            dispatcher: EmptyServiceDispatcher()
        )
        let driverTask = Task {
            try await driver.run()
        }

        let timedOutCall = Task {
            try await handle.callRaw(methodId: 42, payload: [4, 2], timeout: 0.05)
        }
        guard let timedOutRequestId = await awaitRequestId(transport, index: 0) else {
            Issue.record("expected first request to be sent")
            return
        }

        do {
            _ = try await timedOutCall.value
            Issue.record("expected timeout")
        } catch {
            #expect(isTimeout(error))
        }

        await transport.enqueueMessage(
            .response(
                laneId: 0,
                requestId: timedOutRequestId,
                metadata: .null,
                payload: [0xAA]
            )
        )

        let followupCall = Task {
            try await handle.callRaw(methodId: 99, payload: [9], timeout: 1.0)
        }
        guard let followupRequestId = await awaitRequestId(transport, index: 1) else {
            Issue.record("expected follow-up request to be sent")
            return
        }

        await transport.enqueueMessage(
            .response(
                laneId: 0,
                requestId: followupRequestId,
                metadata: .null,
                payload: [0xBB]
            )
        )

        let followupResponse = try await awaitTaskResult(followupCall, timeoutMs: 1_000)
        #expect(followupResponse == [0xBB])
        #expect(await awaitProtocolReason(transport, timeoutMs: 100) == nil)

        try? await transport.close()
        _ = try? await driverTask.value
    }

    // r[verify rpc.pipelining]
    @Test func slowIncomingRequestDoesNotBlockLaterRequest() async throws {
        let transport = ScriptedTransport(
            initialHandshake: .hello(
                Hello(
                    parity: .odd,
                    connectionSettings: ConnectionSettings(
                        parity: .odd,
                        maxConcurrentRequests: 64,
                        initialChannelCredit: 16
                    ),
                    messagePayloadSchema: Data(MessageSchemaClosure),
                    metadata: .null
                ))
        )
        let (controlLane, driver, connectionHandle, _) = try await establishAcceptor(
            conduit: transport,
            dispatcher: PipeliningDispatcher()
        )
        #expect(controlLane.laneId == 0)
        let driverTask: Task<Void, Error> = Task {
            try await driver.run()
        }
        await withAsyncCleanup({
            try? await transport.close()
            await cancelAndDrain(driverTask)
        }) {
            defer {
                _ = controlLane
                _ = connectionHandle
            }
            await transport.enqueueMessage(
                .request(
                    laneId: 0,
                    requestId: 77,
                    methodId: 1,
                    metadata: .null,
                    payload: []
                )
            )

            await transport.enqueueMessage(
                .request(
                    laneId: 0,
                    requestId: 79,
                    methodId: 2,
                    metadata: .null,
                    payload: []
                )
            )

            let second = await awaitResponsePayload(
                transport,
                laneId: 0,
                requestId: 79,
                timeoutMs: 150
            )
            #expect(second == [0x02])
            #expect(
                await awaitResponsePayload(
                    transport,
                    laneId: 0,
                    requestId: 77,
                    timeoutMs: 100
                ) == nil
            )

            let first = await awaitResponsePayload(
                transport,
                laneId: 0,
                requestId: 77
            )
            #expect(first == [0x01])
        }
    }

    @Test func duplicateResponseAfterSuccessIsIgnored() async throws {
        let transport = ScriptedTransport()
        let (handle, driver, _, _) = try await establishInitiator(
            conduit: transport,
            dispatcher: EmptyServiceDispatcher()
        )
        let driverTask = Task {
            try await driver.run()
        }

        let firstCall = Task {
            try await handle.callRaw(methodId: 7, payload: [7], timeout: 1.0)
        }
        guard let firstRequestId = await awaitRequestId(transport, index: 0) else {
            Issue.record("expected first request to be sent")
            return
        }

        await transport.enqueueMessage(
            .response(
                laneId: 0,
                requestId: firstRequestId,
                metadata: .null,
                payload: [0x01]
            )
        )

        let firstResponse = try await awaitTaskResult(firstCall, timeoutMs: 1_000)
        #expect(firstResponse == [0x01])

        await transport.enqueueMessage(
            .response(
                laneId: 0,
                requestId: firstRequestId,
                metadata: .null,
                payload: [0x02]
            )
        )

        let secondCall = Task {
            try await handle.callRaw(methodId: 8, payload: [8], timeout: 1.0)
        }
        guard let secondRequestId = await awaitRequestId(transport, index: 1) else {
            Issue.record("expected second request to be sent")
            return
        }

        await transport.enqueueMessage(
            .response(
                laneId: 0,
                requestId: secondRequestId,
                metadata: .null,
                payload: [0x03]
            )
        )

        let secondResponse = try await awaitTaskResult(secondCall, timeoutMs: 1_000)
        #expect(secondResponse == [0x03])
        #expect(await awaitProtocolReason(transport, timeoutMs: 100) == nil)

        try? await transport.close()
        _ = try? await driverTask.value
    }

    @Test func protocolViolationFromIncomingMessageFailsPendingCalls() async throws {
        let transport = ScriptedTransport()
        let (handle, driver, _, _) = try await establishInitiator(
            conduit: transport,
            dispatcher: EmptyServiceDispatcher()
        )
        let driverTask = Task {
            try await driver.run()
        }

        let pendingCall = Task { try await handle.callRaw(methodId: 7, payload: [7], timeout: 2.0) }
        let reqId = await awaitRequestId(transport, index: 0)
        #expect(reqId != nil)

        await transport.enqueueMessage(.data(laneId: 0, channelId: 0, payload: [1]))

        do {
            _ = try await pendingCall.value
            Issue.record("expected connection closed")
        } catch {
            #expect(isConnectionClosed(error))
        }

        do {
            try await driverTask.value
            Issue.record("expected protocol violation")
        } catch {
            #expect(isProtocolViolation(error, rule: "rpc.channel.allocation"))
        }
    }

    @Test func manyCallsFailFastWhenConnectionDrops() async throws {
        let transport = ScriptedTransport(autoRespondRequestCount: 20, dropAfterRequestCount: 20)
        let (handle, driver, _, _) = try await establishInitiator(
            conduit: transport,
            dispatcher: EmptyServiceDispatcher()
        )
        let driverTask = Task {
            try await driver.run()
        }
        try await withAsyncCleanup({
            try? await transport.close()
            await cancelAndDrain(driverTask)
        }) {
            let results = try await withThrowingTaskGroup(of: Result<[UInt8], Error>.self) {
                group in
                for _ in 0..<100 {
                    group.addTask {
                        do {
                            let response = try await handle.callRaw(
                                methodId: 1, payload: [], timeout: 1.0)
                            return .success(response)
                        } catch {
                            return .failure(error)
                        }
                    }
                }

                var all: [Result<[UInt8], Error>] = []
                for try await result in group {
                    all.append(result)
                }
                return all
            }

            let successCount = results.reduce(0) { partial, result in
                switch result {
                case .success:
                    return partial + 1
                case .failure:
                    return partial
                }
            }
            let closedCount = results.reduce(0) { partial, result in
                switch result {
                case .success:
                    return partial
                case .failure(let error):
                    return partial + (isConnectionClosed(error) ? 1 : 0)
                }
            }
            let timeoutCount = results.reduce(0) { partial, result in
                switch result {
                case .success:
                    return partial
                case .failure(let error):
                    return partial + (isTimeout(error) ? 1 : 0)
                }
            }

            #expect(successCount > 0)
            #expect(closedCount > 0)
            #expect(timeoutCount == 0)
        }
    }

    // r[verify connection.keepalive]
    @Test func keepalivePingPongHealthyPath() async throws {
        let transport = ScriptedTransport(autoRespondPing: true)
        let (handle, driver, _, _) = try await establishInitiator(
            conduit: transport,
            dispatcher: EmptyServiceDispatcher(),
            keepalive: ConnectionKeepaliveConfig(pingInterval: 0.02, pongTimeout: 0.05)
        )
        let driverTask = Task {
            try await driver.run()
        }
        try await withAsyncCleanup({
            try? await transport.close()
            await cancelAndDrain(driverTask)
        }) {
            let firstPing = await awaitSentPingNonce(transport, timeoutMs: 1_000)
            #expect(firstPing != nil)

            let callTask: Task<[UInt8], Error> = Task {
                try await handle.callRaw(methodId: 99, payload: [1, 2], timeout: 1.0)
            }
            guard let requestId = await awaitRequestId(transport, index: 0) else {
                Issue.record("expected outbound request")
                return
            }
            await transport.enqueueMessage(
                .response(laneId: 0, requestId: requestId, metadata: .null, payload: [0x42])
            )

            let response = try await awaitTaskResult(callTask, timeoutMs: 1_000)
            #expect(response == [0x42])
        }
    }

    // r[verify connection.keepalive]
    @Test func keepaliveMissingPongClosesDriver() async throws {
        let transport = ScriptedTransport()
        let (controlLane, driver, _, _) = try await establishInitiator(
            conduit: transport,
            dispatcher: EmptyServiceDispatcher(),
            keepalive: ConnectionKeepaliveConfig(pingInterval: 0.02, pongTimeout: 0.05)
        )
        #expect(controlLane.laneId == 0)
        let driverTask: Task<Void, Error> = Task {
            try await driver.run()
        }
        await withAsyncCleanup({
            try? await transport.close()
            await cancelAndDrain(driverTask)
        }) {
            let firstPing = await awaitSentPingNonce(transport, timeoutMs: 1_000)
            #expect(firstPing != nil)

            do {
                _ = try await awaitTaskResult(driverTask, timeoutMs: 1_000)
                Issue.record("expected connection closed")
            } catch {
                #expect(isConnectionClosed(error))
            }
        }
    }

    // r[verify connection.keepalive]
    @Test func keepaliveFailureFailsPendingCall() async throws {
        let transport = ScriptedTransport()
        let (handle, driver, _, _) = try await establishInitiator(
            conduit: transport,
            dispatcher: EmptyServiceDispatcher(),
            keepalive: ConnectionKeepaliveConfig(pingInterval: 0.02, pongTimeout: 0.05)
        )
        let driverTask: Task<Void, Error> = Task {
            try await driver.run()
        }
        await withAsyncCleanup({
            try? await transport.close()
            await cancelAndDrain(driverTask)
        }) {
            let callTask: Task<[UInt8], Error> = Task {
                try await handle.callRaw(methodId: 123, payload: [9], timeout: TimeInterval?.none)
            }

            do {
                _ = try await awaitTaskResult(callTask, timeoutMs: 1_000)
                Issue.record("expected connection closed")
            } catch {
                #expect(isConnectionClosed(error))
            }
        }

        do {
            _ = try await awaitTaskResult(driverTask, timeoutMs: 1_000)
            Issue.record("expected driver shutdown")
        } catch {
            #expect(isConnectionClosed(error))
        }
    }

    // r[verify lane]
    // r[verify lane.open]
    // r[verify lane.open.wire]
    // r[verify lane.request-channel-parity]
    // r[verify lane.open.api]
    // r[verify lane.open.settings]
    // r[verify connection.message.lane-id]
    // r[verify rpc.request]
    // r[verify rpc.request.id-allocation]
    // r[verify rpc.response]
    @Test func connectionHandleOpenLaneCompletesOnAcceptAndUsesServiceLaneId() async throws {
        let transport = ScriptedTransport()
        let (controlLane, driver, connectionHandle, _) = try await establishInitiator(
            conduit: transport,
            dispatcher: EmptyServiceDispatcher()
        )
        #expect(controlLane.laneId == 0)
        let driverTask: Task<Void, Error> = Task {
            try await driver.run()
        }
        try await withAsyncCleanup({
            try? await transport.close()
            await cancelAndDrain(driverTask)
        }) {
            let localSettings = ConnectionSettings(
                parity: .even,
                maxConcurrentRequests: 8,
                initialChannelCredit: 16
            )
            let openTask: Task<Lane, Error> = Task {
                try await connectionHandle.openLane(
                    settings: localSettings,
                    metadata: meta([("vox-service", "Noop")])
                )
            }

            guard let laneId = await awaitLaneOpenId(transport) else {
                Issue.record("expected LaneOpen")
                return
            }
            #expect(laneId == 1)
            let opens = await transport.sent().compactMap { message -> LaneOpen? in
                if case .laneOpen(let open) = message.payload {
                    return open
                }
                return nil
            }
            #expect(opens.first?.connectionSettings == localSettings)

            await transport.enqueueMessage(
                messageLaneAccept(
                    laneId: laneId,
                    settings: ConnectionSettings(
                        parity: .odd,
                        maxConcurrentRequests: 8,
                        initialChannelCredit: 16
                    ),
                    metadata: .null
                )
            )

            let connection = try await awaitTaskResult(openTask)
            #expect(connection.laneId == laneId)

            let callTask: Task<[UInt8], Error> = Task {
                try await connection.callRaw(methodId: 99, payload: [1, 2, 3], timeout: 1.0)
            }
            guard let request = await awaitRequest(transport, laneId: laneId) else {
                Issue.record("expected request on service lane")
                return
            }
            #expect(request.id == 2)

            await transport.enqueueMessage(
                .response(laneId: laneId, requestId: request.id, metadata: .null, payload: [0x42])
            )
            let response = try await awaitTaskResult(callTask)
            #expect(response == [0x42])
        }
    }

    // r[verify lane.close]
    // r[verify lane.close.semantics]
    @Test func incomingServiceLaneCloseTearsDownLocalHandle() async throws {
        let transport = ScriptedTransport()
        let (controlLane, driver, connectionHandle, _) = try await establishInitiator(
            conduit: transport,
            dispatcher: EmptyServiceDispatcher()
        )
        #expect(controlLane.laneId == 0)
        let driverTask: Task<Void, Error> = Task {
            try await driver.run()
        }
        try await withAsyncCleanup({
            try? await transport.close()
            await cancelAndDrain(driverTask)
        }) {
            let localSettings = ConnectionSettings(
                parity: .even,
                maxConcurrentRequests: 8,
                initialChannelCredit: 16
            )
            let openTask: Task<Lane, Error> = Task {
                try await connectionHandle.openLane(
                    settings: localSettings,
                    metadata: meta([("vox-service", "Noop")])
                )
            }

            guard let laneId = await awaitLaneOpenId(transport) else {
                Issue.record("expected LaneOpen")
                return
            }
            await transport.enqueueMessage(
                messageLaneAccept(
                    laneId: laneId,
                    settings: ConnectionSettings(
                        parity: .odd,
                        maxConcurrentRequests: 8,
                        initialChannelCredit: 16
                    ),
                    metadata: .null
                )
            )
            let connection = try await awaitTaskResult(openTask)

            let pendingCall: Task<[UInt8], Error> = Task {
                try await connection.callRaw(methodId: 99, payload: [0x09], timeout: 1.0)
            }
            guard await awaitRequest(transport, laneId: laneId) != nil else {
                Issue.record("expected request on service lane")
                return
            }

            await transport.enqueueMessage(messageLaneClose(laneId: laneId))
            do {
                _ = try await awaitTaskResult(pendingCall)
                Issue.record("expected pending virtual call to fail")
            } catch {
                #expect(isConnectionClosed(error))
            }

            let staleCall: Task<[UInt8], Error> = Task {
                try await connection.callRaw(methodId: 100, payload: [0x0A], timeout: 1.0)
            }
            do {
                _ = try await awaitTaskResult(staleCall)
                Issue.record("expected stale virtual call to fail")
            } catch {
                #expect(isConnectionClosed(error))
            }

            let sentAfterClose = await transport.sent()
            let virtualCallCount = sentAfterClose.reduce(0) { count, message in
                guard message.laneId == laneId else {
                    return count
                }
                if case .requestMessage(let request) = message.payload,
                    case .call = request.body
                {
                    return count + 1
                }
                return count
            }
            #expect(virtualCallCount == 1)

            await transport.enqueueMessage(
                .request(
                    laneId: laneId,
                    requestId: 88,
                    methodId: 42,
                    metadata: .null,
                    payload: []
                )
            )
            #expect(
                await awaitProtocolReason(transport)
                    == "call.lifecycle.unknown-connection-id"
            )

            do {
                _ = try await awaitTaskResult(driverTask)
                Issue.record("expected protocol violation")
            } catch {
                #expect(isProtocolViolation(error, rule: "call.lifecycle.unknown-connection-id"))
            }
        }
    }

    // r[verify rpc.caller.liveness.refcounted]
    // r[verify rpc.caller.liveness.last-drop-closes-connection]
    @Test func droppingLastOutboundServiceLaneReferenceDoesNotSendClose() async throws {
        let transport = ScriptedTransport()
        let (controlLane, driver, connectionHandle, _) = try await establishInitiator(
            conduit: transport,
            dispatcher: EmptyServiceDispatcher()
        )
        #expect(controlLane.laneId == 0)
        let driverTask: Task<Void, Error> = Task {
            try await driver.run()
        }
        try await withAsyncCleanup({
            try? await transport.close()
            await cancelAndDrain(driverTask)
        }) {
            let localSettings = ConnectionSettings(
                parity: .even,
                maxConcurrentRequests: 8,
                initialChannelCredit: 16
            )
            var openTask: Task<Lane, Error>? = Task {
                try await connectionHandle.openLane(
                    settings: localSettings,
                    metadata: meta([("vox-service", "Noop")])
                )
            }

            guard let laneId = await awaitLaneOpenId(transport) else {
                Issue.record("expected LaneOpen")
                return
            }
            await transport.enqueueMessage(
                messageLaneAccept(
                    laneId: laneId,
                    settings: ConnectionSettings(
                        parity: .odd,
                        maxConcurrentRequests: 8,
                        initialChannelCredit: 16
                    ),
                    metadata: .null
                )
            )

            var firstReference: Lane? = try await awaitTaskResult(openTask!)
            openTask = nil
            var secondReference = firstReference
            firstReference = nil
            #expect(secondReference?.laneId == laneId)

            #expect(
                await awaitLaneClose(transport, laneId: laneId, timeoutMs: 100)
                    == nil)

            secondReference = nil
            #expect(
                await awaitLaneClose(transport, laneId: laneId, timeoutMs: 100)
                    == nil)

            try await connectionHandle.closeLane(laneId)
            guard await awaitLaneClose(transport, laneId: laneId) != nil else {
                Issue.record("expected service lane LaneClose after explicit close")
                return
            }
        }
    }

    // r[verify rpc.caller.liveness.public-handle-drop]
    // r[verify rpc.caller.liveness.explicit-shutdown-required]
    // r[verify connection.lifecycle.driven]
    // r[verify connection.shutdown.explicit]
    // r[verify lane]
    // r[verify lane.open]
    @Test func droppingControlLaneAndServiceLanesDoesNotStopDrivenConnection() async throws {
        let transport = ScriptedTransport()
        let driver: Driver
        let connectionHandle: ConnectionHandle
        var controlLane: Lane?
        do {
            let established = try await establishInitiator(
                conduit: transport,
                dispatcher: EmptyServiceDispatcher()
            )
            controlLane = established.0
            driver = established.1
            connectionHandle = established.2
        }
        let driverTask: Task<Void, Error> = Task {
            try await driver.run()
        }
        try await withAsyncCleanup({
            try? await transport.close()
            await cancelAndDrain(driverTask)
        }) {
            #expect(controlLane?.laneId == 0)
            let localSettings = ConnectionSettings(
                parity: .even,
                maxConcurrentRequests: 8,
                initialChannelCredit: 16
            )
            var openTask: Task<Lane, Error>? = Task {
                try await connectionHandle.openLane(
                    settings: localSettings,
                    metadata: meta([("vox-service", "Noop")])
                )
            }

            guard let laneId = await awaitLaneOpenId(transport) else {
                Issue.record("expected LaneOpen")
                return
            }
            await transport.enqueueMessage(
                messageLaneAccept(
                    laneId: laneId,
                    settings: ConnectionSettings(
                        parity: .odd,
                        maxConcurrentRequests: 8,
                        initialChannelCredit: 16
                    ),
                    metadata: .null
                )
            )
            var serviceLane: Lane? = try await awaitTaskResult(openTask!)
            openTask = nil

            controlLane = nil
            #expect(
                await awaitLaneClose(transport, laneId: 0, timeoutMs: 100)
                    == nil)

            do {
                guard let liveLane = serviceLane else {
                    Issue.record("expected retained service lane")
                    return
                }
                let callTask: Task<[UInt8], Error> = Task {
                    try await liveLane.callRaw(
                        methodId: 99,
                        payload: [1, 2, 3],
                        timeout: 1.0
                    )
                }
                guard let request = await awaitRequest(transport, laneId: laneId) else {
                    Issue.record("expected request on service lane after root drop")
                    return
                }
                await transport.enqueueMessage(
                    .response(
                        laneId: laneId,
                        requestId: request.id,
                        metadata: .null,
                        payload: [0x42]
                    )
                )
                let response = try await awaitTaskResult(callTask)
                #expect(response == [0x42])
            }

            serviceLane = nil
            #expect(
                await awaitLaneClose(transport, laneId: laneId, timeoutMs: 100)
                    == nil)

            try await connectionHandle.closeLane(laneId)
            guard await awaitLaneClose(transport, laneId: laneId) != nil else {
                Issue.record("expected service lane LaneClose after explicit close")
                return
            }

            connectionHandle.shutdown()
            try await awaitTaskResult(driverTask)
        }
    }

    // r[verify lane.accept.api]
    // r[verify lane.service.compat]
    // r[verify lane.authorization]
    // r[verify lane.authorization.context]
    // r[verify request.authorization]
    // r[verify connection.identity.late-claims]
    // r[verify connection.symmetry]
    // r[verify lane.service]
    @Test func inboundOpenLaneAcceptsAndDispatchesOnServiceLane() async throws {
        let transport = ScriptedTransport()
        let (controlLane, driver, _, _) = try await establishInitiator(
            conduit: transport,
            dispatcher: EmptyServiceDispatcher(),
            laneAcceptor: GrantingLaneAcceptor(dispatcher: AuthorizationProbeDispatcher())
        )
        #expect(controlLane.laneId == 0)
        let driverTask: Task<Void, Error> = Task {
            try await driver.run()
        }
        await withAsyncCleanup({
            try? await transport.close()
            await cancelAndDrain(driverTask)
        }) {
            let laneId: UInt64 = 2
            await transport.enqueueMessage(
                messageLaneOpen(
                    laneId: laneId,
                    settings: ConnectionSettings(
                        parity: .odd,
                        maxConcurrentRequests: 8,
                        initialChannelCredit: 16
                    ),
                    metadata: meta([("vox-service", "Noop")])
                )
            )

            guard let accept = await awaitLaneAccept(transport, laneId: laneId) else {
                Issue.record("expected LaneAccept")
                return
            }
            #expect(accept.connectionSettings.parity == .even)
            #expect(accept.connectionSettings.initialChannelCredit == 16)
            #expect(accept.metadata.metaStr("grant-scope") == "swift-probe")

            await transport.enqueueMessage(
                .request(
                    laneId: laneId,
                    requestId: 77,
                    methodId: 42,
                    metadata: .null,
                    payload: [0x07]
                )
            )

            let response = await awaitResponsePayload(
                transport,
                laneId: laneId,
                requestId: 77
            )
            #expect(response == [0x01])
        }
    }

    // r[verify lane.open.wire.rejection]
    // r[verify lane.open.result]
    // r[verify lane.wire.compat]
    // r[verify lane.authorization.filtered]
    // r[verify rejection.reason.taxonomy]
    @Test func inboundOpenLaneRejectsWithStructuredReasonWhenNoAcceptor() async throws {
        let transport = ScriptedTransport()
        let (controlLane, driver, _, _) = try await establishInitiator(
            conduit: transport,
            dispatcher: EmptyServiceDispatcher()
        )
        #expect(controlLane.laneId == 0)
        let driverTask: Task<Void, Error> = Task {
            try await driver.run()
        }
        await withAsyncCleanup({
            try? await transport.close()
            await cancelAndDrain(driverTask)
        }) {
            let laneId: UInt64 = 2
            await transport.enqueueMessage(
                messageLaneOpen(
                    laneId: laneId,
                    settings: ConnectionSettings(
                        parity: .odd,
                        maxConcurrentRequests: 8,
                        initialChannelCredit: 16
                    ),
                    metadata: meta([("vox-service", "Noop")])
                )
            )

            guard let reject = await awaitLaneReject(transport, laneId: laneId) else {
                Issue.record("expected LaneReject")
                return
            }
            let rejection = LaneRejection.fromMetadata(reject.metadata)
            #expect(rejection.reason == .notReady)
            #expect(rejection.message() == "no lane acceptor configured")
        }
    }

    // r[verify lane.open.wire.rejection]
    // r[verify lane.open.result]
    // r[verify lane.wire.compat]
    @Test func connectionHandleOpenLaneFailsOnReject() async throws {
        let transport = ScriptedTransport()
        let (controlLane, driver, connectionHandle, _) = try await establishInitiator(
            conduit: transport,
            dispatcher: EmptyServiceDispatcher()
        )
        #expect(controlLane.laneId == 0)
        let driverTask: Task<Void, Error> = Task {
            try await driver.run()
        }
        await withAsyncCleanup({
            try? await transport.close()
            await cancelAndDrain(driverTask)
        }) {
            let openTask: Task<Lane, Error> = Task {
                try await connectionHandle.openLane(
                    settings: ConnectionSettings(
                        parity: .even,
                        maxConcurrentRequests: 8,
                        initialChannelCredit: 16
                    ),
                    metadata: meta([("vox-service", "Noop")])
                )
            }

            guard let laneId = await awaitLaneOpenId(transport) else {
                Issue.record("expected LaneOpen")
                return
            }

            let rejection = LaneRejection.withMessage(.draining, "busy")
            await transport.enqueueMessage(
                messageLaneReject(laneId: laneId, metadata: rejection.toMetadata())
            )

            do {
                _ = try await awaitTaskResult(openTask)
                Issue.record("expected service lane lane-open rejection")
            } catch {
                let parsed = laneRejection(error)
                #expect(parsed?.reason == .draining)
                #expect(parsed?.message() == "busy")
                #expect(parsed?.metadata.metaStr(voxLaneRejectReasonMetadataKey) == "draining")
            }
        }
    }
}
