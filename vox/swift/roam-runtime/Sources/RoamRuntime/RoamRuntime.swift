import Foundation

// MARK: - Re-exports for public API

// Encoding
public typealias PostcardEncoder<T> = (T) -> [UInt8]
public typealias PostcardDecoder<T> = ([UInt8]) throws -> T

// MARK: - RoamConnection Protocol

/// Protocol for roam connections (used by generated clients).
public protocol RoamConnection: Sendable {
    /// Make a raw RPC call.
    func call(methodId: UInt64, payload: Data) async throws -> Data

    /// Get the channel allocator.
    var channelAllocator: ChannelIdAllocator { get }

    /// Get the incoming channel registry.
    var incomingChannelRegistry: ChannelRegistry { get }

    /// Get task sender for outgoing channel messages.
    var taskSender: TaskSender { get }
}

// MARK: - ConnectionHandle RoamConnection Conformance

extension ConnectionHandle: RoamConnection {
    public func call(methodId: UInt64, payload: Data) async throws -> Data {
        let response = try await callRaw(methodId: methodId, payload: Array(payload))
        return Data(response)
    }

    public var incomingChannelRegistry: ChannelRegistry {
        channelRegistry
    }

    public var taskSender: TaskSender {
        { [weak self] msg in
            // Route through the handle's command channel
            // This requires the driver to handle task messages from the client side
            // For now, this is a stub - full implementation needs driver integration
            _ = self
            _ = msg
        }
    }
}

// MARK: - RoamError

/// Errors that can occur during roam operations.
///
/// r[impl core.error.roam-error] - RoamError represents call-level errors.
/// r[impl core.error.call-vs-connection] - Call errors don't terminate connection.
/// r[impl call.error.roam-error] - RoamError variants for different error types.
/// r[impl call.error.user] - User errors propagate through RoamError.
public enum RoamError: Error {
    case unknownMethod
    case notImplemented
    case decodeError(String)
    case encodeError(String)
    case connectionClosed
    case timeout
    case cancelled
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
/// r[impl call.error.unknown-method] - UnknownMethod when method_id not recognized.
public func encodeUnknownMethodError() -> [UInt8] {
    [1, 1]  // Err discriminant + UnknownMethod variant
}

/// Encode an invalid payload error.
///
/// r[impl call.error.invalid-payload] - InvalidPayload when payload fails to decode.
public func encodeInvalidPayloadError() -> [UInt8] {
    [1, 2]  // Err discriminant + InvalidPayload variant
}

// MARK: - Server-side Channel Helpers

/// Create a server-side Tx for sending to client.
public func createServerTx<T: Sendable>(
    channelId: ChannelId,
    taskSender: @escaping TaskSender,
    serialize: @escaping @Sendable (T) -> [UInt8]
) -> Tx<T> {
    let tx = Tx<T>(serialize: serialize)
    tx.bind(channelId: channelId, taskTx: taskSender)
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
