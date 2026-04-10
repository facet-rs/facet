import Foundation

extension Driver {
    func drainInjectedQueues() async throws {
        let commands = commandQueue.popAll()
        for command in commands {
            await handleCommand(command)
        }
        let taskMessages = taskQueue.popAll()
        for message in taskMessages {
            try await handleTaskMessage(message)
        }
    }

    /// Spawn a reader task that reads from the conduit and yields events.
    private func spawnReaderTask(
        for conduit: any Conduit,
        continuation: AsyncStream<DriverEvent>.Continuation
    ) -> Task<Void, Never> {
        Task {
            do {
                while !Task.isCancelled {
                    if let msg = try await conduit.recv() {
                        traceLog(.driver, "reader received message")
                        continuation.yield(.incomingMessage(msg))
                    } else {
                        traceLog(.driver, "reader observed conduit close")
                        continuation.yield(.conduitClosed)
                        break
                    }
                }
            } catch {
                if !Task.isCancelled {
                    traceLog(.driver, "reader failed: \(String(describing: error))")
                    continuation.yield(.conduitFailed(String(describing: error)))
                }
            }
        }
    }

    /// Try to recover the conduit using the recovery callback.
    ///
    /// If the original session had a resume key (peer supports resumption),
    /// we do a protocol-level resume handshake. If not (peer only supports
    /// fresh sessions), we do a plain fresh handshake and rely on in-flight
    /// request replay to recover pending calls.
    private func tryRecoverConduit() async -> (any Conduit)? {
        guard let recoverAttachment, let localRootSettings, let peerRootSettings, let transport
        else {
            traceLog(.resume, "tryRecoverConduit: missing required fields")
            return nil
        }

        do {
            let attachment = try await recoverAttachment()
            let freshFallback = {
                traceLog(.resume, "trying recoverAttachment with fresh handshake fallback")
                return try await recoverAttachment()
            }

            let establishedAttachment: LinkAttachment
            if let sessionResumeKey {
                // Try protocol-level resume first. If the peer has restarted and
                // no longer recognizes the key, fall back to a fresh handshake.
                traceLog(.resume, "trying recoverAttachment with session key")
                do {
                    let handshake = try await performInitiatorHandshake(
                        link: attachment.link,
                        maxPayloadSize: 1024 * 1024,
                        maxConcurrentRequests: localRootSettings.maxConcurrentRequests,
                        resumable: true,
                        resumeKey: sessionResumeKey
                    )
                    guard handshake.localRootSettings == localRootSettings else {
                        throw ConnectionError.protocolViolation(
                            rule: "local root settings changed across session resume"
                        )
                    }
                    guard handshake.peerRootSettings == peerRootSettings else {
                        throw ConnectionError.protocolViolation(
                            rule: "peer root settings changed across session resume"
                        )
                    }
                    establishedAttachment = attachment
                } catch {
                    traceLog(
                        .resume,
                        "resume handshake failed, retrying with fresh handshake: \(String(describing: error))"
                    )
                    let fallbackAttachment = try await freshFallback()
                    _ = try await performInitiatorHandshake(
                        link: fallbackAttachment.link,
                        maxPayloadSize: 1024 * 1024,
                        maxConcurrentRequests: localRootSettings.maxConcurrentRequests,
                        resumable: false
                    )
                    establishedAttachment = fallbackAttachment
                }
            } else {
                // Fresh-session recovery: peer doesn't support resumption.
                // Do a plain handshake; in-flight requests will be replayed.
                traceLog(.resume, "trying recoverAttachment with fresh handshake (no session key)")
                _ = try await performInitiatorHandshake(
                    link: attachment.link,
                    maxPayloadSize: 1024 * 1024,
                    maxConcurrentRequests: localRootSettings.maxConcurrentRequests,
                    resumable: false
                )
                establishedAttachment = attachment
            }

            let conduit = try await buildEstablishedConduit(
                role: role,
                transport: transport,
                attachment: establishedAttachment,
                recoverAttachment: nil
            )
            traceLog(.resume, "recoverAttachment succeeded")
            return conduit
        } catch {
            traceLog(.resume, "recoverAttachment failed: \(String(describing: error))")
            return nil
        }
    }

    /// Handle successful session resume.
    /// FIXME: it's not so much "resume" as it is "another fresh session" in reality
    private func handleSuccessfulResume(keepaliveRuntime: inout DriverKeepaliveRuntime?) async {
        traceLog(.resume, "session resumed successfully")

        // Reset schema tracker - type IDs are per-connection and must not carry over
        schemaSendTracker.reset()

        // Reset operation ID and request ID allocator
        self.handle.onConduitReset()

        // Reset operations tracker (the peer will send operation 0, 1, 2 etc. again)
        self.operations.onConduitReset()

        // r[impl retry.channel.disconnect.closes] - Close all channels on resume.
        // Channel handles become invalid on disconnect. When idem methods with channels
        // are re-run, fresh channels will be allocated and the handles will be rebound.
        await serverRegistry.closeAllChannels()
        await handle.channelRegistry.closeAllChannels()

        // Note: We do NOT clear incoming in-flight requests on the acceptor side.
        // Handlers that are still processing should complete and send their responses
        // on the new conduit. The inFlightRequests map is kept intact so responseMessage()
        // can find the request context when the handler eventually responds.

        // Replay pending outgoing calls (initiator side)
        await replayPendingCallsAfterResume()

        // Reset keepalive
        keepaliveRuntime = makeKeepaliveRuntime()
    }

    /// Run the driver until connection closes.
    public func run() async throws {
        var keepaliveRuntime = makeKeepaliveRuntime()
        traceLog(.driver, "run start")

        let cont = eventContinuation
        var readerTask = spawnReaderTask(for: conduit, continuation: cont)

        let retryTask = Task {
            while !Task.isCancelled {
                try? await Task.sleep(nanoseconds: 10_000_000)
                cont.yield(.retryTick)
            }
        }

        defer {
            readerTask.cancel()
            retryTask.cancel()
            commandQueue.close()
            taskQueue.close()
            eventContinuation.finish()
        }

        // State: are we currently connected or waiting for resume?
        var isConnected = true
        var needsRecoveryAttempt = false

        do {
            for await event in eventStream {
                // Handle disconnected state
                if !isConnected {
                    switch event {
                    case .resumeConduit(let newConduit):
                        traceLog(.resume, "received resume conduit while disconnected")
                        self.conduit = newConduit
                        readerTask.cancel()
                        await handleSuccessfulResume(keepaliveRuntime: &keepaliveRuntime)
                        readerTask = spawnReaderTask(for: newConduit, continuation: cont)
                        isConnected = true
                        continue

                    case .wake:
                        // While disconnected, try recovery if we haven't yet
                        if needsRecoveryAttempt {
                            needsRecoveryAttempt = false
                            if let recovered = await tryRecoverConduit() {
                                traceLog(.resume, "recovery succeeded while disconnected")
                                self.conduit = recovered
                                readerTask.cancel()
                                await handleSuccessfulResume(keepaliveRuntime: &keepaliveRuntime)
                                readerTask = spawnReaderTask(for: recovered, continuation: cont)
                                isConnected = true
                                continue
                            }
                        }
                        // Drain queues but reject new calls
                        let commands = commandQueue.popAll()
                        for command in commands {
                            if case .call(_, _, _, _, _, _, _, let responseTx, _) = command {
                                responseTx(.failure(.connectionClosed))
                            }
                        }

                    case .retryTick:
                        // Retry recovery on each tick while disconnected
                        needsRecoveryAttempt = true
                        cont.yield(.wake)

                    case .incomingMessage, .conduitClosed, .conduitFailed:
                        // These shouldn't happen while disconnected, ignore
                        break
                    }
                    continue
                }

                // Connected state - normal processing
                try await drainInjectedQueues()
                try await flushPendingTaskMessages()
                try await flushPendingCalls()

                switch event {
                case .incomingMessage(let msg):
                    try await handleMessage(msg, keepaliveRuntime: &keepaliveRuntime)

                case .wake:
                    break

                case .retryTick:
                    try await handleKeepaliveTick(keepaliveRuntime: &keepaliveRuntime)

                case .resumeConduit(let newConduit):
                    // Resume received while connected - replace conduit
                    traceLog(.resume, "received resume conduit while connected")
                    self.conduit = newConduit
                    readerTask.cancel()
                    await handleSuccessfulResume(keepaliveRuntime: &keepaliveRuntime)
                    readerTask = spawnReaderTask(for: newConduit, continuation: cont)

                case .conduitClosed, .conduitFailed:
                    traceLog(.driver, "conduit broke")
                    if resumable {
                        // Enter disconnected state
                        isConnected = false
                        needsRecoveryAttempt = true
                        traceLog(.resume, "entered disconnected state; scheduling recovery")
                        // Trigger a wake to attempt recovery
                        cont.yield(.wake)
                    } else {
                        // Not resumable - exit
                        await failAllPending()
                        eventContinuation.finish()
                    }
                }
            }
        } catch {
            traceLog(.driver, "run threw: \(String(describing: error))")
            eventContinuation.finish()
            await failAllPending()
            try? await conduit.close()
            throw error
        }
        traceLog(.driver, "run exiting")
        await failAllPending()
        try? await conduit.close()
    }
}
