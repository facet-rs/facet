@preconcurrency import NIO
@preconcurrency import NIOCore
@preconcurrency import NIOPosix

private let defaultMaxFrameBytes = 1024 * 1024

// Link prologue — mirrors Rust vox-stream: the first bytes on every connection, before any
// framed message. [magic 'VOXL'][u8 version][u8 flags] (flags bit0 = fd-capable). Gives the
// framing layer a magic + version so a mismatch fails loudly instead of mis-framing, and
// makes the fd-capable header difference (4-byte vs 8-byte) negotiated rather than assumed.
let voxLinkMagic: [UInt8] = [0x56, 0x4F, 0x58, 0x4C]  // "VOXL"
let voxLinkVersion: UInt8 = 1
let voxLinkFlagFdCapable: UInt8 = 0x01
let voxLinkPrologueLen = 6

func voxLinkPrologue(fdCapable: Bool) -> [UInt8] {
    voxLinkMagic + [voxLinkVersion, fdCapable ? voxLinkFlagFdCapable : 0]
}

func validateVoxLinkPrologue(_ bytes: [UInt8], fdCapable: Bool) throws {
    guard bytes.count >= voxLinkPrologueLen else {
        throw TransportError.frameDecoding("short vox link prologue")
    }
    guard Array(bytes[0..<4]) == voxLinkMagic else {
        throw TransportError.frameDecoding("bad vox link magic")
    }
    guard bytes[4] == voxLinkVersion else {
        throw TransportError.frameDecoding(
            "unsupported vox link version \(bytes[4]); this build speaks \(voxLinkVersion)")
    }
    let peerFdCapable = (bytes[5] & voxLinkFlagFdCapable) != 0
    guard peerFdCapable == fdCapable else {
        throw TransportError.frameDecoding(
            "vox link fd-capability mismatch: peer=\(peerFdCapable), local=\(fdCapable)")
    }
}

/// Write this end's link prologue (buffered until the channel is active, flushed first).
func writeVoxLinkPrologue(_ channel: Channel, fdCapable: Bool) {
    var buf = channel.allocator.buffer(capacity: voxLinkPrologueLen)
    buf.writeBytes(voxLinkPrologue(fdCapable: fdCapable))
    channel.writeAndFlush(buf, promise: nil)
}

private enum QueuedFrame {
    case frame([UInt8])
    case failure(Error)
    case eof

    func value() throws -> [UInt8]? {
        switch self {
        case .frame(let bytes):
            return bytes
        case .failure(let error):
            throw error
        case .eof:
            return nil
        }
    }

    func resume(_ continuation: CheckedContinuation<[UInt8]?, Error>) {
        switch self {
        case .frame(let bytes):
            continuation.resume(returning: bytes)
        case .failure(let error):
            continuation.resume(throwing: error)
        case .eof:
            continuation.resume(returning: nil)
        }
    }
}

private actor CancelSafeFrameQueue {
    private var pending: [QueuedFrame] = []
    private var waiters: [(UInt64, CheckedContinuation<[UInt8]?, Error>)] = []
    private var cancelledWaiters = Set<UInt64>()
    private var nextWaiterId: UInt64 = 0
    private var finished = false

    func push(_ result: Result<[UInt8], Error>) {
        switch result {
        case .success(let bytes):
            deliver(.frame(bytes))
        case .failure(let error):
            deliver(.failure(error))
        }
    }

    func finish() {
        finished = true
        while !waiters.isEmpty {
            let (_, continuation) = waiters.removeFirst()
            continuation.resume(returning: nil)
        }
    }

    func recv() async throws -> [UInt8]? {
        if !pending.isEmpty {
            return try pending.removeFirst().value()
        }
        if finished {
            return nil
        }

        let id = nextWaiterId
        nextWaiterId += 1

        return try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                if cancelledWaiters.remove(id) != nil || Task.isCancelled {
                    continuation.resume(throwing: CancellationError())
                } else if !pending.isEmpty {
                    pending.removeFirst().resume(continuation)
                } else if finished {
                    continuation.resume(returning: nil)
                } else {
                    waiters.append((id, continuation))
                }
            }
        } onCancel: {
            Task {
                await self.cancelWaiter(id)
            }
        }
    }

    private func deliver(_ item: QueuedFrame) {
        if waiters.isEmpty {
            pending.append(item)
        } else {
            let (_, continuation) = waiters.removeFirst()
            item.resume(continuation)
        }
    }

    private func cancelWaiter(_ id: UInt64) {
        if let index = waiters.firstIndex(where: { $0.0 == id }) {
            let (_, continuation) = waiters.remove(at: index)
            continuation.resume(throwing: CancellationError())
        } else {
            cancelledWaiters.insert(id)
        }
    }
}

// r[impl transport.stream]
// r[impl transport.stream.kinds]
// r[impl link]
// r[impl link.message]
// r[impl link.order]
public final class NIOFrameLink: Link, @unchecked Sendable {
    private let channel: Channel
    private let frameLimit: FrameLimit
    private let frameQueue: CancelSafeFrameQueue
    private let inboundPump: Task<Void, Never>
    private var owningGroup: MultiThreadedEventLoopGroup?
    /// 8-byte fd-capable framing (unix) vs 4-byte plain framing (TCP/stdio).
    private let fdFramed: Bool

    init(
        channel: Channel,
        frameLimit: FrameLimit,
        inboundStream: AsyncStream<Result<[UInt8], Error>>,
        owningGroup: MultiThreadedEventLoopGroup? = nil,
        fdFramed: Bool
    ) {
        self.channel = channel
        self.frameLimit = frameLimit
        self.fdFramed = fdFramed
        let frameQueue = CancelSafeFrameQueue()
        self.frameQueue = frameQueue
        self.inboundPump = Task {
            var iterator = inboundStream.makeAsyncIterator()
            while let result = await iterator.next() {
                await frameQueue.push(result)
            }
            await frameQueue.finish()
        }
        self.owningGroup = owningGroup
    }

    // r[impl link.tx.cancel-safe]
    public func sendFrame(_ bytes: [UInt8]) async throws {
        let frameLimit = self.frameLimit
        let maxFrameBytes = try await channel.eventLoop.submit {
            frameLimit.maxFrameBytes
        }.get()
        guard bytes.count <= maxFrameBytes else {
            throw TransportError.frameEncoding("Frame exceeds \(maxFrameBytes) bytes")
        }
        try await writeRawFrame(channel: channel, bytes: bytes, fdFramed: fdFramed)
    }

    // r[impl link.rx.recv]
    // r[impl link.rx.eof]
    // r[impl link.rx.error]
    // r[impl rpc.transport.stream.cancel-safe-recv]
    public func recvFrame() async throws -> [UInt8]? {
        try await frameQueue.recv()
    }

    // r[impl link.tx.alloc.limits]
    public func setMaxFrameSize(_ size: Int) async throws {
        let frameLimit = self.frameLimit
        try await channel.eventLoop.submit {
            frameLimit.maxFrameBytes = size
        }.get()
    }

    // r[impl link.tx.close]
    public func close() async throws {
        if channel.isActive {
            try? await channel.close()
        }
        inboundPump.cancel()
        await frameQueue.finish()
        if let group = owningGroup {
            owningGroup = nil
            try await group.shutdownGracefully()
        }
    }

    func socketKeepaliveEnabled() async throws -> Bool {
        let value = try await channel.getOption(ChannelOptions.socketOption(.so_keepalive)).get()
        return value != 0
    }
}

// r[impl transport.stream]
// r[impl rpc.transport.stream.cancel-safe-recv]
// r[impl link.message.empty]
final class LengthPrefixDecoder: ByteToMessageDecoder, @unchecked Sendable {
    typealias InboundOut = [UInt8]
    private let frameLimit: FrameLimit
    /// fd-capable links (unix) use an 8-byte header [u32 len][u32 fd_count]; plain links
    /// (TCP/stdio) use a 4-byte header [u32 len]. Matches Rust FdStreamLink vs StreamLink.
    private let fdFramed: Bool
    private let headerSize: Int
    private var prologueValidated = false

    init(frameLimit: FrameLimit, fdFramed: Bool) {
        self.frameLimit = frameLimit
        self.fdFramed = fdFramed
        self.headerSize = fdFramed ? 8 : 4
    }

    func decode(context: ChannelHandlerContext, buffer: inout ByteBuffer) throws -> DecodingState {
        // First, consume + validate the peer's one-time link prologue.
        if !prologueValidated {
            guard buffer.readableBytes >= voxLinkPrologueLen,
                let header = buffer.getBytes(at: buffer.readerIndex, length: voxLinkPrologueLen)
            else {
                return .needMoreData
            }
            try validateVoxLinkPrologue(header, fdCapable: fdFramed)
            buffer.moveReaderIndex(forwardBy: voxLinkPrologueLen)
            prologueValidated = true
        }

        // Frame header: [u32 body_len LE] (+ [u32 fd_count LE] on fd-capable links). fd_count
        // is parsed-but-ignored for now (always 0 until SCM_RIGHTS lands on this side); the
        // field must still be consumed to stay frame-aligned with Rust.
        guard buffer.readableBytes >= headerSize,
            let frameLen: UInt32 = buffer.getInteger(at: buffer.readerIndex, endianness: .little)
        else {
            return .needMoreData
        }

        let frameLength = Int(frameLen)
        let maxFrameBytes = frameLimit.maxFrameBytes
        if frameLength > maxFrameBytes {
            throw TransportError.frameDecoding("Frame exceeds \(maxFrameBytes) bytes")
        }

        let needed = headerSize + frameLength
        guard buffer.readableBytes >= needed else {
            return .needMoreData
        }

        buffer.moveReaderIndex(forwardBy: headerSize)

        guard let frameBytes = buffer.readBytes(length: frameLength) else {
            return .needMoreData
        }

        context.fireChannelRead(wrapInboundOut(frameBytes))
        return .continue
    }

    func decodeLast(
        context: ChannelHandlerContext,
        buffer: inout ByteBuffer,
        seenEOF: Bool
    ) throws -> DecodingState {
        let state = try decode(context: context, buffer: &buffer)
        if state == .needMoreData && seenEOF && buffer.readableBytes > 0 {
            throw TransportError.frameDecoding(
                "EOF with \(buffer.readableBytes) trailing bytes and no complete frame"
            )
        }
        return state
    }
}

final class RawFrameStreamHandler: ChannelInboundHandler, RemovableChannelHandler, @unchecked Sendable {
    typealias InboundIn = [UInt8]

    private let continuation: AsyncStream<Result<[UInt8], Error>>.Continuation

    init(continuation: AsyncStream<Result<[UInt8], Error>>.Continuation) {
        self.continuation = continuation
    }

    func channelRead(context: ChannelHandlerContext, data: NIOAny) {
        continuation.yield(.success(unwrapInboundIn(data)))
    }

    // r[impl link.rx.error]
    func errorCaught(context: ChannelHandlerContext, error: Error) {
        continuation.yield(.failure(error))
        continuation.finish()
        context.close(promise: nil)
    }

    // r[impl link.rx.eof]
    func channelInactive(context: ChannelHandlerContext) {
        continuation.finish()
    }
}

// r[impl transport.stream]
// r[impl link.message.empty]
// r[impl link.tx.send]
// r[impl link.tx.cancel-safe]
func writeRawFrame(channel: Channel, bytes: [UInt8], fdFramed: Bool) async throws {
    guard let len = UInt32(exactly: bytes.count) else {
        throw TransportError.frameEncoding("frame too large for u32 length prefix")
    }

    // fd-capable links: [u32 body_len LE][u32 fd_count LE][body] (fd_count always 0 until
    // SCM_RIGHTS lands here, but the field must be present or Rust mis-frames). Plain links:
    // [u32 body_len LE][body].
    let headerSize = fdFramed ? 8 : 4
    var buffer = channel.allocator.buffer(capacity: headerSize + bytes.count)
    buffer.writeInteger(len, endianness: .little)
    if fdFramed {
        buffer.writeInteger(UInt32(0), endianness: .little)
    }
    buffer.writeBytes(bytes)
    try await channel.writeAndFlush(buffer)
}

// r[impl transport.stream.local]
// r[impl transport.stream.kinds]
func connectLink(unixPath: String) async throws -> NIOFrameLink {
    let frameLimit = FrameLimit(defaultMaxFrameBytes)
    let group = MultiThreadedEventLoopGroup(numberOfThreads: 1)

    var rawContinuation: AsyncStream<Result<[UInt8], Error>>.Continuation!
    let rawStream = AsyncStream<Result<[UInt8], Error>> { continuation in
        rawContinuation = continuation
    }
    let rawHandler = RawFrameStreamHandler(continuation: rawContinuation!)

    let bootstrap = ClientBootstrap(group: group)
        .channelInitializer { channel in
            do {
                try channel.pipeline.syncOperations.addHandler(
                    ByteToMessageHandler(LengthPrefixDecoder(frameLimit: frameLimit, fdFramed: true))
                )
                try channel.pipeline.syncOperations.addHandler(rawHandler)
                writeVoxLinkPrologue(channel, fdCapable: true)
                return channel.eventLoop.makeSucceededVoidFuture()
            } catch {
                return channel.eventLoop.makeFailedFuture(error)
            }
        }

    do {
        let channel = try await bootstrap.connect(unixDomainSocketPath: unixPath).get()
        return NIOFrameLink(
            channel: channel,
            frameLimit: frameLimit,
            inboundStream: rawStream,
            owningGroup: group,
            fdFramed: true
        )
    } catch {
        try? await group.shutdownGracefully()
        throw error
    }
}

// r[impl transport.stream]
// r[impl transport.stream.kinds]
func connectLink(host: String, port: Int) async throws -> NIOFrameLink {
    let frameLimit = FrameLimit(defaultMaxFrameBytes)
    let group = MultiThreadedEventLoopGroup(numberOfThreads: 1)

    var rawContinuation: AsyncStream<Result<[UInt8], Error>>.Continuation!
    let rawStream = AsyncStream<Result<[UInt8], Error>> { continuation in
        rawContinuation = continuation
    }
    let rawHandler = RawFrameStreamHandler(continuation: rawContinuation!)

    let bootstrap = ClientBootstrap(group: group)
        .channelOption(ChannelOptions.socketOption(.so_keepalive), value: 1)
        .channelInitializer { channel in
            do {
                try channel.pipeline.syncOperations.addHandler(
                    ByteToMessageHandler(LengthPrefixDecoder(frameLimit: frameLimit, fdFramed: false))
                )
                try channel.pipeline.syncOperations.addHandler(rawHandler)
                writeVoxLinkPrologue(channel, fdCapable: false)
                return channel.eventLoop.makeSucceededVoidFuture()
            } catch {
                return channel.eventLoop.makeFailedFuture(error)
            }
        }

    do {
        let channel = try await bootstrap.connect(host: host, port: port).get()
        return NIOFrameLink(
            channel: channel,
            frameLimit: frameLimit,
            inboundStream: rawStream,
            owningGroup: group,
            fdFramed: false
        )
    } catch {
        try? await group.shutdownGracefully()
        throw error
    }
}
