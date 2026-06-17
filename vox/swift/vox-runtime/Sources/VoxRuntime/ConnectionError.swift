/// r[impl lane.id.compat] - Connection-level errors terminate the connection.
/// r[impl connection.protocol-error] - Protocol errors are connection-fatal.
public enum ConnectionError: Error {
    case connectionClosed
    case timeout
    case transportError(String)
    case rejected(LaneRejection)
    case goodbye(reason: String)
    case protocolViolation(rule: String)
    case handshakeFailed(String)
}
