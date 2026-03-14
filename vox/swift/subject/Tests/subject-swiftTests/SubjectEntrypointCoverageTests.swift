#if os(macOS)
import Darwin
import Foundation
@preconcurrency import NIO
@preconcurrency import NIOCore
@preconcurrency import NIOPosix
import Testing
import CRoamShmFfi

@testable import RoamRuntime
@testable import subject_swift

private actor SubjectEnvGate {
    static let shared = SubjectEnvGate()
    private var busy = false
    private var waiters: [CheckedContinuation<Void, Never>] = []

    private func acquire() async {
        if !busy {
            busy = true
            return
        }
        await withCheckedContinuation { cont in
            waiters.append(cont)
        }
    }

    private func release() {
        if waiters.isEmpty {
            busy = false
            return
        }
        let waiter = waiters.removeFirst()
        waiter.resume()
    }

    func withEnvironment<T>(
        _ pairs: [(String, String?)],
        body: () async throws -> T
    ) async rethrows -> T {
        await acquire()
        defer { release() }

        var previous: [(String, String?)] = []
        previous.reserveCapacity(pairs.count)
        for (key, value) in pairs {
            previous.append((key, currentValue(for: key)))
            set(value: value, for: key)
        }
        defer {
            for (key, value) in previous {
                set(value: value, for: key)
            }
        }
        return try await body()
    }

    private func currentValue(for key: String) -> String? {
        guard let raw = getenv(key) else { return nil }
        return String(cString: raw)
    }

    private func set(value: String?, for key: String) {
        if let value {
            _ = setenv(key, value, 1)
        } else {
            _ = unsetenv(key)
        }
    }
}

private enum EntrypointTestError: Error {
    case timeout(String)
    case invalidLocalAddress
    case socketSetupFailed
    case bootstrapThreadDidNotFinish
}

private func withTimeout<T: Sendable>(
    milliseconds: UInt64,
    operation: @escaping @Sendable () async throws -> T
) async throws -> T {
    try await withThrowingTaskGroup(of: T.self) { group in
        group.addTask {
            try await operation()
        }
        group.addTask {
            try await Task.sleep(nanoseconds: milliseconds * 1_000_000)
            throw EntrypointTestError.timeout("operation timed out after \(milliseconds)ms")
        }
        let result = try await group.next()!
        group.cancelAll()
        return result
    }
}

private actor AcceptedTransportBox {
    private var link: NIOFrameLink?
    private var waiters: [CheckedContinuation<NIOFrameLink, Never>] = []

    func publish(_ link: NIOFrameLink) {
        if self.link == nil {
            self.link = link
        }
        let pending = waiters
        waiters.removeAll()
        for waiter in pending {
            waiter.resume(returning: link)
        }
    }

    func wait() async -> NIOFrameLink {
        if let link {
            return link
        }
        return await withCheckedContinuation { cont in
            waiters.append(cont)
        }
    }

    func closeTransport() async {
        if let link {
            try? await link.close()
        }
    }
}

private final class HarnessTransportPrologueHandler: ChannelInboundHandler, RemovableChannelHandler, @unchecked Sendable {
    typealias InboundIn = [UInt8]

    private let accepted: AcceptedTransportBox
    private let link: NIOFrameLink
    private var didAccept = false

    init(
        accepted: AcceptedTransportBox,
        link: NIOFrameLink
    ) {
        self.accepted = accepted
        self.link = link
    }

    func channelRead(context: ChannelHandlerContext, data: NIOAny) {
        guard !didAccept else {
            context.fireChannelRead(data)
            return
        }
        didAccept = true

        let bytes = unwrapInboundIn(data)
        do {
            let requested = try decodeTransportHello(bytes)
            let channel = context.channel
            context.pipeline.removeHandler(self).flatMap {
                self.writeAccept(channel: channel, conduit: requested)
            }.whenComplete { result in
                switch result {
                case .success:
                    Task { await self.accepted.publish(self.link) }
                case .failure(let error):
                    context.fireErrorCaught(error)
                }
            }
        } catch {
            context.fireErrorCaught(error)
        }
    }

    private func writeAccept(
        channel: Channel,
        conduit: TransportConduitKind
    ) -> EventLoopFuture<Void> {
        let bytes = encodeTransportAccept(conduit)
        var buffer = channel.allocator.buffer(capacity: 4 + bytes.count)
        buffer.writeInteger(UInt32(bytes.count), endianness: .little)
        buffer.writeBytes(bytes)
        return channel.writeAndFlush(buffer)
    }
}

private struct NoopDispatcher: ServiceDispatcher {
    func preregister(
        methodId _: UInt64,
        payload _: [UInt8],
        channels _: [UInt64],
        registry _: ChannelRegistry
    ) async {}

    func dispatch(
        methodId _: UInt64,
        payload _: [UInt8],
        channels _: [UInt64],
        requestId _: UInt64,
        registry _: ChannelRegistry,
        taskTx _: @escaping @Sendable (TaskMessage) -> Void
    ) async {}
}

private struct TcpAcceptorHarness {
    let group: MultiThreadedEventLoopGroup
    let listener: Channel
    let accepted: AcceptedTransportBox
    let port: Int

    static func start() async throws -> TcpAcceptorHarness {
        let group = MultiThreadedEventLoopGroup(numberOfThreads: 1)
        let accepted = AcceptedTransportBox()

        let bootstrap = ServerBootstrap(group: group)
            .serverChannelOption(ChannelOptions.backlog, value: 8)
            .serverChannelOption(ChannelOptions.socketOption(.so_reuseaddr), value: 1)
            .childChannelOption(ChannelOptions.socketOption(.so_reuseaddr), value: 1)
            .childChannelInitializer { channel in
                let frameLimit = FrameLimit(1024 * 1024)
                var rawContinuation: AsyncStream<Result<[UInt8], Error>>.Continuation!
                let rawStream = AsyncStream<Result<[UInt8], Error>> { continuation in
                    rawContinuation = continuation
                }
                let capturedRawContinuation = rawContinuation!
                let link = NIOFrameLink(
                    channel: channel,
                    frameLimit: frameLimit,
                    inboundStream: rawStream
                )
                let rawHandler = RawFrameStreamHandler(continuation: capturedRawContinuation)
                let prologueHandler = HarnessTransportPrologueHandler(
                    accepted: accepted,
                    link: link
                )

                return channel.pipeline.addHandler(
                    ByteToMessageHandler(LengthPrefixDecoder(frameLimit: frameLimit))
                ).flatMap {
                    channel.pipeline.addHandler(prologueHandler)
                }.flatMap {
                    channel.pipeline.addHandler(rawHandler)
                }
            }

        let listener = try await bootstrap.bind(host: "127.0.0.1", port: 0).get()
        guard let local = listener.localAddress else {
            try? await listener.close().get()
            await shutdownEventLoopGroup(group)
            throw EntrypointTestError.invalidLocalAddress
        }

        return TcpAcceptorHarness(group: group, listener: listener, accepted: accepted, port: local.port!)
    }

    func waitForTransport() async -> NIOFrameLink {
        await accepted.wait()
    }

    func closeAcceptedTransport() async {
        await accepted.closeTransport()
    }

    func shutdown() async {
        try? await listener.close().get()
        await shutdownEventLoopGroup(group)
    }
}

private func shutdownEventLoopGroup(_ group: MultiThreadedEventLoopGroup) async {
    await withCheckedContinuation { cont in
        group.shutdownGracefully { _ in
            cont.resume()
        }
    }
}

private struct ShmFixture {
    let path: String
    let region: ShmRegion
}

@inline(__always)
private func alignUp(_ value: Int, to alignment: Int) -> Int {
    let mask = alignment - 1
    return (value + mask) & ~mask
}

private func makeShmFixture(path: String) throws -> ShmFixture {
    let maxGuests: UInt32 = 2
    let bipbufCapacity: UInt32 = 512
    let classes = [
        ShmVarSlotClass(slotSize: 256, count: 8),
        ShmVarSlotClass(slotSize: 1024, count: 4),
    ]

    let peerTableOffset = shmSegmentHeaderSize
    let peerTableSize = Int(maxGuests) * shmPeerEntrySize
    let varPoolOffset = alignUp(peerTableOffset + peerTableSize, to: 64)
    let varPoolSize = ShmVarSlotPool.calculateSize(classes: classes)
    let guestAreasOffset = alignUp(varPoolOffset + varPoolSize, to: 64)

    let perPeerArea = alignUp((shmBipbufHeaderSize + Int(bipbufCapacity)) * 2 + shmChannelEntrySize, to: 64)
    let totalSize = guestAreasOffset + perPeerArea * Int(maxGuests)

    let region = try ShmRegion.create(path: path, size: totalSize, cleanup: .manual)

    var header = [UInt8](repeating: 0, count: shmSegmentHeaderSize)
    for (idx, b) in shmSegmentMagic.enumerated() {
        header[idx] = b
    }
    writeU32LE(2, to: &header, at: 8)
    writeU32LE(UInt32(shmSegmentHeaderSize), to: &header, at: 12)
    writeU64LE(UInt64(totalSize), to: &header, at: 16)
    writeU32LE(4096, to: &header, at: 24)
    writeU32LE(32, to: &header, at: 28)
    writeU32LE(maxGuests, to: &header, at: 32)
    writeU32LE(bipbufCapacity, to: &header, at: 36)
    writeU64LE(UInt64(peerTableOffset), to: &header, at: 40)
    writeU64LE(0, to: &header, at: 48)
    writeU32LE(0, to: &header, at: 56)
    writeU32LE(64, to: &header, at: 60)
    writeU32LE(1, to: &header, at: 64)
    writeU32LE(0, to: &header, at: 68)
    writeU64LE(0, to: &header, at: 72)
    writeU64LE(UInt64(varPoolOffset), to: &header, at: 80)
    writeU64LE(UInt64(totalSize), to: &header, at: 88)
    writeU64LE(UInt64(guestAreasOffset), to: &header, at: 96)
    writeU32LE(UInt32(classes.count), to: &header, at: 104)

    let headerBytes = try region.mutableBytes(at: 0, count: shmSegmentHeaderSize)
    headerBytes.copyBytes(from: header)

    let pool = try ShmVarSlotPool(region: region, baseOffset: varPoolOffset, classes: classes)
    pool.initialize()

    for peer in 1...UInt8(maxGuests) {
        let peerOffset = peerTableOffset + Int(peer - 1) * shmPeerEntrySize
        var entry = [UInt8](repeating: 0, count: shmPeerEntrySize)
        let state: UInt32 = (peer == 1) ? ShmPeerState.reserved.rawValue : ShmPeerState.empty.rawValue
        writeU32LE(state, to: &entry, at: 0)

        let ringOffset = guestAreasOffset + Int(peer - 1) * perPeerArea
        writeU64LE(UInt64(ringOffset), to: &entry, at: 32)
        writeU64LE(0, to: &entry, at: 40)
        writeU64LE(UInt64(ringOffset + (shmBipbufHeaderSize + Int(bipbufCapacity)) * 2), to: &entry, at: 48)

        let entryBytes = try region.mutableBytes(at: peerOffset, count: shmPeerEntrySize)
        entryBytes.copyBytes(from: entry)

        _ = try ShmBipBuffer.initialize(region: region, headerOffset: ringOffset, capacity: bipbufCapacity)
        _ = try ShmBipBuffer.initialize(
            region: region,
            headerOffset: ringOffset + shmBipbufHeaderSize + Int(bipbufCapacity),
            capacity: bipbufCapacity
        )
    }

    return ShmFixture(path: path, region: region)
}

private struct BootstrapServerHandle {
    let socketPath: String
    let thread: Thread
}

private func makeUnixListener(path: String) throws -> Int32 {
    unlink(path)

    let fd = socket(AF_UNIX, SOCK_STREAM, 0)
    guard fd >= 0 else {
        throw EntrypointTestError.socketSetupFailed
    }

    var addr = sockaddr_un()
    addr.sun_family = sa_family_t(AF_UNIX)

    let pathBytes = [UInt8](path.utf8)
    let maxPathLen = MemoryLayout.size(ofValue: addr.sun_path)
    guard pathBytes.count < maxPathLen else {
        close(fd)
        throw EntrypointTestError.socketSetupFailed
    }

    withUnsafeMutablePointer(to: &addr.sun_path) { sunPathPtr in
        let raw = UnsafeMutableRawPointer(sunPathPtr)
        raw.initializeMemory(as: UInt8.self, repeating: 0, count: maxPathLen)
        raw.copyMemory(from: pathBytes, byteCount: pathBytes.count)
    }

    let bindResult = withUnsafePointer(to: &addr) { ptr in
        ptr.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockPtr in
            bind(fd, sockPtr, socklen_t(MemoryLayout<sockaddr_un>.size))
        }
    }
    guard bindResult == 0, listen(fd, 1) == 0 else {
        close(fd)
        throw EntrypointTestError.socketSetupFailed
    }

    return fd
}

private func readExactly(fd: Int32, count: Int) -> [UInt8]? {
    if count == 0 { return [] }

    var out = [UInt8](repeating: 0, count: count)
    var offset = 0
    while offset < count {
        let n = out.withUnsafeMutableBytes { raw in
            read(fd, raw.baseAddress!.advanced(by: offset), count - offset)
        }
        if n < 0 {
            if errno == EINTR { continue }
            return nil
        }
        if n == 0 {
            return nil
        }
        offset += n
    }
    return out
}

private func startBootstrapServer(
    expectedSid: String,
    payloadPath: String,
    doorbellFd: Int32,
    shmFd: Int32,
    mmapControlFd: Int32
) throws -> BootstrapServerHandle {
    let socketPath = "/tmp/subject-shm-bootstrap-\(UUID().uuidString.prefix(8)).sock"
    let listener = try makeUnixListener(path: socketPath)

    let thread = Thread {
        defer {
            close(listener)
            unlink(socketPath)
        }

        let client = accept(listener, nil, nil)
        guard client >= 0 else { return }
        defer { close(client) }

        guard let magic = readExactly(fd: client, count: 4), magic == [UInt8]("RSH0".utf8) else { return }
        guard let sidLenBytes = readExactly(fd: client, count: 2) else { return }
        let sidLen = Int(UInt16(sidLenBytes[0]) | (UInt16(sidLenBytes[1]) << 8))
        guard let sidBytes = readExactly(fd: client, count: sidLen),
              let sid = String(bytes: sidBytes, encoding: .utf8),
              sid == expectedSid
        else {
            return
        }

        let payloadBytes = [UInt8](payloadPath.utf8)
        let rc = payloadBytes.withUnsafeBufferPointer { buf in
            roam_shm_bootstrap_response_send_unix(
                client,
                0,
                1,
                buf.baseAddress,
                UInt(buf.count),
                doorbellFd,
                shmFd,
                mmapControlFd
            )
        }
        _ = rc
    }
    thread.start()
    return BootstrapServerHandle(socketPath: socketPath, thread: thread)
}

@inline(__always)
private func writeU32LE(_ value: UInt32, to bytes: inout [UInt8], at index: Int) {
    let le = value.littleEndian
    bytes[index] = UInt8(truncatingIfNeeded: le)
    bytes[index + 1] = UInt8(truncatingIfNeeded: le >> 8)
    bytes[index + 2] = UInt8(truncatingIfNeeded: le >> 16)
    bytes[index + 3] = UInt8(truncatingIfNeeded: le >> 24)
}

@inline(__always)
private func writeU64LE(_ value: UInt64, to bytes: inout [UInt8], at index: Int) {
    let le = value.littleEndian
    bytes[index] = UInt8(truncatingIfNeeded: le)
    bytes[index + 1] = UInt8(truncatingIfNeeded: le >> 8)
    bytes[index + 2] = UInt8(truncatingIfNeeded: le >> 16)
    bytes[index + 3] = UInt8(truncatingIfNeeded: le >> 24)
    bytes[index + 4] = UInt8(truncatingIfNeeded: le >> 32)
    bytes[index + 5] = UInt8(truncatingIfNeeded: le >> 40)
    bytes[index + 6] = UInt8(truncatingIfNeeded: le >> 48)
    bytes[index + 7] = UInt8(truncatingIfNeeded: le >> 56)
}

struct SubjectEntrypointCoverageTests {
    @Test func runServerRequiresPeerAddr() async {
        await SubjectEnvGate.shared.withEnvironment([("PEER_ADDR", nil)]) {
            do {
                try await runServer()
                Issue.record("expected missingEnv")
            } catch let error as SubjectError {
                guard case .missingEnv = error else {
                    Issue.record("expected missingEnv, got \(error)")
                    return
                }
            } catch {
                Issue.record("expected SubjectError.missingEnv, got \(error)")
            }
        }
    }

    @Test func runServerRejectsInvalidPeerAddr() async {
        await SubjectEnvGate.shared.withEnvironment([("PEER_ADDR", "127.0.0.1")]) {
            do {
                try await runServer()
                Issue.record("expected invalidAddr")
            } catch let error as SubjectError {
                guard case .invalidAddr = error else {
                    Issue.record("expected invalidAddr, got \(error)")
                    return
                }
            } catch {
                Issue.record("expected SubjectError.invalidAddr, got \(error)")
            }
        }
    }

    @Test func runClientRequiresPeerAddr() async {
        await SubjectEnvGate.shared.withEnvironment([("PEER_ADDR", nil)]) {
            do {
                try await runClient()
                Issue.record("expected missingEnv")
            } catch let error as SubjectError {
                guard case .missingEnv = error else {
                    Issue.record("expected missingEnv, got \(error)")
                    return
                }
            } catch {
                Issue.record("expected SubjectError.missingEnv, got \(error)")
            }
        }
    }

    @Test func runClientRejectsInvalidPeerAddr() async {
        await SubjectEnvGate.shared.withEnvironment([("PEER_ADDR", "127.0.0.1")]) {
            do {
                try await runClient()
                Issue.record("expected invalidAddr")
            } catch let error as SubjectError {
                guard case .invalidAddr = error else {
                    Issue.record("expected invalidAddr, got \(error)")
                    return
                }
            } catch {
                Issue.record("expected SubjectError.invalidAddr, got \(error)")
            }
        }
    }

    @Test func runClientEchoEndToEndOverTcpHarness() async throws {
        let harness = try await TcpAcceptorHarness.start()
        let serverTask = Task {
            let link = await harness.waitForTransport()
            let dispatcher = TestbedDispatcherAdapter(handler: TestbedService())
            let (_, driver, _, _) = try await establishAcceptor(
                conduit: BareConduit(link: link),
                dispatcher: dispatcher
            )
            try await driver.run()
        }

        defer {
            serverTask.cancel()
            Task {
                await harness.closeAcceptedTransport()
                await harness.shutdown()
            }
        }

        try await SubjectEnvGate.shared.withEnvironment([
            ("PEER_ADDR", "127.0.0.1:\(harness.port)"),
            ("CLIENT_SCENARIO", "echo"),
        ]) {
            try await withTimeout(milliseconds: 2_000) {
                try await runClient()
            }
        }

        serverTask.cancel()
        await harness.closeAcceptedTransport()
        _ = try await withTimeout(milliseconds: 2_000) {
            try await serverTask.value
            return ()
        }
    }

    @Test func runShmClientRequiresControlSock() async {
        await SubjectEnvGate.shared.withEnvironment([
            ("SHM_CONTROL_SOCK", nil),
            ("SHM_SESSION_ID", UUID().uuidString.lowercased()),
        ]) {
            do {
                try await runShmClient()
                Issue.record("expected missingEnv")
            } catch let error as SubjectError {
                guard case .missingEnv = error else {
                    Issue.record("expected missingEnv, got \(error)")
                    return
                }
            } catch {
                Issue.record("expected SubjectError.missingEnv, got \(error)")
            }
        }
    }

    @Test func runShmClientRequiresSessionId() async {
        await SubjectEnvGate.shared.withEnvironment([
            ("SHM_CONTROL_SOCK", "/tmp/does-not-matter.sock"),
            ("SHM_SESSION_ID", nil),
        ]) {
            do {
                try await runShmClient()
                Issue.record("expected missingEnv")
            } catch let error as SubjectError {
                guard case .missingEnv = error else {
                    Issue.record("expected missingEnv, got \(error)")
                    return
                }
            } catch {
                Issue.record("expected SubjectError.missingEnv, got \(error)")
            }
        }
    }

    @Test func runShmServerRequiresControlSock() async {
        await SubjectEnvGate.shared.withEnvironment([
            ("SHM_CONTROL_SOCK", nil),
            ("SHM_SESSION_ID", UUID().uuidString.lowercased()),
        ]) {
            do {
                try await runShmServer()
                Issue.record("expected missingEnv")
            } catch let error as SubjectError {
                guard case .missingEnv = error else {
                    Issue.record("expected missingEnv, got \(error)")
                    return
                }
            } catch {
                Issue.record("expected SubjectError.missingEnv, got \(error)")
            }
        }
    }

    @Test func runShmServerRequiresSessionId() async {
        await SubjectEnvGate.shared.withEnvironment([
            ("SHM_CONTROL_SOCK", "/tmp/does-not-matter.sock"),
            ("SHM_SESSION_ID", nil),
        ]) {
            do {
                try await runShmServer()
                Issue.record("expected missingEnv")
            } catch let error as SubjectError {
                guard case .missingEnv = error else {
                    Issue.record("expected missingEnv, got \(error)")
                    return
                }
            } catch {
                Issue.record("expected SubjectError.missingEnv, got \(error)")
            }
        }
    }

    @Test func runShmHostServerRequiresControlSock() async {
        await SubjectEnvGate.shared.withEnvironment([
            ("SHM_CONTROL_SOCK", nil),
            ("SHM_SESSION_ID", UUID().uuidString.lowercased()),
        ]) {
            do {
                try await runShmHostServer()
                Issue.record("expected missingEnv")
            } catch let error as SubjectError {
                guard case .missingEnv = error else {
                    Issue.record("expected missingEnv, got \(error)")
                    return
                }
            } catch {
                Issue.record("expected SubjectError.missingEnv, got \(error)")
            }
        }
    }

    @Test func runShmHostServerRequiresSessionId() async {
        await SubjectEnvGate.shared.withEnvironment([
            ("SHM_CONTROL_SOCK", "/tmp/does-not-matter.sock"),
            ("SHM_SESSION_ID", nil),
        ]) {
            do {
                try await runShmHostServer()
                Issue.record("expected missingEnv")
            } catch let error as SubjectError {
                guard case .missingEnv = error else {
                    Issue.record("expected missingEnv, got \(error)")
                    return
                }
            } catch {
                Issue.record("expected SubjectError.missingEnv, got \(error)")
            }
        }
    }

    @Test func runShmClientInvalidControlSocketFailsFast() async {
        await SubjectEnvGate.shared.withEnvironment([
            ("SHM_CONTROL_SOCK", "/tmp/roam-nonexistent-\(UUID().uuidString).sock"),
            ("SHM_SESSION_ID", UUID().uuidString.lowercased()),
            ("CLIENT_SCENARIO", "echo"),
        ]) {
            do {
                try await withTimeout(milliseconds: 1_000) {
                    try await runShmClient()
                }
                Issue.record("expected bootstrap/connect failure")
            } catch let error as SubjectError {
                Issue.record("expected transport/bootstrap error, got subject error: \(error)")
            } catch {
                // Expected: bootstrap transport failure.
            }
        }
    }

    @Test func runShmServerInvalidControlSocketFailsFast() async {
        await SubjectEnvGate.shared.withEnvironment([
            ("SHM_CONTROL_SOCK", "/tmp/roam-nonexistent-\(UUID().uuidString).sock"),
            ("SHM_SESSION_ID", UUID().uuidString.lowercased()),
            ("ACCEPT_CONNECTIONS", "1"),
        ]) {
            do {
                try await withTimeout(milliseconds: 1_000) {
                    try await runShmServer()
                }
                Issue.record("expected bootstrap/connect failure")
            } catch let error as SubjectError {
                Issue.record("expected transport/bootstrap error, got subject error: \(error)")
            } catch {
                // Expected: bootstrap transport failure.
            }
        }
    }

    @Test func runShmHostServerEchoEndToEnd() async throws {
        let controlSock = "/tmp/subject-shm-host-\(UUID().uuidString.prefix(8)).sock"
        let hubPath = "/tmp/subject-shm-hub-\(UUID().uuidString).bin"
        let sid = UUID().uuidString.lowercased()
        defer {
            unlink(controlSock)
            try? FileManager.default.removeItem(atPath: hubPath)
        }

        try await SubjectEnvGate.shared.withEnvironment([
            ("SHM_CONTROL_SOCK", controlSock),
            ("SHM_SESSION_ID", sid),
            ("SHM_HUB_PATH", hubPath),
            ("ACCEPT_CONNECTIONS", "0"),
        ]) {
            let serverTask = Task {
                try await runShmHostServer()
            }

            var guestTransport: ShmGuestTransport?
            var driverTask: Task<Void, Error>?
            var ticket: ShmBootstrapTicket?
            defer {
                driverTask?.cancel()
                serverTask.cancel()
                if let guestTransport {
                    Task { try? await guestTransport.close() }
                }
                if let ticket {
                    close(ticket.doorbellFd)
                    if ticket.mmapControlFd >= 0 {
                        close(ticket.mmapControlFd)
                    }
                }
            }

            ticket = try await withTimeout(milliseconds: 2_000) {
                while !FileManager.default.fileExists(atPath: controlSock) {
                    try await Task.sleep(nanoseconds: 5_000_000)
                }
                return try requestShmBootstrapTicket(controlSocketPath: controlSock, sid: sid)
            }

            guestTransport = try ShmGuestTransport.attach(ticket: try #require(ticket))
            let attachedTransport = try #require(guestTransport)
            let (handle, driver, _, _) = try await withTimeout(milliseconds: 2_000) {
                try await establishShmGuest(
                    transport: attachedTransport,
                    dispatcher: NoopDispatcher(),
                    conduit: .bare
                )
            }
            driverTask = Task {
                try await driver.run()
            }

            let client = TestbedClient(connection: handle)
            let echoed = try await client.echo(message: "swift-shm-host")
            #expect(echoed == "swift-shm-host")

            try await attachedTransport.close()
            _ = await driverTask?.result
            _ = try await withTimeout(milliseconds: 2_000) {
                try await serverTask.value
                return ()
            }
        }
    }
}
#endif
