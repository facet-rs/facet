import Foundation
import PhonSchema

// r[impl lane]
// r[impl lane.control]
// r[impl rpc.caller]
public final class Lane: @unchecked Sendable {
    let handle: LaneHandle
    /// Writer schema closures the peer advertised — the generated client uses them for
    /// response decode against the server's advertised response schema through this.
    public let schemaReceiveTracker: SchemaTracker

    init(handle: LaneHandle, schemaReceiveTracker: SchemaTracker) {
        self.handle = handle
        self.schemaReceiveTracker = schemaReceiveTracker
    }

    deinit {
        // r[impl rpc.caller.liveness.refcounted]
        // r[impl rpc.caller.liveness.last-drop-closes-connection]
        // r[impl rpc.caller.liveness.public-handle-drop]
        // r[impl rpc.caller.liveness.explicit-shutdown-required]
    }

    public var channelAllocator: ChannelIdAllocator {
        handle.channelAllocator
    }

    public var laneId: UInt64 {
        handle.laneId
    }

    var connectionId: UInt64 {
        handle.laneId
    }

    public var incomingChannelRegistry: ChannelRegistry {
        handle.channelRegistry
    }

    public var taskSender: TaskSender {
        { [weak self] msg in
            self?.sendTaskMessage(msg)
        }
    }

    public func call(
        methodId: UInt64,
        metadata: Metadata,
        payload: [UInt8],
        channels: [UInt64] = [],
        timeout: TimeInterval?,
        finalizeChannels: (@Sendable () -> Void)? = nil,
        schemaInfo: ClientSchemaInfo? = nil
    ) async throws -> [UInt8] {
        // r[impl rpc.caller]
        try await callRaw(
            methodId: methodId,
            metadata: metadata,
            payload: payload,
            channels: channels,
            timeout: timeout,
            finalizeChannels: finalizeChannels,
            schemaInfo: schemaInfo
        )
    }

    public func callRaw(
        methodId: UInt64,
        metadata: Metadata = .null,
        payload: [UInt8],
        channels: [UInt64] = [],
        timeout: TimeInterval? = nil,
        finalizeChannels: (@Sendable () -> Void)? = nil,
        schemaInfo: ClientSchemaInfo? = nil
    ) async throws -> [UInt8] {
        // r[impl rpc.request]
        try await handle.callRaw(
            methodId: methodId,
            metadata: metadata,
            payload: payload,
            channels: channels,
            timeout: timeout,
            finalizeChannels: finalizeChannels,
            schemaInfo: schemaInfo
        )
    }

    public func sendTaskMessage(_ msg: TaskMessage) {
        handle.sendTaskMessage(msg)
    }
}

extension Lane: VoxLane {}
