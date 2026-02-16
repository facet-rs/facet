import Foundation
@preconcurrency import NIO
@preconcurrency import NIOCore
@preconcurrency import NIOPosix

// MARK: - Debug Logging

/// Logger for roam-runtime debug output.
/// Enable with ROAM_DEBUG=1 environment variable.
private let debugEnabled = ProcessInfo.processInfo.environment["ROAM_DEBUG"] != nil

func debugLog(_ message: String) {
    if debugEnabled {
        let pid = ProcessInfo.processInfo.processIdentifier
        let line = "[\(pid)] DEBUG: \(message)"
        NSLog("%@", line)
    }
}

func warnLog(_ message: String) {
    let pid = ProcessInfo.processInfo.processIdentifier
    let line = "[\(pid)] WARN: \(message)"
    NSLog("%@", line)
}

// MARK: - Transport Protocol

/// Protocol for message transport.
public protocol MessageTransport: Sendable {
    /// Send a message.
    func send(_ message: Message) async throws

    /// Receive the next message, or nil on EOF.
    func recv() async throws -> Message?

    /// Update the maximum frame size the transport will accept.
    /// Called after handshake negotiation to match the negotiated max_payload_size.
    func setMaxFrameSize(_ size: Int) async throws

    /// Close the transport.
    func close() async throws
}

// MARK: - Shared Frame Limit

/// Shared mutable frame limit, referenced by both `NIOTransport` and `LengthPrefixDecoder`.
/// Updates are performed on the NIO event loop via the channel, so no lock is needed.
final class FrameLimit: @unchecked Sendable {
    /// Current maximum frame size in bytes.
    var maxFrameBytes: Int

    init(_ maxFrameBytes: Int) {
        self.maxFrameBytes = maxFrameBytes
    }
}

// MARK: - Length-Prefixed NIO Transport

/// Default frame limit: enough for Hello messages and small payloads during handshake.
/// The real limit is set after handshake via `setMaxFrameSize()`.
private let defaultMaxFrameBytes = 1024 * 1024  // 1 MiB

/// Length-prefixed transport over a NIO channel.
public final class NIOTransport: MessageTransport, @unchecked Sendable {
    private let channel: Channel
    private let frameLimit: FrameLimit
    private let inboundStream: AsyncStream<Result<Message, Error>>
    private var inboundIterator: AsyncStream<Result<Message, Error>>.Iterator

    init(
        channel: Channel, frameLimit: FrameLimit,
        inboundStream: AsyncStream<Result<Message, Error>>
    ) {
        self.channel = channel
        self.frameLimit = frameLimit
        self.inboundStream = inboundStream
        self.inboundIterator = inboundStream.makeAsyncIterator()
    }

    public func send(_ message: Message) async throws {
        let encoded = message.encode()
        guard let len = UInt32(exactly: encoded.count) else {
            throw TransportError.frameEncoding("frame too large for u32 length prefix")
        }

        var buffer = channel.allocator.buffer(capacity: 4 + encoded.count)
        buffer.writeInteger(len, endianness: .little)
        buffer.writeBytes(encoded)
        try await channel.writeAndFlush(buffer)
    }

    public func recv() async throws -> Message? {
        guard let result = await inboundIterator.next() else {
            return nil
        }
        return try result.get()
    }

    /// Update the frame decoder's maximum frame size.
    /// Called after handshake to allow frames up to the negotiated payload size.
    public func setMaxFrameSize(_ size: Int) async throws {
        let frameLimit = self.frameLimit
        // Run on the event loop to safely update the shared limit
        try await channel.eventLoop.submit {
            frameLimit.maxFrameBytes = size
        }.get()
    }

    public func close() async throws {
        try await channel.close()
    }

    // Internal testing hook to verify socket options configured by connect().
    func socketKeepaliveEnabled() async throws -> Bool {
        let value = try await channel.getOption(ChannelOptions.socketOption(.so_keepalive)).get()
        return value != 0
    }
}

// MARK: - Length Prefix Decoder

/// NIO handler that decodes length-prefixed messages.
///
/// The frame limit is shared via `FrameLimit` and can be updated after handshake
/// by calling `NIOTransport.setMaxFrameSize()`. Starts with a small default
/// (enough for Hello messages) and is resized once negotiation completes.
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
            throw TransportError.frameDecoding(
                "Frame exceeds \(maxFrameBytes) bytes")
        }

        let needed = 4 + frameLength
        guard buffer.readableBytes >= needed else {
            return .needMoreData
        }

        // Consume header
        buffer.moveReaderIndex(forwardBy: 4)

        // Read payload
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
                "EOF with \(buffer.readableBytes) trailing bytes and no complete frame")
        }
        return state
    }
}

/// NIO handler that decodes wire messages from decoded frames.
final class MessageDecoder: ChannelInboundHandler, Sendable {
    typealias InboundIn = [UInt8]
    typealias InboundOut = Message

    func channelRead(context: ChannelHandlerContext, data: NIOAny) {
        let bytes = unwrapInboundIn(data)
        do {
            let message = try Message.decode(from: Data(bytes))
            context.fireChannelRead(wrapInboundOut(message))
        } catch {
            context.fireErrorCaught(error)
        }
    }
}

/// NIO handler that passes messages to an AsyncStream.
final class MessageStreamHandler: ChannelInboundHandler, @unchecked Sendable {
    typealias InboundIn = Message

    private let continuation: AsyncStream<Result<Message, Error>>.Continuation

    init(continuation: AsyncStream<Result<Message, Error>>.Continuation) {
        self.continuation = continuation
    }

    func channelRead(context: ChannelHandlerContext, data: NIOAny) {
        let message = unwrapInboundIn(data)
        continuation.yield(.success(message))
    }

    func errorCaught(context: ChannelHandlerContext, error: Error) {
        continuation.yield(.failure(error))
    }

    func channelInactive(context: ChannelHandlerContext) {
        continuation.finish()
    }
}

// MARK: - Connection Factory

/// Connect to a TCP server and return a transport.
///
/// The transport starts with a small frame limit (1 MiB), sufficient for handshake
/// messages. After negotiation, call `setMaxFrameSize()` to allow frames up to
/// the negotiated `max_payload_size`.
public func connect(host: String, port: Int) async throws -> NIOTransport {
    let frameLimit = FrameLimit(defaultMaxFrameBytes)
    let group = MultiThreadedEventLoopGroup(numberOfThreads: 1)

    var inboundContinuation: AsyncStream<Result<Message, Error>>.Continuation!
    let inboundStream = AsyncStream<Result<Message, Error>> { continuation in
        inboundContinuation = continuation
    }
    // Capture as let to satisfy Sendable requirements
    let capturedContinuation = inboundContinuation!

    let bootstrap = ClientBootstrap(group: group)
        .channelOption(ChannelOptions.socketOption(.so_keepalive), value: 1)
        .channelInitializer { channel in
            // Note: ByteToMessageHandler is explicitly non-Sendable in SwiftNIO.
            // This warning is benign - channel initializers run on the event loop.
            channel.pipeline.addHandler(
                ByteToMessageHandler(LengthPrefixDecoder(frameLimit: frameLimit))
            ).flatMap {
                channel.pipeline.addHandler(MessageDecoder())
            }.flatMap {
                channel.pipeline.addHandler(
                    MessageStreamHandler(continuation: capturedContinuation))
            }
        }

    let channel = try await bootstrap.connect(host: host, port: port).get()
    return NIOTransport(channel: channel, frameLimit: frameLimit, inboundStream: inboundStream)
}

// MARK: - Errors

public enum TransportError: Error {
    case connectionClosed
    case wouldBlock
    case frameEncoding(String)
    case frameDecoding(String)
    case transportIO(String)
    case protocolViolation(String)
}
