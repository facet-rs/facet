import Foundation

public final class SessionHandle: @unchecked Sendable {
    private let coordinator: SessionResumeCoordinator

    init(coordinator: SessionResumeCoordinator) {
        self.coordinator = coordinator
    }

    public func resume(_ conduit: any Conduit) async throws {
        try await coordinator.resume(conduit)
    }

    public func acceptResumedConduit(_ conduit: any Conduit) async throws {
        try await coordinator.acceptResumedConduit(conduit)
    }

    public func shutdown() async {
        await coordinator.shutdown()
    }
}
