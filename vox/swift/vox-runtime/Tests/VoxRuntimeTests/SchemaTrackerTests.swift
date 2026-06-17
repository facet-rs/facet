import Foundation
import Testing
@preconcurrency import NIOCore
import PhonSchema

@testable import VoxRuntime

private func appendU32(_ value: UInt32, to bytes: inout [UInt8]) {
    let little = value.littleEndian
    withUnsafeBytes(of: little) { bytes.append(contentsOf: $0) }
}

private func appendU64(_ value: UInt64, to bytes: inout [UInt8]) {
    let little = value.littleEndian
    withUnsafeBytes(of: little) { bytes.append(contentsOf: $0) }
}

private func schemaClosure(root: UInt64, auxiliaryRoots: [(String, UInt64)] = []) -> [UInt8] {
    var bytes: [UInt8] = []
    appendU64(root, to: &bytes)
    appendU32(0, to: &bytes)
    if !auxiliaryRoots.isEmpty {
        appendU32(UInt32(auxiliaryRoots.count), to: &bytes)
        for (role, auxRoot) in auxiliaryRoots {
            let roleBytes = Array(role.utf8)
            appendU32(UInt32(roleBytes.count), to: &bytes)
            bytes.append(contentsOf: roleBytes)
            appendU64(auxRoot, to: &bytes)
        }
    }
    return bytes
}

private final class TaskInbox: @unchecked Sendable {
    private let lock = NSLock()
    private var messages: [TaskMessage] = []

    func append(_ message: TaskMessage) {
        lock.lock()
        messages.append(message)
        lock.unlock()
    }

    func snapshot() -> [TaskMessage] {
        lock.lock()
        defer { lock.unlock() }
        return messages
    }
}

private actor RecordingConduit: Conduit {
    private var messages: [Message] = []

    func send(_ message: Message) async throws {
        messages.append(message)
    }

    func recv() async throws -> Message? {
        nil
    }

    func setMaxFrameSize(_: Int) async throws {}

    func close() async throws {}

    func snapshot() -> [Message] {
        messages
    }
}

private struct EmptyServiceDispatcher: ServiceDispatcher {
    func encodeVoxError(_: VoxRuntimeError) -> [UInt8] {
        []
    }

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

private func testClientSchemaInfo(
    argsSchemaClosure: [UInt8],
    responseSchemaClosure: [UInt8] = []
) -> ClientSchemaInfo {
    let methodSchemas = PhonMethodSchemas(
        argsRoot: MessageRootId,
        argsSchemaClosure: argsSchemaClosure,
        argsDescriptor: MessageDescriptor,
        argsDescriptorBlocks: MessageDescriptorBlocks,
        okRoot: MessageRootId,
        responseRoot: MessageRootId,
        responseSchemaClosure: responseSchemaClosure,
        responseDescriptor: MessageDescriptor,
        responseDescriptorBlocks: MessageDescriptorBlocks
    )
    return ClientSchemaInfo(methodSchemas: methodSchemas, registry: MessageRegistry)
}

@Test
// r[verify schema.format.delivery]
// r[verify schema.tracking.sent]
// r[verify schema.tracking.bindings]
// r[verify schema.exchange.idempotent]
// r[verify schema.principles.sender-driven]
// r[verify schema.principles.no-roundtrips]
func schemaSendTrackerAdvertisesBindingOncePerDirection() {
    let tracker = SchemaSendTracker()
    let closure: [UInt8] = [1, 2, 3]

    #expect(tracker.prepareSchemas(7, .args, closure) == closure)
    #expect(tracker.prepareSchemas(7, .args, closure).isEmpty)
    #expect(tracker.prepareSchemas(7, .response, closure) == closure)
}

@Test
// r[verify schema.tracking.received]
// r[verify schema.tracking.bindings]
// r[verify schema.format.binding-roots]
// r[verify schema.exchange.channels]
// r[verify schema.exchange.channels.rx-args]
func schemaTrackerRecordsChannelAuxiliaryRoots() {
    let tracker = SchemaTracker()
    let closure = schemaClosure(
        root: 1,
        auxiliaryRoots: [("channel.arg.0.rx.element", 2)]
    )

    tracker.recordReceived(7, .args, closure)

    #expect(tracker.auxiliaryRoot(7, .args, role: "channel.arg.0.rx.element") == SchemaId(2))
    #expect(tracker.auxiliaryRoot(7, .args, role: "channel.arg.1.rx.element") == nil)
}

@Test
// r[verify schema.type-id.per-connection]
func schemaTrackerDoesNotShareReceivedBindingsAcrossInstances() {
    let closure = schemaClosure(root: 1)
    let firstConnection = SchemaTracker()
    let secondConnection = SchemaTracker()

    firstConnection.recordReceived(7, .args, closure)

    #expect(firstConnection.hasReceived(7, .args))
    #expect(!secondConnection.hasReceived(7, .args))
}

@Test
// r[verify schema.errors.call-level]
func schemaTrackerDecodePlanFailureIsBindingLocal() {
    let tracker = SchemaTracker()

    tracker.recordReceived(7, .response, schemaClosure(root: 0xdead_beef_cafe_babe))
    #expect(tracker.hasReceived(7, .response))
    let invalidProgram = tracker.buildDecodeProgram(
        7,
        .response,
        readerDescriptor: MessageDescriptor,
        readerBlocks: MessageDescriptorBlocks,
        local: MessageRegistry
    )
    #expect(invalidProgram == nil)

    tracker.recordReceived(9, .response, MessageSchemaClosure)
    let validProgram = tracker.buildDecodeProgram(
        9,
        .response,
        readerDescriptor: MessageDescriptor,
        readerBlocks: MessageDescriptorBlocks,
        local: MessageRegistry
    )
    #expect(validProgram != nil)
    #expect(tracker.hasReceived(7, .response))
}

@Test
// r[verify schema.exchange.caller]
// r[verify schema.exchange.required]
func callerAdvertisesArgsSchemaWithFirstRequestOnConnection() async throws {
    let conduit = RecordingConduit()
    let (_, driver) = makeDriverAndLane(
        conduit: conduit,
        dispatcher: EmptyServiceDispatcher(),
        role: .initiator,
        negotiated: Negotiated(
            maxPayloadSize: 1024,
            initialCredit: 16,
            maxConcurrentRequests: UInt32.max
        )
    )
    let schemas: [UInt8] = [1, 2, 3]
    let schemaInfo = testClientSchemaInfo(argsSchemaClosure: schemas)

    await driver.handleCommand(.call(
        laneId: 0,
        requestId: 1,
        methodId: 77,
        metadata: .null,
        payload: [9],
        channels: [],
        timeout: nil,
        responseTx: { _ in },
        schemaInfo: schemaInfo
    ))
    await driver.handleCommand(.call(
        laneId: 0,
        requestId: 3,
        methodId: 77,
        metadata: .null,
        payload: [10],
        channels: [],
        timeout: nil,
        responseTx: { _ in },
        schemaInfo: schemaInfo
    ))

    let sent = await conduit.snapshot()
    #expect(sent.count == 2)
    guard sent.count == 2 else { return }

    guard case .requestMessage(let firstRequest) = sent[0].payload,
        case .call(let firstCall) = firstRequest.body
    else {
        Issue.record("first sent message was not a call request")
        return
    }
    #expect([UInt8](firstCall.schemas) == schemas)

    guard case .requestMessage(let secondRequest) = sent[1].payload,
        case .call(let secondCall) = secondRequest.body
    else {
        Issue.record("second sent message was not a call request")
        return
    }
    #expect(secondCall.schemas.isEmpty)
}

@Test
// r[verify schema.exchange.callee]
// r[verify schema.exchange.required]
func calleeAdvertisesResponseSchemaWithFirstResponseOnConnection() async throws {
    let conduit = RecordingConduit()
    let (_, driver) = makeDriverAndLane(
        conduit: conduit,
        dispatcher: EmptyServiceDispatcher(),
        role: .acceptor,
        negotiated: Negotiated(
            maxPayloadSize: 1024,
            initialCredit: 16,
            maxConcurrentRequests: UInt32.max
        )
    )
    let schemas: [UInt8] = [4, 5, 6]

    #expect(
        await driver.state.addInFlight(
            9,
            laneId: 0,
            responseMetadata: .null,
            channels: [],
            localMaxConcurrentRequests: UInt32.max
        ) == .inserted
    )
    try await driver.handleTaskMessage(DriverQueuedTaskMessage(
        laneId: 0,
        taskMessage: .response(
            requestId: 9,
            payload: [7],
            methodId: 77,
            responseSchemaClosure: schemas
        )
    ))

    #expect(
        await driver.state.addInFlight(
            11,
            laneId: 0,
            responseMetadata: .null,
            channels: [],
            localMaxConcurrentRequests: UInt32.max
        ) == .inserted
    )
    try await driver.handleTaskMessage(DriverQueuedTaskMessage(
        laneId: 0,
        taskMessage: .response(
            requestId: 11,
            payload: [8],
            methodId: 77,
            responseSchemaClosure: schemas
        )
    ))

    let sent = await conduit.snapshot()
    #expect(sent.count == 2)
    guard sent.count == 2 else { return }

    guard case .requestMessage(let firstRequest) = sent[0].payload,
        case .response(let firstResponse) = firstRequest.body
    else {
        Issue.record("first sent message was not a response")
        return
    }
    #expect([UInt8](firstResponse.schemas) == schemas)

    guard case .requestMessage(let secondRequest) = sent[1].payload,
        case .response(let secondResponse) = secondRequest.body
    else {
        Issue.record("second sent message was not a response")
        return
    }
    #expect(secondResponse.schemas.isEmpty)
}

@Test
// r[verify schema.exchange.channels.tx-args]
func serverTxAdvertisesArgsSchemaBeforeFirstData() async throws {
    let registry = ChannelRegistry()
    let inbox = TaskInbox()
    let tx = await bindServerTx(
        channelId: 9,
        registry: registry,
        taskTx: { inbox.append($0) },
        methodId: 77,
        argsSchemaClosure: [1, 2, 3],
        schemaSendTracker: SchemaSendTracker(),
        serialize: { (value: Int32, buf: inout ByteBuffer) in
            buf.writeInteger(value, endianness: .little)
        }
    )

    try await tx.send(42)

    let messages = inbox.snapshot()
    #expect(messages.count == 2)
    guard messages.count == 2 else { return }
    guard case .schema(let methodId, let direction, let schemas) = messages[0] else {
        Issue.record("first task message was not schema")
        return
    }
    #expect(methodId == 77)
    #expect(direction == .args)
    #expect(schemas == [1, 2, 3])
    guard case .data(let channelId, let payload) = messages[1] else {
        Issue.record("second task message was not data")
        return
    }
    #expect(channelId == 9)
    var buf = ByteBufferAllocator().buffer(bytes: payload)
    #expect(buf.readInteger(endianness: .little, as: Int32.self) == 42)
}
