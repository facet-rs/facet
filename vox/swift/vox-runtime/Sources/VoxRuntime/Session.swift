import Foundation
import PhonSchema

public protocol ExpectedRootClient {
    static var voxServiceName: String { get }
}

public enum NoopClient: ExpectedRootClient {
    public static let voxServiceName = "Noop"
}

private func injectExpectedRootService(
    _ metadata: Metadata,
    serviceName: String
) -> Metadata {
    if metadata.metaHas("vox-service") {
        return metadata
    }
    return metadata.metaSetting("vox-service", .string(serviceName))
}

// r[impl session]
// r[impl connection.root]
public final class Session: @unchecked Sendable {
    public let role: Role
    public let rootConnection: Connection
    public let driver: Driver
    public let handle: SessionHandle
    public let peerMetadata: Metadata

    public var connection: Connection {
        rootConnection
    }

    init(
        role: Role,
        rootConnection: Connection,
        driver: Driver,
        handle: SessionHandle,
        peerMetadata: Metadata
    ) {
        self.role = role
        self.rootConnection = rootConnection
        self.driver = driver
        self.handle = handle
        self.peerMetadata = peerMetadata
    }

    public func run() async throws {
        try await driver.run()
    }

    /// Like `initiator(...)` but injects `vox-service:
    /// ExpectedClient.voxServiceName` into the session-establish
    /// metadata so the peer's per-service router dispatches the
    /// resulting root connection to the matching dispatcher. Use
    /// this when talking to a server that routes connections by
    /// service (e.g. `vox::acceptor_on(...).on_connection(factory)`),
    /// pairing it with the typed client returned by codegen:
    ///
    ///     let session = try await Session.initiator(
    ///         connector,
    ///         expecting: ProfilerClient.self,
    ///         dispatcher: NoopDispatcher()
    ///     )
    ///     let client = ProfilerClient(connection: session.connection)
    public static func initiator<ExpectedClient: ExpectedRootClient>(
        _ connector: some SessionConnector,
        expecting _: ExpectedClient.Type,
        dispatcher: any ServiceDispatcher,
        onConnection: (any ConnectionAcceptor)? = nil,
        keepalive: SessionKeepaliveConfig? = nil,
        metadata: Metadata = .null
    ) async throws -> Session {
        // r[impl rpc.session-setup]
        let metadata = injectExpectedRootService(
            metadata, serviceName: ExpectedClient.voxServiceName)
        return try await initiator(
            connector,
            dispatcher: dispatcher,
            onConnection: onConnection,
            keepalive: keepalive,
            metadata: metadata
        )
    }

    public static func initiator(
        _ connector: some SessionConnector,
        dispatcher: any ServiceDispatcher,
        onConnection: (any ConnectionAcceptor)? = nil,
        keepalive: SessionKeepaliveConfig? = nil,
        metadata: Metadata = .null
    ) async throws -> Session {
        // r[impl rpc.session-setup]
        let attachment = try await connector.openAttachment()
        let (connection, driver, handle, peerMetadata) =
            try await establishInitiator(
                attachment: attachment,
                dispatcher: dispatcher,
                connectionAcceptor: onConnection,
                keepalive: keepalive,
                metadata: metadata
            )
        return Session(
            role: .initiator,
            rootConnection: connection,
            driver: driver,
            handle: handle,
            peerMetadata: peerMetadata
        )
    }

    public static func acceptor<ExpectedClient: ExpectedRootClient>(
        _ connector: some SessionConnector,
        expecting _: ExpectedClient.Type,
        dispatcher: any ServiceDispatcher,
        onConnection: (any ConnectionAcceptor)? = nil,
        keepalive: SessionKeepaliveConfig? = nil,
        metadata: Metadata = .null
    ) async throws -> Session {
        // r[impl rpc.session-setup]
        let attachment = try await connector.openAttachment()
        return try await acceptFreshAttachment(
            attachment,
            expecting: ExpectedClient.self,
            dispatcher: dispatcher,
            onConnection: onConnection,
            keepalive: keepalive,
            metadata: metadata
        )
    }

    public static func acceptor(
        _ connector: some SessionConnector,
        dispatcher: any ServiceDispatcher,
        onConnection: (any ConnectionAcceptor)? = nil,
        keepalive: SessionKeepaliveConfig? = nil,
        metadata: Metadata = .null
    ) async throws -> Session {
        try await acceptor(
            connector,
            expecting: NoopClient.self,
            dispatcher: dispatcher,
            onConnection: onConnection,
            keepalive: keepalive,
            metadata: metadata
        )
    }

    public static func establishOverFreshLink(
        _ link: any Link,
        dispatcher: any ServiceDispatcher,
        onConnection: (any ConnectionAcceptor)? = nil,
        keepalive: SessionKeepaliveConfig? = nil
    ) async throws -> Session {
        // r[impl rpc.session-setup]
        let (connection, driver, handle, peerMetadata) =
            try await establishInitiator(
                attachment: .fresh(link),
                dispatcher: dispatcher,
                connectionAcceptor: onConnection,
                keepalive: keepalive
            )
        return Session(
            role: .initiator,
            rootConnection: connection,
            driver: driver,
            handle: handle,
            peerMetadata: peerMetadata
        )
    }

    public static func acceptFreshAttachment<ExpectedClient: ExpectedRootClient>(
        _ attachment: LinkAttachment,
        expecting _: ExpectedClient.Type,
        dispatcher: any ServiceDispatcher,
        onConnection: (any ConnectionAcceptor)? = nil,
        keepalive: SessionKeepaliveConfig? = nil,
        metadata: Metadata = .null
    ) async throws -> Session {
        // r[impl rpc.session-setup]
        let metadata = injectExpectedRootService(
            metadata, serviceName: ExpectedClient.voxServiceName)
        let (connection, driver, handle, peerMetadata) =
            try await establishAcceptor(
                attachment: attachment,
                dispatcher: dispatcher,
                connectionAcceptor: onConnection,
                keepalive: keepalive,
                metadata: metadata
            )
        return Session(
            role: .acceptor,
            rootConnection: connection,
            driver: driver,
            handle: handle,
            peerMetadata: peerMetadata
        )
    }

    public static func acceptFreshAttachment(
        _ attachment: LinkAttachment,
        dispatcher: any ServiceDispatcher,
        onConnection: (any ConnectionAcceptor)? = nil,
        keepalive: SessionKeepaliveConfig? = nil,
        metadata: Metadata = .null
    ) async throws -> Session {
        try await acceptFreshAttachment(
            attachment,
            expecting: NoopClient.self,
            dispatcher: dispatcher,
            onConnection: onConnection,
            keepalive: keepalive,
            metadata: metadata
        )
    }

    public static func acceptFreshLink<ExpectedClient: ExpectedRootClient>(
        _ link: any Link,
        expecting _: ExpectedClient.Type,
        dispatcher: any ServiceDispatcher,
        onConnection: (any ConnectionAcceptor)? = nil,
        keepalive: SessionKeepaliveConfig? = nil,
        metadata: Metadata = .null
    ) async throws -> Session {
        try await acceptFreshAttachment(
            .fresh(link),
            expecting: ExpectedClient.self,
            dispatcher: dispatcher,
            onConnection: onConnection,
            keepalive: keepalive,
            metadata: metadata
        )
    }

    public static func acceptFreshLink(
        _ link: any Link,
        dispatcher: any ServiceDispatcher,
        onConnection: (any ConnectionAcceptor)? = nil,
        keepalive: SessionKeepaliveConfig? = nil,
        metadata: Metadata = .null
    ) async throws -> Session {
        try await acceptFreshLink(
            link,
            expecting: NoopClient.self,
            dispatcher: dispatcher,
            onConnection: onConnection,
            keepalive: keepalive,
            metadata: metadata
        )
    }

}
