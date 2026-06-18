import Foundation
@preconcurrency import NIOCore

// MARK: - Unbound Channel Types

/// Unbound Tx - created by `channel()`, bound at call time.
/// r[impl rpc.channel]
/// r[impl rpc.channel.direction]
public final class UnboundTx<T: Sendable>: @unchecked Sendable {
    public private(set) var channelId: ChannelId = 0
    private var taskTx: (@Sendable (TaskMessage) -> Void)?
    private var credit: ChannelCreditController?
    // The per-item codec is injected at bind time from the method's phon element program
    // (mirrors TS: the channel is codec-less until bound). No hand-rolled element bytes.
    private var serialize: (@Sendable (T, inout ByteBuffer) -> Void)?
    private var bound = false
    private var closed = false
    private var callBindingFinalized = false
    private let lock = NSLock()
    private var bindingWaiters: [CheckedContinuation<Void, Never>] = []
    weak var pairedRx: AnyObject?

    public init() {}

    /// Inject the phon typed element codec (called by the generated bind helpers).
    func setSerialize(_ serialize: @escaping @Sendable (T, inout ByteBuffer) -> Void) {
        lock.withLock { self.serialize = serialize }
    }

    public var isBound: Bool { bound }

    /// Bind for sending (client-side outgoing).
    func bind(
        channelId: ChannelId,
        taskTx: @escaping @Sendable (TaskMessage) -> Void,
        credit: ChannelCreditController
    ) {
        let (waiters, shouldCloseImmediately) = lock.withLock {
            () -> ([CheckedContinuation<Void, Never>], Bool) in
            self.channelId = channelId
            self.taskTx = taskTx
            self.credit = credit
            self.bound = true
            let shouldCloseImmediately = self.closed && !self.callBindingFinalized
            self.closed = self.callBindingFinalized
            let waiters = self.bindingWaiters
            self.bindingWaiters.removeAll()
            return (waiters, shouldCloseImmediately)
        }
        for waiter in waiters {
            waiter.resume()
        }
        if shouldCloseImmediately {
            Task {
                await credit.close()
            }
            taskTx(.close(channelId: channelId))
        }
    }

    /// Set channel ID only (when paired Rx is bound).
    func setChannelIdOnly(channelId: ChannelId) {
        let waiters = lock.withLock { () -> [CheckedContinuation<Void, Never>] in
            self.channelId = channelId
            self.bound = true
            let waiters = self.bindingWaiters
            self.bindingWaiters.removeAll()
            return waiters
        }
        for waiter in waiters {
            waiter.resume()
        }
    }

    /// Send a value.
    /// r[impl rpc.channel.pair.tx-read]
    public func send(_ value: T) async throws {
        let (taskTx, credit) = try await waitForSendBinding()
        if lock.withLock({ closed }) {
            throw ChannelError.closed
        }
        guard let serialize = lock.withLock({ self.serialize }) else {
            throw ChannelError.notBound
        }
        try await credit.consume()
        var buf = ByteBufferAllocator().buffer(capacity: 64)
        serialize(value, &buf)
        let bytes = buf.readBytes(length: buf.readableBytes) ?? []
        taskTx(.data(channelId: channelId, payload: bytes))
    }

    /// Close this channel.
    public func close() {
        let shouldClose = lock.withLock {
            if closed {
                return false
            }
            closed = true
            return true
        }
        guard shouldClose else {
            return
        }
        if let credit {
            Task {
                await credit.close()
            }
        }
        taskTx?(.close(channelId: channelId))
    }

    func finishCallBinding() {
        let (waiters, credit) = lock.withLock {
            () -> ([CheckedContinuation<Void, Never>], ChannelCreditController?) in
            if !closed {
                closed = true
            }
            callBindingFinalized = true
            let waiters = bindingWaiters
            bindingWaiters.removeAll()
            return (waiters, self.credit)
        }
        if let credit {
            Task {
                await credit.close()
            }
        }
        for waiter in waiters {
            waiter.resume()
        }
        (pairedRx as? AnyCallBindingFinalizableChannel)?.finishCallBinding()
    }

    private func waitForSendBinding() async throws
        -> (@Sendable (TaskMessage) -> Void, ChannelCreditController)
    {
        while true {
            let state = lock.withLock {
                () -> (
                    taskTx: (@Sendable (TaskMessage) -> Void)?,
                    credit: ChannelCreditController?,
                    bound: Bool,
                    closed: Bool
                ) in
                (taskTx, credit, bound, closed)
            }

            if state.closed {
                throw ChannelError.closed
            }
            if let taskTx = state.taskTx, let credit = state.credit {
                return (taskTx, credit)
            }
            if state.bound {
                throw ChannelError.notBound
            }

            await withCheckedContinuation { continuation in
                let shouldResumeImmediately = lock.withLock { () -> Bool in
                    if closed || bound || (taskTx != nil && credit != nil) {
                        return true
                    }
                    bindingWaiters.append(continuation)
                    return false
                }
                if shouldResumeImmediately {
                    continuation.resume()
                }
            }
        }
    }
}

/// Unbound Rx - created by `channel()`, bound at call time.
/// r[impl rpc.channel]
/// r[impl rpc.channel.direction]
public final class UnboundRx<T: Sendable>: @unchecked Sendable {
    public private(set) var channelId: ChannelId = 0
    // Injected at bind time from the method's phon element program (codec-less until then).
    private var deserialize: (@Sendable (inout ByteBuffer) throws -> T)?
    private var bound = false
    private let lock = NSLock()
    private var bindingWaiters: [CheckedContinuation<Void, Never>] = []
    private var receivers: [ChannelReceiver] = []
    private var callBindingFinalized = false

    // Weak reference to paired Tx
    weak var pairedTx: AnyObject?

    public init() {}

    /// Inject the phon typed element codec (called by the generated bind helpers).
    func setDeserialize(_ deserialize: @escaping @Sendable (inout ByteBuffer) throws -> T) {
        lock.withLock { self.deserialize = deserialize }
    }

    public var isBound: Bool { bound }

    /// Bind for receiving (client-side incoming).
    func bind(channelId: ChannelId, receiver: ChannelReceiver) {
        let waiters = lock.withLock { () -> [CheckedContinuation<Void, Never>] in
            self.channelId = channelId
            self.bound = true
            self.receivers.append(receiver)
            let waiters = self.bindingWaiters
            self.bindingWaiters.removeAll()
            return waiters
        }
        for waiter in waiters {
            waiter.resume()
        }
    }

    /// Set channel ID only (when paired Tx is bound).
    func setChannelIdOnly(channelId: ChannelId) {
        let waiters = lock.withLock { () -> [CheckedContinuation<Void, Never>] in
            self.channelId = channelId
            self.bound = true
            let waiters = self.bindingWaiters
            self.bindingWaiters.removeAll()
            return waiters
        }
        for waiter in waiters {
            waiter.resume()
        }
    }

    /// Receive the next value, or nil after a graceful channel close.
    /// r[impl rpc.channel.pair.rx-take]
    public func recv() async throws -> T? {
        while true {
            let receiver = lock.withLock { receivers.first }
            if let receiver {
                if let bytes = try await receiver.recv() {
                    guard let deserialize = lock.withLock({ self.deserialize }) else {
                        throw ChannelError.notBound
                    }
                    var buf = ByteBufferAllocator().buffer(capacity: bytes.count)
                    buf.writeBytes(bytes)
                    return try deserialize(&buf)
                }

                let shouldEnd = lock.withLock { () -> Bool in
                    if let head = receivers.first, head === receiver {
                        receivers.removeFirst()
                    }
                    return callBindingFinalized && receivers.isEmpty
                }
                if shouldEnd {
                    return nil
                }
                continue
            }

            let shouldEnd = lock.withLock { callBindingFinalized && receivers.isEmpty }
            if shouldEnd {
                return nil
            }
            await withCheckedContinuation { continuation in
                let shouldResumeImmediately = lock.withLock { () -> Bool in
                    if !receivers.isEmpty || (callBindingFinalized && receivers.isEmpty) {
                        return true
                    }
                    bindingWaiters.append(continuation)
                    return false
                }
                if shouldResumeImmediately {
                    continuation.resume()
                }
            }
        }
    }

    func finishCallBinding() {
        let (waiters, receivers) = lock.withLock {
            () -> ([CheckedContinuation<Void, Never>], [ChannelReceiver]) in
            callBindingFinalized = true
            let waiters = bindingWaiters
            bindingWaiters.removeAll()
            return (waiters, self.receivers)
        }
        for receiver in receivers {
            // r[impl rpc.request.scope.channels]
            if !receiver.debugSnapshot().closed {
                receiver.deliverReset(.requestClosed)
            }
        }
        for waiter in waiters {
            waiter.resume()
        }
    }
}

// MARK: - AsyncSequence for UnboundRx

extension UnboundRx: AsyncSequence {
    public typealias Element = T

    public func makeAsyncIterator() -> AsyncIterator {
        AsyncIterator(rx: self)
    }

    public struct AsyncIterator: AsyncIteratorProtocol {
        let rx: UnboundRx<T>

        public mutating func next() async throws -> T? {
            try await rx.recv()
        }
    }
}

// MARK: - Channel Factory

/// Create a paired, codec-less unbound channel. The per-item phon element codec is
/// injected at bind time by the generated client (keyed on the method's `elementRoot`),
/// so callers never hand-roll element bytes — mirrors the TS `channel()` design.
/// r[impl rpc.channel]
/// r[impl rpc.channel.direction]
/// r[impl rpc.channel.pair]
/// r[impl rpc.channel.pair.binding-propagation]
public func channel<T: Sendable>() -> (UnboundTx<T>, UnboundRx<T>) {
    let tx = UnboundTx<T>()
    let rx = UnboundRx<T>()
    tx.pairedRx = rx
    rx.pairedTx = tx
    return (tx, rx)
}

// MARK: - Task Sender

/// Type alias for task message sender.
public typealias TaskSender = @Sendable (TaskMessage) -> Void

// MARK: - Incoming Channel Registry

/// Type alias for incoming channel registry.
public typealias IncomingChannelRegistry = ChannelRegistry

// Channel binding by schema-walking (TypeRef/Schema/SchemaKind) was removed:
// channels are now bound OUT-OF-BAND via the generated code's PhonChannelMeta
// (arg index + direction + element root), not by resolving the args schema here.

// MARK: - Type Erasure for Binding

/// Protocol for type-erased UnboundRx binding.
protocol AnyUnboundRx: AnyObject {
    func bindForSchema(
        channelId: ChannelId,
        taskSender: @escaping TaskSender,
        credit: ChannelCreditController
    )
    func channelIdForSchema() -> ChannelId
}

/// Protocol for type-erased UnboundTx binding.
protocol AnyUnboundTx: AnyObject {
    func bindForSchema(channelId: ChannelId, receiver: ChannelReceiver)
    func channelIdForSchema() -> ChannelId
}

extension UnboundRx: AnyUnboundRx {
    // r[impl rpc.channel.pair.binding-propagation]
    func bindForSchema(
        channelId: ChannelId,
        taskSender: @escaping TaskSender,
        credit: ChannelCreditController
    ) {
        // Schema Rx = client sends via Tx, so bind the paired Tx
        if let pairedTx = self.pairedTx as? AnyUnboundTxSender {
            pairedTx.bindForSending(channelId: channelId, taskSender: taskSender, credit: credit)
        }
        self.setChannelIdOnly(channelId: channelId)
    }

    func channelIdForSchema() -> ChannelId {
        channelId
    }
}

extension UnboundTx: AnyUnboundTx {
    // r[impl rpc.channel.pair.binding-propagation]
    func bindForSchema(channelId: ChannelId, receiver: ChannelReceiver) {
        // Schema Tx = client receives via Rx, so this Tx just gets ID
        self.setChannelIdOnly(channelId: channelId)
        if let pairedRx = self.pairedRx as? AnyUnboundRxReceiver {
            pairedRx.bindForReceiving(channelId: channelId, receiver: receiver)
        }
    }

    func channelIdForSchema() -> ChannelId {
        channelId
    }
}

/// Protocol for sending via Tx.
protocol AnyUnboundTxSender: AnyObject {
    func bindForSending(
        channelId: ChannelId,
        taskSender: @escaping TaskSender,
        credit: ChannelCreditController
    )
}

protocol AnyCallBindingFinalizableChannel: AnyObject {
    func finishCallBinding()
}

extension UnboundTx: AnyUnboundTxSender {
    func bindForSending(
        channelId: ChannelId,
        taskSender: @escaping TaskSender,
        credit: ChannelCreditController
    ) {
        self.bind(channelId: channelId, taskTx: taskSender, credit: credit)
    }
}

extension UnboundTx: AnyCallBindingFinalizableChannel {}

protocol AnyUnboundRxReceiver: AnyObject {
    func bindForReceiving(channelId: ChannelId, receiver: ChannelReceiver)
}

extension UnboundRx: AnyUnboundRxReceiver {
    func bindForReceiving(channelId: ChannelId, receiver: ChannelReceiver) {
        self.bind(channelId: channelId, receiver: receiver)
    }
}

extension UnboundRx: AnyCallBindingFinalizableChannel {}
