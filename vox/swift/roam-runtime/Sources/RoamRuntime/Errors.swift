import Foundation

/// roam runtime errors
public enum RoamError: Error {
    case decodeError(String)
    case encodeError(String)
    case transportError(String)
    case unknownMethod
    case invalidPayload
    case cancelled
    case userError  // r[impl unary.error.user] - Application returned an error
}

/// Result type for RPC calls: Result<T, RoamError<E>>
/// where E is the user-defined error type
public enum CallError<UserError> {
    case user(UserError)
    case unknownMethod
    case invalidPayload
    case cancelled
}

extension CallError {
    /// r[impl unary.error.unknown-method]
    public static var unknownMethodError: CallError<UserError> {
        .unknownMethod
    }

    /// r[impl unary.error.invalid-payload]
    public static var invalidPayloadError: CallError<UserError> {
        .invalidPayload
    }
}
