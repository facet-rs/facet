import Foundation

/// Handle for making outgoing RPC calls.
final class ConnectionHandle: @unchecked Sendable {
    private let commandTx: @Sendable (HandleCommand) -> Bool
    private let taskTx: @Sendable (TaskMessage) -> Bool
    private let requestIdAllocator = RequestIdAllocator()
    private let operationIdAllocator = RequestIdAllocator()
    private let requestSemaphore: AsyncSemaphore?
    private let peerSupportsRetry: Bool

    let channelAllocator: ChannelIdAllocator
    let channelRegistry: ChannelRegistry

    init(
        commandTx: @escaping @Sendable (HandleCommand) -> Bool,
        taskTx: @escaping @Sendable (TaskMessage) -> Bool,
        role: Role,
        peerSupportsRetry: Bool,
        maxConcurrentRequests: UInt32 = UInt32.max
    ) {
        self.commandTx = commandTx
        self.taskTx = taskTx
        self.peerSupportsRetry = peerSupportsRetry
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
        retry: RetryPolicy = .volatile,
        timeout: TimeInterval? = nil,
        prepareRetry: (@Sendable () async -> PreparedRetryRequest)? = nil,
        finalizeChannels: (@Sendable () -> Void)? = nil
    ) async throws -> [UInt8] {
        if let semaphore = requestSemaphore {
            try await semaphore.acquire()
        }

        let requestId = await requestIdAllocator.allocate()
        let outboundMetadata: [MetadataEntryV7]
        if peerSupportsRetry {
            let operationId = await operationIdAllocator.allocate()
            outboundMetadata = ensureOperationId(metadata, operationId: operationId)
        } else {
            outboundMetadata = metadata
        }

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
                    requestId: requestId,
                    methodId: methodId,
                    metadata: outboundMetadata,
                    payload: payload,
                    retry: retry,
                    timeout: timeout,
                    prepareRetry: prepareRetry,
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

    func freshOperationMetadata(from metadata: [MetadataEntryV7]) async -> [MetadataEntryV7] {
        guard peerSupportsRetry else {
            return metadata
        }
        let operationId = await operationIdAllocator.allocate()
        return replacingOperationId(metadata, operationId: operationId)
    }

    func sendTaskMessage(_ msg: TaskMessage) {
        _ = taskTx(msg)
    }
}
