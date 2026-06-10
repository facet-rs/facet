import Foundation

public final class SessionHandle: @unchecked Sendable {
    private let commandTx: @Sendable (HandleCommand) -> Bool
    private let eventContinuation: AsyncStream<DriverEvent>.Continuation

    init(
        commandTx: @escaping @Sendable (HandleCommand) -> Bool,
        eventContinuation: AsyncStream<DriverEvent>.Continuation
    ) {
        self.commandTx = commandTx
        self.eventContinuation = eventContinuation
    }

    /// Open a virtual connection on the existing session.
    ///
    /// r[impl rpc.virtual-connection.open]
    /// r[impl connection.open]
    public func openConnection(
        settings: ConnectionSettings,
        metadata: Metadata = emptyMetadata(),
        dispatcher: (any ServiceDispatcher)? = nil
    ) async throws -> Connection {
        try await withCheckedThrowingContinuation { continuation in
            let responseTx = SingleResume<Connection> { result in
                continuation.resume(with: result)
            }
            let accepted = commandTx(
                .openConnection(
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

    /// Shutdown the session.
    public func shutdown() {
        eventContinuation.finish()
    }
}
