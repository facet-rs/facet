import Foundation
@preconcurrency import NIOCore

// MARK: - Re-exports for public API

// Encoding
public typealias PostcardEncoder<T> = (T, inout ByteBuffer) -> Void
public typealias PostcardDecoder<T> = (inout ByteBuffer) throws -> T

// MARK: - Client Schema Info

/// Schema information for a client call. Used to send schema data with outgoing requests.
public struct ClientSchemaInfo: Sendable {
    /// Method schema information
    public let methodInfo: MethodSchemaInfo
    /// Global schema registry
    public let schemaRegistry: [SchemaHash: Schema]

    public init(methodInfo: MethodSchemaInfo, schemaRegistry: [SchemaHash: Schema]) {
        self.methodInfo = methodInfo
        self.schemaRegistry = schemaRegistry
    }
}

// MARK: - VoxConnection Protocol

/// Protocol for vox connections (used by generated clients).
public protocol VoxConnection: Sendable {
    /// Make a raw RPC call.
    func call(
        methodId: UInt64,
        metadata: [MetadataEntry],
        payload: [UInt8],
        retry: RetryPolicy,
        timeout: TimeInterval?,
        prepareRetry: (@Sendable () async -> PreparedRetryRequest)?,
        finalizeChannels: (@Sendable () -> Void)?,
        schemaInfo: ClientSchemaInfo?
    ) async throws -> [UInt8]

    /// Get the channel allocator.
    var channelAllocator: ChannelIdAllocator { get }

    /// Get the incoming channel registry.
    var incomingChannelRegistry: ChannelRegistry { get }

    /// Get task sender for outgoing channel messages.
    var taskSender: TaskSender { get }
}

extension VoxConnection {
    public func call(
        methodId: UInt64,
        metadata: [MetadataEntry],
        payload: [UInt8],
        retry: RetryPolicy,
        timeout: TimeInterval?,
        prepareRetry: (@Sendable () async -> PreparedRetryRequest)?,
        finalizeChannels: (@Sendable () -> Void)?
    ) async throws -> [UInt8] {
        try await call(
            methodId: methodId,
            metadata: metadata,
            payload: payload,
            retry: retry,
            timeout: timeout,
            prepareRetry: prepareRetry,
            finalizeChannels: finalizeChannels,
            schemaInfo: nil
        )
    }

    public func call(
        methodId: UInt64,
        payload: [UInt8],
        retry: RetryPolicy,
        timeout: TimeInterval?
    ) async throws -> [UInt8] {
        try await call(
            methodId: methodId,
            metadata: [],
            payload: payload,
            retry: retry,
            timeout: timeout,
            prepareRetry: nil,
            finalizeChannels: nil,
            schemaInfo: nil
        )
    }

    public func call(
        methodId: UInt64,
        payload: [UInt8],
        retry: RetryPolicy,
        timeout: TimeInterval?,
        prepareRetry: (@Sendable () async -> PreparedRetryRequest)?,
        finalizeChannels: (@Sendable () -> Void)?
    ) async throws -> [UInt8] {
        try await call(
            methodId: methodId,
            metadata: [],
            payload: payload,
            retry: retry,
            timeout: timeout,
            prepareRetry: prepareRetry,
            finalizeChannels: finalizeChannels,
            schemaInfo: nil
        )
    }

    public func call(
        methodId: UInt64,
        metadata: [MetadataEntry],
        payload: [UInt8],
        timeout: TimeInterval?
    ) async throws -> [UInt8] {
        try await call(
            methodId: methodId,
            metadata: metadata,
            payload: payload,
            retry: .volatile,
            timeout: timeout,
            prepareRetry: nil,
            finalizeChannels: nil,
            schemaInfo: nil
        )
    }

    public func call(methodId: UInt64, payload: [UInt8]) async throws -> [UInt8] {
        try await call(
            methodId: methodId,
            metadata: [],
            payload: payload,
            retry: .volatile,
            timeout: nil,
            prepareRetry: nil,
            finalizeChannels: nil,
            schemaInfo: nil
        )
    }

    public func call(methodId: UInt64, payload: [UInt8], timeout: TimeInterval?) async throws
        -> [UInt8]
    {
        try await call(
            methodId: methodId,
            metadata: [],
            payload: payload,
            retry: .volatile,
            timeout: timeout,
            prepareRetry: nil,
            finalizeChannels: nil,
            schemaInfo: nil
        )
    }

}

// MARK: - Connection VoxConnection Conformance

// MARK: - VoxError

/// Errors that can occur during vox operations.
///
/// r[impl rpc.fallible.vox-error] - VoxError represents call-level errors.
/// r[impl rpc.error.scope] - Call errors don't terminate connection.
/// r[impl rpc.fallible] - VoxError variants for different error types.
/// r[impl rpc.fallible.caller-signature] - User errors propagate through VoxError.
public enum VoxError: Error {
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

/// Encode a successful result into a fresh [UInt8] payload.
public func encodeResultOk<T>(_ value: T, encoder: (T, inout ByteBuffer) -> Void) -> [UInt8] {
    var buffer = ByteBufferAllocator().buffer(capacity: 64)
    buffer.writeInteger(UInt8(0))  // Ok discriminant
    encoder(value, &buffer)
    return buffer.readBytes(length: buffer.readableBytes) ?? []
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
    serialize: @escaping @Sendable (T, inout ByteBuffer) -> Void
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
    deserialize: @escaping @Sendable (inout ByteBuffer) throws -> T
) -> Rx<T> {
    let rx = Rx<T>(deserialize: deserialize)
    rx.bind(channelId: channelId, receiver: receiver)
    return rx
}
