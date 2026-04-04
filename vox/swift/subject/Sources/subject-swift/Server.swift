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
        fputs("[subject-server] PEER_ADDR=\(peerAddr)\n", stderr)

        let transport: ConduitKind =
            ProcessInfo.processInfo.environment["SPEC_CONDUIT"] == "stable" ? .stable : .bare
        fputs("[subject-server] transport=\(transport)\n", stderr)

        // r[impl transport.unix]
        // r[impl hosted.peer-addr]
        let session: Session

        if peerAddr.hasPrefix("local://") {
            let path = String(peerAddr.dropFirst("local://".count))
            guard !path.isEmpty else {
                throw ServerError.invalidPeerAddr(peerAddr)
            }
            fputs("[subject-server] connector=unix path=\(path)\n", stderr)
            let connector = UnixConnector(path: path, transport: transport)
            // r[impl core.conn.accept-required] - Check if we should accept incoming virtual connections.
            let acceptConnections = ProcessInfo.processInfo.environment["ACCEPT_CONNECTIONS"] != "0"
            fputs("[subject-server] acceptConnections=\(acceptConnections)\n", stderr)
            fputs("[subject-server] creating initiator session\n", stderr)
            session = try await Session.initiator(
                connector,
                dispatcher: dispatcher,
                acceptConnections: acceptConnections,
                resumable: false
            )
        } else {
            guard let colonIdx = peerAddr.lastIndex(of: ":") else {
                throw ServerError.invalidPeerAddr(peerAddr)
            }

            let host = String(peerAddr[..<colonIdx])
            let portStr = String(peerAddr[peerAddr.index(after: colonIdx)...])
            guard let port = Int(portStr) else {
                throw ServerError.invalidPeerAddr(peerAddr)
            }

            fputs("[subject-server] connector=tcp host=\(host) port=\(port)\n", stderr)
            let connector = TcpConnector(host: host, port: port, transport: transport)
            // r[impl core.conn.accept-required] - Check if we should accept incoming virtual connections.
            let acceptConnections = ProcessInfo.processInfo.environment["ACCEPT_CONNECTIONS"] != "0"
            fputs("[subject-server] acceptConnections=\(acceptConnections)\n", stderr)
            fputs("[subject-server] creating initiator session\n", stderr)
            session = try await Session.initiator(
                connector,
                dispatcher: dispatcher,
                acceptConnections: acceptConnections,
                resumable: false
            )
        }

        let rootConnection = session.connection
        _ = rootConnection
        fputs("[subject-server] session created, root connection retained, entering run loop\n", stderr)
        try await session.run()
        fputs("[subject-server] run loop exited\n", stderr)
    }
}

public enum ServerError: Error {
    case missingPeerAddr
    case invalidPeerAddr(String)
}
