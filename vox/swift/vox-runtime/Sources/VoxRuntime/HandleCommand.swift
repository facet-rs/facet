import Foundation

/// Commands from ConnectionHandle to Driver.
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
    case openConnection(
        settings: ConnectionSettings,
        metadata: Metadata,
        dispatcher: (any ServiceDispatcher)?,
        responseTx: @Sendable (Result<Connection, ConnectionError>) -> Void
    )
    case releaseConnection(connectionId: UInt64)
}
