import Foundation
@preconcurrency import NIOCore

// MARK: - Unbound Channel Types

/// Unbound Tx - created by `channel()`, bound at call time.
public final class UnboundTx<T: Sendable>: @unchecked Sendable {
    public private(set) var channelId: ChannelId = 0
    private var taskTx: (@Sendable (TaskMessage) -> Void)?
    private var credit: ChannelCreditController?
    private let serialize: @Sendable (T, inout ByteBuffer) -> Void
    private var bound = false
    private var closed = false
    private let lock = NSLock()
    private var bindingWaiters: [CheckedContinuation<Void, Never>] = []
    weak var pairedRx: AnyObject?

    public init(serialize: @escaping @Sendable (T, inout ByteBuffer) -> Void) {
        self.serialize = serialize
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
            let shouldCloseImmediately = self.closed
            self.closed = false
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
    public func send(_ value: T) async throws {
        let (taskTx, credit) = try await waitForSendBinding()
        if lock.withLock({ closed }) {
            throw ChannelError.closed
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

    func finishRetryBinding() {
        close()
        (pairedRx as? AnyRetryFinalizableChannel)?.finishRetryBinding()
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
public final class UnboundRx<T: Sendable>: @unchecked Sendable {
    public private(set) var channelId: ChannelId = 0
    private let deserialize: @Sendable (inout ByteBuffer) throws -> T
    private var bound = false
    private let lock = NSLock()
    private var bindingWaiters: [CheckedContinuation<Void, Never>] = []
    private var receivers: [ChannelReceiver] = []
    private var retryFinalized = false

    // Weak reference to paired Tx
    weak var pairedTx: AnyObject?

    public init(deserialize: @escaping @Sendable (inout ByteBuffer) throws -> T) {
        self.deserialize = deserialize
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

    /// Receive the next value, or nil if closed.
    public func recv() async throws -> T? {
        while true {
            let receiver = lock.withLock { receivers.first }
            if let receiver {
                if let bytes = await receiver.recv() {
                    var buf = ByteBufferAllocator().buffer(capacity: bytes.count)
                    buf.writeBytes(bytes)
                    return try deserialize(&buf)
                }

                let shouldEnd = lock.withLock { () -> Bool in
                    if let head = receivers.first, head === receiver {
                        receivers.removeFirst()
                    }
                    return retryFinalized && receivers.isEmpty
                }
                if shouldEnd {
                    return nil
                }
                continue
            }

            let shouldEnd = lock.withLock { retryFinalized && receivers.isEmpty }
            if shouldEnd {
                return nil
            }
            await withCheckedContinuation { continuation in
                let shouldResumeImmediately = lock.withLock { () -> Bool in
                    if !receivers.isEmpty || (retryFinalized && receivers.isEmpty) {
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

    func finishRetryBinding() {
        let waiters = lock.withLock { () -> [CheckedContinuation<Void, Never>] in
            retryFinalized = true
            let waiters = bindingWaiters
            bindingWaiters.removeAll()
            return waiters
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

/// Create paired unbound channels.
public func channel<T: Sendable>(
    serialize: @escaping @Sendable (T, inout ByteBuffer) -> Void,
    deserialize: @escaping @Sendable (inout ByteBuffer) throws -> T
) -> (UnboundTx<T>, UnboundRx<T>) {
    let tx = UnboundTx<T>(serialize: serialize)
    let rx = UnboundRx<T>(deserialize: deserialize)
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

// MARK: - Bind Channels

/// Bind channels from method arguments using schema.
public func bindChannels(
    schemas: [BindingSchema],
    args: [Any],
    allocator: ChannelIdAllocator,
    incomingRegistry: ChannelRegistry,
    taskSender: @escaping TaskSender,
    serializers: any BindingSerializers
) async {
    for (schema, arg) in zip(schemas, args) {
        await bindValue(
            schema: schema,
            value: arg,
            allocator: allocator,
            incomingRegistry: incomingRegistry,
            taskSender: taskSender,
            serializers: serializers
        )
    }
}

public func finalizeBoundChannels(
    schemas: [BindingSchema],
    args: [Any]
) {
    for (schema, arg) in zip(schemas, args) {
        finalizeValue(schema: schema, value: arg)
    }
}

/// Collect bound channel IDs in argument declaration order.
public func collectChannelIds(schemas: [BindingSchema], args: [Any]) -> [UInt64] {
    var channelIds: [UInt64] = []
    for (schema, arg) in zip(schemas, args) {
        collectChannelIdsFromValue(schema: schema, value: arg, out: &channelIds)
    }
    return channelIds
}

private func collectChannelIdsFromValue(schema: BindingSchema, value: Any, out: inout [UInt64]) {
    switch schema {
    case .rx(_, _):
        if let rx = value as? AnyUnboundRx {
            out.append(rx.channelIdForSchema())
        }
    case .tx(_, _):
        if let tx = value as? AnyUnboundTx {
            out.append(tx.channelIdForSchema())
        }
    case .vec(let element):
        if let arr = value as? [Any] {
            for item in arr {
                collectChannelIdsFromValue(schema: element, value: item, out: &out)
            }
        }
    case .option(let inner):
        let mirror = Mirror(reflecting: value)
        if mirror.displayStyle == .optional, let (_, unwrapped) = mirror.children.first {
            collectChannelIdsFromValue(schema: inner, value: unwrapped, out: &out)
        }
    case .struct(let fields):
        let mirror = Mirror(reflecting: value)
        for (fieldName, fieldSchema) in fields {
            if let child = mirror.children.first(where: { $0.label == fieldName }) {
                collectChannelIdsFromValue(schema: fieldSchema, value: child.value, out: &out)
            }
        }
    default:
        break
    }
}

private func bindValue(
    schema: BindingSchema,
    value: Any,
    allocator: ChannelIdAllocator,
    incomingRegistry: ChannelRegistry,
    taskSender: @escaping TaskSender,
    serializers: any BindingSerializers
) async {
    switch schema {
    case .rx(let initialCredit, _):
        // Schema Rx = client passes Rx to method, sends via paired Tx
        // Need to bind Tx for outgoing
        // The value is the Rx; find its paired Tx
        if let rx = value as? AnyUnboundRx {
            let channelId = allocator.allocate()
            let credit = await incomingRegistry.registerOutgoing(
                channelId, initialCredit: initialCredit)
            rx.bindForSchema(channelId: channelId, taskSender: taskSender, credit: credit)
        }

    case .tx(let initialCredit, _):
        // Schema Tx = client passes Tx to method, receives via paired Rx
        // Need to bind Rx for incoming
        if let tx = value as? AnyUnboundTx {
            let channelId = allocator.allocate()
            traceLog(.driver, "bindChannels: registering incoming channelId=\(channelId)")
            let receiver = await incomingRegistry.register(
                channelId,
                initialCredit: initialCredit,
                onConsumed: { additional in
                    taskSender(.grantCredit(channelId: channelId, bytes: additional))
                }
            )
            tx.bindForSchema(channelId: channelId, receiver: receiver)
        }

    case .vec(let element):
        if let arr = value as? [Any] {
            for item in arr {
                await bindValue(
                    schema: element,
                    value: item,
                    allocator: allocator,
                    incomingRegistry: incomingRegistry,
                    taskSender: taskSender,
                    serializers: serializers
                )
            }
        }

    case .option(let inner):
        // Use Mirror to check if value is Some(x) vs None
        let mirror = Mirror(reflecting: value)
        if mirror.displayStyle == .optional, let (_, unwrapped) = mirror.children.first {
            await bindValue(
                schema: inner,
                value: unwrapped,
                allocator: allocator,
                incomingRegistry: incomingRegistry,
                taskSender: taskSender,
                serializers: serializers
            )
        }

    case .struct(let fields):
        // Use Mirror for struct field access
        let mirror = Mirror(reflecting: value)
        for (fieldName, fieldSchema) in fields {
            if let child = mirror.children.first(where: { $0.label == fieldName }) {
                await bindValue(
                    schema: fieldSchema,
                    value: child.value,
                    allocator: allocator,
                    incomingRegistry: incomingRegistry,
                    taskSender: taskSender,
                    serializers: serializers
                )
            }
        }

    default:
        // Primitives and other types - no channels to bind
        break
    }
}

private func finalizeValue(schema: BindingSchema, value: Any) {
    switch schema {
    case .rx:
        (value as? AnyRetryFinalizableChannel)?.finishRetryBinding()
    case .tx:
        (value as? AnyRetryFinalizableChannel)?.finishRetryBinding()
    case .vec(let element):
        if let arr = value as? [Any] {
            for item in arr {
                finalizeValue(schema: element, value: item)
            }
        }
    case .option(let inner):
        let mirror = Mirror(reflecting: value)
        if mirror.displayStyle == .optional, let (_, unwrapped) = mirror.children.first {
            finalizeValue(schema: inner, value: unwrapped)
        }
    case .struct(let fields):
        let mirror = Mirror(reflecting: value)
        for (fieldName, fieldSchema) in fields {
            if let child = mirror.children.first(where: { $0.label == fieldName }) {
                finalizeValue(schema: fieldSchema, value: child.value)
            }
        }
    default:
        break
    }
}

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

protocol AnyRetryFinalizableChannel: AnyObject {
    func finishRetryBinding()
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

extension UnboundTx: AnyRetryFinalizableChannel {}

protocol AnyUnboundRxReceiver: AnyObject {
    func bindForReceiving(channelId: ChannelId, receiver: ChannelReceiver)
}

extension UnboundRx: AnyUnboundRxReceiver {
    func bindForReceiving(channelId: ChannelId, receiver: ChannelReceiver) {
        self.bind(channelId: channelId, receiver: receiver)
    }
}

extension UnboundRx: AnyRetryFinalizableChannel {}
