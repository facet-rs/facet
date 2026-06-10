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
        taskTx: @escaping @Sendable (TaskMessage) -> Void
    ) async
}

/// A `ServiceDispatcher` that serves nothing — every call returns `unknownMethod`.
///
/// Use it to anchor a connection that exists only as a session base: the common case is the
/// root connection when the real services live on virtual connections opened via
/// `SessionHandle.openConnection`. Pairs with `NoopClient` on the peer.
public struct NoopDispatcher: ServiceDispatcher {
    public init() {}

    public func encodeVoxError(_: VoxRuntimeError) -> [UInt8] {
        // The Noop service has no methods, so a real error response is never produced.
        []
    }

    public func preregister(
        methodId _: UInt64,
        payload _: [UInt8],
        channels: [UInt64],
        registry: ChannelRegistry
    ) async {
        for id in channels {
            await registry.markKnown(id)
        }
    }

    public func dispatch(
        methodId: UInt64,
        payload _: [UInt8],
        requestId: UInt64,
        channels _: [UInt64],
        registry _: ChannelRegistry,
        schemaSendTracker _: SchemaSendTracker,
        schemaReceiveTracker _: SchemaTracker,
        taskTx: @escaping @Sendable (TaskMessage) -> Void
    ) async {
        taskTx(.response(requestId: requestId, payload: [], methodId: methodId))
    }
}
