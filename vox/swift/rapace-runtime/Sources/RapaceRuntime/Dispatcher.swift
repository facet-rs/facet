import Foundation

// MARK: - Unary Dispatcher

/// Type alias for method handler functions.
/// Takes a service implementation and request payload, returns response payload.
public typealias MethodHandler<Service> = (Service, Data) async throws -> Data

/// Generic unary dispatcher for Roam RPC services.
///
/// This is the core runtime component - codegen only provides:
/// - The method handler map (methodId -> handler function)
/// - Service-specific encoding/decoding logic within each handler
///
/// Similar to TypeScript's `UnaryDispatcher<H>` and Rust's dispatch functions.
public class UnaryDispatcher<Service> {
    private let methodHandlers: [UInt64: MethodHandler<Service>]

    /// Initialize a dispatcher with a map of method ID -> handler
    public init(methodHandlers: [UInt64: MethodHandler<Service>]) {
        self.methodHandlers = methodHandlers
    }

    /// Dispatch a unary request to the appropriate handler.
    ///
    /// - Parameters:
    ///   - service: The service implementation
    ///   - methodId: The method ID from the request
    ///   - payload: The request payload bytes
    /// - Returns: Response payload bytes (postcard-encoded Result<T, RoamError<E>>)
    ///
    /// r[impl unary.dispatch]
    public func dispatch(
        service: Service,
        methodId: UInt64,
        payload: Data
    ) async -> Data {
        guard let handler = methodHandlers[methodId] else {
            // r[impl unary.error.unknown-method]
            return Data(encodeUnknownMethodError())
        }

        do {
            return try await handler(service, payload)
        } catch {
            // r[impl unary.error.invalid-payload]
            // If the handler throws, treat it as invalid payload
            return Data(encodeInvalidPayloadError())
        }
    }
}

// MARK: - Unary Caller Protocol

/// Protocol for making unary RPC calls.
///
/// This is the client-side equivalent of the dispatcher.
/// Implementations provide the transport layer (TCP, shared memory, etc.),
/// and generated client code uses this protocol to make calls.
///
/// Similar to Rust's `UnaryCaller` trait.
public protocol UnaryCaller {
    associatedtype TransportError: Error

    /// Make a unary RPC call.
    ///
    /// - Parameters:
    ///   - methodId: The method ID
    ///   - payload: Request payload bytes
    /// - Returns: Response payload bytes
    /// - Throws: Transport-level errors
    func callUnary(methodId: UInt64, payload: Data) async throws -> Data
}
