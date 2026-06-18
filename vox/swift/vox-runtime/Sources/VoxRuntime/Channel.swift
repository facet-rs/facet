import Foundation
@preconcurrency import NIOCore

// MARK: - Channel ID

public typealias ChannelId = UInt64

// MARK: - Role

/// Connection role - determines channel ID parity.
public enum Role: Equatable, Sendable {
    case initiator  // Uses odd IDs (1, 3, 5, ...)
    case acceptor  // Uses even IDs (2, 4, 6, ...)
}

func roleForParity(_ parity: Parity) -> Role {
    switch parity {
    case .odd:
        return .initiator
    case .even:
        return .acceptor
    }
}

func oppositeRole(_ role: Role) -> Role {
    switch role {
    case .initiator:
        return .acceptor
    case .acceptor:
        return .initiator
    }
}

func firstId(for role: Role) -> UInt64 {
    switch role {
    case .initiator:
        return 1
    case .acceptor:
        return 2
    }
}

func idMatchesRole(_ id: UInt64, _ role: Role) -> Bool {
    id != 0 && id % 2 == firstId(for: role) % 2
}

// MARK: - Channel ID Allocator

/// Allocates unique channel IDs with correct parity.
///
/// r[impl connection.lane-id-parity] - Initiator uses odd, Acceptor uses even.
/// r[impl rpc.channel.allocation] - Caller allocates ALL channel IDs.
public final class ChannelIdAllocator: @unchecked Sendable {
    private var next: UInt64
    private let lock = NSLock()

    public init(role: Role) {
        next = firstId(for: role)
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
    case grantCredit(channelId: ChannelId, bytes: UInt32)
    case schema(methodId: UInt64, direction: SchemaBindingDirection, schemas: [UInt8])
    case response(
        requestId: UInt64,
        payload: [UInt8],
        methodId: UInt64? = nil,
        // The method's FULL response schema closure (not a pre-resolved advertisement).
        // The driver advertises it idempotently at the sequential send point, so the
        // first response actually written for a method carries the schema — concurrent
        // dispatch tasks must not decide who carries it (the responses can be written
        // in a different order). r[impl schema.exchange.required]
        responseSchemaClosure: [UInt8] = []
    )
}

actor ChannelCreditController {
    private var available: UInt32
    private var closed = false
    private var waiters: [CheckedContinuation<Void, Error>] = []

    init(initialCredit: UInt32) {
        // r[impl rpc.flow-control.credit.initial]
        self.available = initialCredit
    }

    // r[impl rpc.flow-control.credit]
    // r[impl rpc.flow-control.credit.exhaustion]
    func consume() async throws {
        if available > 0 {
            available -= 1
            return
        }
        if closed {
            throw ChannelError.closed
        }
        try await withCheckedThrowingContinuation { continuation in
            waiters.append(continuation)
        }
    }

    func tryConsume() -> CreditConsumeResult {
        if closed {
            return .closed
        }
        if available == 0 {
            return .full
        }
        available -= 1
        return .consumed
    }

    func debugSnapshot() -> (available: UInt32, waiterCount: Int, closed: Bool) {
        (available: available, waiterCount: waiters.count, closed: closed)
    }

    // r[impl rpc.flow-control.credit.grant.additive]
    func grant(_ additional: UInt32) {
        guard additional > 0 else {
            return
        }
        var remaining = additional
        while remaining > 0, !waiters.isEmpty {
            let waiter = waiters.removeFirst()
            waiter.resume()
            remaining -= 1
        }
        if remaining > 0 {
            available &+= remaining
        }
    }

    func close() {
        closed = true
        let waiters = waiters
        self.waiters.removeAll()
        for waiter in waiters {
            waiter.resume(throwing: ChannelError.closed)
        }
    }
}

enum CreditConsumeResult: Sendable {
    case consumed
    case full
    case closed
}

// MARK: - Channel Receiver

/// Receives data on a channel.
public final class ChannelReceiver: @unchecked Sendable {
    private let lock = NSLock()
    private var buffer: [[UInt8]] = []
    private var closed = false
    private var terminalError: ChannelError?
    private var waiter: CheckedContinuation<[UInt8]?, Error>?
    private let replenishmentThreshold: UInt32
    private let onConsumed: (@Sendable (UInt32) -> Void)?
    private let onObservedConsume: (@Sendable () -> Void)?
    private var consumedSinceGrant: UInt32 = 0

    public init(
        replenishmentThreshold: UInt32 = 0,
        onConsumed: (@Sendable (UInt32) -> Void)? = nil,
        onObservedConsume: (@Sendable () -> Void)? = nil
    ) {
        self.replenishmentThreshold = replenishmentThreshold
        self.onConsumed = onConsumed
        self.onObservedConsume = onObservedConsume
    }

    public func deliver(_ data: [UInt8]) {
        // r[impl rpc.channel.delivery.reliable]
        var toResume: CheckedContinuation<[UInt8]?, Error>?
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
        var toResume: CheckedContinuation<[UInt8]?, Error>?
        lock.lock()
        closed = true
        terminalError = nil
        if let w = waiter {
            waiter = nil
            toResume = w
        }
        lock.unlock()
        toResume?.resume(returning: nil)
    }

    /// Handle reset - drain accepted buffered data, then surface a receive error.
    public func deliverReset(_ error: ChannelError = .reset) {
        var toResume: CheckedContinuation<[UInt8]?, Error>?
        lock.lock()
        closed = true
        terminalError = error
        if let w = waiter {
            waiter = nil
            toResume = w
        }
        lock.unlock()
        toResume?.resume(throwing: error)
    }

    public func recv() async throws -> [UInt8]? {
        enum RecvState {
            case value([UInt8])
            case closed(ChannelError?)
            case wait
        }
        let state = lock.withLock { () -> RecvState in
            if !buffer.isEmpty {
                return .value(buffer.removeFirst())
            }
            if closed {
                return .closed(terminalError)
            }
            return .wait
        }
        let value: [UInt8]?
        switch state {
        case .value(let value):
            self.noteConsumptionIfNeeded()
            return value
        case .closed(let error):
            if let error {
                throw error
            }
            return nil
        case .wait:
            break
        }
        value = try await withCheckedThrowingContinuation { cont in
            lock.withLock {
                if !buffer.isEmpty {
                    let value = buffer.removeFirst()
                    cont.resume(returning: value)
                    return
                }
                if closed {
                    if let terminalError {
                        cont.resume(throwing: terminalError)
                    } else {
                        cont.resume(returning: nil)
                    }
                    return
                }
                waiter = cont
            }
        }
        if value != nil {
            self.noteConsumptionIfNeeded()
        }
        return value
    }

    private func noteConsumptionIfNeeded() {
        onObservedConsume?()

        guard replenishmentThreshold > 0, let onConsumed else {
            return
        }

        let additional: UInt32? = lock.withLock {
            consumedSinceGrant &+= 1
            guard consumedSinceGrant >= replenishmentThreshold else {
                return nil
            }
            let additional = consumedSinceGrant
            consumedSinceGrant = 0
            return additional
        }

        if let additional {
            onConsumed(additional)
        }
    }

    func debugSnapshot() -> (bufferedItemCount: Int, closed: Bool, terminalError: ChannelError?) {
        lock.withLock {
            (
                bufferedItemCount: buffer.count,
                closed: closed,
                terminalError: terminalError
            )
        }
    }
}

extension NSLock {
    fileprivate func withLock<T>(_ body: () throws -> T) rethrows -> T {
        lock()
        defer { unlock() }
        return try body()
    }
}

// MARK: - Tx (Send Handle)

/// Handle for sending data on a channel.
///
/// r[impl rpc.channel]
/// r[impl rpc.channel.direction]
/// r[impl rpc.channel.lifecycle]
public final class Tx<T: Sendable>: @unchecked Sendable {
    public var channelId: ChannelId = 0
    private var taskTx: (@Sendable (TaskMessage) -> Void)?
    private var credit: ChannelCreditController?
    private let serialize: @Sendable (T, inout ByteBuffer) -> Void
    private let lock = NSLock()
    private var closed = false

    public init(serialize: @escaping @Sendable (T, inout ByteBuffer) -> Void) {
        self.serialize = serialize
    }

    /// Bind this Tx for sending (server-side).
    func bind(
        channelId: ChannelId,
        taskTx: @escaping @Sendable (TaskMessage) -> Void,
        credit: ChannelCreditController
    ) {
        self.channelId = channelId
        self.taskTx = taskTx
        self.credit = credit
    }

    /// Send a value.
    ///
    /// r[impl rpc.channel.item] - Data messages carry serialized values.
    /// r[impl rpc.flow-control.credit]
    public func send(_ value: T) async throws {
        guard let taskTx = taskTx, let credit else {
            throw ChannelError.notBound
        }
        if lock.withLock({ closed }) {
            throw ChannelError.closed
        }
        try await credit.consume()
        var buf = ByteBufferAllocator().buffer(capacity: 64)
        serialize(value, &buf)
        let bytes = buf.readBytes(length: buf.readableBytes) ?? []
        taskTx(.data(channelId: channelId, payload: bytes))
    }

    // r[impl rpc.flow-control.credit.try-send]
    // r[impl rpc.observability.channel.try-send-detail]
    public func trySend(_ value: T) async throws -> TrySendResult<T> {
        guard let taskTx = taskTx, let credit else {
            observeChannel(
                VoxChannelObserverEvent(
                    kind: .trySend,
                    direction: .outgoing,
                    trySendDetail: .unbound
                ))
            return .full(value)
        }
        if lock.withLock({ closed }) {
            observeChannel(
                VoxChannelObserverEvent(
                    kind: .trySend,
                    channelId: channelId,
                    direction: .outgoing,
                    trySendDetail: .closed
                ))
            return .closed(value)
        }
        switch await credit.tryConsume() {
        case .consumed:
            var buf = ByteBufferAllocator().buffer(capacity: 64)
            serialize(value, &buf)
            let bytes = buf.readBytes(length: buf.readableBytes) ?? []
            taskTx(.data(channelId: channelId, payload: bytes))
            observeChannel(
                VoxChannelObserverEvent(
                    kind: .trySend,
                    channelId: channelId,
                    direction: .outgoing,
                    bytes: bytes.count,
                    trySendDetail: .sent
                ))
            return .sent
        case .full:
            observeChannel(
                VoxChannelObserverEvent(
                    kind: .trySend,
                    channelId: channelId,
                    direction: .outgoing,
                    trySendDetail: .creditExhausted
                ))
            return .full(value)
        case .closed:
            observeChannel(
                VoxChannelObserverEvent(
                    kind: .trySend,
                    channelId: channelId,
                    direction: .outgoing,
                    trySendDetail: .closed
                ))
            return .closed(value)
        }
    }

    /// Close this channel.
    ///
    /// r[impl rpc.channel.close] - Close terminates the channel.
    /// r[impl rpc.channel.lifecycle] - Caller sends Close when done.
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

    deinit {
        close()
    }
}

public enum TrySendResult<T: Sendable>: Sendable {
    case sent
    case full(T)
    case closed(T)
}

// MARK: - Rx (Receive Handle)

/// Handle for receiving data on a channel.
///
/// r[impl rpc.channel]
/// r[impl rpc.channel.direction]
/// r[impl rpc.channel.lifecycle]
public final class Rx<T: Sendable>: @unchecked Sendable {
    public var channelId: ChannelId = 0
    private var receiver: ChannelReceiver?
    private let deserialize: @Sendable (inout ByteBuffer) throws -> T

    public init(deserialize: @escaping @Sendable (inout ByteBuffer) throws -> T) {
        self.deserialize = deserialize
    }

    /// Bind this Rx for receiving (server-side).
    public func bind(channelId: ChannelId, receiver: ChannelReceiver) {
        self.channelId = channelId
        self.receiver = receiver
    }

    /// Receive the next value, or nil after a graceful channel close.
    public func recv() async throws -> T? {
        guard let receiver = receiver else {
            throw ChannelError.notBound
        }
        guard let bytes = try await receiver.recv() else {
            return nil
        }
        var buf = ByteBufferAllocator().buffer(capacity: bytes.count)
        buf.writeBytes(bytes)
        return try deserialize(&buf)
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
/// r[impl rpc.channel.item] - Data messages routed by channel_id; items for a
/// not-yet-bound channel are buffered and drained when the channel binds.
/// r[impl rpc.channel.lifecycle] - Channels are request-scoped sidebands.
/// r[impl rpc.channel.lifecycle] - Response delivery terminates associated channels.
public actor ChannelRegistry {
    private var receivers: [ChannelId: ChannelReceiver] = [:]
    private var pendingData: [ChannelId: [[UInt8]]] = [:]
    private var pendingTerminal: [ChannelId: ChannelTerminal] = [:]
    private var knownChannels: Set<ChannelId> = []
    private var outgoingCredits: [ChannelId: ChannelCreditController] = [:]
    private var contexts: [ChannelId: VoxChannelDebugContext] = [:]

    public init() {}

    /// Mark a channel as known (before dispatch task runs).
    public func markKnown(_ channelId: ChannelId) {
        knownChannels.insert(channelId)
    }

    // r[impl rpc.observability.channel.context]
    public func rememberContext(
        _ channelId: ChannelId,
        _ context: VoxChannelDebugContext
    ) {
        if let existing = contexts[channelId] {
            contexts[channelId] = existing.merged(with: context)
        } else {
            contexts[channelId] = context
        }
    }

    public func context(for channelId: ChannelId) -> VoxChannelDebugContext? {
        contexts[channelId]
    }

    /// Register a channel and return its receiver.
    /// This is async to ensure pending data/close are delivered synchronously
    /// before returning, avoiding race conditions with the handler.
    /// r[impl rpc.channel.binding.callee-args]
    /// r[impl rpc.channel.binding.callee-args.rx]
    /// r[impl rpc.flow-control.credit.initial]
    /// r[impl rpc.observability.channel]
    public func register(
        _ channelId: ChannelId,
        initialCredit: UInt32,
        onConsumed: (@Sendable (UInt32) -> Void)? = nil
    ) async -> ChannelReceiver {
        let receiver = ChannelReceiver(
            replenishmentThreshold: Swift.max(UInt32(1), initialCredit / 2),
            onConsumed: onConsumed,
            onObservedConsume: { [weak self] in
                Task {
                    await self?.observe(
                        kind: .consume,
                        channelId: channelId,
                        direction: .incoming
                    )
                }
            }
        )
        receivers[channelId] = receiver
        knownChannels.insert(channelId)
        observe(kind: .open, channelId: channelId, direction: .incoming)

        // Deliver pending data synchronously - no Task spawning!
        if let pending = pendingData.removeValue(forKey: channelId) {
            for data in pending {
                receiver.deliver(data)
            }
        }

        // Deliver pending terminal state synchronously after all accepted data.
        if let terminal = pendingTerminal.removeValue(forKey: channelId) {
            terminal.deliver(to: receiver)
            receivers.removeValue(forKey: channelId)
            knownChannels.remove(channelId)
        }

        return receiver
    }

    func registerOutgoing(_ channelId: ChannelId, initialCredit: UInt32) -> ChannelCreditController
    {
        // r[impl rpc.channel.binding.callee-args]
        // r[impl rpc.channel.binding.callee-args.tx]
        // r[impl rpc.flow-control.credit.initial]
        // r[impl rpc.observability.channel]
        let controller = ChannelCreditController(initialCredit: initialCredit)
        outgoingCredits[channelId] = controller
        knownChannels.insert(channelId)
        observe(kind: .open, channelId: channelId, direction: .outgoing)
        return controller
    }

    /// Deliver data to a channel. Returns true if known.
    ///
    /// r[impl rpc.channel.close] - Data after close is rejected.
    /// r[impl rpc.observability.channel]
    public func deliverData(channelId: ChannelId, payload: [UInt8]) async -> Bool {
        if pendingTerminal[channelId] != nil {
            return false
        }
        if let receiver = receivers[channelId] {
            receiver.deliver(payload)
            observe(
                kind: .receive,
                channelId: channelId,
                direction: .incoming,
                bytes: payload.count
            )
            return true
        }
        if knownChannels.contains(channelId) {
            pendingData[channelId, default: []].append(payload)
            observe(
                kind: .receive,
                channelId: channelId,
                direction: .incoming,
                bytes: payload.count
            )
            return true
        }
        return false
    }

    /// Deliver close to a channel. Returns true if known.
    /// r[impl rpc.observability.channel]
    public func deliverClose(channelId: ChannelId) async -> Bool {
        if let receiver = receivers[channelId] {
            receiver.deliverClose()
            receivers.removeValue(forKey: channelId)
            pendingTerminal[channelId] = .close
            if let credit = outgoingCredits[channelId] {
                await credit.close()
            }
            outgoingCredits.removeValue(forKey: channelId)
            observe(kind: .close, channelId: channelId, direction: .incoming)
            return true
        }
        if knownChannels.contains(channelId) {
            pendingTerminal[channelId] = .close
            if let credit = outgoingCredits[channelId] {
                await credit.close()
            }
            outgoingCredits.removeValue(forKey: channelId)
            observe(kind: .close, channelId: channelId, direction: .incoming)
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
    /// r[impl rpc.observability.channel]
    public func deliverReset(channelId: ChannelId, error: ChannelError = .reset) async {
        if let receiver = receivers[channelId] {
            receiver.deliverReset(error)
            receivers.removeValue(forKey: channelId)
        } else if knownChannels.contains(channelId) {
            pendingTerminal[channelId] = .error(error)
        }
        if let credit = outgoingCredits[channelId] {
            await credit.close()
        }
        outgoingCredits.removeValue(forKey: channelId)
        observe(
            kind: .reset,
            channelId: channelId,
            direction: .incoming,
            error: String(describing: error)
        )
    }

    /// Deliver credit to a channel.
    ///
    /// r[impl rpc.flow-control.credit.grant] - Credit message grants permission.
    /// r[impl rpc.observability.channel]
    public func deliverCredit(channelId: ChannelId, bytes: UInt32) async {
        if let credit = outgoingCredits[channelId] {
            await credit.grant(bytes)
            observe(
                kind: .credit,
                channelId: channelId,
                direction: .incoming,
                additionalCredit: bytes
            )
        }
    }

    /// Close all channels when the connection closes.
    public func closeAllChannels() async {
        // r[impl rpc.channel.connection-closure]
        for (_, receiver) in receivers {
            receiver.deliverReset(.connectionClosed)
        }
        receivers.removeAll()
        for (_, credit) in outgoingCredits {
            await credit.close()
        }
        outgoingCredits.removeAll()
        knownChannels.removeAll()
        pendingData.removeAll()
        pendingTerminal.removeAll()
        contexts.removeAll()
    }

    // r[impl rpc.debug.snapshot]
    // r[impl rpc.observability.channel.context]
    public func debugSnapshot(laneId: UInt64? = nil) async -> VoxChannelRegistrySnapshot {
        var ids = knownChannels
        ids.formUnion(receivers.keys)
        ids.formUnion(pendingData.keys)
        ids.formUnion(pendingTerminal.keys)
        ids.formUnion(outgoingCredits.keys)
        ids.formUnion(contexts.keys)

        var snapshots: [VoxChannelSnapshot] = []
        for channelId in ids.sorted() {
            let receiverSnapshot = receivers[channelId]?.debugSnapshot()
            let creditSnapshot = await outgoingCredits[channelId]?.debugSnapshot()
            let state: VoxChannelSnapshotState
            if let terminal = pendingTerminal[channelId] {
                switch terminal {
                case .close:
                    state = .closed
                case .error:
                    state = .reset
                }
            } else if receiverSnapshot != nil, creditSnapshot != nil {
                state = .bidirectional
            } else if receiverSnapshot != nil {
                state = .incoming
            } else if creditSnapshot != nil {
                state = .outgoing
            } else {
                state = .known
            }

            let context = contexts[channelId].map { existing in
                guard let laneId else {
                    return existing
                }
                return existing.merged(with: VoxChannelDebugContext(laneId: laneId))
            } ?? laneId.map { VoxChannelDebugContext(laneId: $0) }

            snapshots.append(
                VoxChannelSnapshot(
                    channelId: channelId,
                    state: state,
                    context: context,
                    bufferedItemCount: receiverSnapshot?.bufferedItemCount
                        ?? pendingData[channelId]?.count
                        ?? 0,
                    outgoingCreditAvailable: creditSnapshot?.available,
                    outgoingCreditWaiterCount: creditSnapshot?.waiterCount
                ))
        }
        return VoxChannelRegistrySnapshot(channels: snapshots)
    }

    private func observe(
        kind: VoxChannelObserverKind,
        channelId: ChannelId,
        direction: VoxChannelDirection? = nil,
        bytes: Int? = nil,
        additionalCredit: UInt32? = nil,
        trySendDetail: VoxChannelTrySendDetail? = nil,
        error: String? = nil
    ) {
        observeChannel(
            VoxChannelObserverEvent(
                kind: kind,
                channelId: channelId,
                direction: direction,
                bytes: bytes,
                additionalCredit: additionalCredit,
                trySendDetail: trySendDetail,
                context: contexts[channelId],
                error: error
            ))
    }
}

// MARK: - Errors

private enum ChannelTerminal {
    case close
    case error(ChannelError)

    func deliver(to receiver: ChannelReceiver) {
        switch self {
        case .close:
            receiver.deliverClose()
        case .error(let error):
            receiver.deliverReset(error)
        }
    }
}

public enum ChannelError: Error, Equatable {
    case notBound
    case closed
    case reset
    case requestClosed
    case cancelled
    case timedOut
    case connectionClosed
    case unknown
}
