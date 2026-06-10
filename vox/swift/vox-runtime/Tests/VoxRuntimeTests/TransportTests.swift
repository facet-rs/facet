import Foundation
@preconcurrency import NIO
@preconcurrency import NIOPosix
import Testing

@testable import VoxRuntime

private struct LocalServer {
  let group: MultiThreadedEventLoopGroup
  let channel: Channel
  let port: Int
}

private struct LocalUnixServer {
  let group: MultiThreadedEventLoopGroup
  let channel: Channel
  let path: String
}

private let transportAcceptBareBytes: [UInt8] = Array("VOTA".utf8) + [9, 0, 0, 0]

private actor FrameCapture {
  private var frames: [[UInt8]] = []
  private var inactive = false

  func record(_ bytes: [UInt8]) {
    frames.append(bytes)
  }

  func markInactive() {
    inactive = true
  }

  func waitForFrameCount(_ count: Int, timeoutMs: UInt64 = 1_000) async -> [[UInt8]]? {
    let start = ContinuousClock.now
    let timeout = Duration.milliseconds(Int64(timeoutMs))
    while ContinuousClock.now - start < timeout {
      if frames.count >= count {
        return frames
      }
      try? await Task.sleep(nanoseconds: 5_000_000)
    }
    return nil
  }

  func waitForInactive(timeoutMs: UInt64 = 1_000) async -> Bool {
    let start = ContinuousClock.now
    let timeout = Duration.milliseconds(Int64(timeoutMs))
    while ContinuousClock.now - start < timeout {
      if inactive {
        return true
      }
      try? await Task.sleep(nanoseconds: 5_000_000)
    }
    return inactive
  }
}

private func appendLengthPrefixedFrame(
  _ bytes: [UInt8], to buffer: inout ByteBuffer, fdFramed: Bool = false
) {
  buffer.writeInteger(UInt32(bytes.count), endianness: .little)
  if fdFramed {
    buffer.writeInteger(UInt32(0), endianness: .little)
  }
  buffer.writeBytes(bytes)
}

private func writeLengthPrefixedFrame(_ bytes: [UInt8], to channel: Channel, fdFramed: Bool = false)
{
  let headerSize = fdFramed ? 8 : 4
  var buffer = channel.allocator.buffer(capacity: headerSize + bytes.count)
  appendLengthPrefixedFrame(bytes, to: &buffer, fdFramed: fdFramed)
  channel.write(buffer, promise: nil)
}

private func consumePeerLinkPrologue(
  _ buffer: inout ByteBuffer,
  pending: inout [UInt8],
  fdFramed: Bool
) throws -> Bool {
  while pending.count < voxLinkPrologueLen && buffer.readableBytes > 0 {
    guard let byte: UInt8 = buffer.readInteger() else {
      break
    }
    pending.append(byte)
  }
  if pending.count == voxLinkPrologueLen {
    try validateVoxLinkPrologue(pending, fdCapable: fdFramed)
    return true
  }
  return false
}

private func lengthPrefixedFrameBytes(_ bytes: [UInt8], fdFramed: Bool = false) -> [UInt8] {
  var out = [UInt8]()
  let headerSize = fdFramed ? 8 : 4
  out.reserveCapacity(headerSize + bytes.count)
  let len = UInt32(bytes.count).littleEndian
  withUnsafeBytes(of: len) { out.append(contentsOf: $0) }
  if fdFramed {
    let fdCount = UInt32(0).littleEndian
    withUnsafeBytes(of: fdCount) { out.append(contentsOf: $0) }
  }
  out.append(contentsOf: bytes)
  return out
}

private final class WriteOnActiveHandler: ChannelInboundHandler, Sendable {
  typealias InboundIn = Never

  private let bytes: [UInt8]

  init(bytes: [UInt8]) {
    self.bytes = bytes
  }

  func channelActive(context: ChannelHandlerContext) {
    var buffer = context.channel.allocator.buffer(capacity: voxLinkPrologueLen + 4 + bytes.count)
    buffer.writeBytes(voxLinkPrologue(fdCapable: false))
    appendLengthPrefixedFrame(bytes, to: &buffer)
    context.writeAndFlush(NIOAny(buffer), promise: nil)
    context.fireChannelActive()
  }
}

private final class WriteRawChunksThenCloseHandler: ChannelInboundHandler, @unchecked Sendable {
  typealias InboundIn = ByteBuffer

  private let first: [UInt8]
  private let second: [UInt8]
  private let delayMs: Int64
  private var peerPrologue: [UInt8] = []
  private var didWrite = false

  init(first: [UInt8], second: [UInt8], delayMs: Int64 = 50) {
    self.first = first
    self.second = second
    self.delayMs = delayMs
  }

  func channelRead(context: ChannelHandlerContext, data: NIOAny) {
    guard !didWrite else {
      return
    }
    var buffer = unwrapInboundIn(data)
    do {
      guard try consumePeerLinkPrologue(&buffer, pending: &peerPrologue, fdFramed: false) else {
        return
      }
    } catch {
      context.fireErrorCaught(error)
      return
    }
    didWrite = true

    var firstBuffer = context.channel.allocator.buffer(capacity: voxLinkPrologueLen + first.count)
    firstBuffer.writeBytes(voxLinkPrologue(fdCapable: false))
    firstBuffer.writeBytes(first)
    context.writeAndFlush(NIOAny(firstBuffer), promise: nil)

    let delayedWrite = DelayedRawWrite(context: context, bytes: second)
    context.eventLoop.scheduleTask(in: .milliseconds(delayMs)) {
      delayedWrite.runAndClose()
    }
    context.fireChannelActive()
  }
}

private final class DelayedRawWrite: @unchecked Sendable {
  private let context: ChannelHandlerContext
  private let bytes: [UInt8]

  init(context: ChannelHandlerContext, bytes: [UInt8]) {
    self.context = context
    self.bytes = bytes
  }

  func runAndClose() {
    var buffer = context.channel.allocator.buffer(capacity: bytes.count)
    buffer.writeBytes(bytes)
    context.writeAndFlush(NIOAny(buffer), promise: nil)
    context.close(promise: nil)
  }
}

private final class WriteFramesThenCloseHandler: ChannelInboundHandler, @unchecked Sendable {
  typealias InboundIn = ByteBuffer

  private let frames: [[UInt8]]
  private let fdFramed: Bool
  private var peerPrologue: [UInt8] = []
  private var didWrite = false

  init(frames: [[UInt8]], fdFramed: Bool = false) {
    self.frames = frames
    self.fdFramed = fdFramed
  }

  func channelRead(context: ChannelHandlerContext, data: NIOAny) {
    guard !didWrite else {
      return
    }
    var buffer = unwrapInboundIn(data)
    do {
      guard try consumePeerLinkPrologue(&buffer, pending: &peerPrologue, fdFramed: fdFramed) else {
        return
      }
    } catch {
      context.fireErrorCaught(error)
      return
    }
    didWrite = true

    let frameHeaderSize = fdFramed ? 8 : 4
    let capacity =
      voxLinkPrologueLen + frames.reduce(0) { $0 + frameHeaderSize + $1.count }
    var outbound = context.channel.allocator.buffer(capacity: capacity)
    outbound.writeBytes(voxLinkPrologue(fdCapable: fdFramed))
    for frame in frames {
      appendLengthPrefixedFrame(frame, to: &outbound, fdFramed: fdFramed)
    }
    let channel = context.channel
    context.writeAndFlush(NIOAny(outbound)).whenComplete { _ in
      channel.close(promise: nil)
    }
  }
}

private final class WriteRawThenCloseHandler: ChannelInboundHandler, @unchecked Sendable {
  typealias InboundIn = ByteBuffer

  private let bytes: [UInt8]
  private var peerPrologue: [UInt8] = []
  private var didWrite = false

  init(bytes: [UInt8]) {
    self.bytes = bytes
  }

  func channelRead(context: ChannelHandlerContext, data: NIOAny) {
    guard !didWrite else {
      return
    }
    var buffer = unwrapInboundIn(data)
    do {
      guard try consumePeerLinkPrologue(&buffer, pending: &peerPrologue, fdFramed: false) else {
        return
      }
    } catch {
      context.fireErrorCaught(error)
      return
    }
    didWrite = true

    var outbound = context.channel.allocator.buffer(capacity: voxLinkPrologueLen + bytes.count)
    outbound.writeBytes(voxLinkPrologue(fdCapable: false))
    outbound.writeBytes(bytes)
    let channel = context.channel
    context.writeAndFlush(NIOAny(outbound)).whenComplete { _ in
      channel.close(promise: nil)
    }
  }
}

private final class FirstWriteSuspensionHandler: ChannelOutboundHandler, @unchecked Sendable {
  typealias OutboundIn = ByteBuffer

  private let writesToPassBeforeHold: Int
  private var passedWrites = 0
  private var didHoldFirstWrite = false
  private var heldFirstWritePromise: EventLoopPromise<Void>?
  private var context: ChannelHandlerContext?

  init(writesToPassBeforeHold: Int = 0) {
    self.writesToPassBeforeHold = writesToPassBeforeHold
  }

  func handlerAdded(context: ChannelHandlerContext) {
    self.context = context
  }

  func write(context: ChannelHandlerContext, data: NIOAny, promise: EventLoopPromise<Void>?) {
    if passedWrites < writesToPassBeforeHold {
      passedWrites += 1
      context.write(data, promise: promise)
      return
    }
    if !didHoldFirstWrite {
      didHoldFirstWrite = true
      heldFirstWritePromise = promise
      context.write(data, promise: nil)
    } else {
      context.write(data, promise: promise)
    }
  }

  func flush(context: ChannelHandlerContext) {
    context.flush()
  }

  func failHeldFirstWriteWithCancellation() {
    guard let context else {
      return
    }
    context.eventLoop.execute {
      self.heldFirstWritePromise?.fail(CancellationError())
      self.heldFirstWritePromise = nil
    }
  }
}

private final class CaptureFramesHandler: ChannelInboundHandler, @unchecked Sendable {
  typealias InboundIn = [UInt8]

  private let capture: FrameCapture

  init(capture: FrameCapture) {
    self.capture = capture
  }

  func channelRead(context _: ChannelHandlerContext, data: NIOAny) {
    let frame = unwrapInboundIn(data)
    Task {
      await capture.record(frame)
    }
  }

  func errorCaught(context: ChannelHandlerContext, error _: Error) {
    context.close(promise: nil)
  }

  func channelInactive(context: ChannelHandlerContext) {
    Task {
      await capture.markInactive()
    }
    context.fireChannelInactive()
  }
}

private func startLocalServer(
  childChannelInitializer: @escaping @Sendable (Channel) -> EventLoopFuture<Void> = { channel in
    channel.pipeline.addHandler(WriteOnActiveHandler(bytes: transportAcceptBareBytes))
  }
) async throws -> LocalServer {
  let group = MultiThreadedEventLoopGroup(numberOfThreads: 1)
  let bootstrap = ServerBootstrap(group: group)
    .serverChannelOption(ChannelOptions.backlog, value: 8)
    .serverChannelOption(ChannelOptions.socketOption(.so_reuseaddr), value: 1)
    .childChannelInitializer(childChannelInitializer)

  let channel: Channel
  do {
    channel = try await bootstrap.bind(host: "127.0.0.1", port: 0).get()
  } catch {
    try? await group.shutdownGracefully()
    throw error
  }
  guard let port = channel.localAddress?.port else {
    if channel.isActive {
      try await channel.close()
    }
    try await group.shutdownGracefully()
    throw TransportError.connectionClosed
  }
  return LocalServer(group: group, channel: channel, port: port)
}

private func connectLinkWithSuspendedFirstWrite(host: String, port: Int) async throws
  -> (NIOFrameLink, FirstWriteSuspensionHandler)
{
  let frameLimit = FrameLimit(1024 * 1024)
  let group = MultiThreadedEventLoopGroup(numberOfThreads: 1)
  let firstWriteHandler = FirstWriteSuspensionHandler(writesToPassBeforeHold: 1)

  var rawContinuation: AsyncStream<Result<[UInt8], Error>>.Continuation!
  let rawStream = AsyncStream<Result<[UInt8], Error>> { continuation in
    rawContinuation = continuation
  }
  let rawHandler = RawFrameStreamHandler(continuation: rawContinuation!)

  let bootstrap = ClientBootstrap(group: group)
    .channelOption(ChannelOptions.socketOption(.so_keepalive), value: 1)
    .channelInitializer { channel in
      do {
        try channel.pipeline.syncOperations.addHandler(firstWriteHandler)
        try channel.pipeline.syncOperations.addHandler(
          ByteToMessageHandler(
            LengthPrefixDecoder(frameLimit: frameLimit, fdFramed: false)
          )
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
    return (
      NIOFrameLink(
        channel: channel,
        frameLimit: frameLimit,
        inboundStream: rawStream,
        owningGroup: group,
        fdFramed: false
      ),
      firstWriteHandler
    )
  } catch {
    try? await group.shutdownGracefully()
    throw error
  }
}

private func startLocalUnixServer(
  childChannelInitializer: @escaping @Sendable (Channel) -> EventLoopFuture<Void>
) async throws -> LocalUnixServer {
  let group = MultiThreadedEventLoopGroup(numberOfThreads: 1)
  let path = "\(NSTemporaryDirectory())vox-runtime-\(UUID().uuidString).sock"
  unlink(path)
  let bootstrap = ServerBootstrap(group: group)
    .serverChannelOption(ChannelOptions.backlog, value: 8)
    .childChannelInitializer(childChannelInitializer)

  let channel: Channel
  do {
    channel = try await bootstrap.bind(unixDomainSocketPath: path).get()
  } catch {
    try? await group.shutdownGracefully()
    unlink(path)
    throw error
  }
  return LocalUnixServer(group: group, channel: channel, path: path)
}

private func stopLocalServer(_ server: LocalServer) async {
  if server.channel.isActive {
    try? await server.channel.close()
  }
  try? await server.group.shutdownGracefully()
}

private func stopLocalUnixServer(_ server: LocalUnixServer) async {
  if server.channel.isActive {
    try? await server.channel.close()
  }
  try? await server.group.shutdownGracefully()
  unlink(server.path)
}

private actor TestLink: Link {
  func sendFrame(_: [UInt8]) async throws {}

  func recvFrame() async throws -> [UInt8]? {
    nil
  }

  func setMaxFrameSize(_: Int) async throws {}

  func close() async throws {}
}

@Suite(.serialized)
struct TransportTests {
  // r[verify link.split]
  @Test func singleLinkSourceYieldsOneFreshAttachment() async throws {
    let link = TestLink()
    let source = singleLinkSource(link)

    let first = try await source.nextLink()
    #expect(!first.hasCompletedPrologue)

    do {
      _ = try await source.nextLink()
      Issue.record("single link source yielded a second attachment")
    } catch let error as TransportError {
      guard case .protocolViolation(let message) = error else {
        Issue.record("unexpected error: \(error)")
        return
      }
      #expect(message == "single-use LinkSource exhausted")
    }
  }

  // r[verify link]
  // r[verify link.message]
  // r[verify link.message.empty]
  // r[verify link.order]
  // r[verify link.rx.recv]
  // r[verify link.rx.eof]
  // r[verify transport.stream]
  // r[verify transport.stream.kinds]
  @Test func tcpStreamLinkPreservesBoundariesOrderEmptyPayloadsAndEof() async throws {
    let server = try await startLocalServer { channel in
      channel.pipeline.addHandler(WriteFramesThenCloseHandler(frames: [[], [1], [2, 3]]))
    }
    do {
      let link = try await connectLink(host: "127.0.0.1", port: server.port)
      #expect(try await link.recvFrame() == [])
      #expect(try await link.recvFrame() == [1])
      #expect(try await link.recvFrame() == [2, 3])
      #expect(try await link.recvFrame() == nil)
      #expect(try await link.recvFrame() == nil)
      try? await link.close()
    } catch {
      await stopLocalServer(server)
      throw error
    }
    await stopLocalServer(server)
  }

  // r[verify link.tx.send]
  // r[verify link.tx.close]
  // r[verify link.tx.alloc.limits]
  @Test func tcpStreamLinkSendsFramesClosesAndRejectsOversizedPayloads() async throws {
    let capture = FrameCapture()
    let server = try await startLocalServer { channel in
      let frameLimit = FrameLimit(1024 * 1024)
      do {
        try channel.pipeline.syncOperations.addHandler(
          ByteToMessageHandler(
            LengthPrefixDecoder(frameLimit: frameLimit, fdFramed: false)
          )
        )
        try channel.pipeline.syncOperations.addHandler(CaptureFramesHandler(capture: capture))
        return channel.eventLoop.makeSucceededVoidFuture()
      } catch {
        return channel.eventLoop.makeFailedFuture(error)
      }
    }
    do {
      let link = try await connectLink(host: "127.0.0.1", port: server.port)
      try await link.setMaxFrameSize(2)
      do {
        try await link.sendFrame([1, 2, 3])
        Issue.record("oversized frame send unexpectedly succeeded")
      } catch let error as TransportError {
        guard case .frameEncoding(let message) = error else {
          Issue.record("unexpected send error: \(error)")
          await stopLocalServer(server)
          return
        }
        #expect(message == "Frame exceeds 2 bytes")
      }

      try await link.sendFrame([4, 5])
      try await link.sendFrame([])
      guard let frames = await capture.waitForFrameCount(2) else {
        Issue.record("server did not observe committed frames")
        await stopLocalServer(server)
        return
      }
      #expect(frames == [[4, 5], []])
      try await link.close()
      #expect(await capture.waitForInactive())
    } catch {
      await stopLocalServer(server)
      throw error
    }
    await stopLocalServer(server)
  }

  // r[verify link.tx.cancel-safe]
  @Test func tcpStreamLinkSendCancellationDoesNotPublishPartialFrameOrPoisonLaterSend()
    async throws
  {
    let capture = FrameCapture()
    let server = try await startLocalServer { channel in
      let frameLimit = FrameLimit(1024 * 1024)
      do {
        try channel.pipeline.syncOperations.addHandler(
          ByteToMessageHandler(
            LengthPrefixDecoder(frameLimit: frameLimit, fdFramed: false)
          )
        )
        try channel.pipeline.syncOperations.addHandler(CaptureFramesHandler(capture: capture))
        return channel.eventLoop.makeSucceededVoidFuture()
      } catch {
        return channel.eventLoop.makeFailedFuture(error)
      }
    }
    do {
      let (link, firstWriteHandler) = try await connectLinkWithSuspendedFirstWrite(
        host: "127.0.0.1", port: server.port)
      let firstFrame = Array(UInt8(0)..<UInt8(64))
      let sendTask = Task {
        try await link.sendFrame(firstFrame)
      }

      guard let framesAfterFirstWrite = await capture.waitForFrameCount(1) else {
        Issue.record("server did not observe the first committed frame")
        sendTask.cancel()
        firstWriteHandler.failHeldFirstWriteWithCancellation()
        try? await link.close()
        await stopLocalServer(server)
        return
      }
      #expect(framesAfterFirstWrite == [firstFrame])

      sendTask.cancel()
      firstWriteHandler.failHeldFirstWriteWithCancellation()
      do {
        try await sendTask.value
        Issue.record("cancelled send task unexpectedly completed successfully")
      } catch is CancellationError {
      } catch {
        Issue.record("cancelled send task failed with unexpected error: \(error)")
      }

      let secondFrame: [UInt8] = [9, 8, 7]
      try await link.sendFrame(secondFrame)
      guard let frames = await capture.waitForFrameCount(2) else {
        Issue.record("server did not observe a later frame after send cancellation")
        try? await link.close()
        await stopLocalServer(server)
        return
      }
      #expect(frames == [firstFrame, secondFrame])
      try? await link.close()
    } catch {
      await stopLocalServer(server)
      throw error
    }
    await stopLocalServer(server)
  }

  // r[verify rpc.transport.stream.cancel-safe-recv]
  @Test func tcpStreamLinkReceiveCancellationDoesNotCorruptPartialFrame() async throws {
    let payload: [UInt8] = [9, 8, 7, 6, 5]
    let rawFrame = lengthPrefixedFrameBytes(payload)
    let server = try await startLocalServer { channel in
      channel.pipeline.addHandler(
        WriteRawChunksThenCloseHandler(
          first: Array(rawFrame.prefix(5)),
          second: Array(rawFrame.dropFirst(5))
        ))
    }
    do {
      let link = try await connectLink(host: "127.0.0.1", port: server.port)
      let cancelledRecv = Task {
        try await link.recvFrame()
      }
      try await Task.sleep(nanoseconds: 20_000_000)
      cancelledRecv.cancel()
      do {
        let cancelledValue = try await cancelledRecv.value
        if cancelledValue != nil {
          Issue.record("cancelled recv consumed a complete frame")
        }
      } catch is CancellationError {
      } catch {
        Issue.record("cancelled recv failed with unexpected error: \(error)")
      }

      #expect(try await link.recvFrame() == payload)
      #expect(try await link.recvFrame() == nil)
      try? await link.close()
    } catch {
      await stopLocalServer(server)
      throw error
    }
    await stopLocalServer(server)
  }

  // r[verify link.rx.error]
  @Test func tcpStreamLinkReceiveErrorIsTerminal() async throws {
    let server = try await startLocalServer { channel in
      channel.pipeline.addHandler(WriteRawThenCloseHandler(bytes: [3, 0, 0, 0, 1]))
    }
    do {
      let link = try await connectLink(host: "127.0.0.1", port: server.port)
      do {
        _ = try await link.recvFrame()
        Issue.record("partial frame unexpectedly decoded")
      } catch let error as TransportError {
        guard case .frameDecoding(let message) = error else {
          Issue.record("unexpected receive error: \(error)")
          await stopLocalServer(server)
          return
        }
        #expect(message == "EOF with 5 trailing bytes and no complete frame")
      }
      #expect(try await link.recvFrame() == nil)
      try? await link.close()
    } catch {
      await stopLocalServer(server)
      throw error
    }
    await stopLocalServer(server)
  }

  // r[verify transport.stream.local]
  // r[verify transport.stream.kinds]
  @Test func unixStreamLinkConnectsToLocalSocketTransport() async throws {
    let server = try await startLocalUnixServer { channel in
      channel.pipeline.addHandler(WriteFramesThenCloseHandler(frames: [[7, 8, 9]], fdFramed: true))
    }
    do {
      let link = try await connectLink(unixPath: server.path)
      #expect(try await link.recvFrame() == [7, 8, 9])
      #expect(try await link.recvFrame() == nil)
      try? await link.close()
    } catch {
      await stopLocalUnixServer(server)
      throw error
    }
    await stopLocalUnixServer(server)
  }

  // r[verify transport.prologue]
  // r[verify transport.prologue.request]
  // r[verify transport.prologue.accept]
  @Test func connectEnablesSocketKeepalive() async throws {
    let server = try await startLocalServer()
    do {
      let link = try await connectLink(host: "127.0.0.1", port: server.port)
      try await performInitiatorLinkPrologue(link: link)
      let keepalive = try await link.socketKeepaliveEnabled()
      #expect(keepalive)
      try? await link.close()
    } catch {
      await stopLocalServer(server)
      throw error
    }
    await stopLocalServer(server)
  }

  // r[verify transport.prologue.reject-close]
  @Test func transportPrologueRejectsUnsupportedPrologue() async throws {
    let server = try await startLocalServer { channel in
      channel.pipeline.addHandler(
        WriteOnActiveHandler(bytes: encodeTransportRejectUnsupported()))
    }
    do {
      let link = try await connectLink(host: "127.0.0.1", port: server.port)
      do {
        try await performInitiatorLinkPrologue(link: link)
        Issue.record("connect unexpectedly accepted rejected transport prologue")
      } catch let error as TransportError {
        guard case .protocolViolation(let message) = error else {
          Issue.record("unexpected error: \(error)")
          try? await link.close()
          await stopLocalServer(server)
          return
        }
        #expect(message == "transport rejected unsupported prologue")
      }
      try? await link.close()
    } catch {
      await stopLocalServer(server)
      throw error
    }
    await stopLocalServer(server)
  }

  @Test func transportPrologueTimesOutWhenServerNeverReplies() async throws {
    let server = try await startLocalServer { channel in
      channel.eventLoop.makeSucceededFuture(())
    }
    do {
      do {
        _ = try await connect(
          host: "127.0.0.1",
          port: server.port,
          prologueTimeoutNs: 50_000_000
        )
        Issue.record("connect unexpectedly succeeded without transport prologue response")
      } catch let error as TransportError {
        guard case .protocolViolation(let message) = error else {
          Issue.record("unexpected error: \(error)")
          return
        }
        #expect(message == "transport prologue timed out")
      }
    }
    await stopLocalServer(server)
  }
}
