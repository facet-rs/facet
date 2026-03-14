import Foundation
import RoamRuntime

/// Server helper for running as a spec-test subject.
public struct Server {
    public init() {}

    /// Run as a subject (connects to PEER_ADDR, acts as acceptor).
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

        let session = try await Session.acceptor(
            connector,
            dispatcher: dispatcher,
            acceptConnections: acceptConnections
        )

        try await session.run()
    }
}

public enum ServerError: Error {
    case missingPeerAddr
    case invalidPeerAddr(String)
}
