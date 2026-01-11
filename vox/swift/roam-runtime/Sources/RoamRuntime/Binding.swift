import Foundation

// MARK: - Unbound Channel Types

/// Unbound Tx - created by `channel()`, bound at call time.
public final class UnboundTx<T: Sendable>: @unchecked Sendable {
    public private(set) var channelId: ChannelId = 0
    private var taskTx: (@Sendable (TaskMessage) -> Void)?
    private let serialize: @Sendable (T) -> [UInt8]
    private var bound = false

    public init(serialize: @escaping @Sendable (T) -> [UInt8]) {
        self.serialize = serialize
    }

    public var isBound: Bool { bound }

    /// Bind for sending (client-side outgoing).
    func bind(channelId: ChannelId, taskTx: @escaping @Sendable (TaskMessage) -> Void) {
        precondition(!bound, "UnboundTx already bound")
        self.channelId = channelId
        self.taskTx = taskTx
        self.bound = true
    }

    /// Set channel ID only (when paired Rx is bound).
    func setChannelIdOnly(channelId: ChannelId) {
        precondition(!bound, "UnboundTx already bound")
        self.channelId = channelId
        self.bound = true
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

/// Unbound Rx - created by `channel()`, bound at call time.
public final class UnboundRx<T: Sendable>: @unchecked Sendable {
    public private(set) var channelId: ChannelId = 0
    private var receiver: ChannelReceiver?
    private let deserialize: @Sendable ([UInt8]) throws -> T
    private var bound = false

    // Weak reference to paired Tx
    weak var pairedTx: AnyObject?

    public init(deserialize: @escaping @Sendable ([UInt8]) throws -> T) {
        self.deserialize = deserialize
    }

    public var isBound: Bool { bound }

    /// Bind for receiving (client-side incoming).
    func bind(channelId: ChannelId, receiver: ChannelReceiver) {
        precondition(!bound, "UnboundRx already bound")
        self.channelId = channelId
        self.receiver = receiver
        self.bound = true
    }

    /// Set channel ID only (when paired Tx is bound).
    func setChannelIdOnly(channelId: ChannelId) {
        precondition(!bound, "UnboundRx already bound")
        self.channelId = channelId
        self.bound = true
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
    serialize: @escaping @Sendable (T) -> [UInt8],
    deserialize: @escaping @Sendable ([UInt8]) throws -> T
) -> (UnboundTx<T>, UnboundRx<T>) {
    let tx = UnboundTx<T>(serialize: serialize)
    let rx = UnboundRx<T>(deserialize: deserialize)
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
    schemas: [Schema],
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

private func bindValue(
    schema: Schema,
    value: Any,
    allocator: ChannelIdAllocator,
    incomingRegistry: ChannelRegistry,
    taskSender: @escaping TaskSender,
    serializers: any BindingSerializers
) async {
    switch schema {
    case .rx:
        // Schema Rx = client passes Rx to method, sends via paired Tx
        // Need to bind Tx for outgoing
        // The value is the Rx; find its paired Tx
        if let rx = value as? AnyUnboundRx {
            let channelId = allocator.allocate()
            rx.bindForSchema(channelId: channelId, taskSender: taskSender)
        }

    case .tx:
        // Schema Tx = client passes Tx to method, receives via paired Rx
        // Need to bind Rx for incoming
        if let tx = value as? AnyUnboundTx {
            let channelId = allocator.allocate()
            let receiver = await incomingRegistry.register(channelId)
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

// MARK: - Type Erasure for Binding

/// Protocol for type-erased UnboundRx binding.
protocol AnyUnboundRx: AnyObject {
    func bindForSchema(channelId: ChannelId, taskSender: @escaping TaskSender)
}

/// Protocol for type-erased UnboundTx binding.
protocol AnyUnboundTx: AnyObject {
    func bindForSchema(channelId: ChannelId, receiver: ChannelReceiver)
}

extension UnboundRx: AnyUnboundRx {
    func bindForSchema(channelId: ChannelId, taskSender: @escaping TaskSender) {
        // Schema Rx = client sends via Tx, so bind the paired Tx
        if let pairedTx = self.pairedTx as? AnyUnboundTxSender {
            pairedTx.bindForSending(channelId: channelId, taskSender: taskSender)
        }
        self.setChannelIdOnly(channelId: channelId)
    }
}

extension UnboundTx: AnyUnboundTx {
    func bindForSchema(channelId: ChannelId, receiver: ChannelReceiver) {
        // Schema Tx = client receives via Rx, so this Tx just gets ID
        self.setChannelIdOnly(channelId: channelId)
        // The Rx would be paired but we don't have reference here
        // Client needs to track the Rx separately
    }
}

/// Protocol for sending via Tx.
protocol AnyUnboundTxSender: AnyObject {
    func bindForSending(channelId: ChannelId, taskSender: @escaping TaskSender)
}

extension UnboundTx: AnyUnboundTxSender {
    func bindForSending(channelId: ChannelId, taskSender: @escaping TaskSender) {
        self.bind(channelId: channelId, taskTx: taskSender)
    }
}
