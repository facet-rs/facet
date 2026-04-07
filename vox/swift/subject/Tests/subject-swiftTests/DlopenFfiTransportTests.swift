import Darwin
import Foundation
import Testing

@testable import VoxRuntime
@testable import subject_swift

private enum RustSubjectLibrary {
    static func libraryPath() throws -> URL {
        let env = ProcessInfo.processInfo.environment["VOX_RUST_FFI_DYLIB_PATH"]
        let candidates: [URL]
        if let env, !env.isEmpty {
            candidates = [URL(fileURLWithPath: env)]
        } else {
            let repoRoot = URL(fileURLWithPath: #filePath)
                .deletingLastPathComponent() // subject-swiftTests
                .deletingLastPathComponent() // Tests
                .deletingLastPathComponent() // subject-swift
                .deletingLastPathComponent() // swift
                .deletingLastPathComponent() // repo root
            candidates = [
                repoRoot.appendingPathComponent("target/release/libsubject_rust.dylib"),
                repoRoot.appendingPathComponent("target/debug/libsubject_rust.dylib"),
            ]
        }

        for candidate in candidates where FileManager.default.fileExists(atPath: candidate.path) {
            return candidate
        }

        throw TransportError.protocolViolation(
            "could not find libsubject_rust.dylib; build it with `cargo build --release -p subject-rust`"
        )
    }
}

@Suite(.serialized)
struct DlopenFfiTransportTests {
    // r[verify link.message]
    // r[verify link.order]
    // r[verify link.rx.recv]
    @Test func swiftCanDriveRustSubjectLoadedViaDlopen() async throws {
        let rust = try FfiDynamicLibrary(path: RustSubjectLibrary.libraryPath())
        let endpoint = FfiEndpoint()
        let connector = try endpoint.connector(
            peer: rust.loadVtable(symbol: "subject_rust_v1_vtable")
        )
        let dispatcher = TestbedDispatcher(handler: TestbedService())

        let session = try await Session.initiator(
            connector,
            dispatcher: dispatcher,
            resumable: false
        )
        let driverTask = Task<Void, Error> {
            try await session.run()
        }

        let client = TestbedClient(connection: session.connection)
        let echoed = try await client.echo(message: "hello from swift")
        #expect(echoed == "hello from swift")

        let divided = try await client.divide(dividend: 10, divisor: 2)
        #expect(divided == .success(5))

        session.handle.shutdown()
        _ = try await driverTask.value
    }
}
