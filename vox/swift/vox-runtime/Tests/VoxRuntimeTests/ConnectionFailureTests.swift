import Foundation
import PhonSchema
import Testing

@testable import VoxRuntime

// Test shims for the removed CBOR-era `Message` static factories + encode/decode: map
// them onto the phon `message*` free functions + the phon envelope codec, so the
// scripted-transport tests read unchanged. (Metadata is now a phon `Value`.)
extension Message {
    static func request(
        connId: UInt64,
        requestId: UInt64,
        methodId: UInt64,
        metadata: Metadata,
        payload: [UInt8],
        channels: [UInt64] = []
    ) -> Message {
        messageRequest(
            requestId: requestId, methodId: methodId, payload: payload, metadata: metadata,
            channels: channels, connectionId: connId)
    }
    static func response(
        connId: UInt64, requestId: UInt64, metadata: Metadata, payload: [UInt8]
    ) -> Message {
        messageResponse(
            requestId: requestId, payload: payload, metadata: metadata, connectionId: connId)
    }
    static func data(connId: UInt64, channelId: UInt64, payload: [UInt8]) -> Message {
        messageData(channelId: channelId, item: payload, connectionId: connId)
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
                            connId: 0,
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
        taskTx _: @escaping @Sendable (TaskMessage) -> Void
    ) async {}
}

private struct ScriptedConnector: ConnectionConnector {
    let transport: ScriptedTransport

    func openAttachment() async throws -> LinkAttachment {
        .initiator(transport)
    }
}

private final class RecordingRuntimeObserver: VoxRuntimeObserver, @unchecked Sendable {
    private let lock = NSLock()
    private var events: [VoxDriverObserverEvent] = []

    func driverEvent(_ event: VoxDriverObserverEvent) {
        lock.lock()
        events.append(event)
        lock.unlock()
    }

    func snapshot() -> [VoxDriverObserverEvent] {
        lock.lock()
        defer { lock.unlock() }
        return events
    }
}

@Test
// r[verify session]
// r[verify connection.root]
// r[verify rpc.session-setup]
// r[verify session.peer]
// r[verify session.role]
// r[verify schema.interaction.metadata]
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
// r[verify session.handshake.sorry]
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
        taskTx: @escaping @Sendable (TaskMessage) -> Void
    ) async {
        taskTx(.response(requestId: requestId, payload: [0x01]))
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

private func awaitConnectionOpenId(
    _ transport: ScriptedTransport,
    timeoutMs: UInt64 = 1_000
) async -> UInt64? {
    let start = ContinuousClock.now
    let timeout = Duration.milliseconds(Int64(timeoutMs))
    while ContinuousClock.now - start < timeout {
        let sent = await transport.sent()
        for message in sent {
            if case .laneOpen = message.payload {
                return message.connectionId
            }
        }
        try? await Task.sleep(nanoseconds: 5_000_000)
    }
    return nil
}

private func awaitConnectionAccept(
    _ transport: ScriptedTransport,
    connectionId: UInt64,
    timeoutMs: UInt64 = 1_000
) async -> LaneAccept? {
    let start = ContinuousClock.now
    let timeout = Duration.milliseconds(Int64(timeoutMs))
    while ContinuousClock.now - start < timeout {
        let sent = await transport.sent()
        for message in sent where message.connectionId == connectionId {
            if case .laneAccept(let accept) = message.payload {
                return accept
            }
        }
        try? await Task.sleep(nanoseconds: 5_000_000)
    }
    return nil
}

private func awaitConnectionClose(
    _ transport: ScriptedTransport,
    connectionId: UInt64,
    timeoutMs: UInt64 = 1_000
) async -> LaneClose? {
    let start = ContinuousClock.now
    let timeout = Duration.milliseconds(Int64(timeoutMs))
    while ContinuousClock.now - start < timeout {
        let sent = await transport.sent()
        for message in sent where message.connectionId == connectionId {
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
    connectionId: UInt64,
    timeoutMs: UInt64 = 1_000
) async -> RequestMessage? {
    let start = ContinuousClock.now
    let timeout = Duration.milliseconds(Int64(timeoutMs))
    while ContinuousClock.now - start < timeout {
        let sent = await transport.sent()
        for message in sent where message.connectionId == connectionId {
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
    connectionId: UInt64,
    requestId: UInt64,
    timeoutMs: UInt64 = 1_000
) async -> [UInt8]? {
    let start = ContinuousClock.now
    let timeout = Duration.milliseconds(Int64(timeoutMs))
    while ContinuousClock.now - start < timeout {
        let sent = await transport.sent()
        for message in sent where message.connectionId == connectionId {
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

private func rejectedMetadata(_ error: Error) -> Metadata? {
    guard let connError = error as? ConnectionError else {
        return nil
    }
    if case .rejected(let metadata) = connError {
        return metadata
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
        setVoxRuntimeObserver(observer)
        defer {
            setVoxRuntimeObserver(nil)
        }

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

    // r[verify session.handshake]
    // r[verify session.handshake.phon]
    // r[verify session.handshake.protocol-schema]
    // r[verify session.handshake.protocol-schema.session-scoped]
    // r[verify session.handshake.unversioned]
    // r[verify session.connection-settings]
    // r[verify session.connection-settings.hello]
    // r[verify session.parity]
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

    // r[verify rpc.session-setup]
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

    // r[verify rpc.session-setup]
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

    // r[verify rpc.session-setup]
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

    // r[verify rpc.session-setup]
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
    // r[verify session.message]
    // r[verify session.message.connection-id]
    // r[verify session.message.payloads]
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
        #expect(controlLane.connectionId == 0)
        let driverTask: Task<Void, Error> = Task {
            try await driver.run()
        }
        await withAsyncCleanup({
            try? await transport.close()
            await cancelAndDrain(driverTask)
        }) {
            await transport.enqueueMessage(
                .request(
                    connId: 0,
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

    // r[verify connection]
    // r[verify connection.root]
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
            #expect(handle.connectionId == 0)
            let payload = try await handle.callRaw(methodId: 1, payload: [1, 2, 3], timeout: 2.0)
            #expect(payload == [0])
        }
    }

    // r[verify rpc.flow-control]
    // r[verify rpc.flow-control.max-concurrent-requests]
    // r[verify rpc.flow-control.max-concurrent-requests.outbound]
    // r[verify rpc.flow-control.max-concurrent-requests.counting]
    // r[verify session.parity]
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
                    connId: 0,
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
                    connId: 0,
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
        #expect(controlLane.connectionId == 0)
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
                    connId: 0,
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
                    connId: 0,
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

    // r[verify rpc.flow-control.max-concurrent-requests.session-failure]
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
                .data(connId: 0, channelId: channelId, payload: [0x01])
            )
            try await Task.sleep(nanoseconds: 80_000_000)
            await transport.enqueueMessage(
                .response(connId: 0, requestId: requestId, metadata: .null, payload: [0xBB])
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
        #expect(controlLane.connectionId == 0)
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
                    connId: 0,
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

    // r[verify session.protocol-error]
    // r[verify rpc.observability.session-errors]
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
            .response(connId: 0, requestId: 999, metadata: .null, payload: [7, 7, 7])
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
                connId: 0,
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
                connId: 0,
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
        #expect(controlLane.connectionId == 0)
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
                    connId: 0,
                    requestId: 77,
                    methodId: 1,
                    metadata: .null,
                    payload: []
                )
            )

            await transport.enqueueMessage(
                .request(
                    connId: 0,
                    requestId: 79,
                    methodId: 2,
                    metadata: .null,
                    payload: []
                )
            )

            let second = await awaitResponsePayload(
                transport,
                connectionId: 0,
                requestId: 79,
                timeoutMs: 150
            )
            #expect(second == [0x02])
            #expect(
                await awaitResponsePayload(
                    transport,
                    connectionId: 0,
                    requestId: 77,
                    timeoutMs: 100
                ) == nil
            )

            let first = await awaitResponsePayload(
                transport,
                connectionId: 0,
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
                connId: 0,
                requestId: firstRequestId,
                metadata: .null,
                payload: [0x01]
            )
        )

        let firstResponse = try await awaitTaskResult(firstCall, timeoutMs: 1_000)
        #expect(firstResponse == [0x01])

        await transport.enqueueMessage(
            .response(
                connId: 0,
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
                connId: 0,
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

        await transport.enqueueMessage(.data(connId: 0, channelId: 0, payload: [1]))

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

    // r[verify session.keepalive]
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
                .response(connId: 0, requestId: requestId, metadata: .null, payload: [0x42])
            )

            let response = try await awaitTaskResult(callTask, timeoutMs: 1_000)
            #expect(response == [0x42])
        }
    }

    // r[verify session.keepalive]
    @Test func keepaliveMissingPongClosesDriver() async throws {
        let transport = ScriptedTransport()
        let (controlLane, driver, _, _) = try await establishInitiator(
            conduit: transport,
            dispatcher: EmptyServiceDispatcher(),
            keepalive: ConnectionKeepaliveConfig(pingInterval: 0.02, pongTimeout: 0.05)
        )
        #expect(controlLane.connectionId == 0)
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

    // r[verify session.keepalive]
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

    // r[verify connection.open]
    // r[verify connection.parity]
    // r[verify rpc.virtual-connection.open]
    // r[verify session.connection-settings.open]
    // r[verify session.message.connection-id]
    // r[verify rpc.request]
    // r[verify rpc.request.id-allocation]
    // r[verify rpc.response]
    @Test func connectionHandleOpenLaneCompletesOnAcceptAndUsesServiceLaneId() async throws {
        let transport = ScriptedTransport()
        let (controlLane, driver, connectionHandle, _) = try await establishInitiator(
            conduit: transport,
            dispatcher: EmptyServiceDispatcher()
        )
        #expect(controlLane.connectionId == 0)
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

            guard let connId = await awaitConnectionOpenId(transport) else {
                Issue.record("expected LaneOpen")
                return
            }
            #expect(connId == 1)
            let opens = await transport.sent().compactMap { message -> LaneOpen? in
                if case .laneOpen(let open) = message.payload {
                    return open
                }
                return nil
            }
            #expect(opens.first?.connectionSettings == localSettings)

            await transport.enqueueMessage(
                messageAccept(
                    connectionId: connId,
                    settings: ConnectionSettings(
                        parity: .odd,
                        maxConcurrentRequests: 8,
                        initialChannelCredit: 16
                    ),
                    metadata: .null
                )
            )

            let connection = try await awaitTaskResult(openTask)
            #expect(connection.connectionId == connId)

            let callTask: Task<[UInt8], Error> = Task {
                try await connection.callRaw(methodId: 99, payload: [1, 2, 3], timeout: 1.0)
            }
            guard let request = await awaitRequest(transport, connectionId: connId) else {
                Issue.record("expected request on service lane")
                return
            }
            #expect(request.id == 2)

            await transport.enqueueMessage(
                .response(connId: connId, requestId: request.id, metadata: .null, payload: [0x42])
            )
            let response = try await awaitTaskResult(callTask)
            #expect(response == [0x42])
        }
    }

    // r[verify connection.close]
    // r[verify connection.close.semantics]
    @Test func incomingServiceLaneCloseTearsDownLocalHandle() async throws {
        let transport = ScriptedTransport()
        let (controlLane, driver, connectionHandle, _) = try await establishInitiator(
            conduit: transport,
            dispatcher: EmptyServiceDispatcher()
        )
        #expect(controlLane.connectionId == 0)
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

            guard let connId = await awaitConnectionOpenId(transport) else {
                Issue.record("expected LaneOpen")
                return
            }
            await transport.enqueueMessage(
                messageAccept(
                    connectionId: connId,
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
            guard await awaitRequest(transport, connectionId: connId) != nil else {
                Issue.record("expected request on service lane")
                return
            }

            await transport.enqueueMessage(messageConnectionClose(connectionId: connId))
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
                guard message.connectionId == connId else {
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
                    connId: connId,
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
        #expect(controlLane.connectionId == 0)
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

            guard let connId = await awaitConnectionOpenId(transport) else {
                Issue.record("expected LaneOpen")
                return
            }
            await transport.enqueueMessage(
                messageAccept(
                    connectionId: connId,
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
            #expect(secondReference?.connectionId == connId)

            #expect(
                await awaitConnectionClose(transport, connectionId: connId, timeoutMs: 100)
                    == nil)

            secondReference = nil
            #expect(
                await awaitConnectionClose(transport, connectionId: connId, timeoutMs: 100)
                    == nil)

            try await connectionHandle.closeLane(connId)
            guard await awaitConnectionClose(transport, connectionId: connId) != nil else {
                Issue.record("expected service lane LaneClose after explicit close")
                return
            }
        }
    }

    // r[verify rpc.caller.liveness.root-internal-close]
    // r[verify rpc.caller.liveness.root-teardown-condition]
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
            #expect(controlLane?.connectionId == 0)
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

            guard let connId = await awaitConnectionOpenId(transport) else {
                Issue.record("expected LaneOpen")
                return
            }
            await transport.enqueueMessage(
                messageAccept(
                    connectionId: connId,
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
                await awaitConnectionClose(transport, connectionId: 0, timeoutMs: 100)
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
                guard let request = await awaitRequest(transport, connectionId: connId) else {
                    Issue.record("expected request on service lane after root drop")
                    return
                }
                await transport.enqueueMessage(
                    .response(
                        connId: connId,
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
                await awaitConnectionClose(transport, connectionId: connId, timeoutMs: 100)
                    == nil)

            try await connectionHandle.closeLane(connId)
            guard await awaitConnectionClose(transport, connectionId: connId) != nil else {
                Issue.record("expected service lane LaneClose after explicit close")
                return
            }

            connectionHandle.shutdown()
            try await awaitTaskResult(driverTask)
        }
    }

    // r[verify rpc.virtual-connection.accept]
    // r[verify connection.virtual]
    // r[verify session.symmetry]
    // r[verify lane.service]
    @Test func inboundOpenLaneAcceptsAndDispatchesOnServiceLane() async throws {
        let transport = ScriptedTransport()
        let (controlLane, driver, _, _) = try await establishInitiator(
            conduit: transport,
            dispatcher: EmptyServiceDispatcher(),
            laneAcceptor: DefaultLaneAcceptor(dispatcher: ImmediateResponseDispatcher())
        )
        #expect(controlLane.connectionId == 0)
        let driverTask: Task<Void, Error> = Task {
            try await driver.run()
        }
        await withAsyncCleanup({
            try? await transport.close()
            await cancelAndDrain(driverTask)
        }) {
            let connId: UInt64 = 2
            await transport.enqueueMessage(
                messageConnect(
                    connectionId: connId,
                    settings: ConnectionSettings(
                        parity: .odd,
                        maxConcurrentRequests: 8,
                        initialChannelCredit: 16
                    ),
                    metadata: meta([("vox-service", "Noop")])
                )
            )

            guard let accept = await awaitConnectionAccept(transport, connectionId: connId) else {
                Issue.record("expected LaneAccept")
                return
            }
            #expect(accept.connectionSettings.parity == .even)
            #expect(accept.connectionSettings.initialChannelCredit == 16)

            await transport.enqueueMessage(
                .request(
                    connId: connId,
                    requestId: 77,
                    methodId: 42,
                    metadata: .null,
                    payload: [0x07]
                )
            )

            let response = await awaitResponsePayload(
                transport,
                connectionId: connId,
                requestId: 77
            )
            #expect(response == [0x01])
        }
    }

    // r[verify connection.open.rejection]
    @Test func connectionHandleOpenLaneFailsOnReject() async throws {
        let transport = ScriptedTransport()
        let (controlLane, driver, connectionHandle, _) = try await establishInitiator(
            conduit: transport,
            dispatcher: EmptyServiceDispatcher()
        )
        #expect(controlLane.connectionId == 0)
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

            guard let connId = await awaitConnectionOpenId(transport) else {
                Issue.record("expected LaneOpen")
                return
            }

            let rejectionMetadata = meta([("reason", "busy")])
            await transport.enqueueMessage(
                messageReject(connectionId: connId, metadata: rejectionMetadata)
            )

            do {
                _ = try await awaitTaskResult(openTask)
                Issue.record("expected service lane lane-open rejection")
            } catch {
                #expect(rejectedMetadata(error) == rejectionMetadata)
            }
        }
    }
}
