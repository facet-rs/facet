// Compatibility planning: translate a writer schema with a reader schema into a
// plan, then decode the writer's compact bytes into a reader-shaped
// `Value`.
//
// The plan is built from the two schemas alone, before any payload is touched: if
// it cannot be built the schemas are incompatible and decoding never begins.
// Struct fields are matched by name (writer-only fields skipped, reader-only
// fields defaulted or — when required — failing the plan). Enum variants are
// matched by name. Types match only by the type-match rules — no implicit numeric
// widening.
//
// This is the dynamic-`Value` path: reader-only fields default to `null`.
//
// Mirrors `rust/phon-engine/src/plan.rs` byte-for-byte.

import PhonIR
import PhonSchema

private let planMaxDepth = 128

// MARK: - Plan tree

/// A built translation plan from a writer schema to a reader schema.
public struct Plan {
    let root: Node
}

public enum CompatDirection: Equatable {
    /// The newer schema can read bytes written by the older schema.
    case backward
    /// The older schema can read bytes written by the newer schema.
    case forward
    /// Both schema versions can read each other's bytes.
    case bidirectional
    /// Neither schema version can read the other's bytes.
    case incompatible
}

indirect enum Node {
    case scalar(Primitive)
    case structure(StructPlan)
    /// Writer variant index -> how to produce the reader's variant. An index
    /// absent here is a writer-only variant: a decode error if it arrives.
    case enumeration([UInt32: VariantPlan])
    case tuple([Node])
    /// A `list` (`set == false`) or `set` (`set == true`). `minWire` is the
    /// element's minimum wire size for the count guard.
    case seq(set: Bool, element: Node, minWire: Int)
    case map(key: Node, value: Node)
    case array(element: Node, dims: [UInt64], minWire: Int)
    case option(Node)
    case dynamic
}

struct StructPlan {
    /// One step per writer field, in wire order.
    var steps: [Step]
    /// Reader-only, non-required field names to fill with a default.
    var defaults: [String]
}

enum Step {
    /// A writer field matched to this reader field; decode it with `node`.
    case take(reader: String, node: Node)
    /// A writer-only field: decode it by its writer schema and discard.
    case skip(SchemaRef)
}

struct VariantPlan {
    var reader: String
    var payload: Payload
}

enum Payload {
    case unit
    case newtype(Node)
    case tuple([Node])
    case structure(StructPlan)
}

// MARK: - Public API

/// Build the translation plan from `writerRoot` to `readerRoot`.
// r[impl compat.plan-first]
public func buildPlan(_ writerRoot: SchemaId, _ readerRoot: SchemaId, _ reg: Registry) throws -> Plan {
    let node = try planRef(
        .concrete(id: writerRoot, args: []),
        .concrete(id: readerRoot, args: []),
        reg, 0
    )
    return Plan(root: node)
}

/// Classify compatibility between an older and newer schema by planning both
/// directions. This is tooling over `buildPlan`, not a decode path.
// r[impl compat.direction]
public func compatDirection(_ olderRoot: SchemaId, _ newerRoot: SchemaId, _ reg: Registry) -> CompatDirection {
    let backward = (try? buildPlan(olderRoot, newerRoot, reg)) != nil
    let forward = (try? buildPlan(newerRoot, olderRoot, reg)) != nil
    switch (backward, forward) {
    case (true, true): return .bidirectional
    case (true, false): return .backward
    case (false, true): return .forward
    case (false, false): return .incompatible
    }
}

/// Decode writer compact `bytes` into a reader-shaped value using a prebuilt plan.
public func decodeWithPlan(_ bytes: [UInt8], _ plan: Plan, _ reg: Registry) throws -> Value {
    var r = Reader(bytes)
    let v = try exec(plan.root, &r, reg, 0)
    if r.remaining != 0 {
        throw CompactError.decode(.trailingBytes(r.remaining))
    }
    return v
}

/// Build a plan and decode in one step.
public func planDecode(_ bytes: [UInt8], _ writerRoot: SchemaId, _ readerRoot: SchemaId, _ reg: Registry) throws -> Value {
    let plan = try buildPlan(writerRoot, readerRoot, reg)
    return try decodeWithPlan(bytes, plan, reg)
}

/// Build a plan, lower it to the linear IR, and run the interpreter — the flat
/// counterpart to `planDecode`. The interpreter must produce the same value the
/// recursive `decodeWithPlan` would.
public func decodeViaIr(_ bytes: [UInt8], _ writerRoot: SchemaId, _ readerRoot: SchemaId, _ reg: Registry) throws -> Value {
    let plan = try buildPlan(writerRoot, readerRoot, reg)
    let program = lower(plan)
    return try run(program, bytes, reg)
}

// MARK: - Building the plan

private func incompatible(_ why: String) -> CompactError { .incompatible(why) }

private func planRef(_ w: SchemaRef, _ r: SchemaRef, _ reg: Registry, _ depth: Int) throws -> Node {
    if depth > planMaxDepth { throw incompatible("schema nests too deep") }
    return try planResolved(try resolve(reg, w), try resolve(reg, r), reg, depth)
}

private func planResolved(_ w: Resolved, _ r: Resolved, _ reg: Registry, _ depth: Int) throws -> Node {
    switch (w, r) {
    case (.primitive(let wp), .primitive(let rp)):
        if wp == rp { return .scalar(wp) }
        throw incompatible("primitive \(wp) is not \(rp)")
    case (.composite(let wk), .composite(let rk)):
        return try planKind(wk, rk, reg, depth)
    default:
        throw incompatible("primitive does not match composite")
    }
}

// r[impl compat.type-match]
private func planKind(_ wk: SchemaKind, _ rk: SchemaKind, _ reg: Registry, _ depth: Int) throws -> Node {
    switch (wk, rk) {
    case (.primitive(let wp), .primitive(let rp)):
        if wp == rp { return .scalar(wp) }
        throw incompatible("primitive \(wp) is not \(rp)")
    case (.structure(_, let wf), .structure(_, let rf)):
        return .structure(try planStruct(wf, rf, reg, depth))
    case (.enumeration(_, let wv), .enumeration(_, let rv)):
        return try planEnum(wv, rv, reg, depth)
    case (.tuple(let we), .tuple(let re)):
        if we.count != re.count { throw incompatible("tuple arity differs") }
        var nodes: [Node] = []
        for (w, r) in zip(we, re) { nodes.append(try planRef(w, r, reg, depth + 1)) }
        return .tuple(nodes)
    case (.list(let we), .list(let re)):
        return .seq(set: false, element: try planRef(we, re, reg, depth + 1), minWire: minWireSizeRef(reg, we))
    case (.set(let we), .set(let re)):
        return .seq(set: true, element: try planRef(we, re, reg, depth + 1), minWire: minWireSizeRef(reg, we))
    case (.option(let we), .option(let re)):
        return .option(try planRef(we, re, reg, depth + 1))
    case (.map(let wk2, let wv), .map(let rk2, let rv)):
        return .map(key: try planRef(wk2, rk2, reg, depth + 1), value: try planRef(wv, rv, reg, depth + 1))
    case (.array(let we, let wd), .array(let re, let rd)):
        if wd != rd { throw incompatible("array dimensions differ") }
        return .array(element: try planRef(we, re, reg, depth + 1), dims: wd, minWire: minWireSizeRef(reg, we))
    case (.dynamic, .dynamic):
        return .dynamic
    case (.tensor, .tensor):
        throw CompactError.unsupported("tensor")
    case (.channel, .channel):
        throw CompactError.unsupported("channel")
    case (.external, .external):
        throw CompactError.unsupported("external")
    default:
        throw incompatible("schema kinds differ")
    }
}

// r[impl compat.field-matching]
// r[impl compat.skip-writer-only]
// r[impl compat.reader-only-fields]
// r[impl compat.defaults-are-reader-side]
private func planStruct(_ wFields: [Field], _ rFields: [Field], _ reg: Registry, _ depth: Int) throws -> StructPlan {
    var readerByName: [String: Field] = [:]
    for f in rFields { readerByName[f.name] = f }

    var steps: [Step] = []
    var matched: Set<String> = []
    for wf in wFields {
        if let rf = readerByName[wf.name] {
            let node = try planRef(wf.schema, rf.schema, reg, depth + 1)
            steps.append(.take(reader: rf.name, node: node))
            matched.insert(rf.name)
        } else {
            steps.append(.skip(wf.schema))
        }
    }

    var defaults: [String] = []
    for rf in rFields where !matched.contains(rf.name) {
        if rf.required {
            throw incompatible("required reader field '\(rf.name)' is absent from the writer")
        }
        defaults.append(rf.name)
    }

    return StructPlan(steps: steps, defaults: defaults)
}

// r[impl compat.enum]
private func planEnum(_ wVariants: [Variant], _ rVariants: [Variant], _ reg: Registry, _ depth: Int) throws -> Node {
    var readerByName: [String: Variant] = [:]
    for v in rVariants { readerByName[v.name] = v }

    var byIndex: [UInt32: VariantPlan] = [:]
    for wv in wVariants {
        // A writer variant the reader lacks gets no entry: receiving it at runtime
        // is a decode error, but its absence here is fine.
        if let rv = readerByName[wv.name] {
            let payload = try planPayload(wv.payload, rv.payload, reg, depth)
            byIndex[wv.index] = VariantPlan(reader: rv.name, payload: payload)
        }
    }
    return .enumeration(byIndex)
}

private func planPayload(_ w: VariantPayload, _ r: VariantPayload, _ reg: Registry, _ depth: Int) throws -> Payload {
    switch (w, r) {
    case (.unit, .unit):
        return .unit
    case (.newtype(let wr), .newtype(let rr)):
        return .newtype(try planRef(wr, rr, reg, depth + 1))
    case (.tuple(let wrs), .tuple(let rrs)):
        if wrs.count != rrs.count { throw incompatible("variant tuple arity differs") }
        var nodes: [Node] = []
        for (w, r) in zip(wrs, rrs) { nodes.append(try planRef(w, r, reg, depth + 1)) }
        return .tuple(nodes)
    case (.structure(let wfs), .structure(let rfs)):
        return .structure(try planStruct(wfs, rfs, reg, depth))
    default:
        throw incompatible("variant payload shapes differ")
    }
}

// MARK: - Lowering the plan to the linear IR

/// Flatten a built plan's `Node` tree into a linear `Program`. Every
/// type-directed decision the tree encodes is resolved here, once; what the
/// interpreter runs carries only data-directed control flow.
public func lower(_ plan: Plan) -> Program {
    var out: Program = []
    lowerNode(plan.root, &out)
    return out
}

private func lowerSubtree(_ node: Node) -> Program {
    var out: Program = []
    lowerNode(node, &out)
    return out
}

private func lowerNode(_ node: Node, _ out: inout Program) {
    switch node {
    case .scalar(let p):
        out.append(.scalar(p))
    case .dynamic:
        out.append(.dynamic)
    case .structure(let sp):
        lowerStruct(sp, &out)
    case .enumeration(let byIndex):
        var arms: [EnumArm] = byIndex.map { (idx, vp) in
            EnumArm(writerIndex: idx, readerName: vp.reader, payload: lowerPayload(vp.payload))
        }
        // Deterministic order; the interpreter dispatches by writerIndex.
        arms.sort { $0.writerIndex < $1.writerIndex }
        out.append(.enumeration(arms: arms))
    case .tuple(let nodes):
        for n in nodes { lowerNode(n, &out) }
        out.append(.array(count: nodes.count))
    case .seq(let set, let element, let minWire):
        out.append(.seq(set: set, minWire: minWire, body: lowerSubtree(element)))
    case .map(let key, let value):
        out.append(.map(key: lowerSubtree(key), value: lowerSubtree(value)))
    case .array(let element, let dims, let minWire):
        out.append(.fixedArray(dimensions: dims, minWire: minWire, body: lowerSubtree(element)))
    case .option(let element):
        out.append(.option(some: lowerSubtree(element)))
    }
}

private func lowerStruct(_ sp: StructPlan, _ out: inout Program) {
    var keys: [String] = []
    for step in sp.steps {
        switch step {
        case .take(let reader, let node):
            lowerNode(node, &out)
            keys.append(reader)
        case .skip(let writerRef):
            out.append(.skip(writerRef))
        }
    }
    for name in sp.defaults {
        out.append(.null)
        keys.append(name)
    }
    out.append(.object(keys: keys))
}

private func lowerPayload(_ payload: Payload) -> Program {
    var out: Program = []
    switch payload {
    case .unit:
        out.append(.null)
    case .newtype(let node):
        lowerNode(node, &out)
    case .tuple(let nodes):
        for n in nodes { lowerNode(n, &out) }
        out.append(.array(count: nodes.count))
    case .structure(let sp):
        lowerStruct(sp, &out)
    }
    return out
}

// MARK: - Executing the plan (recursive)

private func exec(_ node: Node, _ r: inout Reader, _ reg: Registry, _ depth: Int) throws -> Value {
    if depth > planMaxDepth { throw CompactError.decode(.depthExceeded) }
    switch node {
    case .scalar(let p):
        return try decodePrimitive(&r, p)
    case .structure(let sp):
        return try execStruct(sp, &r, reg, depth)
    case .enumeration(let byIndex):
        let idx = try r.readU32()
        guard let v = byIndex[idx] else { throw CompactError.writerOnlyVariant(idx) }
        let payload = try execPayload(v.payload, &r, reg, depth)
        return .object([Value.Entry(key: v.reader, value: payload)])
    case .tuple(let nodes):
        var a: [Value] = []
        for n in nodes { a.append(try exec(n, &r, reg, depth + 1)) }
        return .array(a)
    case .seq(let set, let element, let minWire):
        let n = try r.readLen(minElemSize: minWire)
        var a: [Value] = []
        var seen: Set<Value> = []
        for _ in 0..<n {
            let v = try exec(element, &r, reg, depth + 1)
            if set {
                guard seen.insert(v).inserted else { throw CompactError.decode(.duplicateElement) }
            }
            a.append(v)
        }
        return .array(a)
    case .map(let key, let value):
        let n = try r.readLen(minElemSize: 1)
        var obj: [Value.Entry] = []
        var seen: Set<String> = []
        for _ in 0..<n {
            let k = try exec(key, &r, reg, depth + 1)
            let v = try exec(value, &r, reg, depth + 1)
            guard let ks = k.asString else { throw CompactError.unsupported("map with non-string keys") }
            guard seen.insert(ks).inserted else { throw CompactError.decode(.duplicateKey) }
            obj.append(Value.Entry(key: ks, value: v))
        }
        return .object(obj)
    case .array(let element, let dims, let minWire):
        let count = try product(dims)
        try checkFixedCount(count, minWire, r.remaining)
        var a: [Value] = []
        for _ in 0..<count { a.append(try exec(element, &r, reg, depth + 1)) }
        return .array(a)
    case .option(let element):
        switch try r.readU8() {
        case 0: return .null
        case 1: return try exec(element, &r, reg, depth + 1)
        case let b: throw CompactError.decode(.invalidBool(b))
        }
    case .dynamic:
        return try readValue(&r)
    }
}

private func execStruct(_ sp: StructPlan, _ r: inout Reader, _ reg: Registry, _ depth: Int) throws -> Value {
    var obj: [Value.Entry] = []
    for step in sp.steps {
        switch step {
        case .take(let reader, let node):
            let v = try exec(node, &r, reg, depth + 1)
            obj.append(Value.Entry(key: reader, value: v))
        case .skip(let writerRef):
            // Walk the writer field by its own schema and discard it.
            _ = try decodeRef(&r, writerRef, reg, depth + 1)
        }
    }
    for name in sp.defaults {
        obj.append(Value.Entry(key: name, value: .null))
    }
    return .object(obj)
}

private func execPayload(_ p: Payload, _ r: inout Reader, _ reg: Registry, _ depth: Int) throws -> Value {
    switch p {
    case .unit:
        return .null
    case .newtype(let n):
        return try exec(n, &r, reg, depth + 1)
    case .tuple(let ns):
        var a: [Value] = []
        for n in ns { a.append(try exec(n, &r, reg, depth + 1)) }
        return .array(a)
    case .structure(let sp):
        return try execStruct(sp, &r, reg, depth)
    }
}
