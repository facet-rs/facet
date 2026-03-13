import Foundation
@preconcurrency import NIO
@preconcurrency import NIOPosix
import Testing

@testable import RoamRuntime

private struct LocalServer {
    let group: MultiThreadedEventLoopGroup
    let channel: Channel
    let port: Int
}

private let transportAcceptBareBytes: [UInt8] = Array("ROTA".utf8) + [9, 0, 0, 0]

private final class WriteOnActiveHandler: ChannelInboundHandler, Sendable {
    typealias InboundIn = Never

    private let bytes: [UInt8]

    init(bytes: [UInt8]) {
        self.bytes = bytes
    }

    func channelActive(context: ChannelHandlerContext) {
        var buffer = context.channel.allocator.buffer(capacity: 4 + bytes.count)
        buffer.writeInteger(UInt32(bytes.count), endianness: .little)
        buffer.writeBytes(bytes)
        context.writeAndFlush(NIOAny(buffer), promise: nil)
        context.fireChannelActive()
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
        try await channel.close()
        try await group.shutdownGracefully()
        throw TransportError.connectionClosed
    }
    return LocalServer(group: group, channel: channel, port: port)
}

private func stopLocalServer(_ server: LocalServer) async {
    try? await server.channel.close()
    try? await server.group.shutdownGracefully()
}

struct TransportTests {
    @Test func connectEnablesSocketKeepalive() async throws {
        let server = try await startLocalServer()
        do {
            let transport = try await connect(host: "127.0.0.1", port: server.port)
            let keepalive = try await transport.socketKeepaliveEnabled()
            #expect(keepalive)
            try? await transport.close()
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
                    conduit: .bare,
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
