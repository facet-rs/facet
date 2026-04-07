/// Swift subject binary for the vox compliance suite.
///
/// This uses the vox-runtime library to validate that the Swift implementation
/// is compliant with the vox protocol spec.

import Foundation
import VoxRuntime

// MARK: - Testbed Service Implementation

/// Implementation of the Testbed service.
struct TestbedService: TestbedHandler {
    private func streamValues(
        count: UInt32,
        output: Tx<Int32>,
        pauseAfter: Int32? = nil,
        pauseNanoseconds: UInt64 = 0
    ) async throws {
        for i in 0..<Int32(count) {
            log("  sending: \(i)")
            try await output.send(i)
            if let pauseAfter, i + 1 == pauseAfter, count > UInt32(pauseAfter), pauseNanoseconds > 0 {
                try await Task.sleep(nanoseconds: pauseNanoseconds)
            }
        }
    }

    func echo(message: String) async throws -> String {
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
        if dividend == .min && divisor == -1 {
            return .failure(.overflow)
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
        case 100...199:
            return .failure(.accessDenied)
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
        try await streamValues(count: count, output: output)
        log("generate complete, about to return (close will be sent by dispatcher)")
    }

    func generateRetryNonIdem(count: UInt32, output: Tx<Int32>) async throws {
        log("generateRetryNonIdem called: count=\(count)")
        try await streamValues(
            count: count,
            output: output,
            pauseAfter: min(3, Int32(count)),
            pauseNanoseconds: 5_000_000_000
        )
        log("generateRetryNonIdem complete, about to return (close will be sent by dispatcher)")
    }

    func generateRetryIdem(count: UInt32, output: Tx<Int32>) async throws {
        log("generateRetryIdem called: count=\(count)")
        try await streamValues(
            count: count,
            output: output,
            pauseAfter: min(3, Int32(count)),
            pauseNanoseconds: 5_000_000_000
        )
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

    func echoGnarly(payload: GnarlyPayload) async throws -> GnarlyPayload {
        payload
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

    func echoBytes(data: Data) async throws -> Data {
        data
    }

    func echoBool(b: Bool) async throws -> Bool {
        b
    }

    func echoU64(n: UInt64) async throws -> UInt64 {
        n
    }

    func echoOptionString(s: String?) async throws -> String? {
        s
    }

    func sumLarge(numbers: Rx<Int32>) async throws -> Int64 {
        try await sum(numbers: numbers)
    }

    func generateLarge(count: UInt32, output: Tx<Int32>) async throws {
        try await generate(count: count, output: output)
    }

    func allColors() async throws -> [Color] {
        [.red, .green, .blue]
    }

    func describePoint(label: String, x: Int32, y: Int32, active: Bool) async throws -> TaggedPoint
    {
        TaggedPoint(label: label, x: x, y: y, active: active)
    }

    func echoShape(shape: Shape) async throws -> Shape {
        shape
    }

    func echoStatusV1(status: Status) async throws -> Status {
        status
    }

    func echoTagV1(tag: Tag) async throws -> Tag {
        tag
    }

    func echoProfile(profile: Profile) async throws -> Profile {
        profile
    }

    func echoRecord(record: Record) async throws -> Record {
        record
    }

    func echoStatus(status: Status) async throws -> Status {
        status
    }

    func echoTag(tag: Tag) async throws -> Tag {
        tag
    }

    func echoMeasurement(m: Measurement) async throws -> Measurement {
        m
    }

    func echoConfig(c: Config) async throws -> Config {
        c
    }
}

// MARK: - Logging

func log(_ message: String) {
    let pid = ProcessInfo.processInfo.processIdentifier
    NSLog("%@", "[\(pid)] \(message)")
}

func subjectConduit() -> ConduitKind {
    ProcessInfo.processInfo.environment["SPEC_CONDUIT"] == "stable" ? .stable : .bare
}

let retryProbeItemCount: UInt32 = 40

func sameShape(_ lhs: Shape, _ rhs: Shape) -> Bool {
    switch (lhs, rhs) {
    case (.point, .point):
        true
    case (.circle(let lRadius), .circle(let rRadius)):
        lRadius == rRadius
    case (.rectangle(let lWidth, let lHeight), .rectangle(let rWidth, let rHeight)):
        lWidth == rWidth && lHeight == rHeight
    default:
        false
    }
}

// MARK: - Server Mode

/// In "server" mode, the subject acts as the RPC server (handler).
/// But it CONNECTS TO the test harness (specified by PEER_ADDR).
func runServer() async throws {
    let handler = TestbedService()
    let dispatcher = TestbedDispatcher(handler: handler)
    guard let addr = ProcessInfo.processInfo.environment["PEER_ADDR"] else {
        log("PEER_ADDR not set")
        throw SubjectError.missingEnv
    }

    let transport = subjectConduit()
    let acceptConnections = ProcessInfo.processInfo.environment["ACCEPT_CONNECTIONS"] != "0"
    log(
        "server mode: connecting to \(addr), transport=\(transport), acceptConnections=\(acceptConnections)"
    )

    let rootMetadata: [MetadataEntry] = [
        MetadataEntry(key: "vox-service", value: .string("Testbed"), flags: 0),
        MetadataEntry(key: "vox-connection-kind", value: .string("root"), flags: 0),
    ]
    let connection: Connection
    let driver: Driver
    if addr.hasPrefix("local://") {
        let path = String(addr.dropFirst("local://".count))
        guard !path.isEmpty else {
            log("invalid PEER_ADDR format")
            throw SubjectError.invalidAddr
        }
        let connector = UnixConnector(path: path, transport: transport)
        let session = try await Session.initiator(
            connector,
            dispatcher: dispatcher,
            onConnection: acceptConnections
                ? DefaultConnectionAcceptor(dispatcher: dispatcher) : nil,
            resumable: false,
            metadata: rootMetadata
        )
        (connection, driver) = (session.rootConnection, session.driver)
    } else {
        let parts = addr.split(separator: ":")
        guard parts.count == 2, let port = Int(parts[1]) else {
            log("invalid PEER_ADDR format")
            throw SubjectError.invalidAddr
        }
        let host = String(parts[0])
        let connector = TcpConnector(host: host, port: port, transport: transport)
        let session = try await Session.initiator(
            connector,
            dispatcher: dispatcher,
            onConnection: acceptConnections
                ? DefaultConnectionAcceptor(dispatcher: dispatcher) : nil,
            resumable: false,
            metadata: rootMetadata
        )
        (connection, driver) = (session.rootConnection, session.driver)
    }

    let rootConnection = connection
    _ = rootConnection
    try await driver.run()
}

// MARK: - Client Mode

func runClientScenario(client: TestbedClient, scenario: String) async throws {
    log("running client scenario: \(scenario)")

    switch scenario {
    case "echo":
        let result = try await client.echo(message: "hello from swift")
        log("echo result: \(result)")
    case "reverse":
        let result = try await client.reverse(message: "hello")
        guard result == "olleh" else {
            log("reverse expected olleh, got \(result)")
            throw SubjectError.invalidResponse
        }
        log("reverse OK")
    case "divide_success":
        let result = try await client.divide(dividend: 10, divisor: 3)
        guard case .success(3) = result else {
            log("divide_success expected success(3), got \(result)")
            throw SubjectError.invalidResponse
        }
        log("divide_success OK")
    case "divide_zero":
        let result = try await client.divide(dividend: 10, divisor: 0)
        guard case .failure(.divisionByZero) = result else {
            log("divide_zero expected divisionByZero, got \(result)")
            throw SubjectError.invalidResponse
        }
        log("divide_zero OK")
    case "divide_overflow":
        let result = try await client.divide(dividend: .min, divisor: -1)
        guard case .failure(.overflow) = result else {
            log("divide_overflow expected overflow, got \(result)")
            throw SubjectError.invalidResponse
        }
        log("divide_overflow OK")
    case "sum":
        let (tx, rx) = channel(
            serialize: { val, buf in encodeI32(val, into: &buf) },
            deserialize: { buf in try decodeI32(from: &buf) }
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
    case "sum_client_to_server":
        let (tx, rx) = channel(
            serialize: { val, buf in encodeI32(val, into: &buf) },
            deserialize: { buf in try decodeI32(from: &buf) }
        )

        let callTask = Task {
            try await client.sum(numbers: rx)
        }
        for n in [1, 2, 3, 4, 5] {
            try await tx.send(Int32(n))
        }
        tx.close()
        let result = try await callTask.value
        guard result == 15 else {
            log("sum_client_to_server expected 15, got \(result)")
            throw SubjectError.invalidResponse
        }
        log("sum_client_to_server OK")
    case "sum_large":
        let (tx, rx) = channel(
            serialize: { val, buf in encodeI32(val, into: &buf) },
            deserialize: { buf in try decodeI32(from: &buf) }
        )

        let n = 100
        let callTask = Task {
            try await client.sumLarge(numbers: rx)
        }
        for i in 0..<n {
            try await tx.send(Int32(i))
        }
        tx.close()
        let result = try await callTask.value
        let expected = Int64(n * (n - 1) / 2)
        guard result == expected else {
            log("sum_large expected \(expected), got \(result)")
            throw SubjectError.invalidResponse
        }
        log("sum_large OK")
    case "generate":
        let (tx, rx) = channel(
            serialize: { val, buf in encodeI32(val, into: &buf) },
            deserialize: { buf in try decodeI32(from: &buf) }
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
    case "generate_large":
        let (tx, rx) = channel(
            serialize: { val, buf in encodeI32(val, into: &buf) },
            deserialize: { buf in try decodeI32(from: &buf) }
        )

        let count: UInt32 = 100
        async let call: Void = client.generateLarge(count: count, output: tx)
        async let received: [Int32] = {
            var values: [Int32] = []
            for try await n in rx {
                values.append(n)
            }
            return values
        }()
        let (_, receivedValues) = try await (call, received)
        let expected = (0..<Int32(count)).map { $0 }
        guard receivedValues == expected else {
            log("generate_large expected \(expected.count) ordered items, got \(receivedValues)")
            throw SubjectError.invalidResponse
        }
        log("generate_large OK")
    case "channel_retry_non_idem":
        let (tx, rx) = channel(
            serialize: { val, buf in encodeI32(val, into: &buf) },
            deserialize: { buf in try decodeI32(from: &buf) }
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
        } catch VoxError.indeterminate {
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
            serialize: { val, buf in encodeI32(val, into: &buf) },
            deserialize: { buf in try decodeI32(from: &buf) }
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
        guard
            let restart = received.enumerated().dropFirst().first(where: { $0.element == 0 })?
                .offset
        else {
            log("channel_retry_idem expected retry restart, got \(received)")
            throw SubjectError.invalidResponse
        }
        let expectedPrefix = (0..<Int32(restart)).map { $0 }
        guard Array(received[..<restart]) == expectedPrefix else {
            log(
                "channel_retry_idem expected prefix \(expectedPrefix), got \(Array(received[..<restart]))"
            )
            throw SubjectError.invalidResponse
        }
        let expectedRerun = (0..<Int32(retryProbeItemCount)).map { $0 }
        guard Array(received[restart...]) == expectedRerun else {
            log(
                "channel_retry_idem expected rerun \(expectedRerun), got \(Array(received[restart...]))"
            )
            throw SubjectError.invalidResponse
        }
    case "divide_error":
        let result = try await client.divide(dividend: 10, divisor: 0)
        guard case .failure(.divisionByZero) = result else {
            log("divide_error expected division_by_zero")
            throw SubjectError.invalidResponse
        }
        log("divide_error result OK")
    case "lookup_found":
        let result = try await client.lookup(id: 1)
        guard case .success(let person) = result, person.name == "Alice" else {
            log("lookup_found expected Alice, got \(result)")
            throw SubjectError.invalidResponse
        }
        log("lookup_found OK")
    case "lookup_found_no_email":
        let result = try await client.lookup(id: 2)
        guard case .success(let person) = result, person.name == "Bob", person.email == nil else {
            log("lookup_found_no_email expected Bob with nil email, got \(result)")
            throw SubjectError.invalidResponse
        }
        log("lookup_found_no_email OK")
    case "lookup_not_found":
        let result = try await client.lookup(id: 999)
        guard case .failure(.notFound) = result else {
            log("lookup_not_found expected notFound, got \(result)")
            throw SubjectError.invalidResponse
        }
        log("lookup_not_found OK")
    case "lookup_access_denied":
        let result = try await client.lookup(id: 100)
        guard case .failure(.accessDenied) = result else {
            log("lookup_access_denied expected accessDenied, got \(result)")
            throw SubjectError.invalidResponse
        }
        log("lookup_access_denied OK")
    case "echo_point":
        let point = Point(x: 42, y: -7)
        let result = try await client.echoPoint(point: point)
        guard result.x == point.x, result.y == point.y else {
            log("echo_point expected \(point), got \(result)")
            throw SubjectError.invalidResponse
        }
        log("echo_point OK")
    case "create_person":
        let dave = try await client.createPerson(name: "Dave", age: 40, email: "dave@example.com")
        guard dave.name == "Dave", dave.age == 40, dave.email == "dave@example.com" else {
            log("create_person expected Dave, got \(dave)")
            throw SubjectError.invalidResponse
        }
        let eve = try await client.createPerson(name: "Eve", age: 25, email: nil)
        guard eve.name == "Eve", eve.age == 25, eve.email == nil else {
            log("create_person expected Eve with nil email, got \(eve)")
            throw SubjectError.invalidResponse
        }
        log("create_person OK")
    case "rectangle_area":
        let rect = Rectangle(
            topLeft: Point(x: 0, y: 10),
            bottomRight: Point(x: 5, y: 0),
            label: nil
        )
        let result = try await client.rectangleArea(rect: rect)
        guard abs(result - 50.0) < 1e-9 else {
            log("rectangle_area expected 50.0, got \(result)")
            throw SubjectError.invalidResponse
        }
        log("rectangle_area OK")
    case "parse_color":
        guard try await client.parseColor(name: "red") == .red else {
            log("parse_color red failed")
            throw SubjectError.invalidResponse
        }
        guard try await client.parseColor(name: "green") == .green else {
            log("parse_color green failed")
            throw SubjectError.invalidResponse
        }
        guard try await client.parseColor(name: "blue") == .blue else {
            log("parse_color blue failed")
            throw SubjectError.invalidResponse
        }
        guard try await client.parseColor(name: "purple") == nil else {
            log("parse_color purple expected nil")
            throw SubjectError.invalidResponse
        }
        log("parse_color OK")
    case "get_points":
        let result = try await client.getPoints(count: 5)
        guard result.count == 5, result.first?.x == 0, result.last?.x == 4 else {
            log("get_points expected 5 points from 0..4, got \(result)")
            throw SubjectError.invalidResponse
        }
        log("get_points OK")
    case "swap_pair":
        let result = try await client.swapPair(pair: (99, "hello"))
        guard result.0 == "hello", result.1 == 99 else {
            log("swap_pair expected (hello, 99), got \(result)")
            throw SubjectError.invalidResponse
        }
        log("swap_pair OK")
    case "echo_bytes":
        let data = Data([1, 2, 3, 255, 0, 128])
        let result = try await client.echoBytes(data: data)
        guard result == data else {
            log("echo_bytes mismatch")
            throw SubjectError.invalidResponse
        }
        log("echo_bytes OK")
    case "echo_bool":
        guard try await client.echoBool(b: true) == true else {
            log("echo_bool true failed")
            throw SubjectError.invalidResponse
        }
        guard try await client.echoBool(b: false) == false else {
            log("echo_bool false failed")
            throw SubjectError.invalidResponse
        }
        log("echo_bool OK")
    case "echo_u64":
        for n: UInt64 in [0, 1, 1_000_000_000_000, .max] {
            let result = try await client.echoU64(n: n)
            guard result == n else {
                log("echo_u64 expected \(n), got \(result)")
                throw SubjectError.invalidResponse
            }
        }
        log("echo_u64 OK")
    case "echo_option_string":
        let stringResult = try await client.echoOptionString(s: "hello")
        guard stringResult == "hello" else {
            log("echo_option_string Some failed: \(String(describing: stringResult))")
            throw SubjectError.invalidResponse
        }
        let nilResult = try await client.echoOptionString(s: nil)
        guard nilResult == nil else {
            log("echo_option_string None failed: \(String(describing: nilResult))")
            throw SubjectError.invalidResponse
        }
        log("echo_option_string OK")
    case "describe_point":
        let first = try await client.describePoint(label: "origin", x: 0, y: 0, active: true)
        guard first.label == "origin", first.x == 0, first.y == 0, first.active else {
            log("describe_point origin failed: \(first)")
            throw SubjectError.invalidResponse
        }
        let second = try await client.describePoint(label: "far", x: -100, y: 200, active: false)
        guard second.label == "far", second.x == -100, second.y == 200, second.active == false
        else {
            log("describe_point far failed: \(second)")
            throw SubjectError.invalidResponse
        }
        log("describe_point OK")
    case "all_colors":
        let result = try await client.allColors()
        guard result == [.red, .green, .blue] else {
            log("all_colors expected [.red, .green, .blue], got \(result)")
            throw SubjectError.invalidResponse
        }
        log("all_colors OK")
    case "shape_area":
        let result = try await client.shapeArea(shape: .rectangle(width: 3.0, height: 4.0))
        guard result == 12.0 else {
            log("shape_area expected 12.0, got \(result)")
            throw SubjectError.invalidResponse
        }
        log("shape_area result: \(result)")
    case "echo_shape":
        let shapes: [Shape] = [
            .point,
            .circle(radius: 3.14),
            .rectangle(width: 2.0, height: 5.0),
        ]
        for shape in shapes {
            let result = try await client.echoShape(shape: shape)
            guard sameShape(result, shape) else {
                log("echo_shape expected \(shape), got \(result)")
                throw SubjectError.invalidResponse
            }
        }
        log("echo_shape OK")
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
    case "pipelining":
        try await withThrowingTaskGroup(of: Void.self) { group in
            for i in 0..<10 {
                group.addTask {
                    let expected = "msg\(i)"
                    let result = try await client.echo(message: expected)
                    guard result == expected else {
                        throw SubjectError.invalidResponse
                    }
                }
            }
            try await group.waitForAll()
        }
        log("pipelining OK")
    case "process_message":
        let result = try await client.processMessage(msg: .data(Data([1, 2, 3, 4])))
        guard case .data(let payload) = result, payload == Data([4, 3, 2, 1]) else {
            log("process_message returned unexpected payload")
            throw SubjectError.invalidResponse
        }
        log("process_message result OK")
    case "transform_bidi":
        let (inputTx, inputRx) = channel(
            serialize: { val, buf in encodeString(val, into: &buf) },
            deserialize: { buf in try decodeString(from: &buf) }
        )
        let (outputTx, outputRx) = channel(
            serialize: { val, buf in encodeString(val, into: &buf) },
            deserialize: { buf in try decodeString(from: &buf) }
        )
        let messages = ["alpha", "beta", "gamma"]
        async let call: Void = client.transform(input: inputRx, output: outputTx)
        try await Task.sleep(nanoseconds: 50_000_000)
        async let received: [String] = {
            var values: [String] = []
            for try await s in outputRx {
                values.append(s)
            }
            return values
        }()
        for message in messages {
            try await inputTx.send(message)
        }
        inputTx.close()
        let (_, receivedValues) = try await (call, received)
        guard receivedValues == messages else {
            log("transform_bidi expected \(messages), got \(receivedValues)")
            throw SubjectError.invalidResponse
        }
        log("transform_bidi OK")

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
    let dispatcher = TestbedDispatcher(handler: handler)

    let session = try await Session.initiator(
        connector,
        dispatcher: dispatcher,
        resumable: true
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

func runServerListen() async throws {
    let listenPort = ProcessInfo.processInfo.environment["LISTEN_PORT"].flatMap(Int.init) ?? 0
    let acceptor = TcpAcceptor(host: "127.0.0.1", port: listenPort, transport: subjectConduit())
    let acceptConnections = ProcessInfo.processInfo.environment["ACCEPT_CONNECTIONS"] == "1"
    let handler = TestbedService()
    let dispatcher = TestbedDispatcher(handler: handler)
    let session = try await Session.acceptor(
        acceptor,
        dispatcher: dispatcher,
        onConnection: acceptConnections
            ? DefaultConnectionAcceptor(dispatcher: dispatcher) : nil,
        resumable: true
    )
    try await session.run()
}

// MARK: - Errors

enum SubjectError: Error {
    case missingEnv
    case invalidAddr
    case invalidResponse
    case unknownScenario
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
            case "server-listen":
                try await runServerListen()
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
