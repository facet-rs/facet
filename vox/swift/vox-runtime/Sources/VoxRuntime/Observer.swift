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

// r[impl rpc.observability.runtime]
public protocol VoxRuntimeObserver: Sendable {
    func driverEvent(_ event: VoxDriverObserverEvent)
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
