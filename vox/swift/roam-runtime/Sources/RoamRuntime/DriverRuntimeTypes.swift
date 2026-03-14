import Foundation

struct DriverQueuedTaskMessage: Sendable {
    let message: MessageV7
}

struct DriverQueuedCall: Sendable {
    let requestId: UInt64
    let methodId: UInt64
    let metadata: [MetadataEntryV7]
    let payload: [UInt8]
    let channels: [UInt64]
    let timeout: TimeInterval?
}

struct DriverKeepaliveRuntime {
    let pingIntervalNs: UInt64
    let pongTimeoutNs: UInt64
    var nextPingAtNs: UInt64
    var waitingPongNonce: UInt64?
    var pongDeadlineNs: UInt64
    var nextPingNonce: UInt64
}
