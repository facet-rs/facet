// Content-derived schema identity.
//
// A `SchemaId` is the first 8 bytes (little-endian `u64`) of BLAKE3 over a
// schema's *canonical structural encoding*. The encoding is byte-exact and
// reproducible across implementations, so the same logical schema yields the
// same id everywhere with no coordination.
//
// Recursive schemas are handled by partitioning the reference graph into
// strongly-connected components, processing them dependencies-first, and — for a
// cyclic component — hashing each member via a structural unfolding with
// depth-indexed back-references that terminates the walk.
//
// Mirrors `rust/phon-schema/src/identity.rs` byte-for-byte.

// MARK: - SchemaId & primitive id

/// A content-derived type id: BLAKE3 of the canonical encoding, first 8 bytes LE.
// r[impl schema-identity.content-hash]
public struct SchemaId: Hashable, Sendable, CustomStringConvertible {
    public var raw: UInt64
    public init(_ raw: UInt64) { self.raw = raw }
    public var description: String { "0x" + String(raw, radix: 16) }
}

/// The canonical id of a primitive schema. Constant and universal — useful for
/// referencing primitives as already-resolved targets when building a batch.
// r[impl schema-identity.canonical-encoding]
// r[impl schema-identity.computation]
public func primitiveId(_ p: Primitive) -> SchemaId {
    var h = Blake3()
    h.writeStr(p.tag)
    return h.finalizeId()
}

// MARK: - Canonical encoding building blocks

private func writeTypeParams<S: Sink>(_ out: inout S, _ params: [String]) {
    out.writeU32(UInt32(params.count))
    for p in params {
        out.writeStr(p)
    }
}

// MARK: - Graph node index

/// An index into the batch being resolved. A newtype so a batch position can't
/// be confused with any other integer.
private struct NodeIx: Hashable {
    var raw: UInt32
    init(_ i: Int) { raw = UInt32(i) }
    var ix: Int { Int(raw) }
}

// MARK: - Reference graph

/// Visit every `SchemaRef.concrete` target reachable in a kind (including those
/// nested inside type arguments), calling `f` with each referenced id.
private func visitKindTargets(_ kind: SchemaKind, _ f: (SchemaId) -> Void) {
    switch kind {
    case .primitive, .dynamic:
        break
    case .structure(_, let fields):
        for field in fields { visitRefTargets(field.schema, f) }
    case .enumeration(_, let variants):
        for v in variants {
            switch v.payload {
            case .unit:
                break
            case .newtype(let r):
                visitRefTargets(r, f)
            case .tuple(let refs):
                for r in refs { visitRefTargets(r, f) }
            case .structure(let fields):
                for field in fields { visitRefTargets(field.schema, f) }
            }
        }
    case .tuple(let elements):
        for r in elements { visitRefTargets(r, f) }
    case .list(let element),
         .set(let element),
         .option(let element):
        visitRefTargets(element, f)
    case .array(let element, _),
         .tensor(let element, _),
         .channel(_, let element):
        visitRefTargets(element, f)
    case .map(let key, let value):
        visitRefTargets(key, f)
        visitRefTargets(value, f)
    case .external(_, let metadata):
        if let r = metadata { visitRefTargets(r, f) }
    }
}

private func visitRefTargets(_ r: SchemaRef, _ f: (SchemaId) -> Void) {
    if case .concrete(let id, let args) = r {
        f(id)
        for a in args { visitRefTargets(a, f) }
    }
}

// MARK: - Tarjan SCC (yields components dependencies-first)

private final class Tarjan {
    let adj: [[NodeIx]]
    var nextOrder = 0
    var order: [Int?]
    var lowlink: [Int]
    var onStack: [Bool]
    var stack: [NodeIx] = []
    var sccs: [[NodeIx]] = []

    private init(_ adj: [[NodeIx]]) {
        self.adj = adj
        let n = adj.count
        order = Array(repeating: nil, count: n)
        lowlink = Array(repeating: 0, count: n)
        onStack = Array(repeating: false, count: n)
    }

    static func run(_ adj: [[NodeIx]]) -> [[NodeIx]] {
        let t = Tarjan(adj)
        for v in 0..<adj.count where t.order[v] == nil {
            t.strongconnect(NodeIx(v))
        }
        // Components are popped when their root finishes, so dependencies (which
        // finish first) appear before dependents: dependencies-first order.
        return t.sccs
    }

    private func strongconnect(_ v: NodeIx) {
        order[v.ix] = nextOrder
        lowlink[v.ix] = nextOrder
        nextOrder += 1
        stack.append(v)
        onStack[v.ix] = true

        for w in adj[v.ix] {
            if order[w.ix] == nil {
                strongconnect(w)
                lowlink[v.ix] = min(lowlink[v.ix], lowlink[w.ix])
            } else if onStack[w.ix] {
                lowlink[v.ix] = min(lowlink[v.ix], order[w.ix]!)
            }
        }

        if lowlink[v.ix] == order[v.ix]! {
            var scc: [NodeIx] = []
            while true {
                let w = stack.removeLast()
                onStack[w.ix] = false
                scc.append(w)
                if w == v { break }
            }
            sccs.append(scc)
        }
    }
}

// MARK: - The walk

/// Context shared across a member's structural walk.
private struct Walk {
    let batch: [Schema]
    let keyToIndex: [UInt64: NodeIx]
    let component: Set<NodeIx>
    let assigned: [NodeIx: SchemaId]

    /// Walk schema `idx`'s kind, with `path` holding the component members from
    /// the root of this walk down to (and including) `idx`.
    func schema<S: Sink>(_ idx: NodeIx, _ path: [NodeIx], _ out: inout S) {
        let s = batch[idx.ix]
        switch s.kind {
        case .primitive(let p):
            out.writeStr(p.tag)
        case .structure(let name, let fields):
            out.writeStr("struct")
            out.writeStr(name)
            writeTypeParams(&out, s.typeParams)
            out.writeU32(UInt32(fields.count))
            for field in fields { self.field(field, path, &out) }
        case .enumeration(let name, let variants):
            out.writeStr("enum")
            out.writeStr(name)
            writeTypeParams(&out, s.typeParams)
            out.writeU32(UInt32(variants.count))
            for v in variants {
                out.writeStr(v.name)
                out.writeU32(v.index)
                switch v.payload {
                case .unit:
                    out.writeStr("unit")
                case .newtype(let r):
                    out.writeStr("newtype")
                    reference(r, path, &out)
                case .tuple(let refs):
                    out.writeStr("tuple")
                    out.writeU32(UInt32(refs.count))
                    for r in refs { reference(r, path, &out) }
                case .structure(let fields):
                    out.writeStr("struct")
                    out.writeU32(UInt32(fields.count))
                    for field in fields { self.field(field, path, &out) }
                }
            }
        case .tuple(let elements):
            out.writeStr("tuple")
            out.writeU32(UInt32(elements.count))
            for r in elements { reference(r, path, &out) }
        case .list(let element):
            out.writeStr("list")
            reference(element, path, &out)
        case .set(let element):
            out.writeStr("set")
            reference(element, path, &out)
        case .option(let element):
            out.writeStr("option")
            reference(element, path, &out)
        case .map(let key, let value):
            out.writeStr("map")
            reference(key, path, &out)
            reference(value, path, &out)
        case .array(let element, let dimensions):
            out.writeStr("array")
            reference(element, path, &out)
            out.writeU32(UInt32(dimensions.count))
            for d in dimensions { out.writeU64(d) }
        case .tensor(let element, let rank):
            out.writeStr("tensor")
            reference(element, path, &out)
            switch rank {
            case .none:
                out.writeU8(0)
            case .some(let r):
                out.writeU8(1)
                out.writeU32(r)
            }
        case .channel(let direction, let element):
            out.writeStr("channel")
            out.writeStr(direction.rawValue)
            reference(element, path, &out)
        case .dynamic:
            out.writeStr("dynamic")
        case .external(let kind, let metadata):
            out.writeStr("external")
            out.writeStr(kind)
            switch metadata {
            case .none:
                out.writeU8(0)
            case .some(let r):
                out.writeU8(1)
                reference(r, path, &out)
            }
        }
    }

    func field<S: Sink>(_ field: Field, _ path: [NodeIx], _ out: inout S) {
        out.writeStr(field.name)
        out.writeBool(field.required)
        reference(field.schema, path, &out)
    }

    func reference<S: Sink>(_ r: SchemaRef, _ path: [NodeIx], _ out: inout S) {
        switch r {
        case .variable(let name):
            out.writeStr("var")
            out.writeStr(name)
        case .concrete(let id, let args):
            if let target = keyToIndex[id.raw], component.contains(target) {
                if let depth = path.firstIndex(of: target) {
                    // Target is an ancestor on the current walk path: the
                    // back-reference that terminates the walk.
                    out.writeStr("backref")
                    out.writeU32(UInt32(depth))
                } else {
                    // Target is another component member, off-path: inline its
                    // structure with the path extended by it.
                    out.writeStr("inline")
                    schema(target, path + [target], &out)
                }
            } else if let target = keyToIndex[id.raw] {
                // A different, already-processed component: feed its id.
                let rid = assigned[target]!
                out.writeStr("concrete")
                out.writeU64(rid.raw)
            } else {
                // External: the reference already carries a real id.
                out.writeStr("concrete")
                out.writeU64(id.raw)
            }
            out.writeU32(UInt32(args.count))
            for a in args { reference(a, path, &out) }
        }
    }
}

// MARK: - Substitution of provisional keys with computed ids

private func remapRef(_ r: SchemaRef, _ map: [UInt64: SchemaId]) -> SchemaRef {
    switch r {
    case .variable(let name):
        return .variable(name: name)
    case .concrete(let id, let args):
        return .concrete(id: map[id.raw] ?? id, args: args.map { remapRef($0, map) })
    }
}

private func remapField(_ field: Field, _ map: [UInt64: SchemaId]) -> Field {
    Field(name: field.name, schema: remapRef(field.schema, map), required: field.required)
}

private func remapKind(_ kind: SchemaKind, _ map: [UInt64: SchemaId]) -> SchemaKind {
    switch kind {
    case .primitive(let p):
        return .primitive(p)
    case .dynamic:
        return .dynamic
    case .structure(let name, let fields):
        return .structure(name: name, fields: fields.map { remapField($0, map) })
    case .enumeration(let name, let variants):
        return .enumeration(name: name, variants: variants.map { v in
            let payload: VariantPayload
            switch v.payload {
            case .unit:
                payload = .unit
            case .newtype(let r):
                payload = .newtype(remapRef(r, map))
            case .tuple(let refs):
                payload = .tuple(refs.map { remapRef($0, map) })
            case .structure(let fields):
                payload = .structure(fields.map { remapField($0, map) })
            }
            return Variant(name: v.name, index: v.index, payload: payload)
        })
    case .tuple(let elements):
        return .tuple(elements: elements.map { remapRef($0, map) })
    case .list(let element):
        return .list(element: remapRef(element, map))
    case .set(let element):
        return .set(element: remapRef(element, map))
    case .option(let element):
        return .option(element: remapRef(element, map))
    case .map(let key, let value):
        return .map(key: remapRef(key, map), value: remapRef(value, map))
    case .array(let element, let dimensions):
        return .array(element: remapRef(element, map), dimensions: dimensions)
    case .tensor(let element, let rank):
        return .tensor(element: remapRef(element, map), rank: rank)
    case .channel(let direction, let element):
        return .channel(direction: direction, element: remapRef(element, map))
    case .external(let kind, let metadata):
        return .external(kind: kind, metadata: metadata.map { remapRef($0, map) })
    }
}

// MARK: - Entry point

/// Compute content-derived `SchemaId`s for a batch of mutually-referential
/// schemas.
///
/// On input, each schema's `id` and every in-batch `SchemaRef.concrete` id is a
/// caller-assigned *provisional key* (any unique `u64`). A reference whose id is
/// not a provisional key in the batch is treated as already resolved. The
/// returned schemas have real ids substituted everywhere.
// r[impl schema-identity.canonical-encoding]
// r[impl schema-identity.closure]
// r[impl schema-identity.computation]
// r[impl schema-identity.content-hash]
public func resolveIds(_ batch: [Schema]) -> [Schema] {
    let n = batch.count

    // Provisional key -> node index.
    var keyToIndex: [UInt64: NodeIx] = [:]
    keyToIndex.reserveCapacity(n)
    for (i, s) in batch.enumerated() {
        keyToIndex[s.id.raw] = NodeIx(i)
    }

    // Reference graph: edge i -> j when schema i references in-batch schema j.
    var adj: [[NodeIx]] = Array(repeating: [], count: n)
    for (i, s) in batch.enumerated() {
        var seen: Set<NodeIx> = []
        visitKindTargets(s.kind) { id in
            if let j = keyToIndex[id.raw], seen.insert(j).inserted {
                adj[i].append(j)
            }
        }
    }

    let sccs = Tarjan.run(adj)

    // Assign ids component-by-component, dependencies first.
    var assigned: [NodeIx: SchemaId] = [:]
    assigned.reserveCapacity(n)
    for scc in sccs {
        let component = Set(scc)
        let walk = Walk(batch: batch, keyToIndex: keyToIndex, component: component, assigned: assigned)
        // Within a component every member's id is independent, so order here
        // does not matter.
        var local: [(NodeIx, SchemaId)] = []
        for i in scc {
            var hasher = Blake3()
            walk.schema(i, [i], &hasher)
            local.append((i, hasher.finalizeId()))
        }
        for (i, id) in local {
            assigned[i] = id
        }
    }

    // Provisional key -> real id, for rewriting references.
    var keyToReal: [UInt64: SchemaId] = [:]
    keyToReal.reserveCapacity(n)
    for (i, s) in batch.enumerated() {
        keyToReal[s.id.raw] = assigned[NodeIx(i)]!
    }

    return batch.enumerated().map { (i, s) in
        Schema(id: assigned[NodeIx(i)]!, typeParams: s.typeParams, kind: remapKind(s.kind, keyToReal))
    }
}
