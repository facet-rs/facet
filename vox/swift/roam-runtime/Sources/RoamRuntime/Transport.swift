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
    func send(_ message: MessageV7) async throws

    /// Receive the next message, or nil on EOF.
    func recv() async throws -> MessageV7?

    /// Update the maximum frame size the transport will accept.
    /// Called after handshake negotiation to match the negotiated max_payload_size.
    func setMaxFrameSize(_ size: Int) async throws

    /// Close the transport.
    func close() async throws
}

public enum TransportConduitKind: Sendable {
    case bare
    case stable
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
    private let inboundStream: AsyncStream<Result<MessageV7, Error>>
    private var inboundIterator: AsyncStream<Result<MessageV7, Error>>.Iterator
    private var owningGroup: MultiThreadedEventLoopGroup?

    init(
        channel: Channel, frameLimit: FrameLimit,
        inboundStream: AsyncStream<Result<MessageV7, Error>>,
        owningGroup: MultiThreadedEventLoopGroup? = nil
    ) {
        self.channel = channel
        self.frameLimit = frameLimit
        self.inboundStream = inboundStream
        self.inboundIterator = inboundStream.makeAsyncIterator()
        self.owningGroup = owningGroup
    }

    public func send(_ message: MessageV7) async throws {
        let encoded = message.encode()
        guard let len = UInt32(exactly: encoded.count) else {
            throw TransportError.frameEncoding("frame too large for u32 length prefix")
        }

        var buffer = channel.allocator.buffer(capacity: 4 + encoded.count)
        buffer.writeInteger(len, endianness: .little)
        buffer.writeBytes(encoded)
        try await channel.writeAndFlush(buffer)
    }

    public func recv() async throws -> MessageV7? {
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
        if let group = owningGroup {
            owningGroup = nil
            try await group.shutdownGracefully()
        }
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
    typealias InboundOut = MessageV7

    func channelRead(context: ChannelHandlerContext, data: NIOAny) {
        let bytes = unwrapInboundIn(data)
        do {
            let message = try MessageV7.decode(from: Data(bytes))
            context.fireChannelRead(wrapInboundOut(message))
        } catch {
            context.fireErrorCaught(error)
        }
    }
}

/// NIO handler that passes messages to an AsyncStream.
final class MessageStreamHandler: ChannelInboundHandler, @unchecked Sendable {
    typealias InboundIn = MessageV7

    private let continuation: AsyncStream<Result<MessageV7, Error>>.Continuation

    init(continuation: AsyncStream<Result<MessageV7, Error>>.Continuation) {
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

// MARK: - Connection Factory

/// Connect to a TCP server and return a transport.
///
/// The transport starts with a small frame limit (1 MiB), sufficient for handshake
/// messages. After negotiation, call `setMaxFrameSize()` to allow frames up to
/// the negotiated `max_payload_size`.
let transportHelloMagic = Array("ROTH".utf8)
let transportAcceptMagic = Array("ROTA".utf8)
let transportRejectMagic = Array("ROTR".utf8)
let transportVersion: UInt8 = 9
let rejectUnsupportedMode: UInt8 = 1
private let defaultTransportPrologueTimeoutNs: UInt64 = 5_000_000_000

func encodeTransportHello(_ conduit: TransportConduitKind) -> [UInt8] {
    [
        transportHelloMagic[0], transportHelloMagic[1], transportHelloMagic[2], transportHelloMagic[3],
        transportVersion,
        conduit == .stable ? 1 : 0,
        0,
        0,
    ]
}

public func encodeTransportAccept(_ conduit: TransportConduitKind) -> [UInt8] {
    [
        transportAcceptMagic[0], transportAcceptMagic[1], transportAcceptMagic[2], transportAcceptMagic[3],
        transportVersion,
        conduit == .stable ? 1 : 0,
        0,
        0,
    ]
}

func encodeTransportRejectUnsupported() -> [UInt8] {
    [
        transportRejectMagic[0], transportRejectMagic[1], transportRejectMagic[2], transportRejectMagic[3],
        transportVersion,
        rejectUnsupportedMode,
        0,
        0,
    ]
}

public func decodeTransportHello(_ bytes: [UInt8]) throws -> TransportConduitKind {
    guard bytes.count == 8 else {
        throw TransportError.protocolViolation("invalid transport hello size")
    }
    guard Array(bytes[0..<4]) == transportHelloMagic else {
        throw TransportError.protocolViolation("expected TransportHello")
    }
    guard bytes[4] == transportVersion else {
        throw TransportError.protocolViolation("unsupported transport version \(bytes[4])")
    }
    switch bytes[5] {
    case 0:
        return .bare
    case 1:
        return .stable
    default:
        throw TransportError.protocolViolation("unknown conduit mode \(bytes[5])")
    }
}

func validateTransportAccept(_ bytes: [UInt8], requested: TransportConduitKind) throws {
    guard bytes.count == 8 else {
        throw TransportError.protocolViolation("invalid transport prologue response size")
    }
    if Array(bytes[0..<4]) == transportAcceptMagic {
        guard bytes[4] == transportVersion else {
            throw TransportError.protocolViolation("unsupported transport version \(bytes[4])")
        }
        let selected = bytes[5] == 1 ? TransportConduitKind.stable : TransportConduitKind.bare
        guard selected == requested else {
            throw TransportError.protocolViolation("transport selected \(selected) for requested \(requested)")
        }
        return
    }
    if Array(bytes[0..<4]) == transportRejectMagic {
        if bytes[5] == rejectUnsupportedMode {
            throw TransportError.protocolViolation("transport rejected unsupported conduit mode")
        }
        throw TransportError.protocolViolation("transport rejected with reason \(bytes[5])")
    }
    throw TransportError.protocolViolation("expected TransportAccept or TransportReject")
}

public protocol RawTransportPrologueIO: Sendable {
    func sendRawPrologue(_ bytes: [UInt8]) async throws
    func recvRawPrologue() async throws -> [UInt8]?
}

public func performInitiatorTransportPrologue(
    transport: some RawTransportPrologueIO,
    conduit: TransportConduitKind
) async throws {
    try await transport.sendRawPrologue(encodeTransportHello(conduit))
    guard let response = try await transport.recvRawPrologue() else {
        throw TransportError.connectionClosed
    }
    try validateTransportAccept(response, requested: conduit)
}

public func performAcceptorTransportPrologue(
    transport: some RawTransportPrologueIO,
    supportedConduit: TransportConduitKind = .bare
) async throws -> TransportConduitKind {
    guard let request = try await transport.recvRawPrologue() else {
        throw TransportError.connectionClosed
    }
    let requested = try decodeTransportHello(request)
    guard requested == supportedConduit else {
        try await transport.sendRawPrologue(encodeTransportRejectUnsupported())
        throw TransportError.protocolViolation("transport rejected unsupported conduit mode")
    }
    try await transport.sendRawPrologue(encodeTransportAccept(requested))
    return requested
}

private func writeRawFrame(channel: Channel, bytes: [UInt8]) async throws {
    guard let len = UInt32(exactly: bytes.count) else {
        throw TransportError.frameEncoding("frame too large for u32 length prefix")
    }

    var buffer = channel.allocator.buffer(capacity: 4 + bytes.count)
    buffer.writeInteger(len, endianness: .little)
    buffer.writeBytes(bytes)
    try await channel.writeAndFlush(buffer)
}

private func awaitRawFrame(
    _ stream: AsyncStream<Result<[UInt8], Error>>,
    timeoutNs: UInt64
) async throws -> [UInt8] {
    try await withThrowingTaskGroup(of: [UInt8].self) { group in
        group.addTask {
            var iterator = stream.makeAsyncIterator()
            guard let result = await iterator.next() else {
                throw TransportError.connectionClosed
            }
            return try result.get()
        }
        group.addTask {
            try await Task.sleep(nanoseconds: timeoutNs)
            throw TransportError.protocolViolation("transport prologue timed out")
        }
        let response = try await group.next()!
        group.cancelAll()
        return response
    }
}

private func installMessagePipeline(
    channel: Channel,
    continuation: AsyncStream<Result<MessageV7, Error>>.Continuation
) async throws {
    try await channel.pipeline.addHandler(MessageDecoder()).flatMap {
        channel.pipeline.addHandler(MessageStreamHandler(continuation: continuation))
    }.get()
}

private func performTransportPrologue(
    channel: Channel,
    frameLimit: FrameLimit,
    conduit: TransportConduitKind,
    timeoutNs: UInt64
) async throws {
    if conduit == .stable {
        throw TransportError.protocolViolation("swift runtime does not yet support stable conduit")
    }

    var rawContinuation: AsyncStream<Result<[UInt8], Error>>.Continuation!
    let rawStream = AsyncStream<Result<[UInt8], Error>> { continuation in
        rawContinuation = continuation
    }
    let capturedRawContinuation = rawContinuation!
    let rawHandler = RawFrameStreamHandler(continuation: capturedRawContinuation)

    try await channel.pipeline.addHandler(
        ByteToMessageHandler(LengthPrefixDecoder(frameLimit: frameLimit))
    ).flatMap {
        channel.pipeline.addHandler(rawHandler)
    }.get()

    do {
        try await writeRawFrame(channel: channel, bytes: encodeTransportHello(conduit))
        let response = try await awaitRawFrame(rawStream, timeoutNs: timeoutNs)
        try validateTransportAccept(response, requested: conduit)
        try await channel.pipeline.removeHandler(rawHandler).get()
    } catch {
        try? await channel.pipeline.removeHandler(rawHandler).get()
        throw error
    }
}

public func connect(host: String, port: Int, conduit: TransportConduitKind = .bare) async throws -> NIOTransport {
    try await connect(
        host: host,
        port: port,
        conduit: conduit,
        prologueTimeoutNs: defaultTransportPrologueTimeoutNs
    )
}

func connect(
    host: String,
    port: Int,
    conduit: TransportConduitKind = .bare,
    prologueTimeoutNs: UInt64
) async throws -> NIOTransport {
    let frameLimit = FrameLimit(defaultMaxFrameBytes)
    let group = MultiThreadedEventLoopGroup(numberOfThreads: 1)

    var inboundContinuation: AsyncStream<Result<MessageV7, Error>>.Continuation!
    let inboundStream = AsyncStream<Result<MessageV7, Error>> { continuation in
        inboundContinuation = continuation
    }
    // Capture as let to satisfy Sendable requirements
    let capturedContinuation = inboundContinuation!

    let bootstrap = ClientBootstrap(group: group)
        .channelOption(ChannelOptions.socketOption(.so_keepalive), value: 1)

    do {
        let channel = try await bootstrap.connect(host: host, port: port).get()
        do {
            try await performTransportPrologue(
                channel: channel,
                frameLimit: frameLimit,
                conduit: conduit,
                timeoutNs: prologueTimeoutNs
            )
            try await installMessagePipeline(channel: channel, continuation: capturedContinuation)
            return NIOTransport(
                channel: channel,
                frameLimit: frameLimit,
                inboundStream: inboundStream,
                owningGroup: group
            )
        } catch {
            try? await channel.close()
            throw error
        }
    } catch {
        try? await group.shutdownGracefully()
        throw error
    }
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
