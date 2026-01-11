/// Swift subject binary for the roam compliance suite.
///
/// This uses the roam-runtime library to validate that the Swift implementation
/// is compliant with the roam protocol spec.

import Foundation
import RoamRuntime

// MARK: - Testbed Service Implementation

/// Implementation of the Testbed service.
struct TestbedService: TestbedHandler {

    func echo(message: String) async throws -> String {
        log("echo called: \(message)")
        return message
    }

    func reverse(message: String) async throws -> String {
        log("reverse called: \(message)")
        return String(message.reversed())
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
        for i in 0..<Int32(count) {
            log("  sending: \(i)")
            try output.send(i)
        }
        log("generate complete, about to return (close will be sent by dispatcher)")
    }

    func transform(input: Rx<String>, output: Tx<String>) async throws {
        log("transform called")
        for try await s in input {
            log("  transforming: \(s)")
            try output.send(s)
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

// MARK: - Streaming Dispatcher Adapter

/// Adapter to make TestbedStreamingDispatcher conform to ServiceDispatcher.
final class TestbedDispatcherAdapter: ServiceDispatcher, @unchecked Sendable {
    private let handler: TestbedHandler

    init(handler: TestbedHandler) {
        self.handler = handler
    }

    func preregister(
        methodId: UInt64,
        payload: [UInt8],
        registry: ChannelRegistry
    ) async {
        // Pre-register channels before the handler task is spawned.
        // This ensures channels are known before any Data messages arrive.
        await TestbedStreamingDispatcher.preregisterChannels(
            methodId: methodId,
            payload: Data(payload),
            registry: registry
        )
    }

    func dispatch(
        methodId: UInt64,
        payload: [UInt8],
        requestId: UInt64,
        registry: ChannelRegistry,
        taskTx: @escaping @Sendable (TaskMessage) -> Void
    ) async {
        let dispatcher = TestbedStreamingDispatcher(
            handler: handler,
            registry: registry,
            taskSender: taskTx
        )

        // Dispatch the request
        await dispatcher.dispatch(methodId: methodId, requestId: requestId, payload: Data(payload))
    }
}

// MARK: - Logging

func log(_ message: String) {
    let data = "[\(ProcessInfo.processInfo.processIdentifier)] \(message)\n".data(using: .utf8)!
    FileHandle.standardError.write(data)
}

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

    // Use roam-runtime's connect function
    let transport = try await connect(host: host, port: port)
    log("connected")

    // Establish connection as acceptor (we're the server/handler, but we connected)
    let hello = Hello.v1(maxPayloadSize: 1024 * 1024, initialChannelCredit: 64 * 1024)
    let handler = TestbedService()
    let dispatcher = TestbedDispatcherAdapter(handler: handler)

    let (_, driver) = try await establishAcceptor(
        transport: transport,
        ourHello: hello,
        dispatcher: dispatcher
    )

    log("handshake complete, running driver")

    // Run driver
    try await driver.run()

    log("driver finished")
}

// MARK: - Client Mode

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

    // Use roam-runtime's connect function
    let transport = try await connect(host: host, port: port)
    log("connected")

    // Establish connection as initiator
    let hello = Hello.v1(maxPayloadSize: 1024 * 1024, initialChannelCredit: 64 * 1024)
    let handler = TestbedService()
    let dispatcher = TestbedDispatcherAdapter(handler: handler)

    let (handle, driver) = try await establishInitiator(
        transport: transport,
        ourHello: hello,
        dispatcher: dispatcher
    )

    log("handshake complete")

    // Spawn driver
    Task {
        do {
            try await driver.run()
        } catch {
            log("driver error: \(error)")
        }
    }

    // Create client
    let client = TestbedClient(connection: handle)

    // Run test scenario
    let scenario = ProcessInfo.processInfo.environment["CLIENT_SCENARIO"] ?? "echo"
    log("running client scenario: \(scenario)")

    switch scenario {
    case "echo":
        let result = try await client.echo(message: "hello from swift")
        log("echo result: \(result)")

    default:
        log("unknown CLIENT_SCENARIO: \(scenario)")
    }
}

// MARK: - Errors

enum SubjectError: Error {
    case missingEnv
    case invalidAddr
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
