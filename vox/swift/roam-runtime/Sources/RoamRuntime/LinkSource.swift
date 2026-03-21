public struct LinkAttachment: Sendable {
    public let link: any Link
    public let clientHello: [UInt8]?

    public init(link: any Link, clientHello: [UInt8]? = nil) {
        self.link = link
        self.clientHello = clientHello
    }

    public static func initiator(_ link: any Link) -> Self {
        Self(link: link, clientHello: nil)
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
    let conduit: TransportConduitKind

    func nextLink() async throws -> LinkAttachment {
        let attachment = try await source.nextLink()
        guard attachment.clientHello == nil else {
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
            return attachment
        } catch {
            try? await attachment.link.close()
            throw error
        }
    }
}
