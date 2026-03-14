import Foundation

extension Driver {
    private func responseMessage(
        requestId: UInt64,
        payload: [UInt8]
    ) async -> MessageV7? {
        let responseContext = await state.removeInFlight(requestId)
        guard responseContext.removed else {
            return nil
        }
        return .response(
            connId: responseContext.connectionId,
            requestId: requestId,
            metadata: responseContext.responseMetadata,
            channels: [],
            payload: payload
        )
    }

    /// Get the task sender for handlers to send responses.
    func taskSender() -> @Sendable (TaskMessage) -> Void {
        let cont = eventContinuation
        let queue = taskQueue
        return { msg in
            guard queue.push(msg) else {
                return
            }
            _ = cont.yield(.wake)
        }
    }

    /// Handle a task message from a handler.
    func handleTaskMessage(_ msg: TaskMessage) async throws {
        let wireMsg: MessageV7
        switch msg {
        case .data(let channelId, let payload):
            wireMsg = .data(connId: 0, channelId: channelId, payload: payload)
        case .close(let channelId):
            wireMsg = .close(connId: 0, channelId: channelId)
        case .grantCredit(let channelId, let bytes):
            wireMsg = .credit(connId: 0, channelId: channelId, bytes: bytes)
        case .response(let requestId, let payload):
            let checkedPayload: [UInt8]
            if payload.count > Int(negotiated.maxPayloadSize) {
                debugLog(
                    "outgoing response for request \(requestId) exceeds max_payload_size "
                        + "(\(payload.count) > \(negotiated.maxPayloadSize)), sending Cancelled")
                checkedPayload = encodeCancelledError()
            } else {
                checkedPayload = payload
            }
            let waiters = await operations.seal(ownerRequestId: requestId, payload: checkedPayload)
            if !waiters.isEmpty {
                for waiter in waiters {
                    guard let replay = await responseMessage(requestId: waiter, payload: checkedPayload) else {
                        continue
                    }
                    do {
                        try await conduit.send(replay)
                    } catch TransportError.wouldBlock {
                        pendingTaskMessages.append(DriverQueuedTaskMessage(message: replay))
                    }
                }
                return
            }
            guard let response = await responseMessage(requestId: requestId, payload: checkedPayload) else {
                return
            }
            wireMsg = response
        }
        do {
            try await conduit.send(wireMsg)
        } catch TransportError.wouldBlock {
            pendingTaskMessages.append(DriverQueuedTaskMessage(message: wireMsg))
        }
    }

    /// Handle a command from ConnectionHandle.
    func handleCommand(_ cmd: HandleCommand) async {
        switch cmd {
        case .call(
            let requestId, let methodId, let metadata, let payload, let channels, let retry,
            let timeout, let responseTx):
            let isClosed = await state.isConnectionClosed()
            guard !isClosed else {
                responseTx(.failure(.connectionClosed))
                return
            }

            let inserted = await state.addPendingResponse(
                requestId,
                responseTx,
                timeoutTask: nil
            )
            guard inserted else {
                responseTx(.failure(.connectionClosed))
                return
            }

            let msg = MessageV7.request(
                connId: 0,
                requestId: requestId,
                methodId: methodId,
                metadata: metadata,
                channels: channels,
                payload: payload
            )
            do {
                try await conduit.send(msg)
            } catch TransportError.wouldBlock {
                pendingCalls.append(
                    DriverQueuedCall(
                        requestId: requestId,
                        methodId: methodId,
                        metadata: metadata,
                        payload: payload,
                        channels: channels,
                        retry: retry,
                        timeout: timeout
                    ))
                return
            } catch {
                let pending = await state.claimPendingResponse(
                    requestId,
                    reason: "conduit-send-failed"
                )
                pending?.timeoutTask?.cancel()
                warnLog("conduit send failed for request_id \(requestId): \(String(describing: error))")
                pending?.responseTx(.failure(.transportError(String(describing: error))))
                await failAllPending()
                eventContinuation.finish()
                return
            }

            guard let timeout else {
                return
            }

            let timeoutNs = Self.timeoutToNanoseconds(timeout)
            let capturedState = state
            let capturedConduit = conduit
            let timeoutTask = Task {
                do {
                    try await Task.sleep(nanoseconds: timeoutNs)
                } catch {
                    return
                }
                guard let pending = await capturedState.claimPendingResponse(
                    requestId,
                    reason: "timeout"
                ) else {
                    return
                }
                pending.timeoutTask?.cancel()
                warnLog("request timed out request_id=\(requestId) timeout_s=\(timeout)")
                pending.responseTx(.failure(.timeout))
                try? await capturedConduit.send(.cancel(connId: 0, requestId: requestId))
            }
            let installed = await state.setPendingTimeoutTask(requestId, timeoutTask: timeoutTask)
            if !installed {
                timeoutTask.cancel()
            }
        }
    }

    func flushPendingCalls() async throws {
        if pendingCalls.isEmpty {
            return
        }

        while let call = pendingCalls.first {
            let msg = MessageV7.request(
                connId: 0,
                requestId: call.requestId,
                methodId: call.methodId,
                metadata: call.metadata,
                channels: call.channels,
                payload: call.payload
            )

            do {
                try await conduit.send(msg)
            } catch TransportError.wouldBlock {
                return
            } catch {
                let pending = await state.claimPendingResponse(
                    call.requestId,
                    reason: "conduit-send-failed"
                )
                pending?.timeoutTask?.cancel()
                pending?.responseTx(.failure(.transportError(String(describing: error))))
                pendingCalls.removeFirst()
                await failAllPending()
                eventContinuation.finish()
                return
            }

            pendingCalls.removeFirst()

            guard let timeout = call.timeout else {
                continue
            }

            let timeoutNs = Self.timeoutToNanoseconds(timeout)
            let capturedState = state
            let capturedConduit = conduit
            let requestId = call.requestId
            let timeoutTask = Task {
                do {
                    try await Task.sleep(nanoseconds: timeoutNs)
                } catch {
                    return
                }
                guard let pending = await capturedState.claimPendingResponse(
                    requestId,
                    reason: "timeout"
                ) else {
                    return
                }
                pending.timeoutTask?.cancel()
                warnLog("request timed out request_id=\(requestId) timeout_s=\(timeout)")
                pending.responseTx(.failure(.timeout))
                try? await capturedConduit.send(.cancel(connId: 0, requestId: requestId))
            }
            let installed = await state.setPendingTimeoutTask(requestId, timeoutTask: timeoutTask)
            if !installed {
                timeoutTask.cancel()
            }
        }
    }

    func flushPendingTaskMessages() async throws {
        if pendingTaskMessages.isEmpty {
            return
        }

        while let pending = pendingTaskMessages.first {
            do {
                try await conduit.send(pending.message)
            } catch TransportError.wouldBlock {
                return
            } catch {
                await failAllPending()
                eventContinuation.finish()
                return
            }

            pendingTaskMessages.removeFirst()
        }
    }
}
