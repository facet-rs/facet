public protocol SessionConnector: Sendable {
    func openAttachment() async throws -> LinkAttachment
}

// r[impl transport.stream]
// r[impl transport.stream.kinds]
public struct TcpConnector: SessionConnector, LinkSource, Sendable {
    public let host: String
    public let port: Int

    public init(host: String, port: Int) {
        self.host = host
        self.port = port
    }

    public func nextLink() async throws -> LinkAttachment {
        LinkAttachment.initiator(try await connectLink(host: host, port: port))
    }

    public func openAttachment() async throws -> LinkAttachment {
        try await TransportedLinkSource(source: self).nextLink()
    }
}

public struct UnixConnector: SessionConnector, LinkSource, Sendable {
    public let path: String

    public init(path: String) {
        self.path = path
    }

    public func nextLink() async throws -> LinkAttachment {
        LinkAttachment.initiator(try await connectLink(unixPath: path))
    }

    public func openAttachment() async throws -> LinkAttachment {
        try await TransportedLinkSource(source: self).nextLink()
    }
}

public func connect(unixPath: String) async throws -> any Conduit {
    try await connect(
        unixPath: unixPath,
        prologueTimeoutNs: defaultTransportPrologueTimeoutNs
    )
}

// r[impl transport.stream.local]
func connect(
    unixPath: String,
    prologueTimeoutNs: UInt64
) async throws -> any Conduit {
    let connector = UnixConnector(path: unixPath)
    let attachment = try await TimedTransportedLinkSource(
        source: connector,
        timeoutNs: prologueTimeoutNs
    ).nextLink()
    return BareConduit(link: attachment.link, peerMessageSchema: [])  // raw connect(): no handshake yet, decoder inert until establishment
}

public func connect(host: String, port: Int) async throws
    -> any Conduit
{
    try await connect(
        host: host,
        port: port,
        prologueTimeoutNs: defaultTransportPrologueTimeoutNs
    )
}

// r[impl transport.stream]
func connect(
    host: String,
    port: Int,
    prologueTimeoutNs: UInt64
) async throws -> any Conduit {
    let connector = TcpConnector(host: host, port: port)
    let attachment = try await TimedTransportedLinkSource(
        source: connector,
        timeoutNs: prologueTimeoutNs
    ).nextLink()
    return BareConduit(link: attachment.link, peerMessageSchema: [])  // raw connect(): no handshake yet, decoder inert until establishment
}

private struct TimedTransportedLinkSource<Base: LinkSource>: LinkSource {
    let source: Base
    let timeoutNs: UInt64

    // r[impl transport.prologue.first-payload]
    func nextLink() async throws -> LinkAttachment {
        let attachment = try await source.nextLink()
        guard !attachment.hasCompletedPrologue else {
            try? await attachment.link.close()
            throw TransportError.protocolViolation(
                "initiator transport source cannot yield prologue-complete attachments"
            )
        }

        do {
            try await withThrowingTaskGroup(of: Void.self) { group in
                group.addTask {
                    try await performInitiatorLinkPrologue(link: attachment.link)
                }
                group.addTask {
                    try await Task.sleep(nanoseconds: timeoutNs)
                    throw TransportError.protocolViolation("transport prologue timed out")
                }
                defer { group.cancelAll() }
                _ = try await group.next()
            }
            return attachment
        } catch {
            try? await attachment.link.close()
            throw error
        }
    }
}
