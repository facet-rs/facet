import PhonEngine
import PhonIR
import Testing
import VoxRuntime

@testable import subject_swift

// Validates the generated service typed path at runtime. Decode goes through the ONE
// reconciling path — `lowerDecode(writer → reader)` built from the writer's advertised
// schema closure, exactly as the runtime does (here writer ≡ reader, the degenerate
// fused identity). There is no cached same-schema decode program.

private func decodeProgram(
    _ methodId: UInt64, _ dir: SchemaBindingDirection, _ closure: [UInt8], _ reader: Descriptor
) throws -> Lowered {
    let tracker = SchemaTracker()
    tracker.recordReceived(methodId, dir, closure)
    guard let p = tracker.buildDecodeProgram(methodId, dir, readerDescriptor: reader, local: testbedRegistry)
    else { fatalError("no writer schema recorded") }
    return p
}

private let echoId: UInt64 = 0x880b_c4ee_e235_74be

@Test func echoArgsRoundTrip() throws {
    var args = "hello world"
    let payload = withUnsafeBytes(of: &args) {
        encodeWith(testbed_echo_ArgsEncodeProgram, $0.baseAddress!)
    }
    let program = try decodeProgram(
        echoId, .args, testbedMethods[echoId]!.argsSchemaClosure, testbed_echo_ArgsDescriptor)
    let raw = UnsafeMutableRawPointer.allocate(
        byteCount: MemoryLayout<String>.size, alignment: MemoryLayout<String>.alignment)
    defer { raw.deallocate() }
    try decodeInto(program, payload, raw)
    #expect(raw.assumingMemoryBound(to: String.self).move() == "hello world")
}

@Test func echoResponseRoundTrip() throws {
    var resp: Result<String, VoxError<Infallible>> = .success("HELLO WORLD")
    let payload = withUnsafeBytes(of: &resp) {
        encodeWith(testbed_echo_ResponseEncodeProgram, $0.baseAddress!)
    }
    let program = try decodeProgram(
        echoId, .response, testbedMethods[echoId]!.responseSchemaClosure,
        testbed_echo_ResponseDescriptor)
    let raw = UnsafeMutableRawPointer.allocate(
        byteCount: MemoryLayout<Result<String, VoxError<Infallible>>>.size,
        alignment: MemoryLayout<Result<String, VoxError<Infallible>>>.alignment)
    defer { raw.deallocate() }
    try decodeInto(program, payload, raw)
    guard case .success(let v) = raw.assumingMemoryBound(to: Result<String, VoxError<Infallible>>.self).move()
    else {
        Issue.record("expected .success")
        return
    }
    #expect(v == "HELLO WORLD")
}
