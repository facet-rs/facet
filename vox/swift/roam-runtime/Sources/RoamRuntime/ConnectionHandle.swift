import Foundation

/// Handle for making outgoing RPC calls.
final class ConnectionHandle: @unchecked Sendable {
    private let commandTx: @Sendable (HandleCommand) -> Bool
    private let taskTx: @Sendable (TaskMessage) -> Bool
    private let requestIdAllocator = RequestIdAllocator()
    private let requestSemaphore: AsyncSemaphore?

    let channelAllocator: ChannelIdAllocator
    let channelRegistry: ChannelRegistry

    init(
        commandTx: @escaping @Sendable (HandleCommand) -> Bool,
        taskTx: @escaping @Sendable (TaskMessage) -> Bool,
        role: Role,
        maxConcurrentRequests: UInt32 = UInt32.max
    ) {
        self.commandTx = commandTx
        self.taskTx = taskTx
        self.channelAllocator = ChannelIdAllocator(role: role)
        self.channelRegistry = ChannelRegistry()
        if maxConcurrentRequests < UInt32.max {
            self.requestSemaphore = AsyncSemaphore(permits: Int(maxConcurrentRequests))
        } else {
            self.requestSemaphore = nil
        }
    }

    /// Make a raw RPC call.
    ///
    /// r[impl rpc.flow-control.max-concurrent-requests] - Blocks if maxConcurrentRequests are in-flight.
    func callRaw(
        methodId: UInt64,
        metadata: [MetadataEntryV7] = [],
        payload: [UInt8],
        channels: [UInt64] = [],
        timeout: TimeInterval? = nil
    ) async throws -> [UInt8] {
        if let semaphore = requestSemaphore {
            try await semaphore.acquire()
        }

        let requestId = await requestIdAllocator.allocate()

        return try await withCheckedThrowingContinuation { continuation in
            let semaphore = requestSemaphore
            let responseTx = SingleResume<[UInt8]> { result in
                if let semaphore {
                    Task { await semaphore.release() }
                }
                continuation.resume(with: result)
            }
            let accepted = commandTx(
                .call(
                    requestId: requestId,
                    methodId: methodId,
                    metadata: metadata,
                    payload: payload,
                    channels: channels,
                    timeout: timeout,
                    responseTx: { result in responseTx(result) }
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

    func sendTaskMessage(_ msg: TaskMessage) {
        _ = taskTx(msg)
    }
}
