/// phon's wire contract in Swift: the schema model, content-derived schema
/// identity, the dynamic `Value` type, and the self-describing codec.
///
/// A phon `Schema` in Swift is a Swift `enum` with the same variants carrying the
/// same data as the canonical Rust definitions, producing and consuming identical
/// self-describing phon bytes (`r[type-system.canonical-form]`).
///
/// Spec: `docs/content/spec.md` — "Type system", "Schema identity",
/// "Self-describing mode".
public enum PhonSchema {}
