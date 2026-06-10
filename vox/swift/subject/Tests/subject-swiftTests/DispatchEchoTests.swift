import Foundation
import PhonEngine
import PhonIR
import PhonSchema
import Testing
import VoxRuntime

@testable import subject_swift

// Hermetic oracle for the server dispatch path: drive `TestbedDispatcher.dispatch`
// directly with an echo args payload + a recorded args schema (exactly the state the
// runtime sets up after receiving a Call), and inspect the emitted `.response`.
// This isolates the dispatch/handler/schema-advertise logic from the transport.

private final class ResponseBox: @unchecked Sendable {
    private let lock = NSLock()
    private var msg: TaskMessage?
    func set(_ m: TaskMessage) {
        lock.lock(); defer { lock.unlock() }
        if msg == nil { msg = m }
    }
    func get() -> TaskMessage? {
        lock.lock(); defer { lock.unlock() }
        return msg
    }
}

@Test func dispatchEchoEmitsResponseWithSchemas() async throws {
    let dispatcher = TestbedDispatcher(handler: TestbedService())
    let echoId: UInt64 = 0x880b_c4ee_e235_74be

    var args = "hello"
    let argsPayload = withUnsafeBytes(of: &args) {
        encodeWith(testbed_echo_ArgsEncodeProgram, $0.baseAddress!)
    }

    let recvTracker = SchemaTracker()
    recvTracker.recordReceived(echoId, .args, testbedMethods[echoId]!.argsSchemaClosure)
    let sendTracker = SchemaSendTracker()

    let box = ResponseBox()
    let taskTx: @Sendable (TaskMessage) -> Void = { msg in box.set(msg) }

    await dispatcher.dispatch(
        methodId: echoId, payload: argsPayload, requestId: 1, channels: [],
        registry: ChannelRegistry(), schemaSendTracker: sendTracker,
        schemaReceiveTracker: recvTracker, taskTx: taskTx)

    guard case .response(let requestId, let payload, let methodId, let schemas) = box.get() else {
        Issue.record("expected a .response TaskMessage")
        return
    }
    #expect(requestId == 1)
    #expect(methodId == echoId)
    #expect(!schemas.isEmpty, "dispatch produced an EMPTY response schema closure")

    // Decode the response payload to confirm it is Ok("hello"), not an error arm.
    let tracker = SchemaTracker()
    tracker.recordReceived(echoId, .response, testbedMethods[echoId]!.responseSchemaClosure)
    guard
        let program = tracker.buildDecodeProgram(
            echoId, .response, readerDescriptor: testbed_echo_ResponseDescriptor,
            local: testbedRegistry)
    else {
        Issue.record("no response schema recorded")
        return
    }
    let raw = UnsafeMutableRawPointer.allocate(
        byteCount: MemoryLayout<Result<String, VoxError<Infallible>>>.size,
        alignment: MemoryLayout<Result<String, VoxError<Infallible>>>.alignment)
    defer { raw.deallocate() }
    try decodeInto(program, payload, raw)
    let result = raw.assumingMemoryBound(to: Result<String, VoxError<Infallible>>.self).move()
    guard case .success(let v) = result else {
        Issue.record("expected .success(\"hello\"), got \(result)")
        return
    }
    #expect(v == "hello")
}
