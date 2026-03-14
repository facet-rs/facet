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

    /// Run the driver until connection closes.
    public func run() async throws {
        var keepaliveRuntime = makeKeepaliveRuntime()
        var seenResumeGeneration: UInt64 = 0

        let cont = eventContinuation
        let conduit = self.conduit
        let readerTask = Task {
            do {
                while true {
                    if let msg = try await conduit.recv() {
                        cont.yield(.incomingMessage(msg))
                    } else {
                        cont.yield(.conduitClosed)
                        break
                    }
                }
            } catch {
                cont.yield(.conduitFailed(String(describing: error)))
            }
        }

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
        do {
            for await event in eventStream {
                try await drainInjectedQueues()
                try await flushPendingTaskMessages()
                try await flushPendingCalls()
                switch event {
                case .incomingMessage(let msg):
                    try await handleMessage(msg, keepaliveRuntime: &keepaliveRuntime)

                case .wake:
                    break

                case .retryTick:
                    if let resumable = conduit as? ResumableConduit {
                        let generation = await resumable.currentResumeGeneration()
                        if generation != seenResumeGeneration {
                            seenResumeGeneration = generation
                            await replayPendingCallsAfterResume()
                        }
                    }
                    try await handleKeepaliveTick(keepaliveRuntime: &keepaliveRuntime)

                case .conduitClosed:
                    warnLog("conduit reader closed (recv returned nil)")
                    await failAllPending()
                    eventContinuation.finish()

                case .conduitFailed(let reason):
                    warnLog("conduit reader failed: \(reason)")
                    await failAllPending()
                    eventContinuation.finish()
                }
            }
        } catch {
            eventContinuation.finish()
            await failAllPending()
            try? await conduit.close()
            throw error
        }
        await failAllPending()
        try? await conduit.close()
    }
}
