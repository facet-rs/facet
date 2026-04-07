import Foundation

extension Driver {
    private func sendOrEnqueue(_ message: Message) async throws {
        if !pendingTaskMessages.isEmpty {
            pendingTaskMessages.append(DriverQueuedTaskMessage(message: message))
            return
        }

        do {
            try await conduit.send(message)
        } catch TransportError.wouldBlock {
            pendingTaskMessages.append(DriverQueuedTaskMessage(message: message))
        } catch {
            if resumable {
                pendingTaskMessages.append(DriverQueuedTaskMessage(message: message))
                _ = eventContinuation.yield(.conduitFailed(String(describing: error)))
                return
            }
            throw error
        }
    }

    private func responseMessage(
        requestId: UInt64,
        payload: [UInt8],
        schemas: [UInt8] = []
    ) async -> Message? {
        let responseContext = await state.removeInFlight(requestId)
        guard responseContext.removed else {
            return nil
        }
        return .response(
            connId: responseContext.connectionId,
            requestId: requestId,
            metadata: responseContext.responseMetadata,
            schemas: schemas,
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
        let wireMsg: Message
        switch msg {
        case .data(let channelId, let payload):
            wireMsg = .data(connId: 0, channelId: channelId, payload: payload)
        case .close(let channelId):
            wireMsg = .close(connId: 0, channelId: channelId)
        case .grantCredit(let channelId, let bytes):
            wireMsg = .credit(connId: 0, channelId: channelId, bytes: bytes)
        case .response(let requestId, let payload, let methodId, let schemaPayload):
            let checkedPayload: [UInt8]
            if payload.count > Int(negotiated.maxPayloadSize) {
                debugLog(
                    "outgoing response for request \(requestId) exceeds max_payload_size "
                        + "(\(payload.count) > \(negotiated.maxPayloadSize)), sending Cancelled")
                checkedPayload = encodeCancelledError()
            } else {
                checkedPayload = payload
            }
            let schemas: [UInt8]
            if let schemaPayload {
                let filteredPayload = schemaSendTracker.filterForSending(
                    schemaPayload,
                    methodId: methodId
                )
                schemas = filteredPayload.encodeCbor()
            } else {
                schemas = []
            }
            let waiters = await operations.seal(ownerRequestId: requestId, payload: checkedPayload)
            if !waiters.isEmpty {
                for waiter in waiters {
                    guard let replay = await responseMessage(requestId: waiter, payload: checkedPayload, schemas: schemas) else {
                        continue
                    }
                    try await sendOrEnqueue(replay)
                }
                return
            }
            guard let response = await responseMessage(requestId: requestId, payload: checkedPayload, schemas: schemas) else {
                return
            }
            wireMsg = response
        }
        try await sendOrEnqueue(wireMsg)
    }

    /// Handle a command from ConnectionHandle.
    func handleCommand(_ cmd: HandleCommand) async {
        switch cmd {
        case .call(
            let requestId, let methodId, let metadata, let payload, let retry,
            let timeout, let prepareRetry, let responseTx, let schemaInfo):
            let isClosed = await state.isConnectionClosed()
            guard !isClosed else {
                responseTx(.failure(.connectionClosed))
                return
            }

            let queuedCall = DriverQueuedCall(
                requestId: requestId,
                methodId: methodId,
                metadata: metadata,
                payload: payload,
                retry: retry,
                timeout: timeout,
                prepareRetry: prepareRetry,
                schemaInfo: schemaInfo
            )

            let inserted = await state.addPendingResponse(
                requestId,
                request: queuedCall,
                responseTx,
                timeoutTask: nil
            )
            guard inserted else {
                responseTx(.failure(.connectionClosed))
                return
            }

            // Build schema bytes for the request
            let schemas: [UInt8]
            if let schemaInfo {
                let fullPayload = schemaInfo.methodInfo.buildPayload(
                    direction: .args,
                    registry: schemaInfo.schemaRegistry
                )
                let filteredPayload = schemaSendTracker.filterForSending(
                    fullPayload,
                    methodId: methodId
                )
                schemas = filteredPayload.encodeCbor()
            } else {
                schemas = []
            }

            let msg = Message.request(
                connId: 0,
                requestId: requestId,
                methodId: methodId,
                metadata: metadata,
                schemas: schemas,
                payload: payload
            )
            do {
                try await conduit.send(msg)
            } catch TransportError.wouldBlock {
                pendingCalls.append(queuedCall)
                return
            } catch {
                if resumable {
                    pendingCalls.append(queuedCall)
                    _ = eventContinuation.yield(.conduitFailed(String(describing: error)))
                    return
                }
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

        traceLog(.resume, "flushPendingCalls: count=\(pendingCalls.count)")
        while let call = pendingCalls.first {
            let replayCall: DriverQueuedCall
            if let prepareRetry = call.prepareRetry {
                traceLog(.resume, "flushPendingCalls: rebuilding requestId=\(call.requestId) methodId=\(call.methodId)")
                let rebuilt = await prepareRetry()
                let replayMetadata =
                    if call.retry.idem {
                        await handle.freshOperationMetadata(from: call.metadata)
                    } else {
                        call.metadata
                    }
                replayCall = DriverQueuedCall(
                    requestId: call.requestId,
                    methodId: call.methodId,
                    metadata: replayMetadata,
                    payload: rebuilt.payload,
                    retry: call.retry,
                    timeout: call.timeout,
                    prepareRetry: call.prepareRetry,
                    schemaInfo: call.schemaInfo
                )
            } else {
                replayCall = call
            }

            // Build schema bytes for the request (on replay, schemas may have been sent already)
            let schemas: [UInt8]
            if let schemaInfo = replayCall.schemaInfo {
                let fullPayload = schemaInfo.methodInfo.buildPayload(
                    direction: .args,
                    registry: schemaInfo.schemaRegistry
                )
                let filteredPayload = schemaSendTracker.filterForSending(
                    fullPayload,
                    methodId: replayCall.methodId
                )
                schemas = filteredPayload.encodeCbor()
            } else {
                schemas = []
            }

            let msg = Message.request(
                connId: 0,
                requestId: replayCall.requestId,
                methodId: replayCall.methodId,
                metadata: replayCall.metadata,
                schemas: schemas,
                payload: replayCall.payload
            )

            traceLog(.resume, "flushPendingCalls: sending replay requestId=\(replayCall.requestId) methodId=\(replayCall.methodId)")
            do {
                try await conduit.send(msg)
            } catch TransportError.wouldBlock {
                traceLog(.resume, "flushPendingCalls: conduit would block requestId=\(replayCall.requestId)")
                pendingCalls[0] = replayCall
                return
            } catch {
                if resumable {
                    traceLog(.resume, "flushPendingCalls: send failed requestId=\(replayCall.requestId) error=\(String(describing: error))")
                    pendingCalls[0] = replayCall
                    _ = eventContinuation.yield(.conduitFailed(String(describing: error)))
                    return
                }
                let pending = await state.claimPendingResponse(
                    replayCall.requestId,
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

            guard let timeout = replayCall.timeout else {
                continue
            }

            let timeoutNs = Self.timeoutToNanoseconds(timeout)
            let capturedState = state
            let capturedConduit = conduit
            let requestId = replayCall.requestId
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

    func replayPendingCallsAfterResume() async {
        let inFlight = await state.pendingCallsSnapshot()
        pendingCalls.removeAll()
        traceLog(.resume, "replayPendingCallsAfterResume: inFlight=\(inFlight.count)")
        for call in inFlight {
            if call.prepareRetry != nil && !call.retry.idem {
                traceLog(.resume, "replayPendingCallsAfterResume: indeterminate requestId=\(call.requestId)")
                guard let pending = await state.claimPendingResponse(
                    call.requestId,
                    reason: "resume-channel-indeterminate"
                ) else {
                    continue
                }
                pending.timeoutTask?.cancel()
                pending.responseTx(.success(encodeIndeterminateError()))
                continue
            }
            traceLog(.resume, "replayPendingCallsAfterResume: queueing requestId=\(call.requestId) methodId=\(call.methodId)")
            pendingCalls.append(call)
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
                if resumable {
                    _ = eventContinuation.yield(.conduitFailed(String(describing: error)))
                    return
                }
                await failAllPending()
                eventContinuation.finish()
                return
            }

            pendingTaskMessages.removeFirst()
        }
    }
}
