/// r[impl connection] - Connection-level errors terminate the connection.
/// r[impl session.protocol-error] - Protocol errors are connection-fatal.
public enum ConnectionError: Error {
    case connectionClosed
    case timeout
    case transportError(String)
    case goodbye(reason: String)
    case protocolViolation(rule: String)
    case handshakeFailed(String)
}
