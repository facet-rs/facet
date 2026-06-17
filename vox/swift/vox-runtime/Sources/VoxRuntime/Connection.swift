import Foundation
import PhonSchema

// r[impl connection.protocol]
// r[impl connection.model]
// r[impl connection.lifecycle.driven]
public final class Connection: @unchecked Sendable {
    public let role: Role
    let controlLane: Lane
    let driver: Driver
    public let handle: ConnectionHandle
    public let peerMetadata: Metadata

    init(
        role: Role,
        controlLane: Lane,
        driver: Driver,
        handle: ConnectionHandle,
        peerMetadata: Metadata
    ) {
        self.role = role
        self.controlLane = controlLane
        self.driver = driver
        self.handle = handle
        self.peerMetadata = peerMetadata
    }

    public func run() async throws {
        // r[impl connection.lifecycle.driven]
        try await driver.run()
    }

    public func openLane(
        settings: ConnectionSettings,
        metadata: Metadata = emptyMetadata(),
        dispatcher: (any ServiceDispatcher)? = nil
    ) async throws -> Lane {
        try await handle.openLane(
            settings: settings,
            metadata: metadata,
            dispatcher: dispatcher
        )
    }

    public func closeLane(
        _ laneId: UInt64,
        metadata: Metadata = emptyMetadata()
    ) async throws {
        try await handle.closeLane(laneId, metadata: metadata)
    }

    public func shutdown() {
        // r[impl connection.shutdown.explicit]
        handle.shutdown()
    }

    public func debugSnapshot() async -> VoxConnectionDebugSnapshot {
        // r[impl rpc.debug.snapshot]
        await driver.debugSnapshot()
    }

    public static func connect(
        _ connector: some ConnectionConnector,
        onLane: (any LaneAcceptor)? = nil,
        keepalive: ConnectionKeepaliveConfig? = nil,
        metadata: Metadata = .null
    ) async throws -> Connection {
        try await connect(
            connector,
            controlDispatcher: ConnectionControlDispatcher(),
            onLane: onLane,
            keepalive: keepalive,
            metadata: metadata
        )
    }

    static func connect(
        _ connector: some ConnectionConnector,
        controlDispatcher: any ServiceDispatcher,
        onLane: (any LaneAcceptor)? = nil,
        keepalive: ConnectionKeepaliveConfig? = nil,
        metadata: Metadata = .null
    ) async throws -> Connection {
        // r[impl rpc.connection-setup]
        let attachment = try await connector.openAttachment()
        let (controlLane, driver, handle, peerMetadata) =
            try await establishInitiator(
                attachment: attachment,
                dispatcher: controlDispatcher,
                laneAcceptor: onLane,
                keepalive: keepalive,
                metadata: metadata
            )
        return Connection(
            role: .initiator,
            controlLane: controlLane,
            driver: driver,
            handle: handle,
            peerMetadata: peerMetadata
        )
    }

    public static func accept(
        _ connector: some ConnectionConnector,
        onLane: (any LaneAcceptor)? = nil,
        keepalive: ConnectionKeepaliveConfig? = nil,
        metadata: Metadata = .null
    ) async throws -> Connection {
        try await accept(
            connector,
            controlDispatcher: ConnectionControlDispatcher(),
            onLane: onLane,
            keepalive: keepalive,
            metadata: metadata
        )
    }

    static func accept(
        _ connector: some ConnectionConnector,
        controlDispatcher: any ServiceDispatcher,
        onLane: (any LaneAcceptor)? = nil,
        keepalive: ConnectionKeepaliveConfig? = nil,
        metadata: Metadata = .null
    ) async throws -> Connection {
        // r[impl rpc.connection-setup]
        let attachment = try await connector.openAttachment()
        return try await accept(
            attachment,
            controlDispatcher: controlDispatcher,
            onLane: onLane,
            keepalive: keepalive,
            metadata: metadata
        )
    }

    public static func connect(
        overFreshLink link: any Link,
        onLane: (any LaneAcceptor)? = nil,
        keepalive: ConnectionKeepaliveConfig? = nil,
        metadata: Metadata = .null
    ) async throws -> Connection {
        try await connect(
            overFreshLink: link,
            controlDispatcher: ConnectionControlDispatcher(),
            onLane: onLane,
            keepalive: keepalive,
            metadata: metadata
        )
    }

    static func connect(
        overFreshLink link: any Link,
        controlDispatcher: any ServiceDispatcher,
        onLane: (any LaneAcceptor)? = nil,
        keepalive: ConnectionKeepaliveConfig? = nil,
        metadata: Metadata = .null
    ) async throws -> Connection {
        // r[impl rpc.connection-setup]
        let (controlLane, driver, handle, peerMetadata) =
            try await establishInitiator(
                attachment: .fresh(link),
                dispatcher: controlDispatcher,
                laneAcceptor: onLane,
                keepalive: keepalive,
                metadata: metadata
            )
        return Connection(
            role: .initiator,
            controlLane: controlLane,
            driver: driver,
            handle: handle,
            peerMetadata: peerMetadata
        )
    }

    public static func accept(
        _ attachment: LinkAttachment,
        onLane: (any LaneAcceptor)? = nil,
        keepalive: ConnectionKeepaliveConfig? = nil,
        metadata: Metadata = .null
    ) async throws -> Connection {
        try await accept(
            attachment,
            controlDispatcher: ConnectionControlDispatcher(),
            onLane: onLane,
            keepalive: keepalive,
            metadata: metadata
        )
    }

    static func accept(
        _ attachment: LinkAttachment,
        controlDispatcher: any ServiceDispatcher,
        onLane: (any LaneAcceptor)? = nil,
        keepalive: ConnectionKeepaliveConfig? = nil,
        metadata: Metadata = .null
    ) async throws -> Connection {
        // r[impl rpc.connection-setup]
        let (controlLane, driver, handle, peerMetadata) =
            try await establishAcceptor(
                attachment: attachment,
                dispatcher: controlDispatcher,
                laneAcceptor: onLane,
                keepalive: keepalive,
                metadata: metadata
            )
        return Connection(
            role: .acceptor,
            controlLane: controlLane,
            driver: driver,
            handle: handle,
            peerMetadata: peerMetadata
        )
    }

    public static func accept(
        freshLink link: any Link,
        onLane: (any LaneAcceptor)? = nil,
        keepalive: ConnectionKeepaliveConfig? = nil,
        metadata: Metadata = .null
    ) async throws -> Connection {
        try await accept(
            .fresh(link),
            onLane: onLane,
            keepalive: keepalive,
            metadata: metadata
        )
    }

    static func accept(
        freshLink link: any Link,
        controlDispatcher: any ServiceDispatcher,
        onLane: (any LaneAcceptor)? = nil,
        keepalive: ConnectionKeepaliveConfig? = nil,
        metadata: Metadata = .null
    ) async throws -> Connection {
        try await accept(
            .fresh(link),
            controlDispatcher: controlDispatcher,
            onLane: onLane,
            keepalive: keepalive,
            metadata: metadata
        )
    }
}
