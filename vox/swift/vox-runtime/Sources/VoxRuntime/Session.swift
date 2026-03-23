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
        resumable: Bool = true
    ) async throws -> Session {
        let attachment = try await connector.openAttachment()
        let recoverAttachment: (@Sendable () async throws -> LinkAttachment)?
        if resumable {
            recoverAttachment = { @Sendable in
                try await connector.openAttachment()
            }
        } else {
            recoverAttachment = nil
        }
        let (connection, driver, handle, sessionResumeKey) = try await establishInitiator(
            attachment: attachment,
            transport: connector.transport,
            dispatcher: dispatcher,
            acceptConnections: acceptConnections,
            keepalive: keepalive,
            resumable: resumable,
            recoverAttachment: recoverAttachment
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
        let attachment = try await connector.openAttachment()
        return try await acceptorOn(
            attachment,
            transport: connector.transport,
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
        let attachment = try await connector.openAttachment()
        return try await acceptorOnOrResume(
            attachment,
            transport: connector.transport,
            registry: registry,
            dispatcher: dispatcher,
            acceptConnections: acceptConnections,
            keepalive: keepalive,
            resumable: resumable
        )
    }

    public static func initiatorOn(
        _ link: any Link,
        transport: TransportConduitKind = .bare,
        dispatcher: any ServiceDispatcher,
        acceptConnections: Bool = false,
        keepalive: DriverKeepaliveConfig? = nil,
        resumable: Bool = false
    ) async throws -> Session {
        let (connection, driver, handle, sessionResumeKey) = try await establishInitiator(
            attachment: .initiator(link),
            transport: transport,
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
        _ attachment: LinkAttachment,
        transport: TransportConduitKind? = nil,
        dispatcher: any ServiceDispatcher,
        acceptConnections: Bool = false,
        keepalive: DriverKeepaliveConfig? = nil,
        resumable: Bool = false
    ) async throws -> Session {
        let selectedTransport = transport ?? (attachment.clientHello == nil ? .bare : .stable)
        let (connection, driver, handle, sessionResumeKey) = try await establishAcceptor(
            attachment: attachment,
            transport: selectedTransport,
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

    public static func acceptorOn(
        _ link: any Link,
        transport: TransportConduitKind = .bare,
        dispatcher: any ServiceDispatcher,
        acceptConnections: Bool = false,
        keepalive: DriverKeepaliveConfig? = nil,
        resumable: Bool = false
    ) async throws -> Session {
        try await acceptorOn(
            .init(link: link),
            transport: transport,
            dispatcher: dispatcher,
            acceptConnections: acceptConnections,
            keepalive: keepalive,
            resumable: resumable
        )
    }

    public static func acceptorOnOrResume(
        _ attachment: LinkAttachment,
        transport: TransportConduitKind? = nil,
        registry: SessionRegistry,
        dispatcher: any ServiceDispatcher,
        acceptConnections: Bool = false,
        keepalive: DriverKeepaliveConfig? = nil,
        resumable: Bool = false
    ) async throws -> SessionAcceptOutcome {
        guard let firstBytes = try await attachment.link.recvRawPrologue() else {
            throw ConnectionError.connectionClosed
        }
        let firstMessage = try HandshakeMessage.decodeCbor(firstBytes)
        guard case .hello(let hello) = firstMessage else {
            throw ConnectionError.handshakeFailed("expected Hello")
        }

        let prefetchedAttachment = LinkAttachment(
            link: PrefetchedLink(firstRawPrologue: firstBytes, base: attachment.link),
            clientHello: attachment.clientHello
        )
        let selectedTransport = transport ?? (attachment.clientHello == nil ? .bare : .stable)

        if let resumeKey = hello.resumeKey?.bytes {
            guard let handle = registry.get(resumeKey) else {
                throw ConnectionError.protocolViolation(rule: "unknown session resume key")
            }
            do {
                try await handle.acceptResumedAttachment(prefetchedAttachment)
            } catch {
                registry.remove(resumeKey)
                throw error
            }
            return .resumed
        }

        let session = try await acceptorOn(
            prefetchedAttachment,
            transport: selectedTransport,
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

    public static func acceptorOnOrResume(
        _ link: any Link,
        transport: TransportConduitKind = .bare,
        registry: SessionRegistry,
        dispatcher: any ServiceDispatcher,
        acceptConnections: Bool = false,
        keepalive: DriverKeepaliveConfig? = nil,
        resumable: Bool = false
    ) async throws -> SessionAcceptOutcome {
        try await acceptorOnOrResume(
            .init(link: link),
            transport: transport,
            registry: registry,
            dispatcher: dispatcher,
            acceptConnections: acceptConnections,
            keepalive: keepalive,
            resumable: resumable
        )
    }
}

public enum SessionAcceptOutcome: Sendable {
    case established(Session)
    case resumed
}
