//! Verified abstractions for Trame's state machines.
//!
//! This module provides traits that abstract over the unsafe operations in Trame,
//! allowing us to verify state machine correctness with Kani while the production
//! code performs actual memory operations.

/// The state of a single field in a struct frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldState {
    /// Field has not been initialized.
    NotStarted,
    /// Field is being built by a child frame.
    InProgress,
    /// Field has been fully initialized.
    Complete,
}

/// Abstract interface for struct field storage.
///
/// Production implementation performs actual memory operations.
/// Kani implementation just tracks states and verifies invariants.
pub trait StructStorage {
    /// Number of fields in this struct.
    fn field_count(&self) -> usize;

    /// Get the current state of a field.
    fn field_state(&self, idx: usize) -> FieldState;

    /// Begin writing to a field (for immediate set).
    ///
    /// # Preconditions
    /// - `idx < field_count()`
    ///
    /// # Effects
    /// - If field was Complete, drops the existing value
    /// - Field becomes NotStarted (ready for write)
    fn prepare_field(&mut self, idx: usize);

    /// Complete a field write (mark as initialized).
    ///
    /// # Preconditions
    /// - `idx < field_count()`
    /// - Field is NotStarted (prepare_field was called, or never touched)
    ///
    /// # Effects
    /// - Field becomes Complete
    fn complete_field(&mut self, idx: usize);

    /// Begin staging a field (for incremental construction).
    ///
    /// # Preconditions
    /// - `idx < field_count()`
    ///
    /// # Effects
    /// - If field was Complete, drops the existing value
    /// - Field becomes InProgress
    fn begin_field(&mut self, idx: usize);

    /// End staging a field (child frame completed).
    ///
    /// # Preconditions
    /// - `idx < field_count()`
    /// - Field is InProgress
    ///
    /// # Effects
    /// - Field becomes Complete
    fn end_field(&mut self, idx: usize);

    /// Drop a field and mark as not started.
    ///
    /// # Preconditions
    /// - `idx < field_count()`
    ///
    /// # Effects
    /// - If field was Complete, drops the value
    /// - Field becomes NotStarted
    fn drop_field(&mut self, idx: usize);

    /// Check if all fields are complete.
    fn all_complete(&self) -> bool;
}

/// Maximum number of fields supported in verified storage.
pub const MAX_FIELDS: usize = 8;

/// A verified struct storage that just tracks field states.
///
/// This is used by Kani to verify the state machine without
/// performing any actual memory operations.
///
/// Uses a fixed-size array to avoid Vec overhead in Kani proofs.
#[derive(Debug, Clone, Copy)]
pub struct VerifiedStructStorage {
    fields: [FieldState; MAX_FIELDS],
    len: usize,
}

impl VerifiedStructStorage {
    pub fn new(field_count: usize) -> Self {
        assert!(field_count <= MAX_FIELDS);
        Self {
            fields: [FieldState::NotStarted; MAX_FIELDS],
            len: field_count,
        }
    }
}

impl StructStorage for VerifiedStructStorage {
    fn field_count(&self) -> usize {
        self.len
    }

    fn field_state(&self, idx: usize) -> FieldState {
        self.fields[idx]
    }

    fn prepare_field(&mut self, idx: usize) {
        // If complete, we'd drop here - just mark not started
        self.fields[idx] = FieldState::NotStarted;
    }

    fn complete_field(&mut self, idx: usize) {
        assert_eq!(
            self.fields[idx],
            FieldState::NotStarted,
            "complete_field requires NotStarted"
        );
        self.fields[idx] = FieldState::Complete;
    }

    fn begin_field(&mut self, idx: usize) {
        // If complete, we'd drop here first
        self.fields[idx] = FieldState::InProgress;
    }

    fn end_field(&mut self, idx: usize) {
        assert_eq!(
            self.fields[idx],
            FieldState::InProgress,
            "end_field requires InProgress"
        );
        self.fields[idx] = FieldState::Complete;
    }

    fn drop_field(&mut self, idx: usize) {
        // If complete, we'd drop here - just mark not started
        self.fields[idx] = FieldState::NotStarted;
    }

    fn all_complete(&self) -> bool {
        let mut i: usize = 0;
        while i < self.len {
            if self.fields[i] != FieldState::Complete {
                return false;
            }
            i += 1;
        }
        true
    }
}

/// Operations that can be applied to a struct frame.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum StructOp {
    /// Set field `idx` immediately (like Source::Imm or Source::Default).
    SetField { idx: usize },
    /// Begin staging field `idx` (like Source::Stage).
    BeginField { idx: usize },
    /// End the current child frame (marks parent field complete).
    EndField { idx: usize },
}

/// Result of applying an operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpResult {
    /// Operation succeeded.
    Ok,
    /// Field index out of bounds.
    OutOfBounds,
    /// Tried to end a field that wasn't in progress.
    NotInProgress,
}

/// Apply an operation to a struct storage, returning success or error.
pub fn apply_struct_op<S: StructStorage>(storage: &mut S, op: StructOp) -> OpResult {
    match op {
        StructOp::SetField { idx } => {
            if idx >= storage.field_count() {
                return OpResult::OutOfBounds;
            }
            storage.prepare_field(idx);
            storage.complete_field(idx);
            OpResult::Ok
        }
        StructOp::BeginField { idx } => {
            if idx >= storage.field_count() {
                return OpResult::OutOfBounds;
            }
            storage.begin_field(idx);
            OpResult::Ok
        }
        StructOp::EndField { idx } => {
            if idx >= storage.field_count() {
                return OpResult::OutOfBounds;
            }
            if storage.field_state(idx) != FieldState::InProgress {
                return OpResult::NotInProgress;
            }
            storage.end_field(idx);
            OpResult::Ok
        }
    }
}

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    /// Verify: SetField on a valid index always succeeds and marks the field complete.
    #[kani::proof]
    fn verify_set_field_succeeds() {
        let field_count: usize = kani::any();
        kani::assume(field_count > 0 && field_count <= 4);

        let idx: usize = kani::any();
        kani::assume(idx < field_count);

        let mut storage = VerifiedStructStorage::new(field_count);

        let result = apply_struct_op(&mut storage, StructOp::SetField { idx });

        kani::assert(result == OpResult::Ok, "SetField on valid index succeeds");
        kani::assert(
            storage.field_state(idx) == FieldState::Complete,
            "SetField marks field complete",
        );
    }

    /// Verify: SetField on out-of-bounds index returns error.
    #[kani::proof]
    fn verify_set_field_bounds() {
        let field_count: usize = kani::any();
        kani::assume(field_count > 0 && field_count <= 4);

        let idx: usize = kani::any();
        kani::assume(idx >= field_count);

        let mut storage = VerifiedStructStorage::new(field_count);

        let result = apply_struct_op(&mut storage, StructOp::SetField { idx });

        kani::assert(
            result == OpResult::OutOfBounds,
            "SetField on invalid index fails",
        );
    }

    /// Verify: BeginField followed by EndField marks field complete.
    #[kani::proof]
    fn verify_begin_end_field() {
        let field_count: usize = kani::any();
        kani::assume(field_count > 0 && field_count <= 4);

        let idx: usize = kani::any();
        kani::assume(idx < field_count);

        let mut storage = VerifiedStructStorage::new(field_count);

        let r1 = apply_struct_op(&mut storage, StructOp::BeginField { idx });
        kani::assert(r1 == OpResult::Ok, "BeginField succeeds");
        kani::assert(
            storage.field_state(idx) == FieldState::InProgress,
            "BeginField marks field in progress",
        );

        let r2 = apply_struct_op(&mut storage, StructOp::EndField { idx });
        kani::assert(r2 == OpResult::Ok, "EndField succeeds");
        kani::assert(
            storage.field_state(idx) == FieldState::Complete,
            "EndField marks field complete",
        );
    }

    /// Verify: EndField without BeginField returns error.
    #[kani::proof]
    fn verify_end_without_begin_fails() {
        let field_count: usize = kani::any();
        kani::assume(field_count > 0 && field_count <= 4);

        let idx: usize = kani::any();
        kani::assume(idx < field_count);

        let mut storage = VerifiedStructStorage::new(field_count);

        let result = apply_struct_op(&mut storage, StructOp::EndField { idx });

        kani::assert(
            result == OpResult::NotInProgress,
            "EndField without BeginField fails",
        );
    }

    /// Verify: Setting all fields makes all_complete() return true.
    #[kani::proof]
    #[kani::unwind(16)]
    fn verify_all_complete() {
        let field_count: usize = kani::any();
        kani::assume(field_count > 0 && field_count <= MAX_FIELDS);

        let mut storage = VerifiedStructStorage::new(field_count);

        // Set all fields
        for i in 0..field_count {
            let _ = apply_struct_op(&mut storage, StructOp::SetField { idx: i });
        }

        kani::assert(
            storage.all_complete(),
            "all_complete is true after setting all fields",
        );
    }

    /// Verify: Partial completion means all_complete() returns false.
    #[kani::proof]
    #[kani::unwind(16)]
    fn verify_partial_not_complete() {
        let field_count: usize = kani::any();
        kani::assume(field_count >= 2 && field_count <= MAX_FIELDS);

        let skip_idx: usize = kani::any();
        kani::assume(skip_idx < field_count);

        let mut storage = VerifiedStructStorage::new(field_count);

        // Set all fields except skip_idx
        for i in 0..field_count {
            if i != skip_idx {
                let _ = apply_struct_op(&mut storage, StructOp::SetField { idx: i });
            }
        }

        kani::assert(
            !storage.all_complete(),
            "all_complete is false when one field missing",
        );
    }

    /// Verify: Re-setting a field is idempotent (field stays complete).
    #[kani::proof]
    fn verify_reset_field() {
        let field_count: usize = kani::any();
        kani::assume(field_count > 0 && field_count <= 4);

        let idx: usize = kani::any();
        kani::assume(idx < field_count);

        let mut storage = VerifiedStructStorage::new(field_count);

        // Set once
        let _ = apply_struct_op(&mut storage, StructOp::SetField { idx });
        kani::assert(
            storage.field_state(idx) == FieldState::Complete,
            "first set",
        );

        // Set again (overwrite)
        let _ = apply_struct_op(&mut storage, StructOp::SetField { idx });
        kani::assert(
            storage.field_state(idx) == FieldState::Complete,
            "second set also complete",
        );
    }

    /// Helper to constrain a StructOp's index to be within bounds.
    fn constrain_op_index(op: StructOp, field_count: usize) -> StructOp {
        match op {
            StructOp::SetField { idx } => StructOp::SetField {
                idx: idx % field_count,
            },
            StructOp::BeginField { idx } => StructOp::BeginField {
                idx: idx % field_count,
            },
            StructOp::EndField { idx } => StructOp::EndField {
                idx: idx % field_count,
            },
        }
    }

    /// Verify: Any sequence of 3 ops maintains consistent state.
    ///
    /// This is the key property: we throw symbolic ops at the state machine
    /// and verify it never panics and maintains its invariants.
    #[kani::proof]
    fn verify_op_sequence_3() {
        // Fixed field count to reduce state space
        const FIELD_COUNT: usize = 3;

        let mut storage = VerifiedStructStorage::new(FIELD_COUNT);

        // Generate all ops upfront (outside any loop)
        let op1: StructOp = kani::any();
        let op2: StructOp = kani::any();
        let op3: StructOp = kani::any();

        // Constrain indices to valid range
        let op1 = constrain_op_index(op1, FIELD_COUNT);
        let op2 = constrain_op_index(op2, FIELD_COUNT);
        let op3 = constrain_op_index(op3, FIELD_COUNT);

        // Apply ops sequentially
        let _ = apply_struct_op(&mut storage, op1);
        let _ = apply_struct_op(&mut storage, op2);
        let _ = apply_struct_op(&mut storage, op3);

        // Final invariant: all_complete consistency
        let manual_check = storage.field_state(0) == FieldState::Complete
            && storage.field_state(1) == FieldState::Complete
            && storage.field_state(2) == FieldState::Complete;
        kani::assert(
            storage.all_complete() == manual_check,
            "all_complete matches manual check",
        );
    }

    /// Verify: A longer sequence (5 ops) also maintains invariants.
    #[kani::proof]
    fn verify_op_sequence_5() {
        const FIELD_COUNT: usize = 2;

        let mut storage = VerifiedStructStorage::new(FIELD_COUNT);

        // Generate all ops upfront
        let op1: StructOp = kani::any();
        let op2: StructOp = kani::any();
        let op3: StructOp = kani::any();
        let op4: StructOp = kani::any();
        let op5: StructOp = kani::any();

        // Constrain indices
        let op1 = constrain_op_index(op1, FIELD_COUNT);
        let op2 = constrain_op_index(op2, FIELD_COUNT);
        let op3 = constrain_op_index(op3, FIELD_COUNT);
        let op4 = constrain_op_index(op4, FIELD_COUNT);
        let op5 = constrain_op_index(op5, FIELD_COUNT);

        // Apply ops
        let _ = apply_struct_op(&mut storage, op1);
        let _ = apply_struct_op(&mut storage, op2);
        let _ = apply_struct_op(&mut storage, op3);
        let _ = apply_struct_op(&mut storage, op4);
        let _ = apply_struct_op(&mut storage, op5);

        // all_complete consistency
        let manual_check = storage.field_state(0) == FieldState::Complete
            && storage.field_state(1) == FieldState::Complete;
        kani::assert(
            storage.all_complete() == manual_check,
            "all_complete matches manual check",
        );
    }
}

// =============================================================================
// Frame Tree Model
// =============================================================================
//
// Models the parent-child relationship between frames in Trame.
// This is the core abstraction that tracks where we are in the build tree.

/// Maximum number of frames in the arena.
pub const MAX_FRAMES: usize = 16;

/// Index into the frame arena. Uses sentinel values for invalid states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameIdx(u8);

impl FrameIdx {
    pub const INVALID: FrameIdx = FrameIdx(u8::MAX);
    pub const ROOT: FrameIdx = FrameIdx(0);

    pub fn new(idx: usize) -> Self {
        assert!(idx < MAX_FRAMES);
        FrameIdx(idx as u8)
    }

    pub fn is_valid(self) -> bool {
        (self.0 as usize) < MAX_FRAMES
    }

    pub fn as_usize(self) -> usize {
        self.0 as usize
    }
}

/// What kind of frame this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameKindModel {
    /// A struct with N fields.
    Struct { field_count: usize },
    /// A scalar value (no children).
    Scalar,
}

/// Link back to parent frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParentLinkModel {
    /// This is the root frame.
    Root,
    /// This is a struct field of the parent.
    StructField { parent: FrameIdx, field_idx: usize },
}

/// A single frame in the tree.
#[derive(Debug, Clone, Copy)]
pub struct FrameModel {
    /// What kind of frame this is.
    pub kind: FrameKindModel,
    /// Link to parent (or Root if this is the root).
    pub parent_link: ParentLinkModel,
    /// Field states (only used for Struct frames).
    pub fields: [FieldState; MAX_FIELDS],
    /// Is this frame slot occupied?
    pub occupied: bool,
}

impl FrameModel {
    pub fn empty() -> Self {
        Self {
            kind: FrameKindModel::Scalar,
            parent_link: ParentLinkModel::Root,
            fields: [FieldState::NotStarted; MAX_FIELDS],
            occupied: false,
        }
    }

    pub fn new_struct(field_count: usize, parent_link: ParentLinkModel) -> Self {
        Self {
            kind: FrameKindModel::Struct { field_count },
            parent_link,
            fields: [FieldState::NotStarted; MAX_FIELDS],
            occupied: true,
        }
    }

    pub fn new_scalar(parent_link: ParentLinkModel) -> Self {
        Self {
            kind: FrameKindModel::Scalar,
            parent_link,
            fields: [FieldState::NotStarted; MAX_FIELDS],
            occupied: true,
        }
    }
}

/// The frame tree - an arena of frames plus a current pointer.
#[derive(Debug, Clone)]
pub struct FrameTree {
    /// All frames in the arena.
    pub frames: [FrameModel; MAX_FRAMES],
    /// Index of the current frame we're building.
    pub current: FrameIdx,
    /// Next free slot in the arena.
    pub next_free: usize,
}

impl FrameTree {
    /// Create a new frame tree with a root struct frame.
    pub fn new(root_field_count: usize) -> Self {
        let mut frames = [FrameModel::empty(); MAX_FRAMES];
        frames[0] = FrameModel::new_struct(root_field_count, ParentLinkModel::Root);
        Self {
            frames,
            current: FrameIdx::ROOT,
            next_free: 1,
        }
    }

    /// Allocate a new frame, returns its index.
    pub fn alloc(&mut self, frame: FrameModel) -> Option<FrameIdx> {
        if self.next_free >= MAX_FRAMES {
            return None;
        }
        let idx = FrameIdx::new(self.next_free);
        self.frames[self.next_free] = frame;
        self.next_free += 1;
        Some(idx)
    }

    /// Get a reference to a frame.
    pub fn get(&self, idx: FrameIdx) -> &FrameModel {
        assert!(idx.is_valid());
        &self.frames[idx.as_usize()]
    }

    /// Get a mutable reference to a frame.
    pub fn get_mut(&mut self, idx: FrameIdx) -> &mut FrameModel {
        assert!(idx.is_valid());
        &mut self.frames[idx.as_usize()]
    }

    /// Get the current frame.
    pub fn current_frame(&self) -> &FrameModel {
        self.get(self.current)
    }

    /// Get the current frame mutably.
    pub fn current_frame_mut(&mut self) -> &mut FrameModel {
        self.get_mut(self.current)
    }
}

/// Operations on the frame tree.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum TreeOp {
    /// Set field `idx` of current frame immediately.
    SetField { idx: usize },
    /// Begin building field `idx` as a child struct with `child_fields` fields.
    BeginStructField { idx: usize, child_fields: usize },
    /// Begin building field `idx` as a scalar.
    BeginScalarField { idx: usize },
    /// End the current frame, return to parent.
    End,
}

/// Result of a tree operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeOpResult {
    Ok,
    OutOfBounds,
    NotAStruct,
    FieldNotInProgress,
    AtRoot,
    ArenaFull,
}

/// Apply an operation to the frame tree.
pub fn apply_tree_op(tree: &mut FrameTree, op: TreeOp) -> TreeOpResult {
    match op {
        TreeOp::SetField { idx } => {
            let frame = tree.current_frame_mut();
            let FrameKindModel::Struct { field_count } = frame.kind else {
                return TreeOpResult::NotAStruct;
            };
            if idx >= field_count {
                return TreeOpResult::OutOfBounds;
            }
            // prepare + complete
            frame.fields[idx] = FieldState::Complete;
            TreeOpResult::Ok
        }

        TreeOp::BeginStructField { idx, child_fields } => {
            let current_idx = tree.current;
            let frame = tree.current_frame_mut();
            let FrameKindModel::Struct { field_count } = frame.kind else {
                return TreeOpResult::NotAStruct;
            };
            if idx >= field_count {
                return TreeOpResult::OutOfBounds;
            }
            // Mark field as in progress
            frame.fields[idx] = FieldState::InProgress;

            // Allocate child frame
            let child = FrameModel::new_struct(
                child_fields,
                ParentLinkModel::StructField {
                    parent: current_idx,
                    field_idx: idx,
                },
            );
            let Some(child_idx) = tree.alloc(child) else {
                return TreeOpResult::ArenaFull;
            };
            tree.current = child_idx;
            TreeOpResult::Ok
        }

        TreeOp::BeginScalarField { idx } => {
            let current_idx = tree.current;
            let frame = tree.current_frame_mut();
            let FrameKindModel::Struct { field_count } = frame.kind else {
                return TreeOpResult::NotAStruct;
            };
            if idx >= field_count {
                return TreeOpResult::OutOfBounds;
            }
            // Mark field as in progress
            frame.fields[idx] = FieldState::InProgress;

            // Allocate child frame
            let child = FrameModel::new_scalar(ParentLinkModel::StructField {
                parent: current_idx,
                field_idx: idx,
            });
            let Some(child_idx) = tree.alloc(child) else {
                return TreeOpResult::ArenaFull;
            };
            tree.current = child_idx;
            TreeOpResult::Ok
        }

        TreeOp::End => {
            let frame = tree.current_frame();
            let ParentLinkModel::StructField { parent, field_idx } = frame.parent_link else {
                return TreeOpResult::AtRoot;
            };

            // Mark parent's field as complete
            let parent_frame = tree.get_mut(parent);
            if parent_frame.fields[field_idx] != FieldState::InProgress {
                return TreeOpResult::FieldNotInProgress;
            }
            parent_frame.fields[field_idx] = FieldState::Complete;

            // Move current back to parent
            tree.current = parent;
            TreeOpResult::Ok
        }
    }
}

#[cfg(kani)]
mod tree_proofs {
    use super::*;

    /// Verify: After Begin + End, we're back at the same frame with field complete.
    #[kani::proof]
    fn verify_begin_end_returns_to_parent() {
        let root_fields: usize = kani::any();
        kani::assume(root_fields > 0 && root_fields <= MAX_FIELDS);

        let field_idx: usize = kani::any();
        kani::assume(field_idx < root_fields);

        let child_fields: usize = kani::any();
        kani::assume(child_fields > 0 && child_fields <= MAX_FIELDS);

        let mut tree = FrameTree::new(root_fields);
        let original_current = tree.current;

        // Begin a child struct
        let r1 = apply_tree_op(
            &mut tree,
            TreeOp::BeginStructField {
                idx: field_idx,
                child_fields,
            },
        );
        kani::assert(r1 == TreeOpResult::Ok, "BeginStructField succeeds");
        kani::assert(tree.current != original_current, "current changed");

        // End back to parent
        let r2 = apply_tree_op(&mut tree, TreeOp::End);
        kani::assert(r2 == TreeOpResult::Ok, "End succeeds");
        kani::assert(tree.current == original_current, "back to original");

        // Field should be complete
        let frame = tree.get(original_current);
        kani::assert(
            frame.fields[field_idx] == FieldState::Complete,
            "field is complete after End",
        );
    }

    /// Verify: End at root returns AtRoot error.
    #[kani::proof]
    fn verify_end_at_root_fails() {
        let root_fields: usize = kani::any();
        kani::assume(root_fields > 0 && root_fields <= MAX_FIELDS);

        let mut tree = FrameTree::new(root_fields);

        let result = apply_tree_op(&mut tree, TreeOp::End);
        kani::assert(result == TreeOpResult::AtRoot, "End at root fails");
    }

    /// Verify: current always points to a valid, occupied frame.
    #[kani::proof]
    #[kani::unwind(8)]
    fn verify_current_always_valid() {
        let root_fields: usize = kani::any();
        kani::assume(root_fields > 0 && root_fields <= 4);

        let mut tree = FrameTree::new(root_fields);

        // Apply a sequence of operations
        for _ in 0..3 {
            let op_kind: u8 = kani::any();
            let idx: usize = kani::any();
            kani::assume(idx < root_fields);

            let child_fields: usize = kani::any();
            kani::assume(child_fields > 0 && child_fields <= 4);

            let op = match op_kind % 4 {
                0 => TreeOp::SetField { idx },
                1 => TreeOp::BeginStructField { idx, child_fields },
                2 => TreeOp::BeginScalarField { idx },
                _ => TreeOp::End,
            };

            let _ = apply_tree_op(&mut tree, op);

            // Invariant: current is always valid and occupied
            kani::assert(tree.current.is_valid(), "current is valid index");
            kani::assert(
                tree.frames[tree.current.as_usize()].occupied,
                "current frame is occupied",
            );
        }
    }

    /// Verify: Parent link always points to a valid frame.
    #[kani::proof]
    fn verify_parent_link_valid() {
        let root_fields: usize = kani::any();
        kani::assume(root_fields > 0 && root_fields <= MAX_FIELDS);

        let field_idx: usize = kani::any();
        kani::assume(field_idx < root_fields);

        let child_fields: usize = kani::any();
        kani::assume(child_fields > 0 && child_fields <= MAX_FIELDS);

        let mut tree = FrameTree::new(root_fields);

        // Begin a child
        let _ = apply_tree_op(
            &mut tree,
            TreeOp::BeginStructField {
                idx: field_idx,
                child_fields,
            },
        );

        // Check that child's parent link points to a valid frame
        let child_frame = tree.current_frame();
        if let ParentLinkModel::StructField { parent, .. } = child_frame.parent_link {
            kani::assert(parent.is_valid(), "parent index is valid");
            kani::assert(
                tree.frames[parent.as_usize()].occupied,
                "parent frame is occupied",
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_set_field() {
        let mut storage = VerifiedStructStorage::new(3);

        assert_eq!(storage.field_state(0), FieldState::NotStarted);
        assert_eq!(storage.field_state(1), FieldState::NotStarted);
        assert_eq!(storage.field_state(2), FieldState::NotStarted);
        assert!(!storage.all_complete());

        apply_struct_op(&mut storage, StructOp::SetField { idx: 0 });
        assert_eq!(storage.field_state(0), FieldState::Complete);
        assert!(!storage.all_complete());

        apply_struct_op(&mut storage, StructOp::SetField { idx: 1 });
        apply_struct_op(&mut storage, StructOp::SetField { idx: 2 });
        assert!(storage.all_complete());
    }

    #[test]
    fn test_begin_end_field() {
        let mut storage = VerifiedStructStorage::new(2);

        apply_struct_op(&mut storage, StructOp::BeginField { idx: 0 });
        assert_eq!(storage.field_state(0), FieldState::InProgress);

        apply_struct_op(&mut storage, StructOp::EndField { idx: 0 });
        assert_eq!(storage.field_state(0), FieldState::Complete);
    }

    #[test]
    fn test_end_without_begin_fails() {
        let mut storage = VerifiedStructStorage::new(2);

        let result = apply_struct_op(&mut storage, StructOp::EndField { idx: 0 });
        assert_eq!(result, OpResult::NotInProgress);
    }

    #[test]
    fn test_overwrite_complete_field() {
        let mut storage = VerifiedStructStorage::new(2);

        // Set field
        apply_struct_op(&mut storage, StructOp::SetField { idx: 0 });
        assert_eq!(storage.field_state(0), FieldState::Complete);

        // Set again (should prepare then complete)
        apply_struct_op(&mut storage, StructOp::SetField { idx: 0 });
        assert_eq!(storage.field_state(0), FieldState::Complete);
    }

    // Frame tree tests

    #[test]
    fn test_frame_tree_basic() {
        let mut tree = FrameTree::new(3);
        assert_eq!(tree.current, FrameIdx::ROOT);

        // Set a field directly
        let r = apply_tree_op(&mut tree, TreeOp::SetField { idx: 0 });
        assert_eq!(r, TreeOpResult::Ok);
        assert_eq!(tree.current_frame().fields[0], FieldState::Complete);
    }

    #[test]
    fn test_frame_tree_begin_end() {
        let mut tree = FrameTree::new(2);

        // Begin a child struct
        let r1 = apply_tree_op(
            &mut tree,
            TreeOp::BeginStructField {
                idx: 0,
                child_fields: 3,
            },
        );
        assert_eq!(r1, TreeOpResult::Ok);
        assert_ne!(tree.current, FrameIdx::ROOT);

        // We're now in the child - set its fields
        let r2 = apply_tree_op(&mut tree, TreeOp::SetField { idx: 0 });
        assert_eq!(r2, TreeOpResult::Ok);

        // End back to parent
        let r3 = apply_tree_op(&mut tree, TreeOp::End);
        assert_eq!(r3, TreeOpResult::Ok);
        assert_eq!(tree.current, FrameIdx::ROOT);

        // Parent's field 0 should be complete
        assert_eq!(tree.current_frame().fields[0], FieldState::Complete);
    }

    #[test]
    fn test_frame_tree_nested() {
        let mut tree = FrameTree::new(2);

        // Root -> child1
        apply_tree_op(
            &mut tree,
            TreeOp::BeginStructField {
                idx: 0,
                child_fields: 2,
            },
        );
        let child1 = tree.current;

        // child1 -> child2
        apply_tree_op(
            &mut tree,
            TreeOp::BeginStructField {
                idx: 0,
                child_fields: 1,
            },
        );
        let child2 = tree.current;
        assert_ne!(child2, child1);

        // End child2 -> back to child1
        apply_tree_op(&mut tree, TreeOp::End);
        assert_eq!(tree.current, child1);

        // End child1 -> back to root
        apply_tree_op(&mut tree, TreeOp::End);
        assert_eq!(tree.current, FrameIdx::ROOT);
    }

    #[test]
    fn test_frame_tree_end_at_root_fails() {
        let mut tree = FrameTree::new(2);
        let r = apply_tree_op(&mut tree, TreeOp::End);
        assert_eq!(r, TreeOpResult::AtRoot);
    }
}
