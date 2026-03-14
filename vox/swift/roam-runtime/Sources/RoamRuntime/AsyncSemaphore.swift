actor AsyncSemaphore {
    private var permits: Int
    private var waiters: [CheckedContinuation<Void, Error>] = []
    private var closed = false

    init(permits: Int) {
        self.permits = permits
    }

    func acquire() async throws {
        if closed { throw ConnectionError.connectionClosed }
        if permits > 0 {
            permits -= 1
            return
        }
        try await withCheckedThrowingContinuation { (cont: CheckedContinuation<Void, Error>) in
            if closed {
                cont.resume(throwing: ConnectionError.connectionClosed)
            } else {
                waiters.append(cont)
            }
        }
    }

    func release() {
        if !waiters.isEmpty {
            waiters.removeFirst().resume()
        } else {
            permits += 1
        }
    }

    func close() {
        closed = true
        let pending = waiters
        waiters.removeAll()
        for waiter in pending {
            waiter.resume(throwing: ConnectionError.connectionClosed)
        }
    }
}
