/// Protocol for dispatching incoming requests.
/// r[impl rpc.service]
/// r[impl rpc.service.methods]
/// r[impl rpc.handler]
public protocol ServiceDispatcher: Sendable {
    /// Encode a runtime-originated `VoxRuntimeError` (cancelled, indeterminate,
    /// invalid payload, …) as a response payload. The wire type is
    /// `Result<T, VoxError<E>>`, whose `Err` arm is independent of the method's
    /// `T`/`E`, so the generated dispatcher encodes it through any method's response
    /// descriptor (mirrors TS `encodeVoxError`).
    /// r[impl rpc.fallible]
    /// r[impl rpc.fallible.vox-error]
    func encodeVoxError(_ error: VoxRuntimeError) -> [UInt8]

    /// Pre-register the call's out-of-band channels synchronously BEFORE spawning the
    /// handler task, so incoming `Data` on those ids buffers instead of being rejected
    /// as unknown. `channels` is `RequestCall.channels` (the caller-allocated ids).
    func preregister(
        methodId: UInt64,
        payload: [UInt8],
        channels: [UInt64],
        registry: ChannelRegistry
    ) async

    /// Dispatch a request. Called in a spawned task after preregister. `channels` is
    /// `RequestCall.channels`; a channel arg in the decoded payload is a u32 wire index
    /// into this list, which the generated dispatcher binds to a local `Tx`/`Rx`.
    func dispatch(
        methodId: UInt64,
        payload: [UInt8],
        requestId: UInt64,
        channels: [UInt64],
        registry: ChannelRegistry,
        schemaSendTracker: SchemaSendTracker,
        schemaReceiveTracker: SchemaTracker,
        context: RequestContext,
        taskTx: @escaping @Sendable (TaskMessage) -> Void
    ) async
}

/// Internal dispatcher for the reserved control lane while the Swift runtime still
/// bridges onto the current wire messages.
struct ConnectionControlDispatcher: ServiceDispatcher {
    init() {}

    func encodeVoxError(_: VoxRuntimeError) -> [UInt8] {
        // The control lane has no public service methods.
        []
    }

    func preregister(
        methodId _: UInt64,
        payload _: [UInt8],
        channels: [UInt64],
        registry: ChannelRegistry
    ) async {
        for id in channels {
            await registry.markKnown(id)
        }
    }

    func dispatch(
        methodId: UInt64,
        payload _: [UInt8],
        requestId: UInt64,
        channels _: [UInt64],
        registry _: ChannelRegistry,
        schemaSendTracker _: SchemaSendTracker,
        schemaReceiveTracker _: SchemaTracker,
        context _: RequestContext,
        taskTx: @escaping @Sendable (TaskMessage) -> Void
    ) async {
        taskTx(.response(requestId: requestId, payload: [], methodId: methodId))
    }
}
