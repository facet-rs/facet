import Foundation
import Network
import os

/// A thread-safe flag to track continuation state
private final class ContinuationState: @unchecked Sendable {
    private let lock = OSAllocatedUnfairLock(initialState: false)

    var hasResumed: Bool {
        lock.withLock { $0 }
    }

    /// Returns true if this is the first call (successfully claimed)
    func claim() -> Bool {
        lock.withLock { resumed in
            if resumed {
                return false
            }
            resumed = true
            return true
        }
    }
}

/// Errors that can occur during TCP connection operations
public enum TCPConnectionError: Error, CustomStringConvertible {
    case connectionFailed(String)
    case connectionClosed
    case sendFailed(String)
    case receiveFailed(String)
    case notConnected
    case timeout

    public var description: String {
        switch self {
        case .connectionFailed(let reason):
            return "Connection failed: \(reason)"
        case .connectionClosed:
            return "Connection closed"
        case .sendFailed(let reason):
            return "Send failed: \(reason)"
        case .receiveFailed(let reason):
            return "Receive failed: \(reason)"
        case .notConnected:
            return "Not connected"
        case .timeout:
            return "Operation timed out"
        }
    }
}

/// An actor-based async TCP client using Network.framework
public actor TCPConnection {
    private var connection: NWConnection?
    private var isConnected: Bool = false

    public init() {}

    /// Connect to a TCP server at the specified host and port
    public func connect(host: String, port: UInt16) async throws {
        let nwHost = NWEndpoint.Host(host)
        let nwPort = NWEndpoint.Port(rawValue: port)!

        let parameters = NWParameters.tcp
        parameters.allowLocalEndpointReuse = true

        let conn = NWConnection(host: nwHost, port: nwPort, using: parameters)
        self.connection = conn

        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            let continuationState = ContinuationState()

            conn.stateUpdateHandler = { [weak conn] connectionState in
                switch connectionState {
                case .ready:
                    guard continuationState.claim() else { return }
                    continuation.resume()

                case .failed(let error):
                    guard continuationState.claim() else { return }
                    conn?.cancel()
                    continuation.resume(throwing: TCPConnectionError.connectionFailed(error.localizedDescription))

                case .cancelled:
                    guard continuationState.claim() else { return }
                    continuation.resume(throwing: TCPConnectionError.connectionFailed("Connection cancelled"))

                case .waiting(let error):
                    // Connection is waiting, could retry or fail
                    guard continuationState.claim() else { return }
                    conn?.cancel()
                    continuation.resume(throwing: TCPConnectionError.connectionFailed("Waiting: \(error.localizedDescription)"))

                default:
                    // .setup, .preparing - keep waiting
                    break
                }
            }

            conn.start(queue: .global())
        }

        self.isConnected = true
    }

    /// Send data over the connection
    public func send(_ data: Data) async throws {
        guard let conn = connection, isConnected else {
            throw TCPConnectionError.notConnected
        }

        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            conn.send(content: data, completion: .contentProcessed { error in
                if let error = error {
                    continuation.resume(throwing: TCPConnectionError.sendFailed(error.localizedDescription))
                } else {
                    continuation.resume()
                }
            })
        }
    }

    /// Receive exactly the specified number of bytes
    /// This will block until all bytes are received or an error occurs
    public func receive(exactly count: Int) async throws -> Data {
        guard let conn = connection, isConnected else {
            throw TCPConnectionError.notConnected
        }

        return try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Data, Error>) in
            conn.receive(minimumIncompleteLength: count, maximumLength: count) { content, _, isComplete, error in
                if let error = error {
                    continuation.resume(throwing: TCPConnectionError.receiveFailed(error.localizedDescription))
                    return
                }

                if let data = content, data.count == count {
                    continuation.resume(returning: data)
                } else if isComplete {
                    continuation.resume(throwing: TCPConnectionError.connectionClosed)
                } else if let data = content {
                    // Got some data but not enough - this shouldn't happen with minimumIncompleteLength
                    // but handle it anyway
                    continuation.resume(throwing: TCPConnectionError.receiveFailed("Received \(data.count) bytes, expected \(count)"))
                } else {
                    continuation.resume(throwing: TCPConnectionError.receiveFailed("No data received"))
                }
            }
        }
    }

    /// Receive up to the specified maximum number of bytes
    /// Returns immediately when any data is available
    public func receive(upTo maxCount: Int) async throws -> Data {
        guard let conn = connection, isConnected else {
            throw TCPConnectionError.notConnected
        }

        return try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Data, Error>) in
            conn.receive(minimumIncompleteLength: 1, maximumLength: maxCount) { content, _, isComplete, error in
                if let error = error {
                    continuation.resume(throwing: TCPConnectionError.receiveFailed(error.localizedDescription))
                    return
                }

                if let data = content, !data.isEmpty {
                    continuation.resume(returning: data)
                } else if isComplete {
                    continuation.resume(throwing: TCPConnectionError.connectionClosed)
                } else {
                    continuation.resume(throwing: TCPConnectionError.receiveFailed("No data received"))
                }
            }
        }
    }

    /// Close the connection
    public func close() {
        isConnected = false
        connection?.cancel()
        connection = nil
    }

    deinit {
        connection?.cancel()
    }
}
