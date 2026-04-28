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

// MARK: - Resolve helpers

/// Resolve a TypeRef to a SchemaKind via the registry.
private func resolveKind(_ typeRef: TypeRef, _ registry: [UInt64: Schema]) -> SchemaKind? {
    guard case .concrete(let typeId, _) = typeRef,
        let schema = registry[typeId]
    else {
        return nil
    }
    return schema.kind
}

// MARK: - Bind Channels

/// Bind channels from method arguments using schema.
///
/// - Parameters:
///   - argsRoot: TypeRef for the args tuple schema (must resolve to `.tuple`).
///   - schemaRegistry: Global schema registry for this service.
///   - args: Array of argument values, one per tuple element.
public func bindChannels(
    argsRoot: TypeRef,
    schemaRegistry: [UInt64: Schema],
    args: [Any],
    allocator: ChannelIdAllocator,
    incomingRegistry: ChannelRegistry,
    taskSender: @escaping TaskSender
) async {
    guard let kind = resolveKind(argsRoot, schemaRegistry),
        case .tuple(let elements) = kind
    else {
        fatalError("argsRoot must resolve to a tuple schema")
    }
    for (typeRef, arg) in zip(elements, args) {
        await bindValue(
            typeRef: typeRef,
            schemaRegistry: schemaRegistry,
            value: arg,
            allocator: allocator,
            incomingRegistry: incomingRegistry,
            taskSender: taskSender
        )
    }
}

/// Finalize bound channels after a call completes.
public func finalizeBoundChannels(
    argsRoot: TypeRef,
    schemaRegistry: [UInt64: Schema],
    args: [Any]
) {
    guard let kind = resolveKind(argsRoot, schemaRegistry),
        case .tuple(let elements) = kind
    else {
        return
    }
    for (typeRef, arg) in zip(elements, args) {
        finalizeValue(typeRef: typeRef, schemaRegistry: schemaRegistry, value: arg)
    }
}

/// Collect bound channel IDs in argument declaration order.
public func collectChannelIds(
    argsRoot: TypeRef,
    schemaRegistry: [UInt64: Schema],
    args: [Any]
) -> [UInt64] {
    var channelIds: [UInt64] = []
    guard let kind = resolveKind(argsRoot, schemaRegistry),
        case .tuple(let elements) = kind
    else {
        return channelIds
    }
    for (typeRef, arg) in zip(elements, args) {
        collectChannelIdsFromValue(
            typeRef: typeRef, schemaRegistry: schemaRegistry, value: arg, out: &channelIds)
    }
    return channelIds
}

private func collectChannelIdsFromValue(
    typeRef: TypeRef,
    schemaRegistry: [UInt64: Schema],
    value: Any,
    out: inout [UInt64]
) {
    guard let kind = resolveKind(typeRef, schemaRegistry) else { return }
    switch kind {
    case .channel:
        if let rx = value as? AnyUnboundRx {
            out.append(rx.channelIdForSchema())
        }
        if let tx = value as? AnyUnboundTx {
            out.append(tx.channelIdForSchema())
        }
    case .list(let element):
        if let arr = value as? [Any] {
            for item in arr {
                collectChannelIdsFromValue(
                    typeRef: element, schemaRegistry: schemaRegistry, value: item, out: &out)
            }
        }
    case .option(let element):
        let mirror = Mirror(reflecting: value)
        if mirror.displayStyle == .optional, let (_, unwrapped) = mirror.children.first {
            collectChannelIdsFromValue(
                typeRef: element, schemaRegistry: schemaRegistry, value: unwrapped, out: &out)
        }
    case .struct(_, let fields):
        let mirror = Mirror(reflecting: value)
        for field in fields {
            if let child = mirror.children.first(where: { $0.label == field.name }) {
                collectChannelIdsFromValue(
                    typeRef: field.typeRef, schemaRegistry: schemaRegistry, value: child.value,
                    out: &out)
            }
        }
    case .tuple(let elements):
        if let arr = value as? [Any] {
            for (element, item) in zip(elements, arr) {
                collectChannelIdsFromValue(
                    typeRef: element, schemaRegistry: schemaRegistry, value: item, out: &out)
            }
        }
    default:
        break
    }
}

private func bindValue(
    typeRef: TypeRef,
    schemaRegistry: [UInt64: Schema],
    value: Any,
    allocator: ChannelIdAllocator,
    incomingRegistry: ChannelRegistry,
    taskSender: @escaping TaskSender
) async {
    guard let kind = resolveKind(typeRef, schemaRegistry) else { return }
    switch kind {
    case .channel(let direction, _):
        if direction == .rx {
            // Schema Rx = client passes Rx to method, sends via paired Tx
            if let rx = value as? AnyUnboundRx {
                let channelId = allocator.allocate()
                let credit = await incomingRegistry.registerOutgoing(
                    channelId, initialCredit: 16)
                rx.bindForSchema(channelId: channelId, taskSender: taskSender, credit: credit)
            }
        } else {
            // Schema Tx = client passes Tx to method, receives via paired Rx
            if let tx = value as? AnyUnboundTx {
                let channelId = allocator.allocate()
                traceLog(.driver, "bindChannels: registering incoming channelId=\(channelId)")
                let receiver = await incomingRegistry.register(
                    channelId,
                    initialCredit: 16,
                    onConsumed: { additional in
                        taskSender(.grantCredit(channelId: channelId, bytes: additional))
                    }
                )
                tx.bindForSchema(channelId: channelId, receiver: receiver)
            }
        }

    case .list(let element):
        if let arr = value as? [Any] {
            for item in arr {
                await bindValue(
                    typeRef: element,
                    schemaRegistry: schemaRegistry,
                    value: item,
                    allocator: allocator,
                    incomingRegistry: incomingRegistry,
                    taskSender: taskSender
                )
            }
        }

    case .option(let element):
        let mirror = Mirror(reflecting: value)
        if mirror.displayStyle == .optional, let (_, unwrapped) = mirror.children.first {
            await bindValue(
                typeRef: element,
                schemaRegistry: schemaRegistry,
                value: unwrapped,
                allocator: allocator,
                incomingRegistry: incomingRegistry,
                taskSender: taskSender
            )
        }

    case .struct(_, let fields):
        let mirror = Mirror(reflecting: value)
        for field in fields {
            if let child = mirror.children.first(where: { $0.label == field.name }) {
                await bindValue(
                    typeRef: field.typeRef,
                    schemaRegistry: schemaRegistry,
                    value: child.value,
                    allocator: allocator,
                    incomingRegistry: incomingRegistry,
                    taskSender: taskSender
                )
            }
        }

    case .tuple(let elements):
        if let arr = value as? [Any] {
            for (element, item) in zip(elements, arr) {
                await bindValue(
                    typeRef: element,
                    schemaRegistry: schemaRegistry,
                    value: item,
                    allocator: allocator,
                    incomingRegistry: incomingRegistry,
                    taskSender: taskSender
                )
            }
        }

    default:
        // Primitives and other types - no channels to bind
        break
    }
}

private func finalizeValue(
    typeRef: TypeRef,
    schemaRegistry: [UInt64: Schema],
    value: Any
) {
    guard let kind = resolveKind(typeRef, schemaRegistry) else { return }
    switch kind {
    case .channel:
        (value as? AnyRetryFinalizableChannel)?.finishRetryBinding()
    case .list(let element):
        if let arr = value as? [Any] {
            for item in arr {
                finalizeValue(typeRef: element, schemaRegistry: schemaRegistry, value: item)
            }
        }
    case .option(let element):
        let mirror = Mirror(reflecting: value)
        if mirror.displayStyle == .optional, let (_, unwrapped) = mirror.children.first {
            finalizeValue(typeRef: element, schemaRegistry: schemaRegistry, value: unwrapped)
        }
    case .struct(_, let fields):
        let mirror = Mirror(reflecting: value)
        for field in fields {
            if let child = mirror.children.first(where: { $0.label == field.name }) {
                finalizeValue(
                    typeRef: field.typeRef, schemaRegistry: schemaRegistry, value: child.value)
            }
        }
    case .tuple(let elements):
        if let arr = value as? [Any] {
            for (element, item) in zip(elements, arr) {
                finalizeValue(typeRef: element, schemaRegistry: schemaRegistry, value: item)
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
