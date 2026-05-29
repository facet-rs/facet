/// Swift's optional copy-and-patch JIT, on the same compiler substrate as Rust's
/// (swiftc/LLVM). Each IR op is a stencil; "compiling" a program is memcpy +
/// patch. Identical results to the interpreter, only faster (`r[exec.jit-optional]`).
///
/// Reached only by opting in to the `PhonJIT` product; runs where the platform
/// permits allocating executable memory (macOS does) (`r[crates.jit-opt-in]`).
///
/// Spec: `docs/content/spec.md` — `r[ir.stencils]`, `r[ir.inlining]`, "Swift".
public enum PhonJIT {}
