/// Protocol for dispatching incoming requests.
public protocol ServiceDispatcher: Sendable {
    func retryPolicy(methodId: UInt64) -> RetryPolicy

    /// Pre-register any channels in the request payload.
    /// This is called synchronously BEFORE spawning the handler task,
    /// ensuring channels are registered before any Data messages arrive.
    func preregister(
        methodId: UInt64,
        payload: [UInt8],
        registry: ChannelRegistry
    ) async

    /// Dispatch a request. Called in a spawned task after preregister.
    func dispatch(
        methodId: UInt64,
        payload: [UInt8],
        requestId: UInt64,
        registry: ChannelRegistry,
        taskTx: @escaping @Sendable (TaskMessage) -> Void
    ) async
}

public extension ServiceDispatcher {
    func retryPolicy(methodId _: UInt64) -> RetryPolicy {
        .volatile
    }
}
