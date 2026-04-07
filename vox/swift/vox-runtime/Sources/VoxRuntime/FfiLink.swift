import Foundation

public typealias VoxSendFn = @convention(c) (
    _ buf: UnsafePointer<UInt8>?,
    _ len: Int
) -> Void

public typealias VoxFreeFn = @convention(c) (
    _ buf: UnsafePointer<UInt8>?
) -> Void

public typealias VoxAttachFn = @convention(c) (
    _ peer: UnsafeRawPointer?
) -> Void

public struct VoxLinkVtable {
    public var send: VoxSendFn?
    public var free: VoxFreeFn?
    public var attach: VoxAttachFn?

    public init(send: VoxSendFn?, free: VoxFreeFn?, attach: VoxAttachFn?) {
        self.send = send
        self.free = free
        self.attach = attach
    }
}

private struct IncomingFrame: @unchecked Sendable {
    let ptr: UnsafePointer<UInt8>?
    let len: Int
}

private final class OutboundLoan {
    let ptr: UnsafeMutablePointer<UInt8>
    let len: Int

    init(bytes: [UInt8]) {
        len = bytes.count
        ptr = UnsafeMutablePointer<UInt8>.allocate(capacity: max(bytes.count, 1))
        if !bytes.isEmpty {
            bytes.withUnsafeBufferPointer { source in
                ptr.initialize(from: source.baseAddress!, count: bytes.count)
            }
        }
    }

    deinit {
        if len > 0 {
            ptr.deinitialize(count: len)
        }
        ptr.deallocate()
    }
}

private final class EndpointCore: @unchecked Sendable {
    private let lock = NSLock()
    private var peer: UnsafePointer<VoxLinkVtable>?
    private var linkTaken = false
    private var inbox: [IncomingFrame] = []
    private var outbound: [UInt: OutboundLoan] = [:]
    private var recvWaiter: CheckedContinuation<IncomingFrame, Never>?
    private var acceptWaiter: CheckedContinuation<Void, Error>?
    private var vtableStorage: UnsafeMutablePointer<VoxLinkVtable>?

    func install(vtableStorage: UnsafeMutablePointer<VoxLinkVtable>) {
        lock.lock()
        self.vtableStorage = vtableStorage
        lock.unlock()
    }

    func exportedVtable() -> UnsafePointer<VoxLinkVtable> {
        lock.lock()
        let storage = vtableStorage
        lock.unlock()
        return UnsafePointer(storage!)
    }

    func connect(to peer: UnsafePointer<VoxLinkVtable>) throws -> FfiLink {
        lock.lock()
        if self.peer != nil {
            lock.unlock()
            throw TransportError.protocolViolation("ffi endpoint already attached")
        }
        self.peer = peer
        let local = UnsafePointer(vtableStorage!)
        lock.unlock()

        peer.pointee.attach?(UnsafeRawPointer(local))
        return try takeLink()
    }

    func accept() async throws -> FfiLink {
        try await withCheckedThrowingContinuation { continuation in
            lock.lock()
            if peer != nil {
                lock.unlock()
                continuation.resume(returning: ())
                return
            }

            acceptWaiter = continuation
            lock.unlock()
        }

        return try takeLink()
    }

    func attach(peer: UnsafePointer<VoxLinkVtable>) {
        lock.lock()
        if self.peer == nil {
            self.peer = peer
        }
        let waiter = acceptWaiter
        acceptWaiter = nil
        lock.unlock()

        waiter?.resume(returning: ())
    }

    func send(_ bytes: [UInt8]) throws {
        lock.lock()
        guard let peer else {
            lock.unlock()
            throw TransportError.connectionClosed
        }

        let loan = OutboundLoan(bytes: bytes)
        let ptr = loan.ptr
        let len = loan.len
        outbound[UInt(bitPattern: ptr)] = loan
        lock.unlock()

        peer.pointee.send?(UnsafePointer(ptr), len)
    }

    func nextFrame() async -> IncomingFrame {
        await withCheckedContinuation { continuation in
            lock.lock()
            if !inbox.isEmpty {
                let frame = inbox.removeFirst()
                lock.unlock()
                continuation.resume(returning: frame)
                return
            }

            recvWaiter = continuation
            lock.unlock()
        }
    }

    func releaseIncoming(_ ptr: UnsafePointer<UInt8>?) {
        lock.lock()
        let peer = self.peer
        lock.unlock()
        peer?.pointee.free?(ptr)
    }

    func receive(_ ptr: UnsafePointer<UInt8>?, len: Int) {
        let frame = IncomingFrame(ptr: ptr, len: len)

        lock.lock()
        if let waiter = recvWaiter {
            recvWaiter = nil
            lock.unlock()
            waiter.resume(returning: frame)
            return
        }

        inbox.append(frame)
        lock.unlock()
    }

    func free(_ ptr: UnsafePointer<UInt8>?) {
        guard let ptr else {
            return
        }
        lock.lock()
        outbound.removeValue(forKey: UInt(bitPattern: ptr))
        lock.unlock()
    }

    func outstandingLoanCount() -> Int {
        lock.lock()
        defer { lock.unlock() }
        return outbound.count
    }

    private func takeLink() throws -> FfiLink {
        lock.lock()
        defer { lock.unlock() }

        guard !linkTaken else {
            throw TransportError.protocolViolation("ffi endpoint already connected")
        }
        linkTaken = true
        return FfiLink(core: self)
    }
}

private final class EndpointHostState: @unchecked Sendable {
    let lock = NSLock()
    var core: EndpointCore?
}

private func activeEndpointCore(for state: EndpointHostState) -> EndpointCore {
    state.lock.lock()
    defer { state.lock.unlock() }
    guard let core = state.core else {
        fatalError("FFI endpoint is not installed")
    }
    return core
}

private struct EndpointHostSlot {
    let state: EndpointHostState
    let send: VoxSendFn
    let free: VoxFreeFn
    let attach: VoxAttachFn
}

private enum EndpointHost0 {
    static let state = EndpointHostState()

    static func send(_ buf: UnsafePointer<UInt8>?, _ len: Int) {
        activeEndpointCore(for: state).receive(buf, len: len)
    }

    static func free(_ buf: UnsafePointer<UInt8>?) {
        activeEndpointCore(for: state).free(buf)
    }

    static func attach(_ peer: UnsafeRawPointer?) {
        guard let peer else { return }
        activeEndpointCore(for: state).attach(peer: peer.assumingMemoryBound(to: VoxLinkVtable.self))
    }
}

private enum EndpointHost1 {
    static let state = EndpointHostState()

    static func send(_ buf: UnsafePointer<UInt8>?, _ len: Int) {
        activeEndpointCore(for: state).receive(buf, len: len)
    }

    static func free(_ buf: UnsafePointer<UInt8>?) {
        activeEndpointCore(for: state).free(buf)
    }

    static func attach(_ peer: UnsafeRawPointer?) {
        guard let peer else { return }
        activeEndpointCore(for: state).attach(peer: peer.assumingMemoryBound(to: VoxLinkVtable.self))
    }
}

private enum EndpointHost2 {
    static let state = EndpointHostState()

    static func send(_ buf: UnsafePointer<UInt8>?, _ len: Int) {
        activeEndpointCore(for: state).receive(buf, len: len)
    }

    static func free(_ buf: UnsafePointer<UInt8>?) {
        activeEndpointCore(for: state).free(buf)
    }

    static func attach(_ peer: UnsafeRawPointer?) {
        guard let peer else { return }
        activeEndpointCore(for: state).attach(peer: peer.assumingMemoryBound(to: VoxLinkVtable.self))
    }
}

private enum EndpointHost3 {
    static let state = EndpointHostState()

    static func send(_ buf: UnsafePointer<UInt8>?, _ len: Int) {
        activeEndpointCore(for: state).receive(buf, len: len)
    }

    static func free(_ buf: UnsafePointer<UInt8>?) {
        activeEndpointCore(for: state).free(buf)
    }

    static func attach(_ peer: UnsafeRawPointer?) {
        guard let peer else { return }
        activeEndpointCore(for: state).attach(peer: peer.assumingMemoryBound(to: VoxLinkVtable.self))
    }
}

private enum EndpointHosts {
    static let all: [EndpointHostSlot] = [
        EndpointHostSlot(
            state: EndpointHost0.state,
            send: EndpointHost0.send,
            free: EndpointHost0.free,
            attach: EndpointHost0.attach
        ),
        EndpointHostSlot(
            state: EndpointHost1.state,
            send: EndpointHost1.send,
            free: EndpointHost1.free,
            attach: EndpointHost1.attach
        ),
        EndpointHostSlot(
            state: EndpointHost2.state,
            send: EndpointHost2.send,
            free: EndpointHost2.free,
            attach: EndpointHost2.attach
        ),
        EndpointHostSlot(
            state: EndpointHost3.state,
            send: EndpointHost3.send,
            free: EndpointHost3.free,
            attach: EndpointHost3.attach
        ),
    ]

    static func claim(for core: EndpointCore) -> EndpointHostSlot {
        for host in all {
            host.state.lock.lock()
            if host.state.core == nil {
                host.state.core = core
                host.state.lock.unlock()
                return host
            }
            host.state.lock.unlock()
        }
        fatalError("no available FFI endpoint host slots")
    }

    static func release(_ host: EndpointHostSlot, core: EndpointCore) {
        host.state.lock.lock()
        if host.state.core === core {
            host.state.core = nil
        }
        host.state.lock.unlock()
    }
}

public final class FfiEndpoint: @unchecked Sendable {
    private let core: EndpointCore
    private let storage: UnsafeMutablePointer<VoxLinkVtable>
    private let host: EndpointHostSlot

    public init() {
        let core = EndpointCore()
        let host = EndpointHosts.claim(for: core)
        let storage = UnsafeMutablePointer<VoxLinkVtable>.allocate(capacity: 1)
        storage.initialize(
            to: VoxLinkVtable(
                send: host.send,
                free: host.free,
                attach: host.attach
            )
        )

        core.install(vtableStorage: storage)

        self.core = core
        self.storage = storage
        self.host = host
    }

    deinit {
        EndpointHosts.release(host, core: core)
        storage.deinitialize(count: 1)
        storage.deallocate()
    }

    public func exportedVtable() -> UnsafePointer<VoxLinkVtable> {
        core.exportedVtable()
    }

    public func connect(peer: UnsafePointer<VoxLinkVtable>) throws -> FfiLink {
        try core.connect(to: peer)
    }

    public func accept() async throws -> FfiLink {
        try await core.accept()
    }

    func outstandingLoanCount() -> Int {
        core.outstandingLoanCount()
    }
}

/// r[impl link] - FFI callbacks provide a message-oriented bidirectional link.
/// r[impl link.message] - Each callback-delivered payload stays separate.
public final class FfiLink: Link, @unchecked Sendable {
    private let core: EndpointCore
    private let frameLimit = FrameLimit(Int.max)

    fileprivate init(core: EndpointCore) {
        self.core = core
    }

    public func sendFrame(_ bytes: [UInt8]) async throws {
        try core.send(bytes)
    }

    /// r[impl link.rx.recv] - Swift copies bytes before calling the peer free callback.
    public func recvFrame() async throws -> [UInt8]? {
        let frame = await core.nextFrame()
        defer { core.releaseIncoming(frame.ptr) }

        if frame.len > frameLimit.maxFrameBytes {
            throw TransportError.frameDecoding("ffi frame exceeded configured limit")
        }
        if frame.len == 0 {
            return []
        }
        guard let ptr = frame.ptr else {
            throw TransportError.protocolViolation("ffi frame pointer was null")
        }

        return Array(UnsafeBufferPointer(start: ptr, count: frame.len))
    }

    public func setMaxFrameSize(_ size: Int) async throws {
        frameLimit.maxFrameBytes = size
    }

    public func close() async throws {
    }
}
