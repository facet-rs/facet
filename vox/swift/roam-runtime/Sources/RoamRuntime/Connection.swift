import Foundation

public final class Connection: @unchecked Sendable {
    let handle: ConnectionHandle

    init(handle: ConnectionHandle) {
        self.handle = handle
    }

    public var channelAllocator: ChannelIdAllocator {
        handle.channelAllocator
    }

    public var incomingChannelRegistry: ChannelRegistry {
        handle.channelRegistry
    }

    public var taskSender: TaskSender {
        { [weak self] msg in
            self?.sendTaskMessage(msg)
        }
    }

    public func call(
        methodId: UInt64,
        metadata: [MetadataEntry],
        payload: Data,
        retry: RetryPolicy = .volatile,
        timeout: TimeInterval?,
        prepareRetry: (@Sendable () async -> PreparedRetryRequest)? = nil,
        finalizeChannels: (@Sendable () -> Void)? = nil
    ) async throws -> Data {
        let response = try await callRaw(
            methodId: methodId,
            metadata: metadata,
            payload: Array(payload),
            retry: retry,
            timeout: timeout,
            prepareRetry: prepareRetry,
            finalizeChannels: finalizeChannels
        )
        return Data(response)
    }

    public func callRaw(
        methodId: UInt64,
        metadata: [MetadataEntry] = [],
        payload: [UInt8],
        retry: RetryPolicy = .volatile,
        timeout: TimeInterval? = nil,
        prepareRetry: (@Sendable () async -> PreparedRetryRequest)? = nil,
        finalizeChannels: (@Sendable () -> Void)? = nil
    ) async throws -> [UInt8] {
        try await handle.callRaw(
            methodId: methodId,
            metadata: metadata,
            payload: payload,
            retry: retry,
            timeout: timeout,
            prepareRetry: prepareRetry,
            finalizeChannels: finalizeChannels
        )
    }

    public func sendTaskMessage(_ msg: TaskMessage) {
        handle.sendTaskMessage(msg)
    }
}

extension Connection: RoamConnection {}
