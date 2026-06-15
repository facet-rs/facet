import Foundation

/// Actor that holds mutable driver state to avoid NSLock in async contexts.
actor DriverState {
    private let retainFinalizedRequests = true

    enum AddInFlightResult: Sendable, Equatable {
        case inserted
        case duplicate
        case limitExceeded(limit: UInt32, inFlight: Int)
    }

    private struct FinalizedRequest: Sendable {
        let reason: String
        let atUptimeNs: UInt64
    }

    struct PendingCall: Sendable {
        let request: DriverQueuedCall
        let responseTx: @Sendable (Result<[UInt8], ConnectionError>) -> Void
        var timeoutTask: Task<Void, Never>?
    }

    var pendingResponses: [UInt64: PendingCall] = [:]
    var inFlightRequests: Set<UInt64> = []
    var inFlightResponseContext: [UInt64: InFlightResponseContext] = [:]
    private var finalizedRequests: [UInt64: FinalizedRequest] = [:]
    var isClosed = false
    private var controlLaneInternallyClosed = false

    func addPendingResponse(
        _ requestId: UInt64,
        request: DriverQueuedCall,
        _ handler: @escaping @Sendable (Result<[UInt8], ConnectionError>) -> Void,
        timeoutTask: Task<Void, Never>?
    ) -> Bool {
        guard !isClosed else {
            return false
        }
        pendingResponses[requestId] = PendingCall(
            request: request,
            responseTx: handler,
            timeoutTask: timeoutTask
        )
        return true
    }

    func claimPendingResponse(_ requestId: UInt64, reason: String) -> PendingCall? {
        guard let pending = pendingResponses.removeValue(forKey: requestId) else {
            return nil
        }
        markFinalizedRequest(requestId, reason: reason)
        return pending
    }

    func claimPendingResponses(connectionId: UInt64, reason: String) -> [UInt64: PendingCall] {
        let requestIds = pendingResponses.compactMap { requestId, pending in
            pending.request.connectionId == connectionId ? requestId : nil
        }
        var claimed: [UInt64: PendingCall] = [:]
        for requestId in requestIds {
            guard let pending = pendingResponses.removeValue(forKey: requestId) else {
                continue
            }
            claimed[requestId] = pending
            markFinalizedRequest(requestId, reason: reason)
        }
        return claimed
    }

    func markFinalizedRequest(_ requestId: UInt64, reason: String) {
        guard retainFinalizedRequests else {
            return
        }
        let now = DispatchTime.now().uptimeNanoseconds
        finalizedRequests[requestId] = FinalizedRequest(reason: reason, atUptimeNs: now)
        pruneFinalizedRequests(now: now)
    }

    func takeFinalizedRequest(_ requestId: UInt64) -> (reason: String, ageMs: UInt64)? {
        guard retainFinalizedRequests else {
            return nil
        }
        let now = DispatchTime.now().uptimeNanoseconds
        pruneFinalizedRequests(now: now)
        guard let finalized = finalizedRequests.removeValue(forKey: requestId) else {
            return nil
        }
        let ageNs = now >= finalized.atUptimeNs ? now - finalized.atUptimeNs : 0
        return (reason: finalized.reason, ageMs: ageNs / 1_000_000)
    }

    func contextSummary(requestId: UInt64?) -> String {
        let pendingCount = pendingResponses.count
        let inFlightCount = inFlightRequests.count
        let pendingHasRequest = requestId.map { pendingResponses[$0] != nil } ?? false
        let inFlightHasRequest = requestId.map { inFlightRequests.contains($0) } ?? false
        return
            "pending_count=\(pendingCount) in_flight_count=\(inFlightCount) "
            + "pending_has_request=\(pendingHasRequest) in_flight_has_request=\(inFlightHasRequest)"
    }

    private func pruneFinalizedRequests(now: UInt64) {
        let keepNs: UInt64 = 120 * 1_000_000_000
        finalizedRequests = finalizedRequests.filter { _, finalized in
            now >= finalized.atUptimeNs && (now - finalized.atUptimeNs) <= keepNs
        }
    }

    func setPendingTimeoutTask(_ requestId: UInt64, timeoutTask: Task<Void, Never>) -> Bool {
        guard var pending = pendingResponses[requestId] else {
            return false
        }
        pending.timeoutTask = timeoutTask
        pendingResponses[requestId] = pending
        return true
    }

    func addInFlight(
        _ requestId: UInt64,
        connectionId: UInt64,
        responseMetadata: Metadata,
        localMaxConcurrentRequests: UInt32
    ) -> AddInFlightResult {
        guard !inFlightRequests.contains(requestId) else {
            return .duplicate
        }

        let inFlightOnConnection = inFlightResponseContext.values.lazy.filter {
            $0.connectionId == connectionId
        }.count
        if UInt64(inFlightOnConnection) >= UInt64(localMaxConcurrentRequests) {
            return .limitExceeded(
                limit: localMaxConcurrentRequests,
                inFlight: inFlightOnConnection
            )
        }

        inFlightRequests.insert(requestId)
        inFlightResponseContext[requestId] = InFlightResponseContext(
            connectionId: connectionId,
            responseMetadata: responseMetadata
        )
        return .inserted
    }

    func removeInFlight(_ requestId: UInt64) -> (
        removed: Bool,
        connectionId: UInt64,
        responseMetadata: Metadata
    ) {
        let removed = inFlightRequests.remove(requestId) != nil
        let context = inFlightResponseContext.removeValue(forKey: requestId)
        return (
            removed,
            context?.connectionId ?? 0,
            context?.responseMetadata ?? .null
        )
    }

    func claimAllPendingResponses(reason: String) -> [UInt64: PendingCall] {
        isClosed = true
        let responses = pendingResponses
        pendingResponses.removeAll()
        inFlightRequests.removeAll()
        inFlightResponseContext.removeAll()
        for requestId in responses.keys {
            markFinalizedRequest(requestId, reason: reason)
        }
        return responses
    }

    func isConnectionClosed() -> Bool {
        isClosed
    }

    func markControlLaneInternallyClosed() {
        controlLaneInternallyClosed = true
    }

    func isControlLaneInternallyClosed() -> Bool {
        controlLaneInternallyClosed
    }
}

/// Actor for service-lane state.
actor LaneState {
    struct LaneRecord: Sendable {
        let dispatcher: any ServiceDispatcher
        let localSettings: ConnectionSettings
        let channelRegistry: ChannelRegistry
    }

    private var nextConnId: UInt64
    private var lanes: [UInt64: LaneRecord] = [:]
    private var pendingOutbound: [UInt64: PendingOutboundLane] = [:]

    init(role: Role) {
        nextConnId = firstId(for: role)
    }

    // r[impl connection.open]
    // r[impl connection.parity]
    func allocateLaneId() -> UInt64 {
        let id = nextConnId
        nextConnId += 2
        return id
    }

    func contains(_ connId: UInt64) -> Bool {
        lanes[connId] != nil || pendingOutbound[connId] != nil
    }

    func addLane(
        _ connId: UInt64,
        dispatcher: any ServiceDispatcher,
        localSettings: ConnectionSettings,
        channelRegistry: ChannelRegistry
    ) {
        lanes[connId] = LaneRecord(
            dispatcher: dispatcher,
            localSettings: localSettings,
            channelRegistry: channelRegistry
        )
    }

    @discardableResult
    func removeLane(_ connId: UInt64) -> Bool {
        lanes.removeValue(forKey: connId) != nil
    }

    func lane(for connId: UInt64) -> LaneRecord? {
        lanes[connId]
    }

    func isEmpty() -> Bool {
        lanes.isEmpty && pendingOutbound.isEmpty
    }

    func addPendingOutbound(_ connId: UInt64, pending: PendingOutboundLane) {
        pendingOutbound[connId] = pending
    }

    func takePendingOutbound(_ connId: UInt64) -> PendingOutboundLane? {
        pendingOutbound.removeValue(forKey: connId)
    }
}
