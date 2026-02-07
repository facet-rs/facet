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
        let data = "[\(pid)] DEBUG: \(message)\n".data(using: .utf8)!
        FileHandle.standardError.write(data)
    }
}

// MARK: - Transport Protocol

/// Protocol for message transport.
public protocol MessageTransport: Sendable {
    /// Send a message.
    func send(_ message: Message) async throws

    /// Receive the next message, or nil on EOF.
    func recv() async throws -> Message?

    /// Close the transport.
    func close() async throws
}

// MARK: - Length-Prefixed NIO Transport

/// Length-prefixed transport over a NIO channel.
public final class NIOTransport: MessageTransport, @unchecked Sendable {
    private let channel: Channel
    private let inboundStream: AsyncStream<Result<Message, Error>>
    private var inboundIterator: AsyncStream<Result<Message, Error>>.Iterator

    init(channel: Channel, inboundStream: AsyncStream<Result<Message, Error>>) {
        self.channel = channel
        self.inboundStream = inboundStream
        self.inboundIterator = inboundStream.makeAsyncIterator()
    }

    public func send(_ message: Message) async throws {
        let encoded = message.encode()
        guard let len = UInt32(exactly: encoded.count) else {
            throw TransportError.decodeFailed("frame too large for u32 length prefix")
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

    public func close() async throws {
        try await channel.close()
    }
}

// MARK: - Length Prefix Decoder

/// NIO handler that decodes length-prefixed messages.
final class LengthPrefixDecoder: ByteToMessageDecoder, @unchecked Sendable {
    typealias InboundOut = [UInt8]
    private let maxFrameBytes: Int

    init(maxFrameBytes: Int = 16 * 1024 * 1024) {
        self.maxFrameBytes = maxFrameBytes
    }

    func decode(context: ChannelHandlerContext, buffer: inout ByteBuffer) throws -> DecodingState {
        guard let frameLen: UInt32 = buffer.getInteger(at: buffer.readerIndex, endianness: .little)
        else {
            return .needMoreData
        }

        let frameLength = Int(frameLen)
        if frameLength > maxFrameBytes {
            throw TransportError.decodeFailed(
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
            throw TransportError.decodeFailed(
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
/// - Parameters:
///   - host: The host to connect to.
///   - port: The port to connect to.
///   - maxPayloadSize: The maximum payload size that will be negotiated. The frame decoder
///     limit is set to `maxPayloadSize + 64` to account for message header overhead.
///     Defaults to 16 MiB if not specified.
public func connect(host: String, port: Int, maxPayloadSize: UInt32? = nil) async throws
    -> NIOTransport
{
    // Frame limit must accommodate the payload plus message header overhead.
    let maxFrameBytes: Int =
        if let maxPayloadSize {
            Int(maxPayloadSize) + 64
        } else {
            16 * 1024 * 1024
        }

    let group = MultiThreadedEventLoopGroup(numberOfThreads: 1)

    var inboundContinuation: AsyncStream<Result<Message, Error>>.Continuation!
    let inboundStream = AsyncStream<Result<Message, Error>> { continuation in
        inboundContinuation = continuation
    }
    // Capture as let to satisfy Sendable requirements
    let capturedContinuation = inboundContinuation!

    let bootstrap = ClientBootstrap(group: group)
        .channelInitializer { channel in
            // Note: ByteToMessageHandler is explicitly non-Sendable in SwiftNIO.
            // This warning is benign - channel initializers run on the event loop.
            channel.pipeline.addHandler(
                ByteToMessageHandler(LengthPrefixDecoder(maxFrameBytes: maxFrameBytes))
            ).flatMap {
                channel.pipeline.addHandler(MessageDecoder())
            }.flatMap {
                channel.pipeline.addHandler(
                    MessageStreamHandler(continuation: capturedContinuation))
            }
        }

    let channel = try await bootstrap.connect(host: host, port: port).get()
    return NIOTransport(channel: channel, inboundStream: inboundStream)
}

// MARK: - Errors

public enum TransportError: Error {
    case connectionClosed
    case decodeFailed(String)
}
