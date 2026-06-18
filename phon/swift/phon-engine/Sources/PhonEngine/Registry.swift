// The compact codec's error type, the schema registry, and generic resolution.
//
// Mirrors the corresponding parts of `rust/phon-engine/src/compact.rs`.

import PhonSchema

/// Maximum nesting depth on decode.
// r[impl validate.depth]
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
    /// A received schema bundle carried an id that does not match its content.
    case bundleSchemaIdMismatch(stated: SchemaId, recomputed: SchemaId)
    /// Two schemas cannot be translated by a compatibility plan.
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
    case .bundleSchemaIdMismatch: return "BundleSchemaIdMismatch"
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

    /// Validate a received schema closure before making it executable.
    ///
    /// This recomputes every member's content-derived `SchemaId`, rejects
    /// references that are neither primitive nor present in the bundle, and
    /// bounds fixed arrays whose elements have zero wire size.
    // r[impl validate.bundles]
    public init(validating schemas: [Schema]) throws {
        try validateBundle(schemas)
        self.init(schemas)
    }

    func primitive(_ id: SchemaId) -> Primitive? { primitives[id] }
    func composite(_ id: SchemaId) -> Schema? { composites[id] }

    /// A new registry with additional composite schemas merged in — used when a
    /// peer advertises its (writer) schema closure on top of the local one.
    public func with(_ extra: [Schema]) -> Registry {
        Registry(Array(composites.values) + extra)
    }

    /// Merge an advertised writer closure after applying the received-bundle
    /// validation path.
    // r[impl validate.bundles]
    public func withValidating(_ extra: [Schema]) throws -> Registry {
        try Registry(validating: Array(composites.values) + extra)
    }
}

private func validateBundle(_ schemas: [Schema]) throws {
    let recomputed = resolveIds(schemas)
    for (schema, recomputedSchema) in zip(schemas, recomputed) where schema.id != recomputedSchema.id {
        throw CompactError.bundleSchemaIdMismatch(stated: schema.id, recomputed: recomputedSchema.id)
    }

    let provided = Set(schemas.map(\.id))
    let primitives = Set(Primitive.allCases.map { primitiveId($0) })
    for schema in schemas {
        try validateKindRefs(schema.kind, provided: provided, primitives: primitives)
    }

    let reg = Registry(schemas)
    for schema in schemas {
        try validateFixedArrayCaps(schema.kind, reg)
    }
}

private func validateKindRefs(_ kind: SchemaKind, provided: Set<SchemaId>, primitives: Set<SchemaId>) throws {
    switch kind {
    case .primitive, .dynamic:
        return
    case .structure(_, let fields):
        for field in fields { try validateRef(field.schema, provided: provided, primitives: primitives) }
    case .enumeration(_, let variants):
        for variant in variants { try validatePayloadRefs(variant.payload, provided: provided, primitives: primitives) }
    case .tuple(let elements):
        for element in elements { try validateRef(element, provided: provided, primitives: primitives) }
    case .list(let element),
         .set(let element),
         .array(let element, _),
         .tensor(let element, _),
         .option(let element),
         .channel(_, let element):
        try validateRef(element, provided: provided, primitives: primitives)
    case .map(let key, let value):
        try validateRef(key, provided: provided, primitives: primitives)
        try validateRef(value, provided: provided, primitives: primitives)
    case .external(_, let metadata):
        if let metadata { try validateRef(metadata, provided: provided, primitives: primitives) }
    }
}

private func validatePayloadRefs(
    _ payload: VariantPayload,
    provided: Set<SchemaId>,
    primitives: Set<SchemaId>
) throws {
    switch payload {
    case .unit:
        return
    case .newtype(let ref):
        try validateRef(ref, provided: provided, primitives: primitives)
    case .tuple(let elements):
        for element in elements { try validateRef(element, provided: provided, primitives: primitives) }
    case .structure(let fields):
        for field in fields { try validateRef(field.schema, provided: provided, primitives: primitives) }
    }
}

private func validateRef(_ ref: SchemaRef, provided: Set<SchemaId>, primitives: Set<SchemaId>) throws {
    switch ref {
    case .variable:
        return
    case .concrete(let id, let args):
        guard provided.contains(id) || primitives.contains(id) else {
            throw CompactError.unknownSchema(id)
        }
        for arg in args { try validateRef(arg, provided: provided, primitives: primitives) }
    }
}

private func validateFixedArrayCaps(_ kind: SchemaKind, _ reg: Registry) throws {
    switch kind {
    case .primitive, .dynamic:
        return
    case .structure(_, let fields):
        for field in fields { try validateFixedArrayRef(field.schema, reg) }
    case .enumeration(_, let variants):
        for variant in variants { try validateFixedArrayPayload(variant.payload, reg) }
    case .tuple(let elements):
        for element in elements { try validateFixedArrayRef(element, reg) }
    case .list(let element),
         .set(let element),
         .tensor(let element, _),
         .option(let element),
         .channel(_, let element):
        try validateFixedArrayRef(element, reg)
    case .map(let key, let value):
        try validateFixedArrayRef(key, reg)
        try validateFixedArrayRef(value, reg)
    case .array(let element, let dimensions):
        let count = try product(dimensions)
        if minWireSizeRef(reg, element) == 0 && count > UInt64(zstCountCap) {
            throw CompactError.decode(.lengthTooLarge(count: count, remaining: zstCountCap))
        }
        try validateFixedArrayRef(element, reg)
    case .external(_, let metadata):
        if let metadata { try validateFixedArrayRef(metadata, reg) }
    }
}

private func validateFixedArrayPayload(_ payload: VariantPayload, _ reg: Registry) throws {
    switch payload {
    case .unit:
        return
    case .newtype(let ref):
        try validateFixedArrayRef(ref, reg)
    case .tuple(let elements):
        for element in elements { try validateFixedArrayRef(element, reg) }
    case .structure(let fields):
        for field in fields { try validateFixedArrayRef(field.schema, reg) }
    }
}

private func validateFixedArrayRef(_ ref: SchemaRef, _ reg: Registry) throws {
    switch ref {
    case .variable:
        return
    case .concrete(_, let args):
        for arg in args { try validateFixedArrayRef(arg, reg) }
    }
}

/// An additional writer root carried by the same schema binding, keyed by role.
public struct AuxiliaryRoot: Sendable, Hashable {
    public let role: String
    public let root: SchemaId

    public init(role: String, root: SchemaId) {
        self.role = role
        self.root = root
    }
}

/// Parse a vox schema-binding blob into its primary root id and composite schemas:
/// `[u64 root LE][u32 count LE]` then `count` schemas each as
/// `[u32 len LE][self-describing schema bytes]`, optionally followed by auxiliary
/// roots as `[u32 count][u32 role_len][role UTF-8][u64 root]...`. The framing
/// shared with Rust (`vox_phon::parse_schema_bytes`) and TypeScript
/// (`parseSchemaClosure`).
public func parseSchemaClosure(_ bytes: [UInt8]) throws -> (
    root: SchemaId, schemas: [Schema], auxiliaryRoots: [AuxiliaryRoot]
) {
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
    var auxiliaryRoots: [AuxiliaryRoot] = []
    if r.remaining > 0 {
        let auxCount = try r.readU32()
        auxiliaryRoots.reserveCapacity(Int(auxCount))
        for _ in 0..<auxCount {
            let roleLen = Int(try r.readU32())
            let roleBytes = try r.readSlice(roleLen)
            let role = String(decoding: roleBytes, as: UTF8.self)
            let auxRoot = SchemaId(try r.readU64())
            auxiliaryRoots.append(AuxiliaryRoot(role: role, root: auxRoot))
        }
    }
    if r.remaining != 0 {
        throw DecodeError.trailingBytes(r.remaining)
    }
    return (root, schemas, auxiliaryRoots)
}

/// A reference resolved to either a primitive or a fully type-substituted kind.
enum Resolved {
    case primitive(Primitive)
    case composite(SchemaKind)
}

/// Resolve a reference against the registry, applying generic substitution.
/// Shared by the compact codec's walks and the compatibility planner.
// r[impl type-system.generic-resolution]
// r[impl schema-identity.unknown-is-error]
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
