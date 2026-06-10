//! A reusable **copy-and-patch** JIT substrate, with no knowledge of any
//! particular IR, schema, or value model. It is the bottom layer a stencil JIT
//! sits on: produce machine code for each operation as a *stencil* (a compiler
//! emits it, leaving the continuation branch as a relocation hole), then at run
//! time copy the stencils into executable memory and patch the holes to chain
//! them. This crate provides exactly that bottom layer:
//!
//! - `ExecBuf` — allocate executable memory, copy machine code in, satisfy the
//!   platform's write-xor-execute and instruction-cache rules, and free it on
//!   drop. (Apple Silicon / `MAP_JIT` today; other backends slot in here.)
//! - [`patch_branch26`] — patch an AArch64 `B`/`BL` (`BRANCH26`) immediate so a
//!   copied stencil's continuation branch targets the next stencil.
//! - the `extract` module (the `build` feature) — at *build* time, compile a stencil
//!   source file with rustc (`--emit=obj`) and pull each stencil's bytes and its
//!   continuation relocations out of the object file by symbol. Use this from a
//!   `build.rs` (depend on copypatch with `features = ["build"]`).
//!
//! What stays in the *caller*: the stencils themselves, the per-op state structs,
//! and the lowering from the caller's IR to a chain of stencils. copypatch only
//! runs and patches bytes — it never encodes an instruction or interprets a
//! schema.

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod exec;
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub use exec::ExecBuf;

mod patch;
pub use patch::patch_branch26;

#[cfg(feature = "build")]
pub mod extract;
