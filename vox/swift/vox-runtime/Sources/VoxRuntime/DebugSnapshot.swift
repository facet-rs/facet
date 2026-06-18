import Foundation

public enum VoxChannelSnapshotState: String, Equatable, Sendable {
    case known
    case incoming
    case outgoing
    case bidirectional
    case closed
    case reset
}

public struct VoxChannelSnapshot: Equatable, Sendable {
    public let channelId: ChannelId
    public let state: VoxChannelSnapshotState
    public let context: VoxChannelDebugContext?
    public let bufferedItemCount: Int
    public let outgoingCreditAvailable: UInt32?
    public let outgoingCreditWaiterCount: Int?

    public init(
        channelId: ChannelId,
        state: VoxChannelSnapshotState,
        context: VoxChannelDebugContext? = nil,
        bufferedItemCount: Int = 0,
        outgoingCreditAvailable: UInt32? = nil,
        outgoingCreditWaiterCount: Int? = nil
    ) {
        self.channelId = channelId
        self.state = state
        self.context = context
        self.bufferedItemCount = bufferedItemCount
        self.outgoingCreditAvailable = outgoingCreditAvailable
        self.outgoingCreditWaiterCount = outgoingCreditWaiterCount
    }
}

public struct VoxChannelRegistrySnapshot: Equatable, Sendable {
    public let channels: [VoxChannelSnapshot]

    public init(channels: [VoxChannelSnapshot]) {
        self.channels = channels
    }
}

public enum VoxRequestScopeDebugState: String, Equatable, Sendable {
    case waitingForResponse = "waiting-for-response"
    case handlerRunning = "handler-running"
    case succeeded
    case failed
    case cancelled
    case laneClosed = "lane-closed"
    case connectionLost = "connection-lost"
    case timedOut = "timed-out"
}

public struct VoxRequestScopeDebugSnapshot: Equatable, Sendable {
    public let requestId: UInt64
    public let laneId: UInt64?
    public let state: VoxRequestScopeDebugState
    public let channelIds: [UInt64]

    public init(
        requestId: UInt64,
        laneId: UInt64? = nil,
        state: VoxRequestScopeDebugState,
        channelIds: [UInt64] = []
    ) {
        self.requestId = requestId
        self.laneId = laneId
        self.state = state
        self.channelIds = channelIds
    }
}

public struct VoxDriverStateDebugSnapshot: Equatable, Sendable {
    public let isClosed: Bool
    public let requestScopes: [VoxRequestScopeDebugSnapshot]

    public init(
        isClosed: Bool,
        requestScopes: [VoxRequestScopeDebugSnapshot]
    ) {
        self.isClosed = isClosed
        self.requestScopes = requestScopes
    }
}

public struct VoxLaneDebugSnapshot: Equatable, Sendable {
    public let laneId: UInt64
    public let isPendingOpen: Bool
    public let channels: VoxChannelRegistrySnapshot?

    public init(
        laneId: UInt64,
        isPendingOpen: Bool,
        channels: VoxChannelRegistrySnapshot? = nil
    ) {
        self.laneId = laneId
        self.isPendingOpen = isPendingOpen
        self.channels = channels
    }
}

public struct VoxConnectionDebugSnapshot: Equatable, Sendable {
    public let role: Role
    public let driverState: VoxDriverStateDebugSnapshot
    public let controlLaneChannels: VoxChannelRegistrySnapshot
    public let serverChannels: VoxChannelRegistrySnapshot
    public let lanes: [VoxLaneDebugSnapshot]
    public let pendingCallCount: Int
    public let pendingTaskMessageCount: Int

    public init(
        role: Role,
        driverState: VoxDriverStateDebugSnapshot,
        controlLaneChannels: VoxChannelRegistrySnapshot,
        serverChannels: VoxChannelRegistrySnapshot,
        lanes: [VoxLaneDebugSnapshot],
        pendingCallCount: Int,
        pendingTaskMessageCount: Int
    ) {
        self.role = role
        self.driverState = driverState
        self.controlLaneChannels = controlLaneChannels
        self.serverChannels = serverChannels
        self.lanes = lanes
        self.pendingCallCount = pendingCallCount
        self.pendingTaskMessageCount = pendingTaskMessageCount
    }
}
