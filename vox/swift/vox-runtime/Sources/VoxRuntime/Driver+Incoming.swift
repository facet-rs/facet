import Foundation

extension Driver {
    func addVirtualConnection(_ connId: UInt64) async {
        await virtualConnState.addConnection(connId)
    }

    func removeVirtualConnection(_ connId: UInt64) async {
        await virtualConnState.removeConnection(connId)
    }

    /// Handle an incoming message.
    ///
    /// r[impl connection.close.semantics] - Stop sending, close connection, fail in-flight.
    /// r[impl rpc.request] - Request before Response in message sequence.
    /// r[impl session.protocol-error] - Unknown message variant triggers Goodbye.
    func handleMessage(
        _ msg: Message,
        keepaliveRuntime: inout DriverKeepaliveRuntime?
    ) async throws {
        switch msg.payload {
        case .ping(let ping):
            do {
                try await conduit.send(.pong(.init(nonce: ping.nonce)))
            } catch TransportError.wouldBlock {
                pendingTaskMessages.append(
                    DriverQueuedTaskMessage(message: .pong(.init(nonce: ping.nonce))))
            }
        case .pong(let pong):
            handlePong(nonce: pong.nonce, keepaliveRuntime: &keepaliveRuntime)
        case .protocolError(let error):
            await failAllPending()
            throw ConnectionError.protocolViolation(rule: error.description)
        case .connectionOpen(let open):
            if acceptConnections {
                await addVirtualConnection(msg.connectionId)
                try await conduit.send(
                    .connectionAccept(
                        connId: msg.connectionId,
                        settings: open.connectionSettings,
                        metadata: []
                    ))
            } else {
                try await conduit.send(.connectionReject(connId: msg.connectionId, metadata: []))
            }
        case .connectionAccept, .connectionReject:
            break
        case .connectionClose:
            warnLog("received ConnectionClose conn_id=\(msg.connectionId)")
            if msg.connectionId == 0 {
                warnLog("received ConnectionClose for root connection; shutting down driver")
                await failAllPending()
                throw ConnectionError.connectionClosed
            }
            await removeVirtualConnection(msg.connectionId)
        case .requestMessage(let request):
            switch request.body {
            case .call(let call):
                try await handleRequest(
                    connId: msg.connectionId,
                    requestId: request.id,
                    methodId: call.methodId,
                    metadata: call.metadata,
                    payload: call.args.bytes
                )
            case .response(let response):
                let payload = response.ret.bytes
                guard let pending = await state.claimPendingResponse(request.id, reason: "response")
                else {
                    if let finalized = await state.takeFinalizedRequest(request.id) {
                        warnLog(
                            "dropping late response for finalized request_id \(request.id) "
                                + "(reason=\(finalized.reason) age_ms=\(finalized.ageMs) "
                                + "payload_size=\(payload.count)); continuing"
                        )
                        return
                    }
                    let stateContext = await state.contextSummary(requestId: request.id)
                    warnLog(
                        "received response for unknown request_id \(request.id) "
                            + "(payload_size=\(payload.count)); state{\(stateContext)} "
                            + "queues{pending_calls=\(pendingCalls.count) "
                            + "pending_task_messages=\(pendingTaskMessages.count)}; closing connection"
                    )
                    try await sendProtocolError("call.lifecycle.unknown-request-id")
                    throw ConnectionError.protocolViolation(rule: "call.lifecycle.unknown-request-id")
                }
                pending.timeoutTask?.cancel()
                pending.responseTx(.success(payload))
            case .cancel:
                switch await operations.cancel(requestId: request.id) {
                case .none, .detach:
                    let _ = await state.removeInFlight(request.id)
                case .keepLive:
                    break
                case .release(_, let waiters):
                    let taskTx = taskSender()
                    for waiter in waiters {
                        taskTx(.response(requestId: waiter, payload: encodeCancelledError()))
                    }
                }
            }
        case .channelMessage(let channel):
            switch channel.body {
            case .item(let item):
                try await handleData(channelId: channel.id, payload: item.item.bytes)
            case .close:
                try await handleClose(channelId: channel.id)
            case .reset:
                await serverRegistry.deliverReset(channelId: channel.id)
                await handle.channelRegistry.deliverReset(channelId: channel.id)
            case .grantCredit(let credit):
                await serverRegistry.deliverCredit(channelId: channel.id, bytes: credit.additional)
                await handle.channelRegistry.deliverCredit(channelId: channel.id, bytes: credit.additional)
            }
        }
    }

    /// r[impl rpc.flow-control.credit.exhaustion] - Payloads bounded by max_payload_size.
    /// r[impl session.connection-settings.hello] - Exceeding limit requires Goodbye.
    /// r[impl rpc.request.id-allocation] - Each request uses a unique ID.
    func handleRequest(
        connId: UInt64,
        requestId: UInt64,
        methodId: UInt64,
        metadata: [MetadataEntry],
        payload: [UInt8]
    ) async throws {
        let retry = dispatcher.retryPolicy(methodId: methodId)
        let inserted = await state.addInFlight(
            requestId,
            connectionId: connId,
            responseMetadata: responseMetadataFromRequest(metadata)
        )

        guard inserted else {
            try await sendProtocolError("call.request-id.duplicate-detection")
            throw ConnectionError.protocolViolation(rule: "call.request-id.duplicate-detection")
        }

        if payload.count > Int(negotiated.maxPayloadSize) {
            try await sendProtocolError("rpc.flow-control.credit.exhaustion")
            throw ConnectionError.protocolViolation(rule: "rpc.flow-control.credit.exhaustion")
        }

        let taskTx = taskSender()
        if let operationId = metadataOperationId(metadata) {
            switch await operations.admit(
                operationId: operationId,
                methodId: methodId,
                args: payload,
                retry: retry,
                requestId: requestId
            ) {
            case .start:
                break
            case .attached:
                return
            case .replay(let replayPayload):
                taskTx(.response(requestId: requestId, payload: replayPayload))
                return
            case .conflict:
                taskTx(.response(requestId: requestId, payload: encodeInvalidPayloadError()))
                return
            case .indeterminate:
                taskTx(.response(requestId: requestId, payload: encodeIndeterminateError()))
                return
            }
        }

        await dispatcher.preregister(
            methodId: methodId,
            payload: payload,
            registry: serverRegistry
        )

        Task {
            await dispatcher.dispatch(
                methodId: methodId,
                payload: payload,
                requestId: requestId,
                registry: serverRegistry,
                schemaSendTracker: schemaSendTracker,
                taskTx: taskTx
            )
        }
    }

    /// r[impl rpc.channel.allocation] - Channel ID 0 is reserved.
    /// r[impl rpc.metadata.unknown] - Unknown channel IDs cause Goodbye.
    /// r[impl rpc.channel.item] - Data messages routed by channel_id.
    func handleData(channelId: UInt64, payload: [UInt8]) async throws {
        if channelId == 0 {
            try await sendProtocolError("rpc.channel.allocation")
            throw ConnectionError.protocolViolation(rule: "rpc.channel.allocation")
        }

        traceLog(.driver, "handleData: channelId=\(channelId)")
        var delivered = await serverRegistry.deliverData(channelId: channelId, payload: payload)
        if !delivered {
            delivered = await handle.channelRegistry.deliverData(
                channelId: channelId, payload: payload)
        }

        if !delivered {
            traceLog(.driver, "handleData: unknown channelId=\(channelId)")
            try await sendProtocolError("rpc.metadata.unknown")
            throw ConnectionError.protocolViolation(rule: "rpc.metadata.unknown")
        }
    }

    /// r[impl rpc.channel.allocation] - Channel ID 0 is reserved.
    /// r[impl rpc.metadata.unknown] - Unknown channel IDs cause Goodbye.
    /// r[impl rpc.channel.close] - Close terminates the channel.
    func handleClose(channelId: UInt64) async throws {
        if channelId == 0 {
            try await sendProtocolError("rpc.channel.allocation")
            throw ConnectionError.protocolViolation(rule: "rpc.channel.allocation")
        }

        var delivered = await serverRegistry.deliverClose(channelId: channelId)
        if !delivered {
            delivered = await handle.channelRegistry.deliverClose(channelId: channelId)
        }

        if !delivered {
            try await sendProtocolError("rpc.metadata.unknown")
            throw ConnectionError.protocolViolation(rule: "rpc.metadata.unknown")
        }
    }

    /// r[impl connection.close.semantics] - Send Goodbye with rule ID before closing.
    /// r[impl session.protocol-error] - Reason contains violated rule ID.
    func sendProtocolError(_ reason: String) async throws {
        try await conduit.send(.protocolError(description: reason))
    }

    func failAllPending() async {
        await handle.closeRequestSemaphore()

        let responses = await state.claimAllPendingResponses(reason: "connection-closed")

        for (_, pending) in responses {
            pending.timeoutTask?.cancel()
            pending.responseTx(.failure(.connectionClosed))
        }
    }
}
