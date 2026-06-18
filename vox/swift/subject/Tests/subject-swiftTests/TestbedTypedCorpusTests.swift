// The exhaustive cross-shape sweep over the generated Testbed descriptors, through the
// SHARED equivalence harness (PhonEngineTestSupport). Each case proves the codegen-emitted
// args/response descriptor lowers and that tree-walk == interpreter (== JIT, once added to
// `allTypedEngines` in one place). Covers scalar / string / bytes / option / struct / tuple /
// enum (unit, newtype, tuple, struct variants) / list / nested / Result<T, VoxError<E>>.

import Foundation
import PhonEngineTestSupport
import Testing

@testable import subject_swift

// MARK: - args (each method's args tuple descriptor)

@Test func corpusScalarArgs() throws {
    try assertTypedEquivalence(UInt64(0xDEAD_BEEF_0BAD_F00D), descriptor: testbed_echoU64_ArgsDescriptor, registry: testbedRegistry, "echoU64")
    try assertTypedEquivalence(true, descriptor: testbed_echoBool_ArgsDescriptor, registry: testbedRegistry, "echoBool")
}

@Test func corpusStringBytesArgs() throws {
    try assertTypedEquivalence("hello, phon 🌀", descriptor: testbed_echo_ArgsDescriptor, registry: testbedRegistry, "echo")
    try assertTypedEquivalence(Data([0, 1, 2, 250, 255]), descriptor: testbed_echoBytes_ArgsDescriptor, registry: testbedRegistry, "echoBytes")
}

@Test func corpusOptionArgs() throws {
    try assertTypedEquivalence(Optional("present"), descriptor: testbed_echoOptionString_ArgsDescriptor, registry: testbedRegistry, "optionString-some")
    try assertTypedEquivalence(Optional<String>.none, descriptor: testbed_echoOptionString_ArgsDescriptor, registry: testbedRegistry, "optionString-none")
}

@Test func corpusTupleArgs() throws {
    try assertTypedEquivalence((Int64(42), Int64(-7)), descriptor: testbed_divide_ArgsDescriptor, registry: testbedRegistry, "divide-args")
}

@Test func corpusStructArgs() throws {
    try assertTypedEquivalence(Point(x: -3, y: 9), descriptor: testbed_echoPoint_ArgsDescriptor, registry: testbedRegistry, "point")
    try assertTypedEquivalence(Measurement(unit: "meters", value: 1.5), descriptor: testbed_echoMeasurement_ArgsDescriptor, registry: testbedRegistry, "measurement")
    try assertTypedEquivalence(Record(alpha: 1, beta: "two", gamma: 3.0), descriptor: testbed_echoRecord_ArgsDescriptor, registry: testbedRegistry, "record")
}

@Test func corpusEnumUnitVariants() throws {
    try assertTypedEquivalence(Status.active, descriptor: testbed_echoStatus_ArgsDescriptor, registry: testbedRegistry, "status-active")
    try assertTypedEquivalence(Status.inactive, descriptor: testbed_echoStatus_ArgsDescriptor, registry: testbedRegistry, "status-inactive")
}

@Test func corpusEnumNewtypeVariants() throws {
    try assertTypedEquivalence(Message.text("hi"), descriptor: testbed_processMessage_ArgsDescriptor, registry: testbedRegistry, "msg-text")
    try assertTypedEquivalence(Message.number(-99), descriptor: testbed_processMessage_ArgsDescriptor, registry: testbedRegistry, "msg-number")
    try assertTypedEquivalence(Message.data(Data([9, 8, 7])), descriptor: testbed_processMessage_ArgsDescriptor, registry: testbedRegistry, "msg-data")
}

@Test func corpusEnumStructVariants() throws {
    try assertTypedEquivalence(Shape.circle(radius: 2.5), descriptor: testbed_echoShape_ArgsDescriptor, registry: testbedRegistry, "shape-circle")
    try assertTypedEquivalence(Shape.rectangle(width: 4.0, height: 6.0), descriptor: testbed_echoShape_ArgsDescriptor, registry: testbedRegistry, "shape-rect")
    try assertTypedEquivalence(Shape.point, descriptor: testbed_echoShape_ArgsDescriptor, registry: testbedRegistry, "shape-point")
}

@Test func corpusNestedGnarly() throws {
    let payload = GnarlyPayload(
        revision: 7,
        mount: "/mnt/data",
        entries: [
            GnarlyEntry(
                id: 1, parent: nil, name: "root", path: "/",
                attrs: [GnarlyAttr(key: "owner", value: "amos")],
                chunks: [Data([1, 2]), Data()],
                kind: .directory(childCount: 2, children: ["a", "b"])),
            GnarlyEntry(
                id: 2, parent: 1, name: "link", path: "/link",
                attrs: [],
                chunks: [],
                kind: .symlink(target: "/root", hops: [10, 20, 30])),
        ],
        footer: "end",
        digest: Data([0xAA, 0xBB, 0xCC]))
    try assertTypedEquivalence(payload, descriptor: testbed_echoGnarly_ArgsDescriptor, registry: testbedRegistry, "gnarly")
}

// MARK: - responses (Result<T, VoxError<E>>)

@Test func corpusResponses() throws {
    try assertTypedEquivalence(
        Result<String, VoxError<Infallible>>.success("ok"),
        descriptor: testbed_echo_ResponseDescriptor, registry: testbedRegistry, "echo-resp")

    try assertTypedEquivalence(
        Result<Int64, VoxError<MathError>>.success(6),
        descriptor: testbed_divide_ResponseDescriptor, registry: testbedRegistry, "divide-ok")
    try assertTypedEquivalence(
        Result<Int64, VoxError<MathError>>.failure(.user(.divisionByZero)),
        descriptor: testbed_divide_ResponseDescriptor, registry: testbedRegistry, "divide-user-err")
    try assertTypedEquivalence(
        Result<Int64, VoxError<MathError>>.failure(.cancelled),
        descriptor: testbed_divide_ResponseDescriptor, registry: testbedRegistry, "divide-vox-err")

    try assertTypedEquivalence(
        Result<Point, VoxError<Infallible>>.success(Point(x: 1, y: 2)),
        descriptor: testbed_echoPoint_ResponseDescriptor, registry: testbedRegistry, "point-resp")
}
