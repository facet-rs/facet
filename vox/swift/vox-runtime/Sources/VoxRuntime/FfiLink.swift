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

private final class ActiveFfiEndpointState: @unchecked Sendable {
    static let shared = ActiveFfiEndpointState()

    let lock = NSLock()
    var core: EndpointCore?
}

private func activeEndpointCore() -> EndpointCore {
    let state = ActiveFfiEndpointState.shared
    state.lock.lock()
    defer { state.lock.unlock() }
    guard let core = state.core else {
        fatalError("FFI endpoint is not installed")
    }
    return core
}

private func voxFfiSendCallback(
    _ buf: UnsafePointer<UInt8>?,
    _ len: Int
) {
    activeEndpointCore().receive(buf, len: len)
}

private func voxFfiFreeCallback(
    _ buf: UnsafePointer<UInt8>?
) {
    activeEndpointCore().free(buf)
}

private func voxFfiAttachCallback(
    _ peer: UnsafeRawPointer?
) {
    guard let peer else {
        return
    }
    activeEndpointCore().attach(peer: peer.assumingMemoryBound(to: VoxLinkVtable.self))
}

public final class FfiEndpoint: @unchecked Sendable {
    private let core: EndpointCore
    private let storage: UnsafeMutablePointer<VoxLinkVtable>

    public init() {
        let core = EndpointCore()
        let storage = UnsafeMutablePointer<VoxLinkVtable>.allocate(capacity: 1)
        storage.initialize(
            to: VoxLinkVtable(
                send: voxFfiSendCallback,
                free: voxFfiFreeCallback,
                attach: voxFfiAttachCallback
            )
        )

        let state = ActiveFfiEndpointState.shared
        state.lock.lock()
        precondition(state.core == nil, "only one FFI endpoint may be installed")
        state.core = core
        state.lock.unlock()

        core.install(vtableStorage: storage)

        self.core = core
        self.storage = storage
    }

    deinit {
        let state = ActiveFfiEndpointState.shared
        state.lock.lock()
        if state.core === core {
            state.core = nil
        }
        state.lock.unlock()

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
