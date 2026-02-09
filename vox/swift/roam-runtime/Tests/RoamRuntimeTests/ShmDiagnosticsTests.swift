#if os(macOS)
import Foundation
import Testing

@testable import RoamRuntime

struct ShmDiagnosticsTests {
    @Test func registryDumpsRegisteredSnapshotsWhenEnabled() {
        ShmDiagnosticsRegistry.setEnabled(true)
        defer { ShmDiagnosticsRegistry.setEnabled(false) }

        let id = UUID()
        ShmDiagnosticsRegistry.register(id: id) {
            ShmTransportDiagnosticsSnapshot(
                id: id,
                peerId: 7,
                maxPayloadSize: 4096,
                initialCredit: 1024,
                maxFrameSize: 8192,
                closed: false,
                hostGoodbye: false,
                timestamp: Date()
            )
        }
        defer { ShmDiagnosticsRegistry.unregister(id: id) }

        let dump = ShmDiagnosticsRegistry.dumpAllState()
        #expect(dump.contains("peer=7"))
        #expect(dump.contains("max_payload=4096"))
    }
}
#endif
