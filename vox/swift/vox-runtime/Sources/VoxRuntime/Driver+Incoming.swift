import Foundation

extension Driver {
    // r[impl lane.accept.api]
    // r[impl lane.service.compat]
    func addLane(
        _ laneId: UInt64,
        dispatcher: any ServiceDispatcher,
        localSettings: ConnectionSettings,
        channelRegistry: ChannelRegistry = ChannelRegistry(),
        laneGrant: LaneGrant = .empty
    ) async {
        await laneState.addLane(
            laneId,
            dispatcher: dispatcher,
            localSettings: localSettings,
            channelRegistry: channelRegistry,
            laneGrant: laneGrant
        )
    }

    // r[impl lane.close]
    // r[impl lane.service.compat]
    func removeLane(_ laneId: UInt64) async {
        if let record = await laneState.removeLane(laneId) {
            observeLaneGrantRevocation(laneId: laneId, grant: record.laneGrant)
        }
    }

    func observeLaneGrantRevocation(laneId: UInt64, grant: LaneGrant) {
        guard !grant.metadata.metaIsEmpty else { return }
        let context = VoxEstablishmentContext(
            role: voxEstablishmentRole(role),
            phase: .laneGrantRevocation,
            laneId: laneId
        )
        let startedAt = observeEstablishmentStarted(context)
        observeEstablishmentFinished(context, startedAt: startedAt, outcome: .ok)
    }

    func observeLaneGrantCreation(laneId: UInt64, grant: LaneGrant) {
        guard !grant.metadata.metaIsEmpty else { return }
        let context = VoxEstablishmentContext(
            role: voxEstablishmentRole(role),
            phase: .laneGrant,
            laneId: laneId
        )
        let startedAt = observeEstablishmentStarted(context)
        observeEstablishmentFinished(context, startedAt: startedAt, outcome: .ok)
    }

    /// Handle an incoming message.
    ///
    /// r[impl lane.close.semantics] - Stop sending, close connection, fail in-flight.
    /// r[impl rpc.request] - Request before Response in message sequence.
    /// r[impl connection.protocol-error] - Unknown message variant triggers Goodbye.
    /// r[impl connection.message]
    /// r[impl connection.message.lane-id]
    /// r[impl connection.message.payloads]
    /// r[impl rpc.cancel.channels]
    /// r[impl rpc.channel.connection-closure]
    func handleMessage(
        _ msg: Message,
        keepaliveRuntime: inout DriverKeepaliveRuntime?
    ) async throws {
        switch msg.payload {
        case .schemaMessage(let schema):
            // The peer advertises a binding's (writer) schema closure out-of-band, as a
            // standalone message sent before the payload it describes (mirrors the Rust
            // connection runtime SchemaMessage send/recv). Record it into the same
            // receive tracker the dispatcher uses for compatibility decode. Messages
            // are delivered in order, so the schema is recorded before the
            // Call/Response that needs it is handled.
            // r[impl schema.tracking.received]
            let dir: SchemaBindingDirection
            switch schema.direction {
            case .args: dir = .args
            case .response: dir = .response
            }
            debugLog(
                "recv SchemaMessage method=\(schema.methodId) dir=\(dir) "
                    + "schemasLen=\(schema.schemas.count)")
            if !schema.schemas.isEmpty {
                schemaReceiveTracker.recordReceived(schema.methodId, dir, [UInt8](schema.schemas))
            }
        case .ping(let ping):
            do {
                try await conduit.send(messagePong(nonce: ping.nonce))
            } catch TransportError.wouldBlock {
                pendingTaskMessages.append(
                    DriverQueuedWireMessage(message: messagePong(nonce: ping.nonce)))
            }
        case .pong(let pong):
            handlePong(nonce: pong.nonce, keepaliveRuntime: &keepaliveRuntime)
        case .protocolError(let error):
            await failAllPending()
            throw ConnectionError.protocolViolation(rule: error.description)
        case .laneOpen(let open):
            // r[impl lane.accept.api]
            // r[impl lane.open.wire.rejection]
            // r[impl lane.open.wire]
            // r[impl lane.request-channel-parity]
            // r[impl lane.open]
            // r[impl lane.wire.compat]
            let establishmentContext = VoxEstablishmentContext(
                role: voxEstablishmentRole(role),
                phase: .serviceLaneOpen,
                laneId: msg.laneId
            )
            let establishmentStartedAt = observeEstablishmentStarted(establishmentContext)
            let peerRole = oppositeRole(role)
            guard idMatchesRole(msg.laneId, peerRole) else {
                observeEstablishmentFinished(
                    establishmentContext,
                    startedAt: establishmentStartedAt,
                    outcome: .error
                )
                try await sendProtocolError("connection.open")
                throw ConnectionError.protocolViolation(rule: "connection.open")
            }
            guard !(await laneState.contains(msg.laneId)) else {
                observeEstablishmentFinished(
                    establishmentContext,
                    startedAt: establishmentStartedAt,
                    outcome: .error
                )
                try await sendProtocolError("connection.open")
                throw ConnectionError.protocolViolation(rule: "connection.open")
            }
            if let acceptor = laneAcceptor {
                let metadata = open.metadata
                guard let service = metadata.metaStr("vox-service") else {
                    let authorizationContext = VoxEstablishmentContext(
                        role: voxEstablishmentRole(role),
                        phase: .laneAuthorization,
                        laneId: msg.laneId
                    )
                    let authorizationStartedAt = observeEstablishmentStarted(authorizationContext)
                    let rejection = LaneRejection.withMessage(
                        .unknownService,
                        "missing vox-service metadata"
                    )
                    try await conduit.send(
                        messageLaneReject(
                            laneId: msg.laneId,
                            metadata: rejection.toMetadata()
                        ))
                    observeEstablishmentFinished(
                        authorizationContext,
                        startedAt: authorizationStartedAt,
                        outcome: .rejected
                    )
                    observeEstablishmentFinished(
                        establishmentContext,
                        startedAt: establishmentStartedAt,
                        outcome: .rejected
                    )
                    return
                }
                guard open.connectionSettings.initialChannelCredit > 0 else {
                    let rejection = LaneRejection.withMessage(
                        .policyRejected,
                        "initial_channel_credit must be greater than zero"
                    )
                    try await conduit.send(
                        messageLaneReject(
                            laneId: msg.laneId,
                            metadata: rejection.toMetadata()
                        ))
                    observeEstablishmentFinished(
                        establishmentContext,
                        startedAt: establishmentStartedAt,
                        outcome: .rejected
                    )
                    return
                }
                let localSettings: ConnectionSettings
                do {
                    localSettings = try makeConnectionSettings(
                        parity: oppositeParity(open.connectionSettings.parity),
                        maxConcurrentRequests: open.connectionSettings.maxConcurrentRequests,
                        initialChannelCredit: open.connectionSettings.initialChannelCredit
                    )
                } catch {
                    observeEstablishmentFinished(
                        establishmentContext,
                        startedAt: establishmentStartedAt,
                        outcome: .error,
                        error: error
                    )
                        throw error
                }
                let request = LaneRequest(
                    metadata: metadata,
                    service: service,
                    peerIdentity: peerIdentity,
                    peerEvidence: peerEvidence
                )
                let laneId = msg.laneId
                let authorizationContext = VoxEstablishmentContext(
                    role: voxEstablishmentRole(role),
                    phase: .laneAuthorization,
                    laneId: laneId
                )
                let authorizationStartedAt = observeEstablishmentStarted(authorizationContext)
                let pending = PendingLane(
                    accept: { [weak self] dispatcher, grant in
                        guard let self else { return }
                        Task {
                            observeEstablishmentFinished(
                                authorizationContext,
                                startedAt: authorizationStartedAt,
                                outcome: .ok
                            )
                            await self.addLane(
                                laneId,
                                dispatcher: dispatcher,
                                localSettings: localSettings,
                                laneGrant: grant
                            )
                            let grantContext = VoxEstablishmentContext(
                                role: voxEstablishmentRole(self.role),
                                phase: .laneGrant,
                                laneId: laneId
                            )
                            let grantStartedAt = observeEstablishmentStarted(grantContext)
                            do {
                                try await self.conduit.send(
                                    messageLaneAccept(
                                        laneId: laneId,
                                        settings: localSettings,
                                        metadata: grant.metadata
                                    ))
                                observeEstablishmentFinished(
                                    grantContext,
                                    startedAt: grantStartedAt,
                                    outcome: .ok
                                )
                            } catch {
                                observeEstablishmentFinished(
                                    grantContext,
                                    startedAt: grantStartedAt,
                                    outcome: .error,
                                    error: error
                                )
                            }
                            observeEstablishmentFinished(
                                establishmentContext,
                                startedAt: establishmentStartedAt,
                                outcome: .ok
                            )
                        }
                    },
                    reject: { [weak self] rejection in
                        guard let self else { return }
                        Task {
                            observeEstablishmentFinished(
                                authorizationContext,
                                startedAt: authorizationStartedAt,
                                outcome: .rejected
                            )
                            try? await self.conduit.send(
                                messageLaneReject(
                                    laneId: laneId,
                                    metadata: rejection.toMetadata()
                                ))
                            observeEstablishmentFinished(
                                establishmentContext,
                                startedAt: establishmentStartedAt,
                                outcome: .rejected
                            )
                        }
                    }
                )
                acceptor.accept(request: request, lane: pending)
            } else {
                let authorizationContext = VoxEstablishmentContext(
                    role: voxEstablishmentRole(role),
                    phase: .laneAuthorization,
                    laneId: msg.laneId
                )
                let authorizationStartedAt = observeEstablishmentStarted(authorizationContext)
                let rejection = LaneRejection.withMessage(
                    .notReady,
                    "no lane acceptor configured"
                )
                try await conduit.send(
                    messageLaneReject(
                        laneId: msg.laneId,
                        metadata: rejection.toMetadata()
                    ))
                observeEstablishmentFinished(
                    authorizationContext,
                    startedAt: authorizationStartedAt,
                    outcome: .rejected
                )
                observeEstablishmentFinished(
                    establishmentContext,
                    startedAt: establishmentStartedAt,
                    outcome: .rejected
                )
            }
        case .laneAccept(let accept):
            // r[impl lane.open.wire]
            // r[impl lane.open.api]
            // r[impl lane.open.result]
            // r[impl lane.wire.compat]
            guard let pending = await laneState.takePendingOutbound(msg.laneId) else {
                break
            }
            do {
                try validateInitialChannelCredit(accept.connectionSettings.initialChannelCredit)
            } catch {
                observeEstablishmentFinished(
                    pending.establishmentContext,
                    startedAt: pending.establishmentStartedAt,
                    outcome: .error,
                    error: error
                )
                pending.responseTx(.failure(.protocolViolation(rule: "connection.open")))
                try await sendProtocolError("connection.open")
                throw ConnectionError.protocolViolation(rule: "connection.open")
            }
            let lane = makeLane(
                laneId: msg.laneId,
                localSettings: pending.localSettings,
                peerSettings: accept.connectionSettings
            )
            let grant = LaneGrant(metadata: accept.metadata)
            await laneState.addLane(
                msg.laneId,
                dispatcher: pending.dispatcher ?? dispatcher,
                localSettings: pending.localSettings,
                channelRegistry: lane.incomingChannelRegistry,
                laneGrant: grant
            )
            observeLaneGrantCreation(laneId: msg.laneId, grant: grant)
            observeEstablishmentFinished(
                pending.establishmentContext,
                startedAt: pending.establishmentStartedAt,
                outcome: .ok
            )
            pending.responseTx(.success(lane))
        case .laneReject(let reject):
            // r[impl lane.open.wire.rejection]
            // r[impl lane.open.result]
            // r[impl lane.wire.compat]
            guard let pending = await laneState.takePendingOutbound(msg.laneId) else {
                break
            }
            observeEstablishmentFinished(
                pending.establishmentContext,
                startedAt: pending.establishmentStartedAt,
                outcome: .rejected
            )
            pending.responseTx(.failure(.rejected(LaneRejection.fromMetadata(reject.metadata))))
        case .laneClose:
            // r[impl lane.close]
            warnLog("received LaneClose lane_id=\(msg.laneId)")
            if msg.laneId == 0 {
                warnLog("received LaneClose for control lane; shutting down driver")
                await failAllPending()
                throw ConnectionError.connectionClosed
            }
            await removeLane(msg.laneId)
            await failPendingResponses(laneId: msg.laneId)
        case .requestMessage(let request):
            switch request.body {
            case .call(let call):
                debugLog(
                    "recv Call req=\(request.id) method=\(call.methodId) "
                        + "argsLen=\(call.args.count) schemasLen=\(call.schemas.count)")
                // The peer advertised its args (writer) schema closure on this binding.
                if !call.schemas.isEmpty {
                    schemaReceiveTracker.recordReceived(call.methodId, .args, [UInt8](call.schemas))
                }
                let argsBytes = [UInt8](call.args)
                try await handleRequest(
                    laneId: msg.laneId,
                    requestId: request.id,
                    methodId: call.methodId,
                    metadata: call.metadata,
                    payload: argsBytes,
                    channels: call.channels
                )
            case .response(let response):
                // r[impl rpc.request.scope.terminal]
                // r[impl rpc.request.scope.channels]
                // r[impl rpc.response]
                let payload = [UInt8](response.ret)
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
                    throw ConnectionError.protocolViolation(
                        rule: "call.lifecycle.unknown-request-id")
                }
                // The server advertised its (writer) response schema on this binding;
                // record it so the generated client builds the response compatibility decode.
                if !response.schemas.isEmpty {
                    schemaReceiveTracker.recordReceived(
                        pending.request.methodId, .response, [UInt8](response.schemas))
                }
                pending.timeoutTask?.cancel()
                pending.responseTx(.success(payload))
            case .cancel:
                // r[impl rpc.cancel]
                // r[impl rpc.request.scope.terminal]
                // r[impl rpc.request.scope.channels]
                let responseContext = await state.removeInFlight(request.id)
                await terminateRequestChannels(
                    laneId: responseContext.laneId,
                    channelIds: responseContext.channels,
                    error: .cancelled
                )
            }
        case .channelMessage(let channel):
            switch channel.body {
            case .item(let item):
                let itemBytes = [UInt8](item.item)
                try await handleData(
                    laneId: msg.laneId,
                    channelId: channel.id,
                    payload: itemBytes
                )
                await markChannelRequestProgress(
                    laneId: msg.laneId,
                    channelId: channel.id
                )
            case .close:
                try await handleClose(laneId: msg.laneId, channelId: channel.id)
                await markChannelRequestProgress(
                    laneId: msg.laneId,
                    channelId: channel.id
                )
            case .reset:
                await deliverChannelReset(laneId: msg.laneId, channelId: channel.id)
                await markChannelRequestProgress(
                    laneId: msg.laneId,
                    channelId: channel.id
                )
            case .grantCredit(let credit):
                await deliverChannelCredit(
                    laneId: msg.laneId,
                    channelId: channel.id,
                    bytes: credit.additional
                )
                await markChannelRequestProgress(
                    laneId: msg.laneId,
                    channelId: channel.id
                )
            }
        }
    }

    /// r[impl connection.handshake.lane-settings] - Exceeding limit requires Goodbye.
    /// r[impl rpc.request.id-allocation] - Each request uses a unique ID.
    /// r[impl rpc]
    /// r[impl rpc.service]
    /// r[impl rpc.handler]
    /// r[impl rpc.service.methods]
    /// r[impl lane.service]
    /// r[impl rpc.pipelining]
    /// r[impl request.authorization]
    func handleRequest(
        laneId: UInt64,
        requestId: UInt64,
        methodId: UInt64,
        metadata: Metadata,
        payload: [UInt8],
        channels: [UInt64] = []
    ) async throws {
        // Resolve which dispatcher handles this connection.
        let effectiveDispatcher: any ServiceDispatcher
        let localMaxConcurrentRequests: UInt32
        let channelRegistry: ChannelRegistry
        let laneGrant: LaneGrant
        if laneId == 0 {
            effectiveDispatcher = dispatcher
            localMaxConcurrentRequests =
                localControlSettings?.maxConcurrentRequests ?? negotiated.maxConcurrentRequests
            channelRegistry = serverRegistry
            laneGrant = .empty
        } else if let laneRecord = await laneState.lane(for: laneId) {
            effectiveDispatcher = laneRecord.dispatcher
            localMaxConcurrentRequests = laneRecord.localSettings.maxConcurrentRequests
            channelRegistry = laneRecord.channelRegistry
            laneGrant = laneRecord.laneGrant
        } else {
            try await sendProtocolError("call.lifecycle.unknown-connection-id")
            throw ConnectionError.protocolViolation(
                rule: "call.lifecycle.unknown-connection-id")
        }

        let addInFlight = await state.addInFlight(
            requestId,
            laneId: laneId,
            responseMetadata: responseMetadataFromRequest(metadata),
            channels: channels,
            localMaxConcurrentRequests: localMaxConcurrentRequests
        )

        switch addInFlight {
        case .inserted:
            break
        case .duplicate:
            try await sendProtocolError("call.request-id.duplicate-detection")
            throw ConnectionError.protocolViolation(rule: "call.request-id.duplicate-detection")
        case .limitExceeded:
            // r[impl rpc.flow-control.max-concurrent-requests.inbound]
            try await sendProtocolError("rpc.flow-control.max-concurrent-requests.inbound")
            throw ConnectionError.protocolViolation(
                rule: "rpc.flow-control.max-concurrent-requests.inbound")
        }

        if payload.count > Int(negotiated.maxPayloadSize) {
            try await sendProtocolError("rpc.flow-control.credit.exhaustion")
            throw ConnectionError.protocolViolation(rule: "rpc.flow-control.credit.exhaustion")
        }

        await rememberChannelContexts(
            registry: channelRegistry,
            laneId: laneId,
            requestId: requestId,
            methodId: methodId,
            channels: channels,
            side: "server"
        )

        let taskTx = taskSender(laneId: laneId)
        let requestContext = RequestContext(
            methodId: methodId,
            requestId: requestId,
            laneId: laneId,
            metadata: metadata,
            authorization: RequestAuthorizationContext(
                peerIdentity: peerIdentity,
                peerEvidence: peerEvidence,
                laneGrant: laneGrant
            )
        )
        traceLog(.driver, "handleRequest method=\(methodId) req=\(requestId) channels=\(channels)")
        await effectiveDispatcher.preregister(
            methodId: methodId,
            payload: payload,
            channels: channels,
            registry: channelRegistry
        )
        traceLog(.driver, "preregistered req=\(requestId) channels=\(channels)")

        Task {
            await effectiveDispatcher.dispatch(
                methodId: methodId,
                payload: payload,
                requestId: requestId,
                channels: channels,
                registry: channelRegistry,
                schemaSendTracker: schemaSendTracker,
                schemaReceiveTracker: schemaReceiveTracker,
                context: requestContext,
                taskTx: taskTx
            )
        }
    }

    /// r[impl rpc.channel.allocation] - Channel ID 0 is reserved.
    /// r[impl rpc.channel.item] - Data messages routed by channel_id; an item for a
    /// not-yet-bound channel is buffered (its declaring Call may not be processed yet).
    /// r[impl rpc.flow-control]
    func handleData(laneId: UInt64, channelId: UInt64, payload: [UInt8]) async throws {
        if channelId == 0 {
            try await sendProtocolError("rpc.channel.allocation")
            throw ConnectionError.protocolViolation(rule: "rpc.channel.allocation")
        }

        traceLog(.driver, "handleData: channelId=\(channelId)")
        let delivered = await deliverChannelData(
            laneId: laneId,
            channelId: channelId,
            payload: payload
        )

        if !delivered {
            // A channel item for a channel not opened by a preceding Call is a sender
            // ordering violation — frames on a connection are ordered, so the Call that
            // declares a channel MUST precede any item on it. Reject hard (do not
            // buffer): ordering is the sender's responsibility, not the receiver's.
            traceLog(.driver, "handleData: item for unopened channelId=\(channelId)")
            try await sendProtocolError("rpc.channel.item")
            throw ConnectionError.protocolViolation(rule: "rpc.channel.item")
        }
    }

    /// r[impl rpc.channel.allocation] - Channel ID 0 is reserved.
    /// r[impl rpc.channel.close] - Close terminates the channel.
    /// r[impl rpc.channel.connection-closure]
    func handleClose(laneId: UInt64, channelId: UInt64) async throws {
        if channelId == 0 {
            try await sendProtocolError("rpc.channel.allocation")
            throw ConnectionError.protocolViolation(rule: "rpc.channel.allocation")
        }

        let delivered = await deliverChannelClose(laneId: laneId, channelId: channelId)

        if !delivered {
            try await sendProtocolError("rpc.channel.item")
            throw ConnectionError.protocolViolation(rule: "rpc.channel.item")
        }
    }

    private func deliverChannelData(
        laneId: UInt64,
        channelId: UInt64,
        payload: [UInt8]
    ) async -> Bool {
        if laneId == 0 {
            if await serverRegistry.deliverData(channelId: channelId, payload: payload) {
                return true
            }
            return await handle.channelRegistry.deliverData(channelId: channelId, payload: payload)
        }

        guard let lane = await laneState.lane(for: laneId) else {
            return false
        }
        return await lane.channelRegistry.deliverData(channelId: channelId, payload: payload)
    }

    private func deliverChannelClose(laneId: UInt64, channelId: UInt64) async -> Bool {
        if laneId == 0 {
            if await serverRegistry.deliverClose(channelId: channelId) {
                return true
            }
            return await handle.channelRegistry.deliverClose(channelId: channelId)
        }

        guard let lane = await laneState.lane(for: laneId) else {
            return false
        }
        return await lane.channelRegistry.deliverClose(channelId: channelId)
    }

    func terminateRequestChannels(
        laneId: UInt64,
        channelIds: [UInt64],
        error: ChannelError
    ) async {
        // r[impl rpc.request.scope.channels]
        // r[impl rpc.channel.lifecycle]
        for channelId in channelIds {
            await deliverChannelReset(
                laneId: laneId,
                channelId: channelId,
                error: error
            )
        }
    }

    private func deliverChannelReset(
        laneId: UInt64,
        channelId: UInt64,
        error: ChannelError = .reset
    ) async {
        if laneId == 0 {
            await serverRegistry.deliverReset(channelId: channelId, error: error)
            await handle.channelRegistry.deliverReset(channelId: channelId, error: error)
            return
        }

        guard let lane = await laneState.lane(for: laneId) else {
            return
        }
        await lane.channelRegistry.deliverReset(channelId: channelId, error: error)
    }

    private func deliverChannelCredit(laneId: UInt64, channelId: UInt64, bytes: UInt32) async {
        if laneId == 0 {
            await serverRegistry.deliverCredit(channelId: channelId, bytes: bytes)
            await handle.channelRegistry.deliverCredit(channelId: channelId, bytes: bytes)
            return
        }

        guard let lane = await laneState.lane(for: laneId) else {
            return
        }
        await lane.channelRegistry.deliverCredit(channelId: channelId, bytes: bytes)
    }

    /// r[impl lane.close.semantics] - Send Goodbye with rule ID before closing.
    /// r[impl connection.protocol-error] - Reason contains violated rule ID.
    func sendProtocolError(_ reason: String) async throws {
        try await conduit.send(messageProtocolError(description: reason))
    }

    func failAllPending() async {
        // r[impl rpc.flow-control.max-concurrent-requests.connection-failure]
        // r[impl rpc.channel.connection-closure]
        let lanes = await laneState.drainLanes()
        for (laneId, record) in lanes {
            observeLaneGrantRevocation(laneId: laneId, grant: record.laneGrant)
        }

        await handle.closeRequestSemaphore()

        let responses = await state.claimAllPendingResponses(reason: "connection-closed")

        for (_, pending) in responses {
            pending.timeoutTask?.cancel()
            await terminateRequestChannels(
                laneId: pending.request.laneId,
                channelIds: pending.request.channels,
                error: .connectionClosed
            )
            pending.responseTx(.failure(.connectionClosed))
        }
    }

    func failPendingResponses(laneId: UInt64) async {
        let responses = await state.claimPendingResponses(
            laneId: laneId,
            reason: "connection-closed"
        )

        for (_, pending) in responses {
            pending.timeoutTask?.cancel()
            await terminateRequestChannels(
                laneId: pending.request.laneId,
                channelIds: pending.request.channels,
                error: .connectionClosed
            )
            pending.responseTx(.failure(.connectionClosed))
        }
    }

}
