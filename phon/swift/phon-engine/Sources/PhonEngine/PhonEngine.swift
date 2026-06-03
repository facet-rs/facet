/// Swift's backend-blind baseline engine: the compact codec, compatibility
/// planning, and the IR interpreter. Always works, including where the JIT
/// cannot run (`r[exec.interpreter-baseline]`).
/// r[impl exec.interpreter-baseline]
///
/// Consumes Swift descriptors and an IR; reaches for no runtime reflection of its
/// own — that is the binding's job (`r[crates.engine-is-binding-free]`).
///
/// Spec: `docs/content/spec.md` — "Compact mode", "Compatibility", "Decoding",
/// "Decoding untrusted input".
public enum PhonEngine {}
