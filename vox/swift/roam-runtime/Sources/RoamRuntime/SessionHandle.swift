import Foundation

public final class SessionHandle: @unchecked Sendable {
    private let coordinator: SessionResumeCoordinator

    init(coordinator: SessionResumeCoordinator) {
        self.coordinator = coordinator
    }

    public func resume(_ link: any Link) async throws {
        try await coordinator.resume(.initiator(link))
    }

    public func resume(_ attachment: LinkAttachment) async throws {
        try await coordinator.resume(attachment)
    }

    public func acceptResumedLink(_ link: any Link) async throws {
        try await coordinator.acceptResumedAttachment(.init(link: link))
    }

    public func acceptResumedAttachment(_ attachment: LinkAttachment) async throws {
        try await coordinator.acceptResumedAttachment(attachment)
    }

    public func shutdown() async {
        await coordinator.shutdown()
    }
}
