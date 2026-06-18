import Foundation

@testable import VoxRuntime

private actor VoxRuntimeObserverTestGate {
    private var locked = false
    private var waiters: [CheckedContinuation<Void, Never>] = []

    func acquire() async {
        if !locked {
            locked = true
            return
        }
        await withCheckedContinuation { continuation in
            waiters.append(continuation)
        }
    }

    func release() {
        if waiters.isEmpty {
            locked = false
            return
        }
        let waiter = waiters.removeFirst()
        waiter.resume()
    }
}

private let voxRuntimeObserverTestGate = VoxRuntimeObserverTestGate()

func withVoxRuntimeObserverForTest<T>(
    _ observer: any VoxRuntimeObserver,
    _ body: () async throws -> T
) async throws -> T {
    await voxRuntimeObserverTestGate.acquire()
    setVoxRuntimeObserver(observer)
    do {
        let result = try await body()
        setVoxRuntimeObserver(nil)
        await voxRuntimeObserverTestGate.release()
        return result
    } catch {
        setVoxRuntimeObserver(nil)
        await voxRuntimeObserverTestGate.release()
        throw error
    }
}
