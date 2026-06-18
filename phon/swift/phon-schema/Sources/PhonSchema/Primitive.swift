// The leaf types of the phon type system. Order and tag strings mirror
// `rust/phon-schema/src/schema.rs`; the tag is the sole input to a primitive's
// content hash (`r[schema-identity.canonical-encoding]`).

public enum Primitive: String, Sendable, Hashable, CaseIterable {
    case bool
    case u8
    case u16
    case u32
    case u64
    case u128
    case i8
    case i16
    case i32
    case i64
    case i128
    case f32
    case f64
    case char
    case string
    case bytes
    /// An instant or civil time, carried as its RFC 3339 / ISO 8601 string.
    case datetime
    /// A UUID, carried as its lowercase hyphenated string.
    case uuid
    /// A qualified name, carried as its James Clark `{namespace}local` string.
    case qname
    case unit
    case never

    /// The tag string fed to the identity hash (and used in the wire tag set).
    public var tag: String { rawValue }
}
