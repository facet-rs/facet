public enum LinkAttachmentState: Sendable {
    case fresh
    case prologueComplete
}

public struct LinkAttachment: Sendable {
    public let link: any Link
    public let state: LinkAttachmentState
    public let peerEvidence: PeerEvidence

    public init(
        link: any Link,
        state: LinkAttachmentState = .fresh,
        peerEvidence: PeerEvidence = .none
    ) {
        self.link = link
        self.state = state
        self.peerEvidence = peerEvidence
    }

    public static func initiator(
        _ link: any Link,
        peerEvidence: PeerEvidence = .none
    ) -> Self {
        Self(link: link, state: .fresh, peerEvidence: peerEvidence)
    }

    public static func fresh(
        _ link: any Link,
        peerEvidence: PeerEvidence = .none
    ) -> Self {
        Self(link: link, state: .fresh, peerEvidence: peerEvidence)
    }

    public static func prologueComplete(
        _ link: any Link,
        peerEvidence: PeerEvidence = .none
    ) -> Self {
        Self(link: link, state: .prologueComplete, peerEvidence: peerEvidence)
    }

    public var hasCompletedPrologue: Bool {
        switch state {
        case .fresh:
            false
        case .prologueComplete:
            true
        }
    }
}

public protocol LinkSource: Sendable {
    // r[impl link.split]
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

// r[impl link.split]
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
            try await performInitiatorLinkPrologue(link: attachment.link)
            return .prologueComplete(
                attachment.link,
                peerEvidence: attachment.peerEvidence
            )
        } catch {
            try? await attachment.link.close()
            throw error
        }
    }
}
