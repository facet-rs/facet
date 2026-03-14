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
        maxConcurrentRequests: negotiated.maxConcurrentRequests
    )

    let driver = Driver(
        conduit: conduit,
        dispatcher: dispatcher,
        role: role,
        negotiated: negotiated,
        handle: handle,
        acceptConnections: acceptConnections,
        keepalive: keepalive,
        eventStream: eventStream,
        eventContinuation: continuation,
        commandQueue: commandQueue,
        taskQueue: taskQueue
    )

    return (Connection(handle: handle), driver)
}
