import Foundation

/// Commands from lane and connection handles to Driver.
enum HandleCommand: Sendable {
    case call(
        connectionId: UInt64,
        requestId: UInt64,
        methodId: UInt64,
        metadata: Metadata,
        payload: [UInt8],
        channels: [UInt64],
        timeout: TimeInterval?,
        responseTx: @Sendable (Result<[UInt8], ConnectionError>) -> Void,
        schemaInfo: ClientSchemaInfo?
    )
    case openLane(
        settings: ConnectionSettings,
        metadata: Metadata,
        dispatcher: (any ServiceDispatcher)?,
        responseTx: @Sendable (Result<Lane, ConnectionError>) -> Void
    )
    case closeLane(
        laneId: UInt64,
        metadata: Metadata,
        responseTx: @Sendable (Result<Void, ConnectionError>) -> Void
    )
}
