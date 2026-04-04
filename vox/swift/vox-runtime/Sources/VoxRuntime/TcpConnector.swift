public protocol SessionConnector: Sendable {
    var transport: ConduitKind { get }
    func openAttachment() async throws -> LinkAttachment
}

public struct TcpConnector: SessionConnector, LinkSource, Sendable {
    public let host: String
    public let port: Int
    public let transport: ConduitKind

    public init(host: String, port: Int, transport: ConduitKind = .bare) {
        self.host = host
        self.port = port
        self.transport = transport
    }

    public func bare() -> Self {
        Self(host: host, port: port, transport: .bare)
    }

    public func stable() -> Self {
        Self(host: host, port: port, transport: .stable)
    }

    public func nextLink() async throws -> LinkAttachment {
        LinkAttachment.initiator(try await connectLink(host: host, port: port))
    }

    public func openAttachment() async throws -> LinkAttachment {
        try await TransportedLinkSource(source: self, conduit: transport).nextLink()
    }
}

public struct UnixConnector: SessionConnector, LinkSource, Sendable {
    public let path: String
    public let transport: ConduitKind

    public init(path: String, transport: ConduitKind = .bare) {
        self.path = path
        self.transport = transport
    }

    public func bare() -> Self {
        Self(path: path, transport: .bare)
    }

    public func stable() -> Self {
        Self(path: path, transport: .stable)
    }

    public func nextLink() async throws -> LinkAttachment {
        LinkAttachment.initiator(try await connectLink(unixPath: path))
    }

    public func openAttachment() async throws -> LinkAttachment {
        try await TransportedLinkSource(source: self, conduit: transport).nextLink()
    }
}

public func connect(unixPath: String, conduit: ConduitKind = .bare) async throws -> any Conduit {
    try await connect(
        unixPath: unixPath,
        conduit: conduit,
        prologueTimeoutNs: defaultTransportPrologueTimeoutNs
    )
}

func connect(
    unixPath: String,
    conduit: ConduitKind = .bare,
    prologueTimeoutNs: UInt64
) async throws -> any Conduit {
    let connector = UnixConnector(path: unixPath, transport: conduit)
    if conduit == .bare {
        let attachment = try await TimedTransportedLinkSource(
            source: connector,
            conduit: conduit,
            timeoutNs: prologueTimeoutNs
        ).nextLink()
        return BareConduit(link: attachment.link)
    }

    let source = TimedTransportedLinkSource(
        source: connector,
        conduit: conduit,
        timeoutNs: prologueTimeoutNs
    )
    return try await StableConduit.connect(source: source)
}

public func connect(host: String, port: Int, conduit: ConduitKind = .bare) async throws -> any Conduit {
    try await connect(
        host: host,
        port: port,
        conduit: conduit,
        prologueTimeoutNs: defaultTransportPrologueTimeoutNs
    )
}

func connect(
    host: String,
    port: Int,
    conduit: ConduitKind = .bare,
    prologueTimeoutNs: UInt64
) async throws -> any Conduit {
    let connector = TcpConnector(host: host, port: port, transport: conduit)
    if conduit == .bare {
        let attachment = try await TimedTransportedLinkSource(
            source: connector,
            conduit: conduit,
            timeoutNs: prologueTimeoutNs
        ).nextLink()
        return BareConduit(link: attachment.link)
    }

    let source = TimedTransportedLinkSource(
        source: connector,
        conduit: conduit,
        timeoutNs: prologueTimeoutNs
    )
    return try await StableConduit.connect(source: source)
}

private struct TimedTransportedLinkSource<Base: LinkSource>: LinkSource {
    let source: Base
    let conduit: ConduitKind
    let timeoutNs: UInt64

    func nextLink() async throws -> LinkAttachment {
        let attachment = try await source.nextLink()
        guard attachment.negotiatedConduit == nil else {
            try? await attachment.link.close()
            throw TransportError.protocolViolation(
                "initiator transport source cannot yield acceptor-prepared attachments"
            )
        }

        do {
            try await withThrowingTaskGroup(of: Void.self) { group in
                group.addTask {
                    try await performInitiatorTransportPrologue(
                        transport: attachment.link,
                        conduit: conduit
                    )
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
