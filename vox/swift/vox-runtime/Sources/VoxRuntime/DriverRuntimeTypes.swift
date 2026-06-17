import Foundation

struct DriverQueuedTaskMessage: Sendable {
    let connectionId: UInt64
    let taskMessage: TaskMessage
}

struct DriverQueuedWireMessage: Sendable {
    let message: Message
}

struct DriverQueuedCall: Sendable {
    let connectionId: UInt64
    let requestId: UInt64
    let methodId: UInt64
    let metadata: Metadata
    let payload: [UInt8]
    let channels: [UInt64]
    let timeout: TimeInterval?
    let schemaInfo: ClientSchemaInfo?
}

struct PendingOutboundLane: Sendable {
    let localSettings: ConnectionSettings
    let dispatcher: (any ServiceDispatcher)?
    let establishmentContext: VoxEstablishmentContext
    let establishmentStartedAt: UInt64
    let responseTx: @Sendable (Result<Lane, ConnectionError>) -> Void
}

struct DriverKeepaliveRuntime {
    let pingIntervalNs: UInt64
    let pongTimeoutNs: UInt64
    var nextPingAtNs: UInt64
    var waitingPongNonce: UInt64?
    var pongDeadlineNs: UInt64
    var nextPingNonce: UInt64
}
