import Foundation

private let stableClientHelloMagic = Array("ROCH".utf8)
private let stableServerHelloMagic = Array("ROSH".utf8)

private let stableClientHelloHasResumeKey: UInt8 = 1 << 0
private let stableClientHelloHasLastReceived: UInt8 = 1 << 1
private let stableServerHelloRejected: UInt8 = 1 << 0
private let stableServerHelloHasLastReceived: UInt8 = 1 << 1
private let stableHelloSize = 25

private struct StableClientHello: Sendable {
    let resumeKey: [UInt8]?
    let lastReceived: UInt32?
}

private struct StableServerHello: Sendable {
    let resumeKey: [UInt8]
    let lastReceived: UInt32?
    let rejected: Bool
}

private struct StablePacketAck: Sendable {
    let maxDelivered: UInt32
}

private struct StableFrame: Sendable {
    let seq: UInt32
    let ack: StablePacketAck?
    let item: Message
}

private struct ReplayEntry: Sendable {
    let seq: UInt32
    let itemBytes: [UInt8]
}

private struct StableLinkLease: Sendable {
    let link: any Link
    let generation: UInt64
}

private func stableRandomResumeKey() -> [UInt8] {
    var generator = SystemRandomNumberGenerator()
    return (0..<16).map { _ in UInt8.random(in: UInt8.min...UInt8.max, using: &generator) }
}

private func stableAppendFixedU32(_ value: UInt32, to bytes: inout [UInt8]) {
    bytes.append(UInt8(truncatingIfNeeded: value))
    bytes.append(UInt8(truncatingIfNeeded: value >> 8))
    bytes.append(UInt8(truncatingIfNeeded: value >> 16))
    bytes.append(UInt8(truncatingIfNeeded: value >> 24))
}

private func stableReadFixedU32(_ bytes: [UInt8], offset: Int) throws -> UInt32 {
    guard offset + 4 <= bytes.count else {
        throw TransportError.protocolViolation("stable handshake truncated")
    }
    return UInt32(bytes[offset])
        | (UInt32(bytes[offset + 1]) << 8)
        | (UInt32(bytes[offset + 2]) << 16)
        | (UInt32(bytes[offset + 3]) << 24)
}

private func encodeStableClientHello(
    resumeKey: [UInt8]?,
    lastReceived: UInt32?
) -> [UInt8] {
    var bytes = [UInt8]()
    bytes.reserveCapacity(stableHelloSize)
    bytes += stableClientHelloMagic

    var flags: UInt8 = 0
    if resumeKey != nil {
        flags |= stableClientHelloHasResumeKey
    }
    if lastReceived != nil {
        flags |= stableClientHelloHasLastReceived
    }
    bytes.append(flags)
    bytes += resumeKey ?? Array(repeating: 0, count: 16)
    stableAppendFixedU32(lastReceived ?? 0, to: &bytes)
    return bytes
}

private func decodeStableClientHello(_ bytes: [UInt8]) throws -> StableClientHello {
    guard bytes.count == stableHelloSize else {
        throw TransportError.protocolViolation("invalid StableConduit ClientHello size")
    }
    guard Array(bytes[0..<4]) == stableClientHelloMagic else {
        throw TransportError.protocolViolation("invalid StableConduit ClientHello magic")
    }

    let flags = bytes[4]
    let resumeKey = (flags & stableClientHelloHasResumeKey) == 0 ? nil : Array(bytes[5..<21])
    let lastReceived = (flags & stableClientHelloHasLastReceived) == 0
        ? nil
        : try stableReadFixedU32(bytes, offset: 21)
    return StableClientHello(resumeKey: resumeKey, lastReceived: lastReceived)
}

private func encodeStableServerHello(
    resumeKey: [UInt8],
    lastReceived: UInt32?,
    rejected: Bool
) -> [UInt8] {
    var bytes = [UInt8]()
    bytes.reserveCapacity(stableHelloSize)
    bytes += stableServerHelloMagic

    var flags: UInt8 = 0
    if rejected {
        flags |= stableServerHelloRejected
    }
    if lastReceived != nil {
        flags |= stableServerHelloHasLastReceived
    }
    bytes.append(flags)
    bytes += resumeKey
    stableAppendFixedU32(lastReceived ?? 0, to: &bytes)
    return bytes
}

private func decodeStableServerHello(_ bytes: [UInt8]) throws -> StableServerHello {
    guard bytes.count == stableHelloSize else {
        throw TransportError.protocolViolation("invalid StableConduit ServerHello size")
    }
    guard Array(bytes[0..<4]) == stableServerHelloMagic else {
        throw TransportError.protocolViolation("invalid StableConduit ServerHello magic")
    }

    let flags = bytes[4]
    return StableServerHello(
        resumeKey: Array(bytes[5..<21]),
        lastReceived: (flags & stableServerHelloHasLastReceived) == 0
            ? nil
            : try stableReadFixedU32(bytes, offset: 21),
        rejected: (flags & stableServerHelloRejected) != 0
    )
}

private func encodeStableFrame(
    seq: UInt32,
    ack: UInt32?,
    itemBytes: [UInt8]
) -> [UInt8] {
    var bytes = [UInt8]()
    bytes.reserveCapacity(itemBytes.count + 16)
    bytes += encodeU32(seq)
    bytes += encodeOption(ack) { encodeU32($0) }
    bytes += itemBytes
    return bytes
}

private func decodeStableFrame(_ bytes: [UInt8]) throws -> StableFrame {
    let data = Data(bytes)
    var offset = 0
    let seq = try decodeU32(from: data, offset: &offset)
    let ackValue = try decodeOption(from: data, offset: &offset) { data, offset in
        try decodeU32(from: data, offset: &offset)
    }
    let item = try Message.decode(from: data, offset: &offset)
    guard offset == data.count else {
        throw WireError.trailingBytes
    }
    return StableFrame(
        seq: seq,
        ack: ackValue.map { StablePacketAck(maxDelivered: $0) },
        item: item
    )
}

public func prepareStableAcceptorAttachment(link: any Link) async throws -> LinkAttachment {
    guard let clientHello = try await link.recvRawPrologue() else {
        throw TransportError.connectionClosed
    }
    _ = try decodeStableClientHello(clientHello)
    return LinkAttachment(link: link, clientHello: clientHello)
}

actor StableConduitState {
    private let source: any LinkSource
    private var currentLink: (any Link)?
    private var currentGeneration: UInt64 = 0
    private var resumeKey: [UInt8]?
    private var nextSendSeq: UInt32 = 0
    private var lastReceived: UInt32?
    private var replay: [ReplayEntry] = []
    private var maxFrameSize: Int?
    private var closed = false

    init(source: any LinkSource) {
        self.source = source
    }

    func enqueueReplayEntry(itemBytes: [UInt8]) -> UInt32 {
        let seq = nextSendSeq
        nextSendSeq &+= 1
        replay.append(ReplayEntry(seq: seq, itemBytes: itemBytes))
        return seq
    }

    func currentAck() -> UInt32? {
        lastReceived
    }

    fileprivate func ensureLink() async throws -> StableLinkLease {
        if closed {
            throw ConnectionError.connectionClosed
        }
        if let currentLink {
            return StableLinkLease(link: currentLink, generation: currentGeneration)
        }

        let attachment = try await source.nextLink()
        let link = attachment.link
        if let clientHello = attachment.clientHello {
            try await attachAcceptor(link: link, clientHelloBytes: clientHello)
        } else {
            try await attachInitiator(link: link)
        }
        if let maxFrameSize {
            try await link.setMaxFrameSize(maxFrameSize)
        }
        currentGeneration &+= 1
        currentLink = link
        return StableLinkLease(link: link, generation: currentGeneration)
    }

    func invalidate(generation: UInt64) {
        if currentGeneration == generation {
            currentLink = nil
        }
    }

    func trimReplay(maxDelivered: UInt32) {
        replay.removeAll { $0.seq <= maxDelivered }
    }

    func markReceived(seq: UInt32) -> Bool {
        if let lastReceived, seq <= lastReceived {
            return false
        }
        lastReceived = seq
        return true
    }

    func setMaxFrameSize(_ size: Int) async throws {
        maxFrameSize = size
        if let currentLink {
            try await currentLink.setMaxFrameSize(size)
        }
    }

    func close() async {
        closed = true
        let link = currentLink
        currentLink = nil
        if let link {
            try? await link.close()
        }
    }

    private func attachInitiator(link: any Link) async throws {
        try await link.sendRawPrologue(
            encodeStableClientHello(
                resumeKey: resumeKey,
                lastReceived: lastReceived
            )
        )
        guard let rawHello = try await link.recvRawPrologue() else {
            throw TransportError.connectionClosed
        }
        let serverHello = try decodeStableServerHello(rawHello)
        if serverHello.rejected {
            throw TransportError.protocolViolation("stable conduit session lost")
        }
        resumeKey = serverHello.resumeKey
        try await replayBufferedMessages(
            on: link,
            peerLastReceived: serverHello.lastReceived
        )
    }

    private func attachAcceptor(link: any Link, clientHelloBytes: [UInt8]) async throws {
        let clientHello = try decodeStableClientHello(clientHelloBytes)
        let newResumeKey = stableRandomResumeKey()
        try await link.sendRawPrologue(
            encodeStableServerHello(
                resumeKey: newResumeKey,
                lastReceived: lastReceived,
                rejected: false
            )
        )
        resumeKey = newResumeKey
        try await replayBufferedMessages(
            on: link,
            peerLastReceived: clientHello.lastReceived
        )
    }

    private func replayBufferedMessages(
        on link: any Link,
        peerLastReceived: UInt32?
    ) async throws {
        let ack = lastReceived
        for entry in replay {
            if let peerLastReceived, entry.seq <= peerLastReceived {
                continue
            }
            try await link.sendFrame(
                encodeStableFrame(
                    seq: entry.seq,
                    ack: ack,
                    itemBytes: entry.itemBytes
                )
            )
        }
    }
}

public final class StableConduit: Conduit, @unchecked Sendable {
    private let state: StableConduitState
    private let sendSemaphore = AsyncSemaphore(permits: 1)

    private init(source: any LinkSource) {
        self.state = StableConduitState(source: source)
    }

    public static func connect(source: some LinkSource) async throws -> StableConduit {
        let conduit = StableConduit(source: source)
        _ = try await conduit.state.ensureLink()
        return conduit
    }

    public func send(_ message: Message) async throws {
        try await withSendPermit {
            let itemBytes = message.encode()
            let seq = await state.enqueueReplayEntry(itemBytes: itemBytes)
            while true {
                let lease = try await state.ensureLink()
                let frameBytes = encodeStableFrame(
                    seq: seq,
                    ack: await state.currentAck(),
                    itemBytes: itemBytes
                )
                do {
                    try await lease.link.sendFrame(frameBytes)
                    return
                } catch {
                    await state.invalidate(generation: lease.generation)
                    continue
                }
            }
        }
    }

    public func recv() async throws -> Message? {
        while true {
            let lease = try await state.ensureLink()
            let payload: [UInt8]
            do {
                guard let nextPayload = try await lease.link.recvFrame() else {
                    await state.invalidate(generation: lease.generation)
                    continue
                }
                payload = nextPayload
            } catch {
                await state.invalidate(generation: lease.generation)
                continue
            }

            let frame = try decodeStableFrame(payload)
            if let ack = frame.ack {
                await state.trimReplay(maxDelivered: ack.maxDelivered)
            }
            let shouldDeliver = await state.markReceived(seq: frame.seq)
            if shouldDeliver {
                return frame.item
            }
        }
    }

    public func setMaxFrameSize(_ size: Int) async throws {
        try await state.setMaxFrameSize(size)
    }

    public func close() async throws {
        await state.close()
        await sendSemaphore.close()
    }

    private func withSendPermit<T>(
        _ body: @Sendable () async throws -> T
    ) async throws -> T {
        try await sendSemaphore.acquire()
        do {
            let result = try await body()
            await sendSemaphore.release()
            return result
        } catch {
            await sendSemaphore.release()
            throw error
        }
    }
}
