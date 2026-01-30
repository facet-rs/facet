// =============================================================================
// Deferred Operations
// =============================================================================

use std::collections::BTreeMap;

use crate::{
    ReflectError,
    api2::ImmediatePartial,
    arena::{Arena, Idx},
};

use super::PartialOp;
use facet_path::Path;

/// Frame info for deferred
struct Frame {
    // TODO: add tracking etc.
}

type FrameId = Idx<Frame>;

type FrameArena = Arena<FrameId>;

/// Tasks that are deferred until FinishDeferred is processed.
/// Processed deepest-first.
struct Task {
    depth: usize,
    kind: TaskKind,
}

enum TaskKind {
    /// Insert a key-value pair into a Map
    MapInsert {
        // TODO
    },

    /// Insert an element into a Set
    SetInsert {
        // TODO
    },

    /// Require a frame to be fully initialized, update parent's iset
    RequireInit {
        // TODO
    },
}

/// Deferred frame
struct DFrame {
    // TODO: fill with useful info
}

// =============================================================================
// Deferred Mode
// =============================================================================

/// Deferred mode partial - stores frames, defers validation until FinishDeferred
pub(crate) struct DeferredPartial {
    /// Storage for frames
    arena: FrameArena,

    /// Current state of frames
    stack: Vec<FrameId>,

    /// Frames stored when popped, available for re-entry
    stored_frames: BTreeMap<Path, FrameId>,

    /// Tasks to execute on FinishDeferred, processed deepest-first
    // TODO: use a data structure that keeps it sorted?
    tasks: Vec<Task>,

    /// The path where deferred mode was started
    base_path: Path,
}

impl DeferredPartial {
    /// Process a batch of operations (no BeginDeferred/FinishDeferred)
    fn submit(&mut self, ops: &[PartialOp<'_>]) -> Result<(), ReflectError> {
        for op in ops {
            match op {
                // Scalars
                PartialOp::Set { .. } => todo!(),

                // Structs (check stored_frames for existing frame at path)
                PartialOp::BeginField { .. } => todo!(),
                PartialOp::BeginNthField { .. } => todo!(),

                // Enums
                PartialOp::SelectVariant { .. } => todo!(),
                PartialOp::SelectNthVariant { .. } => todo!(),

                // Options
                PartialOp::BeginSome => todo!(),
                PartialOp::SetNone => todo!(),

                // Results
                PartialOp::BeginOk => todo!(),
                PartialOp::BeginErr => todo!(),

                // Lists
                PartialOp::InitList => todo!(),
                PartialOp::BeginListItem => todo!(),

                // Arrays
                PartialOp::InitArray => todo!(),

                // Maps
                PartialOp::InitMap => todo!(),
                PartialOp::BeginKey => todo!(),
                PartialOp::BeginValue => todo!(),

                // Sets
                PartialOp::InitSet => todo!(),
                PartialOp::BeginSetItem => todo!(),

                // Smart pointers
                PartialOp::BeginSmartPtr => todo!(),
                PartialOp::BeginInner => todo!(),

                // Defaults
                PartialOp::SetDefault => todo!(),
                PartialOp::SetNthFieldToDefault { .. } => todo!(),

                // Parsing
                PartialOp::ParseFromStr { .. } => todo!(),

                // Navigation (store frame by path, queue DeferredOp for maps)
                PartialOp::End => todo!(),

                // Mode switches (handled by Partial wrapper)
                PartialOp::BeginDeferred | PartialOp::FinishDeferred => {
                    unreachable!("mode switches handled by Partial::submit")
                }
            }
        }
        Ok(())
    }

    /// Finish deferred mode: process all deferred ops, validate
    fn finish(&mut self) -> Result<ImmediatePartial, ReflectError> {
        // Sort deferred ops by depth, deepest first
        self.tasks.sort_by_key(|task| std::cmp::Reverse(task.depth));

        // Process each task
        for task in std::mem::take(&mut self.tasks) {
            match task.kind {
                TaskKind::MapInsert { .. } => {
                    // fill_defaults on the value frame
                    // then insert into map
                    // then dealloc key/value buffers
                    todo!()
                }
                TaskKind::SetInsert { .. } => {
                    // fill_defaults on the element frame
                    // then insert into set
                    // then dealloc element buffer
                    todo!()
                }
                TaskKind::RequireInit { .. } => {
                    // fill_defaults
                    // require_full_initialization
                    // update parent's iset
                    todo!()
                }
            }
        }

        // Return the inner ImmediatePartial (which now has the validated state)
        todo!()
    }
}
