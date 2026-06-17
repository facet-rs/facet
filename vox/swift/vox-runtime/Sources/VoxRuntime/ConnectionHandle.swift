import Foundation

public final class ConnectionHandle: @unchecked Sendable {
    private let commandTx: @Sendable (HandleCommand) -> Bool
    private let eventContinuation: AsyncStream<DriverEvent>.Continuation

    init(
        commandTx: @escaping @Sendable (HandleCommand) -> Bool,
        eventContinuation: AsyncStream<DriverEvent>.Continuation
    ) {
        self.commandTx = commandTx
        self.eventContinuation = eventContinuation
    }

    /// Open a service lane on the existing connection.
    ///
    /// r[impl rpc.virtual-connection.open]
    /// r[impl connection.open]
    /// r[impl lane.open]
    public func openLane(
        settings: ConnectionSettings,
        metadata: Metadata = emptyMetadata(),
        dispatcher: (any ServiceDispatcher)? = nil
    ) async throws -> Lane {
        try await withCheckedThrowingContinuation { continuation in
            let responseTx = SingleResume<Lane> { result in
                continuation.resume(with: result)
            }
            let accepted = commandTx(
                .openLane(
                    settings: settings,
                    metadata: metadata,
                    dispatcher: dispatcher,
                    responseTx: { result in responseTx(result) }
                ))
            guard accepted else {
                responseTx(.failure(.connectionClosed))
                return
            }
        }
    }

    /// Close an open service lane.
    ///
    /// r[impl connection]
    /// r[impl connection.virtual]
    /// r[impl connection.close]
    /// r[impl connection.close.semantics]
    public func closeLane(
        _ laneId: UInt64,
        metadata: Metadata = emptyMetadata()
    ) async throws {
        try await withCheckedThrowingContinuation { continuation in
            let responseTx = SingleResume<Void> { result in
                continuation.resume(with: result)
            }
            let accepted = commandTx(
                .closeLane(
                    laneId: laneId,
                    metadata: metadata,
                    responseTx: { result in responseTx(result) }
                ))
            guard accepted else {
                responseTx(.failure(.connectionClosed))
                return
            }
        }
    }

    /// Request shutdown of the driven connection.
    /// r[impl connection.shutdown.explicit]
    public func shutdown() {
        eventContinuation.finish()
    }
}
