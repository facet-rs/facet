// The IR interpreter: run a lowered `Program` against a reader to produce a
// `Value`. The reference semantics a JIT must match exactly.
//
// A small stack machine. Leaf ops decode from the wire and push a value;
// container ops pop their children's values and push the assembled one. The
// invariant holds throughout: running a complete lowered subtree nets exactly one
// value on the stack, so the whole program leaves a single result.
//
// The flat counterpart to `exec` (which walks the `Node` tree recursively). The
// two must agree value-for-value.
//
// Mirrors `rust/phon-engine/src/interp.rs` byte-for-byte.

import PhonIR
import PhonSchema

/// Run a lowered program against `bytes`, producing the decoded value and
/// rejecting trailing bytes.
public func run(_ program: Program, _ bytes: [UInt8], _ reg: Registry) throws -> Value {
    try run(ValueProgram(program: program), bytes, reg)
}

/// Run a lowered dynamic-value program with its recursive block registry.
public func run(_ lowered: ValueProgram, _ bytes: [UInt8], _ reg: Registry) throws -> Value {
    var r = Reader(bytes)
    var stack: [Value] = []
    try execOps(lowered.program, &r, reg, lowered.blocks, &stack)
    if r.remaining != 0 {
        throw CompactError.decode(.trailingBytes(r.remaining))
    }
    guard let v = stack.popLast() else {
        throw CompactError.decode(.malformed("program produced no value"))
    }
    return v
}

private func execOps(
    _ ops: Program,
    _ r: inout Reader,
    _ reg: Registry,
    _ blocks: [SchemaId: Program],
    _ stack: inout [Value]
) throws {
    for op in ops { try execOp(op, &r, reg, blocks, &stack) }
}

private func execOp(
    _ op: Op,
    _ r: inout Reader,
    _ reg: Registry,
    _ blocks: [SchemaId: Program],
    _ stack: inout [Value]
) throws {
    switch op {
    case .scalar(let p):
        stack.append(try decodePrimitive(&r, p))
    case .dynamic:
        stack.append(try readValue(&r))
    case .callBlock(let schema):
        guard let block = blocks[schema] else {
            throw CompactError.decode(.malformed("missing recursion block"))
        }
        try execOps(block, &r, reg, blocks, &stack)
    case .null:
        stack.append(.null)
    case .skip(let writerRef):
        // Walk the writer-only field by its own schema and drop it.
        _ = try decodeRef(&r, writerRef, reg, 0)
    case .object(let keys):
        let vals = Array(stack.suffix(keys.count))
        stack.removeLast(keys.count)
        var obj: [Value.Entry] = []
        obj.reserveCapacity(keys.count)
        for (k, v) in zip(keys, vals) { obj.append(Value.Entry(key: k, value: v)) }
        stack.append(.object(obj))
    case .array(let count):
        let vals = Array(stack.suffix(count))
        stack.removeLast(count)
        stack.append(.array(vals))
    case .seq(let set, let minWire, let body):
        let n = try r.readLen(minElemSize: minWire)
        var arr: [Value] = []
        var seen: Set<Value> = []
        for _ in 0..<n {
            try execOps(body, &r, reg, blocks, &stack)
            let v = stack.removeLast()
            if set {
                guard seen.insert(v).inserted else { throw CompactError.decode(.duplicateElement) }
            }
            arr.append(v)
        }
        stack.append(.array(arr))
    case .map(let key, let value):
        let n = try r.readLen(minElemSize: 1)
        var obj: [Value.Entry] = []
        var seen: Set<String> = []
        for _ in 0..<n {
            try execOps(key, &r, reg, blocks, &stack)
            let k = stack.removeLast()
            try execOps(value, &r, reg, blocks, &stack)
            let v = stack.removeLast()
            guard let ks = k.asString else { throw CompactError.unsupported("map with non-string keys") }
            guard seen.insert(ks).inserted else { throw CompactError.decode(.duplicateKey) }
            obj.append(Value.Entry(key: ks, value: v))
        }
        stack.append(.object(obj))
    case .fixedArray(let dimensions, let minWire, let body):
        let count = try product(dimensions)
        try checkFixedCount(count, minWire, r.remaining)
        var arr: [Value] = []
        for _ in 0..<count {
            try execOps(body, &r, reg, blocks, &stack)
            arr.append(stack.removeLast())
        }
        stack.append(.array(arr))
    case .option(let some):
        switch try r.readU8() {
        case 0: stack.append(.null)
        case 1: try execOps(some, &r, reg, blocks, &stack)
        case let b: throw CompactError.decode(.invalidBool(b))
        }
    case .enumeration(let arms):
        let idx = try r.readU32()
        guard let arm = arms.first(where: { $0.writerIndex == idx }) else {
            throw CompactError.writerOnlyVariant(idx)
        }
        try execOps(arm.payload, &r, reg, blocks, &stack)
        let payload = stack.removeLast()
        stack.append(.object([Value.Entry(key: arm.readerName, value: payload)]))
    }
}
