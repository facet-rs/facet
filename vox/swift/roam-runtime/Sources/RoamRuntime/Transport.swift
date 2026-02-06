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

// MARK: - COBS-Framed NIO Transport

/// COBS-framed transport over a NIO channel.
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
        var framed = cobsEncode(encoded)
        framed.append(0)  // Frame delimiter

        var buffer = channel.allocator.buffer(capacity: framed.count)
        buffer.writeBytes(framed)
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

// MARK: - COBS Frame Decoder

/// NIO handler that decodes COBS-framed messages.
final class COBSFrameDecoder: ByteToMessageDecoder, @unchecked Sendable {
    typealias InboundOut = [UInt8]
    private static let maxBufferedFrameBytes = 16 * 1024 * 1024
    private var searchOffset = 0

    func decode(context: ChannelHandlerContext, buffer: inout ByteBuffer) throws -> DecodingState {
        let readable = buffer.readableBytes
        if searchOffset > readable {
            searchOffset = 0
        }

        // Search only bytes not previously scanned since the last decoder invocation.
        guard
            let zeroIndex = buffer.readableBytesView.dropFirst(searchOffset).firstIndex(of: 0)
        else {
            searchOffset = readable
            if readable > Self.maxBufferedFrameBytes {
                throw TransportError.decodeFailed(
                    "COBS frame exceeds \(Self.maxBufferedFrameBytes) bytes without delimiter")
            }
            return .needMoreData
        }

        let frameLength = zeroIndex - buffer.readableBytesView.startIndex
        searchOffset = 0

        if frameLength == 0 {
            // Empty frame, skip the zero and continue
            buffer.moveReaderIndex(forwardBy: 1)
            return .continue
        }

        // Read the frame (excluding the zero)
        guard let frameBytes = buffer.readBytes(length: frameLength) else {
            return .needMoreData
        }

        // Skip the zero delimiter
        buffer.moveReaderIndex(forwardBy: 1)

        // Decode COBS
        let decoded = try cobsDecode(frameBytes)
        context.fireChannelRead(wrapInboundOut(decoded))

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
                "EOF with \(buffer.readableBytes) trailing bytes and no frame delimiter")
        }
        return state
    }
}

/// NIO handler that decodes wire messages from decoded COBS frames.
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
public func connect(host: String, port: Int) async throws -> NIOTransport {
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
            channel.pipeline.addHandler(ByteToMessageHandler(COBSFrameDecoder())).flatMap {
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
