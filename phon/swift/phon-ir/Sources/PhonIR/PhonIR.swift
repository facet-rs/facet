/// The execution vocabulary shared by Swift's phon backends: the descriptor
/// model (Swift memory layout, sourced from runtime metadata) and the IR the
/// interpreter and JIT both consume.
///
/// The model's *shape* is shared with Rust and documented once in the spec;
/// Swift has its own descriptors describing Swift memory, and they never cross
/// the language boundary (`r[descriptors.separate-implementations]`).
///
/// Spec: `docs/content/spec.md` — "The descriptor model", "The intermediate
/// representation".
public enum PhonIR {}
