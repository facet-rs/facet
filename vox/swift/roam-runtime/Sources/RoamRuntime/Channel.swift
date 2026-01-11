import Foundation

// MARK: - Channel ID

public typealias ChannelId = UInt64

// MARK: - Role

/// Connection role - determines channel ID parity.
public enum Role: Sendable {
    case initiator  // Uses odd IDs (1, 3, 5, ...)
    case acceptor  // Uses even IDs (2, 4, 6, ...)
}

// MARK: - Channel ID Allocator

/// Allocates unique channel IDs with correct parity.
public final class ChannelIdAllocator: @unchecked Sendable {
    private var next: UInt64
    private let lock = NSLock()

    public init(role: Role) {
        switch role {
        case .initiator: next = 1
        case .acceptor: next = 2
        }
    }

    public func allocate() -> ChannelId {
        lock.lock()
        defer { lock.unlock() }
        let id = next
        next += 2
        return id
    }
}

// MARK: - Task Message

/// Messages from handler tasks to the driver.
public enum TaskMessage: Sendable {
    case data(channelId: ChannelId, payload: [UInt8])
    case close(channelId: ChannelId)
    case response(requestId: UInt64, payload: [UInt8])
}

// MARK: - Channel Receiver

/// Receives data on a channel.
public actor ChannelReceiver {
    private var buffer: [[UInt8]] = []
    private var closed = false
    private var waiter: CheckedContinuation<[UInt8]?, Never>?

    public init() {}

    public func deliver(_ data: [UInt8]) {
        if let w = waiter {
            waiter = nil
            w.resume(returning: data)
        } else {
            buffer.append(data)
        }
    }

    public func deliverClose() {
        closed = true
        if let w = waiter {
            waiter = nil
            w.resume(returning: nil)
        }
    }

    public func recv() async -> [UInt8]? {
        if !buffer.isEmpty {
            return buffer.removeFirst()
        }
        if closed {
            return nil
        }
        return await withCheckedContinuation { cont in
            waiter = cont
        }
    }
}

// MARK: - Tx (Send Handle)

/// Handle for sending data on a channel.
///
/// From the caller's perspective, Tx means "I send on this channel".
public final class Tx<T: Sendable>: @unchecked Sendable {
    public var channelId: ChannelId = 0
    private var taskTx: (@Sendable (TaskMessage) -> Void)?
    private let serialize: @Sendable (T) -> [UInt8]

    public init(serialize: @escaping @Sendable (T) -> [UInt8]) {
        self.serialize = serialize
    }

    /// Bind this Tx for sending (server-side).
    public func bind(channelId: ChannelId, taskTx: @escaping @Sendable (TaskMessage) -> Void) {
        self.channelId = channelId
        self.taskTx = taskTx
    }

    /// Send a value.
    public func send(_ value: T) throws {
        guard let taskTx = taskTx else {
            throw ChannelError.notBound
        }
        let bytes = serialize(value)
        taskTx(.data(channelId: channelId, payload: bytes))
    }

    /// Close this channel.
    public func close() {
        taskTx?(.close(channelId: channelId))
    }
}

// MARK: - Rx (Receive Handle)

/// Handle for receiving data on a channel.
///
/// From the caller's perspective, Rx means "I receive from this channel".
public final class Rx<T: Sendable>: @unchecked Sendable {
    public var channelId: ChannelId = 0
    private var receiver: ChannelReceiver?
    private let deserialize: @Sendable ([UInt8]) throws -> T

    public init(deserialize: @escaping @Sendable ([UInt8]) throws -> T) {
        self.deserialize = deserialize
    }

    /// Bind this Rx for receiving (server-side).
    public func bind(channelId: ChannelId, receiver: ChannelReceiver) {
        self.channelId = channelId
        self.receiver = receiver
    }

    /// Receive the next value, or nil if closed.
    public func recv() async throws -> T? {
        guard let receiver = receiver else {
            throw ChannelError.notBound
        }
        guard let bytes = await receiver.recv() else {
            return nil
        }
        return try deserialize(bytes)
    }
}

// MARK: - AsyncSequence for Rx

extension Rx: AsyncSequence {
    public typealias Element = T

    public func makeAsyncIterator() -> AsyncIterator {
        AsyncIterator(rx: self)
    }

    public struct AsyncIterator: AsyncIteratorProtocol {
        let rx: Rx<T>

        public mutating func next() async throws -> T? {
            try await rx.recv()
        }
    }
}

// MARK: - Channel Registry

/// Registry for incoming channels.
public actor ChannelRegistry {
    private var receivers: [ChannelId: ChannelReceiver] = [:]
    private var pendingData: [ChannelId: [[UInt8]]] = [:]
    private var pendingClose: Set<ChannelId> = []
    private var knownChannels: Set<ChannelId> = []

    public init() {}

    /// Mark a channel as known (before dispatch task runs).
    public func markKnown(_ channelId: ChannelId) {
        knownChannels.insert(channelId)
    }

    /// Register a channel and return its receiver.
    public func register(_ channelId: ChannelId) -> ChannelReceiver {
        let receiver = ChannelReceiver()
        receivers[channelId] = receiver
        knownChannels.insert(channelId)

        // Deliver pending data
        if let pending = pendingData.removeValue(forKey: channelId) {
            Task {
                for data in pending {
                    await receiver.deliver(data)
                }
            }
        }

        // Deliver pending close
        if pendingClose.remove(channelId) != nil {
            Task {
                await receiver.deliverClose()
            }
        }

        return receiver
    }

    /// Deliver data to a channel. Returns true if known.
    public func deliverData(channelId: ChannelId, payload: [UInt8]) async -> Bool {
        if let receiver = receivers[channelId] {
            await receiver.deliver(payload)
            return true
        }
        if knownChannels.contains(channelId) {
            pendingData[channelId, default: []].append(payload)
            return true
        }
        return false
    }

    /// Deliver close to a channel. Returns true if known.
    public func deliverClose(channelId: ChannelId) async -> Bool {
        if let receiver = receivers[channelId] {
            await receiver.deliverClose()
            receivers.removeValue(forKey: channelId)
            return true
        }
        if knownChannels.contains(channelId) {
            pendingClose.insert(channelId)
            return true
        }
        return false
    }

    /// Check if a channel is known.
    public func isKnown(_ channelId: ChannelId) -> Bool {
        knownChannels.contains(channelId) || receivers[channelId] != nil
    }
}

// MARK: - Errors

public enum ChannelError: Error {
    case notBound
    case closed
    case unknown
}
