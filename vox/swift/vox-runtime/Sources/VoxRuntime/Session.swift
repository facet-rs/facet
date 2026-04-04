import Foundation

public final class Session: @unchecked Sendable {
    public let role: Role
    public let rootConnection: Connection
    public let driver: Driver
    public let handle: SessionHandle
    public let peerMetadata: [MetadataEntry]
    let sessionResumeKey: [UInt8]?

    public var connection: Connection {
        rootConnection
    }

    init(
        role: Role,
        rootConnection: Connection,
        driver: Driver,
        handle: SessionHandle,
        peerMetadata: [MetadataEntry],
        sessionResumeKey: [UInt8]?
    ) {
        self.role = role
        self.rootConnection = rootConnection
        self.driver = driver
        self.handle = handle
        self.peerMetadata = peerMetadata
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
        let (connection, driver, handle, sessionResumeKey, peerMetadata) = try await establishInitiator(
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
            peerMetadata: peerMetadata,
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
        return try await acceptFreshAttachment(
            attachment,
            conduit: connector.transport,
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
        return try await acceptFreshAttachmentOrResume(
            attachment,
            conduit: connector.transport,
            registry: registry,
            dispatcher: dispatcher,
            acceptConnections: acceptConnections,
            keepalive: keepalive,
            resumable: resumable
        )
    }

    public static func establishOverFreshLink(
        _ link: any Link,
        conduit: ConduitKind = .bare,
        dispatcher: any ServiceDispatcher,
        acceptConnections: Bool = false,
        keepalive: DriverKeepaliveConfig? = nil,
        resumable: Bool = false
    ) async throws -> Session {
        let (connection, driver, handle, sessionResumeKey, peerMetadata) = try await establishInitiator(
            attachment: .fresh(link),
            transport: conduit,
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
            peerMetadata: peerMetadata,
            sessionResumeKey: sessionResumeKey
        )
    }

    @available(*, deprecated, renamed: "establishOverFreshLink(_:conduit:dispatcher:acceptConnections:keepalive:resumable:)")
    public static func initiatorOn(
        _ link: any Link,
        transport: ConduitKind = .bare,
        dispatcher: any ServiceDispatcher,
        acceptConnections: Bool = false,
        keepalive: DriverKeepaliveConfig? = nil,
        resumable: Bool = false
    ) async throws -> Session {
        try await establishOverFreshLink(
            link,
            conduit: transport,
            dispatcher: dispatcher,
            acceptConnections: acceptConnections,
            keepalive: keepalive,
            resumable: resumable
        )
    }

    public static func acceptFreshAttachment(
        _ attachment: LinkAttachment,
        conduit: ConduitKind? = nil,
        dispatcher: any ServiceDispatcher,
        acceptConnections: Bool = false,
        keepalive: DriverKeepaliveConfig? = nil,
        resumable: Bool = false
    ) async throws -> Session {
        let selectedConduit = conduit ?? attachment.negotiatedConduit ?? .bare
        let (connection, driver, handle, sessionResumeKey, peerMetadata) = try await establishAcceptor(
            attachment: attachment,
            transport: selectedConduit,
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
            peerMetadata: peerMetadata,
            sessionResumeKey: sessionResumeKey
        )
    }

    @available(*, deprecated, renamed: "acceptFreshAttachment(_:conduit:dispatcher:acceptConnections:keepalive:resumable:)")
    public static func acceptorOn(
        _ attachment: LinkAttachment,
        transport: ConduitKind? = nil,
        dispatcher: any ServiceDispatcher,
        acceptConnections: Bool = false,
        keepalive: DriverKeepaliveConfig? = nil,
        resumable: Bool = false
    ) async throws -> Session {
        try await acceptFreshAttachment(
            attachment,
            conduit: transport,
            dispatcher: dispatcher,
            acceptConnections: acceptConnections,
            keepalive: keepalive,
            resumable: resumable
        )
    }

    public static func acceptFreshLink(
        _ link: any Link,
        conduit: ConduitKind = .bare,
        dispatcher: any ServiceDispatcher,
        acceptConnections: Bool = false,
        keepalive: DriverKeepaliveConfig? = nil,
        resumable: Bool = false
    ) async throws -> Session {
        try await acceptFreshAttachment(
            .fresh(link),
            conduit: conduit,
            dispatcher: dispatcher,
            acceptConnections: acceptConnections,
            keepalive: keepalive,
            resumable: resumable
        )
    }

    @available(*, deprecated, renamed: "acceptFreshLink(_:conduit:dispatcher:acceptConnections:keepalive:resumable:)")
    public static func acceptorOn(
        _ link: any Link,
        transport: ConduitKind = .bare,
        dispatcher: any ServiceDispatcher,
        acceptConnections: Bool = false,
        keepalive: DriverKeepaliveConfig? = nil,
        resumable: Bool = false
    ) async throws -> Session {
        try await acceptFreshLink(
            link,
            conduit: transport,
            dispatcher: dispatcher,
            acceptConnections: acceptConnections,
            keepalive: keepalive,
            resumable: resumable
        )
    }

    public static func acceptFreshAttachmentOrResume(
        _ attachment: LinkAttachment,
        conduit: ConduitKind? = nil,
        registry: SessionRegistry,
        dispatcher: any ServiceDispatcher,
        acceptConnections: Bool = false,
        keepalive: DriverKeepaliveConfig? = nil,
        resumable: Bool = false
    ) async throws -> SessionAcceptOutcome {
        let selectedConduit = conduit ?? attachment.negotiatedConduit ?? .bare
        let readyAttachment: LinkAttachment
        if attachment.negotiatedConduit == nil {
            let negotiatedTransport = try await performAcceptorTransportPrologue(
                transport: attachment.link,
                supportedConduit: selectedConduit
            )
            guard negotiatedTransport == selectedConduit else {
                throw TransportError.protocolViolation(
                    "transport negotiated \(negotiatedTransport) for requested \(selectedConduit)"
                )
            }
            readyAttachment = .negotiated(attachment.link, conduit: negotiatedTransport)
        } else {
            readyAttachment = attachment
        }

        guard let firstBytes = try await readyAttachment.link.recvRawPrologue() else {
            throw ConnectionError.connectionClosed
        }
        let firstMessage = try HandshakeMessage.decodeCbor(firstBytes)
        guard case .hello(let hello) = firstMessage else {
            throw ConnectionError.handshakeFailed("expected Hello")
        }

        let prefetchedAttachment = LinkAttachment(
            link: PrefetchedLink(firstRawPrologue: firstBytes, base: readyAttachment.link),
            state: .conduitNegotiated(selectedConduit)
        )

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

        let session = try await acceptFreshAttachment(
            prefetchedAttachment,
            conduit: selectedConduit,
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

    @available(*, deprecated, renamed: "acceptFreshAttachmentOrResume(_:conduit:registry:dispatcher:acceptConnections:keepalive:resumable:)")
    public static func acceptorOnOrResume(
        _ attachment: LinkAttachment,
        transport: ConduitKind? = nil,
        registry: SessionRegistry,
        dispatcher: any ServiceDispatcher,
        acceptConnections: Bool = false,
        keepalive: DriverKeepaliveConfig? = nil,
        resumable: Bool = false
    ) async throws -> SessionAcceptOutcome {
        try await acceptFreshAttachmentOrResume(
            attachment,
            conduit: transport,
            registry: registry,
            dispatcher: dispatcher,
            acceptConnections: acceptConnections,
            keepalive: keepalive,
            resumable: resumable
        )
    }

    public static func acceptFreshLinkOrResume(
        _ link: any Link,
        conduit: ConduitKind = .bare,
        registry: SessionRegistry,
        dispatcher: any ServiceDispatcher,
        acceptConnections: Bool = false,
        keepalive: DriverKeepaliveConfig? = nil,
        resumable: Bool = false
    ) async throws -> SessionAcceptOutcome {
        try await acceptFreshAttachmentOrResume(
            .fresh(link),
            conduit: conduit,
            registry: registry,
            dispatcher: dispatcher,
            acceptConnections: acceptConnections,
            keepalive: keepalive,
            resumable: resumable
        )
    }

    @available(*, deprecated, renamed: "acceptFreshLinkOrResume(_:conduit:registry:dispatcher:acceptConnections:keepalive:resumable:)")
    public static func acceptorOnOrResume(
        _ link: any Link,
        transport: ConduitKind = .bare,
        registry: SessionRegistry,
        dispatcher: any ServiceDispatcher,
        acceptConnections: Bool = false,
        keepalive: DriverKeepaliveConfig? = nil,
        resumable: Bool = false
    ) async throws -> SessionAcceptOutcome {
        try await acceptFreshLinkOrResume(
            link,
            conduit: transport,
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
