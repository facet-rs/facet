import Foundation

actor SessionResumeCoordinator {
    private struct ResumeRequest {
        let conduit: any Conduit
        let result: CheckedContinuation<Void, Error>
    }

    private let role: Role
    private let localRootSettings: ConnectionSettingsV7
    private let peerRootSettings: ConnectionSettingsV7
    private let resumable: Bool
    private let sessionResumeKey: [UInt8]?
    private let recoverConduit: (@Sendable () async throws -> any Conduit)?

    private var closed = false
    private var disconnected = false
    private var pendingResumes: [ResumeRequest] = []
    private var resumeWaiter: CheckedContinuation<ResumeRequest?, Never>?

    init(
        role: Role,
        localRootSettings: ConnectionSettingsV7,
        peerRootSettings: ConnectionSettingsV7,
        resumable: Bool,
        sessionResumeKey: [UInt8]?,
        recoverConduit: (@Sendable () async throws -> any Conduit)?
    ) {
        self.role = role
        self.localRootSettings = localRootSettings
        self.peerRootSettings = peerRootSettings
        self.resumable = resumable
        self.sessionResumeKey = sessionResumeKey
        self.recoverConduit = recoverConduit
    }

    func sessionHandle() -> SessionHandle {
        SessionHandle(coordinator: self)
    }

    func sessionResumeKeyValue() -> [UInt8]? {
        sessionResumeKey
    }

    func resume(_ conduit: any Conduit) async throws {
        guard resumable else {
            throw ConnectionError.protocolViolation(rule: "session is not resumable")
        }
        try await enqueueResume(conduit)
    }

    func acceptResumedConduit(_ conduit: any Conduit) async throws {
        guard resumable else {
            throw ConnectionError.protocolViolation(rule: "session is not resumable")
        }
        try await enqueueResume(conduit)
    }

    private func enqueueResume(_ conduit: any Conduit) async throws {
        try await withCheckedThrowingContinuation { continuation in
            let request = ResumeRequest(conduit: conduit, result: continuation)
            if let waiter = resumeWaiter {
                resumeWaiter = nil
                waiter.resume(returning: request)
            } else {
                pendingResumes.append(request)
            }
        }
    }

    func replacementConduit() async throws -> (any Conduit)? {
        guard resumable else {
            return nil
        }

        disconnected = true
        if let recoverConduit {
            do {
                let conduit = try await recoverConduit()
                try await resumeOnConduit(conduit)
                disconnected = false
                return conduit
            } catch {
            }
        }

        while !closed {
            guard let pending = await nextResume() else {
                return nil
            }
            do {
                try await resumeOnConduit(pending.conduit)
                disconnected = false
                pending.result.resume()
                return pending.conduit
            } catch {
                pending.result.resume(throwing: error)
            }
        }

        return nil
    }

    func shutdown() {
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
            return pendingResumes.removeFirst()
        }
        if closed {
            return nil
        }
        return await withCheckedContinuation { continuation in
            resumeWaiter = continuation
        }
    }

    private func resumeOnConduit(_ conduit: any Conduit) async throws {
        guard let sessionResumeKey else {
            throw ConnectionError.protocolViolation(rule: "session is not resumable")
        }

        switch role {
        case .initiator:
            let metadata = appendSessionResumeKeyMetadata(
                appendRetrySupportMetadata([]),
                key: sessionResumeKey
            )
            try await conduit.send(
                .hello(
                    HelloV7(
                        version: 7,
                        connectionSettings: localRootSettings,
                        metadata: metadata
                    ))
            )
            let helloYourself = try await waitForHelloYourself(conduit)
            guard helloYourself.connectionSettings == peerRootSettings else {
                throw ConnectionError.protocolViolation(
                    rule: "peer root settings changed across session resume"
                )
            }
            guard let echoedKey = metadataSessionResumeKey(helloYourself.metadata),
                sessionResumeKeysEqual(echoedKey, sessionResumeKey)
            else {
                throw ConnectionError.protocolViolation(rule: "session resume key mismatch")
            }

        case .acceptor:
            let hello = try await waitForHello(conduit)
            guard hello.connectionSettings == peerRootSettings else {
                throw ConnectionError.protocolViolation(
                    rule: "peer root settings changed across session resume"
                )
            }
            guard let actualKey = metadataSessionResumeKey(hello.metadata),
                sessionResumeKeysEqual(actualKey, sessionResumeKey)
            else {
                throw ConnectionError.protocolViolation(rule: "session resume key mismatch")
            }

            let metadata = appendSessionResumeKeyMetadata(
                appendRetrySupportMetadata([]),
                key: sessionResumeKey
            )
            try await conduit.send(
                .helloYourself(
                    HelloYourselfV7(
                        connectionSettings: localRootSettings,
                        metadata: metadata
                    ))
            )
        }
    }
}

final actor ResumableConduit: Conduit {
    private var conduit: (any Conduit)?
    private let coordinator: SessionResumeCoordinator
    private var closed = false
    private var maxFrameSize: Int?
    private var replacementTask: Task<(any Conduit)?, Error>?

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
        closed = true
        await coordinator.shutdown()
        try await conduit?.close()
    }

    private func activeConduit() async throws -> any Conduit {
        if let conduit {
            return conduit
        }
        guard let replacement = try await ensureReplacementConduit() else {
            throw ConnectionError.connectionClosed
        }
        return replacement
    }

    private func ensureReplacementConduit() async throws -> (any Conduit)? {
        if let replacementTask {
            return try await replacementTask.value
        }

        let task = Task<(any Conduit)?, Error> { [coordinator] in
            try await coordinator.replacementConduit()
        }
        replacementTask = task
        defer { replacementTask = nil }

        let replacement = try await task.value
        conduit = replacement
        if let replacement, let maxFrameSize {
            try await replacement.setMaxFrameSize(maxFrameSize)
        }
        return replacement
    }
}
