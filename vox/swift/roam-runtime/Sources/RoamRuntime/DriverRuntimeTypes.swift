import Foundation

public struct PreparedRetryRequest: Sendable {
    public let payload: [UInt8]
    public let channels: [UInt64]

    public init(payload: [UInt8], channels: [UInt64]) {
        self.payload = payload
        self.channels = channels
    }
}

struct DriverQueuedTaskMessage: Sendable {
    let message: MessageV7
}

struct DriverQueuedCall: Sendable {
    let requestId: UInt64
    let methodId: UInt64
    let metadata: [MetadataEntryV7]
    let payload: [UInt8]
    let channels: [UInt64]
    let retry: RetryPolicy
    let timeout: TimeInterval?
    let prepareRetry: (@Sendable () async -> PreparedRetryRequest)?
}

struct DriverKeepaliveRuntime {
    let pingIntervalNs: UInt64
    let pongTimeoutNs: UInt64
    var nextPingAtNs: UInt64
    var waitingPongNonce: UInt64?
    var pongDeadlineNs: UInt64
    var nextPingNonce: UInt64
}
