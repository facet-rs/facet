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

        let transport = try await connect(host: host, port: port)

        // r[impl message.hello.version] - Use v3 for metadata flags support.
        let hello = Hello.v3(maxPayloadSize: 1024 * 1024, initialChannelCredit: 64 * 1024)

        // r[impl core.conn.accept-required] - Check if we should accept incoming virtual connections.
        let acceptConnections = ProcessInfo.processInfo.environment["ACCEPT_CONNECTIONS"] == "1"

        let (_, driver) = try await establishAcceptor(
            transport: transport,
            ourHello: hello,
            dispatcher: dispatcher,
            acceptConnections: acceptConnections
        )

        try await driver.run()
    }
}

public enum ServerError: Error {
    case missingPeerAddr
    case invalidPeerAddr(String)
}
