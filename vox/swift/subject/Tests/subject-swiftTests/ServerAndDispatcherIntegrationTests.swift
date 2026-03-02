import Darwin
import Foundation
import Testing

@testable import RoamRuntime
@testable import subject_swift

private actor SubjectEnvGate {
    static let shared = SubjectEnvGate()

    func withEnvironment<T>(
        _ pairs: [(String, String?)],
        body: () async throws -> T
    ) async rethrows -> T {
        var previous: [(String, String?)] = []
        previous.reserveCapacity(pairs.count)
        for (key, value) in pairs {
            previous.append((key, currentValue(for: key)))
            set(value: value, for: key)
        }
        defer {
            for (key, value) in previous {
                set(value: value, for: key)
            }
        }
        return try await body()
    }

    private func currentValue(for key: String) -> String? {
        guard let raw = getenv(key) else {
            return nil
        }
        return String(cString: raw)
    }

    private func set(value: String?, for key: String) {
        if let value {
            _ = setenv(key, value, 1)
        } else {
            _ = unsetenv(key)
        }
    }
}

private actor TaskMessageRecorder {
    private var messages: [TaskMessage] = []

    func append(_ message: TaskMessage) {
        messages.append(message)
    }

    func firstResponse() -> (UInt64, [UInt8])? {
        for message in messages {
            if case .response(let requestId, let payload) = message {
                return (requestId, payload)
            }
        }
        return nil
    }

    func all() -> [TaskMessage] {
        messages
    }
}

private enum IntegrationTestError: Error {
    case timeout(String)
    case shortRead(expected: Int, actual: Int)
}

private struct HelloHarness {
    let port: Int
    private let listenerFd: Int32
    private let task: Task<MessageV7?, Error>

    static func start() throws -> HelloHarness {
        let listenerFd = socket(AF_INET, SOCK_STREAM, 0)
        guard listenerFd >= 0 else {
            throw POSIXError(.EIO)
        }

        var reuse: Int32 = 1
        guard setsockopt(
            listenerFd,
            SOL_SOCKET,
            SO_REUSEADDR,
            &reuse,
            socklen_t(MemoryLayout<Int32>.size)
        ) == 0
        else {
            close(listenerFd)
            throw POSIXError(.EIO)
        }

        var addr = sockaddr_in()
        addr.sin_len = UInt8(MemoryLayout<sockaddr_in>.size)
        addr.sin_family = sa_family_t(AF_INET)
        addr.sin_port = 0
        addr.sin_addr = in_addr(s_addr: inet_addr("127.0.0.1"))

        var bindAddr = addr
        let bindResult = withUnsafePointer(to: &bindAddr) { ptr in
            ptr.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                Darwin.bind(listenerFd, $0, socklen_t(MemoryLayout<sockaddr_in>.size))
            }
        }
        guard bindResult == 0 else {
            close(listenerFd)
            throw POSIXError(.EIO)
        }

        guard listen(listenerFd, 1) == 0 else {
            close(listenerFd)
            throw POSIXError(.EIO)
        }

        var localAddr = sockaddr_in()
        var localLen = socklen_t(MemoryLayout<sockaddr_in>.size)
        let nameResult = withUnsafeMutablePointer(to: &localAddr) { ptr in
            ptr.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                getsockname(listenerFd, $0, &localLen)
            }
        }
        guard nameResult == 0 else {
            close(listenerFd)
            throw POSIXError(.EIO)
        }
        let port = Int(UInt16(bigEndian: localAddr.sin_port))

        let task = Task.detached(priority: .userInitiated) { () throws -> MessageV7? in
            var peerStorage = sockaddr()
            var peerLen = socklen_t(MemoryLayout<sockaddr>.size)
            let connFd = withUnsafeMutablePointer(to: &peerStorage) { ptr in
                Darwin.accept(listenerFd, ptr, &peerLen)
            }
            guard connFd >= 0 else {
                throw POSIXError(.EIO)
            }
            defer { close(connFd) }

            let hello = MessageV7.hello(
                HelloV7(
                    version: 7,
                    connectionSettings: ConnectionSettingsV7(parity: .odd, maxConcurrentRequests: 64),
                    metadata: []
                )
            )
            try writeFrame(connFd, message: hello)
            let response = try readFrame(connFd)
            return response
        }

        return HelloHarness(port: port, listenerFd: listenerFd, task: task)
    }

    func waitForPeerMessage(timeoutMs: UInt64 = 2_000) async throws -> MessageV7? {
        try await withTimeout(milliseconds: timeoutMs) {
            try await task.value
        }
    }

    func shutdown() {
        close(listenerFd)
        task.cancel()
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
            throw IntegrationTestError.timeout("operation timed out after \(milliseconds)ms")
        }
        let result = try await group.next()!
        group.cancelAll()
        return result
    }
}

private func writeFrame(_ fd: Int32, message: MessageV7) throws {
    let payload = message.encode()
    var frame: [UInt8] = []
    frame.reserveCapacity(4 + payload.count)
    var len = UInt32(payload.count).littleEndian
    withUnsafeBytes(of: &len) { raw in
        frame.append(contentsOf: raw)
    }
    frame.append(contentsOf: payload)
    try writeAll(fd, bytes: frame)
}

private func readFrame(_ fd: Int32) throws -> MessageV7? {
    let header = try readExactly(fd, count: 4)
    if header.isEmpty {
        return nil
    }
    let frameLen = header.withUnsafeBytes { raw in
        UInt32(littleEndian: raw.load(as: UInt32.self))
    }
    let payload = try readExactly(fd, count: Int(frameLen))
    return try MessageV7.decode(from: Data(payload))
}

private func writeAll(_ fd: Int32, bytes: [UInt8]) throws {
    var sent = 0
    while sent < bytes.count {
        let n = bytes.withUnsafeBytes { raw -> Int in
            guard let base = raw.baseAddress else { return -1 }
            return Darwin.send(fd, base.advanced(by: sent), bytes.count - sent, 0)
        }
        if n > 0 {
            sent += n
            continue
        }
        throw POSIXError(.EIO)
    }
}

private func readExactly(_ fd: Int32, count: Int) throws -> [UInt8] {
    if count == 0 {
        return []
    }
    var out = [UInt8](repeating: 0, count: count)
    var offset = 0
    while offset < count {
        let n = out.withUnsafeMutableBytes { raw -> Int in
            guard let base = raw.baseAddress else { return -1 }
            return Darwin.recv(fd, base.advanced(by: offset), count - offset, 0)
        }
        if n == 0 {
            if offset == 0 {
                return []
            }
            throw IntegrationTestError.shortRead(expected: count, actual: offset)
        }
        if n < 0 {
            throw POSIXError(.EIO)
        }
        offset += n
    }
    return out
}

private func isConnectionClosed(_ error: Error) -> Bool {
    if let connError = error as? ConnectionError, case .connectionClosed = connError {
        return true
    }
    if let transportError = error as? TransportError, case .connectionClosed = transportError {
        return true
    }
    return false
}

struct ServerAndDispatcherIntegrationTests {
    // r[verify core.conn.accept-required]
    @Test func serverRunSubjectRequiresPeerAddr() async {
        let server = Server()
        await SubjectEnvGate.shared.withEnvironment([("PEER_ADDR", nil)]) {
            do {
                try await server.runSubject(dispatcher: TestbedDispatcherAdapter(handler: TestbedService()))
                Issue.record("expected missingPeerAddr")
            } catch let error as ServerError {
                guard case .missingPeerAddr = error else {
                    Issue.record("expected missingPeerAddr, got \(error)")
                    return
                }
            } catch {
                Issue.record("expected ServerError.missingPeerAddr, got \(error)")
            }
        }
    }

    // r[verify core.conn.accept-required]
    @Test func serverRunSubjectRejectsInvalidPeerAddr() async {
        let server = Server()
        await SubjectEnvGate.shared.withEnvironment([("PEER_ADDR", "127.0.0.1")]) {
            do {
                try await server.runSubject(dispatcher: TestbedDispatcherAdapter(handler: TestbedService()))
                Issue.record("expected invalidPeerAddr")
            } catch let error as ServerError {
                guard case .invalidPeerAddr(let value) = error else {
                    Issue.record("expected invalidPeerAddr, got \(error)")
                    return
                }
                #expect(value == "127.0.0.1")
            } catch {
                Issue.record("expected ServerError.invalidPeerAddr, got \(error)")
            }
        }
    }

    // r[verify core.conn.accept-required]
    @Test func serverRunSubjectHandshakePathExchangesHello() async throws {
        let harness = try HelloHarness.start()
        defer { harness.shutdown() }

        let runResult: Result<Void, Error> = await SubjectEnvGate.shared.withEnvironment([
            ("PEER_ADDR", "127.0.0.1:\(harness.port)"),
            ("ACCEPT_CONNECTIONS", "1"),
        ]) {
            do {
                try await withTimeout(milliseconds: 2_000) {
                    try await Server().runSubject(
                        dispatcher: TestbedDispatcherAdapter(handler: TestbedService())
                    )
                }
                return .success(())
            } catch {
                return .failure(error)
            }
        }

        let peerMsg = try await harness.waitForPeerMessage()
        guard let peerMsg else {
            Issue.record("expected HelloYourself from subject")
            return
        }
        guard case .helloYourself = peerMsg.payload else {
            Issue.record("expected helloYourself payload, got \(peerMsg.payload)")
            return
        }

        if case .failure(let error) = runResult {
            #expect(isConnectionClosed(error))
        }
    }

    // r[verify transport.message.binary]
    @Test func dispatcherAdapterEchoRoundTripProducesResponse() async throws {
        let recorder = TaskMessageRecorder()
        let registry = ChannelRegistry()
        let adapter = TestbedDispatcherAdapter(handler: TestbedService())
        let requestId: UInt64 = 42

        await adapter.dispatch(
            methodId: TestbedMethodId.echo,
            payload: encodeString("swift-subject"),
            channels: [],
            requestId: requestId,
            registry: registry,
            taskTx: { msg in
                Task { await recorder.append(msg) }
            }
        )

        let response = try await withTimeout(milliseconds: 500) {
            while true {
                if let response = await recorder.firstResponse() {
                    return response
                }
                try await Task.sleep(nanoseconds: 1_000_000)
            }
        }
        #expect(response.0 == requestId)
        var offset = 0
        let payload = Data(response.1)
        let resultDiscriminant = try decodeVarint(from: payload, offset: &offset)
        #expect(resultDiscriminant == 0)
        let echoed = try decodeString(from: payload, offset: &offset)
        #expect(echoed == "swift-subject")
    }

    // r[verify rpc.channel.allocation]
    @Test func dispatcherAdapterPreregisterMarksIncomingChannelsKnown() async {
        let adapter = TestbedDispatcherAdapter(handler: TestbedService())
        let registry = ChannelRegistry()

        await adapter.preregister(
            methodId: TestbedMethodId.sum,
            payload: [],
            channels: [1001],
            registry: registry
        )
        #expect(await registry.isKnown(1001))

        await adapter.preregister(
            methodId: TestbedMethodId.transform,
            payload: [],
            channels: [2001, 2002],
            registry: registry
        )
        #expect(await registry.isKnown(2001))
        #expect(!(await registry.isKnown(2002)))
    }
}
