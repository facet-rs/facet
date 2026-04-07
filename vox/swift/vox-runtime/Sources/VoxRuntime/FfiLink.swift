import Foundation

// ---------------------------------------------------------------------------
// C types matching vox-ffi's bridge.rs
// ---------------------------------------------------------------------------

/// Release callback: called when the receiver is done with a loaned buffer.
public typealias VoxFfiReleaseFn = @convention(c) (UnsafeMutableRawPointer?) -> Void

/// Receive callback: delivers a frame to the receiver.
public typealias VoxFfiRecvFn = @convention(c) (
    _ ctx: UnsafeMutableRawPointer?,
    _ buf: UnsafePointer<UInt8>?,
    _ len: Int,
    _ release: VoxFfiReleaseFn?,
    _ releaseCtx: UnsafeMutableRawPointer?
) -> Void

/// Drop callback: called when the peer closes permanently.
public typealias VoxFfiDropFn = @convention(c) (UnsafeMutableRawPointer?) -> Void

/// C-ABI vtable for one direction of an FFI link.
public struct VoxFfiVtable {
    public var ctx: UnsafeMutableRawPointer?
    public var recv_fn: VoxFfiRecvFn?
    public var drop_fn: VoxFfiDropFn?
}

// ---------------------------------------------------------------------------
// Imports from vox-ffi Rust bridge (linked via bee-ffi static lib)
// ---------------------------------------------------------------------------

/// Create a bridge link. Swift passes its receive vtable; Rust returns a handle.
@_silgen_name("vox_ffi_link_create")
func vox_ffi_link_create(_ swiftVtable: VoxFfiVtable) -> OpaquePointer

/// Get the vtable Swift uses to send frames into Rust.
@_silgen_name("vox_ffi_link_rust_vtable")
func vox_ffi_link_rust_vtable(_ handle: OpaquePointer) -> VoxFfiVtable

/// Extract the Rust-side vox Link from the handle (one-shot).
@_silgen_name("vox_ffi_link_take_link")
func vox_ffi_link_take_link(_ handle: OpaquePointer) -> OpaquePointer?

/// Destroy the bridge handle.
@_silgen_name("vox_ffi_link_destroy")
func vox_ffi_link_destroy(_ handle: OpaquePointer)

// ---------------------------------------------------------------------------
// FfiLink — Swift Link backed by vox-ffi bridge
// ---------------------------------------------------------------------------

/// In-process Link that communicates with a Rust vox peer via C-ABI vtables.
///
/// Usage:
/// ```swift
/// let (swiftLink, rustLinkPtr) = FfiLink.create()
/// // Pass rustLinkPtr to Rust for use with a vox session
/// // Use swiftLink as a normal Link for the Swift-side session
/// ```
public final class FfiLink: Link, @unchecked Sendable {
    /// The bridge handle (owns the Swift vtable + Rust rx mailbox)
    private let handle: OpaquePointer
    /// Vtable for sending frames Swift → Rust
    private let rustVtable: VoxFfiVtable
    /// Inbound frames from Rust → Swift
    private var inboundIterator: AsyncStream<[UInt8]>.Iterator
    /// The continuation Rust calls into (via recv_fn callback)
    private let inboundContinuation: AsyncStream<[UInt8]>.Continuation

    /// The context object that the C recv_fn callback captures.
    /// Must be allocated on the heap so the pointer remains stable.
    private let callbackCtx: FfiLinkCallbackCtx

    private init(
        handle: OpaquePointer,
        rustVtable: VoxFfiVtable,
        inboundStream: AsyncStream<[UInt8]>,
        inboundContinuation: AsyncStream<[UInt8]>.Continuation,
        callbackCtx: FfiLinkCallbackCtx
    ) {
        self.handle = handle
        self.rustVtable = rustVtable
        self.inboundIterator = inboundStream.makeAsyncIterator()
        self.inboundContinuation = inboundContinuation
        self.callbackCtx = callbackCtx
    }

    deinit {
        inboundContinuation.finish()
        vox_ffi_link_destroy(handle)
    }

    /// Create a linked pair: a Swift-side FfiLink and an opaque pointer to
    /// the Rust-side BridgeLink (for use with vox sessions on the Rust side).
    ///
    /// The caller must pass `rustLink` to Rust code that will use it as a
    /// `vox_types::Link`. Rust takes ownership of that pointer.
    public static func create() -> (swiftLink: FfiLink, rustLink: OpaquePointer) {
        // 1. Create the Swift receive side
        let (stream, continuation) = AsyncStream<[UInt8]>.makeStream()
        let ctx = FfiLinkCallbackCtx(continuation: continuation)
        let ctxPtr = Unmanaged.passRetained(ctx).toOpaque()

        // 2. Build the Swift vtable (Rust→Swift delivery)
        let swiftVtable = VoxFfiVtable(
            ctx: ctxPtr,
            recv_fn: ffiLinkRecvCallback,
            drop_fn: ffiLinkDropCallback
        )

        // 3. Create the bridge handle (passing Swift vtable to Rust)
        let handle = vox_ffi_link_create(swiftVtable)

        // 4. Get the Rust vtable (Swift→Rust delivery)
        let rustVtable = vox_ffi_link_rust_vtable(handle)

        // 5. Take the Rust-side Link
        guard let rustLink = vox_ffi_link_take_link(handle) else {
            fatalError("vox_ffi_link_take_link returned null")
        }

        let link = FfiLink(
            handle: handle,
            rustVtable: rustVtable,
            inboundStream: stream,
            inboundContinuation: continuation,
            callbackCtx: ctx
        )

        return (link, rustLink)
    }

    // MARK: - Link protocol

    public func sendFrame(_ bytes: [UInt8]) async throws {
        bytes.withUnsafeBufferPointer { buf in
            guard let baseAddress = buf.baseAddress else { return }
            rustVtable.recv_fn?(
                rustVtable.ctx,
                baseAddress,
                buf.count,
                noopRelease,
                nil
            )
        }
    }

    public func recvFrame() async throws -> [UInt8]? {
        await inboundIterator.next()
    }

    public func setMaxFrameSize(_ size: Int) async throws {
        // No frame size limit for in-process link
    }

    public func close() async throws {
        inboundContinuation.finish()
    }
}

// ---------------------------------------------------------------------------
// Callback context + C function pointers
// ---------------------------------------------------------------------------

/// Heap-allocated context captured by the C callbacks.
final class FfiLinkCallbackCtx {
    let continuation: AsyncStream<[UInt8]>.Continuation

    init(continuation: AsyncStream<[UInt8]>.Continuation) {
        self.continuation = continuation
    }
}

/// C callback: Rust delivers a frame to Swift.
///
/// Copies the bytes into a Swift array, then immediately calls release.
private func ffiLinkRecvCallback(
    ctx: UnsafeMutableRawPointer?,
    buf: UnsafePointer<UInt8>?,
    len: Int,
    release: VoxFfiReleaseFn?,
    releaseCtx: UnsafeMutableRawPointer?
) {
    guard let ctx, let buf else { return }
    let callbackCtx = Unmanaged<FfiLinkCallbackCtx>.fromOpaque(ctx).takeUnretainedValue()
    let bytes = Array(UnsafeBufferPointer(start: buf, count: len))
    // Release the Rust buffer immediately since we copied
    release?(releaseCtx)
    callbackCtx.continuation.yield(bytes)
}

/// C callback: Rust is closing its send direction.
private func ffiLinkDropCallback(ctx: UnsafeMutableRawPointer?) {
    guard let ctx else { return }
    let callbackCtx = Unmanaged<FfiLinkCallbackCtx>.fromOpaque(ctx).takeRetainedValue()
    callbackCtx.continuation.finish()
}

/// No-op release for Swift→Rust sends (Rust copies the data).
private func noopRelease(_ ctx: UnsafeMutableRawPointer?) {}
