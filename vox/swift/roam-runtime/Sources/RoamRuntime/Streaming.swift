import Foundation

// MARK: - Stream ID

/// A unique identifier for a stream within a connection.
///
/// r[impl streaming.id.uniqueness] - Stream IDs must be unique within a connection.
/// r[impl streaming.id.zero-reserved] - Stream ID 0 is reserved.
public struct StreamId: Hashable, Sendable {
    public let value: UInt64

    public init(_ value: UInt64) {
        self.value = value
    }

    /// The stream ID to encode on the wire.
    public var streamId: UInt64 {
        return value
    }
}

// MARK: - Push (Caller→Callee)

/// A stream for sending data from caller to callee.
///
/// r[impl streaming.caller-pov] - Push means "I send on this".
/// r[impl core.stream] - Push represents data flowing from caller to callee.
///
/// The caller uses Push to send data. The callee (handler) receives a Pull
/// with the same stream ID since the types are inverted for the handler.
public struct Push<T: Sendable>: Sendable {
    /// The stream ID assigned by the caller.
    public let streamId: UInt64

    /// A continuation for sending values on this stream.
    private let continuation: AsyncStream<T>.Continuation

    /// Create a new Push stream with the given ID and continuation.
    public init(streamId: UInt64, continuation: AsyncStream<T>.Continuation) {
        self.streamId = streamId
        self.continuation = continuation
    }

    /// Send a value on this stream.
    ///
    /// r[impl streaming.data] - Each Data message contains exactly one value.
    public func send(_ value: T) {
        continuation.yield(value)
    }

    /// Close this stream, signaling end of data.
    ///
    /// r[impl streaming.close] - Caller sends Close when done.
    /// r[impl streaming.lifecycle.caller-closes-pushes] - Caller must send Close.
    public func close() {
        continuation.finish()
    }
}

// MARK: - Pull (Callee→Caller)

/// A stream for receiving data from callee to caller.
///
/// r[impl streaming.caller-pov] - Pull means "I receive from this".
/// r[impl core.stream] - Pull represents data flowing from callee to caller.
///
/// The caller uses Pull to receive data. The callee (handler) receives a Push
/// with the same stream ID since the types are inverted for the handler.
public struct Pull<T: Sendable>: Sendable {
    /// The stream ID assigned by the caller.
    public let streamId: UInt64

    /// The underlying async stream for receiving values.
    public let stream: AsyncThrowingStream<T, Error>

    /// Create a new Pull stream with the given ID and stream.
    public init(streamId: UInt64, stream: AsyncThrowingStream<T, Error>) {
        self.streamId = streamId
        self.stream = stream
    }

    /// Receive all values from this stream.
    ///
    /// r[impl streaming.lifecycle.response-closes-pulls] - Stream closes when Response is sent.
    public func collect() async throws -> [T] {
        var result: [T] = []
        for try await value in stream {
            result.append(value)
        }
        return result
    }
}

// MARK: - Stream Registry

/// A registry for managing active streams within a connection.
///
/// r[impl streaming.id.uniqueness] - Ensures stream IDs are unique.
/// r[impl streaming.allocation.caller] - Caller allocates all stream IDs.
public actor StreamRegistry {
    /// The next stream ID to allocate.
    /// r[impl streaming.id.parity] - Initiator uses odd IDs, acceptor uses even.
    private var nextStreamId: UInt64

    /// Whether this peer is the initiator (odd IDs) or acceptor (even IDs).
    private let isInitiator: Bool

    /// Active streams indexed by their ID.
    private var streams: [UInt64: AnyObject] = [:]

    /// Create a new stream registry.
    ///
    /// - Parameter isInitiator: True if this peer initiated the connection.
    public init(isInitiator: Bool) {
        self.isInitiator = isInitiator
        // r[impl streaming.id.parity] - Initiator starts at 1 (odd), acceptor at 2 (even).
        self.nextStreamId = isInitiator ? 1 : 2
    }

    /// Allocate a new stream ID.
    ///
    /// r[impl streaming.allocation.caller] - Caller allocates all stream IDs.
    public func allocateStreamId() -> UInt64 {
        let id = nextStreamId
        // r[impl streaming.id.parity] - Skip by 2 to maintain parity.
        nextStreamId += 2
        return id
    }

    /// Register a stream with its ID.
    public func register<T: AnyObject>(stream: T, id: UInt64) {
        streams[id] = stream
    }

    /// Unregister a stream by its ID.
    public func unregister(id: UInt64) {
        streams.removeValue(forKey: id)
    }

    /// Get a stream by its ID.
    public func get<T: AnyObject>(id: UInt64) -> T? {
        return streams[id] as? T
    }
}
