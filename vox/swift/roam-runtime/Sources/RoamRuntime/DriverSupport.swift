import Foundation

struct InFlightResponseContext: Sendable {
    let connectionId: UInt64
    let responseMetadata: [MetadataEntryV7]
}

enum DriverEvent: Sendable {
    case incomingMessage(MessageV7)
    case wake
    case retryTick
    case conduitClosed
    case conduitFailed(String)
}

final class LockedQueue<T>: @unchecked Sendable {
    private let lock = NSLock()
    private var items: [T] = []
    private var closed = false

    func push(_ item: T) -> Bool {
        lock.lock()
        defer { lock.unlock() }
        if closed {
            return false
        }
        items.append(item)
        return true
    }

    func popAll() -> [T] {
        lock.lock()
        defer { lock.unlock() }
        if items.isEmpty {
            return []
        }
        let out = items
        items = []
        return out
    }

    func close() {
        lock.lock()
        defer { lock.unlock() }
        closed = true
        items.removeAll()
    }
}

func makeDriverAndConnection(
    conduit: any Conduit,
    dispatcher: any ServiceDispatcher,
    role: Role,
    negotiated: Negotiated,
    peerSupportsRetry: Bool,
    acceptConnections: Bool,
    keepalive: DriverKeepaliveConfig? = nil
) -> (Connection, Driver) {
    let commandQueue = LockedQueue<HandleCommand>()
    let taskQueue = LockedQueue<TaskMessage>()
    var continuation: AsyncStream<DriverEvent>.Continuation!
    let eventStream = AsyncStream<DriverEvent> { cont in
        continuation = cont
    }
    let capturedContinuation = continuation!

    let commandSender: @Sendable (HandleCommand) -> Bool = { cmd in
        guard commandQueue.push(cmd) else {
            return false
        }
        let result = capturedContinuation.yield(.wake)
        guard case .terminated = result else {
            return true
        }
        return false
    }
    let taskSender: @Sendable (TaskMessage) -> Bool = { msg in
        guard taskQueue.push(msg) else {
            return false
        }
        let result = capturedContinuation.yield(.wake)
        guard case .terminated = result else {
            return true
        }
        return false
    }

    let handle = ConnectionHandle(
        commandTx: commandSender,
        taskTx: taskSender,
        role: role,
        peerSupportsRetry: peerSupportsRetry,
        maxConcurrentRequests: negotiated.maxConcurrentRequests
    )

    let driver = Driver(
        conduit: conduit,
        dispatcher: dispatcher,
        role: role,
        negotiated: negotiated,
        handle: handle,
        operations: OperationRegistry(),
        acceptConnections: acceptConnections,
        keepalive: keepalive,
        eventStream: eventStream,
        eventContinuation: continuation,
        commandQueue: commandQueue,
        taskQueue: taskQueue
    )

    return (Connection(handle: handle), driver)
}

func makeSessionDriverAndConnection(
    conduit: any Conduit,
    dispatcher: any ServiceDispatcher,
    role: Role,
    negotiated: Negotiated,
    peerSupportsRetry: Bool,
    acceptConnections: Bool,
    keepalive: DriverKeepaliveConfig? = nil,
    resumable: Bool,
    sessionResumeKey: [UInt8]?,
    localRootSettings: ConnectionSettingsV7,
    peerRootSettings: ConnectionSettingsV7,
    transport: TransportConduitKind,
    recoverAttachment: (@Sendable () async throws -> LinkAttachment)? = nil
) -> (Connection, Driver, SessionHandle) {
    let coordinator = SessionResumeCoordinator(
        role: role,
        localRootSettings: localRootSettings,
        peerRootSettings: peerRootSettings,
        transport: transport,
        resumable: resumable,
        sessionResumeKey: sessionResumeKey,
        recoverAttachment: recoverAttachment
    )

    let runtimeConduit: any Conduit
    if resumable {
        runtimeConduit = ResumableConduit(conduit: conduit, coordinator: coordinator)
    } else {
        runtimeConduit = conduit
    }

    let (connection, driver) = makeDriverAndConnection(
        conduit: runtimeConduit,
        dispatcher: dispatcher,
        role: role,
        negotiated: negotiated,
        peerSupportsRetry: peerSupportsRetry,
        acceptConnections: acceptConnections,
        keepalive: keepalive
    )
    return (connection, driver, SessionHandle(coordinator: coordinator))
}
