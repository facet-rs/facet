import Foundation

public protocol ExpectedRootClient {
    static var voxServiceName: String { get }
}

public enum NoopClient: ExpectedRootClient {
    public static let voxServiceName = "Noop"
}

private func injectExpectedRootService(
    _ metadata: [MetadataEntry],
    serviceName: String
) -> [MetadataEntry] {
    if metadata.contains(where: { $0.key == "vox-service" }) {
        return metadata
    }
    return [MetadataEntry(key: "vox-service", value: .string(serviceName), flags: 0)] + metadata
}

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
        keepalive: SessionKeepaliveConfig? = nil,
        resumable: Bool = true,
        metadata: [MetadataEntry] = []
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
        let (connection, driver, handle, sessionResumeKey, peerMetadata) =
            try await establishInitiator(
                attachment: attachment,
                transport: connector.transport,
                dispatcher: dispatcher,
                acceptConnections: acceptConnections,
                keepalive: keepalive,
                resumable: resumable,
                recoverAttachment: recoverAttachment,
                metadata: metadata
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

    public static func acceptor<ExpectedClient: ExpectedRootClient>(
        _ connector: some SessionConnector,
        expecting _: ExpectedClient.Type,
        dispatcher: any ServiceDispatcher,
        acceptConnections: Bool = false,
        keepalive: SessionKeepaliveConfig? = nil,
        resumable: Bool = false,
        metadata: [MetadataEntry] = []
    ) async throws -> Session {
        let attachment = try await connector.openAttachment()
        return try await acceptFreshAttachment(
            attachment,
            conduit: connector.transport,
            expecting: ExpectedClient.self,
            dispatcher: dispatcher,
            acceptConnections: acceptConnections,
            keepalive: keepalive,
            resumable: resumable,
            metadata: metadata
        )
    }

    public static func acceptor(
        _ connector: some SessionConnector,
        dispatcher: any ServiceDispatcher,
        acceptConnections: Bool = false,
        keepalive: SessionKeepaliveConfig? = nil,
        resumable: Bool = false,
        metadata: [MetadataEntry] = []
    ) async throws -> Session {
        try await acceptor(
            connector,
            expecting: NoopClient.self,
            dispatcher: dispatcher,
            acceptConnections: acceptConnections,
            keepalive: keepalive,
            resumable: resumable,
            metadata: metadata
        )
    }

    public static func acceptorOrResume<ExpectedClient: ExpectedRootClient>(
        _ connector: some SessionConnector,
        expecting _: ExpectedClient.Type,
        registry: SessionRegistry,
        dispatcher: any ServiceDispatcher,
        acceptConnections: Bool = false,
        keepalive: SessionKeepaliveConfig? = nil,
        resumable: Bool = false,
        metadata: [MetadataEntry] = []
    ) async throws -> SessionAcceptOutcome {
        let attachment = try await connector.openAttachment()
        return try await acceptFreshAttachmentOrResume(
            attachment,
            conduit: connector.transport,
            expecting: ExpectedClient.self,
            registry: registry,
            dispatcher: dispatcher,
            acceptConnections: acceptConnections,
            keepalive: keepalive,
            resumable: resumable,
            metadata: metadata
        )
    }

    public static func acceptorOrResume(
        _ connector: some SessionConnector,
        registry: SessionRegistry,
        dispatcher: any ServiceDispatcher,
        acceptConnections: Bool = false,
        keepalive: SessionKeepaliveConfig? = nil,
        resumable: Bool = false,
        metadata: [MetadataEntry] = []
    ) async throws -> SessionAcceptOutcome {
        try await acceptorOrResume(
            connector,
            expecting: NoopClient.self,
            registry: registry,
            dispatcher: dispatcher,
            acceptConnections: acceptConnections,
            keepalive: keepalive,
            resumable: resumable,
            metadata: metadata
        )
    }

    public static func establishOverFreshLink(
        _ link: any Link,
        conduit: ConduitKind = .bare,
        dispatcher: any ServiceDispatcher,
        acceptConnections: Bool = false,
        keepalive: SessionKeepaliveConfig? = nil,
        resumable: Bool = false
    ) async throws -> Session {
        let (connection, driver, handle, sessionResumeKey, peerMetadata) =
            try await establishInitiator(
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

    public static func acceptFreshAttachment<ExpectedClient: ExpectedRootClient>(
        _ attachment: LinkAttachment,
        conduit: ConduitKind? = nil,
        expecting _: ExpectedClient.Type,
        dispatcher: any ServiceDispatcher,
        acceptConnections: Bool = false,
        keepalive: SessionKeepaliveConfig? = nil,
        resumable: Bool = false,
        metadata: [MetadataEntry] = []
    ) async throws -> Session {
        let selectedConduit = conduit ?? attachment.negotiatedConduit ?? .bare
        let metadata = injectExpectedRootService(
            metadata, serviceName: ExpectedClient.voxServiceName)
        let (connection, driver, handle, sessionResumeKey, peerMetadata) =
            try await establishAcceptor(
                attachment: attachment,
                transport: selectedConduit,
                dispatcher: dispatcher,
                acceptConnections: acceptConnections,
                keepalive: keepalive,
                resumable: resumable,
                metadata: metadata
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

    public static func acceptFreshAttachment(
        _ attachment: LinkAttachment,
        conduit: ConduitKind? = nil,
        dispatcher: any ServiceDispatcher,
        acceptConnections: Bool = false,
        keepalive: SessionKeepaliveConfig? = nil,
        resumable: Bool = false,
        metadata: [MetadataEntry] = []
    ) async throws -> Session {
        try await acceptFreshAttachment(
            attachment,
            conduit: conduit,
            expecting: NoopClient.self,
            dispatcher: dispatcher,
            acceptConnections: acceptConnections,
            keepalive: keepalive,
            resumable: resumable,
            metadata: metadata
        )
    }

    public static func acceptFreshLink<ExpectedClient: ExpectedRootClient>(
        _ link: any Link,
        conduit: ConduitKind = .bare,
        expecting _: ExpectedClient.Type,
        dispatcher: any ServiceDispatcher,
        acceptConnections: Bool = false,
        keepalive: SessionKeepaliveConfig? = nil,
        resumable: Bool = false,
        metadata: [MetadataEntry] = []
    ) async throws -> Session {
        try await acceptFreshAttachment(
            .fresh(link),
            conduit: conduit,
            expecting: ExpectedClient.self,
            dispatcher: dispatcher,
            acceptConnections: acceptConnections,
            keepalive: keepalive,
            resumable: resumable,
            metadata: metadata
        )
    }

    public static func acceptFreshLink(
        _ link: any Link,
        conduit: ConduitKind = .bare,
        dispatcher: any ServiceDispatcher,
        acceptConnections: Bool = false,
        keepalive: SessionKeepaliveConfig? = nil,
        resumable: Bool = false,
        metadata: [MetadataEntry] = []
    ) async throws -> Session {
        try await acceptFreshLink(
            link,
            conduit: conduit,
            expecting: NoopClient.self,
            dispatcher: dispatcher,
            acceptConnections: acceptConnections,
            keepalive: keepalive,
            resumable: resumable,
            metadata: metadata
        )
    }

    public static func acceptFreshAttachmentOrResume<ExpectedClient: ExpectedRootClient>(
        _ attachment: LinkAttachment,
        conduit: ConduitKind? = nil,
        expecting _: ExpectedClient.Type,
        registry: SessionRegistry,
        dispatcher: any ServiceDispatcher,
        acceptConnections: Bool = false,
        keepalive: SessionKeepaliveConfig? = nil,
        resumable: Bool = false,
        metadata: [MetadataEntry] = []
    ) async throws -> SessionAcceptOutcome {
        let selectedConduit = conduit ?? attachment.negotiatedConduit ?? .bare
        let readyAttachment: LinkAttachment
        if attachment.negotiatedConduit == nil {
            let negotiatedTransport = try await performAcceptorLinkPrologue(
                link: attachment.link,
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
            expecting: ExpectedClient.self,
            dispatcher: dispatcher,
            acceptConnections: acceptConnections,
            keepalive: keepalive,
            resumable: resumable,
            metadata: metadata
        )
        if let resumeKey = session.sessionResumeKey {
            registry.insert(resumeKey, handle: session.handle)
        }
        return .established(session)
    }

    public static func acceptFreshAttachmentOrResume(
        _ attachment: LinkAttachment,
        conduit: ConduitKind? = nil,
        registry: SessionRegistry,
        dispatcher: any ServiceDispatcher,
        acceptConnections: Bool = false,
        keepalive: SessionKeepaliveConfig? = nil,
        resumable: Bool = false,
        metadata: [MetadataEntry] = []
    ) async throws -> SessionAcceptOutcome {
        try await acceptFreshAttachmentOrResume(
            attachment,
            conduit: conduit,
            expecting: NoopClient.self,
            registry: registry,
            dispatcher: dispatcher,
            acceptConnections: acceptConnections,
            keepalive: keepalive,
            resumable: resumable,
            metadata: metadata
        )
    }

    public static func acceptFreshLinkOrResume<ExpectedClient: ExpectedRootClient>(
        _ link: any Link,
        conduit: ConduitKind = .bare,
        expecting _: ExpectedClient.Type,
        registry: SessionRegistry,
        dispatcher: any ServiceDispatcher,
        acceptConnections: Bool = false,
        keepalive: SessionKeepaliveConfig? = nil,
        resumable: Bool = false,
        metadata: [MetadataEntry] = []
    ) async throws -> SessionAcceptOutcome {
        try await acceptFreshAttachmentOrResume(
            .fresh(link),
            conduit: conduit,
            expecting: ExpectedClient.self,
            registry: registry,
            dispatcher: dispatcher,
            acceptConnections: acceptConnections,
            keepalive: keepalive,
            resumable: resumable,
            metadata: metadata
        )
    }

    public static func acceptFreshLinkOrResume(
        _ link: any Link,
        conduit: ConduitKind = .bare,
        registry: SessionRegistry,
        dispatcher: any ServiceDispatcher,
        acceptConnections: Bool = false,
        keepalive: SessionKeepaliveConfig? = nil,
        resumable: Bool = false,
        metadata: [MetadataEntry] = []
    ) async throws -> SessionAcceptOutcome {
        try await acceptFreshLinkOrResume(
            link,
            conduit: conduit,
            expecting: NoopClient.self,
            registry: registry,
            dispatcher: dispatcher,
            acceptConnections: acceptConnections,
            keepalive: keepalive,
            resumable: resumable,
            metadata: metadata
        )
    }

}

public enum SessionAcceptOutcome: Sendable {
    case established(Session)
    case resumed
}
