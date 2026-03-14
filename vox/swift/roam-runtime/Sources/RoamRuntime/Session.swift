import Foundation

public final class Session: @unchecked Sendable {
    public let role: Role
    public let rootConnection: Connection
    public let driver: Driver
    public let handle: SessionHandle
    let sessionResumeKey: [UInt8]?

    public var connection: Connection {
        rootConnection
    }

    init(
        role: Role,
        rootConnection: Connection,
        driver: Driver,
        handle: SessionHandle,
        sessionResumeKey: [UInt8]?
    ) {
        self.role = role
        self.rootConnection = rootConnection
        self.driver = driver
        self.handle = handle
        self.sessionResumeKey = sessionResumeKey
    }

    public func run() async throws {
        try await driver.run()
    }

    public static func initiator(
        _ connector: some SessionConnector,
        dispatcher: any ServiceDispatcher,
        acceptConnections: Bool = false,
        keepalive: DriverKeepaliveConfig? = nil,
        resumable: Bool = false
    ) async throws -> Session {
        let conduit = try await connector.openConduit()
        let recoverConduit: (@Sendable () async throws -> any Conduit)?
        if resumable {
            recoverConduit = { @Sendable in
                try await connector.openConduit()
            }
        } else {
            recoverConduit = nil
        }
        let (connection, driver, handle, sessionResumeKey) = try await establishInitiator(
            conduit: conduit,
            dispatcher: dispatcher,
            acceptConnections: acceptConnections,
            keepalive: keepalive,
            resumable: resumable,
            recoverConduit: recoverConduit
        )
        return Session(
            role: .initiator,
            rootConnection: connection,
            driver: driver,
            handle: handle,
            sessionResumeKey: sessionResumeKey
        )
    }

    public static func acceptor(
        _ connector: some SessionConnector,
        dispatcher: any ServiceDispatcher,
        acceptConnections: Bool = false,
        keepalive: DriverKeepaliveConfig? = nil,
        resumable: Bool = false
    ) async throws -> Session {
        let conduit = try await connector.openConduit()
        return try await acceptorOn(
            conduit,
            dispatcher: dispatcher,
            acceptConnections: acceptConnections,
            keepalive: keepalive,
            resumable: resumable
        )
    }

    public static func acceptorOrResume(
        _ connector: some SessionConnector,
        registry: SessionRegistry,
        dispatcher: any ServiceDispatcher,
        acceptConnections: Bool = false,
        keepalive: DriverKeepaliveConfig? = nil,
        resumable: Bool = false
    ) async throws -> SessionAcceptOutcome {
        let conduit = try await connector.openConduit()
        return try await acceptorOnOrResume(
            conduit,
            registry: registry,
            dispatcher: dispatcher,
            acceptConnections: acceptConnections,
            keepalive: keepalive,
            resumable: resumable
        )
    }

    public static func initiatorOn(
        _ conduit: any Conduit,
        dispatcher: any ServiceDispatcher,
        acceptConnections: Bool = false,
        keepalive: DriverKeepaliveConfig? = nil,
        resumable: Bool = false
    ) async throws -> Session {
        let (connection, driver, handle, sessionResumeKey) = try await establishInitiator(
            conduit: conduit,
            dispatcher: dispatcher,
            acceptConnections: acceptConnections,
            keepalive: keepalive,
            resumable: resumable
        )
        return Session(
            role: .initiator,
            rootConnection: connection,
            driver: driver,
            handle: handle,
            sessionResumeKey: sessionResumeKey
        )
    }

    public static func acceptorOn(
        _ conduit: any Conduit,
        dispatcher: any ServiceDispatcher,
        acceptConnections: Bool = false,
        keepalive: DriverKeepaliveConfig? = nil,
        resumable: Bool = false
    ) async throws -> Session {
        let (connection, driver, handle, sessionResumeKey) = try await establishAcceptor(
            conduit: conduit,
            dispatcher: dispatcher,
            acceptConnections: acceptConnections,
            keepalive: keepalive,
            resumable: resumable
        )
        return Session(
            role: .acceptor,
            rootConnection: connection,
            driver: driver,
            handle: handle,
            sessionResumeKey: sessionResumeKey
        )
    }

    public static func acceptorOnOrResume(
        _ conduit: any Conduit,
        registry: SessionRegistry,
        dispatcher: any ServiceDispatcher,
        acceptConnections: Bool = false,
        keepalive: DriverKeepaliveConfig? = nil,
        resumable: Bool = false
    ) async throws -> SessionAcceptOutcome {
        guard let first = try await conduit.recv() else {
            throw ConnectionError.connectionClosed
        }
        guard case .hello(let hello) = first.payload else {
            throw ConnectionError.handshakeFailed("expected Hello")
        }

        if let resumeKey = metadataSessionResumeKey(hello.metadata) {
            guard let handle = registry.get(resumeKey) else {
                throw ConnectionError.protocolViolation(rule: "unknown session resume key")
            }
            do {
                try await handle.acceptResumedConduit(
                    PrefetchedConduit(firstMessage: first, base: conduit)
                )
            } catch {
                registry.remove(resumeKey)
                throw error
            }
            return .resumed
        }

        let session = try await acceptorOn(
            PrefetchedConduit(firstMessage: first, base: conduit),
            dispatcher: dispatcher,
            acceptConnections: acceptConnections,
            keepalive: keepalive,
            resumable: resumable
        )
        if let resumeKey = session.sessionResumeKey {
            registry.insert(resumeKey, handle: session.handle)
        }
        return .established(session)
    }
}

public enum SessionAcceptOutcome: Sendable {
    case established(Session)
    case resumed
}
