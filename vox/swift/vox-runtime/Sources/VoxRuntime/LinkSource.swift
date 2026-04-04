public enum LinkAttachmentState: Sendable {
    case fresh
    case conduitNegotiated(ConduitKind)
    case stableClientHello([UInt8])
}

public struct LinkAttachment: Sendable {
    public let link: any Link
    public let state: LinkAttachmentState

    public init(link: any Link, state: LinkAttachmentState = .fresh) {
        self.link = link
        self.state = state
    }

    public static func initiator(_ link: any Link) -> Self {
        Self(link: link, state: .fresh)
    }

    public static func fresh(_ link: any Link) -> Self {
        Self(link: link, state: .fresh)
    }

    public static func negotiated(_ link: any Link, conduit: ConduitKind) -> Self {
        Self(link: link, state: .conduitNegotiated(conduit))
    }

    public static func stableAccepted(_ link: any Link, clientHello: [UInt8]) -> Self {
        Self(link: link, state: .stableClientHello(clientHello))
    }

    public var negotiatedConduit: ConduitKind? {
        switch state {
        case .fresh:
            nil
        case .conduitNegotiated(let conduit):
            conduit
        case .stableClientHello:
            .stable
        }
    }

    public var stableClientHello: [UInt8]? {
        guard case .stableClientHello(let clientHello) = state else {
            return nil
        }
        return clientHello
    }
}

public protocol LinkSource: Sendable {
    func nextLink() async throws -> LinkAttachment
}

public struct AnyLinkSource: LinkSource, Sendable {
    private let nextLinkFn: @Sendable () async throws -> LinkAttachment

    public init(_ nextLink: @escaping @Sendable () async throws -> LinkAttachment) {
        self.nextLinkFn = nextLink
    }

    public func nextLink() async throws -> LinkAttachment {
        try await nextLinkFn()
    }
}

public actor SingleAttachmentSource: LinkSource {
    private var attachment: LinkAttachment?

    public init(attachment: LinkAttachment) {
        self.attachment = attachment
    }

    public func nextLink() async throws -> LinkAttachment {
        guard let attachment else {
            throw TransportError.protocolViolation("single-use LinkSource exhausted")
        }
        self.attachment = nil
        return attachment
    }
}

public func singleAttachmentSource(_ attachment: LinkAttachment) -> some LinkSource {
    SingleAttachmentSource(attachment: attachment)
}

public func singleLinkSource(_ link: any Link) -> some LinkSource {
    singleAttachmentSource(.initiator(link))
}

public actor PrefetchedLinkSource<Base: LinkSource>: LinkSource {
    private var first: LinkAttachment?
    private let base: Base

    public init(first: LinkAttachment, base: Base) {
        self.first = first
        self.base = base
    }

    public func nextLink() async throws -> LinkAttachment {
        if let first {
            self.first = nil
            return first
        }
        return try await base.nextLink()
    }
}

struct TransportedLinkSource<Base: LinkSource>: LinkSource {
    let source: Base
    let conduit: ConduitKind

    func nextLink() async throws -> LinkAttachment {
        let attachment = try await source.nextLink()
        guard attachment.negotiatedConduit == nil else {
            try? await attachment.link.close()
            throw TransportError.protocolViolation(
                "initiator transport source cannot yield acceptor-prepared attachments"
            )
        }

        do {
            try await performInitiatorTransportPrologue(
                transport: attachment.link,
                conduit: conduit
            )
            return .negotiated(attachment.link, conduit: conduit)
        } catch {
            try? await attachment.link.close()
            throw error
        }
    }
}
