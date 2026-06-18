import Foundation

extension Driver {
    // r[impl rpc.flow-control]
    private func sendOrEnqueue(_ message: Message) async throws {
        if !pendingTaskMessages.isEmpty {
            pendingTaskMessages.append(DriverQueuedWireMessage(message: message))
            return
        }

        do {
            try await conduit.send(message)
        } catch TransportError.wouldBlock {
            pendingTaskMessages.append(DriverQueuedWireMessage(message: message))
        } catch {
            throw error
        }
    }

    private func responseMessage(
        requestId: UInt64,
        payload: [UInt8],
        schemas: [UInt8] = []
    ) async -> Message? {
        // r[impl rpc.response]
        let responseContext = await state.removeInFlight(requestId)
        guard responseContext.removed else {
            return nil
        }
        await terminateRequestChannels(
            laneId: responseContext.laneId,
            channelIds: responseContext.channels,
            error: .requestClosed
        )
        return messageResponse(
            requestId: requestId,
            payload: payload,
            metadata: responseContext.responseMetadata,
            laneId: responseContext.laneId,
            schemas: schemas
        )
    }

    private func commandSender() -> @Sendable (HandleCommand) -> Bool {
        let cont = eventContinuation
        let queue = commandQueue
        return { command in
            guard queue.push(command) else {
                return false
            }
            let result = cont.yield(.wake)
            guard case .terminated = result else {
                return true
            }
            return false
        }
    }

    private func taskQueueSender(laneId: UInt64) -> @Sendable (TaskMessage) -> Bool {
        let cont = eventContinuation
        let queue = taskQueue
        return { msg in
            guard queue.push(DriverQueuedTaskMessage(laneId: laneId, taskMessage: msg))
            else {
                return false
            }
            let result = cont.yield(.wake)
            guard case .terminated = result else {
                return true
            }
            return false
        }
    }

    // r[impl rpc.observability.channel.context]
    func rememberChannelContexts(
        registry: ChannelRegistry,
        laneId: UInt64,
        requestId: UInt64,
        methodId: UInt64,
        channels: [UInt64],
        side: String
    ) async {
        let context = VoxChannelDebugContext(
            laneId: laneId,
            requestId: requestId,
            methodId: methodId,
            side: side
        )
        for channelId in channels {
            await registry.rememberContext(channelId, context)
        }
    }

    // r[impl rpc.observability.channel.context]
    func rememberOutboundChannelContexts(
        laneId: UInt64,
        requestId: UInt64,
        methodId: UInt64,
        channels: [UInt64]
    ) async {
        guard !channels.isEmpty else {
            return
        }
        if laneId == 0 {
            await rememberChannelContexts(
                registry: handle.channelRegistry,
                laneId: laneId,
                requestId: requestId,
                methodId: methodId,
                channels: channels,
                side: "client"
            )
            return
        }
        guard let lane = await laneState.lane(for: laneId) else {
            return
        }
        await rememberChannelContexts(
            registry: lane.channelRegistry,
            laneId: laneId,
            requestId: requestId,
            methodId: methodId,
            channels: channels,
            side: "client"
        )
    }

    func channelContext(
        laneId: UInt64,
        channelId: UInt64
    ) async -> VoxChannelDebugContext? {
        if laneId == 0 {
            if let context = await handle.channelRegistry.context(for: channelId) {
                return context
            }
            return await serverRegistry.context(for: channelId)
        }
        guard let lane = await laneState.lane(for: laneId) else {
            return nil
        }
        return await lane.channelRegistry.context(for: channelId)
    }

    // r[impl rpc.observability.channel]
    private func observeOutgoingChannelMessage(_ msg: TaskMessage, laneId: UInt64) async {
        switch msg {
        case .data(let channelId, let payload):
            observeChannel(
                VoxChannelObserverEvent(
                    kind: .send,
                    channelId: channelId,
                    direction: .outgoing,
                    bytes: payload.count,
                    context: await channelContext(laneId: laneId, channelId: channelId)
                ))
        case .close(let channelId):
            observeChannel(
                VoxChannelObserverEvent(
                    kind: .close,
                    channelId: channelId,
                    direction: .outgoing,
                    context: await channelContext(laneId: laneId, channelId: channelId)
                ))
        case .grantCredit(let channelId, let bytes):
            observeChannel(
                VoxChannelObserverEvent(
                    kind: .credit,
                    channelId: channelId,
                    direction: .outgoing,
                    additionalCredit: bytes,
                    context: await channelContext(laneId: laneId, channelId: channelId)
                ))
        case .schema, .response:
            break
        }
    }

    func makeLane(
        laneId: UInt64,
        localSettings: ConnectionSettings,
        peerSettings: ConnectionSettings
    ) -> Lane {
        let handle = LaneHandle(
            laneId: laneId,
            commandTx: commandSender(),
            taskTx: taskQueueSender(laneId: laneId),
            role: roleForParity(localSettings.parity),
            maxConcurrentRequests: peerSettings.maxConcurrentRequests
        )
        return Lane(handle: handle, schemaReceiveTracker: schemaReceiveTracker)
    }

    /// Get the task sender for handlers to send responses.
    func taskSender(laneId: UInt64) -> @Sendable (TaskMessage) -> Void {
        let sink = taskQueueSender(laneId: laneId)
        return { msg in
            _ = sink(msg)
        }
    }

    /// Handle a task message from a handler.
    /// r[impl rpc.response]
    /// r[impl rpc.channel.connection-closure]
    func handleTaskMessage(_ queued: DriverQueuedTaskMessage) async throws {
        let msg = queued.taskMessage
        let laneId = queued.laneId
        let wireMsg: Message
        let progressChannelId: UInt64?
        switch msg {
        case .data(let channelId, let payload):
            wireMsg = messageData(channelId: channelId, item: payload, laneId: laneId)
            progressChannelId = channelId
        case .close(let channelId):
            wireMsg = messageChannelClose(channelId: channelId, laneId: laneId)
            progressChannelId = channelId
        case .grantCredit(let channelId, let bytes):
            wireMsg = messageCredit(
                channelId: channelId,
                additional: bytes,
                laneId: laneId
            )
            progressChannelId = channelId
        case .schema(let methodId, let direction, let schemas):
            wireMsg = messageSchema(
                methodId: methodId,
                direction: direction,
                schemas: schemas,
                laneId: laneId
            )
            progressChannelId = nil
        case .response(let requestId, let payload, let methodId, let responseSchemaClosure):
            // Advertise the response schema at THIS sequential send point (not in the
            // concurrent dispatch task): under pipelining many responses for a method
            // are written here in order, and the first one MUST carry the schema. A
            // dispatch-time decision races — a schema-less response could be written
            // first. prepareSchemas is idempotent, so only the first send advertises.
            // r[impl schema.exchange.required]
            // r[impl schema.exchange.callee]
            let schemas: [UInt8]
            if let methodId, !responseSchemaClosure.isEmpty {
                schemas = schemaSendTracker.prepareSchemas(
                    methodId, .response, responseSchemaClosure)
            } else {
                schemas = []
            }
            debugLog(
                "send Response req=\(requestId) payloadLen=\(payload.count) "
                    + "schemasLen=\(schemas.count)")
            let checkedPayload: [UInt8]
            if payload.count > Int(negotiated.maxPayloadSize) {
                debugLog(
                    "outgoing response for request \(requestId) exceeds max_payload_size "
                        + "(\(payload.count) > \(negotiated.maxPayloadSize)), sending Cancelled")
                // Replace the over-sized payload with a typed `Cancelled` VoxError (its
                // Err arm is T-independent on the wire, so any method's response program
                // encodes it).
                checkedPayload = dispatcher.encodeVoxError(.cancelled)
            } else {
                checkedPayload = payload
            }
            guard
                let response = await responseMessage(
                    requestId: requestId,
                    payload: checkedPayload,
                    schemas: schemas
                )
            else {
                return
            }
            wireMsg = response
            progressChannelId = nil
        }
        try await sendOrEnqueue(wireMsg)
        await observeOutgoingChannelMessage(msg, laneId: laneId)
        if let progressChannelId {
            await markChannelRequestProgress(
                laneId: laneId,
                channelId: progressChannelId
            )
        }
    }

    /// Handle a command from a lane or connection handle.
    /// r[impl rpc.caller]
    /// r[impl rpc.request]
    /// r[impl rpc.pipelining]
    func handleCommand(_ cmd: HandleCommand) async {
        switch cmd {
        case .call(
            let laneId, let requestId, let methodId, let metadata, let payload, let channels,
            let timeout, let responseTx, let schemaInfo):
            let isClosed = await state.isConnectionClosed()
            guard !isClosed else {
                responseTx(.failure(.connectionClosed))
                return
            }
            if laneId != 0 {
                let isLaneOpen = await laneState.contains(laneId)
                guard isLaneOpen else {
                    responseTx(.failure(.connectionClosed))
                    return
                }
            }

            let queuedCall = DriverQueuedCall(
                laneId: laneId,
                requestId: requestId,
                methodId: methodId,
                metadata: metadata,
                payload: payload,
                channels: channels,
                timeout: timeout,
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
            await rememberOutboundChannelContexts(
                laneId: laneId,
                requestId: requestId,
                methodId: methodId,
                channels: channels
            )

            // Advertise the args schema closure (at most once per method, deduped).
            // r[impl schema.exchange.caller]
            let schemas: [UInt8]
            if let schemaInfo {
                schemas = schemaSendTracker.prepareSchemas(
                    methodId, .args, schemaInfo.methodSchemas.argsSchemaClosure)
            } else {
                schemas = []
            }

            let msg = messageRequest(
                requestId: requestId,
                methodId: methodId,
                payload: payload,
                metadata: metadata,
                channels: channels,
                laneId: laneId,
                schemas: schemas
            )
            do {
                try await conduit.send(msg)
            } catch TransportError.wouldBlock {
                pendingCalls.append(queuedCall)
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

            if let timeout {
                await installRequestIdleTimeout(
                    requestId: requestId,
                    timeout: timeout
                )
            }
        case .openLane(let settings, let metadata, let dispatcher, let responseTx):
            // r[impl lane.open]
            // r[impl lane.wire]
            let isClosed = await state.isConnectionClosed()
            guard !isClosed else {
                responseTx(.failure(.connectionClosed))
                return
            }
            guard settings.initialChannelCredit > 0 else {
                responseTx(
                    .failure(.protocolViolation(rule: "rpc.flow-control.credit.initial.zero"))
                )
                return
            }

            let laneId = await laneState.allocateLaneId()
            let establishmentContext = VoxEstablishmentContext(
                role: voxEstablishmentRole(role),
                phase: .serviceLaneOpen,
                laneId: laneId
            )
            let establishmentStartedAt = observeEstablishmentStarted(establishmentContext)
            await laneState.addPendingOutbound(
                laneId,
                pending: PendingOutboundLane(
                    localSettings: settings,
                    dispatcher: dispatcher,
                    establishmentContext: establishmentContext,
                    establishmentStartedAt: establishmentStartedAt,
                    responseTx: responseTx
                )
            )

            do {
                try await sendOrEnqueue(
                    messageLaneOpen(laneId: laneId, settings: settings, metadata: metadata)
                )
            } catch {
                let pending = await laneState.takePendingOutbound(laneId)
                if let pending {
                    observeEstablishmentFinished(
                        pending.establishmentContext,
                        startedAt: pending.establishmentStartedAt,
                        outcome: .error,
                        error: error
                    )
                }
                pending?.responseTx(.failure(.transportError(String(describing: error))))
            }
        case .closeLane(let laneId, let metadata, let responseTx):
            await handleLaneCloseRequest(
                laneId: laneId,
                metadata: metadata,
                responseTx: responseTx
            )
        }
    }

    func handleLaneCloseRequest(
        laneId: UInt64,
        metadata: Metadata,
        responseTx: @Sendable (Result<Void, ConnectionError>) -> Void
    ) async {
        // r[impl lane.close]
        // r[impl lane.close.semantics]
        let isClosed = await state.isConnectionClosed()
        guard !isClosed else {
            responseTx(.failure(.connectionClosed))
            return
        }

        if laneId == 0 {
            // r[impl lane.control]
            responseTx(.failure(.protocolViolation(rule: "connection.close")))
            return
        }

        guard let record = await laneState.removeLane(laneId) else {
            responseTx(.failure(.protocolViolation(rule: "connection.close")))
            return
        }
        observeLaneGrantRevocation(laneId: laneId, grant: record.laneGrant)
        await failPendingResponses(laneId: laneId)
        do {
            try await sendOrEnqueue(
                messageLaneClose(laneId: laneId, metadata: metadata))
        } catch {
            await failAllPending()
            eventContinuation.finish()
            responseTx(.failure(.transportError(String(describing: error))))
            return
        }
        responseTx(.success(()))
    }

    func flushPendingCalls() async throws {
        if pendingCalls.isEmpty {
            return
        }

        while let call = pendingCalls.first {
            // Advertise the args schema closure (at most once per method, deduped).
            // r[impl schema.exchange.caller]
            let schemas: [UInt8]
            if let schemaInfo = call.schemaInfo {
                schemas = schemaSendTracker.prepareSchemas(
                    call.methodId, .args, schemaInfo.methodSchemas.argsSchemaClosure)
            } else {
                schemas = []
            }

            let msg = messageRequest(
                requestId: call.requestId,
                methodId: call.methodId,
                payload: call.payload,
                metadata: call.metadata,
                channels: call.channels,
                laneId: call.laneId,
                schemas: schemas
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

            if let timeout = call.timeout {
                await installRequestIdleTimeout(
                    requestId: call.requestId,
                    timeout: timeout
                )
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

    // r[impl rpc.timeout.idle-progress]
    func installRequestIdleTimeout(
        requestId: UInt64,
        timeout: TimeInterval
    ) async {
        let timeoutNs = Self.timeoutToNanoseconds(timeout)
        let timeoutTask = Task { [weak self] in
            do {
                try await Task.sleep(nanoseconds: timeoutNs)
            } catch {
                return
            }
            await self?.expireRequestForIdleTimeout(requestId: requestId, timeout: timeout)
        }
        let replacement = await state.replacePendingTimeoutTask(
            requestId,
            timeoutTask: timeoutTask
        )
        guard replacement.installed else {
            timeoutTask.cancel()
            return
        }
        replacement.previous?.cancel()
    }

    // r[impl rpc.timeout.idle-progress]
    func markChannelRequestProgress(laneId: UInt64, channelId: UInt64) async {
        let contexts = await state.pendingTimeoutContexts(
            laneId: laneId,
            channelId: channelId
        )
        for context in contexts {
            await installRequestIdleTimeout(
                requestId: context.requestId,
                timeout: context.timeout
            )
        }
    }

    // r[impl rpc.timeout.idle-progress]
    // r[impl rpc.request.scope.terminal]
    // r[impl rpc.request.scope.channels]
    func expireRequestForIdleTimeout(requestId: UInt64, timeout: TimeInterval) async {
        guard let pending = await state.claimPendingResponse(requestId, reason: "timeout") else {
            return
        }
        pending.timeoutTask?.cancel()
        warnLog("request timed out request_id=\(requestId) timeout_s=\(timeout)")
        await terminateRequestChannels(
            laneId: pending.request.laneId,
            channelIds: pending.request.channels,
            error: .timedOut
        )
        pending.responseTx(.failure(.timeout))
        try? await conduit.send(
            messageCancel(requestId: requestId, laneId: pending.request.laneId)
        )
    }
}
