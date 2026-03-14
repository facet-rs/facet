import Foundation

/// Parameters negotiated during handshake.
///
/// r[impl session.handshake] - Effective limit is min of both peers.
/// r[impl rpc.flow-control.credit.initial] - Negotiated during handshake.
public struct Negotiated: Sendable {
    public let maxPayloadSize: UInt32
    public let initialCredit: UInt32
    public let maxConcurrentRequests: UInt32

    public init(maxPayloadSize: UInt32, initialCredit: UInt32, maxConcurrentRequests: UInt32) {
        self.maxPayloadSize = maxPayloadSize
        self.initialCredit = initialCredit
        self.maxConcurrentRequests = maxConcurrentRequests
    }
}

/// Driver-level protocol keepalive configuration.
public struct DriverKeepaliveConfig: Sendable {
    public let pingInterval: TimeInterval
    public let pongTimeout: TimeInterval

    public init(pingInterval: TimeInterval, pongTimeout: TimeInterval) {
        self.pingInterval = pingInterval
        self.pongTimeout = pongTimeout
    }
}
