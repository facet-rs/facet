// The compact codec's error type, the schema registry, and generic resolution.
//
// Mirrors the corresponding parts of `rust/phon-engine/src/compact.rs`.

import PhonSchema

/// Maximum nesting depth on decode (`r[validate.depth]`).
let compactMaxDepth = 128

// MARK: - Errors

/// Why a compact encode or decode failed.
public enum CompactError: Error {
    /// A referenced schema id is not in the registry.
    case unknownSchema(SchemaId)
    /// A kind or feature not yet implemented in this codec.
    case unsupported(String)
    /// The value's shape does not match the schema it is being encoded against.
    case typeMismatch(expected: String)
    /// An enum value names a variant the schema does not have.
    case unknownVariant(String)
    /// A decoded enum variant index is out of range.
    case badVariantIndex(UInt32)
    /// A generic schema applied with the wrong number of type arguments.
    case genericArity(params: Int, args: Int)
    /// A structurally malformed schema (unbound type variable, primitive carrying
    /// type arguments, …).
    case malformed(String)
    /// Two schemas cannot be reconciled into a translation plan.
    case incompatible(String)
    /// A decoded enum variant exists in the writer schema but not the reader.
    case writerOnlyVariant(UInt32)
    /// A decode-side validation failure from the byte reader.
    case decode(DecodeError)
    /// A dynamic (self-describing) sub-value failed to encode.
    case encode(String)
}

/// The `CompactError` variant name, matched against the conformance corpus's
/// `error_kind` so the recorded string stays stable across implementations.
public func errorKindName(_ e: CompactError) -> String {
    switch e {
    case .unknownSchema: return "UnknownSchema"
    case .unsupported: return "Unsupported"
    case .typeMismatch: return "TypeMismatch"
    case .unknownVariant: return "UnknownVariant"
    case .badVariantIndex: return "BadVariantIndex"
    case .genericArity: return "GenericArity"
    case .malformed: return "Malformed"
    case .incompatible: return "Incompatible"
    case .writerOnlyVariant: return "WriterOnlyVariant"
    case .decode: return "Decode"
    case .encode: return "Encode"
    }
}

// MARK: - Registry

/// A resolved schema closure: composite schemas by id, plus intrinsic
/// recognition of the primitive ids.
public struct Registry {
    private var composites: [SchemaId: Schema]
    private var primitives: [SchemaId: Primitive]

    /// Build a registry from a closure of composite schemas. Primitive schemas
    /// need not be supplied — they are recognized by their canonical id.
    public init(_ schemas: [Schema]) {
        var prims: [SchemaId: Primitive] = [:]
        for p in Primitive.allCases {
            prims[primitiveId(p)] = p
        }
        var comps: [SchemaId: Schema] = [:]
        for s in schemas {
            comps[s.id] = s
        }
        self.primitives = prims
        self.composites = comps
    }

    func primitive(_ id: SchemaId) -> Primitive? { primitives[id] }
    func composite(_ id: SchemaId) -> Schema? { composites[id] }

    /// A new registry with additional composite schemas merged in — used when a
    /// peer advertises its (writer) schema closure on top of the local one.
    public func with(_ extra: [Schema]) -> Registry {
        Registry(Array(composites.values) + extra)
    }
}

/// Parse a vox schema-closure blob into its root id and composite schemas:
/// `[u64 root LE][u32 count LE]` then `count` schemas each as
/// `[u32 len LE][self-describing schema bytes]`. The framing shared with Rust
/// (`vox_phon::parse_schema_bytes`) and TypeScript (`parseSchemaClosure`).
public func parseSchemaClosure(_ bytes: [UInt8]) throws -> (root: SchemaId, schemas: [Schema]) {
    var r = Reader(bytes)
    let root = SchemaId(try r.readU64())
    let count = try r.readU32()
    var schemas: [Schema] = []
    schemas.reserveCapacity(Int(count))
    for _ in 0..<count {
        let len = Int(try r.readU32())
        let slice = try r.readSlice(len)
        schemas.append(try schemaFromBytes(Array(slice)))
    }
    return (root, schemas)
}

/// A reference resolved to either a primitive or a fully type-substituted kind.
enum Resolved {
    case primitive(Primitive)
    case composite(SchemaKind)
}

/// Resolve a reference against the registry, applying generic substitution.
/// Shared by the compact codec's walks and the compatibility planner.
func resolve(_ reg: Registry, _ r: SchemaRef) throws -> Resolved {
    switch r {
    case .variable:
        throw CompactError.malformed("unbound type variable")
    case .concrete(let id, let args):
        if let p = reg.primitive(id) {
            if !args.isEmpty {
                throw CompactError.malformed("primitive carrying type arguments")
            }
            return .primitive(p)
        } else if let schema = reg.composite(id) {
            if schema.typeParams.count != args.count {
                throw CompactError.genericArity(params: schema.typeParams.count, args: args.count)
            }
            let kind = args.isEmpty
                ? schema.kind
                : substituteKind(schema.kind, schema.typeParams, args)
            return .composite(kind)
        } else {
            throw CompactError.unknownSchema(id)
        }
    }
}

// MARK: - Generic substitution
//
// Resolving a parametric schema substitutes its type parameters with the
// arguments from a concrete reference, throughout its kind. Substitution is eager
// and per-reference: each `concrete(id, args)` produces a Var-free kind before it
// is walked, so the walker never meets a `variable`.

func substituteRef(_ r: SchemaRef, _ params: [String], _ args: [SchemaRef]) -> SchemaRef {
    switch r {
    case .variable(let name):
        if let i = params.firstIndex(of: name) { return args[i] }
        return r
    case .concrete(let id, let inner):
        return .concrete(id: id, args: inner.map { substituteRef($0, params, args) })
    }
}

private func substituteField(_ f: Field, _ params: [String], _ args: [SchemaRef]) -> Field {
    Field(name: f.name, schema: substituteRef(f.schema, params, args), required: f.required)
}

func substituteKind(_ kind: SchemaKind, _ params: [String], _ args: [SchemaRef]) -> SchemaKind {
    switch kind {
    case .primitive(let p):
        return .primitive(p)
    case .dynamic:
        return .dynamic
    case .structure(let name, let fields):
        return .structure(name: name, fields: fields.map { substituteField($0, params, args) })
    case .enumeration(let name, let variants):
        return .enumeration(name: name, variants: variants.map { v in
            let payload: VariantPayload
            switch v.payload {
            case .unit:
                payload = .unit
            case .newtype(let r):
                payload = .newtype(substituteRef(r, params, args))
            case .tuple(let rs):
                payload = .tuple(rs.map { substituteRef($0, params, args) })
            case .structure(let fs):
                payload = .structure(fs.map { substituteField($0, params, args) })
            }
            return Variant(name: v.name, index: v.index, payload: payload)
        })
    case .tuple(let elements):
        return .tuple(elements: elements.map { substituteRef($0, params, args) })
    case .list(let element):
        return .list(element: substituteRef(element, params, args))
    case .set(let element):
        return .set(element: substituteRef(element, params, args))
    case .option(let element):
        return .option(element: substituteRef(element, params, args))
    case .map(let key, let value):
        return .map(key: substituteRef(key, params, args), value: substituteRef(value, params, args))
    case .array(let element, let dimensions):
        return .array(element: substituteRef(element, params, args), dimensions: dimensions)
    case .tensor(let element, let rank):
        return .tensor(element: substituteRef(element, params, args), rank: rank)
    case .channel(let direction, let element):
        return .channel(direction: direction, element: substituteRef(element, params, args))
    case .external(let kind, let metadata):
        return .external(kind: kind, metadata: metadata.map { substituteRef($0, params, args) })
    }
}
