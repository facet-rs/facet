/// Swift subject binary for the vox compliance suite.
///
/// This uses the vox-runtime library to validate that the Swift implementation
/// is compliant with the vox protocol spec.

import Foundation
import VoxRuntime

#if canImport(Darwin)
    import Darwin
#elseif canImport(Glibc)
    import Glibc
#endif

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
    let dispatcher = TestbedChannelingDispatcher(handler: handler)
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
        let attachment = try await connector.openAttachment()
        (connection, driver, _, _, _) = try await establishInitiator(
            attachment: attachment,
            transport: transport,
            dispatcher: dispatcher,
            acceptConnections: acceptConnections,
            resumable: false,
            metadata: rootMetadata
        )
    } else {
        let parts = addr.split(separator: ":")
        guard parts.count == 2, let port = Int(parts[1]) else {
            log("invalid PEER_ADDR format")
            throw SubjectError.invalidAddr
        }
        let host = String(parts[0])
        let connector = TcpConnector(host: host, port: port, transport: transport)
        let attachment = try await connector.openAttachment()
        (connection, driver, _, _, _) = try await establishInitiator(
            attachment: attachment,
            transport: transport,
            dispatcher: dispatcher,
            acceptConnections: acceptConnections,
            resumable: false,
            metadata: rootMetadata
        )
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
    case "sum_client_to_server":
        let (tx, rx) = channel(
            serialize: { encodeI32($0) },
            deserialize: { bytes in
                var offset = 0
                return try decodeI32(from: Data(bytes), offset: &offset)
            }
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
            serialize: { encodeI32($0) },
            deserialize: { bytes in
                var offset = 0
                return try decodeI32(from: Data(bytes), offset: &offset)
            }
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
    case "generate_large":
        let (tx, rx) = channel(
            serialize: { encodeI32($0) },
            deserialize: { bytes in
                var offset = 0
                return try decodeI32(from: Data(bytes), offset: &offset)
            }
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
    let dispatcher = TestbedChannelingDispatcher(handler: handler)

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

final class SocketLink: Link, @unchecked Sendable {
    private let fd: Int32
    private let readQueue = DispatchQueue(label: "bearcove.vox.subject-swift.socket-link.read")
    private let writeQueue = DispatchQueue(label: "bearcove.vox.subject-swift.socket-link.write")
    private let stateQueue = DispatchQueue(label: "bearcove.vox.subject-swift.socket-link.state")
    private var maxFrameSize = 1024 * 1024
    private var closed = false

    init(fd: Int32) {
        self.fd = fd
    }

    func sendFrame(_ bytes: [UInt8]) async throws {
        try await withCheckedThrowingContinuation {
            (continuation: CheckedContinuation<Void, Error>) in
            self.writeQueue.async {
                do {
                    try self.ensureOpen()
                    guard let len = UInt32(exactly: bytes.count) else {
                        throw SubjectError.socketSetupFailed
                    }
                    var header = withUnsafeBytes(of: len.littleEndian) { Array($0) }
                    try writeAll(self.fd, bytes: header)
                    header.removeAll(keepingCapacity: false)
                    try writeAll(self.fd, bytes: bytes)
                    continuation.resume()
                } catch {
                    continuation.resume(throwing: error)
                }
            }
        }
    }

    func recvFrame() async throws -> [UInt8]? {
        try await withCheckedThrowingContinuation { continuation in
            self.readQueue.async {
                do {
                    try self.ensureOpen()
                    let header = try readFrameHeader(self.fd)
                    guard let header else {
                        continuation.resume(returning: nil)
                        return
                    }
                    let frameLength = Int(UInt32(littleEndian: header))
                    let maxFrameSize = self.currentMaxFrameSize()
                    guard frameLength <= maxFrameSize else {
                        throw TransportError.frameDecoding("Frame exceeds \(maxFrameSize) bytes")
                    }
                    let bytes = try readExactly(fd: self.fd, count: frameLength)
                    continuation.resume(returning: bytes)
                } catch {
                    continuation.resume(throwing: error)
                }
            }
        }
    }

    func setMaxFrameSize(_ size: Int) async throws {
        stateQueue.sync {
            maxFrameSize = size
        }
    }

    func close() async throws {
        try await withCheckedThrowingContinuation { continuation in
            self.writeQueue.async {
                let wasClosed = self.stateQueue.sync { () -> Bool in
                    let wasClosed = self.closed
                    if !wasClosed {
                        self.closed = true
                    }
                    return wasClosed
                }
                if !wasClosed {
                    #if canImport(Darwin)
                        _ = Darwin.shutdown(self.fd, SHUT_RDWR)
                        _ = Darwin.close(self.fd)
                    #else
                        _ = Glibc.shutdown(self.fd, Int32(SHUT_RDWR))
                        _ = Glibc.close(self.fd)
                    #endif
                }
                continuation.resume()
            }
        }
    }

    private func ensureOpen() throws {
        let isClosed = stateQueue.sync { closed }
        if isClosed {
            throw ConnectionError.connectionClosed
        }
    }

    private func currentMaxFrameSize() -> Int {
        stateQueue.sync { maxFrameSize }
    }
}

private func readFrameHeader(_ fd: Int32) throws -> UInt32? {
    let headerBytes = try readExactlyAllowingEof(fd: fd, count: 4)
    guard let headerBytes else {
        return nil
    }
    return headerBytes.withUnsafeBytes { raw in
        raw.load(as: UInt32.self)
    }
}

private func readExactlyAllowingEof(fd: Int32, count: Int) throws -> [UInt8]? {
    if count == 0 {
        return []
    }

    var out = [UInt8](repeating: 0, count: count)
    var offset = 0
    while offset < count {
        let n = out.withUnsafeMutableBytes { raw -> Int in
            guard let base = raw.baseAddress else { return -1 }
            #if canImport(Darwin)
                return Darwin.recv(fd, base.advanced(by: offset), count - offset, 0)
            #else
                return Glibc.recv(fd, base.advanced(by: offset), count - offset, 0)
            #endif
        }
        if n == 0 {
            if offset == 0 {
                return nil
            }
            throw SubjectError.socketSetupFailed
        }
        if n < 0 {
            if errno == EINTR {
                continue
            }
            throw SubjectError.socketSetupFailed
        }
        offset += n
    }
    return out
}

private func writeAll(_ fd: Int32, bytes: [UInt8]) throws {
    var sent = 0
    while sent < bytes.count {
        let n = bytes.withUnsafeBytes { raw -> Int in
            guard let base = raw.baseAddress else { return -1 }
            #if canImport(Darwin)
                return Darwin.send(fd, base.advanced(by: sent), bytes.count - sent, 0)
            #else
                return Glibc.send(fd, base.advanced(by: sent), bytes.count - sent, 0)
            #endif
        }
        if n > 0 {
            sent += n
            continue
        }
        if n < 0, errno == EINTR {
            continue
        }
        throw SubjectError.socketSetupFailed
    }
}

private func readExactly(fd: Int32, count: Int) throws -> [UInt8] {
    if count == 0 { return [] }
    guard let bytes = try readExactlyAllowingEof(fd: fd, count: count) else {
        throw SubjectError.socketSetupFailed
    }
    return bytes
}

private func makeTcpListener(port: Int) throws -> (fd: Int32, boundPort: Int) {
    #if canImport(Glibc)
        let fd = socket(AF_INET, Int32(SOCK_STREAM.rawValue), 0)
    #else
        let fd = socket(AF_INET, SOCK_STREAM, 0)
    #endif
    guard fd >= 0 else {
        throw SubjectError.socketSetupFailed
    }

    var yes: Int32 = 1
    guard setsockopt(fd, SOL_SOCKET, SO_REUSEADDR, &yes, socklen_t(MemoryLayout<Int32>.size)) == 0
    else {
        #if canImport(Darwin)
            _ = Darwin.close(fd)
        #else
            _ = Glibc.close(fd)
        #endif
        throw SubjectError.socketSetupFailed
    }

    var addr = sockaddr_in()
    #if canImport(Darwin)
        addr.sin_len = UInt8(MemoryLayout<sockaddr_in>.size)
    #endif
    addr.sin_family = sa_family_t(AF_INET)
    addr.sin_port = in_port_t(UInt16(port).bigEndian)
    addr.sin_addr = in_addr(s_addr: inet_addr("127.0.0.1"))

    let bindResult = withUnsafePointer(to: &addr) { ptr in
        ptr.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockPtr in
            #if canImport(Darwin)
                Darwin.bind(fd, sockPtr, socklen_t(MemoryLayout<sockaddr_in>.size))
            #else
                Glibc.bind(fd, sockPtr, socklen_t(MemoryLayout<sockaddr_in>.size))
            #endif
        }
    }
    guard bindResult == 0, listen(fd, 1) == 0 else {
        #if canImport(Darwin)
            _ = Darwin.close(fd)
        #else
            _ = Glibc.close(fd)
        #endif
        throw SubjectError.socketSetupFailed
    }

    var localAddr = sockaddr_in()
    var localLen = socklen_t(MemoryLayout<sockaddr_in>.size)
    let nameResult = withUnsafeMutablePointer(to: &localAddr) { ptr in
        ptr.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockPtr in
            getsockname(fd, sockPtr, &localLen)
        }
    }
    guard nameResult == 0 else {
        #if canImport(Darwin)
            _ = Darwin.close(fd)
        #else
            _ = Glibc.close(fd)
        #endif
        throw SubjectError.socketSetupFailed
    }

    let boundPort = Int(UInt16(bigEndian: localAddr.sin_port))
    return (fd, boundPort)
}

private func acceptTcpConnection(listenerFd: Int32) async throws -> Int32 {
    try await withCheckedThrowingContinuation { continuation in
        DispatchQueue.global().async(
            execute: DispatchWorkItem {
                #if canImport(Darwin)
                    let clientFd = Darwin.accept(listenerFd, nil, nil)
                #else
                    let clientFd = Glibc.accept(listenerFd, nil, nil)
                #endif
                if clientFd >= 0 {
                    continuation.resume(returning: clientFd)
                } else {
                    continuation.resume(throwing: SubjectError.socketSetupFailed)
                }
            })
    }
}

func runServerListen() async throws {
    let listenPort = ProcessInfo.processInfo.environment["LISTEN_PORT"].flatMap(Int.init) ?? 0
    let (listenerFd, boundPort) = try makeTcpListener(port: listenPort)
    defer {
        #if canImport(Darwin)
            _ = Darwin.close(listenerFd)
        #else
            _ = Glibc.close(listenerFd)
        #endif
    }

    FileHandle.standardOutput.write(Data("LISTEN_ADDR=127.0.0.1:\(boundPort)\n".utf8))
    log("server-listen mode: bound to 127.0.0.1:\(boundPort)")

    let clientFd = try await acceptTcpConnection(listenerFd: listenerFd)
    let link = SocketLink(fd: clientFd)
    _ = try await performAcceptorTransportPrologue(
        transport: link,
        supportedConduit: subjectConduit()
    )

    let acceptConnections = ProcessInfo.processInfo.environment["ACCEPT_CONNECTIONS"] == "1"
    let handler = TestbedService()
    let dispatcher = TestbedChannelingDispatcher(handler: handler)
    let session = try await Session.acceptFreshLink(
        link,
        conduit: subjectConduit(),
        dispatcher: dispatcher,
        acceptConnections: acceptConnections,
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
    case socketSetupFailed
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
