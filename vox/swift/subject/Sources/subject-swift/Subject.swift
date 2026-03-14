/// Swift subject binary for the roam compliance suite.
///
/// This uses the roam-runtime library to validate that the Swift implementation
/// is compliant with the roam protocol spec.

import Foundation
import RoamRuntime

// MARK: - Testbed Service Implementation

/// Implementation of the Testbed service.
struct TestbedService: TestbedHandler {
    private func streamRetryProbeValues(count: UInt32, output: Tx<Int32>) async throws {
        for i in 0..<Int32(count) {
            log("  sending: \(i)")
            try await output.send(i)
        }
    }

    func echo(message: String) async throws -> String {
        log("echo called: \(message)")
        return message
    }

    func reverse(message: String) async throws -> String {
        log("reverse called: \(message)")
        return String(message.reversed())
    }

    func divide(dividend: Int64, divisor: Int64) async throws -> Result<Int64, MathError> {
        log("divide called: \(dividend) / \(divisor)")
        if divisor == 0 {
            return .failure(.divisionByZero)
        }
        return .success(dividend / divisor)
    }

    func lookup(id: UInt32) async throws -> Result<Person, LookupError> {
        log("lookup called: \(id)")
        switch id {
        case 1:
            return .success(Person(name: "Alice", age: 30, email: "alice@example.com"))
        case 2:
            return .success(Person(name: "Bob", age: 25, email: nil))
        case 3:
            return .success(Person(name: "Charlie", age: 35, email: "charlie@example.com"))
        default:
            return .failure(.notFound)
        }
    }

    func sum(numbers: Rx<Int32>) async throws -> Int64 {
        log("sum called, starting to receive numbers")
        var total: Int64 = 0
        for try await n in numbers {
            log("  received: \(n)")
            total += Int64(n)
        }
        log("sum complete: \(total)")
        return total
    }

    func generate(count: UInt32, output: Tx<Int32>) async throws {
        log("generate called: count=\(count)")
        try await streamRetryProbeValues(count: count, output: output)
        log("generate complete, about to return (close will be sent by dispatcher)")
    }

    func generateRetryNonIdem(count: UInt32, output: Tx<Int32>) async throws {
        log("generateRetryNonIdem called: count=\(count)")
        try await streamRetryProbeValues(count: count, output: output)
        log("generateRetryNonIdem complete, about to return (close will be sent by dispatcher)")
    }

    func generateRetryIdem(count: UInt32, output: Tx<Int32>) async throws {
        log("generateRetryIdem called: count=\(count)")
        try await streamRetryProbeValues(count: count, output: output)
        log("generateRetryIdem complete, about to return (close will be sent by dispatcher)")
    }

    func transform(input: Rx<String>, output: Tx<String>) async throws {
        log("transform called")
        for try await s in input {
            log("  transforming: \(s)")
            try await output.send(s)
        }
        log("transform complete")
    }

    func echoPoint(point: Point) async throws -> Point {
        return point
    }

    func createPerson(name: String, age: UInt8, email: String?) async throws -> Person {
        return Person(name: name, age: age, email: email)
    }

    func rectangleArea(rect: Rectangle) async throws -> Double {
        let width = abs(Double(rect.bottomRight.x - rect.topLeft.x))
        let height = abs(Double(rect.bottomRight.y - rect.topLeft.y))
        return width * height
    }

    func parseColor(name: String) async throws -> Color? {
        switch name.lowercased() {
        case "red": return .red
        case "green": return .green
        case "blue": return .blue
        default: return nil
        }
    }

    func shapeArea(shape: Shape) async throws -> Double {
        switch shape {
        case .circle(let radius):
            return Double.pi * radius * radius
        case .rectangle(let width, let height):
            return width * height
        case .point:
            return 0.0
        }
    }

    func createCanvas(name: String, shapes: [Shape], background: Color) async throws -> Canvas {
        return Canvas(name: name, shapes: shapes, background: background)
    }

    func processMessage(msg: Message) async throws -> Message {
        switch msg {
        case .text(let s):
            return .text("processed: \(s)")
        case .number(let n):
            return .number(n * 2)
        case .data(let d):
            return .data(Data(d.reversed()))
        }
    }

    func getPoints(count: UInt32) async throws -> [Point] {
        return (0..<Int32(count)).map { Point(x: $0, y: $0 * 2) }
    }

    func swapPair(pair: (Int32, String)) async throws -> (String, Int32) {
        return (pair.1, pair.0)
    }
}

// MARK: - Channeling Dispatcher Adapter

/// Adapter to make TestbedChannelingDispatcher conform to ServiceDispatcher.
final class TestbedDispatcherAdapter: ServiceDispatcher, @unchecked Sendable {
    private let handler: TestbedHandler

    init(handler: TestbedHandler) {
        self.handler = handler
    }

    func retryPolicy(methodId: UInt64) -> RetryPolicy {
        TestbedChannelingDispatcher.retryPolicy(methodId: methodId)
    }

    func preregister(
        methodId: UInt64,
        payload: [UInt8],
        channels: [UInt64],
        registry: ChannelRegistry
    ) async {
        // Pre-register channels before the handler task is spawned.
        // This ensures channels are known before any Data messages arrive.
        await TestbedChannelingDispatcher.preregisterChannels(
            methodId: methodId,
            channels: channels,
            registry: registry
        )
    }

    func dispatch(
        methodId: UInt64,
        payload: [UInt8],
        channels: [UInt64],
        requestId: UInt64,
        registry: ChannelRegistry,
        taskTx: @escaping @Sendable (TaskMessage) -> Void
    ) async {
        let dispatcher = TestbedChannelingDispatcher(
            handler: handler,
            registry: registry,
            taskSender: taskTx
        )

        // Dispatch the request
        await dispatcher.dispatch(
            methodId: methodId,
            requestId: requestId,
            channels: channels,
            payload: Data(payload)
        )
    }
}

// MARK: - Logging

func log(_ message: String) {
    let pid = ProcessInfo.processInfo.processIdentifier
    NSLog("%@", "[\(pid)] \(message)")
}

func subjectConduit() -> TransportConduitKind {
    ProcessInfo.processInfo.environment["SPEC_CONDUIT"] == "stable" ? .stable : .bare
}

let retryProbeItemCount: UInt32 = 40

// MARK: - Server Mode

/// In "server" mode, the subject acts as the RPC server (handler).
/// But it CONNECTS TO the test harness (specified by PEER_ADDR).
func runServer() async throws {
    guard let addr = ProcessInfo.processInfo.environment["PEER_ADDR"] else {
        log("PEER_ADDR not set")
        throw SubjectError.missingEnv
    }

    log("connecting to \(addr)")

    // Parse host:port
    let parts = addr.split(separator: ":")
    guard parts.count == 2, let port = Int(parts[1]) else {
        log("invalid PEER_ADDR format")
        throw SubjectError.invalidAddr
    }
    let host = String(parts[0])

    let connector = TcpConnector(host: host, port: port, transport: subjectConduit())
    log("connecting via \(connector.transport)")

    // r[impl core.conn.accept-required] - Check if we should accept incoming virtual connections.
    let acceptConnections = ProcessInfo.processInfo.environment["ACCEPT_CONNECTIONS"]
        .map { $0 == "1" }
        ?? true

    let handler = TestbedService()
    let dispatcher = TestbedDispatcherAdapter(handler: handler)

    let session = try await Session.initiator(
        connector,
        dispatcher: dispatcher,
        acceptConnections: acceptConnections
    )

    log("handshake complete, running driver")

    // Run driver
    try await session.run()

    log("driver finished")
}

// MARK: - Client Mode

func runClientScenario(client: TestbedClient, scenario: String) async throws {
    log("running client scenario: \(scenario)")

    switch scenario {
    case "echo":
        let result = try await client.echo(message: "hello from swift")
        log("echo result: \(result)")
    case "sum":
        let (tx, rx) = channel(
            serialize: { encodeI32($0) },
            deserialize: { bytes in
                var offset = 0
                return try decodeI32(from: Data(bytes), offset: &offset)
            }
        )

        let sender = Task {
            try await Task.sleep(nanoseconds: 50_000_000)
            try await tx.send(1)
            try await tx.send(2)
            try await tx.send(3)
            try await tx.send(4)
            try await tx.send(5)
            tx.close()
        }

        let result = try await client.sum(numbers: rx)
        _ = try await sender.value
        guard result == 15 else {
            log("sum expected 15, got \(result)")
            throw SubjectError.invalidResponse
        }
        log("sum result: \(result)")
    case "generate":
        let (tx, rx) = channel(
            serialize: { encodeI32($0) },
            deserialize: { bytes in
                var offset = 0
                return try decodeI32(from: Data(bytes), offset: &offset)
            }
        )

        try await client.generate(count: 5, output: tx)

        var received: [Int32] = []
        for try await n in rx {
            received.append(n)
        }
        guard received == [0, 1, 2, 3, 4] else {
            log("generate expected [0, 1, 2, 3, 4], got \(received)")
            throw SubjectError.invalidResponse
        }
        log("generate result OK: \(received)")
    case "channel_retry_non_idem":
        let (tx, rx) = channel(
            serialize: { encodeI32($0) },
            deserialize: { bytes in
                var offset = 0
                return try decodeI32(from: Data(bytes), offset: &offset)
            }
        )

        let callTask = Task {
            try await client.generateRetryNonIdem(count: retryProbeItemCount, output: tx)
        }
        let receiveTask = Task { () throws -> [Int32] in
            var received: [Int32] = []
            for try await n in rx {
                received.append(n)
            }
            return received
        }

        do {
            try await callTask.value
            log("channel_retry_non_idem expected indeterminate")
            throw SubjectError.invalidResponse
        } catch RoamError.indeterminate {
        } catch {
            log("channel_retry_non_idem unexpected error: \(error)")
            throw error
        }

        let received = try await receiveTask.value
        let expected = (0..<Int32(received.count)).map { $0 }
        guard received == expected else {
            log("channel_retry_non_idem expected prefix \(expected), got \(received)")
            throw SubjectError.invalidResponse
        }
    case "channel_retry_idem":
        let (tx, rx) = channel(
            serialize: { encodeI32($0) },
            deserialize: { bytes in
                var offset = 0
                return try decodeI32(from: Data(bytes), offset: &offset)
            }
        )

        let callTask = Task {
            try await client.generateRetryIdem(count: retryProbeItemCount, output: tx)
        }
        let receiveTask = Task { () throws -> [Int32] in
            var received: [Int32] = []
            for try await n in rx {
                received.append(n)
            }
            return received
        }

        try await callTask.value
        let received = try await receiveTask.value
        guard let restart = received.enumerated().dropFirst().first(where: { $0.element == 0 })?.offset else {
            log("channel_retry_idem expected retry restart, got \(received)")
            throw SubjectError.invalidResponse
        }
        let expectedPrefix = (0..<Int32(restart)).map { $0 }
        guard Array(received[..<restart]) == expectedPrefix else {
            log("channel_retry_idem expected prefix \(expectedPrefix), got \(Array(received[..<restart]))")
            throw SubjectError.invalidResponse
        }
        let expectedRerun = (0..<Int32(retryProbeItemCount)).map { $0 }
        guard Array(received[restart...]) == expectedRerun else {
            log("channel_retry_idem expected rerun \(expectedRerun), got \(Array(received[restart...]))")
            throw SubjectError.invalidResponse
        }
    case "divide_error":
        let result = try await client.divide(dividend: 10, divisor: 0)
        guard case .failure(.divisionByZero) = result else {
            log("divide_error expected division_by_zero")
            throw SubjectError.invalidResponse
        }
        log("divide_error result OK")
    case "shape_area":
        let result = try await client.shapeArea(shape: .rectangle(width: 3.0, height: 4.0))
        guard result == 12.0 else {
            log("shape_area expected 12.0, got \(result)")
            throw SubjectError.invalidResponse
        }
        log("shape_area result: \(result)")
    case "create_canvas":
        let result = try await client.createCanvas(
            name: "enum-canvas",
            shapes: [.point, .circle(radius: 2.5)],
            background: .green
        )
        guard result.name == "enum-canvas" else {
            log("create_canvas expected name enum-canvas, got \(result.name)")
            throw SubjectError.invalidResponse
        }
        guard case .green = result.background else {
            log("create_canvas expected green background")
            throw SubjectError.invalidResponse
        }
        guard result.shapes.count == 2 else {
            log("create_canvas expected 2 shapes, got \(result.shapes.count)")
            throw SubjectError.invalidResponse
        }
        guard case .point = result.shapes[0] else {
            log("create_canvas expected first shape to be point")
            throw SubjectError.invalidResponse
        }
        guard case .circle(let radius) = result.shapes[1], radius == 2.5 else {
            log("create_canvas expected second shape to be circle(radius: 2.5)")
            throw SubjectError.invalidResponse
        }
        log("create_canvas result OK")
    case "process_message":
        let result = try await client.processMessage(msg: .data(Data([1, 2, 3, 4])))
        guard case .data(let payload) = result, payload == Data([4, 3, 2, 1]) else {
            log("process_message returned unexpected payload")
            throw SubjectError.invalidResponse
        }
        log("process_message result OK")

    default:
        log("unknown CLIENT_SCENARIO: \(scenario)")
        throw SubjectError.unknownScenario
    }
}

func runClient() async throws {
    guard let addr = ProcessInfo.processInfo.environment["PEER_ADDR"] else {
        log("PEER_ADDR not set")
        throw SubjectError.missingEnv
    }

    log("connecting to \(addr)")

    // Parse host:port
    let parts = addr.split(separator: ":")
    guard parts.count == 2, let port = Int(parts[1]) else {
        log("invalid PEER_ADDR format")
        throw SubjectError.invalidAddr
    }
    let host = String(parts[0])

    let connector = TcpConnector(host: host, port: port, transport: subjectConduit())
    log("connecting via \(connector.transport)")

    let handler = TestbedService()
    let dispatcher = TestbedDispatcherAdapter(handler: handler)

    let session = try await Session.initiator(
        connector,
        dispatcher: dispatcher
    )

    log("handshake complete")

    // Spawn driver
    Task {
        do {
            try await session.run()
        } catch {
            log("driver error: \(error)")
        }
    }

    // Create client
    let client = TestbedClient(connection: session.connection)
    let scenario = ProcessInfo.processInfo.environment["CLIENT_SCENARIO"] ?? "echo"
    try await runClientScenario(client: client, scenario: scenario)
}

func runShmClient() async throws {
    guard let controlSock = ProcessInfo.processInfo.environment["SHM_CONTROL_SOCK"] else {
        log("SHM_CONTROL_SOCK not set")
        throw SubjectError.missingEnv
    }
    guard let sid = ProcessInfo.processInfo.environment["SHM_SESSION_ID"] else {
        log("SHM_SESSION_ID not set")
        throw SubjectError.missingEnv
    }

    let ticket = try requestShmBootstrapTicket(controlSocketPath: controlSock, sid: sid)
    let transport = try ShmGuestTransport.attach(ticket: ticket)

    let handler = TestbedService()
    let dispatcher = TestbedDispatcherAdapter(handler: handler)
    let (handle, driver, _, _) = try await establishShmGuest(
        transport: transport,
        dispatcher: dispatcher,
        conduit: subjectConduit()
    )

    let driverTask = Task {
        do {
            try await driver.run()
        } catch {
            log("driver error: \(error)")
        }
    }

    let client = TestbedClient(connection: handle)
    let scenario = ProcessInfo.processInfo.environment["CLIENT_SCENARIO"] ?? "echo"
    try await runClientScenario(client: client, scenario: scenario)

    try await transport.close()
    _ = await driverTask.result
}

/// SHM mode equivalent of `runServer`: attach over SHM and only serve incoming RPC.
func runShmServer() async throws {
    guard let controlSock = ProcessInfo.processInfo.environment["SHM_CONTROL_SOCK"] else {
        log("SHM_CONTROL_SOCK not set")
        throw SubjectError.missingEnv
    }
    guard let sid = ProcessInfo.processInfo.environment["SHM_SESSION_ID"] else {
        log("SHM_SESSION_ID not set")
        throw SubjectError.missingEnv
    }

    let ticket = try requestShmBootstrapTicket(controlSocketPath: controlSock, sid: sid)
    let transport = try ShmGuestTransport.attach(ticket: ticket)

    let acceptConnections = ProcessInfo.processInfo.environment["ACCEPT_CONNECTIONS"] == "1"
    let handler = TestbedService()
    let dispatcher = TestbedDispatcherAdapter(handler: handler)

    let (_, driver, _, _) = try await establishShmGuest(
        transport: transport,
        dispatcher: dispatcher,
        role: .initiator,
        conduit: subjectConduit(),
        acceptConnections: acceptConnections
    )
    try await driver.run()
}

/// SHM host mode: create the hub segment locally, serve one bootstrap request,
/// then run as RPC acceptor over a host-side SHM transport.
func runShmHostServer() async throws {
    guard let controlSock = ProcessInfo.processInfo.environment["SHM_CONTROL_SOCK"] else {
        log("SHM_CONTROL_SOCK not set")
        throw SubjectError.missingEnv
    }
    guard let sid = ProcessInfo.processInfo.environment["SHM_SESSION_ID"] else {
        log("SHM_SESSION_ID not set")
        throw SubjectError.missingEnv
    }

    let hubPath = ProcessInfo.processInfo.environment["SHM_HUB_PATH"]
        ?? "/tmp/roam-swift-subject-\(UUID().uuidString).shm"
    let acceptConnections = ProcessInfo.processInfo.environment["ACCEPT_CONNECTIONS"] == "1"

    let segment = try ShmHostSegment.create(
        path: hubPath,
        config: ShmHostSegmentConfig(
            maxGuests: 1,
            bipbufCapacity: 64 * 1024,
            maxPayloadSize: 1024 * 1024,
            inlineThreshold: 256,
            heartbeatInterval: 0,
            sizeClasses: [
                ShmVarSlotClass(slotSize: 256, count: 64),
                ShmVarSlotClass(slotSize: 1024, count: 32),
                ShmVarSlotClass(slotSize: 4096, count: 16),
                ShmVarSlotClass(slotSize: 16384, count: 8),
                ShmVarSlotClass(slotSize: 65536, count: 4),
                ShmVarSlotClass(slotSize: 262144, count: 2),
            ]
        )
    )
    let prepared = try segment.reservePeer()

    let listenerFd = try makeUnixListener(path: controlSock)
    defer {
        close(listenerFd)
        unlink(controlSock)
    }

    let clientFd = accept(listenerFd, nil, nil)
    guard clientFd >= 0 else {
        throw SubjectError.socketSetupFailed
    }
    defer { close(clientFd) }

    let magic = try readExactly(fd: clientFd, count: 4)
    guard magic == [UInt8]("RSH0".utf8) else {
        throw SubjectError.invalidBootstrapRequest
    }
    let sidLenBytes = try readExactly(fd: clientFd, count: 2)
    let sidLen = Int(UInt16(sidLenBytes[0]) | (UInt16(sidLenBytes[1]) << 8))
    let sidBytes = try readExactly(fd: clientFd, count: sidLen)
    guard let receivedSid = String(bytes: sidBytes, encoding: .utf8), receivedSid == sid else {
        throw SubjectError.bootstrapSidMismatch
    }

    try prepared.sendBootstrapSuccess(controlFd: clientFd, hubPath: hubPath)

    let transport = try prepared.intoTransport()
    _ = try await performAcceptorTransportPrologue(
        transport: transport,
        supportedConduit: .bare
    )
    let conduit = BareConduit(link: transport)
    let handler = TestbedService()
    let dispatcher = TestbedDispatcherAdapter(handler: handler)

    let (_, driver, _, _) = try await establishAcceptor(
        conduit: conduit,
        dispatcher: dispatcher,
        acceptConnections: acceptConnections
    )
    prepared.closeGuestEndpoints()
    try await driver.run()
}

private func makeUnixListener(path: String) throws -> Int32 {
    unlink(path)

    #if canImport(Glibc)
    let fd = socket(AF_UNIX, Int32(SOCK_STREAM.rawValue), 0)
    #else
    let fd = socket(AF_UNIX, SOCK_STREAM, 0)
    #endif
    guard fd >= 0 else {
        throw SubjectError.socketSetupFailed
    }

    var addr = sockaddr_un()
    addr.sun_family = sa_family_t(AF_UNIX)

    let pathBytes = [UInt8](path.utf8)
    let maxPathLen = MemoryLayout.size(ofValue: addr.sun_path)
    guard pathBytes.count < maxPathLen else {
        close(fd)
        throw SubjectError.socketSetupFailed
    }

    withUnsafeMutablePointer(to: &addr.sun_path) { sunPathPtr in
        let raw = UnsafeMutableRawPointer(sunPathPtr)
        raw.initializeMemory(as: UInt8.self, repeating: 0, count: maxPathLen)
        raw.copyMemory(from: pathBytes, byteCount: pathBytes.count)
    }

    let bindResult = withUnsafePointer(to: &addr) { ptr in
        ptr.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockPtr in
            bind(fd, sockPtr, socklen_t(MemoryLayout<sockaddr_un>.size))
        }
    }
    guard bindResult == 0, listen(fd, 1) == 0 else {
        close(fd)
        throw SubjectError.socketSetupFailed
    }

    return fd
}

private func readExactly(fd: Int32, count: Int) throws -> [UInt8] {
    if count == 0 { return [] }

    var out = [UInt8](repeating: 0, count: count)
    var offset = 0
    while offset < count {
        let n = out.withUnsafeMutableBytes { raw in
            read(fd, raw.baseAddress!.advanced(by: offset), count - offset)
        }
        if n < 0 {
            if errno == EINTR {
                continue
            }
            throw SubjectError.socketSetupFailed
        }
        if n == 0 {
            throw SubjectError.invalidBootstrapRequest
        }
        offset += n
    }
    return out
}

// MARK: - Errors

enum SubjectError: Error {
    case missingEnv
    case invalidAddr
    case invalidResponse
    case unknownScenario
    case socketSetupFailed
    case invalidBootstrapRequest
    case bootstrapSidMismatch
}

// MARK: - Main Entry Point

@main
struct SubjectMain {
    static func main() async {
        let mode = ProcessInfo.processInfo.environment["SUBJECT_MODE"] ?? "server"
        log("subject-swift starting in \(mode) mode")

        do {
            switch mode {
            case "server":
                try await runServer()
            case "client":
                try await runClient()
            case "shm-client":
                try await runShmClient()
            case "shm-server":
                try await runShmServer()
            case "shm-host-server":
                try await runShmHostServer()
            default:
                log("unknown SUBJECT_MODE: \(mode)")
                exit(1)
            }
        } catch {
            log("error: \(error)")
            exit(1)
        }
    }
}
