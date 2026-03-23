import Foundation
import Testing

@testable import VoxRuntime
@testable import subject_swift

private actor TaskResponseInbox {
    private var responses: [(UInt64, [UInt8])] = []
    private var waiters: [UInt64: CheckedContinuation<[UInt8], Never>] = [:]

    func push(_ message: TaskMessage) {
        guard case .response(let requestId, let payload, _) = message else {
            return
        }
        if let waiter = waiters.removeValue(forKey: requestId) {
            waiter.resume(returning: payload)
            return
        }
        responses.append((requestId, payload))
    }

    func nextResponse(for requestId: UInt64) async -> [UInt8] {
        if let index = responses.firstIndex(where: { $0.0 == requestId }) {
            return responses.remove(at: index).1
        }
        return await withCheckedContinuation { cont in
            waiters[requestId] = cont
        }
    }
}

private func withTimeout<T: Sendable>(
    milliseconds: UInt64,
    operation: @escaping @Sendable () async throws -> T
) async throws -> T {
    try await withThrowingTaskGroup(of: T.self) { group in
        group.addTask {
            try await operation()
        }
        group.addTask {
            try await Task.sleep(nanoseconds: milliseconds * 1_000_000)
            throw POSIXError(.ETIMEDOUT)
        }
        let result = try await group.next()!
        group.cancelAll()
        return result
    }
}

private func collectValues<T: Sendable>(from rx: UnboundRx<T>) async throws -> [T] {
    var values: [T] = []
    for try await value in rx {
        values.append(value)
    }
    return values
}

private final class OrderedMessagePump<Message: Sendable>: @unchecked Sendable {
    private let continuation: AsyncStream<Message>.Continuation
    private let worker: Task<Void, Never>

    init(handler: @escaping @Sendable (Message) async -> Void) {
        var captured: AsyncStream<Message>.Continuation?
        let stream = AsyncStream<Message> { continuation in
            captured = continuation
        }
        self.continuation = captured!
        self.worker = Task {
            for await message in stream {
                await handler(message)
            }
        }
    }

    func send(_ message: Message) {
        continuation.yield(message)
    }

    deinit {
        continuation.finish()
        worker.cancel()
    }
}

private struct ServerEnvelope: Sendable {
    let message: TaskMessage
    let inbox: TaskResponseInbox
}

private final class LoopbackConnection: VoxConnection, @unchecked Sendable {
    let channelAllocator = ChannelIdAllocator(role: .initiator)
    let incomingChannelRegistry = ChannelRegistry()

    private let adapter: TestbedDispatcherAdapter
    private let serverRegistry = ChannelRegistry()
    private let schemaSendTracker = SchemaSendTracker()
    private let lock = NSLock()
    private var nextRequestId: UInt64 = 1

    private lazy var clientPump = OrderedMessagePump<TaskMessage> { [weak self] message in
        guard let self else { return }
        await self.routeClientTaskMessage(message)
    }

    private lazy var serverPump = OrderedMessagePump<ServerEnvelope> { [weak self] envelope in
        guard let self else { return }
        await self.routeServerTaskMessage(envelope.message, inbox: envelope.inbox)
    }

    init(handler: TestbedHandler = TestbedService()) {
        self.adapter = TestbedDispatcherAdapter(handler: handler)
    }

    var taskSender: TaskSender {
        { [weak self] message in
            guard let self else { return }
            self.clientPump.send(message)
        }
    }

    func call(
        methodId: UInt64,
        metadata _: [MetadataEntry],
        payload: Data,
        retry _: RetryPolicy,
        timeout _: TimeInterval?,
        prepareRetry _: (@Sendable () async -> PreparedRetryRequest)?,
        finalizeChannels: (@Sendable () -> Void)?,
        schemaInfo _: ClientSchemaInfo?
    ) async throws -> Data {
        let requestId: UInt64 = lock.withLock {
            let id = nextRequestId
            nextRequestId += 1
            return id
        }

        let inbox = TaskResponseInbox()

        await adapter.preregister(
            methodId: methodId,
            payload: Array(payload),
            registry: serverRegistry
        )

        await adapter.dispatch(
            methodId: methodId,
            payload: Array(payload),
            requestId: requestId,
            registry: serverRegistry,
            schemaSendTracker: schemaSendTracker,
            taskTx: { [weak self] message in
                guard let self else { return }
                self.serverPump.send(ServerEnvelope(message: message, inbox: inbox))
            }
        )

        let responsePayload = try await withTimeout(milliseconds: 500) {
            await inbox.nextResponse(for: requestId)
        }
        finalizeChannels?()
        return Data(responsePayload)
    }

    private func routeClientTaskMessage(_ message: TaskMessage) async {
        switch message {
        case .data(let channelId, let payload):
            _ = await serverRegistry.deliverData(channelId: channelId, payload: payload)
        case .close(let channelId):
            _ = await serverRegistry.deliverClose(channelId: channelId)
        case .grantCredit(let channelId, let bytes):
            await serverRegistry.deliverCredit(channelId: channelId, bytes: bytes)
        case .response:
            return
        }
    }

    private func routeServerTaskMessage(_ message: TaskMessage, inbox: TaskResponseInbox) async {
        switch message {
        case .response:
            await inbox.push(message)
        case .data(let channelId, let payload):
            _ = await incomingChannelRegistry.deliverData(channelId: channelId, payload: payload)
        case .close(let channelId):
            _ = await incomingChannelRegistry.deliverClose(channelId: channelId)
        case .grantCredit(let channelId, let bytes):
            await incomingChannelRegistry.deliverCredit(channelId: channelId, bytes: bytes)
        }
    }
}

private extension NSLock {
    func withLock<T>(_ body: () -> T) -> T {
        lock()
        defer { unlock() }
        return body()
    }
}

private func rpcErrorCode(from payload: [UInt8]) -> RpcErrorCode? {
    var offset = 0
    do {
        let response = Data(payload)
        let resultDiscriminant = try decodeVarint(from: response, offset: &offset)
        guard resultDiscriminant == 1 else {
            return nil
        }
        let rawCode = try decodeU8(from: response, offset: &offset)
        return RpcErrorCode(rawValue: rawCode)
    } catch {
        return nil
    }
}

struct TestbedServiceCoverageTests {
    @Test func unaryAndCompositeMethodsCoverBranches() async throws {
        let service = TestbedService()

        #expect(try await service.echo(message: "abc") == "abc")
        #expect(try await service.reverse(message: "abc") == "cba")

        #expect(try await service.divide(dividend: 9, divisor: 3) == .success(3))
        #expect(try await service.divide(dividend: 9, divisor: 0) == .failure(.divisionByZero))

        let lookup1 = try await service.lookup(id: 1)
        if case .success(let person) = lookup1 {
            #expect(person.name == "Alice")
            #expect(person.age == 30)
            #expect(person.email == "alice@example.com")
        } else {
            Issue.record("lookup(id: 1) should succeed")
        }

        let lookup2 = try await service.lookup(id: 2)
        if case .success(let person) = lookup2 {
            #expect(person.name == "Bob")
            #expect(person.age == 25)
            #expect(person.email == nil)
        } else {
            Issue.record("lookup(id: 2) should succeed")
        }

        let lookupMissing = try await service.lookup(id: 99)
        if case .failure(let error) = lookupMissing {
            #expect(error == .notFound)
        } else {
            Issue.record("lookup(id: 99) should fail with notFound")
        }

        #expect(try await service.echoPoint(point: Point(x: 3, y: -4)).x == 3)
        #expect(try await service.createPerson(name: "Dana", age: 41, email: nil).name == "Dana")

        let area = try await service.rectangleArea(
            rect: Rectangle(
                topLeft: Point(x: 10, y: 12),
                bottomRight: Point(x: 4, y: 2),
                label: "r"
            )
        )
        #expect(area == 60)

        #expect(try await service.parseColor(name: "RED") == .red)
        #expect(try await service.parseColor(name: "green") == .green)
        #expect(try await service.parseColor(name: "blue") == .blue)
        #expect(try await service.parseColor(name: "unknown") == nil)

        #expect(try await service.shapeArea(shape: .circle(radius: 2)) == Double.pi * 4)
        #expect(try await service.shapeArea(shape: .rectangle(width: 3, height: 4)) == 12)
        #expect(try await service.shapeArea(shape: .point) == 0)

        let canvas = try await service.createCanvas(
            name: "c",
            shapes: [.point, .rectangle(width: 2, height: 3)],
            background: .blue
        )
        #expect(canvas.name == "c")
        #expect(canvas.shapes.count == 2)
        #expect(canvas.background == .blue)

        let textMessage = try await service.processMessage(msg: .text("x"))
        if case .text(let value) = textMessage {
            #expect(value == "processed: x")
        } else {
            Issue.record("processMessage(text) should return text")
        }

        let numberMessage = try await service.processMessage(msg: .number(12))
        if case .number(let value) = numberMessage {
            #expect(value == 24)
        } else {
            Issue.record("processMessage(number) should return number")
        }

        let dataMessage = try await service.processMessage(msg: .data(Data([1, 2, 3])))
        if case .data(let value) = dataMessage {
            #expect(value == Data([3, 2, 1]))
        } else {
            Issue.record("processMessage(data) should return data")
        }

        let points = try await service.getPoints(count: 3)
        #expect(points.count == 3)
        #expect(points[0].x == 0 && points[0].y == 0)
        #expect(points[1].x == 1 && points[1].y == 2)
        #expect(points[2].x == 2 && points[2].y == 4)

        let swapped = try await service.swapPair(pair: (7, "seven"))
        #expect(swapped.0 == "seven")
        #expect(swapped.1 == 7)
    }
}

struct TestbedDispatcherCoverageTests {
    @Test func unknownMethodReturnsUnknownMethodError() async {
        let adapter = TestbedDispatcherAdapter(handler: TestbedService())
        let registry = ChannelRegistry()
        let inbox = TaskResponseInbox()

        await adapter.dispatch(
            methodId: 0xFFFF_FFFF_FFFF_FFFF,
            payload: [],
            requestId: 11,
            registry: registry,
            schemaSendTracker: SchemaSendTracker(),
            taskTx: { message in Task { await inbox.push(message) } }
        )

        let payload = await inbox.nextResponse(for: 11)
        #expect(rpcErrorCode(from: payload) == .unknownMethod)
    }

    @Test func malformedPayloadReturnsInvalidPayloadError() async {
        let adapter = TestbedDispatcherAdapter(handler: TestbedService())
        let registry = ChannelRegistry()
        let inbox = TaskResponseInbox()

        await adapter.dispatch(
            methodId: TestbedMethodId.parseColor,
            payload: [0x80],
            requestId: 12,
            registry: registry,
            schemaSendTracker: SchemaSendTracker(),
            taskTx: { message in Task { await inbox.push(message) } }
        )

        let payload = await inbox.nextResponse(for: 12)
        #expect(rpcErrorCode(from: payload) == .invalidPayload)
    }
}

struct TestbedSerializersCoverageTests {
    @Test func primitiveSchemasRoundTrip() throws {
        let serializers = TestbedSerializers()

        let boolBytes = serializers.txSerializer(for: .bool)(true)
        #expect((try serializers.rxDeserializer(for: .bool)(boolBytes)) as? Bool == true)

        let u8Bytes = serializers.txSerializer(for: .u8)(UInt8(7))
        #expect((try serializers.rxDeserializer(for: .u8)(u8Bytes)) as? UInt8 == 7)

        let i8Bytes = serializers.txSerializer(for: .i8)(Int8(-7))
        #expect((try serializers.rxDeserializer(for: .i8)(i8Bytes)) as? Int8 == -7)

        let u16Bytes = serializers.txSerializer(for: .u16)(UInt16(1234))
        #expect((try serializers.rxDeserializer(for: .u16)(u16Bytes)) as? UInt16 == 1234)

        let i16Bytes = serializers.txSerializer(for: .i16)(Int16(-1234))
        #expect((try serializers.rxDeserializer(for: .i16)(i16Bytes)) as? Int16 == -1234)

        let u32Bytes = serializers.txSerializer(for: .u32)(UInt32(123_456))
        #expect((try serializers.rxDeserializer(for: .u32)(u32Bytes)) as? UInt32 == 123_456)

        let i32Bytes = serializers.txSerializer(for: .i32)(Int32(-123_456))
        #expect((try serializers.rxDeserializer(for: .i32)(i32Bytes)) as? Int32 == -123_456)

        let u64Bytes = serializers.txSerializer(for: .u64)(UInt64(9_876_543_210))
        #expect((try serializers.rxDeserializer(for: .u64)(u64Bytes)) as? UInt64 == 9_876_543_210)

        let i64Bytes = serializers.txSerializer(for: .i64)(Int64(-9_876_543_210))
        #expect((try serializers.rxDeserializer(for: .i64)(i64Bytes)) as? Int64 == -9_876_543_210)

        let f32Bytes = serializers.txSerializer(for: .f32)(Float(3.5))
        #expect((try serializers.rxDeserializer(for: .f32)(f32Bytes)) as? Float == 3.5)

        let f64Bytes = serializers.txSerializer(for: .f64)(Double(-42.25))
        #expect((try serializers.rxDeserializer(for: .f64)(f64Bytes)) as? Double == -42.25)

        let stringBytes = serializers.txSerializer(for: .string)("hello")
        #expect((try serializers.rxDeserializer(for: .string)(stringBytes)) as? String == "hello")

        let data = Data([1, 2, 3, 4])
        let dataBytes = serializers.txSerializer(for: .bytes)(data)
        #expect((try serializers.rxDeserializer(for: .bytes)(dataBytes)) as? Data == data)
    }
}

struct GeneratedClientUnitCoverageTests {
    @Test func generatedClientUnaryAndCompositeMethodsRoundTripOverLoopbackDispatcher() async throws {
        let client = TestbedClient(connection: LoopbackConnection())

        #expect(try await client.echo(message: "hello from swift") == "hello from swift")
        #expect(try await client.reverse(message: "hello") == "olleh")

        #expect(try await client.divide(dividend: 10, divisor: 3) == .success(3))
        #expect(try await client.divide(dividend: 10, divisor: 0) == .failure(.divisionByZero))
        #expect(try await client.divide(dividend: .min, divisor: -1) == .failure(.overflow))

        let found = try await client.lookup(id: 1)
        if case .success(let person) = found {
            #expect(person.name == "Alice")
        } else {
            Issue.record("lookup(id: 1) should succeed")
        }

        let foundNoEmail = try await client.lookup(id: 2)
        if case .success(let person) = foundNoEmail {
            #expect(person.name == "Bob")
            #expect(person.email == nil)
        } else {
            Issue.record("lookup(id: 2) should succeed")
        }

        let notFound = try await client.lookup(id: 999)
        if case .failure(let error) = notFound {
            #expect(error == .notFound)
        } else {
            Issue.record("lookup(id: 999) should fail with notFound")
        }

        let accessDenied = try await client.lookup(id: 100)
        if case .failure(let error) = accessDenied {
            #expect(error == .accessDenied)
        } else {
            Issue.record("lookup(id: 100) should fail with accessDenied")
        }

        let echoedPoint = try await client.echoPoint(point: Point(x: 42, y: -7))
        #expect(echoedPoint.x == 42)
        #expect(echoedPoint.y == -7)

        let dave = try await client.createPerson(name: "Dave", age: 40, email: "dave@example.com")
        #expect(dave.name == "Dave")
        #expect(dave.age == 40)
        #expect(dave.email == "dave@example.com")

        let eve = try await client.createPerson(name: "Eve", age: 25, email: nil)
        #expect(eve.name == "Eve")
        #expect(eve.email == nil)

        let area = try await client.rectangleArea(
            rect: Rectangle(
                topLeft: Point(x: 0, y: 10),
                bottomRight: Point(x: 5, y: 0),
                label: nil
            )
        )
        #expect(abs(area - 50.0) < 1e-9)

        #expect(try await client.parseColor(name: "red") == .red)
        #expect(try await client.parseColor(name: "green") == .green)
        #expect(try await client.parseColor(name: "blue") == .blue)
        #expect(try await client.parseColor(name: "purple") == nil)

        let points = try await client.getPoints(count: 5)
        #expect(points.count == 5)
        #expect(points.first?.x == 0)
        #expect(points.last?.x == 4)

        let swapped = try await client.swapPair(pair: (99, "hello"))
        #expect(swapped.0 == "hello")
        #expect(swapped.1 == 99)

        let bytes = Data([1, 2, 3, 255, 0, 128])
        #expect(try await client.echoBytes(data: bytes) == bytes)
        #expect(try await client.echoBool(b: true) == true)
        #expect(try await client.echoBool(b: false) == false)

        for n: UInt64 in [0, 1, 1_000_000_000_000, .max] {
            #expect(try await client.echoU64(n: n) == n)
        }

        #expect(try await client.echoOptionString(s: "hello") == "hello")
        #expect(try await client.echoOptionString(s: nil) == nil)

        let taggedPoint = try await client.describePoint(label: "origin", x: 0, y: 0, active: true)
        #expect(taggedPoint.label == "origin")
        #expect(taggedPoint.active)

        #expect(try await client.allColors() == [.red, .green, .blue])
        #expect(try await client.shapeArea(shape: .rectangle(width: 3.0, height: 4.0)) == 12.0)

        let pointShape = try await client.echoShape(shape: .point)
        if case .point = pointShape {
        } else {
            Issue.record("echoShape(.point) should round-trip")
        }

        let circleShape = try await client.echoShape(shape: .circle(radius: 3.14))
        if case .circle(let radius) = circleShape {
            #expect(radius == 3.14)
        } else {
            Issue.record("echoShape(.circle) should round-trip")
        }

        let rectShape = try await client.echoShape(shape: .rectangle(width: 2.0, height: 5.0))
        if case .rectangle(let width, let height) = rectShape {
            #expect(width == 2.0)
            #expect(height == 5.0)
        } else {
            Issue.record("echoShape(.rectangle) should round-trip")
        }

        let canvas = try await client.createCanvas(
            name: "enum-canvas",
            shapes: [.point, .circle(radius: 2.5)],
            background: .green
        )
        #expect(canvas.name == "enum-canvas")
        #expect(canvas.background == .green)
        #expect(canvas.shapes.count == 2)

        let processed = try await client.processMessage(msg: .data(Data([1, 2, 3, 4])))
        if case .data(let payload) = processed {
            #expect(payload == Data([4, 3, 2, 1]))
        } else {
            Issue.record("processMessage(.data) should return reversed data")
        }
    }

    @Test func runClientScenarioRejectsUnknownScenario() async {
        let client = TestbedClient(connection: LoopbackConnection())

        do {
            try await runClientScenario(client: client, scenario: "not-a-scenario")
            Issue.record("expected SubjectError.unknownScenario")
        } catch let error as SubjectError {
            guard case .unknownScenario = error else {
                Issue.record("expected unknownScenario, got \(error)")
                return
            }
        } catch {
            Issue.record("expected SubjectError.unknownScenario, got \(error)")
        }
    }

    @Test func generatedClientPipeliningRoundTripsOverLoopbackDispatcher() async throws {
        let client = TestbedClient(connection: LoopbackConnection())

        try await withThrowingTaskGroup(of: Void.self) { group in
            for i in 0..<10 {
                group.addTask {
                    let expected = "msg\(i)"
                    let result = try await client.echo(message: expected)
                    #expect(result == expected)
                }
            }
            try await group.waitForAll()
        }
    }

    @Test func generatedClientClientToServerStreamingCrossesInitialCreditWindow() async throws {
        let client = TestbedClient(connection: LoopbackConnection())

        try await withTimeout(milliseconds: 1_000) {
            let (tx, rx) = channel(
                serialize: { encodeI32($0) },
                deserialize: { bytes in
                    var offset = 0
                    return try decodeI32(from: Data(bytes), offset: &offset)
                }
            )

            async let call: Int64 = client.sum(numbers: rx)
            try await Task.sleep(nanoseconds: 50_000_000)
            for i in 1...40 {
                try await tx.send(Int32(i))
            }
            tx.close()
            let result = try await call
            #expect(result == 820)
        }
    }

    @Test func generatedClientServerToClientStreamingCrossesInitialCreditWindow() async throws {
        let client = TestbedClient(connection: LoopbackConnection())

        try await withTimeout(milliseconds: 1_000) {
            let (tx, rx) = channel(
                serialize: { encodeI32($0) },
                deserialize: { bytes in
                    var offset = 0
                    return try decodeI32(from: Data(bytes), offset: &offset)
                }
            )

            async let call: Void = client.generate(count: 64, output: tx)
            try await Task.sleep(nanoseconds: 50_000_000)
            let values = try await collectValues(from: rx)
            try await call

            #expect(values.count == 64)
            #expect(values.first == 0)
            #expect(values.last == 63)
        }
    }

    @Test func generatedClientBidirectionalStreamingCrossesInitialCreditWindow() async throws {
        let client = TestbedClient(connection: LoopbackConnection())

        try await withTimeout(milliseconds: 1_000) {
            let (inputTx, inputRx) = channel(
                serialize: { encodeString($0) },
                deserialize: { bytes in
                    var offset = 0
                    return try decodeString(from: Data(bytes), offset: &offset)
                }
            )
            let (outputTx, outputRx) = channel(
                serialize: { encodeString($0) },
                deserialize: { bytes in
                    var offset = 0
                    return try decodeString(from: Data(bytes), offset: &offset)
                }
            )

            async let call: Void = client.transform(input: inputRx, output: outputTx)
            try await Task.sleep(nanoseconds: 50_000_000)
            async let echoed: [String] = collectValues(from: outputRx)

            for i in 0..<40 {
                try await inputTx.send("msg-\(i)")
            }
            inputTx.close()

            let echoedValues = try await echoed
            try await call

            #expect(echoedValues.count == 40)
            #expect(echoedValues.first == "msg-0")
            #expect(echoedValues.last == "msg-39")
        }
    }
}
