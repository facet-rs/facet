public enum ConduitKind: Sendable {
    case bare
    case stable
}

@available(*, deprecated, renamed: "ConduitKind")
public typealias TransportConduitKind = ConduitKind

public enum TransportError: Error {
    case connectionClosed
    case wouldBlock
    case frameEncoding(String)
    case frameDecoding(String)
    case transportIO(String)
    case protocolViolation(String)
}
