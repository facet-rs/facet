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

private func startLocalServer() async throws -> LocalServer {
    let group = MultiThreadedEventLoopGroup(numberOfThreads: 1)
    let bootstrap = ServerBootstrap(group: group)
        .serverChannelOption(ChannelOptions.backlog, value: 8)
        .serverChannelOption(ChannelOptions.socketOption(.so_reuseaddr), value: 1)
        .childChannelInitializer { channel in
            channel.eventLoop.makeSucceededFuture(())
        }

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
}
