//! The machine — vix's evaluator, final shape. There is exactly one.
//!
//! This module exists because of a ruling (engine-demand-semantics.md,
//! amendment A5, 2026-07-04): the two-evaluator era is over. The eager
//! oracle and the recursive-driver engine are FROZEN — no new semantics
//! land in them, ever — and both get deleted once the corpus passes
//! here. There is no intermediate path; this is the final shape, built
//! deliberately small before it is built large.
//!
//! Conventions that are not up for renegotiation (each has a ruling
//! behind it; see the constitution and the taste record in the vixen
//! repo, docs/design/):
//!
//! - **Demand drives everything.** Nothing forces locally. The demand
//!   graph of the constitution is the program; evaluation is demand
//!   backpropagating from selected outputs. Suspension happens ONLY at
//!   run boundaries; the pending frontier is enumerable at any moment.
//! - **The driver is an async runtime, not a recursion.** Worker
//!   threads pick up ready nodes; work-stealing is legal because
//!   scheduling order is unobservable by construction (canonical total
//!   order over values, canonical collect).
//! - **Execution is weavy-shaped.** Node bodies lower to the
//!   weavy-async machine: explicit context, operand stack, suspend by
//!   returning up, resume by re-entry. The interpreter lane is the
//!   reference and is always available (wasm/iOS forbid JIT); stencils
//!   are an accelerator that may lag, never a prerequisite.
//! - **Values: scalars unboxed, composites behind handles.** See
//!   [`value`]. There is no boxed do-everything Value enum here and
//!   there never will be one; that enum is a frozen-evaluator artifact.
//! - **Types have two authorities, one description vocabulary.** A type
//!   defined in Rust is described by facet — that is how lowering knows
//!   how to build and read it. A type defined in vix owns an optimized
//!   ABI (roughly what a Rust enum would do), and that ABI is RECORDED
//!   as a [`value::Layout`] and OBSERVABLE from Rust. Never two worlds
//!   for one type.
//! - **Persist facts keyed by content, never positions.** The lowering
//!   cache (canonical hash → graph shape) is primary machinery, not an
//!   optimization; per-evaluation node indices die with the evaluation.
//! - **Semantics land once.** Here. The corpus pins behavior with
//!   absolute assertions (expected values, event multisets, journal
//!   pins) — correctness is never "agrees with a second implementation".
//!
//! Build order (kept current as slices land): value model (this
//! commit) → node graph + async driver → lowering from the typed AST
//! with the content-keyed lowering cache → exec/journal seams → corpus
//! parity → the frozen evaluators are deleted.

pub mod graph;
pub mod value;

pub use graph::{Graph, InputId, MachineError, NodeId, NodeOp};
pub use value::{
    Field, Handle, Layout, LayoutError, LayoutId, Registry, Slot, SlotTy, Store, TotalF64,
    ValueRef, Variant,
};
