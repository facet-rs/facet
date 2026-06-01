// The coarse self-describing value. The rich wire tag set folds onto this small
// model — one number, one array, one object — exactly as `facet_value::Value`
// does in Rust. A schema-less decode recovers a `Value`, and the `Dynamic` kind
// round-trips one.
//
// Mirrors the cases `rust/phon-schema/src/selfdescribing.rs` reads and writes.

/// A canonical number. The width is the *narrowest* of the signed/unsigned 64-
/// and 128-bit forms that holds the value (floats are always `f64`), matching the
/// tag the self-describing encoder emits. Decoding any integer tag canonicalizes
/// to this set, so re-encoding is deterministic and byte-stable.
public enum Number: Hashable, Sendable {
    case f64(Double)
    case i64(Int64)
    case u64(UInt64)
    case i128(Int128)
    case u128(UInt128)

    /// Canonicalize a non-negative magnitude: the narrowest of `i64`/`u64`/`i128`/
    /// `u128` that holds it.
    public static func canonical(unsigned v: UInt128) -> Number {
        if v <= UInt128(Int64.max) { return .i64(Int64(v)) }
        if v <= UInt128(UInt64.max) { return .u64(UInt64(v)) }
        if v <= UInt128(Int128.max) { return .i128(Int128(v)) }
        return .u128(v)
    }

    /// Canonicalize a signed value: `i64` when it fits, else `u64` when it fits a
    /// non-negative `u64`, else `i128` (always holds a 128-bit signed value).
    public static func canonical(signed v: Int128) -> Number {
        if v >= Int128(Int64.min) && v <= Int128(Int64.max) { return .i64(Int64(v)) }
        if v >= 0 && v <= Int128(UInt64.max) { return .u64(UInt64(v)) }
        return .i128(v)
    }
}

/// A self-describing value. Aggregates carry their elements in wire order, so a
/// decoded value re-encodes byte-for-byte.
public indirect enum Value: Hashable, Sendable {
    case null
    case bool(Bool)
    case number(Number)
    case string(String)
    case bytes([UInt8])
    case char(Unicode.Scalar)
    case array([Value])
    /// An ordered string-keyed map. Keys are unique and held in wire order.
    case object([Entry])
    case datetime(DateTime)
    case uuid(UInt128)
    case qname(namespace: String?, local: String)

    /// One `object` entry: a string key and its value.
    public struct Entry: Hashable, Sendable {
        public var key: String
        public var value: Value
        public init(key: String, value: Value) {
            self.key = key
            self.value = value
        }
    }
}

// MARK: - Accessors

public extension Value {
    var isNull: Bool { if case .null = self { return true }; return false }

    var asBool: Bool? { if case .bool(let b) = self { return b }; return nil }
    var asNumber: Number? { if case .number(let n) = self { return n }; return nil }
    var asString: String? { if case .string(let s) = self { return s }; return nil }
    var asBytes: [UInt8]? { if case .bytes(let b) = self { return b }; return nil }
    var asChar: Unicode.Scalar? { if case .char(let c) = self { return c }; return nil }
    var asArray: [Value]? { if case .array(let a) = self { return a }; return nil }
    var asObject: [Entry]? { if case .object(let o) = self { return o }; return nil }

    /// Look up a key in an `object` value (linear scan; objects are small and
    /// ordered).
    func get(_ key: String) -> Value? {
        guard case .object(let entries) = self else { return nil }
        return entries.first { $0.key == key }?.value
    }
}

// MARK: - Number conversions
//
// Lossy width conversions matching `facet_value::VNumber`'s accessors, used by
// the compact codec when writing a number at a fixed primitive width. `nil` means
// "does not fit / not an integer of that signedness" (the codec then writes 0).

public extension Number {
    /// The value as `u64` if it is a non-negative integer that fits.
    var toU64: UInt64? {
        switch self {
        case .i64(let v): return v >= 0 ? UInt64(v) : nil
        case .u64(let v): return v
        case .i128(let v): return (v >= 0 && v <= Int128(UInt64.max)) ? UInt64(v) : nil
        case .u128(let v): return v <= UInt128(UInt64.max) ? UInt64(truncatingIfNeeded: v) : nil
        case .f64: return nil
        }
    }

    /// The value as `i64` if it is an integer that fits.
    var toI64: Int64? {
        switch self {
        case .i64(let v): return v
        case .u64(let v): return v <= UInt64(Int64.max) ? Int64(v) : nil
        case .i128(let v): return (v >= Int128(Int64.min) && v <= Int128(Int64.max)) ? Int64(v) : nil
        case .u128(let v): return v <= UInt128(Int64.max) ? Int64(truncatingIfNeeded: v) : nil
        case .f64: return nil
        }
    }

    /// The value as `u128` if it is a non-negative integer.
    var toU128: UInt128? {
        switch self {
        case .i64(let v): return v >= 0 ? UInt128(v) : nil
        case .u64(let v): return UInt128(v)
        case .i128(let v): return v >= 0 ? UInt128(v) : nil
        case .u128(let v): return v
        case .f64: return nil
        }
    }

    /// The value as `i128` if it is an integer.
    var toI128: Int128? {
        switch self {
        case .i64(let v): return Int128(v)
        case .u64(let v): return Int128(v)
        case .i128(let v): return v
        case .u128(let v): return v <= UInt128(Int128.max) ? Int128(v) : nil
        case .f64: return nil
        }
    }

    /// The value as `f64`, lossily (an integer is converted to the nearest double).
    var toF64Lossy: Double {
        switch self {
        case .f64(let v): return v
        case .i64(let v): return Double(v)
        case .u64(let v): return Double(v)
        case .i128(let v): return Double(v)
        case .u128(let v): return Double(v)
        }
    }
}
