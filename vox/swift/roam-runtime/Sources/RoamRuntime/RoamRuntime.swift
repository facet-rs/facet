import Foundation

// MARK: - Re-exports for public API

// Encoding
public typealias PostcardEncoder<T> = (T) -> [UInt8]
public typealias PostcardDecoder<T> = ([UInt8]) throws -> T

// MARK: - RoamConnection Protocol

/// Protocol for roam connections (used by generated clients).
public protocol RoamConnection: Sendable {
    /// Make a raw RPC call.
    func call(
        methodId: UInt64,
        metadata: [MetadataEntryV7],
        payload: Data,
        channels: [UInt64],
        retry: RetryPolicy,
        timeout: TimeInterval?,
        prepareRetry: (@Sendable () async -> PreparedRetryRequest)?,
        finalizeChannels: (@Sendable () -> Void)?
    ) async throws -> Data

    /// Get the channel allocator.
    var channelAllocator: ChannelIdAllocator { get }

    /// Get the incoming channel registry.
    var incomingChannelRegistry: ChannelRegistry { get }

    /// Get task sender for outgoing channel messages.
    var taskSender: TaskSender { get }
}

public extension RoamConnection {
    func call(
        methodId: UInt64,
        payload: Data,
        channels: [UInt64],
        retry: RetryPolicy,
        timeout: TimeInterval?
    ) async throws -> Data {
        try await call(
            methodId: methodId,
            metadata: [],
            payload: payload,
            channels: channels,
            retry: retry,
            timeout: timeout,
            prepareRetry: nil,
            finalizeChannels: nil
        )
    }

    func call(
        methodId: UInt64,
        payload: Data,
        channels: [UInt64],
        retry: RetryPolicy,
        timeout: TimeInterval?,
        prepareRetry: (@Sendable () async -> PreparedRetryRequest)?,
        finalizeChannels: (@Sendable () -> Void)?
    ) async throws -> Data {
        try await call(
            methodId: methodId,
            metadata: [],
            payload: payload,
            channels: channels,
            retry: retry,
            timeout: timeout,
            prepareRetry: prepareRetry,
            finalizeChannels: finalizeChannels
        )
    }

    func call(
        methodId: UInt64,
        metadata: [MetadataEntryV7],
        payload: Data,
        channels: [UInt64],
        timeout: TimeInterval?
    ) async throws -> Data {
        try await call(
            methodId: methodId,
            metadata: metadata,
            payload: payload,
            channels: channels,
            retry: .volatile,
            timeout: timeout,
            prepareRetry: nil,
            finalizeChannels: nil
        )
    }

    func call(methodId: UInt64, payload: Data) async throws -> Data {
        try await call(
            methodId: methodId,
            metadata: [],
            payload: payload,
            channels: [],
            retry: .volatile,
            timeout: nil,
            prepareRetry: nil,
            finalizeChannels: nil
        )
    }

    func call(methodId: UInt64, payload: Data, timeout: TimeInterval?) async throws -> Data {
        try await call(
            methodId: methodId,
            metadata: [],
            payload: payload,
            channels: [],
            retry: .volatile,
            timeout: timeout,
            prepareRetry: nil,
            finalizeChannels: nil
        )
    }

    func call(
        methodId: UInt64,
        metadata: [MetadataEntryV7],
        payload: Data,
        timeout: TimeInterval?
    ) async throws -> Data {
        try await call(
            methodId: methodId,
            metadata: metadata,
            payload: payload,
            channels: [],
            retry: .volatile,
            timeout: timeout,
            prepareRetry: nil,
            finalizeChannels: nil
        )
    }

    func call(
        methodId: UInt64,
        payload: Data,
        channels: [UInt64],
        timeout: TimeInterval?
    ) async throws -> Data {
        try await call(
            methodId: methodId,
            metadata: [],
            payload: payload,
            channels: channels,
            retry: .volatile,
            timeout: timeout,
            prepareRetry: nil,
            finalizeChannels: nil
        )
    }
}

// MARK: - Connection RoamConnection Conformance

// MARK: - RoamError

/// Errors that can occur during roam operations.
///
/// r[impl rpc.fallible.roam-error] - RoamError represents call-level errors.
/// r[impl rpc.error.scope] - Call errors don't terminate connection.
/// r[impl rpc.fallible] - RoamError variants for different error types.
/// r[impl rpc.fallible.caller-signature] - User errors propagate through RoamError.
public enum RoamError: Error {
    case unknownMethod
    case notImplemented
    case decodeError(String)
    case encodeError(String)
    case connectionClosed
    case timeout
    case cancelled
    case indeterminate
}

// MARK: - Response Encoding Helpers

/// Encode a successful result.
public func encodeResultOk<T>(_ value: T, encoder: (T) -> [UInt8]) -> [UInt8] {
    [0] + encoder(value)  // 0 = Ok discriminant
}

/// Encode a successful unit result.
public func encodeResultOkUnit() -> [UInt8] {
    [0]  // Ok(()) - just the discriminant
}

/// Encode an unknown method error.
///
/// r[impl rpc.unknown-method] - UnknownMethod when method_id not recognized.
public func encodeUnknownMethodError() -> [UInt8] {
    [1, 1]  // Err discriminant + UnknownMethod variant
}

/// Encode an invalid payload error.
///
/// r[impl rpc.error.scope] - InvalidPayload when payload fails to decode.
public func encodeInvalidPayloadError() -> [UInt8] {
    [1, 2]  // Err discriminant + InvalidPayload variant
}

/// Encode a cancelled error.
public func encodeCancelledError() -> [UInt8] {
    [1, 3]
}

/// Encode an indeterminate error.
public func encodeIndeterminateError() -> [UInt8] {
    [1, 4]
}

// MARK: - Server-side Channel Helpers

/// Create a server-side Tx for sending to client.
public func createServerTx<T: Sendable>(
    channelId: ChannelId,
    taskSender: @escaping TaskSender,
    registry: ChannelRegistry,
    initialCredit: UInt32,
    serialize: @escaping @Sendable (T) -> [UInt8]
) async -> Tx<T> {
    let tx = Tx<T>(serialize: serialize)
    let credit = await registry.registerOutgoing(channelId, initialCredit: initialCredit)
    tx.bind(channelId: channelId, taskTx: taskSender, credit: credit)
    return tx
}

/// Create a server-side Rx for receiving from client.
public func createServerRx<T: Sendable>(
    channelId: ChannelId,
    receiver: ChannelReceiver,
    deserialize: @escaping @Sendable ([UInt8]) throws -> T
) -> Rx<T> {
    let rx = Rx<T>(deserialize: deserialize)
    rx.bind(channelId: channelId, receiver: receiver)
    return rx
}
