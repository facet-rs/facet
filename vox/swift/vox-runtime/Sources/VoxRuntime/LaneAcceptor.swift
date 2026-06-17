import Foundation

// MARK: - LaneRequest

/// Metadata wrapper for an incoming lane-open request.
public struct LaneRequest: Sendable {
    /// All metadata entries from the lane-open message.
    public let metadata: Metadata
    /// The service name, extracted from the "vox-service" metadata key.
    public let service: String
}

// MARK: - PendingLane

/// An inbound service lane awaiting a handler decision.
///
/// Call `handleWith(_:)` to accept the lane with a specific service dispatcher.
/// Call `reject(_:)` to reject it with structured metadata. If the `PendingLane`
/// is deallocated without either call, the runtime rejects it with
/// `.policyRejected` as a safety fallback.
public final class PendingLane: @unchecked Sendable {
    private let lock = NSLock()
    private var handled = false
    private let acceptFn: @Sendable (any ServiceDispatcher) -> Void
    private let rejectFn: @Sendable (LaneRejection) -> Void

    init(
        accept: @escaping @Sendable (any ServiceDispatcher) -> Void,
        reject: @escaping @Sendable (LaneRejection) -> Void
    ) {
        self.acceptFn = accept
        self.rejectFn = reject
    }

    /// Accept the lane and route it to the given dispatcher.
    /// Safe to call multiple times; only the first call takes effect.
    public func handleWith(_ dispatcher: any ServiceDispatcher) {
        lock.lock()
        let wasHandled = handled
        handled = true
        lock.unlock()
        if !wasHandled {
            acceptFn(dispatcher)
        }
    }

    /// Reject the lane with structured metadata for the peer.
    /// Safe to call multiple times; only the first call takes effect.
    public func reject(_ rejection: LaneRejection = .new(.policyRejected)) {
        lock.lock()
        let wasHandled = handled
        handled = true
        lock.unlock()
        if !wasHandled {
            rejectFn(rejection)
        }
    }

    deinit {
        lock.lock()
        let wasHandled = handled
        handled = true
        lock.unlock()
        if !wasHandled {
            rejectFn(.new(.policyRejected))
        }
    }
}

// MARK: - LaneAcceptor

/// Routes incoming service lanes to appropriate service dispatchers.
///
/// Implement this protocol to decide, per lane, which `ServiceDispatcher`
/// should handle it — or to reject it explicitly with `PendingLane.reject(_:)`.
public protocol LaneAcceptor: Sendable {
    /// Called for each incoming service lane.
    ///
    /// - Parameters:
    ///   - request: Metadata about the incoming lane, including the service name.
    ///   - lane: The pending lane. Call `handleWith(_:)` to accept, or
    ///     `reject(_:)` to reject with structured metadata.
    func accept(request: LaneRequest, lane: PendingLane)
}

// MARK: - DefaultLaneAcceptor

/// A `LaneAcceptor` that routes all incoming lanes to the same dispatcher.
///
/// This is the common case for subjects that handle a single service: every service
/// lane that arrives is accepted and dispatched to the same `ServiceDispatcher`
/// regardless of service name.
public struct DefaultLaneAcceptor: LaneAcceptor {
    public let dispatcher: any ServiceDispatcher

    public init(dispatcher: any ServiceDispatcher) {
        self.dispatcher = dispatcher
    }

    public func accept(request _: LaneRequest, lane: PendingLane) {
        lane.handleWith(dispatcher)
    }
}
