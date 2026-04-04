import Foundation

// MARK: - ConnectionRequest

/// Metadata wrapper for an incoming virtual connection request.
/// Mirrors Rust's `ConnectionRequest`.
public struct ConnectionRequest: Sendable {
    /// All metadata entries from the connection open message.
    public let metadata: [MetadataEntry]
    /// The service name, extracted from the "vox-service" metadata key.
    public let service: String
}

// MARK: - PendingConnection

/// An inbound virtual connection awaiting a handler decision.
///
/// Call `handleWith(_:)` to accept the connection with a specific service dispatcher.
/// If the `PendingConnection` is dropped (deallocated) without calling `handleWith(_:)`,
/// the connection is automatically rejected.
///
/// Mirrors Rust's `PendingConnection`.
public final class PendingConnection: @unchecked Sendable {
    private let lock = NSLock()
    private var handled = false
    private let acceptFn: @Sendable (any ServiceDispatcher) -> Void
    private let rejectFn: @Sendable () -> Void

    init(
        accept: @escaping @Sendable (any ServiceDispatcher) -> Void,
        reject: @escaping @Sendable () -> Void
    ) {
        self.acceptFn = accept
        self.rejectFn = reject
    }

    /// Accept the connection and route it to the given dispatcher.
    /// Safe to call multiple times; only the first call takes effect.
    public func handleWith(_ dispatcher: any ServiceDispatcher) {
        lock.lock()
        let wasHandled = handled
        handled = true
        lock.unlock()
        if !wasHandled {
            acceptFn(dispatcher)
        }
    }

    deinit {
        lock.lock()
        let wasHandled = handled
        lock.unlock()
        if !wasHandled {
            rejectFn()
        }
    }
}

// MARK: - ConnectionAcceptor

/// Routes incoming virtual connections to appropriate service dispatchers.
///
/// Implement this protocol to decide, per connection, which `ServiceDispatcher`
/// should handle it — or to reject it by dropping the `PendingConnection`.
///
/// Mirrors Rust's `ConnectionAcceptor` trait.
public protocol ConnectionAcceptor: Sendable {
    /// Called for each incoming virtual connection.
    ///
    /// - Parameters:
    ///   - request: Metadata about the incoming connection, including the service name.
    ///   - connection: The pending connection. Call `handleWith(_:)` to accept, or let it
    ///     drop to reject.
    func accept(request: ConnectionRequest, connection: PendingConnection)
}

// MARK: - DefaultConnectionAcceptor

/// A `ConnectionAcceptor` that routes all incoming connections to the same dispatcher.
///
/// This is the common case for subjects that handle a single service: every virtual
/// connection that arrives is accepted and dispatched to the same `ServiceDispatcher`
/// regardless of service name.
public struct DefaultConnectionAcceptor: ConnectionAcceptor {
    public let dispatcher: any ServiceDispatcher

    public init(dispatcher: any ServiceDispatcher) {
        self.dispatcher = dispatcher
    }

    public func accept(request _: ConnectionRequest, connection: PendingConnection) {
        connection.handleWith(dispatcher)
    }
}
