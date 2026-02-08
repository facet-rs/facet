import Foundation
import Testing

@testable import RoamRuntime
@testable import subject_swift

private actor CallRecorder {
    var observedTimeouts: [TimeInterval?] = []
    var observedMethodIds: [UInt64] = []

    func append(methodId: UInt64, timeout: TimeInterval?) {
        observedMethodIds.append(methodId)
        observedTimeouts.append(timeout)
    }

    func timeouts() -> [TimeInterval?] { observedTimeouts }
    func methodIds() -> [UInt64] { observedMethodIds }
}

private final class CapturingConnection: RoamConnection, @unchecked Sendable {
    let channelAllocator = ChannelIdAllocator(role: .initiator)
    let incomingChannelRegistry = ChannelRegistry()
    let taskSender: TaskSender = { _ in }

    private let recorder = CallRecorder()

    func call(methodId: UInt64, payload _: Data, timeout: TimeInterval?) async throws -> Data {
        await recorder.append(methodId: methodId, timeout: timeout)
        return Data([0] + encodeString("ok"))
    }

    func timeouts() async -> [TimeInterval?] { await recorder.timeouts() }
    func methodIds() async -> [UInt64] { await recorder.methodIds() }
}

struct GeneratedClientTimeoutTests {
    @Test func generatedClientUsesDefaultTimeout() async throws {
        let connection = CapturingConnection()
        let client = TestbedClient(connection: connection)

        let result = try await client.echo(message: "hello")

        #expect(result == "ok")
        let timeouts = await connection.timeouts()
        let methodIds = await connection.methodIds()
        #expect(timeouts.count == 1)
        #expect(timeouts[0] == 30.0)
        #expect(methodIds == [TestbedMethodId.echo])
    }

    @Test func generatedClientUsesConfiguredTimeout() async throws {
        let connection = CapturingConnection()
        let client = TestbedClient(connection: connection, timeout: 1.25)

        let result = try await client.echo(message: "hello")

        #expect(result == "ok")
        let timeouts = await connection.timeouts()
        #expect(timeouts.count == 1)
        #expect(timeouts[0] == 1.25)
    }

    @Test func generatedClientCanDisableTimeout() async throws {
        let connection = CapturingConnection()
        let client = TestbedClient(connection: connection, timeout: nil)

        let result = try await client.echo(message: "hello")

        #expect(result == "ok")
        let timeouts = await connection.timeouts()
        #expect(timeouts.count == 1)
        #expect(timeouts[0] == nil)
    }
}
