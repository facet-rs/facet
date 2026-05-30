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

/// Stencils: one small function per op plus the threaded state ABI. Today they
/// are called through a function pointer; the machine-code version extracts their
/// bytes and patches the holes. Same functions, same immediates, same ABI.
///
/// Spec: `r[ir.stencils]`.
pub mod stencil;

/// Lowering: compile a linear IR program into a flat `(stencil, immediates)`
/// table — the copy-and-patch shape. The machine-code backend will splice and
/// patch; this stand-in threads through function pointers, identical results.
///
/// Spec: `r[ir.inlining]`, `r[ir.memory]`, `r[exec.jit-optional]`.
pub mod lower;

// r[impl exec.jit-optional]
pub use lower::{CompiledDecode, CompiledEncode, compile_decode, compile_encode};
