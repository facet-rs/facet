import Foundation

actor SessionResumeCoordinator {
    private struct ResumeRequest {
        let attachment: LinkAttachment
        let result: CheckedContinuation<Void, Error>
    }

    private let role: Role
    private let localRootSettings: ConnectionSettingsV7
    private let peerRootSettings: ConnectionSettingsV7
    private let transport: TransportConduitKind
    private let resumable: Bool
    private let sessionResumeKey: [UInt8]?
    private let recoverAttachment: (@Sendable () async throws -> LinkAttachment)?

    private var closed = false
    private var pendingResumes: [ResumeRequest] = []
    private var resumeWaiter: CheckedContinuation<ResumeRequest?, Never>?

    init(
        role: Role,
        localRootSettings: ConnectionSettingsV7,
        peerRootSettings: ConnectionSettingsV7,
        transport: TransportConduitKind,
        resumable: Bool,
        sessionResumeKey: [UInt8]?,
        recoverAttachment: (@Sendable () async throws -> LinkAttachment)?
    ) {
        self.role = role
        self.localRootSettings = localRootSettings
        self.peerRootSettings = peerRootSettings
        self.transport = transport
        self.resumable = resumable
        self.sessionResumeKey = sessionResumeKey
        self.recoverAttachment = recoverAttachment
    }

    func sessionHandle() -> SessionHandle {
        SessionHandle(coordinator: self)
    }

    func sessionResumeKeyValue() -> [UInt8]? {
        sessionResumeKey
    }

    func resume(_ attachment: LinkAttachment) async throws {
        guard resumable else {
            throw ConnectionError.protocolViolation(rule: "session is not resumable")
        }
        traceLog(.resume, "resume requested transport=\(transport)")
        try await enqueueResume(attachment)
    }

    func acceptResumedAttachment(_ attachment: LinkAttachment) async throws {
        guard resumable else {
            throw ConnectionError.protocolViolation(rule: "session is not resumable")
        }
        try await enqueueResume(attachment)
    }

    private func enqueueResume(_ attachment: LinkAttachment) async throws {
        traceLog(
            .resume,
            "enqueue pending=\(pendingResumes.count) waiter=\(resumeWaiter != nil)"
        )
        try await withCheckedThrowingContinuation { continuation in
            let request = ResumeRequest(attachment: attachment, result: continuation)
            if let waiter = resumeWaiter {
                resumeWaiter = nil
                traceLog(.resume, "resume delivered directly to parked waiter")
                waiter.resume(returning: request)
            } else {
                pendingResumes.append(request)
                traceLog(.resume, "resume queued pending=\(pendingResumes.count)")
            }
        }
        traceLog(.resume, "resume continuation completed")
    }

    func replacementConduit() async throws -> (any Conduit)? {
        guard resumable else {
            return nil
        }

        if let recoverAttachment {
            do {
                traceLog(.resume, "trying recoverAttachment")
                let attachment = try await recoverAttachment()
                traceLog(.resume, "recoverAttachment succeeded")
                return try await buildResumedConduit(from: attachment)
            } catch {
                traceLog(.resume, "recoverAttachment failed: \(String(describing: error))")
            }
        }

        while !closed {
            guard let pending = await nextResume() else {
                traceLog(.resume, "replacementConduit observed closed coordinator")
                return nil
            }
            do {
                let conduit = try await buildResumedConduit(from: pending.attachment)
                pending.result.resume()
                traceLog(.resume, "replacementConduit built resumed conduit")
                return conduit
            } catch {
                traceLog(.resume, "replacementConduit failed: \(String(describing: error))")
                pending.result.resume(throwing: error)
            }
        }

        return nil
    }

    func shutdown() {
        traceLog(
            .resume,
            "shutdown pending=\(pendingResumes.count) waiter=\(resumeWaiter != nil)"
        )
        closed = true
        let waiter = resumeWaiter
        resumeWaiter = nil
        waiter?.resume(returning: nil)
        let error = ConnectionError.connectionClosed
        let pending = pendingResumes
        pendingResumes.removeAll()
        for resume in pending {
            resume.result.resume(throwing: error)
        }
    }

    private func nextResume() async -> ResumeRequest? {
        if !pendingResumes.isEmpty {
            traceLog(.resume, "nextResume returning queued pending=\(pendingResumes.count)")
            return pendingResumes.removeFirst()
        }
        if closed {
            return nil
        }
        traceLog(.resume, "nextResume parking waiter")
        return await withCheckedContinuation { continuation in
            resumeWaiter = continuation
        }
    }

    private func buildResumedConduit(
        from attachment: LinkAttachment
    ) async throws -> any Conduit {
        guard let sessionResumeKey else {
            throw ConnectionError.protocolViolation(rule: "session is not resumable")
        }
        traceLog(.resume, "buildResumedConduit role=\(role)")

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
            recoverAttachment: recoverAttachment
        )
    }
}

final actor ResumableConduit: Conduit {
    private var conduit: (any Conduit)?
    private let coordinator: SessionResumeCoordinator
    private var closed = false
    private var maxFrameSize: Int?
    private var replacementTask: Task<(any Conduit)?, Error>?
    private var resumeGeneration: UInt64 = 0

    init(conduit: any Conduit, coordinator: SessionResumeCoordinator) {
        self.conduit = conduit
        self.coordinator = coordinator
    }

    func send(_ message: MessageV7) async throws {
        while true {
            if closed {
                throw ConnectionError.connectionClosed
            }

            let active = try await activeConduit()
            do {
                try await active.send(message)
                return
            } catch {
                conduit = nil
                guard try await ensureReplacementConduit() != nil else {
                    throw error
                }
            }
        }
    }

    func recv() async throws -> MessageV7? {
        while true {
            if closed {
                return nil
            }

            let active = try await activeConduit()
            do {
                if let message = try await active.recv() {
                    return message
                }
                conduit = nil
                guard try await ensureReplacementConduit() != nil else {
                    return nil
                }
            } catch {
                conduit = nil
                guard try await ensureReplacementConduit() != nil else {
                    throw error
                }
            }
        }
    }

    func setMaxFrameSize(_ size: Int) async throws {
        maxFrameSize = size
        try await conduit?.setMaxFrameSize(size)
    }

    func close() async throws {
        traceLog(.resume, "resumable conduit closing")
        closed = true
        await coordinator.shutdown()
        try await conduit?.close()
    }

    func currentResumeGeneration() -> UInt64 {
        resumeGeneration
    }

    private func activeConduit() async throws -> any Conduit {
        if let conduit {
            return conduit
        }
        traceLog(.resume, "active conduit missing, ensuring replacement")
        guard let replacement = try await ensureReplacementConduit() else {
            throw ConnectionError.connectionClosed
        }
        return replacement
    }

    private func ensureReplacementConduit() async throws -> (any Conduit)? {
        if let replacementTask {
            traceLog(.resume, "awaiting existing replacement task")
            return try await replacementTask.value
        }

        traceLog(.resume, "spawning replacement task")
        let task = Task<(any Conduit)?, Error> { [coordinator] in
            try await coordinator.replacementConduit()
        }
        replacementTask = task
        defer { replacementTask = nil }

        let replacement = try await task.value
        traceLog(.resume, "replacement task completed replacement=\(replacement != nil)")
        conduit = replacement
        if let replacement, let maxFrameSize {
            try await replacement.setMaxFrameSize(maxFrameSize)
        }
        if replacement != nil {
            resumeGeneration &+= 1
        }
        return replacement
    }
}
