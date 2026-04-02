@preconcurrency import NIO
@preconcurrency import NIOCore
@preconcurrency import NIOPosix

private let defaultMaxFrameBytes = 1024 * 1024

public final class NIOFrameLink: Link, @unchecked Sendable {
    private let channel: Channel
    private let frameLimit: FrameLimit
    private let inboundStream: AsyncStream<Result<[UInt8], Error>>
    private var inboundIterator: AsyncStream<Result<[UInt8], Error>>.Iterator
    private var owningGroup: MultiThreadedEventLoopGroup?

    init(
        channel: Channel,
        frameLimit: FrameLimit,
        inboundStream: AsyncStream<Result<[UInt8], Error>>,
        owningGroup: MultiThreadedEventLoopGroup? = nil
    ) {
        self.channel = channel
        self.frameLimit = frameLimit
        self.inboundStream = inboundStream
        self.inboundIterator = inboundStream.makeAsyncIterator()
        self.owningGroup = owningGroup
    }

    public func sendFrame(_ bytes: [UInt8]) async throws {
        try await writeRawFrame(channel: channel, bytes: bytes)
    }

    public func recvFrame() async throws -> [UInt8]? {
        guard let result = await inboundIterator.next() else {
            return nil
        }
        return try result.get()
    }

    public func setMaxFrameSize(_ size: Int) async throws {
        let frameLimit = self.frameLimit
        try await channel.eventLoop.submit {
            frameLimit.maxFrameBytes = size
        }.get()
    }

    public func close() async throws {
        if channel.isActive {
            try? await channel.close()
        }
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

final class LengthPrefixDecoder: ByteToMessageDecoder, @unchecked Sendable {
    typealias InboundOut = [UInt8]
    private let frameLimit: FrameLimit

    init(frameLimit: FrameLimit) {
        self.frameLimit = frameLimit
    }

    func decode(context: ChannelHandlerContext, buffer: inout ByteBuffer) throws -> DecodingState {
        guard let frameLen: UInt32 = buffer.getInteger(at: buffer.readerIndex, endianness: .little)
        else {
            return .needMoreData
        }

        let frameLength = Int(frameLen)
        let maxFrameBytes = frameLimit.maxFrameBytes
        if frameLength > maxFrameBytes {
            throw TransportError.frameDecoding("Frame exceeds \(maxFrameBytes) bytes")
        }

        let needed = 4 + frameLength
        guard buffer.readableBytes >= needed else {
            return .needMoreData
        }

        buffer.moveReaderIndex(forwardBy: 4)

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

    func errorCaught(context: ChannelHandlerContext, error: Error) {
        continuation.yield(.failure(error))
    }

    func channelInactive(context: ChannelHandlerContext) {
        continuation.finish()
    }
}

func writeRawFrame(channel: Channel, bytes: [UInt8]) async throws {
    guard let len = UInt32(exactly: bytes.count) else {
        throw TransportError.frameEncoding("frame too large for u32 length prefix")
    }

    var buffer = channel.allocator.buffer(capacity: 4 + bytes.count)
    buffer.writeInteger(len, endianness: .little)
    buffer.writeBytes(bytes)
    try await channel.writeAndFlush(buffer)
}

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
                    ByteToMessageHandler(LengthPrefixDecoder(frameLimit: frameLimit))
                )
                try channel.pipeline.syncOperations.addHandler(rawHandler)
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
            owningGroup: group
        )
    } catch {
        try? await group.shutdownGracefully()
        throw error
    }
}

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
                    ByteToMessageHandler(LengthPrefixDecoder(frameLimit: frameLimit))
                )
                try channel.pipeline.syncOperations.addHandler(rawHandler)
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
            owningGroup: group
        )
    } catch {
        try? await group.shutdownGracefully()
        throw error
    }
}
