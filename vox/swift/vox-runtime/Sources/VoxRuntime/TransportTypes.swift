public enum TransportError: Error {
    case connectionClosed
    case wouldBlock
    case frameEncoding(String)
    case frameDecoding(String)
    case transportIO(String)
    case protocolViolation(String)
}
