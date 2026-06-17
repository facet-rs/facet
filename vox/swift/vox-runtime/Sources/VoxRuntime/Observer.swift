import Foundation

public enum VoxDriverObserverEvent: Equatable, Sendable {
    case runStarted
    case readerReceivedMessage
    case readerClosed
    case readerFailed(String)
    case conduitBroke
    case runFailed(String)
    case runExited
}

public enum VoxChannelDirection: String, Equatable, Sendable {
    case incoming
    case outgoing
}

public enum VoxChannelObserverKind: String, Equatable, Sendable {
    case open
    case send
    case trySend
    case credit
    case receive
    case consume
    case close
    case reset
}

public enum VoxChannelTrySendDetail: String, Equatable, Sendable {
    case sent
    case unbound
    case creditExhausted = "credit-exhausted"
    case runtimeQueueFull = "runtime-queue-full"
    case closed
}

public struct VoxChannelDebugContext: Equatable, Sendable {
    public let laneId: UInt64?
    public let requestId: UInt64?
    public let methodId: UInt64?
    public let service: String?
    public let method: String?
    public let channelDirection: String?
    public let side: String?

    public init(
        laneId: UInt64? = nil,
        requestId: UInt64? = nil,
        methodId: UInt64? = nil,
        service: String? = nil,
        method: String? = nil,
        channelDirection: String? = nil,
        side: String? = nil
    ) {
        self.laneId = laneId
        self.requestId = requestId
        self.methodId = methodId
        self.service = service
        self.method = method
        self.channelDirection = channelDirection
        self.side = side
    }

    func merged(with next: VoxChannelDebugContext) -> VoxChannelDebugContext {
        VoxChannelDebugContext(
            laneId: next.laneId ?? laneId,
            requestId: next.requestId ?? requestId,
            methodId: next.methodId ?? methodId,
            service: next.service ?? service,
            method: next.method ?? method,
            channelDirection: next.channelDirection ?? channelDirection,
            side: next.side ?? side
        )
    }
}

public struct VoxChannelObserverEvent: Equatable, Sendable {
    public let kind: VoxChannelObserverKind
    public let channelId: ChannelId?
    public let direction: VoxChannelDirection?
    public let bytes: Int?
    public let additionalCredit: UInt32?
    public let trySendDetail: VoxChannelTrySendDetail?
    public let context: VoxChannelDebugContext?
    public let error: String?

    public init(
        kind: VoxChannelObserverKind,
        channelId: ChannelId? = nil,
        direction: VoxChannelDirection? = nil,
        bytes: Int? = nil,
        additionalCredit: UInt32? = nil,
        trySendDetail: VoxChannelTrySendDetail? = nil,
        context: VoxChannelDebugContext? = nil,
        error: String? = nil
    ) {
        self.kind = kind
        self.channelId = channelId
        self.direction = direction
        self.bytes = bytes
        self.additionalCredit = additionalCredit
        self.trySendDetail = trySendDetail
        self.context = context
        self.error = error
    }
}

public enum VoxEstablishmentRole: String, Equatable, Sendable {
    case initiator
    case acceptor
}

public enum VoxEstablishmentPhase: String, Equatable, Sendable {
    case transportPrologue = "transport-prologue"
    case connectionHandshake = "connection-handshake"
    case identityResolution = "identity-resolution"
    case connectionPolicy = "connection-policy"
    case schemaDecodePlan = "schema-decode-plan"
    case serviceLaneOpen = "service-lane-open"
    case laneAuthorization = "lane-authorization"
    case laneGrant = "lane-grant"
    case laneGrantRevocation = "lane-grant-revocation"
}

public enum VoxEstablishmentOutcome: String, Equatable, Sendable {
    case ok
    case rejected
    case error
}

public struct VoxEstablishmentContext: Equatable, Sendable {
    public let role: VoxEstablishmentRole
    public let phase: VoxEstablishmentPhase
    public let laneId: UInt64?

    public init(
        role: VoxEstablishmentRole,
        phase: VoxEstablishmentPhase,
        laneId: UInt64? = nil
    ) {
        self.role = role
        self.phase = phase
        self.laneId = laneId
    }
}

public enum VoxEstablishmentObserverEvent: Equatable, Sendable {
    case started(VoxEstablishmentContext)
    case finished(
        context: VoxEstablishmentContext,
        outcome: VoxEstablishmentOutcome,
        elapsedMs: UInt64,
        error: String?
    )
}

// r[impl rpc.observability.runtime]
public protocol VoxRuntimeObserver: Sendable {
    func driverEvent(_ event: VoxDriverObserverEvent)
    func channelEvent(_ event: VoxChannelObserverEvent)
    func establishmentEvent(_ event: VoxEstablishmentObserverEvent)
}

public extension VoxRuntimeObserver {
    func channelEvent(_: VoxChannelObserverEvent) {}
    func establishmentEvent(_: VoxEstablishmentObserverEvent) {}
}

private final class VoxRuntimeObserverStorage: @unchecked Sendable {
    private let lock = NSLock()
    private var observer: (any VoxRuntimeObserver)?

    func set(_ observer: (any VoxRuntimeObserver)?) {
        lock.lock()
        self.observer = observer
        lock.unlock()
    }

    func get() -> (any VoxRuntimeObserver)? {
        lock.lock()
        defer { lock.unlock() }
        return observer
    }

    func driverEvent(_ event: VoxDriverObserverEvent) {
        let observer = get()
        observer?.driverEvent(event)
    }

    func channelEvent(_ event: VoxChannelObserverEvent) {
        let observer = get()
        observer?.channelEvent(event)
    }

    func establishmentEvent(_ event: VoxEstablishmentObserverEvent) {
        let observer = get()
        observer?.establishmentEvent(event)
    }
}

private let voxRuntimeObserverStorage = VoxRuntimeObserverStorage()

// r[impl rpc.observability.runtime]
public func setVoxRuntimeObserver(_ observer: (any VoxRuntimeObserver)?) {
    voxRuntimeObserverStorage.set(observer)
}

// r[impl rpc.observability.runtime]
public func voxRuntimeObserver() -> (any VoxRuntimeObserver)? {
    voxRuntimeObserverStorage.get()
}

// r[impl rpc.observability.driver]
func observeDriver(_ event: VoxDriverObserverEvent) {
    voxRuntimeObserverStorage.driverEvent(event)
}

// r[impl rpc.observability.channel]
// r[impl rpc.observability.channel.try-send-detail]
func observeChannel(_ event: VoxChannelObserverEvent) {
    voxRuntimeObserverStorage.channelEvent(event)
}

// r[impl rpc.observability.establishment]
func observeEstablishmentStarted(_ context: VoxEstablishmentContext) -> UInt64 {
    let startedAt = DispatchTime.now().uptimeNanoseconds
    voxRuntimeObserverStorage.establishmentEvent(.started(context))
    return startedAt
}

// r[impl rpc.observability.establishment]
func observeEstablishmentFinished(
    _ context: VoxEstablishmentContext,
    startedAt: UInt64,
    outcome: VoxEstablishmentOutcome,
    error: Error? = nil
) {
    let now = DispatchTime.now().uptimeNanoseconds
    let elapsedMs = now >= startedAt ? (now - startedAt) / 1_000_000 : 0
    voxRuntimeObserverStorage.establishmentEvent(
        .finished(
            context: context,
            outcome: outcome,
            elapsedMs: elapsedMs,
            error: error.map { String(describing: $0) }
        ))
}

func voxEstablishmentRole(_ role: Role) -> VoxEstablishmentRole {
    switch role {
    case .initiator:
        return .initiator
    case .acceptor:
        return .acceptor
    }
}

func withObservedEstablishment<T>(
    _ context: VoxEstablishmentContext,
    _ operation: () async throws -> T
) async throws -> T {
    let startedAt = observeEstablishmentStarted(context)
    do {
        let result = try await operation()
        observeEstablishmentFinished(context, startedAt: startedAt, outcome: .ok)
        return result
    } catch {
        let outcome: VoxEstablishmentOutcome =
            error is ConnectionDeclinedError ? .rejected : .error
        observeEstablishmentFinished(
            context,
            startedAt: startedAt,
            outcome: outcome,
            error: error
        )
        throw error
    }
}

// r[impl rpc.observability.low-cardinality]
public func voxObserverMetricLabels(_ input: [String: String]) -> [String: String] {
    let allowed = Set([
        "service",
        "method",
        "side",
        "outcome",
        "error_kind",
        "channel_direction",
        "rejection_reason",
        "identity_form",
    ])
    return input.filter { key, value in
        allowed.contains(key) && !value.isEmpty
    }
}
