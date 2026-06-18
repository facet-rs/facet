/// The phon front door in Swift: the binding that produces descriptors by
/// probing the Swift runtime — reflection over stored properties, enum-case
/// layout, type metadata — and offers the ergonomic typed encode/decode API.
///
/// The only Swift module that touches the runtime's reflection; the design
/// transfers from Rust, the code does not (`r[descriptors.separate-implementations]`).
/// Swift consumes phon codegen output, so there is no Swift codegen module.
///
/// Spec: `docs/content/spec.md` — "Swift", "The descriptor model".
// r[impl crates.concern-separation]
public enum Phon {}
