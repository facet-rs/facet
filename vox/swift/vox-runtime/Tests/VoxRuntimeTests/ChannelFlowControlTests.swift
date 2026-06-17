import Foundation
@preconcurrency import NIOCore
import Testing

@testable import VoxRuntime

// Local little-endian i32 codec — this suite tests channel credit/flow-control, not the
// element codec, so it just needs to put/read i32 values in channel payloads.
private enum TestCodecError: Error { case shortRead }
private func encI32(_ v: Int32, into buf: inout ByteBuffer) {
    buf.writeInteger(v, endianness: .little)
}
private func decI32(from buf: inout ByteBuffer) throws -> Int32 {
    guard let v = buf.readInteger(endianness: .little, as: Int32.self) else {
        throw TestCodecError.shortRead
    }
    return v
}

private final class GrantInbox: @unchecked Sendable {
    private let lock = NSLock()
    private var grants: [UInt32] = []

    func append(_ value: UInt32) {
        lock.lock()
        grants.append(value)
        lock.unlock()
    }

    func snapshot() -> [UInt32] {
        lock.lock()
        defer { lock.unlock() }
        return grants
    }
}

private final class PayloadInbox: @unchecked Sendable {
    private let lock = NSLock()
    private var payloads: [[UInt8]] = []

    func append(_ payload: [UInt8]) {
        lock.lock()
        payloads.append(payload)
        lock.unlock()
    }

    func snapshot() -> [[UInt8]] {
        lock.lock()
        defer { lock.unlock() }
        return payloads
    }
}

private final class TaskMessageInbox: @unchecked Sendable {
    private let lock = NSLock()
    private var messages: [TaskMessage] = []

    func append(_ message: TaskMessage) {
        lock.lock()
        messages.append(message)
        lock.unlock()
    }

    func snapshot() -> [TaskMessage] {
        lock.lock()
        defer { lock.unlock() }
        return messages
    }
}

private func testLane(taskInbox: TaskMessageInbox = TaskMessageInbox()) -> (Lane, TaskMessageInbox) {
    let handle = LaneHandle(
        commandTx: { _ in false },
        taskTx: { message in
            taskInbox.append(message)
            return true
        },
        role: .initiator
    )
    return (Lane(handle: handle, schemaReceiveTracker: SchemaTracker()), taskInbox)
}

@Suite(.serialized)
struct ChannelFlowControlTests {
    // r[verify rpc.channel]
    // r[verify rpc.channel.allocation]
    // r[verify rpc.channel.binding]
    // r[verify rpc.channel.binding.caller-args]
    // r[verify rpc.channel.binding.caller-args.rx]
    // r[verify rpc.channel.pair]
    // r[verify rpc.channel.pair.binding-propagation]
    // r[verify rpc.channel.pair.tx-read]
    // r[verify rpc.channel.payload-encoding]
    @Test func clientRxArgumentBindsPairedTxForSending() async throws {
        let (connection, inbox) = testLane()
        let (tx, rx): (UnboundTx<Int32>, UnboundRx<Int32>) = channel()

        let channelId = await connection.bindClientRxArg(rx, serialize: encI32)
        #expect(channelId == 1)
        #expect(channelWireIndexBytes(0) == [0, 0, 0, 0])

        try await tx.send(7)

        let messages = inbox.snapshot()
        #expect(messages.count == 1)
        guard case .data(let sentChannelId, let payload) = messages.first else {
            Issue.record("client Rx argument did not bind paired Tx for data")
            return
        }
        #expect(sentChannelId == channelId)
        var buf = ByteBufferAllocator().buffer(bytes: payload)
        #expect(try decI32(from: &buf) == 7)
    }

    // r[verify rpc.channel]
    // r[verify rpc.channel.allocation]
    // r[verify rpc.channel.binding]
    // r[verify rpc.channel.binding.caller-args]
    // r[verify rpc.channel.binding.caller-args.tx]
    // r[verify rpc.channel.pair]
    // r[verify rpc.channel.pair.binding-propagation]
    // r[verify rpc.channel.pair.rx-take]
    // r[verify rpc.channel.payload-encoding]
    @Test func clientTxArgumentBindsPairedRxForReceiving() async throws {
        let (connection, _) = testLane()
        let (tx, rx): (UnboundTx<Int32>, UnboundRx<Int32>) = channel()

        let channelId = await connection.bindClientTxArg(tx, deserialize: decI32)
        #expect(channelId == 1)
        #expect(channelWireIndexBytes(1) == [1, 0, 0, 0])

        var payload = ByteBufferAllocator().buffer(capacity: 4)
        encI32(11, into: &payload)
        let bytes = payload.readBytes(length: payload.readableBytes) ?? []
        #expect(await connection.incomingChannelRegistry.deliverData(channelId: channelId, payload: bytes))

        #expect(try await rx.recv() == 11)
    }

    // r[verify rpc.channel.close]
    // r[verify rpc.channel.lifecycle]
    // r[verify rpc.channel.reset]
    @Test func registryCloseAndResetTerminateReceivers() async throws {
        let registry = ChannelRegistry()

        let closeReceiver = await registry.register(21, initialCredit: 2)
        #expect(await registry.deliverClose(channelId: 21))
        #expect(try await closeReceiver.recv() == nil)

        let resetReceiver = await registry.register(23, initialCredit: 2)
        #expect(await registry.deliverData(channelId: 23, payload: [1, 2, 3]))
        await registry.deliverReset(channelId: 23)
        #expect(try await resetReceiver.recv() == [1, 2, 3])
        do {
            _ = try await resetReceiver.recv()
            Issue.record("expected reset receiver to observe channel error")
        } catch {
            #expect(error as? VoxRuntime.ChannelError == .reset)
        }
    }

    // r[verify rpc.channel.connection-closure]
    @Test func registryCloseAllTerminatesReceiversAndBlockedSenders() async throws {
        let registry = ChannelRegistry()
        let receiver = await registry.register(31, initialCredit: 2)
        let credit = await registry.registerOutgoing(33, initialCredit: 0)

        let blockedSend = Task {
            try await credit.consume()
        }
        for _ in 0..<10 {
            await Task.yield()
        }

        await registry.closeAllChannels()

        do {
            _ = try await receiver.recv()
            Issue.record("expected receiver to observe connection closure")
        } catch {
            #expect(error as? VoxRuntime.ChannelError == .connectionClosed)
        }
        do {
            try await blockedSend.value
            Issue.record("expected blocked sender to observe channel closure")
        } catch {
            #expect(error is VoxRuntime.ChannelError)
        }
    }

    // r[verify schema.interaction.channels]
    // r[verify rpc.channel]
    // r[verify rpc.channel.direction]
    // r[verify rpc.channel.binding.callee-args]
    // r[verify rpc.channel.binding.callee-args.tx]
    // r[verify rpc.channel.item]
    // r[verify rpc.flow-control.credit]
    // r[verify rpc.flow-control.credit.exhaustion]
    // r[verify rpc.flow-control.credit.grant]
    // r[verify rpc.flow-control.credit.initial]
    @Test func senderWaitsForGrantCredit() async throws {
        let registry = ChannelRegistry()
        let payloads = PayloadInbox()
        let credit = await registry.registerOutgoing(1, initialCredit: 1)
        let tx = Tx<Int32>(serialize: { val, buf in encI32(val, into: &buf) })
        tx.bind(
            channelId: 1,
            taskTx: { message in
                guard case .data(_, let payload) = message else {
                    return
                }
                payloads.append(payload)
            }, credit: credit)

        try await tx.send(1)

        let secondSend = Task {
            try await tx.send(2)
        }

        for _ in 0..<10 {
            await Task.yield()
        }

        let beforeGrant = payloads.snapshot()
        #expect(beforeGrant.count == 1)
        var beforeBuf = ByteBufferAllocator().buffer(bytes: beforeGrant[0])
        #expect(try decI32(from: &beforeBuf) == 1)

        await registry.deliverCredit(channelId: 1, bytes: 1)
        try await secondSend.value

        let afterGrant = payloads.snapshot()
        #expect(afterGrant.count == 2)
        var afterBuf = ByteBufferAllocator().buffer(bytes: afterGrant[1])
        #expect(try decI32(from: &afterBuf) == 2)
    }

    // r[verify rpc.flow-control.credit.try-send]
    @Test func trySendReturnsFullOrClosedWithOriginalValue() async throws {
        let registry = ChannelRegistry()
        let payloads = PayloadInbox()
        let credit = await registry.registerOutgoing(31, initialCredit: 1)
        let tx = Tx<Int32>(serialize: { val, buf in encI32(val, into: &buf) })
        tx.bind(
            channelId: 31,
            taskTx: { message in
                guard case .data(_, let payload) = message else {
                    return
                }
                payloads.append(payload)
            }, credit: credit)

        guard case .sent = try await tx.trySend(1) else {
            Issue.record("first trySend did not send")
            return
        }
        guard case .full(let value) = try await tx.trySend(2) else {
            Issue.record("second trySend did not report full")
            return
        }
        #expect(value == 2)

        let sent = payloads.snapshot()
        #expect(sent.count == 1)
        var sentBuf = ByteBufferAllocator().buffer(bytes: sent[0])
        #expect(try decI32(from: &sentBuf) == 1)

        tx.close()
        guard case .closed(let closedValue) = try await tx.trySend(3) else {
            Issue.record("trySend after close did not report closed")
            return
        }
        #expect(closedValue == 3)
    }

    // r[verify rpc.channel.binding.callee-args]
    // r[verify rpc.channel.binding.callee-args.rx]
    // r[verify rpc.channel.delivery.reliable]
    // r[verify rpc.flow-control.credit.grant]
    // r[verify rpc.flow-control.credit.grant.additive]
    // r[verify rpc.flow-control.credit.initial]
    @Test func receiverBatchesGrantCreditAtHalfWindow() async throws {
        let registry = ChannelRegistry()
        let inbox = GrantInbox()
        let receiver = await registry.register(7, initialCredit: 16) { additional in
            inbox.append(additional)
        }

        for i in 0..<8 {
            var buf = ByteBufferAllocator().buffer(capacity: 4)
            encI32(Int32(i), into: &buf)
            receiver.deliver(buf.readBytes(length: buf.readableBytes) ?? [])
        }

        for i in 0..<8 {
            let bytes = try await receiver.recv()
            #expect(bytes != nil)
            var itemBuf = ByteBufferAllocator().buffer(bytes: bytes!)
            let value = try decI32(from: &itemBuf)
            #expect(value == Int32(i))
        }

        let grants = inbox.snapshot()
        #expect(grants == [8])
    }

    // r[verify rpc.flow-control.credit.initial.high-level]
    // r[verify rpc.flow-control.credit.initial.zero]
    @Test func zeroInitialChannelCreditIsRejectedBeforeAdvertisingSettings() throws {
        do {
            _ = try makeConnectionSettings(
                parity: .odd,
                maxConcurrentRequests: 64,
                initialChannelCredit: 0
            )
            Issue.record("zero initial channel credit was accepted")
        } catch ConnectionError.protocolViolation(let rule) {
            #expect(rule == "rpc.flow-control.credit.initial.zero")
        } catch {
            Issue.record("unexpected error: \(error)")
        }
    }
}
