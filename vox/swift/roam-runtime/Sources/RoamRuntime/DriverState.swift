import Foundation

/// Actor that holds mutable driver state to avoid NSLock in async contexts.
actor DriverState {
    private let retainFinalizedRequests = true

    private struct FinalizedRequest: Sendable {
        let reason: String
        let atUptimeNs: UInt64
    }

    struct PendingCall: Sendable {
        let responseTx: @Sendable (Result<[UInt8], ConnectionError>) -> Void
        var timeoutTask: Task<Void, Never>?
    }

    var pendingResponses: [UInt64: PendingCall] = [:]
    var inFlightRequests: Set<UInt64> = []
    var inFlightResponseContext: [UInt64: InFlightResponseContext] = [:]
    private var finalizedRequests: [UInt64: FinalizedRequest] = [:]
    var isClosed = false

    func addPendingResponse(
        _ requestId: UInt64,
        _ handler: @escaping @Sendable (Result<[UInt8], ConnectionError>) -> Void,
        timeoutTask: Task<Void, Never>?
    ) -> Bool {
        guard !isClosed else {
            return false
        }
        pendingResponses[requestId] = PendingCall(responseTx: handler, timeoutTask: timeoutTask)
        return true
    }

    func claimPendingResponse(_ requestId: UInt64, reason: String) -> PendingCall? {
        guard let pending = pendingResponses.removeValue(forKey: requestId) else {
            return nil
        }
        markFinalizedRequest(requestId, reason: reason)
        return pending
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
        responseMetadata: [MetadataEntryV7]
    ) -> Bool {
        let inserted = inFlightRequests.insert(requestId).inserted
        if inserted {
            inFlightResponseContext[requestId] = InFlightResponseContext(
                connectionId: connectionId,
                responseMetadata: responseMetadata
            )
        }
        return inserted
    }

    func removeInFlight(_ requestId: UInt64) -> (
        removed: Bool,
        connectionId: UInt64,
        responseMetadata: [MetadataEntryV7]
    ) {
        let removed = inFlightRequests.remove(requestId) != nil
        let context = inFlightResponseContext.removeValue(forKey: requestId)
        return (
            removed,
            context?.connectionId ?? 0,
            context?.responseMetadata ?? []
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
}

/// Actor for virtual connection state.
actor VirtualConnectionState {
    private var nextConnId: UInt64 = 1
    private var virtualConnections: Set<UInt64> = []

    func allocateConnId() -> UInt64 {
        let id = nextConnId
        nextConnId += 1
        return id
    }

    func addConnection(_ connId: UInt64) {
        virtualConnections.insert(connId)
    }

    func removeConnection(_ connId: UInt64) {
        virtualConnections.remove(connId)
    }
}
