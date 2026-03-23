import Foundation
import VoxRuntime

/// Server helper for running as a spec-test subject.
public struct Server {
    public init() {}

    /// Run as a subject.
    ///
    /// The subject connects out to the harness named by `PEER_ADDR`, so the
    /// transport is client-side even though the subject is serving RPC methods.
    public func runSubject(dispatcher: any ServiceDispatcher) async throws {
        guard let peerAddr = ProcessInfo.processInfo.environment["PEER_ADDR"] else {
            throw ServerError.missingPeerAddr
        }

        guard let colonIdx = peerAddr.lastIndex(of: ":") else {
            throw ServerError.invalidPeerAddr(peerAddr)
        }

        let host = String(peerAddr[..<colonIdx])
        let portStr = String(peerAddr[peerAddr.index(after: colonIdx)...])
        guard let port = Int(portStr) else {
            throw ServerError.invalidPeerAddr(peerAddr)
        }

        let connector = TcpConnector(
            host: host,
            port: port,
            transport: ProcessInfo.processInfo.environment["SPEC_CONDUIT"] == "stable" ? .stable : .bare
        )

        // r[impl core.conn.accept-required] - Check if we should accept incoming virtual connections.
        let acceptConnections = ProcessInfo.processInfo.environment["ACCEPT_CONNECTIONS"] == "1"

        let session = try await Session.initiator(
            connector,
            dispatcher: dispatcher,
            acceptConnections: acceptConnections,
            resumable: false
        )

        try await session.run()
    }
}

public enum ServerError: Error {
    case missingPeerAddr
    case invalidPeerAddr(String)
}
