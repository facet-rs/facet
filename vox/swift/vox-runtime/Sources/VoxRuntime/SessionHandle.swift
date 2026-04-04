import Foundation

/// Handle for session resumption operations.
///
/// When a session is resumable, this handle can be used to:
/// - Client-side: typically handled automatically via recovery callback
/// - Server-side: resume an existing session when a client reconnects with a known key
public final class SessionHandle: @unchecked Sendable {
    private let eventContinuation: AsyncStream<DriverEvent>.Continuation
    private let role: Role
    private let localRootSettings: ConnectionSettings
    private let peerRootSettings: ConnectionSettings
    private let transport: ConduitKind
    let sessionResumeKey: [UInt8]?

    init(
        eventContinuation: AsyncStream<DriverEvent>.Continuation,
        role: Role,
        localRootSettings: ConnectionSettings,
        peerRootSettings: ConnectionSettings,
        transport: ConduitKind,
        sessionResumeKey: [UInt8]?
    ) {
        self.eventContinuation = eventContinuation
        self.role = role
        self.localRootSettings = localRootSettings
        self.peerRootSettings = peerRootSettings
        self.transport = transport
        self.sessionResumeKey = sessionResumeKey
    }

    /// Resume the session with a new link (client-side).
    public func resume(_ link: any Link) async throws {
        try await resume(.initiator(link))
    }

    /// Resume the session with a new attachment (client-side).
    public func resume(_ attachment: LinkAttachment) async throws {
        let conduit = try await buildResumedConduit(from: attachment)
        eventContinuation.yield(.resumeConduit(conduit))
    }

    /// Accept a resumed link from a reconnecting client (server-side).
    public func acceptResumedLink(_ link: any Link) async throws {
        try await acceptResumedAttachment(.init(link: link))
    }

    /// Accept a resumed attachment from a reconnecting client (server-side).
    public func acceptResumedAttachment(_ attachment: LinkAttachment) async throws {
        let conduit = try await buildResumedConduit(from: attachment)
        eventContinuation.yield(.resumeConduit(conduit))
    }

    /// Shutdown the session.
    public func shutdown() {
        eventContinuation.finish()
    }

    private func buildResumedConduit(from attachment: LinkAttachment) async throws -> any Conduit {
        guard let sessionResumeKey else {
            throw ConnectionError.protocolViolation(rule: "session is not resumable")
        }

        switch role {
        case .initiator:
            let handshake = try await performInitiatorHandshake(
                link: attachment.link,
                maxPayloadSize: 1024 * 1024,
                maxConcurrentRequests: localRootSettings.maxConcurrentRequests,
                resumable: true,
                resumeKey: sessionResumeKey
            )
            guard handshake.localRootSettings == localRootSettings else {
                throw ConnectionError.protocolViolation(
                    rule: "local root settings changed across session resume"
                )
            }
            guard handshake.peerRootSettings == peerRootSettings else {
                throw ConnectionError.protocolViolation(
                    rule: "peer root settings changed across session resume"
                )
            }
            guard let echoedKey = handshake.sessionResumeKey,
                sessionResumeKeysEqual(echoedKey, sessionResumeKey)
            else {
                throw ConnectionError.protocolViolation(rule: "session resume key mismatch")
            }

        case .acceptor:
            let handshake = try await performAcceptorHandshake(
                link: attachment.link,
                maxPayloadSize: 1024 * 1024,
                maxConcurrentRequests: localRootSettings.maxConcurrentRequests,
                resumable: false,
                expectedResumeKey: sessionResumeKey
            )
            guard handshake.localRootSettings == localRootSettings else {
                throw ConnectionError.protocolViolation(
                    rule: "local root settings changed across session resume"
                )
            }
            guard handshake.peerRootSettings == peerRootSettings else {
                throw ConnectionError.protocolViolation(
                    rule: "peer root settings changed across session resume"
                )
            }
        }

        return try await buildEstablishedConduit(
            role: role,
            transport: transport,
            attachment: attachment,
            recoverAttachment: nil
        )
    }
}
