import Foundation

private struct OperationSignature: Sendable {
    let methodId: UInt64
    let args: [UInt8]

    func matches(methodId: UInt64, args: [UInt8]) -> Bool {
        self.methodId == methodId && self.args == args
    }
}

private struct StoredOperation: Sendable {
    let signature: OperationSignature
    let retry: RetryPolicy
}

private struct LiveOperation: Sendable {
    let stored: StoredOperation
    let ownerRequestId: UInt64
    var waiters: [UInt64]
}

private struct SealedOperation: Sendable {
    let stored: StoredOperation
    let payload: [UInt8]
}

private enum OperationState: Sendable {
    case live(LiveOperation)
    case released(StoredOperation)
    case sealed(SealedOperation)
    case indeterminate(StoredOperation)
}

enum OperationAdmit: Sendable {
    case start
    case attached
    case replay([UInt8])
    case conflict
    case indeterminate
}

enum OperationCancel: Sendable {
    case none
    case detach
    case keepLive
    case release(ownerRequestId: UInt64, waiters: [UInt64])
}

actor OperationRegistry {
    private var states: [UInt64: OperationState] = [:]
    private var requestToOperation: [UInt64: UInt64] = [:]

    func admit(
        operationId: UInt64,
        methodId: UInt64,
        args: [UInt8],
        retry: RetryPolicy,
        requestId: UInt64
    ) -> OperationAdmit {
        let signature = OperationSignature(methodId: methodId, args: args)
        guard let existing = states[operationId] else {
            requestToOperation[requestId] = operationId
            states[operationId] = .live(
                LiveOperation(
                    stored: StoredOperation(signature: signature, retry: retry),
                    ownerRequestId: requestId,
                    waiters: [requestId]
                )
            )
            return .start
        }

        switch existing {
        case .live(var live):
            guard live.stored.signature.matches(methodId: methodId, args: args) else {
                return .conflict
            }
            live.waiters.append(requestId)
            requestToOperation[requestId] = operationId
            states[operationId] = .live(live)
            return .attached
        case .sealed(let sealed):
            guard sealed.stored.signature.matches(methodId: methodId, args: args) else {
                return .conflict
            }
            return .replay(sealed.payload)
        case .released(let stored), .indeterminate(let stored):
            guard stored.signature.matches(methodId: methodId, args: args) else {
                return .conflict
            }
            guard stored.retry.idem else {
                return .indeterminate
            }
            requestToOperation[requestId] = operationId
            states[operationId] = .live(
                LiveOperation(
                    stored: StoredOperation(signature: signature, retry: stored.retry),
                    ownerRequestId: requestId,
                    waiters: [requestId]
                )
            )
            return .start
        }
    }

    func seal(ownerRequestId: UInt64, payload: [UInt8]) -> [UInt64] {
        guard let operationId = requestToOperation[ownerRequestId],
            let existing = states[operationId]
        else {
            return []
        }
        guard case .live(let live) = existing, live.ownerRequestId == ownerRequestId else {
            return []
        }
        for waiter in live.waiters {
            requestToOperation.removeValue(forKey: waiter)
        }
        states[operationId] = .sealed(
            SealedOperation(
                stored: live.stored,
                payload: payload
            )
        )
        return live.waiters
    }

    func failWithoutReply(ownerRequestId: UInt64) -> (waiters: [UInt64], persist: Bool)? {
        guard let operationId = requestToOperation[ownerRequestId],
            let existing = states[operationId]
        else {
            return nil
        }
        guard case .live(let live) = existing, live.ownerRequestId == ownerRequestId else {
            return nil
        }
        for waiter in live.waiters {
            requestToOperation.removeValue(forKey: waiter)
        }
        if live.stored.retry.persist {
            states[operationId] = .indeterminate(live.stored)
        } else {
            states[operationId] = .released(live.stored)
        }
        return (live.waiters, live.stored.retry.persist)
    }

    func cancel(requestId: UInt64) -> OperationCancel {
        guard let operationId = requestToOperation[requestId],
            let existing = states[operationId]
        else {
            return .none
        }
        guard case .live(var live) = existing else {
            requestToOperation.removeValue(forKey: requestId)
            return .none
        }

        if live.stored.retry.persist {
            if live.ownerRequestId == requestId {
                return .keepLive
            }
            live.waiters.removeAll { $0 == requestId }
            requestToOperation.removeValue(forKey: requestId)
            states[operationId] = .live(live)
            return .detach
        }

        for waiter in live.waiters {
            requestToOperation.removeValue(forKey: waiter)
        }
        states[operationId] = .released(live.stored)
        return .release(ownerRequestId: live.ownerRequestId, waiters: live.waiters)
    }
}
