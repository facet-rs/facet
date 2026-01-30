//! Partial v2 - Clean separation of Immediate and Deferred modes
//!
//! This is a sketch/design doc for refactoring Partial.

use crate::ReflectError;

mod ops;
pub use ops::*;

mod immediate;
use immediate::ImmediatePartial;

mod deferred;
use deferred::DeferredPartial;

// =============================================================================
// Public API - erases the difference
// =============================================================================

enum PartialInner {
    Immediate(ImmediatePartial),
    Deferred(Box<DeferredPartial>),
}

/// The public Partial API - wraps either Immediate or Deferred mode
pub struct Partial {
    inner: PartialInner,
}

impl Partial {
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
        // TODO: transform Immediate -> Deferred
        todo!()
    }

    /// Exit deferred mode, validate everything
    fn finish_deferred(&mut self) -> Result<(), ReflectError> {
        // TODO: process deferred tasks, transform Deferred -> Immediate
        todo!()
    }
}
