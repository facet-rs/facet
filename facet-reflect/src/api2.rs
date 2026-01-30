//! Partial v2 - Clean separation of Immediate and Deferred modes
//!
//! This is a sketch/design doc for refactoring Partial.

use std::collections::BTreeMap;

use facet_core::{MapDef, SetDef, Shape};
use facet_path::Path;

// =============================================================================
// Operations
// =============================================================================

/// An operation to execute on a Partial.
///
/// Operations are processed in batches via `submit()`. Pointers in `Set` ops
/// are only valid during the `submit()` call - data is copied immediately.
#[derive(Clone, Copy)]
pub enum PartialOp<'a> {
    // -------------------------------------------------------------------------
    // Scalars
    // -------------------------------------------------------------------------
    /// Set the current value (type-erased pointer + shape)
    Set {
        ptr: *const (),
        shape: &'static Shape,
    },

    // -------------------------------------------------------------------------
    // Structs
    // -------------------------------------------------------------------------
    /// Begin a struct field by name
    BeginField { name: &'a str },

    /// Begin a struct field by index
    BeginNthField { index: usize },

    // -------------------------------------------------------------------------
    // Enums
    // -------------------------------------------------------------------------
    /// Select an enum variant by name
    SelectVariant { name: &'a str },

    // -------------------------------------------------------------------------
    // Options
    // -------------------------------------------------------------------------
    /// Begin the Some variant of an Option
    BeginSome,

    /// Set an Option to None
    SetNone,

    // -------------------------------------------------------------------------
    // Lists (Vec, etc.)
    // -------------------------------------------------------------------------
    /// Initialize a list
    InitList,

    /// Begin a list item
    BeginListItem,

    // -------------------------------------------------------------------------
    // Maps (HashMap, etc.)
    // -------------------------------------------------------------------------
    /// Initialize a map
    InitMap,

    /// Begin a map key
    BeginKey,

    /// Begin a map value
    BeginValue,

    // -------------------------------------------------------------------------
    // Navigation
    // -------------------------------------------------------------------------
    /// End the current frame, return to parent
    End,

    // -------------------------------------------------------------------------
    // Mode switches
    // -------------------------------------------------------------------------
    /// Enter deferred mode
    BeginDeferred,

    /// Exit deferred mode, validate everything
    FinishDeferred,
}

// =============================================================================
// Frame Arena
// =============================================================================

type FrameId = usize;

struct Frame {
    // TODO: contents from existing Frame
}

struct FrameArena {
    frames: Vec<Frame>,
}

impl FrameArena {
    fn new() -> Self {
        Self { frames: Vec::new() }
    }

    fn alloc(&mut self, frame: Frame) -> FrameId {
        let id = self.frames.len();
        self.frames.push(frame);
        id
    }

    fn get(&self, id: FrameId) -> &Frame {
        &self.frames[id]
    }

    fn get_mut(&mut self, id: FrameId) -> &mut Frame {
        &mut self.frames[id]
    }
}

// =============================================================================
// Deferred Operations
// =============================================================================

/// Operations that are deferred until FinishDeferred is processed.
/// Processed deepest-first.
enum DeferredOp {
    /// Insert a key-value pair into a Map
    MapInsert {
        map_ptr: PtrUninit,
        map_def: MapDef,
        key_ptr: PtrUninit,
        value_frame: FrameId,
        depth: usize,
    },

    /// Insert an element into a Set
    SetInsert {
        set_ptr: PtrUninit,
        set_def: SetDef,
        element_frame: FrameId,
        depth: usize,
    },

    /// Require a frame to be fully initialized, update parent's iset
    RequireInit {
        frame: FrameId,
        parent_frame: FrameId,
        field_idx: usize,
        depth: usize,
    },
}

impl DeferredOp {
    fn depth(&self) -> usize {
        match self {
            DeferredOp::MapInsert { depth, .. } => *depth,
            DeferredOp::SetInsert { depth, .. } => *depth,
            DeferredOp::RequireInit { depth, .. } => *depth,
        }
    }
}

// =============================================================================
// Immediate Mode
// =============================================================================

/// Immediate mode partial - validates on End, no frame storage
struct ImmediatePartial<'facet, const BORROW: bool> {
    arena: FrameArena,
    stack: Vec<FrameId>,
    root_plan: std::sync::Arc<crate::typeplan::TypePlanCore>,
    state: PartialState,
    _marker: std::marker::PhantomData<&'facet ()>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PartialState {
    Active,
    Built,
    Poisoned,
}

impl<'facet, const BORROW: bool> ImmediatePartial<'facet, BORROW> {
    /// Process a batch of operations (no BeginDeferred/FinishDeferred)
    fn submit(&mut self, ops: &[PartialOp<'_>]) -> Result<(), ReflectError> {
        for op in ops {
            match op {
                PartialOp::Set { ptr, shape } => {
                    // Copy value from ptr into current frame
                    todo!()
                }
                PartialOp::BeginField { name } => {
                    // Push a field frame onto stack
                    todo!()
                }
                PartialOp::BeginNthField { index } => {
                    // Push a field frame by index
                    todo!()
                }
                PartialOp::SelectVariant { name } => {
                    // Select enum variant
                    todo!()
                }
                PartialOp::BeginSome => {
                    // Begin Option::Some
                    todo!()
                }
                PartialOp::SetNone => {
                    // Set Option to None
                    todo!()
                }
                PartialOp::InitList => {
                    // Initialize a list
                    todo!()
                }
                PartialOp::BeginListItem => {
                    // Push a list item frame
                    todo!()
                }
                PartialOp::InitMap => {
                    // Initialize a map
                    todo!()
                }
                PartialOp::BeginKey => {
                    // Push a map key frame
                    todo!()
                }
                PartialOp::BeginValue => {
                    // Push a map value frame
                    todo!()
                }
                PartialOp::End => {
                    // Pop current frame, validate, merge into parent
                    todo!()
                }
                PartialOp::BeginDeferred | PartialOp::FinishDeferred => {
                    unreachable!("mode switches handled by Partial::submit")
                }
            }
        }
        Ok(())
    }
}

// =============================================================================
// Deferred Mode
// =============================================================================

/// Deferred mode partial - stores frames, defers validation until FinishDeferred
struct DeferredPartial<'facet, const BORROW: bool> {
    arena: FrameArena,
    stack: Vec<FrameId>,

    /// Frames stored when popped, available for re-entry
    stored_frames: BTreeMap<Path, FrameId>,

    /// Operations to execute on FinishDeferred, processed deepest-first
    deferred_ops: Vec<DeferredOp>,

    /// The path where deferred mode was started
    base_path: Path,

    /// The underlying immediate partial
    inner: ImmediatePartial<'facet, BORROW>,
}

impl<'facet, const BORROW: bool> DeferredPartial<'facet, BORROW> {
    /// Process a batch of operations (no BeginDeferred/FinishDeferred)
    fn submit(&mut self, ops: &[PartialOp<'_>]) -> Result<(), ReflectError> {
        for op in ops {
            match op {
                PartialOp::Set { ptr, shape } => {
                    // Copy value from ptr into current frame
                    todo!()
                }
                PartialOp::BeginField { name } => {
                    // Check stored_frames for existing frame at this path
                    // If found, restore it; otherwise push new frame
                    todo!()
                }
                PartialOp::BeginNthField { index } => {
                    // Same as BeginField but by index
                    todo!()
                }
                PartialOp::SelectVariant { name } => {
                    // Select enum variant
                    todo!()
                }
                PartialOp::BeginSome => {
                    // Begin Option::Some
                    todo!()
                }
                PartialOp::SetNone => {
                    // Set Option to None
                    todo!()
                }
                PartialOp::InitList => {
                    // Initialize a list
                    todo!()
                }
                PartialOp::BeginListItem => {
                    // Push a list item frame
                    todo!()
                }
                PartialOp::InitMap => {
                    // Initialize a map
                    todo!()
                }
                PartialOp::BeginKey => {
                    // Push a map key frame
                    todo!()
                }
                PartialOp::BeginValue => {
                    // Push a map value frame
                    todo!()
                }
                PartialOp::End => {
                    // Pop current frame, store it by path (don't validate yet)
                    // For maps: queue a DeferredOp::MapInsert
                    todo!()
                }
                PartialOp::BeginDeferred | PartialOp::FinishDeferred => {
                    unreachable!("mode switches handled by Partial::submit")
                }
            }
        }
        Ok(())
    }

    /// Finish deferred mode: process all deferred ops, validate
    fn finish(&mut self) -> Result<ImmediatePartial<'facet, BORROW>, ReflectError> {
        // Sort deferred ops by depth, deepest first
        self.deferred_ops
            .sort_by_key(|op| std::cmp::Reverse(op.depth()));

        // Process each deferred op
        for op in std::mem::take(&mut self.deferred_ops) {
            match op {
                DeferredOp::MapInsert {
                    map_ptr,
                    map_def,
                    key_ptr,
                    value_frame,
                    ..
                } => {
                    let frame = self.arena.get_mut(value_frame);
                    // fill_defaults on the value frame
                    // then insert into map
                    // then dealloc key/value buffers
                    todo!()
                }
                DeferredOp::SetInsert {
                    set_ptr,
                    set_def,
                    element_frame,
                    ..
                } => {
                    let frame = self.arena.get_mut(element_frame);
                    // fill_defaults on the element frame
                    // then insert into set
                    // then dealloc element buffer
                    todo!()
                }
                DeferredOp::RequireInit {
                    frame,
                    parent_frame,
                    field_idx,
                    ..
                } => {
                    let frame = self.arena.get_mut(frame);
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

// Placeholder types
#[derive(Clone, Copy)]
struct PtrUninit;
struct ReflectError;
