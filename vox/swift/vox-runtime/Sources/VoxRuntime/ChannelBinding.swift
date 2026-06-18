import Foundation
@preconcurrency import NIOCore

// Out-of-band channel binding (mirrors TS `channeling/binding.ts` + driver.ts).
//
// `Tx`/`Rx` arguments are opaque on the wire: each encodes only a u32 wire index
// into `RequestCall.channels`, and the allocated channel ids travel out-of-band in
// that list. The caller allocates ids and binds the *paired* handle (the one it
// keeps); the callee resolves each wire index to a channel id and binds a local
// `Tx`/`Rx`. Per-item bytes use the channel's own element codec (the caller supplies
// it via `channel(...)`; the generated server uses the matching element codec).
// r[impl schema.interaction.channels]
// r[impl rpc.channel.binding]
// r[impl rpc.channel.payload-encoding]

/// The initial channel credit window (in items) both peers advertise at handshake
/// (`ConnectionEstablishment.swift` `initialChannelCredit`). Credit is per-item: a sender
/// may send this many items before a grant, and the receiver re-grants as it consumes
/// (replenishment threshold = window/2). This MUST match the advertised window — using
/// a larger value here starves the sender (the receiver would only re-grant after
/// window/2 ≫ advertised items, deadlocking mid-stream).
/// r[impl rpc.flow-control.credit.initial]
public let defaultInitialChannelCredit: UInt32 = 16

/// The 4-byte little-endian phon-compact encoding of a u32 wire index.
/// r[impl rpc.channel.payload-encoding]
public func channelWireIndexBytes(_ index: Int) -> [UInt8] {
    let v = UInt32(index).littleEndian
    return withUnsafeBytes(of: v) { Array($0) }
}

/// Read a u32 LE wire index from a channel arg's decoded bytes.
public func channelWireIndex(_ data: Data) -> Int {
    var v: UInt32 = 0
    for (i, byte) in data.prefix(4).enumerated() {
        v |= UInt32(byte) << (8 * UInt32(i))
    }
    return Int(v)
}

// MARK: - Client-side binding (the caller keeps the paired handle)

extension VoxLane {
    /// Bind an `Rx<T>` argument: the method wants an `Rx` (callee receives), so the
    /// caller passed an `Rx` and keeps the paired `Tx` — the caller SENDS. Inject the
    /// phon typed encode codec into the paired `Tx`, allocate a channel id, bind the
    /// paired `Tx` for outgoing, and return the id.
    /// r[impl rpc.channel.binding.caller-args]
    /// r[impl rpc.channel.binding.caller-args.rx]
    /// r[impl rpc.channel.pair.binding-propagation]
    public func bindClientRxArg<T>(
        _ rx: UnboundRx<T>,
        serialize: @escaping @Sendable (T, inout ByteBuffer) -> Void
    ) async -> UInt64 {
        (rx.pairedTx as? UnboundTx<T>)?.setSerialize(serialize)
        let channelId = channelAllocator.allocate()
        let credit = await incomingChannelRegistry.registerOutgoing(
            channelId, initialCredit: defaultInitialChannelCredit)
        rx.bindForSchema(channelId: channelId, taskSender: taskSender, credit: credit)
        return channelId
    }

    /// Bind a `Tx<T>` argument: the method wants a `Tx` (callee sends), so the caller
    /// passed a `Tx` and keeps the paired `Rx` — the caller RECEIVES. Inject the phon
    /// typed decode codec into the paired `Rx`, allocate a channel id, register an
    /// incoming receiver, bind the paired `Rx`, and return the id.
    /// r[impl rpc.channel.binding.caller-args]
    /// r[impl rpc.channel.binding.caller-args.tx]
    /// r[impl rpc.channel.pair.binding-propagation]
    public func bindClientTxArg<T>(
        _ tx: UnboundTx<T>,
        deserialize: @escaping @Sendable (inout ByteBuffer) throws -> T
    ) async -> UInt64 {
        (tx.pairedRx as? UnboundRx<T>)?.setDeserialize(deserialize)
        let channelId = channelAllocator.allocate()
        let sender = taskSender
        let receiver = await incomingChannelRegistry.register(
            channelId, initialCredit: defaultInitialChannelCredit,
            onConsumed: { additional in
                sender(.grantCredit(channelId: channelId, bytes: additional))
            })
        tx.bindForSchema(channelId: channelId, receiver: receiver)
        return channelId
    }
}

/// Finalize a bound channel handle when its call settles (mirrors TS `finalize`):
/// closes or finalizes the paired call-bound endpoint so the receive loop can
/// terminate once no further items can arrive for that call binding.
public func finalizeChannel<T>(_ tx: UnboundTx<T>) { tx.finishCallBinding() }
public func finalizeChannel<T>(_ rx: UnboundRx<T>) { rx.finishCallBinding() }

// MARK: - Server-side binding (the dispatcher creates the local handle)

private final class ChannelSchemaTaskSender: @unchecked Sendable {
    private let methodId: UInt64
    private let argsSchemaClosure: [UInt8]
    private let schemaSendTracker: SchemaSendTracker
    private let taskTx: @Sendable (TaskMessage) -> Void
    private let lock = NSLock()
    private var advertised = false

    init(
        methodId: UInt64,
        argsSchemaClosure: [UInt8],
        schemaSendTracker: SchemaSendTracker,
        taskTx: @escaping @Sendable (TaskMessage) -> Void
    ) {
        self.methodId = methodId
        self.argsSchemaClosure = argsSchemaClosure
        self.schemaSendTracker = schemaSendTracker
        self.taskTx = taskTx
    }

    func send(_ message: TaskMessage) {
        if case .data = message {
            advertiseOnce()
        }
        taskTx(message)
    }

    private func advertiseOnce() {
        let shouldAdvertise: Bool
        lock.lock()
        if advertised {
            shouldAdvertise = false
        } else {
            advertised = true
            shouldAdvertise = true
        }
        lock.unlock()
        guard shouldAdvertise else { return }
        // r[impl schema.exchange.channels.tx-args]
        let schemas = schemaSendTracker.prepareSchemas(methodId, .args, argsSchemaClosure)
        if !schemas.isEmpty {
            taskTx(.schema(methodId: methodId, direction: .args, schemas: schemas))
        }
    }
}

private func schemaAdvertisingTaskSender(
    methodId: UInt64,
    argsSchemaClosure: [UInt8],
    schemaSendTracker: SchemaSendTracker,
    taskTx: @escaping @Sendable (TaskMessage) -> Void
) -> @Sendable (TaskMessage) -> Void {
    let sender = ChannelSchemaTaskSender(
        methodId: methodId,
        argsSchemaClosure: argsSchemaClosure,
        schemaSendTracker: schemaSendTracker,
        taskTx: taskTx
    )
    return { message in sender.send(message) }
}

/// Bind a server `Rx<T>` (the handler RECEIVES). Registers an incoming receiver on the
/// server registry (so buffered/early data is delivered) and returns a bound `Rx`.
/// r[impl rpc.channel.binding.callee-args]
/// r[impl rpc.channel.binding.callee-args.rx]
public func bindServerRx<T: Sendable>(
    channelId: UInt64,
    registry: ChannelRegistry,
    taskTx: @escaping @Sendable (TaskMessage) -> Void,
    deserialize: @escaping @Sendable (inout ByteBuffer) throws -> T
) async -> Rx<T> {
    let receiver = await registry.register(
        channelId, initialCredit: defaultInitialChannelCredit,
        onConsumed: { additional in
            taskTx(.grantCredit(channelId: channelId, bytes: additional))
        })
    let rx = Rx<T>(deserialize: deserialize)
    rx.bind(channelId: channelId, receiver: receiver)
    return rx
}

/// Bind a server `Tx<T>` (the handler SENDS). Registers an outgoing credit controller
/// on the server registry and returns a bound `Tx`.
/// r[impl rpc.channel.binding.callee-args]
/// r[impl rpc.channel.binding.callee-args.tx]
public func bindServerTx<T: Sendable>(
    channelId: UInt64,
    registry: ChannelRegistry,
    taskTx: @escaping @Sendable (TaskMessage) -> Void,
    methodId: UInt64? = nil,
    argsSchemaClosure: [UInt8] = [],
    schemaSendTracker: SchemaSendTracker? = nil,
    serialize: @escaping @Sendable (T, inout ByteBuffer) -> Void
) async -> Tx<T> {
    let credit = await registry.registerOutgoing(
        channelId, initialCredit: defaultInitialChannelCredit)
    let txTaskSender: @Sendable (TaskMessage) -> Void
    if let methodId, let schemaSendTracker, !argsSchemaClosure.isEmpty {
        txTaskSender = schemaAdvertisingTaskSender(
            methodId: methodId,
            argsSchemaClosure: argsSchemaClosure,
            schemaSendTracker: schemaSendTracker,
            taskTx: taskTx
        )
    } else {
        txTaskSender = taskTx
    }
    let tx = Tx<T>(serialize: serialize)
    tx.bind(channelId: channelId, taskTx: txTaskSender, credit: credit)
    return tx
}
