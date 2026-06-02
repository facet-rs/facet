// The typed-path recursion oracle: a self-recursive type (a tree whose children
// are more trees) must lower to FINITE blocks — `Access.recurse` descriptor
// back-edges become `MemOp.callBlock`, resolved against `Lowered.blocks` — and
// encode/decode byte-identically to the dynamic compact/Value codec, then
// round-trip. Mirrors Rust's `derived_recursive_typed_matches_dynamic_and_roundtrips`
// (`r[ir.recursion]`). Where Rust derives the descriptor + blocks from a `Shape`,
// the Swift side hand-builds them exactly as codegen will emit them.

import Testing

@testable import PhonEngine
import PhonIR
import PhonSchema

private struct Tree: Equatable {
    var value: UInt32
    var children: [Tree]
}

// The recursive value as a nested phon Value — the compact-codec oracle. The
// dynamic path is value-driven, so it walks the finite value and terminates even
// though the schema is cyclic.
private func treeValue(_ t: Tree) -> Value {
    .object([
        .init(key: "value", value: .number(.canonical(unsigned: UInt128(t.value)))),
        .init(key: "children", value: .array(t.children.map { treeValue($0) })),
    ])
}

private func scalarDesc(_ p: Primitive) -> Descriptor {
    let size = fixedSize(p)!
    return Descriptor(
        schema: .concrete(primitiveId(p)),
        layout: Layout(size: size, align: alignment(p)),
        access: .scalar
    )
}

@Test
func typedRecursiveTreeMatchesValueOracleAndRoundTrips() throws {
    // The cycle: Tree { value: u32, children: VecTree } and VecTree = [Tree]. Both
    // schemas are cyclic, so both lower to callable blocks rather than inline.
    let treeId = SchemaId(1)
    let vecId = SchemaId(2)
    let tree = Schema(id: treeId, kind: .structure(name: "Tree", fields: [
        Field(name: "value", schema: .concrete(primitiveId(.u32)), required: true),
        Field(name: "children", schema: .concrete(vecId), required: true),
    ]))
    let vec = Schema(id: vecId, kind: .list(element: .concrete(treeId)))
    let reg = Registry([tree, vec])

    let treeLayout = Layout(size: MemoryLayout<Tree>.size, align: MemoryLayout<Tree>.alignment)
    let vecLayout = Layout(size: MemoryLayout<[Tree]>.size, align: MemoryLayout<[Tree]>.alignment)

    // Recurse stand-ins (the descriptor back-edges) for each cyclic schema, mirroring
    // what the Rust derive (and the Swift codegen) splice in at a cyclic position.
    let recurseTree = Descriptor(schema: .concrete(treeId), layout: treeLayout, access: .recurse)
    let recurseVec = Descriptor(schema: .concrete(vecId), layout: vecLayout, access: .recurse)

    // The block bodies, keyed by schema id — what codegen emits as `descriptorBlocks`.
    let treeBody = Descriptor(
        schema: .concrete(treeId), layout: treeLayout,
        access: .record(RecordAccess(fields: [
            FieldAccess(offset: MemoryLayout<Tree>.offset(of: \Tree.value)!, descriptor: scalarDesc(.u32)),
            FieldAccess(offset: MemoryLayout<Tree>.offset(of: \Tree.children)!, descriptor: recurseVec),
        ], construct: .inPlace)))
    let vecBody = Descriptor(
        schema: .concrete(vecId), layout: vecLayout,
        access: .sequence(SequenceAccess(
            element: recurseTree,
            stride: MemoryLayout<Tree>.stride,
            elemAlign: MemoryLayout<Tree>.alignment,
            witness: SeqWitness.of(Tree.self))))
    let blocks: [SchemaId: Descriptor] = [treeId: treeBody, vecId: vecBody]

    // The root descriptor of a recursive type is itself a Recurse stand-in.
    let program = try lowerTyped(recurseTree, reg, blocks)
    #expect(!program.blocks.isEmpty, "a recursive type must lower to at least one block")

    let t = Tree(value: 1, children: [
        Tree(value: 2, children: []),
        Tree(value: 3, children: [Tree(value: 4, children: [])]),
    ])

    let typedBytes = withUnsafeBytes(of: t) { encodeWith(program, $0.baseAddress!) }

    // Oracle: byte-identical to the dynamic codec for the equivalent nested object.
    let oracleBytes = try encode(treeValue(t), treeId, reg)
    #expect(typedBytes == oracleBytes, "typed recursion bytes must match the dynamic oracle")

    // Round-trip the whole tree back (decode into fresh storage, then move out so the
    // managed `[Tree]` children are not double-initialized).
    let raw = UnsafeMutableRawPointer.allocate(
        byteCount: MemoryLayout<Tree>.size, alignment: MemoryLayout<Tree>.alignment)
    defer { raw.deallocate() }
    try decodeInto(program, typedBytes, raw)
    let decoded = raw.assumingMemoryBound(to: Tree.self).move()
    #expect(decoded == t, "typed recursion did not round-trip")

    // The RECONCILING decode path (lowerDecode — what vox's RPC args/response decode
    // through) must also handle the cyclic reader. Same-schema here (writer == reader).
    let decProgram = try lowerDecode(treeId, recurseTree, reg, blocks)
    #expect(!decProgram.blocks.isEmpty, "a recursive reconciling decode must build blocks")
    let raw2 = UnsafeMutableRawPointer.allocate(
        byteCount: MemoryLayout<Tree>.size, alignment: MemoryLayout<Tree>.alignment)
    defer { raw2.deallocate() }
    try decodeInto(decProgram, typedBytes, raw2)
    let reconciled = raw2.assumingMemoryBound(to: Tree.self).move()
    #expect(reconciled == t, "reconciling recursion did not round-trip")
}
