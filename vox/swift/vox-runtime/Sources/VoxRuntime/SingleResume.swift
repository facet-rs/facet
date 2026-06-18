import Foundation

final class SingleResume<ResultValue: Sendable>: @unchecked Sendable {
    private let lock = NSLock()
    private var finished = false
    private let body: @Sendable (Result<ResultValue, ConnectionError>) -> Void

    init(body: @escaping @Sendable (Result<ResultValue, ConnectionError>) -> Void) {
        self.body = body
    }

    func callAsFunction(_ result: Result<ResultValue, ConnectionError>) {
        let shouldRun = lock.withLock {
            if finished {
                return false
            }
            finished = true
            return true
        }
        guard shouldRun else {
            return
        }
        body(result)
    }
}
