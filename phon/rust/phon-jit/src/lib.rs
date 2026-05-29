//! The optional copy-and-patch JIT.
//!
//! Each IR op is a stencil — a fragment of machine code compiled once at build
//! time from a small Rust function, with holes for its patch values.
//! "JIT-compiling" a program is memcpying its stencils into executable memory
//! and patching the holes: no optimizer, no unwinding. The result is identical
//! to the interpreter, only faster (`r[exec.jit-optional]`).
//!
//! This crate is reached only through the `jit` Cargo feature on the `phon`
//! front door, so the baseline pays nothing for it and platforms that forbid
//! executable memory omit it entirely (`r[crates.jit-opt-in]`). Like the engine,
//! it is binding-free — it consumes only the IR.
//!
//! Spec: `docs/content/spec.md` — `r[ir.stencils]`, `r[ir.inlining]`,
//! `r[ir.memory]`.

/// Stencils: the per-op machine-code fragments, extracted at build time, and
/// their patch-hole descriptions.
///
/// Spec: `r[ir.stencils]`.
pub mod stencil {}

/// Lowering: turn a linear IR program into patched executable memory — splicing
/// inlined chains, emitting `call-program` at cycle re-entry and the size cap,
/// threading state through the fixed-register ABI.
///
/// Spec: `r[ir.inlining]`, `r[ir.memory]`.
pub mod lower {}
