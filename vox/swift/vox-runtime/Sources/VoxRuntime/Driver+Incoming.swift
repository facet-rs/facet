import Foundation

extension Driver {
    // r[impl rpc.virtual-connection.accept]
    // r[impl connection.virtual]
    func addLane(
        _ connId: UInt64,
        dispatcher: any ServiceDispatcher,
        localSettings: ConnectionSettings,
        channelRegistry: ChannelRegistry = ChannelRegistry()
    ) async {
        await laneState.addLane(
            connId,
            dispatcher: dispatcher,
            localSettings: localSettings,
            channelRegistry: channelRegistry
        )
    }

    // r[impl connection.close]
    // r[impl connection.virtual]
    func removeLane(_ connId: UInt64) async {
        await laneState.removeLane(connId)
    }

    /// Handle an incoming message.
    ///
    /// r[impl connection.close.semantics] - Stop sending, close connection, fail in-flight.
    /// r[impl rpc.request] - Request before Response in message sequence.
    /// r[impl session.protocol-error] - Unknown message variant triggers Goodbye.
    /// r[impl session.message]
    /// r[impl session.message.connection-id]
    /// r[impl session.message.payloads]
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
            // session: rust/vox-core/src/session/mod.rs SchemaMessage send/recv). Record it
            // into the same receive tracker the dispatcher uses for compatibility decode. Messages are
            // delivered in order, so the schema is recorded before the Call/Response that
            // needs it is handled.
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
            // r[impl rpc.virtual-connection.accept]
            // r[impl connection.open.rejection]
            // r[impl connection.open]
            // r[impl connection.parity]
            let peerRole = oppositeRole(role)
            guard idMatchesRole(msg.connectionId, peerRole) else {
                try await sendProtocolError("connection.open")
                throw ConnectionError.protocolViolation(rule: "connection.open")
            }
            guard !(await laneState.contains(msg.connectionId)) else {
                try await sendProtocolError("connection.open")
                throw ConnectionError.protocolViolation(rule: "connection.open")
            }
            if let acceptor = laneAcceptor {
                let metadata = open.metadata
                guard let service = metadata.metaStr("vox-service") else {
                    // Missing or non-string vox-service metadata — reject
                    try await conduit.send(
                        messageReject(connectionId: msg.connectionId, metadata: .null))
                    return
                }
                guard open.connectionSettings.initialChannelCredit > 0 else {
                    try await conduit.send(
                        messageReject(connectionId: msg.connectionId, metadata: .null))
                    return
                }
                let localSettings = try makeConnectionSettings(
                    parity: oppositeParity(open.connectionSettings.parity),
                    maxConcurrentRequests: open.connectionSettings.maxConcurrentRequests,
                    initialChannelCredit: open.connectionSettings.initialChannelCredit
                )
                let request = LaneRequest(metadata: metadata, service: service)
                let connId = msg.connectionId
                let pending = PendingLane(
                    accept: { [weak self] dispatcher in
                        guard let self else { return }
                        Task {
                            await self.addLane(
                                connId,
                                dispatcher: dispatcher,
                                localSettings: localSettings
                            )
                            try? await self.conduit.send(
                                messageAccept(
                                    connectionId: connId,
                                    settings: localSettings,
                                    metadata: .null
                                ))
                        }
                    },
                    reject: { [weak self] in
                        guard let self else { return }
                        Task {
                            try? await self.conduit.send(
                                messageReject(connectionId: connId, metadata: .null))
                        }
                    }
                )
                acceptor.accept(request: request, lane: pending)
            } else {
                try await conduit.send(messageReject(connectionId: msg.connectionId, metadata: .null))
            }
        case .laneAccept(let accept):
            // r[impl connection.open]
            // r[impl rpc.virtual-connection.open]
            guard let pending = await laneState.takePendingOutbound(msg.connectionId) else {
                break
            }
            do {
                try validateInitialChannelCredit(accept.connectionSettings.initialChannelCredit)
            } catch {
                pending.responseTx(.failure(.protocolViolation(rule: "connection.open")))
                try await sendProtocolError("connection.open")
                throw ConnectionError.protocolViolation(rule: "connection.open")
            }
            let lane = makeLane(
                connectionId: msg.connectionId,
                localSettings: pending.localSettings,
                peerSettings: accept.connectionSettings
            )
            await laneState.addLane(
                msg.connectionId,
                dispatcher: pending.dispatcher ?? dispatcher,
                localSettings: pending.localSettings,
                channelRegistry: lane.incomingChannelRegistry
            )
            pending.responseTx(.success(lane))
        case .laneReject(let reject):
            // r[impl connection.open.rejection]
            guard let pending = await laneState.takePendingOutbound(msg.connectionId) else {
                break
            }
            pending.responseTx(.failure(.rejected(metadata: reject.metadata)))
        case .laneClose:
            // r[impl connection.close]
            warnLog("received LaneClose conn_id=\(msg.connectionId)")
            if msg.connectionId == 0 {
                warnLog("received LaneClose for control lane; shutting down driver")
                await failAllPending()
                throw ConnectionError.connectionClosed
            }
            await removeLane(msg.connectionId)
            await failPendingResponses(connectionId: msg.connectionId)
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
                    connId: msg.connectionId,
                    requestId: request.id,
                    methodId: call.methodId,
                    metadata: call.metadata,
                    payload: argsBytes,
                    channels: call.channels
                )
            case .response(let response):
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
                let responseContext = await state.removeInFlight(request.id)
                await terminateRequestChannels(
                    connectionId: responseContext.connectionId,
                    channelIds: responseContext.channels,
                    error: .cancelled
                )
            }
        case .channelMessage(let channel):
            switch channel.body {
            case .item(let item):
                let itemBytes = [UInt8](item.item)
                try await handleData(
                    connectionId: msg.connectionId,
                    channelId: channel.id,
                    payload: itemBytes
                )
            case .close:
                try await handleClose(connectionId: msg.connectionId, channelId: channel.id)
            case .reset:
                await deliverChannelReset(connectionId: msg.connectionId, channelId: channel.id)
            case .grantCredit(let credit):
                await deliverChannelCredit(
                    connectionId: msg.connectionId,
                    channelId: channel.id,
                    bytes: credit.additional
                )
            }
        }
    }

    /// r[impl session.connection-settings.hello] - Exceeding limit requires Goodbye.
    /// r[impl rpc.request.id-allocation] - Each request uses a unique ID.
    /// r[impl rpc]
    /// r[impl rpc.service]
    /// r[impl rpc.handler]
    /// r[impl rpc.service.methods]
    /// r[impl lane.service]
    /// r[impl rpc.pipelining]
    func handleRequest(
        connId: UInt64,
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
        if connId == 0 {
            effectiveDispatcher = dispatcher
            localMaxConcurrentRequests =
                localControlSettings?.maxConcurrentRequests ?? negotiated.maxConcurrentRequests
            channelRegistry = serverRegistry
        } else if let vconn = await laneState.lane(for: connId) {
            effectiveDispatcher = vconn.dispatcher
            localMaxConcurrentRequests = vconn.localSettings.maxConcurrentRequests
            channelRegistry = vconn.channelRegistry
        } else {
            try await sendProtocolError("call.lifecycle.unknown-connection-id")
            throw ConnectionError.protocolViolation(
                rule: "call.lifecycle.unknown-connection-id")
        }

        let addInFlight = await state.addInFlight(
            requestId,
            connectionId: connId,
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

        let taskTx = taskSender(connectionId: connId)
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
                taskTx: taskTx
            )
        }
    }

    /// r[impl rpc.channel.allocation] - Channel ID 0 is reserved.
    /// r[impl rpc.channel.item] - Data messages routed by channel_id; an item for a
    /// not-yet-bound channel is buffered (its declaring Call may not be processed yet).
    /// r[impl rpc.flow-control]
    func handleData(connectionId: UInt64, channelId: UInt64, payload: [UInt8]) async throws {
        if channelId == 0 {
            try await sendProtocolError("rpc.channel.allocation")
            throw ConnectionError.protocolViolation(rule: "rpc.channel.allocation")
        }

        traceLog(.driver, "handleData: channelId=\(channelId)")
        let delivered = await deliverChannelData(
            connectionId: connectionId,
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
    func handleClose(connectionId: UInt64, channelId: UInt64) async throws {
        if channelId == 0 {
            try await sendProtocolError("rpc.channel.allocation")
            throw ConnectionError.protocolViolation(rule: "rpc.channel.allocation")
        }

        let delivered = await deliverChannelClose(connectionId: connectionId, channelId: channelId)

        if !delivered {
            try await sendProtocolError("rpc.channel.item")
            throw ConnectionError.protocolViolation(rule: "rpc.channel.item")
        }
    }

    private func deliverChannelData(
        connectionId: UInt64,
        channelId: UInt64,
        payload: [UInt8]
    ) async -> Bool {
        if connectionId == 0 {
            if await serverRegistry.deliverData(channelId: channelId, payload: payload) {
                return true
            }
            return await handle.channelRegistry.deliverData(channelId: channelId, payload: payload)
        }

        guard let lane = await laneState.lane(for: connectionId) else {
            return false
        }
        return await lane.channelRegistry.deliverData(channelId: channelId, payload: payload)
    }

    private func deliverChannelClose(connectionId: UInt64, channelId: UInt64) async -> Bool {
        if connectionId == 0 {
            if await serverRegistry.deliverClose(channelId: channelId) {
                return true
            }
            return await handle.channelRegistry.deliverClose(channelId: channelId)
        }

        guard let lane = await laneState.lane(for: connectionId) else {
            return false
        }
        return await lane.channelRegistry.deliverClose(channelId: channelId)
    }

    func terminateRequestChannels(
        connectionId: UInt64,
        channelIds: [UInt64],
        error: ChannelError
    ) async {
        for channelId in channelIds {
            await deliverChannelReset(
                connectionId: connectionId,
                channelId: channelId,
                error: error
            )
        }
    }

    private func deliverChannelReset(
        connectionId: UInt64,
        channelId: UInt64,
        error: ChannelError = .reset
    ) async {
        if connectionId == 0 {
            await serverRegistry.deliverReset(channelId: channelId, error: error)
            await handle.channelRegistry.deliverReset(channelId: channelId, error: error)
            return
        }

        guard let lane = await laneState.lane(for: connectionId) else {
            return
        }
        await lane.channelRegistry.deliverReset(channelId: channelId, error: error)
    }

    private func deliverChannelCredit(connectionId: UInt64, channelId: UInt64, bytes: UInt32) async {
        if connectionId == 0 {
            await serverRegistry.deliverCredit(channelId: channelId, bytes: bytes)
            await handle.channelRegistry.deliverCredit(channelId: channelId, bytes: bytes)
            return
        }

        guard let lane = await laneState.lane(for: connectionId) else {
            return
        }
        await lane.channelRegistry.deliverCredit(channelId: channelId, bytes: bytes)
    }

    /// r[impl connection.close.semantics] - Send Goodbye with rule ID before closing.
    /// r[impl session.protocol-error] - Reason contains violated rule ID.
    func sendProtocolError(_ reason: String) async throws {
        try await conduit.send(messageProtocolError(description: reason))
    }

    func failAllPending() async {
        // r[impl rpc.flow-control.max-concurrent-requests.session-failure]
        // r[impl rpc.channel.connection-closure]
        await handle.closeRequestSemaphore()

        let responses = await state.claimAllPendingResponses(reason: "connection-closed")

        for (_, pending) in responses {
            pending.timeoutTask?.cancel()
            pending.responseTx(.failure(.connectionClosed))
        }
    }

    func failPendingResponses(connectionId: UInt64) async {
        let responses = await state.claimPendingResponses(
            connectionId: connectionId,
            reason: "connection-closed"
        )

        for (_, pending) in responses {
            pending.timeoutTask?.cancel()
            pending.responseTx(.failure(.connectionClosed))
        }
    }

}
