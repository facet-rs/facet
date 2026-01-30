//! Partial v2 - Clean separation of Immediate and Deferred modes
//!
//! This is a sketch/design doc for refactoring Partial.

use std::collections::BTreeMap;

use facet_core::{MapDef, SetDef, Shape};
use facet_path::Path;

mod ops;
pub use ops::*;

mod immediate;
pub use immediate::*;

mod deferred;
pub use deferred::*;

// =============================================================================
// Public API - erases the difference
// =============================================================================

enum PartialInner<'facet, const BORROW: bool> {
    Immediate(ImmediatePartial<'facet, BORROW>),
    Deferred(Box<DeferredPartial<'facet, BORROW>>),
}

/// The public Partial API - wraps either Immediate or Deferred mode
pub struct Partial<'facet, const BORROW: bool> {
    inner: PartialInner<'facet, BORROW>,
}

impl<'facet, const BORROW: bool> Partial<'facet, BORROW> {
    /// Execute a batch of operations.
    ///
    /// All operations are processed before returning. This is required because
    /// `Set` ops may contain pointers to the caller's stack.
    ///
    /// Mode switches (BeginDeferred/FinishDeferred) are handled transparently
    /// by splitting the batch at those points.
    pub fn submit(&mut self, ops: &[PartialOp<'_>]) -> Result<(), ReflectError> {
        let mut i = 0;
        while i < ops.len() {
            match ops[i] {
                PartialOp::BeginDeferred => {
                    // Process everything before the switch
                    self.submit_to_inner(&ops[..i])?;
                    // Do the switch
                    self.begin_deferred()?;
                    // Continue with the rest (recurse to handle any further switches)
                    return self.submit(&ops[i + 1..]);
                }
                PartialOp::FinishDeferred => {
                    // Process everything before the switch
                    self.submit_to_inner(&ops[..i])?;
                    // Do the switch
                    self.finish_deferred()?;
                    // Continue with the rest
                    return self.submit(&ops[i + 1..]);
                }
                _ => i += 1,
            }
        }
        // No mode switches found, process all ops
        self.submit_to_inner(ops)
    }

    /// Submit ops to the current mode (guaranteed no mode switches in ops)
    fn submit_to_inner(&mut self, ops: &[PartialOp<'_>]) -> Result<(), ReflectError> {
        if ops.is_empty() {
            return Ok(());
        }
        match &mut self.inner {
            PartialInner::Immediate(imm) => imm.submit(ops),
            PartialInner::Deferred(def) => def.submit(ops),
        }
    }

    /// Enter deferred mode
    fn begin_deferred(&mut self) -> Result<(), ReflectError> {
        // Take ownership of inner, transform, put back
        let old_inner = std::mem::replace(
            &mut self.inner,
            // Temporary placeholder - will be replaced
            PartialInner::Immediate(ImmediatePartial {
                arena: FrameArena::new(),
                stack: Vec::new(),
                root_plan: std::sync::Arc::new(todo!()),
                state: PartialState::Poisoned,
                _marker: std::marker::PhantomData,
            }),
        );

        match old_inner {
            PartialInner::Immediate(imm) => {
                // TODO: compute current path from imm.stack
                let base_path: Path = todo!();
                self.inner = PartialInner::Deferred(Box::new(DeferredPartial {
                    arena: imm.arena,
                    stack: imm.stack,
                    stored_frames: BTreeMap::new(),
                    deferred_ops: Vec::new(),
                    base_path,
                    inner: ImmediatePartial {
                        arena: FrameArena::new(),
                        stack: Vec::new(),
                        root_plan: imm.root_plan,
                        state: imm.state,
                        _marker: std::marker::PhantomData,
                    },
                }));
                Ok(())
            }
            PartialInner::Deferred(_) => {
                Err(ReflectError) // already in deferred mode
            }
        }
    }

    /// Exit deferred mode, validate everything
    fn finish_deferred(&mut self) -> Result<(), ReflectError> {
        let old_inner = std::mem::replace(
            &mut self.inner,
            PartialInner::Immediate(ImmediatePartial {
                arena: FrameArena::new(),
                stack: Vec::new(),
                root_plan: std::sync::Arc::new(todo!()),
                state: PartialState::Poisoned,
                _marker: std::marker::PhantomData,
            }),
        );

        match old_inner {
            PartialInner::Deferred(mut def) => {
                let imm = def.finish()?;
                self.inner = PartialInner::Immediate(imm);
                Ok(())
            }
            PartialInner::Immediate(_) => {
                Err(ReflectError) // not in deferred mode
            }
        }
    }
}
