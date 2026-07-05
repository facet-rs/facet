//! The machine — vix's evaluator. There is exactly one, ever.
//!
//! Governing rulings: engine-demand-semantics.md amendments A5 and A6
//! in the vixen repo (docs/design/). The load-bearing parts, restated
//! where implementers will actually read them:
//!
//! - **One implementation of vix.** Not a reference, not an oracle, not
//!   an interpreter tier, not a simplified or eager one. The frozen
//!   legacy evaluators (oracle.rs, engine.rs) die at corpus parity.
//!   Correctness is defined by the written semantics plus the pinned
//!   trace corpus — never by agreement with another implementation.
//! - **Typed instructions over UNTAGGED operands.** `AddI64` and
//!   `AddF64` are different instructions, chosen at lowering time
//!   because the types are known then. Runtime never asks what a value
//!   is. No tags, no kind-markers, no fallible arithmetic, no checked
//!   mode — no dynamic-language machinery of any sort. A sum type's
//!   discriminant is the type's own layout, matched by code that
//!   statically knows the type: that is not a tag. If a safety net is
//!   ever wanted, it is static validation of lowered programs
//!   (wasm-style), never runtime checks.
//! - **Two layout authorities, one description vocabulary — and the
//!   vocabulary is `weavy::mem`.** Rust-authored types reach it from
//!   facet shapes; vix/fable-authored types reach it from the
//!   language's checker computing an optimized ABI (what a Rust enum
//!   would do) and emitting the same `weavy::mem::Descriptor`s,
//!   schema-keyed by content-addressed type identity. Layouts are
//!   compile-time knowledge plus introspection metadata (DWARF-like);
//!   execution never dispatches on them.
//! - **Demand drives everything; suspension is only for the
//!   started-and-blocked.** An undemanded node is pure data — no
//!   future, no frame, no cost; graphs have millions of nodes. A
//!   computation suspends only after it began and cannot make forward
//!   progress. Joint-demand batching at memo boundaries is a scheduler
//!   policy about when to begin — never a return to spawning.
//! - **Vix lowers to weavy; portability is weavy's concern.** How a
//!   target executes (JIT or portable) is invisible above the
//!   waterline. Fable (this repo, ../fable) is a SIBLING language on
//!   the same substrate — it grows the shared teeth first (typed user
//!   types, host async, the ABI/frame model) because small programs
//!   derisk the substrate; vix never lowers through it.
//! - **One unified trace; instrumentation is weavy IR.** Emit IR with
//!   the instrumentation the mode wants (tests trace innards,
//!   production keeps suspension points); weavy strips by mode. Tests
//!   are assertions over traces — sequences, presence, ABSENCE
//!   (absence is how laziness is proven).
//! - **Wasm must execute, not just build.** Machine changes have a
//!   wasm floor because `wasm32` has 32-bit `usize`; any value-store
//!   word, handle, hash, run id, timestamp, or frame word that is
//!   semantically 64-bit must stay `i64`/`u64` until an explicitly
//!   checked small index conversion. The permanent smoke is the
//!   playground `runVixMachine` flow over `merge-demand::selected`
//!   and `eval::demo`.
//! - **Persist facts keyed by content, never positions.** The lowering
//!   cache (canonical hash → lowered program) is primary machinery;
//!   per-evaluation state dies with the evaluation.
//!
//! Build order from here: fable×weavy teeth (typed instruction
//! vocabulary, frames/ABI, layouts in motion, host async) → vix
//! lowering onto that substrate → the milestone: a compound value from
//! several producers, one path selected, unselected producers provably
//! never executing (trace absence), through the VM and JIT.

mod ast_probe;
pub mod driver;
mod elf;
pub mod lower;
mod oci;
pub mod value;
mod version;

pub use driver::{
    CodeBundle, CodeRef, DriveEvent, MachineExecBackend, MachineExecRequest, MachinePathDemand,
    MachinePendingRun, RenderedValue, StoreHandle, StoreValue, ValueBundle,
};
pub use lower::{Machine, MachineArg, NamedArg, ReloadDiff};
pub use value::TotalF64;
