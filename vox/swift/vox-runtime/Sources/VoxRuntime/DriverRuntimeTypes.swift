import Foundation

public struct PreparedRetryRequest: Sendable {
    public let payload: [UInt8]

    public init(payload: [UInt8]) {
        self.payload = payload
    }
}

struct DriverQueuedTaskMessage: Sendable {
    let message: Message
}

struct DriverQueuedCall: Sendable {
    let requestId: UInt64
    let methodId: UInt64
    let metadata: [MetadataEntry]
    let payload: [UInt8]
    let retry: RetryPolicy
    let timeout: TimeInterval?
    let prepareRetry: (@Sendable () async -> PreparedRetryRequest)?
    let schemaInfo: ClientSchemaInfo?
}

struct DriverKeepaliveRuntime {
    let pingIntervalNs: UInt64
    let pongTimeoutNs: UInt64
    var nextPingAtNs: UInt64
    var waitingPongNonce: UInt64?
    var pongDeadlineNs: UInt64
    var nextPingNonce: UInt64
}
