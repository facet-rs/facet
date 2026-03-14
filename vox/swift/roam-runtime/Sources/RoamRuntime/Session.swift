import Foundation

public final class Session: @unchecked Sendable {
    public let role: Role
    public let rootConnection: Connection
    public let driver: Driver

    public var connection: Connection {
        rootConnection
    }

    init(role: Role, rootConnection: Connection, driver: Driver) {
        self.role = role
        self.rootConnection = rootConnection
        self.driver = driver
    }

    public func run() async throws {
        try await driver.run()
    }

    public static func initiator(
        _ connector: some SessionConnector,
        dispatcher: any ServiceDispatcher,
        acceptConnections: Bool = false,
        keepalive: DriverKeepaliveConfig? = nil
    ) async throws -> Session {
        let conduit = try await connector.openConduit()
        return try await initiatorOn(
            conduit,
            dispatcher: dispatcher,
            acceptConnections: acceptConnections,
            keepalive: keepalive
        )
    }

    public static func acceptor(
        _ connector: some SessionConnector,
        dispatcher: any ServiceDispatcher,
        acceptConnections: Bool = false,
        keepalive: DriverKeepaliveConfig? = nil
    ) async throws -> Session {
        let conduit = try await connector.openConduit()
        return try await acceptorOn(
            conduit,
            dispatcher: dispatcher,
            acceptConnections: acceptConnections,
            keepalive: keepalive
        )
    }

    public static func initiatorOn(
        _ conduit: any Conduit,
        dispatcher: any ServiceDispatcher,
        acceptConnections: Bool = false,
        keepalive: DriverKeepaliveConfig? = nil
    ) async throws -> Session {
        let (connection, driver) = try await establishInitiator(
            conduit: conduit,
            dispatcher: dispatcher,
            acceptConnections: acceptConnections,
            keepalive: keepalive
        )
        return Session(role: .initiator, rootConnection: connection, driver: driver)
    }

    public static func acceptorOn(
        _ conduit: any Conduit,
        dispatcher: any ServiceDispatcher,
        acceptConnections: Bool = false,
        keepalive: DriverKeepaliveConfig? = nil
    ) async throws -> Session {
        let (connection, driver) = try await establishAcceptor(
            conduit: conduit,
            dispatcher: dispatcher,
            acceptConnections: acceptConnections,
            keepalive: keepalive
        )
        return Session(role: .acceptor, rootConnection: connection, driver: driver)
    }
}
