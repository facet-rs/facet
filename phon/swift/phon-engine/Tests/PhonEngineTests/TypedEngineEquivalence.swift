// A small corpus exercising several Access/MemOp shapes through the shared
// `assertTypedEquivalence` harness (PhonEngineTestSupport). Each case is ~1 line; the
// harness runs the tree-walk oracle + every engine. The JIT, once added to the harness's
// engine list, covers all of these with no changes here.

import Testing

import PhonEngineTestSupport
import PhonIR
import PhonSchema

@testable import PhonEngine

private func u32Field(_ name: String) -> Field {
    Field(name: name, schema: .concrete(primitiveId(.u32)), required: true)
}
private func u32Desc() -> Descriptor {
    Descriptor(schema: .concrete(primitiveId(.u32)), layout: Layout(size: 4, align: 4), access: .scalar)
}

private struct Pair: Equatable {
    var a: UInt32
    var b: UInt32
}

@Test func equivalenceRecord() throws {
    let schema = Schema(id: SchemaId(1), kind: .structure(name: "Pair", fields: [u32Field("a"), u32Field("b")]))
    let reg = Registry([schema])
    let desc = Descriptor(
        schema: .concrete(SchemaId(1)), layout: MemoryLayout<Pair>.phonLayout,
        access: .record(RecordAccess(
            fields: [
                FieldAccess(offset: MemoryLayout<Pair>.offset(of: \Pair.a)!, descriptor: u32Desc()),
                FieldAccess(offset: MemoryLayout<Pair>.offset(of: \Pair.b)!, descriptor: u32Desc()),
            ], construct: .inPlace)))
    // r[verify ir.stencils]
    // r[verify descriptors.fact-driven]
    // r[verify ir.two-forms]
    // r[verify exec.jit-optional]
    try assertTypedEquivalence(Pair(a: 7, b: 99), descriptor: desc, registry: reg, "record")
}

private struct OptHolder: Equatable { var v: UInt32? }

@Test func equivalenceOption() throws {
    let batch = resolveIds([
        Schema(id: SchemaId(1), kind: .structure(name: "H", fields: [
            Field(name: "v", schema: .concrete(SchemaId(2)), required: false),
        ])),
        Schema(id: SchemaId(2), kind: .option(element: .concrete(primitiveId(.u32)))),
    ])
    let reg = Registry(batch)
    let optDesc = Descriptor(
        schema: .concrete(batch[1].id),
        layout: Layout(size: MemoryLayout<UInt32?>.size, align: MemoryLayout<UInt32?>.alignment),
        access: .option(OptionAccess(witness: .of(UInt32.self), some: u32Desc())))
    let desc = Descriptor(
        schema: .concrete(batch[0].id), layout: MemoryLayout<OptHolder>.phonLayout,
        access: .record(RecordAccess(
            fields: [FieldAccess(offset: MemoryLayout<OptHolder>.offset(of: \OptHolder.v)!, descriptor: optDesc)],
            construct: .inPlace)))
    try assertTypedEquivalence(OptHolder(v: 42), descriptor: desc, registry: reg, "option-some")
    try assertTypedEquivalence(OptHolder(v: nil), descriptor: desc, registry: reg, "option-none")
}
