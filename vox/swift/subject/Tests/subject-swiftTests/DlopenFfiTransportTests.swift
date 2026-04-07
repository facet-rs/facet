import Darwin
import Foundation
import Testing

@testable import VoxRuntime
@testable import subject_swift

private struct FfiAttachmentConnector: SessionConnector {
    let attachment: LinkAttachment
    let transport: ConduitKind = .bare

    func openAttachment() async throws -> LinkAttachment {
        guard attachment.negotiatedConduit == nil else {
            return attachment
        }
        try await performInitiatorLinkPrologue(
            link: attachment.link,
            conduit: transport
        )
        return .negotiated(attachment.link, conduit: transport)
    }
}

private func dlopenTestLog(_ message: String) {
    let line = "[DlopenFfiTransportTests] \(message)\n"
    FileHandle.standardError.write(Data(line.utf8))
    let url = URL(fileURLWithPath: "/tmp/dlopen-swift-test.trace")
    if !FileManager.default.fileExists(atPath: url.path) {
        FileManager.default.createFile(atPath: url.path, contents: nil)
    }
    if let handle = try? FileHandle(forWritingTo: url) {
        defer { try? handle.close() }
        _ = try? handle.seekToEnd()
        try? handle.write(contentsOf: Data(line.utf8))
    }
}

private final class RustSubjectLibrary {
    let handle: UnsafeMutableRawPointer
    let vtable: UnsafePointer<VoxLinkVtable>
    private let shutdownFn: @convention(c) () -> Void

    init() throws {
        let path = try RustSubjectLibrary.libraryPath()
        let handle = path.path.withCString { dlopen($0, RTLD_NOW | RTLD_LOCAL) }
        guard let handle else {
            throw TransportError.protocolViolation(
                "failed to dlopen \(path.path): \(String(cString: dlerror()))")
        }

        guard let symbol = dlsym(handle, "subject_rust_v1_vtable") else {
            dlclose(handle)
            throw TransportError.protocolViolation(
                "missing subject_rust_v1_vtable in \(path.path)")
        }

        typealias ExportFn = @convention(c) () -> UnsafeMutableRawPointer?
        let export = unsafeBitCast(symbol, to: ExportFn.self)
        guard let vtable = export() else {
            dlclose(handle)
            throw TransportError.protocolViolation(
                "subject_rust_v1_vtable returned a null pointer")
        }
        guard let shutdownSymbol = dlsym(handle, "subject_rust_v1_shutdown") else {
            dlclose(handle)
            throw TransportError.protocolViolation(
                "missing subject_rust_v1_shutdown in \(path.path)")
        }

        self.handle = handle
        self.vtable = UnsafePointer(vtable.assumingMemoryBound(to: VoxLinkVtable.self))
        self.shutdownFn = unsafeBitCast(shutdownSymbol, to: (@convention(c) () -> Void).self)
    }

    deinit {
        dlclose(handle)
    }

    func shutdown() {
        shutdownFn()
    }

    private static func libraryPath() throws -> URL {
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
        dlopenTestLog("loading RustSubjectLibrary")
        let rust = try RustSubjectLibrary()
        dlopenTestLog("creating FfiEndpoint")
        let endpoint = FfiEndpoint()
        dlopenTestLog("connecting endpoint to rust vtable")
        let link = try endpoint.connect(peer: rust.vtable)
        let attachment = LinkAttachment.initiator(link)
        let connector = FfiAttachmentConnector(attachment: attachment)
        let dispatcher = TestbedDispatcher(handler: TestbedService())

        dlopenTestLog("establishing initiator session")
        let session = try await Session.initiator(
            connector,
            dispatcher: dispatcher,
            resumable: false
        )
        dlopenTestLog("spawning session.run task")
        let driverTask = Task {
            try await session.run()
        }

        let client = TestbedClient(connection: session.connection)
        dlopenTestLog("calling echo")
        let echoed = try await client.echo(message: "hello from swift")
        #expect(echoed == "hello from swift")

        dlopenTestLog("calling divide")
        let divided = try await client.divide(dividend: 10, divisor: 2)
        #expect(divided == .success(5))

        dlopenTestLog("requesting rust shutdown")
        rust.shutdown()
        dlopenTestLog("shutting down session")
        session.handle.shutdown()
        dlopenTestLog("awaiting driver task")
        try await driverTask.value
        dlopenTestLog("test complete")
    }
}
