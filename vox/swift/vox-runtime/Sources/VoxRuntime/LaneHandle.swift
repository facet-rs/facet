import Foundation

/// Handle for making outgoing RPC calls on a lane.
/// r[impl rpc.caller]
final class LaneHandle: @unchecked Sendable {
    let laneId: UInt64
    private let commandTx: @Sendable (HandleCommand) -> Bool
    private let taskTx: @Sendable (TaskMessage) -> Bool
    private let requestSemaphore: AsyncSemaphore?
    private let role: Role

    private var requestIdAllocator: RequestIdAllocator

    let channelAllocator: ChannelIdAllocator
    let channelRegistry: ChannelRegistry

    init(
        laneId: UInt64 = 0,
        commandTx: @escaping @Sendable (HandleCommand) -> Bool,
        taskTx: @escaping @Sendable (TaskMessage) -> Bool,
        role: Role,
        maxConcurrentRequests: UInt32 = UInt32.max
    ) {
        self.laneId = laneId
        self.commandTx = commandTx
        self.taskTx = taskTx
        self.role = role
        self.requestIdAllocator = RequestIdAllocator(role: role)
        self.channelAllocator = ChannelIdAllocator(role: role)
        self.channelRegistry = ChannelRegistry()
        if maxConcurrentRequests < UInt32.max {
            // r[impl rpc.flow-control.max-concurrent-requests.outbound]
            self.requestSemaphore = AsyncSemaphore(permits: Int(maxConcurrentRequests))
        } else {
            self.requestSemaphore = nil
        }
    }

    /// Make a raw RPC call.
    ///
    /// r[impl rpc.flow-control.max-concurrent-requests] - Blocks if maxConcurrentRequests are in-flight.
    /// r[impl rpc.flow-control.max-concurrent-requests.counting]
    /// r[impl rpc.flow-control.max-concurrent-requests.outbound]
    /// r[impl rpc.caller]
    func callRaw(
        methodId: UInt64,
        metadata: Metadata = .null,
        payload: [UInt8],
        channels: [UInt64] = [],
        timeout: TimeInterval? = nil,
        finalizeChannels: (@Sendable () -> Void)? = nil,
        schemaInfo: ClientSchemaInfo? = nil
    ) async throws -> [UInt8] {
        if let semaphore = requestSemaphore {
            try await semaphore.acquire()
        }

        let requestId = await requestIdAllocator.allocate()

        return try await withCheckedThrowingContinuation { continuation in
            let semaphore = requestSemaphore
            let responseTx = SingleResume<[UInt8]> { result in
                finalizeChannels?()
                if let semaphore {
                    Task { await semaphore.release() }
                }
                continuation.resume(with: result)
            }
            let accepted = commandTx(
                .call(
                    connectionId: laneId,
                    requestId: requestId,
                    methodId: methodId,
                    metadata: metadata,
                    payload: payload,
                    channels: channels,
                    timeout: timeout,
                    responseTx: { result in responseTx(result) },
                    schemaInfo: schemaInfo
                ))
            guard accepted else {
                responseTx(.failure(.connectionClosed))
                return
            }
        }
    }

    func closeRequestSemaphore() async {
        await requestSemaphore?.close()
    }

    // The session has been started fresh on a new conduit. Reset request IDs so
    // future calls use the new connection's identifier space.
    func onConduitReset() {
        self.requestIdAllocator = RequestIdAllocator(role: role)
    }

    func sendTaskMessage(_ msg: TaskMessage) {
        _ = taskTx(msg)
    }
}
