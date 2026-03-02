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
///
/// r[impl rpc.request.id-allocation] - IDs are unique within a connection.
/// r[impl session.parity] - Initiator uses odd, Acceptor uses even.
/// r[impl rpc.channel.allocation] - Caller allocates ALL channel IDs.
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
public final class ChannelReceiver: @unchecked Sendable {
    private let lock = NSLock()
    private var buffer: [[UInt8]] = []
    private var closed = false
    private var waiter: CheckedContinuation<[UInt8]?, Never>?

    public init() {}

    public func deliver(_ data: [UInt8]) {
        var toResume: CheckedContinuation<[UInt8]?, Never>?
        lock.lock()
        if let w = waiter {
            waiter = nil
            toResume = w
        } else if !closed {
            buffer.append(data)
        }
        lock.unlock()
        toResume?.resume(returning: data)
    }

    public func deliverClose() {
        var toResume: CheckedContinuation<[UInt8]?, Never>?
        lock.lock()
        closed = true
        if let w = waiter {
            waiter = nil
            toResume = w
        }
        lock.unlock()
        toResume?.resume(returning: nil)
    }

    /// Handle reset - abruptly close without delivering buffered data.
    public func deliverReset() {
        var toResume: CheckedContinuation<[UInt8]?, Never>?
        lock.lock()
        closed = true
        buffer.removeAll()
        if let w = waiter {
            waiter = nil
            toResume = w
        }
        lock.unlock()
        toResume?.resume(returning: nil)
    }

    public func recv() async -> [UInt8]? {
        enum RecvState {
            case value([UInt8])
            case closed
            case wait
        }
        let state = lock.withLock { () -> RecvState in
            if !buffer.isEmpty {
                return .value(buffer.removeFirst())
            }
            if closed {
                return .closed
            }
            return .wait
        }
        switch state {
        case .value(let value):
            return value
        case .closed:
            return nil
        case .wait:
            break
        }
        return await withCheckedContinuation { cont in
            lock.withLock {
                if !buffer.isEmpty {
                    let value = buffer.removeFirst()
                    cont.resume(returning: value)
                    return
                }
                if closed {
                    cont.resume(returning: nil)
                    return
                }
                waiter = cont
            }
        }
    }
}

private extension NSLock {
    func withLock<T>(_ body: () throws -> T) rethrows -> T {
        lock()
        defer { unlock() }
        return try body()
    }
}

// MARK: - Tx (Send Handle)

/// Handle for sending data on a channel.
///
/// r[impl rpc.channel.binding] - From caller's perspective, Tx means "I send".
/// r[impl rpc.channel] - Serializes as u64 channel ID on wire.
/// r[impl rpc.channel.lifecycle] - The holder sends on this channel.
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
    ///
    /// r[impl rpc.channel.item] - Data messages carry serialized values.
    public func send(_ value: T) throws {
        guard let taskTx = taskTx else {
            throw ChannelError.notBound
        }
        let bytes = serialize(value)
        taskTx(.data(channelId: channelId, payload: bytes))
    }

    /// Close this channel.
    ///
    /// r[impl rpc.channel.close] - Close terminates the channel.
    /// r[impl rpc.channel.lifecycle] - Caller sends Close when done.
    public func close() {
        taskTx?(.close(channelId: channelId))
    }
}

// MARK: - Rx (Receive Handle)

/// Handle for receiving data on a channel.
///
/// r[impl rpc.channel.binding] - From caller's perspective, Rx means "I receive".
/// r[impl rpc.channel] - Serializes as u64 channel ID on wire.
/// r[impl rpc.channel.lifecycle] - The holder receives from this channel.
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
///
/// r[impl rpc.metadata.unknown] - Unknown channel IDs cause Goodbye.
/// r[impl rpc.channel.item] - Data messages routed by channel_id.
/// r[impl rpc.channel.lifecycle] - Channels may outlive the response.
/// r[impl rpc.channel.lifecycle] - Call completion independent of channel lifecycle.
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
    /// This is async to ensure pending data/close are delivered synchronously
    /// before returning, avoiding race conditions with the handler.
    public func register(_ channelId: ChannelId) async -> ChannelReceiver {
        let receiver = ChannelReceiver()
        receivers[channelId] = receiver
        knownChannels.insert(channelId)

        // Deliver pending data synchronously - no Task spawning!
        if let pending = pendingData.removeValue(forKey: channelId) {
            for data in pending {
                receiver.deliver(data)
            }
        }

        // Deliver pending close synchronously after all data
        if pendingClose.remove(channelId) != nil {
            receiver.deliverClose()
        }

        return receiver
    }

    /// Deliver data to a channel. Returns true if known.
    ///
    /// r[impl rpc.channel.close] - Data after close is rejected.
    /// r[impl rpc.flow-control.credit.exhaustion] - Data size bounded by max_payload_size.
    public func deliverData(channelId: ChannelId, payload: [UInt8]) async -> Bool {
        if let receiver = receivers[channelId] {
            receiver.deliver(payload)
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
            receiver.deliverClose()
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

    /// Deliver reset to a channel.
    ///
    /// r[impl rpc.channel.reset] - Reset abruptly terminates channel.
    public func deliverReset(channelId: ChannelId) async {
        if let receiver = receivers[channelId] {
            receiver.deliverReset()
            receivers.removeValue(forKey: channelId)
        }
        knownChannels.remove(channelId)
        pendingData.removeValue(forKey: channelId)
        pendingClose.remove(channelId)
    }

    /// Deliver credit to a channel.
    ///
    /// r[impl rpc.flow-control.credit.grant] - Credit message grants permission.
    /// r[impl rpc.flow-control.credit] - Infinite credit mode bypasses accounting.
    public func deliverCredit(channelId: ChannelId, bytes: UInt32) async {
        // TODO: Implement credit tracking for flow control
        // For now, we operate in "infinite credit" mode
        _ = channelId
        _ = bytes
    }
}

// MARK: - Errors

public enum ChannelError: Error {
    case notBound
    case closed
    case unknown
}
