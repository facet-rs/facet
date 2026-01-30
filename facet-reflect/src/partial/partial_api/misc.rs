use facet_path::{Path, PathStep};

use super::*;
use crate::partial::{AllocatedShape, Frame, FrameOwnership};
use crate::typeplan::{self, DeserStrategy, TypePlanNodeKind};

////////////////////////////////////////////////////////////////////////////////////////////////////
// Misc.
////////////////////////////////////////////////////////////////////////////////////////////////////
impl<'facet, const BORROW: bool> Partial<'facet, BORROW> {
    /// Applies a closure to this Partial, enabling chaining with operations that
    /// take ownership and return `Result<Self, E>`.
    ///
    /// This is useful for chaining deserializer methods that need `&mut self`:
    ///
    /// ```ignore
    /// wip = wip
    ///     .begin_field("name")?
    ///     .with(|w| deserializer.deserialize_into(w))?
    ///     .end()?;
    /// ```
    #[inline]
    pub fn with<F, E>(self, f: F) -> Result<Self, E>
    where
        F: FnOnce(Self) -> Result<Self, E>,
    {
        f(self)
    }

    /// Returns true if the Partial is in an active state (not built or poisoned).
    ///
    /// After `build()` succeeds or after an error causes poisoning, the Partial
    /// becomes inactive and most operations will fail.
    #[inline]
    pub fn is_active(&self) -> bool {
        self.state == PartialState::Active
    }

    /// Returns the current frame count (depth of nesting)
    ///
    /// The initial frame count is 1 — `begin_field` would push a new frame,
    /// bringing it to 2, then `end` would bring it back to `1`.
    ///
    /// This is an implementation detail of `Partial`, kinda, but deserializers
    /// might use this for debug assertions, to make sure the state is what
    /// they think it is.
    #[inline]
    pub const fn frame_count(&self) -> usize {
        self.frames().len()
    }

    /// Returns the shape of the current frame.
    ///
    /// # Panics
    ///
    /// Panics if the Partial has been poisoned or built, or if there are no frames
    /// (which indicates a bug in the Partial implementation).
    #[inline]
    pub fn shape(&self) -> &'static Shape {
        if self.state != PartialState::Active {
            panic!(
                "Partial::shape() called on non-active Partial (state: {:?})",
                self.state
            );
        }
        self.frames()
            .last()
            .expect("Partial::shape() called but no frames exist - this is a bug")
            .allocated
            .shape()
    }

    /// Returns the shape of the current frame, or `None` if the Partial is
    /// inactive (poisoned or built) or has no frames.
    ///
    /// This is useful for debugging/logging where you want to inspect the state
    /// without risking a panic.
    #[inline]
    pub fn try_shape(&self) -> Option<&'static Shape> {
        if self.state != PartialState::Active {
            return None;
        }
        self.frames().last().map(|f| f.allocated.shape())
    }

    /// Returns the TypePlanCore for this Partial.
    ///
    /// This provides access to the arena-based type plan data, useful for
    /// resolving field lookups and accessing precomputed metadata.
    #[inline]
    pub fn type_plan_core(&self) -> &crate::typeplan::TypePlanCore {
        &self.root_plan
    }

    /// Returns the precomputed StructPlan for the current frame, if available.
    ///
    /// This provides O(1) or O(log n) field lookup instead of O(n) linear scanning.
    /// Returns `None` if:
    /// - The Partial is not active
    /// - The current frame has no TypePlan (e.g., custom deserialization frames)
    /// - The current type is not a struct
    #[inline]
    pub fn struct_plan(&self) -> Option<&crate::typeplan::StructPlan> {
        if self.state != PartialState::Active {
            return None;
        }
        let frame = self.frames().last()?;
        self.root_plan.struct_plan_by_id(frame.type_plan)
    }

    /// Returns the precomputed EnumPlan for the current frame, if available.
    ///
    /// This provides O(1) or O(log n) variant lookup instead of O(n) linear scanning.
    /// Returns `None` if:
    /// - The Partial is not active
    /// - The current type is not an enum
    #[inline]
    pub fn enum_plan(&self) -> Option<&crate::typeplan::EnumPlan> {
        if self.state != PartialState::Active {
            return None;
        }
        let frame = self.frames().last()?;
        self.root_plan.enum_plan_by_id(frame.type_plan)
    }

    /// Returns the precomputed field plans for the current frame.
    ///
    /// This provides access to precomputed validators and default handling without
    /// runtime attribute scanning.
    ///
    /// Returns `None` if the current type is not a struct or enum variant.
    #[inline]
    pub fn field_plans(&self) -> Option<&[crate::typeplan::FieldPlan]> {
        use crate::typeplan::TypePlanNodeKind;
        let frame = self.frames().last().unwrap();
        let node = self.root_plan.node(frame.type_plan);
        match &node.kind {
            TypePlanNodeKind::Struct(struct_plan) => {
                Some(self.root_plan.fields(struct_plan.fields))
            }
            TypePlanNodeKind::Enum(enum_plan) => {
                // For enums, we need the variant index from the tracker
                if let crate::partial::Tracker::Enum { variant_idx, .. } = &frame.tracker {
                    self.root_plan
                        .variants(enum_plan.variants)
                        .get(*variant_idx)
                        .map(|v| self.root_plan.fields(v.fields))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Returns the precomputed TypePlanNode for the current frame.
    ///
    /// This provides access to the precomputed deserialization strategy and
    /// other metadata computed at Partial allocation time.
    ///
    /// Returns `None` if:
    /// - The Partial is not active
    /// - There are no frames
    #[inline]
    pub fn plan_node(&self) -> Option<&crate::typeplan::TypePlanNode> {
        if self.state != PartialState::Active {
            return None;
        }
        let frame = self.frames().last()?;
        Some(self.root_plan.node(frame.type_plan))
    }

    /// Returns the node ID for the current frame's type plan.
    ///
    /// Returns `None` if:
    /// - The Partial is not active
    /// - There are no frames
    #[inline]
    pub fn plan_node_id(&self) -> Option<crate::typeplan::NodeId> {
        if self.state != PartialState::Active {
            return None;
        }
        let frame = self.frames().last()?;
        Some(frame.type_plan)
    }

    /// Returns the precomputed deserialization strategy for the current frame.
    ///
    /// This tells facet-format exactly how to deserialize the current type without
    /// runtime inspection of Shape/Def/vtable. The strategy is computed once at
    /// TypePlan build time.
    ///
    /// If the current node is a BackRef (recursive type), this automatically
    /// follows the reference to return the target node's strategy.
    ///
    /// Returns `None` if:
    /// - The Partial is not active
    /// - There are no frames
    #[inline]
    pub fn deser_strategy(&self) -> Option<&DeserStrategy> {
        let node = self.plan_node()?;
        // Resolve BackRef if needed - resolve_backref returns the node unchanged if not a BackRef
        let resolved = self.root_plan.resolve_backref(node);
        Some(&resolved.strategy)
    }

    /// Returns the precomputed proxy nodes for the current frame's type.
    ///
    /// These contain TypePlan nodes for all proxies (format-agnostic and format-specific)
    /// on this type, allowing runtime lookup based on format namespace.
    #[inline]
    pub fn proxy_nodes(&self) -> Option<&crate::typeplan::ProxyNodes> {
        let node = self.plan_node()?;
        let resolved = self.root_plan.resolve_backref(node);
        Some(&resolved.proxies)
    }

    /// Returns true if the current frame is building a smart pointer slice (Arc<\[T\]>, Rc<\[T\]>, Box<\[T\]>).
    ///
    /// This is used by deserializers to determine if they should deserialize as a list
    /// rather than recursing into the smart pointer type.
    #[inline]
    pub fn is_building_smart_ptr_slice(&self) -> bool {
        if self.state != PartialState::Active {
            return false;
        }
        self.frames()
            .last()
            .is_some_and(|f| matches!(f.tracker, Tracker::SmartPointerSlice { .. }))
    }

    /// Returns the current path in deferred mode (for debugging/tracing).
    #[inline]
    pub fn current_path(&self) -> Option<facet_path::Path> {
        if self.is_deferred() {
            Some(self.path())
        } else {
            None
        }
    }

    /// Enables deferred materialization mode with the given Resolution.
    ///
    /// When deferred mode is enabled:
    /// - `end()` stores frames instead of validating them
    /// - Re-entering a path restores the stored frame with its state intact
    /// - `finish_deferred()` performs final validation and materialization
    ///
    /// This allows deserializers to handle interleaved fields (e.g., TOML dotted
    /// keys, flattened structs) where nested fields aren't contiguous in the input.
    ///
    /// # Use Cases
    ///
    /// - TOML dotted keys: `inner.x = 1` followed by `count = 2` then `inner.y = 3`
    /// - Flattened structs where nested fields appear at the parent level
    /// - Any format where field order doesn't match struct nesting
    ///
    /// # Errors
    ///
    /// Returns an error if already in deferred mode.
    #[inline]
    pub fn begin_deferred(mut self) -> Result<Self, ReflectError> {
        // Cannot enable deferred mode if already in deferred mode
        if self.is_deferred() {
            return Err(self.err(ReflectErrorKind::InvariantViolation {
                invariant: "begin_deferred() called but already in deferred mode",
            }));
        }

        // Take the stack out of Strict mode and wrap in Deferred mode
        let FrameMode::Strict { stack } = core::mem::replace(
            &mut self.mode,
            FrameMode::Strict { stack: Vec::new() }, // temporary placeholder
        ) else {
            unreachable!("just checked we're not in deferred mode");
        };

        let start_depth = stack.len();
        self.mode = FrameMode::Deferred {
            stack,
            start_depth,
            stored_frames: BTreeMap::new(),
            pending_map_insertions: Vec::new(),
        };
        Ok(self)
    }

    /// Finishes deferred mode: validates all stored frames and finalizes.
    ///
    /// This method:
    /// 1. Validates that all stored frames are fully initialized
    /// 2. Processes frames from deepest to shallowest, updating parent ISets
    /// 3. Validates the root frame
    ///
    /// # Errors
    ///
    /// Returns an error if any required fields are missing or if the partial is
    /// not in deferred mode.
    pub fn finish_deferred(mut self) -> Result<Self, ReflectError> {
        // Check if we're in deferred mode first, before extracting state
        if !self.is_deferred() {
            return Err(self.err(ReflectErrorKind::InvariantViolation {
                invariant: "finish_deferred() called but deferred mode is not enabled",
            }));
        }

        // Extract deferred state, transitioning back to Strict mode
        let FrameMode::Deferred {
            stack,
            mut stored_frames,
            pending_map_insertions,
            ..
        } = core::mem::replace(&mut self.mode, FrameMode::Strict { stack: Vec::new() })
        else {
            unreachable!("just checked is_deferred()");
        };

        // Restore the stack to self.mode
        self.mode = FrameMode::Strict { stack };

        // Sort paths by depth (deepest first) so we process children before parents
        let mut paths: Vec<_> = stored_frames.keys().cloned().collect();
        paths.sort_by_key(|b| core::cmp::Reverse(b.len()));

        trace!(
            "finish_deferred: Processing {} stored frames in order: [{}]",
            paths.len(),
            paths
                .iter()
                .map(|p| p.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );

        // Process each stored frame from deepest to shallowest
        for path in paths {
            let stored = stored_frames.remove(&path).unwrap();
            let mut frame = stored.frame;
            let parent_frame_index = stored.parent_frame_index;

            trace!(
                "finish_deferred: Processing frame at {}, shape {}, tracker {:?}, parent_frame_index={}",
                path,
                frame.allocated.shape(),
                frame.tracker.kind(),
                parent_frame_index
            );

            // Fill in defaults for unset fields that have defaults
            if let Err(e) = frame.fill_defaults() {
                // Before cleanup, clear the parent's iset bit for the frame that failed.
                Self::clear_parent_iset_for_stored(
                    &path,
                    parent_frame_index,
                    self.frames_mut(),
                    &mut stored_frames,
                );
                frame.deinit();
                frame.dealloc();
                // Clean up remaining stored frames safely (deepest first, clearing parent isets)
                Self::cleanup_stored_frames_on_error(stored_frames, self.frames_mut());
                return Err(self.err(e));
            }

            // Validate the frame is fully initialized
            if let Err(e) = frame.require_full_initialization() {
                // Before cleanup, clear the parent's iset bit for the frame that failed.
                Self::clear_parent_iset_for_stored(
                    &path,
                    parent_frame_index,
                    self.frames_mut(),
                    &mut stored_frames,
                );
                frame.deinit();
                frame.dealloc();
                // Clean up remaining stored frames safely (deepest first, clearing parent isets)
                Self::cleanup_stored_frames_on_error(stored_frames, self.frames_mut());
                return Err(self.err(e));
            }

            // For List frames, finalize the Vec's length based on element_count
            if let Tracker::List { element_count, .. } = &frame.tracker
                && let Def::List(list_def) = frame.allocated.shape().def
            {
                if let Some(set_len_fn) = list_def.set_len() {
                    crate::trace!("finish_deferred: finalizing Vec len to {}", element_count);
                    unsafe {
                        set_len_fn(frame.data.assume_init(), *element_count);
                    }
                }
            }

            // Update parent's ISet to mark this field as initialized.
            // We use the stored parent_frame_index to find the parent directly.
            if let Some(last_step) = path.steps.last() {
                // Construct parent path for looking up in stored_frames
                let parent_path = facet_path::Path {
                    shape: path.shape,
                    steps: path.steps[..path.steps.len() - 1].to_vec(),
                };

                // Special handling for Option inner values: when path ends with OptionSome,
                // the parent is an Option frame and we need to complete the Option by
                // writing the inner value into the Option's memory.
                if matches!(last_step, PathStep::OptionSome) {
                    // Find the Option frame (parent) - try stored_frames first, then stack
                    let option_frame =
                        if let Some(stored_parent) = stored_frames.get_mut(&parent_path) {
                            Some(&mut stored_parent.frame)
                        } else {
                            self.frames_mut().get_mut(parent_frame_index)
                        };

                    if let Some(option_frame) = option_frame {
                        // The frame contains the inner value - write it into the Option's memory
                        Self::complete_option_frame(option_frame, frame);
                        // Frame data has been transferred to Option - don't drop it
                        continue;
                    }
                }

                // Determine what to mark based on path step type
                let field_to_mark: Option<usize> = match last_step {
                    PathStep::Field(field_idx) => Some(*field_idx as usize),
                    PathStep::Index(idx) => Some(*idx as usize),
                    PathStep::Variant(_) => {
                        // For enums, find the Field step that contains this enum.
                        // Path like [Field(1), Variant(0)] means enum at field 1.
                        // We need to mark the grandparent's iset for field 1.
                        if let Some(PathStep::Field(field_idx)) = parent_path.steps.last() {
                            Some(*field_idx as usize)
                        } else {
                            None
                        }
                    }
                    _ => None,
                };

                if let Some(field_idx) = field_to_mark {
                    // For Variant steps, we mark the grandparent (the struct containing the enum)
                    // For other steps, we mark the direct parent
                    let (target_path, target_index) = if matches!(last_step, PathStep::Variant(_)) {
                        // Grandparent: path without the last two steps (Field + Variant)
                        let grandparent_path = facet_path::Path {
                            shape: path.shape,
                            steps: parent_path.steps[..parent_path.steps.len().saturating_sub(1)]
                                .to_vec(),
                        };
                        // The grandparent's index: look up the parent's stored parent_frame_index
                        let grandparent_index =
                            if let Some(stored_parent) = stored_frames.get(&parent_path) {
                                stored_parent.parent_frame_index
                            } else {
                                // Parent is on stack, so grandparent is one below parent_frame_index
                                parent_frame_index.saturating_sub(1)
                            };
                        (grandparent_path, grandparent_index)
                    } else {
                        (parent_path.clone(), parent_frame_index)
                    };

                    crate::trace!(
                        "finish_deferred: marking field {} at target_index={}, path={:?}",
                        field_idx,
                        target_index,
                        path
                    );

                    // Try stored_frames first, then stack
                    if let Some(stored_parent) = stored_frames.get_mut(&target_path) {
                        Self::mark_field_initialized_by_index(&mut stored_parent.frame, field_idx);
                    } else if let Some(parent_frame) = self.frames_mut().get_mut(target_index) {
                        Self::mark_field_initialized_by_index(parent_frame, field_idx);
                    }
                }
            }

            // Frame is validated and parent is updated - dealloc if needed
            frame.dealloc();
        }

        // Process pending map insertions. The values may have Option fields that
        // need fill_defaults called before we can safely insert into the map.
        for pending in pending_map_insertions {
            crate::trace!(
                "finish_deferred: Processing pending map insertion for value shape={}",
                pending.map_def.v()
            );

            // Create a temporary frame for the value so we can call fill_defaults
            let value_shape = pending.map_def.v();
            let value_layout = match value_shape.layout.sized_layout() {
                Ok(l) => l,
                Err(_) => {
                    // Clean up the key and value buffers on error
                    if let Ok(key_layout) = pending.map_def.k().layout.sized_layout()
                        && key_layout.size() > 0
                    {
                        unsafe {
                            ::alloc::alloc::dealloc(pending.key_ptr.as_mut_byte_ptr(), key_layout);
                        }
                    }
                    if let Ok(value_layout) = value_shape.layout.sized_layout()
                        && value_layout.size() > 0
                    {
                        unsafe {
                            ::alloc::alloc::dealloc(
                                pending.value_ptr.as_mut_byte_ptr(),
                                value_layout,
                            );
                        }
                    }
                    return Err(self.err(ReflectErrorKind::InvariantViolation {
                        invariant: "pending map insertion value has unsized layout",
                    }));
                }
            };

            // Create a frame just for fill_defaults - we mark it as init since the
            // value was already written, we just need to fill in missing Option fields
            let mut value_frame = Frame::new(
                pending.value_ptr,
                AllocatedShape::new(value_shape, value_layout.size()),
                FrameOwnership::TrackedBuffer,
                typeplan::NodeId::invalid(),
            );
            value_frame.is_init = true;

            // Fill defaults on the value (e.g., Option fields -> None)
            if let Err(e) = value_frame.fill_defaults() {
                // Clean up
                if let Ok(key_layout) = pending.map_def.k().layout.sized_layout()
                    && key_layout.size() > 0
                {
                    unsafe {
                        ::alloc::alloc::dealloc(pending.key_ptr.as_mut_byte_ptr(), key_layout);
                    }
                }
                value_frame.deinit();
                value_frame.dealloc();
                return Err(self.err(e));
            }

            // Now insert into the map
            let insert = pending.map_def.vtable.insert;
            unsafe {
                insert(
                    PtrMut::new(pending.map_ptr.as_mut_byte_ptr()),
                    PtrMut::new(pending.key_ptr.as_mut_byte_ptr()),
                    PtrMut::new(pending.value_ptr.as_mut_byte_ptr()),
                );
            }

            // Deallocate the temporary key and value buffers (values have been moved into map)
            if let Ok(key_layout) = pending.map_def.k().layout.sized_layout()
                && key_layout.size() > 0
            {
                unsafe {
                    ::alloc::alloc::dealloc(pending.key_ptr.as_mut_byte_ptr(), key_layout);
                }
            }
            if value_layout.size() > 0 {
                unsafe {
                    ::alloc::alloc::dealloc(pending.value_ptr.as_mut_byte_ptr(), value_layout);
                }
            }
        }

        // Invariant check: we must have at least one frame after finish_deferred
        if self.frames().is_empty() {
            // No need to poison - returning Err consumes self, Drop will handle cleanup
            return Err(self.err(ReflectErrorKind::InvariantViolation {
                invariant: "finish_deferred() left Partial with no frames",
            }));
        }

        // Fill defaults and validate the root frame is fully initialized
        if let Some(frame) = self.frames_mut().last_mut() {
            // Fill defaults - this can fail if a field has #[facet(default)] but no default impl
            if let Err(e) = frame.fill_defaults() {
                return Err(self.err(e));
            }
            // Root validation failed. At this point, all stored frames have been
            // processed and their parent isets updated.
            // No need to poison - returning Err consumes self, Drop will handle cleanup
            if let Err(e) = frame.require_full_initialization() {
                return Err(self.err(e));
            }
        }

        Ok(self)
    }

    /// Mark a field as initialized in a frame's tracker by index
    fn mark_field_initialized_by_index(frame: &mut Frame, idx: usize) {
        crate::trace!(
            "mark_field_initialized_by_index: idx={}, frame shape={}, tracker={:?}",
            idx,
            frame.allocated.shape(),
            frame.tracker.kind()
        );

        // If the tracker is Scalar but this is a struct type, upgrade to Struct tracker.
        // This can happen if the frame was deinit'd (e.g., by a failed set_default)
        // which resets the tracker to Scalar.
        if matches!(frame.tracker, Tracker::Scalar)
            && let Type::User(UserType::Struct(struct_type)) = frame.allocated.shape().ty
        {
            frame.tracker = Tracker::Struct {
                iset: ISet::new(struct_type.fields.len()),
                current_child: None,
            };
        }

        match &mut frame.tracker {
            Tracker::Struct { iset, .. } => {
                crate::trace!(
                    "mark_field_initialized_by_index (Struct): setting iset[{}] for shape={}",
                    idx,
                    frame.allocated.shape()
                );
                iset.set(idx);
            }
            Tracker::Enum { data, .. } => {
                crate::trace!(
                    "mark_field_initialized_by_index (Enum): setting data[{}] for shape={}, before={:?}",
                    idx,
                    frame.allocated.shape(),
                    data
                );
                data.set(idx);
                crate::trace!("mark_field_initialized_by_index (Enum): after={:?}", data);
            }
            Tracker::Array { iset, .. } => {
                crate::trace!(
                    "mark_field_initialized_by_index (Array): setting iset[{}] for shape={}",
                    idx,
                    frame.allocated.shape()
                );
                iset.set(idx);
            }
            _ => {
                crate::trace!(
                    "mark_field_initialized_by_index: no match for tracker {:?}",
                    frame.tracker.kind()
                );
            }
        }
    }

    /// Clear a parent frame's iset bit for a stored frame.
    /// Uses the stored parent_frame_index directly instead of computing from path.
    fn clear_parent_iset_for_stored(
        path: &Path,
        parent_frame_index: usize,
        stack: &mut [Frame],
        stored_frames: &mut ::alloc::collections::BTreeMap<Path, crate::partial::StoredFrame>,
    ) {
        if let Some(&PathStep::Field(field_idx)) = path.steps.last() {
            let field_idx = field_idx as usize;
            let parent_path = Path {
                shape: path.shape,
                steps: path.steps[..path.steps.len() - 1].to_vec(),
            };

            // Try stored_frames first, then use the stored parent_frame_index
            if let Some(stored_parent) = stored_frames.get_mut(&parent_path) {
                Self::unset_field_in_tracker(&mut stored_parent.frame.tracker, field_idx);
            } else if let Some(parent_frame) = stack.get_mut(parent_frame_index) {
                Self::unset_field_in_tracker(&mut parent_frame.tracker, field_idx);
            }
        }
    }

    /// Helper to unset a field index in a tracker's iset
    fn unset_field_in_tracker(tracker: &mut Tracker, field_idx: usize) {
        match tracker {
            Tracker::Struct { iset, .. } => {
                iset.unset(field_idx);
            }
            Tracker::Enum { data, .. } => {
                data.unset(field_idx);
            }
            Tracker::Array { iset, .. } => {
                iset.unset(field_idx);
            }
            _ => {}
        }
    }

    /// Safely clean up stored frames on error in finish_deferred.
    ///
    /// This mirrors the cleanup logic in Drop: process frames deepest-first and
    /// clear parent's iset bits before deiniting children to prevent double-drops.
    fn cleanup_stored_frames_on_error(
        mut stored_frames: ::alloc::collections::BTreeMap<Path, crate::partial::StoredFrame>,
        stack: &mut [Frame],
    ) {
        // Sort by depth (deepest first) so children are processed before parents
        let mut paths: Vec<_> = stored_frames.keys().cloned().collect();
        paths.sort_by_key(|p| core::cmp::Reverse(p.steps.len()));

        for path in paths {
            if let Some(stored) = stored_frames.remove(&path) {
                let mut frame = stored.frame;
                let parent_frame_index = stored.parent_frame_index;
                // Before dropping this frame, clear the parent's iset bit so the
                // parent won't try to drop this field again.
                Self::clear_parent_iset_for_stored(
                    &path,
                    parent_frame_index,
                    stack,
                    &mut stored_frames,
                );
                frame.deinit();
                frame.dealloc();
            }
        }
    }

    /// Complete an Option frame by writing the inner value and marking it initialized.
    /// Used in finish_deferred when processing a stored frame at a path ending with "Some".
    fn complete_option_frame(option_frame: &mut Frame, inner_frame: Frame) {
        if let Def::Option(option_def) = option_frame.allocated.shape().def {
            // Use the Option vtable to initialize Some(inner_value)
            let init_some_fn = option_def.vtable.init_some;

            // The inner frame contains the inner value
            let inner_value_ptr = unsafe { inner_frame.data.assume_init().as_const() };

            // Initialize the Option as Some(inner_value)
            unsafe {
                init_some_fn(option_frame.data, inner_value_ptr);
            }

            // Deallocate the inner value's memory since init_some_fn moved it
            if let FrameOwnership::Owned = inner_frame.ownership
                && let Ok(layout) = inner_frame.allocated.shape().layout.sized_layout()
                && layout.size() > 0
            {
                unsafe {
                    ::alloc::alloc::dealloc(inner_frame.data.as_mut_byte_ptr(), layout);
                }
            }

            // Mark the Option as initialized
            option_frame.tracker = Tracker::Option {
                building_inner: false,
            };
            option_frame.is_init = true;
        }
    }

    /// Pops the current frame off the stack, indicating we're done initializing the current field
    pub fn end(mut self) -> Result<Self, ReflectError> {
        // FAST PATH: Handle the common case of ending a simple scalar field in a struct.
        // This avoids all the edge-case checks (SmartPointerSlice, deferred mode, custom
        // deserialization, etc.) that dominate the slow path.
        if self.frames().len() >= 2 && !self.is_deferred() {
            let frames = self.frames_mut();
            let top_idx = frames.len() - 1;
            let parent_idx = top_idx - 1;

            // Check if this is a simple scalar field being returned to a struct parent
            if let (
                Tracker::Scalar,
                true, // is_init
                FrameOwnership::Field { field_idx },
                false, // not using custom deserialization
            ) = (
                &frames[top_idx].tracker,
                frames[top_idx].is_init,
                frames[top_idx].ownership,
                frames[top_idx].using_custom_deserialization,
            ) && matches!(&frames[parent_idx].tracker, Tracker::Struct { .. })
            {
                // Fast path: just update parent's iset and pop
                // Extract shapes before mutable borrow
                crate::trace!(
                    %field_idx,
                    parent_shape = %frames[parent_idx].allocated.shape(),
                    child_shape = %frames[top_idx].allocated.shape(),
                    "end() FAST PATH: setting iset",
                );
                if let Tracker::Struct {
                    iset,
                    current_child,
                } = &mut frames[parent_idx].tracker
                {
                    iset.set(field_idx);
                    *current_child = None;
                }
                frames.pop();
                return Ok(self);
            }
        }

        // SLOW PATH: Handle all the edge cases
        if let Some(_frame) = self.frames().last() {
            crate::trace!(
                "end() called: shape={}, tracker={:?}, is_init={}",
                _frame.allocated.shape(),
                _frame.tracker.kind(),
                _frame.is_init
            );
        }

        // Special handling for SmartPointerSlice - convert builder to Arc
        // Check if the current (top) frame is a SmartPointerSlice that needs conversion
        let needs_slice_conversion = {
            let frames = self.frames();
            if frames.is_empty() {
                false
            } else {
                let top_idx = frames.len() - 1;
                crate::trace!(
                    "end(): frames.len()={}, top tracker={:?}",
                    frames.len(),
                    frames[top_idx].tracker.kind()
                );
                matches!(
                    frames[top_idx].tracker,
                    Tracker::SmartPointerSlice {
                        building_item: false,
                        ..
                    }
                )
            }
        };

        crate::trace!("end(): needs_slice_conversion={}", needs_slice_conversion);

        if needs_slice_conversion {
            // Get shape info upfront to avoid borrow conflicts
            let current_shape = self.frames().last().unwrap().allocated.shape();

            let frames = self.frames_mut();
            let top_idx = frames.len() - 1;

            if let Tracker::SmartPointerSlice { vtable, .. } = &frames[top_idx].tracker {
                // Convert the builder to Arc<[T]>
                let vtable = *vtable;
                let builder_ptr = unsafe { frames[top_idx].data.assume_init() };
                let arc_ptr = unsafe { (vtable.convert_fn)(builder_ptr) };

                match frames[top_idx].ownership {
                    FrameOwnership::Field { field_idx } => {
                        // Arc<[T]> is a field in a struct
                        // The field frame's original data pointer was overwritten with the builder pointer,
                        // so we need to reconstruct where the Arc should be written.

                        // Get parent frame and field info
                        let parent_idx = top_idx - 1;
                        let parent_frame = &frames[parent_idx];

                        // Get the field to find its offset
                        let field = if let Type::User(UserType::Struct(struct_type)) =
                            parent_frame.allocated.shape().ty
                        {
                            &struct_type.fields[field_idx]
                        } else {
                            return Err(self.err(ReflectErrorKind::InvariantViolation {
                                invariant: "SmartPointerSlice field frame parent must be a struct",
                            }));
                        };

                        // Calculate where the Arc should be written (parent.data + field.offset)
                        let field_location =
                            unsafe { parent_frame.data.field_uninit(field.offset) };

                        // Write the Arc to the parent struct's field location
                        let arc_layout = match current_shape.layout.sized_layout() {
                            Ok(layout) => layout,
                            Err(_) => {
                                return Err(self.err(ReflectErrorKind::Unsized {
                                    shape: current_shape,
                                    operation: "SmartPointerSlice conversion requires sized Arc",
                                }));
                            }
                        };
                        let arc_size = arc_layout.size();
                        unsafe {
                            core::ptr::copy_nonoverlapping(
                                arc_ptr.as_byte_ptr(),
                                field_location.as_mut_byte_ptr(),
                                arc_size,
                            );
                        }

                        // Free the staging allocation from convert_fn (the Arc was copied to field_location)
                        unsafe {
                            ::alloc::alloc::dealloc(arc_ptr.as_byte_ptr() as *mut u8, arc_layout);
                        }

                        // Update the frame to point to the correct field location and mark as initialized
                        frames[top_idx].data = field_location;
                        frames[top_idx].tracker = Tracker::Scalar;
                        frames[top_idx].is_init = true;

                        // Return WITHOUT popping - the field frame will be popped by the next end() call
                        return Ok(self);
                    }
                    FrameOwnership::Owned => {
                        // Arc<[T]> is the root type or owned independently
                        // The frame already has the allocation, we just need to update it with the Arc

                        // The frame's data pointer is currently the builder, but we allocated
                        // the Arc memory in the convert_fn. Update to point to the Arc.
                        frames[top_idx].data = PtrUninit::new(arc_ptr.as_byte_ptr() as *mut u8);
                        frames[top_idx].tracker = Tracker::Scalar;
                        frames[top_idx].is_init = true;
                        // Keep Owned ownership so Guard will properly deallocate

                        // Return WITHOUT popping - the frame stays and will be built/dropped normally
                        return Ok(self);
                    }
                    FrameOwnership::TrackedBuffer
                    | FrameOwnership::BorrowedInPlace
                    | FrameOwnership::External
                    | FrameOwnership::ListSlot => {
                        return Err(self.err(ReflectErrorKind::InvariantViolation {
                            invariant: "SmartPointerSlice cannot have TrackedBuffer/BorrowedInPlace/External/ListSlot ownership after conversion",
                        }));
                    }
                }
            }
        }

        if self.frames().len() <= 1 {
            // Never pop the last/root frame - this indicates a broken state machine
            // No need to poison - returning Err consumes self, Drop will handle cleanup
            return Err(self.err(ReflectErrorKind::InvariantViolation {
                invariant: "Partial::end() called with only one frame on the stack",
            }));
        }

        // In deferred mode, cannot pop below the start depth
        if let Some(start_depth) = self.start_depth()
            && self.frames().len() <= start_depth
        {
            // No need to poison - returning Err consumes self, Drop will handle cleanup
            return Err(self.err(ReflectErrorKind::InvariantViolation {
                invariant: "Partial::end() called but would pop below deferred start depth",
            }));
        }

        // Require that the top frame is fully initialized before popping.
        // In deferred mode, ALL validation is deferred to finish_deferred().
        let requires_full_init = !self.is_deferred();

        if requires_full_init {
            // Try the optimized path using precomputed FieldInitPlan
            // Extract frame info first (borrows only self.mode)
            let frame_info = self.mode.stack().last().map(|frame| {
                let variant_idx = match &frame.tracker {
                    Tracker::Enum { variant_idx, .. } => Some(*variant_idx),
                    _ => None,
                };
                (frame.type_plan, variant_idx)
            });

            // Look up plans from the type plan node - need to resolve NodeId to get the actual node
            let plans_info = frame_info.and_then(|(type_plan_id, variant_idx)| {
                let type_plan = self.root_plan.node(type_plan_id);
                match &type_plan.kind {
                    TypePlanNodeKind::Struct(struct_plan) => Some(struct_plan.fields),
                    TypePlanNodeKind::Enum(enum_plan) => {
                        let variants = self.root_plan.variants(enum_plan.variants);
                        variant_idx.and_then(|idx| variants.get(idx).map(|v| v.fields))
                    }
                    _ => None,
                }
            });

            if let Some(plans_range) = plans_info {
                // Resolve the SliceRange to an actual slice
                let plans = self.root_plan.fields(plans_range);
                // Now mutably borrow mode.stack to get the frame
                // (root_plan borrow of `plans` is still active but that's fine -
                // mode and root_plan are separate fields)
                let frame = self.mode.stack_mut().last_mut().unwrap();
                crate::trace!(
                    "end(): Using optimized fill_and_require_fields for {}, tracker={:?}",
                    frame.allocated.shape(),
                    frame.tracker.kind()
                );
                frame
                    .fill_and_require_fields(plans, plans.len(), &self.root_plan)
                    .map_err(|e| self.err(e))?;
            } else {
                // Fall back to the old path if optimized path wasn't available
                // Fill defaults before checking full initialization
                // This handles structs/enums that have fields with #[facet(default)] or
                // fields whose types implement Default - they should be auto-filled.
                if let Some(frame) = self.frames_mut().last_mut() {
                    crate::trace!(
                        "end(): Filling defaults before full init check for {}, tracker={:?}",
                        frame.allocated.shape(),
                        frame.tracker.kind()
                    );
                    frame.fill_defaults().map_err(|e| self.err(e))?;
                }

                let frame = self.frames().last().unwrap();
                crate::trace!(
                    "end(): Checking full init for {}, tracker={:?}, is_init={}",
                    frame.allocated.shape(),
                    frame.tracker.kind(),
                    frame.is_init
                );
                let result = frame.require_full_initialization();
                crate::trace!(
                    "end(): require_full_initialization result: {:?}",
                    result.is_ok()
                );
                result.map_err(|e| self.err(e))?
            }
        }

        // Pop the frame first
        let mut popped_frame = self.frames_mut().pop().unwrap();

        // In deferred mode, store most frames for later validation in finish_deferred().
        // EXCEPTIONS that should NOT be stored (they need to go through normal processing):
        // - TrackedBuffer frames (map keys/values) - need to be inserted into the map
        // - Set element frames (when parent's current_child=true) - need to be inserted into the set
        // We compute the path AFTER popping so the frame's own tracker state doesn't
        // pollute its path (e.g., Option's building_inner shouldn't add OptionSome to its own path).
        let is_set_element = self.frames().last().is_some_and(|parent| {
            matches!(
                parent.tracker,
                Tracker::Set {
                    current_child: true
                }
            )
        });
        let should_store = self.is_deferred()
            && !matches!(popped_frame.ownership, FrameOwnership::TrackedBuffer)
            && !is_set_element;

        if should_store {
            let field_path = self.path();
            trace!("path() returned steps: {:?}", field_path.steps);
            if !field_path.is_empty() {
                let storage_path = field_path;
                trace!(
                    "end(): Storing frame for deferred path {}, shape {}",
                    storage_path,
                    popped_frame.allocated.shape()
                );

                if let FrameMode::Deferred {
                    stack,
                    stored_frames,
                    ..
                } = &mut self.mode
                {
                    // For ListSlot frames, increment the parent's element_count so subsequent
                    // elements get the correct index. The actual set_len happens in finish_deferred.
                    if matches!(popped_frame.ownership, FrameOwnership::ListSlot)
                        && let Some(parent_frame) = stack.last_mut()
                        && let Tracker::List { element_count, .. } = &mut parent_frame.tracker
                    {
                        *element_count += 1;
                        crate::trace!(
                            "end(): ListSlot - incremented element_count to {}",
                            element_count
                        );
                    }

                    // Don't mark the field as initialized yet - that happens in finish_deferred
                    // after the frame is validated. The parent's iset should only reflect
                    // actually-initialized memory.

                    // Store the parent frame index - it's the current top of stack after popping
                    let parent_frame_index = stack.len().saturating_sub(1);
                    stored_frames.insert(
                        storage_path,
                        crate::partial::StoredFrame {
                            frame: popped_frame,
                            parent_frame_index,
                        },
                    );

                    // Clear parent's current_child tracking
                    if let Some(parent_frame) = stack.last_mut() {
                        crate::trace!(
                            "end(): Clearing current_child on parent shape={}, tracker={:?}",
                            parent_frame.allocated.shape(),
                            parent_frame.tracker.kind()
                        );
                        parent_frame.tracker.clear_current_child();
                    } else {
                        crate::trace!("end(): No parent frame to clear current_child on");
                    }
                }

                return Ok(self);
            }
        }

        // check if this needs deserialization from a different shape
        if popped_frame.using_custom_deserialization {
            // First check the proxy stored in the frame (used for format-specific proxies
            // and container-level proxies), then fall back to field-level proxy.
            // This ordering is important because format-specific proxies store their
            // proxy in shape_level_proxy, and we want them to take precedence over
            // the format-agnostic field.proxy().
            let deserialize_with: Option<facet_core::ProxyConvertInFn> =
                popped_frame.shape_level_proxy.map(|p| p.convert_in);

            // Fall back to field-level proxy (format-agnostic)
            let deserialize_with = deserialize_with.or_else(|| {
                self.parent_field()
                    .and_then(|f| f.proxy().map(|p| p.convert_in))
            });

            if let Some(deserialize_with) = deserialize_with {
                // Get parent shape upfront to avoid borrow conflicts
                let parent_shape = self.frames().last().unwrap().allocated.shape();
                let parent_frame = self.frames_mut().last_mut().unwrap();

                trace!(
                    "Detected custom conversion needed from {} to {}",
                    popped_frame.allocated.shape(),
                    parent_shape
                );

                unsafe {
                    let res = {
                        let inner_value_ptr = popped_frame.data.assume_init().as_const();
                        (deserialize_with)(inner_value_ptr, parent_frame.data)
                    };
                    let popped_frame_shape = popped_frame.allocated.shape();

                    // Note: We do NOT call deinit() here because deserialize_with uses
                    // ptr::read to take ownership of the source value. Calling deinit()
                    // would cause a double-free. We mark is_init as false to satisfy
                    // dealloc()'s assertion, then deallocate the memory.
                    popped_frame.is_init = false;
                    popped_frame.dealloc();
                    let parent_data = parent_frame.data;
                    match res {
                        Ok(rptr) => {
                            if rptr.as_uninit() != parent_data {
                                return Err(self.err(
                                    ReflectErrorKind::CustomDeserializationError {
                                        message:
                                            "deserialize_with did not return the expected pointer"
                                                .into(),
                                        src_shape: popped_frame_shape,
                                        dst_shape: parent_shape,
                                    },
                                ));
                            }
                        }
                        Err(message) => {
                            return Err(self.err(ReflectErrorKind::CustomDeserializationError {
                                message,
                                src_shape: popped_frame_shape,
                                dst_shape: parent_shape,
                            }));
                        }
                    }
                    // Re-borrow parent_frame after potential early returns
                    let parent_frame = self.frames_mut().last_mut().unwrap();
                    parent_frame.mark_as_init();
                }
                return Ok(self);
            }
        }

        // Update parent frame's tracking when popping from a child
        // Get parent shape upfront to avoid borrow conflicts
        let parent_shape = self.frames().last().unwrap().allocated.shape();
        // Cache is_deferred before taking mutable borrow of frames
        let is_deferred = self.is_deferred();

        let parent_frame = self.frames_mut().last_mut().unwrap();

        crate::trace!(
            "end(): Popped {} (tracker {:?}), Parent {} (tracker {:?})",
            popped_frame.allocated.shape(),
            popped_frame.tracker.kind(),
            parent_shape,
            parent_frame.tracker.kind()
        );

        // Check if we need to do a conversion - this happens when:
        // 1. The parent frame has a builder_shape or inner type that matches the popped frame's shape
        // 2. The parent frame has try_from
        // 3. The parent frame is not yet initialized
        // 4. The parent frame's tracker is Scalar (not Option, SmartPointer, etc.)
        //    This ensures we only do conversion when begin_inner was used, not begin_some
        let needs_conversion = !parent_frame.is_init
            && matches!(parent_frame.tracker, Tracker::Scalar)
            && ((parent_shape.builder_shape.is_some()
                && parent_shape.builder_shape.unwrap() == popped_frame.allocated.shape())
                || (parent_shape.inner.is_some()
                    && parent_shape.inner.unwrap() == popped_frame.allocated.shape()))
            && match parent_shape.vtable {
                facet_core::VTableErased::Direct(vt) => vt.try_from.is_some(),
                facet_core::VTableErased::Indirect(vt) => vt.try_from.is_some(),
            };

        if needs_conversion {
            trace!(
                "Detected implicit conversion needed from {} to {}",
                popped_frame.allocated.shape(),
                parent_shape
            );

            // The conversion requires the source frame to be fully initialized
            // (we're about to call assume_init() and pass to try_from)
            if let Err(e) = popped_frame.require_full_initialization() {
                // Deallocate the memory since the frame wasn't fully initialized
                if let FrameOwnership::Owned = popped_frame.ownership
                    && let Ok(layout) = popped_frame.allocated.shape().layout.sized_layout()
                    && layout.size() > 0
                {
                    trace!(
                        "Deallocating uninitialized conversion frame memory: size={}, align={}",
                        layout.size(),
                        layout.align()
                    );
                    unsafe {
                        ::alloc::alloc::dealloc(popped_frame.data.as_mut_byte_ptr(), layout);
                    }
                }
                return Err(self.err(e));
            }

            // Perform the conversion
            let inner_ptr = unsafe { popped_frame.data.assume_init().as_const() };
            let inner_shape = popped_frame.allocated.shape();

            trace!("Converting from {} to {}", inner_shape, parent_shape);

            // Handle Direct and Indirect vtables - both return TryFromOutcome
            let outcome = match parent_shape.vtable {
                facet_core::VTableErased::Direct(vt) => {
                    if let Some(try_from_fn) = vt.try_from {
                        unsafe {
                            try_from_fn(
                                parent_frame.data.as_mut_byte_ptr() as *mut (),
                                inner_shape,
                                inner_ptr,
                            )
                        }
                    } else {
                        return Err(self.err(ReflectErrorKind::OperationFailed {
                            shape: parent_shape,
                            operation: "try_from not available for this type",
                        }));
                    }
                }
                facet_core::VTableErased::Indirect(vt) => {
                    if let Some(try_from_fn) = vt.try_from {
                        // parent_frame.data is uninitialized - we're writing the converted
                        // value into it
                        let ox_uninit =
                            facet_core::OxPtrUninit::new(parent_frame.data, parent_shape);
                        unsafe { try_from_fn(ox_uninit, inner_shape, inner_ptr) }
                    } else {
                        return Err(self.err(ReflectErrorKind::OperationFailed {
                            shape: parent_shape,
                            operation: "try_from not available for this type",
                        }));
                    }
                }
            };

            // Handle the TryFromOutcome, which explicitly communicates ownership semantics:
            // - Converted: source was consumed, conversion succeeded
            // - Unsupported: source was NOT consumed, caller retains ownership
            // - Failed: source WAS consumed, but conversion failed
            match outcome {
                facet_core::TryFromOutcome::Converted => {
                    trace!("Conversion succeeded, marking parent as initialized");
                    parent_frame.is_init = true;
                }
                facet_core::TryFromOutcome::Unsupported => {
                    trace!("Source type not supported for conversion - source NOT consumed");

                    // Source was NOT consumed, so we need to drop it properly
                    if let FrameOwnership::Owned = popped_frame.ownership
                        && let Ok(layout) = popped_frame.allocated.shape().layout.sized_layout()
                        && layout.size() > 0
                    {
                        // Drop the value, then deallocate
                        unsafe {
                            popped_frame
                                .allocated
                                .shape()
                                .call_drop_in_place(popped_frame.data.assume_init());
                            ::alloc::alloc::dealloc(popped_frame.data.as_mut_byte_ptr(), layout);
                        }
                    }

                    return Err(self.err(ReflectErrorKind::TryFromError {
                        src_shape: inner_shape,
                        dst_shape: parent_shape,
                        inner: facet_core::TryFromError::UnsupportedSourceType,
                    }));
                }
                facet_core::TryFromOutcome::Failed(e) => {
                    trace!("Conversion failed after consuming source: {e:?}");

                    // Source WAS consumed, so we only deallocate memory (don't drop)
                    if let FrameOwnership::Owned = popped_frame.ownership
                        && let Ok(layout) = popped_frame.allocated.shape().layout.sized_layout()
                        && layout.size() > 0
                    {
                        trace!(
                            "Deallocating conversion frame memory after failure: size={}, align={}",
                            layout.size(),
                            layout.align()
                        );
                        unsafe {
                            ::alloc::alloc::dealloc(popped_frame.data.as_mut_byte_ptr(), layout);
                        }
                    }

                    return Err(self.err(ReflectErrorKind::TryFromError {
                        src_shape: inner_shape,
                        dst_shape: parent_shape,
                        inner: facet_core::TryFromError::Generic(e.into_owned()),
                    }));
                }
            }

            // Deallocate the inner value's memory since try_from consumed it
            if let FrameOwnership::Owned = popped_frame.ownership
                && let Ok(layout) = popped_frame.allocated.shape().layout.sized_layout()
                && layout.size() > 0
            {
                trace!(
                    "Deallocating conversion frame memory: size={}, align={}",
                    layout.size(),
                    layout.align()
                );
                unsafe {
                    ::alloc::alloc::dealloc(popped_frame.data.as_mut_byte_ptr(), layout);
                }
            }

            return Ok(self);
        }

        // For Field-owned frames, reclaim responsibility in parent's tracker
        // Only mark as initialized if the child frame was actually initialized.
        // This prevents double-free when begin_inner/begin_some drops a value via
        // prepare_for_reinitialization but then fails, leaving the child uninitialized.
        //
        // We use require_full_initialization() rather than just is_init because:
        // - Scalar frames use is_init as the source of truth
        // - Struct/Array/Enum frames use their iset/data as the source of truth
        //   (is_init may never be set to true for these tracker types)
        if let FrameOwnership::Field { field_idx } = popped_frame.ownership {
            // In deferred mode, fill defaults on the child frame before checking initialization.
            // Fill defaults for child frame before checking if it's fully initialized.
            // This handles structs/enums with optional fields that should auto-fill.
            if let Err(e) = popped_frame.fill_defaults() {
                return Err(self.err(e));
            }
            let child_is_initialized = popped_frame.require_full_initialization().is_ok();
            match &mut parent_frame.tracker {
                Tracker::Struct {
                    iset,
                    current_child,
                } => {
                    crate::trace!(
                        "end() SLOW PATH (Struct): field_idx={}, child_is_initialized={}, parent shape={}, child shape={}",
                        field_idx,
                        child_is_initialized,
                        parent_frame.allocated.shape(),
                        popped_frame.allocated.shape()
                    );
                    if child_is_initialized {
                        iset.set(field_idx); // Parent reclaims responsibility only if child was init
                    }
                    *current_child = None;
                }
                Tracker::Array {
                    iset,
                    current_child,
                } => {
                    crate::trace!(
                        "end() SLOW PATH (Array): field_idx={}, child_is_initialized={}, parent shape={}, child shape={}",
                        field_idx,
                        child_is_initialized,
                        parent_frame.allocated.shape(),
                        popped_frame.allocated.shape()
                    );
                    if child_is_initialized {
                        iset.set(field_idx); // Parent reclaims responsibility only if child was init
                    }
                    *current_child = None;
                }
                Tracker::Enum {
                    data,
                    current_child,
                    ..
                } => {
                    crate::trace!(
                        "end() SLOW PATH (Enum): field_idx={}, child_is_initialized={}, parent shape={}, child shape={}, data before={:?}",
                        field_idx,
                        child_is_initialized,
                        parent_frame.allocated.shape(),
                        popped_frame.allocated.shape(),
                        data
                    );
                    if child_is_initialized {
                        data.set(field_idx); // Parent reclaims responsibility only if child was init
                    }
                    crate::trace!("end() SLOW PATH (Enum): data after={:?}", data);
                    *current_child = None;
                }
                _ => {}
            }
            return Ok(self);
        }

        match &mut parent_frame.tracker {
            Tracker::SmartPointer => {
                // We just popped the inner value frame, so now we need to create the smart pointer
                if let Def::Pointer(smart_ptr_def) = parent_frame.allocated.shape().def {
                    // The inner value must be fully initialized before we can create the smart pointer
                    if let Err(e) = popped_frame.require_full_initialization() {
                        // Inner value wasn't initialized, deallocate and return error
                        popped_frame.deinit();
                        popped_frame.dealloc();
                        return Err(self.err(e));
                    }

                    let Some(new_into_fn) = smart_ptr_def.vtable.new_into_fn else {
                        popped_frame.deinit();
                        popped_frame.dealloc();
                        return Err(self.err(ReflectErrorKind::OperationFailed {
                            shape: parent_shape,
                            operation: "SmartPointer missing new_into_fn",
                        }));
                    };

                    // The child frame contained the inner value
                    let inner_ptr = PtrMut::new(popped_frame.data.as_mut_byte_ptr());

                    // Use new_into_fn to create the Box
                    unsafe {
                        new_into_fn(parent_frame.data, inner_ptr);
                    }

                    // We just moved out of it
                    popped_frame.tracker = Tracker::Scalar;
                    popped_frame.is_init = false;

                    // Deallocate the inner value's memory since new_into_fn moved it
                    popped_frame.dealloc();

                    parent_frame.is_init = true;
                }
            }
            Tracker::List {
                current_child,
                element_count,
            } if parent_frame.is_init => {
                if current_child.is_some() {
                    // We just popped an element frame, now add it to the list
                    if let Def::List(list_def) = parent_shape.def {
                        // Check if we used direct-fill (ListSlot) or heap allocation (Owned)
                        if matches!(popped_frame.ownership, FrameOwnership::ListSlot) {
                            // Direct-fill: element was written directly into Vec's buffer
                            // Just increment the length
                            let Some(set_len_fn) = list_def.set_len() else {
                                return Err(self.err(ReflectErrorKind::OperationFailed {
                                    shape: parent_shape,
                                    operation: "List missing set_len function for direct-fill",
                                }));
                            };
                            let current_len = unsafe {
                                (list_def.vtable.len)(parent_frame.data.assume_init().as_const())
                            };
                            unsafe {
                                set_len_fn(parent_frame.data.assume_init(), current_len + 1);
                            }
                            // Increment element_count to track how many elements we've pushed
                            // (used by begin_list_item to compute offsets)
                            *element_count += 1;
                            // No dealloc needed - memory belongs to Vec
                        } else {
                            // Fallback: element is in separate heap buffer, use push to copy
                            let Some(push_fn) = list_def.push() else {
                                return Err(self.err(ReflectErrorKind::OperationFailed {
                                    shape: parent_shape,
                                    operation: "List missing push function",
                                }));
                            };

                            // The child frame contained the element value
                            let element_ptr = PtrMut::new(popped_frame.data.as_mut_byte_ptr());

                            // Use push to add element to the list
                            unsafe {
                                push_fn(
                                    PtrMut::new(parent_frame.data.as_mut_byte_ptr()),
                                    element_ptr,
                                );
                            }

                            // Push moved out of popped_frame
                            popped_frame.tracker = Tracker::Scalar;
                            popped_frame.is_init = false;
                            popped_frame.dealloc();
                            // Increment element_count to track how many elements we've pushed
                            *element_count += 1;
                        }

                        *current_child = None;
                    }
                }
            }
            Tracker::Map { insert_state } if parent_frame.is_init => {
                match insert_state {
                    MapInsertState::PushingKey { key_ptr, .. } => {
                        // We just popped the key frame - mark key as initialized and transition
                        // to PushingValue state. key_frame_on_stack = false because the frame
                        // was just popped, so Map now owns the key buffer.
                        *insert_state = MapInsertState::PushingValue {
                            key_ptr: *key_ptr,
                            value_ptr: None,
                            value_initialized: false,
                            value_frame_on_stack: false, // No value frame yet
                        };
                    }
                    MapInsertState::PushingValue {
                        key_ptr, value_ptr, ..
                    } => {
                        // We just popped the value frame, now insert the pair
                        if let (Some(value_ptr), Def::Map(map_def)) =
                            (*value_ptr, parent_frame.allocated.shape().def)
                        {
                            // Capture what we need before potentially borrowing self.mode
                            let map_ptr = parent_frame.data;
                            let key_ptr = *key_ptr;

                            if is_deferred {
                                // In deferred mode, we can't insert yet because the value may not
                                // be fully initialized (e.g., Option fields that need to default to None).
                                // Store the pending insertion for processing in finish_deferred().
                                if let FrameMode::Deferred {
                                    pending_map_insertions,
                                    ..
                                } = &mut self.mode
                                {
                                    crate::trace!(
                                        "end(): Deferring Map insertion for value shape={}",
                                        map_def.v()
                                    );
                                    pending_map_insertions.push(
                                        crate::partial::PendingMapInsertion {
                                            map_ptr,
                                            map_def,
                                            key_ptr,
                                            value_ptr,
                                        },
                                    );
                                }
                                // Reset to idle but DON'T deallocate - finish_deferred will handle it
                                if let Some(parent_frame) = self.frames_mut().last_mut() {
                                    if let Tracker::Map { insert_state } = &mut parent_frame.tracker
                                    {
                                        *insert_state = MapInsertState::Idle;
                                    }
                                }
                            } else {
                                // Not in deferred mode - insert immediately
                                let insert = map_def.vtable.insert;

                                // Use insert to add key-value pair to the map
                                unsafe {
                                    insert(
                                        PtrMut::new(map_ptr.as_mut_byte_ptr()),
                                        PtrMut::new(key_ptr.as_mut_byte_ptr()),
                                        PtrMut::new(value_ptr.as_mut_byte_ptr()),
                                    );
                                }

                                // Note: We don't deallocate the key and value memory here.
                                // The insert function has semantically moved the values into the map,
                                // but we still need to deallocate the temporary buffers.
                                // However, since we don't have frames for them anymore (they were popped),
                                // we need to handle deallocation here.
                                if let Ok(key_shape) = map_def.k().layout.sized_layout()
                                    && key_shape.size() > 0
                                {
                                    unsafe {
                                        ::alloc::alloc::dealloc(
                                            key_ptr.as_mut_byte_ptr(),
                                            key_shape,
                                        );
                                    }
                                }
                                if let Ok(value_shape) = map_def.v().layout.sized_layout()
                                    && value_shape.size() > 0
                                {
                                    unsafe {
                                        ::alloc::alloc::dealloc(
                                            value_ptr.as_mut_byte_ptr(),
                                            value_shape,
                                        );
                                    }
                                }

                                // Reset to idle state
                                *insert_state = MapInsertState::Idle;
                            }
                        }
                    }
                    MapInsertState::Idle => {
                        // Nothing to do
                    }
                }
            }
            Tracker::Set { current_child } if parent_frame.is_init => {
                if *current_child {
                    // We just popped an element frame, now insert it into the set
                    if let Def::Set(set_def) = parent_frame.allocated.shape().def {
                        let insert = set_def.vtable.insert;

                        // The child frame contained the element value
                        let element_ptr = PtrMut::new(popped_frame.data.as_mut_byte_ptr());

                        // Use insert to add element to the set
                        unsafe {
                            insert(
                                PtrMut::new(parent_frame.data.as_mut_byte_ptr()),
                                element_ptr,
                            );
                        }

                        // Insert moved out of popped_frame
                        popped_frame.tracker = Tracker::Scalar;
                        popped_frame.is_init = false;
                        popped_frame.dealloc();

                        *current_child = false;
                    }
                }
            }
            Tracker::Option { building_inner } => {
                crate::trace!(
                    "end(): matched Tracker::Option, building_inner={}",
                    *building_inner
                );
                // We just popped the inner value frame for an Option's Some variant
                if *building_inner {
                    if let Def::Option(option_def) = parent_frame.allocated.shape().def {
                        // Use the Option vtable to initialize Some(inner_value)
                        let init_some_fn = option_def.vtable.init_some;

                        // The popped frame contains the inner value
                        let inner_value_ptr = unsafe { popped_frame.data.assume_init().as_const() };

                        // Initialize the Option as Some(inner_value)
                        unsafe {
                            init_some_fn(parent_frame.data, inner_value_ptr);
                        }

                        // Deallocate the inner value's memory since init_some_fn moved it
                        if let FrameOwnership::Owned = popped_frame.ownership
                            && let Ok(layout) = popped_frame.allocated.shape().layout.sized_layout()
                            && layout.size() > 0
                        {
                            unsafe {
                                ::alloc::alloc::dealloc(
                                    popped_frame.data.as_mut_byte_ptr(),
                                    layout,
                                );
                            }
                        }

                        // Mark that we're no longer building the inner value
                        *building_inner = false;
                        crate::trace!("end(): set building_inner to false");
                        // Mark the Option as initialized
                        parent_frame.is_init = true;
                        crate::trace!("end(): set parent_frame.is_init to true");
                    } else {
                        return Err(self.err(ReflectErrorKind::OperationFailed {
                            shape: parent_shape,
                            operation: "Option frame without Option definition",
                        }));
                    }
                } else {
                    // building_inner is false - the Option was already initialized but
                    // begin_some was called again. The popped frame was not used to
                    // initialize the Option, so we need to clean it up.
                    popped_frame.deinit();
                    if let FrameOwnership::Owned = popped_frame.ownership
                        && let Ok(layout) = popped_frame.allocated.shape().layout.sized_layout()
                        && layout.size() > 0
                    {
                        unsafe {
                            ::alloc::alloc::dealloc(popped_frame.data.as_mut_byte_ptr(), layout);
                        }
                    }
                }
            }
            Tracker::Result {
                is_ok,
                building_inner,
            } => {
                crate::trace!(
                    "end(): matched Tracker::Result, is_ok={}, building_inner={}",
                    *is_ok,
                    *building_inner
                );
                // We just popped the inner value frame for a Result's Ok or Err variant
                if *building_inner {
                    if let Def::Result(result_def) = parent_frame.allocated.shape().def {
                        // The popped frame contains the inner value
                        let inner_value_ptr = unsafe { popped_frame.data.assume_init().as_const() };

                        // Initialize the Result as Ok(inner_value) or Err(inner_value)
                        if *is_ok {
                            let init_ok_fn = result_def.vtable.init_ok;
                            unsafe {
                                init_ok_fn(parent_frame.data, inner_value_ptr);
                            }
                        } else {
                            let init_err_fn = result_def.vtable.init_err;
                            unsafe {
                                init_err_fn(parent_frame.data, inner_value_ptr);
                            }
                        }

                        // Deallocate the inner value's memory since init_ok/err_fn moved it
                        if let FrameOwnership::Owned = popped_frame.ownership
                            && let Ok(layout) = popped_frame.allocated.shape().layout.sized_layout()
                            && layout.size() > 0
                        {
                            unsafe {
                                ::alloc::alloc::dealloc(
                                    popped_frame.data.as_mut_byte_ptr(),
                                    layout,
                                );
                            }
                        }

                        // Mark that we're no longer building the inner value
                        *building_inner = false;
                        crate::trace!("end(): set building_inner to false");
                        // Mark the Result as initialized
                        parent_frame.is_init = true;
                        crate::trace!("end(): set parent_frame.is_init to true");
                    } else {
                        return Err(self.err(ReflectErrorKind::OperationFailed {
                            shape: parent_shape,
                            operation: "Result frame without Result definition",
                        }));
                    }
                } else {
                    // building_inner is false - the Result was already initialized but
                    // begin_ok/begin_err was called again. The popped frame was not used to
                    // initialize the Result, so we need to clean it up.
                    popped_frame.deinit();
                    if let FrameOwnership::Owned = popped_frame.ownership
                        && let Ok(layout) = popped_frame.allocated.shape().layout.sized_layout()
                        && layout.size() > 0
                    {
                        unsafe {
                            ::alloc::alloc::dealloc(popped_frame.data.as_mut_byte_ptr(), layout);
                        }
                    }
                }
            }
            Tracker::Scalar => {
                // the main case here is: the popped frame was a `String` and the
                // parent frame is an `Arc<str>`, `Box<str>` etc.
                match &parent_shape.def {
                    Def::Pointer(smart_ptr_def) => {
                        let pointee = match smart_ptr_def.pointee() {
                            Some(p) => p,
                            None => {
                                return Err(self.err(ReflectErrorKind::InvariantViolation {
                                    invariant: "pointer type doesn't have a pointee",
                                }));
                            }
                        };

                        if !pointee.is_shape(str::SHAPE) {
                            return Err(self.err(ReflectErrorKind::InvariantViolation {
                                invariant: "only T=str is supported when building SmartPointer<T> and T is unsized",
                            }));
                        }

                        if !popped_frame.allocated.shape().is_shape(String::SHAPE) {
                            return Err(self.err(ReflectErrorKind::InvariantViolation {
                                invariant: "the popped frame should be String when building a SmartPointer<T>",
                            }));
                        }

                        if let Err(e) = popped_frame.require_full_initialization() {
                            return Err(self.err(e));
                        }

                        // if the just-popped frame was a SmartPointerStr, we have some conversion to do:
                        // Special-case: SmartPointer<str> (Box<str>, Arc<str>, Rc<str>) via SmartPointerStr tracker
                        // Here, popped_frame actually contains a value for String that should be moved into the smart pointer.
                        // We convert the String into Box<str>, Arc<str>, or Rc<str> as appropriate and write it to the parent frame.
                        use ::alloc::{rc::Rc, string::String, sync::Arc};

                        let Some(known) = smart_ptr_def.known else {
                            return Err(self.err(ReflectErrorKind::OperationFailed {
                                shape: parent_shape,
                                operation: "SmartPointerStr for unknown smart pointer kind",
                            }));
                        };

                        parent_frame.deinit();

                        // Interpret the memory as a String, then convert and write.
                        let string_ptr = popped_frame.data.as_mut_byte_ptr() as *mut String;
                        let string_value = unsafe { core::ptr::read(string_ptr) };

                        match known {
                            KnownPointer::Box => {
                                let boxed: Box<str> = string_value.into_boxed_str();
                                unsafe {
                                    core::ptr::write(
                                        parent_frame.data.as_mut_byte_ptr() as *mut Box<str>,
                                        boxed,
                                    );
                                }
                            }
                            KnownPointer::Arc => {
                                let arc: Arc<str> = Arc::from(string_value.into_boxed_str());
                                unsafe {
                                    core::ptr::write(
                                        parent_frame.data.as_mut_byte_ptr() as *mut Arc<str>,
                                        arc,
                                    );
                                }
                            }
                            KnownPointer::Rc => {
                                let rc: Rc<str> = Rc::from(string_value.into_boxed_str());
                                unsafe {
                                    core::ptr::write(
                                        parent_frame.data.as_mut_byte_ptr() as *mut Rc<str>,
                                        rc,
                                    );
                                }
                            }
                            _ => {
                                return Err(self.err(ReflectErrorKind::OperationFailed {
                                    shape: parent_shape,
                                    operation: "Don't know how to build this pointer type",
                                }));
                            }
                        }

                        parent_frame.is_init = true;

                        popped_frame.tracker = Tracker::Scalar;
                        popped_frame.is_init = false;
                        popped_frame.dealloc();
                    }
                    _ => {
                        // This can happen if begin_inner() was called on a type that
                        // has shape.inner but isn't a SmartPointer (e.g., Option).
                        // In this case, we can't complete the conversion, so return error.
                        return Err(self.err(ReflectErrorKind::OperationFailed {
                            shape: parent_shape,
                            operation: "end() called but parent has Uninit/Init tracker and isn't a SmartPointer",
                        }));
                    }
                }
            }
            Tracker::SmartPointerSlice {
                vtable,
                building_item,
            } => {
                if *building_item {
                    // We just popped an element frame, now push it to the slice builder
                    let element_ptr = PtrMut::new(popped_frame.data.as_mut_byte_ptr());

                    // Use the slice builder's push_fn to add the element
                    crate::trace!("Pushing element to slice builder");
                    unsafe {
                        let parent_ptr = parent_frame.data.assume_init();
                        (vtable.push_fn)(parent_ptr, element_ptr);
                    }

                    popped_frame.tracker = Tracker::Scalar;
                    popped_frame.is_init = false;
                    popped_frame.dealloc();

                    if let Tracker::SmartPointerSlice {
                        building_item: bi, ..
                    } = &mut parent_frame.tracker
                    {
                        *bi = false;
                    }
                }
            }
            Tracker::DynamicValue {
                state: DynamicValueState::Array { building_element },
            } => {
                if *building_element {
                    // Check that the element is initialized before pushing
                    if !popped_frame.is_init {
                        // Element was never set - clean up and return error
                        let shape = parent_frame.allocated.shape();
                        popped_frame.dealloc();
                        *building_element = false;
                        // No need to poison - returning Err consumes self, Drop will handle cleanup
                        return Err(self.err(ReflectErrorKind::OperationFailed {
                            shape,
                            operation: "end() called but array element was never initialized",
                        }));
                    }

                    // We just popped an element frame, now push it to the dynamic array
                    if let Def::DynamicValue(dyn_def) = parent_frame.allocated.shape().def {
                        // Get mutable pointers - both array and element need PtrMut
                        let array_ptr = unsafe { parent_frame.data.assume_init() };
                        let element_ptr = unsafe { popped_frame.data.assume_init() };

                        // Use push_array_element to add element to the array
                        unsafe {
                            (dyn_def.vtable.push_array_element)(array_ptr, element_ptr);
                        }

                        // Push moved out of popped_frame
                        popped_frame.tracker = Tracker::Scalar;
                        popped_frame.is_init = false;
                        popped_frame.dealloc();

                        *building_element = false;
                    }
                }
            }
            Tracker::DynamicValue {
                state: DynamicValueState::Object { insert_state },
            } => {
                if let DynamicObjectInsertState::BuildingValue { key } = insert_state {
                    // Check that the value is initialized before inserting
                    if !popped_frame.is_init {
                        // Value was never set - clean up and return error
                        let shape = parent_frame.allocated.shape();
                        popped_frame.dealloc();
                        *insert_state = DynamicObjectInsertState::Idle;
                        // No need to poison - returning Err consumes self, Drop will handle cleanup
                        return Err(self.err(ReflectErrorKind::OperationFailed {
                            shape,
                            operation: "end() called but object entry value was never initialized",
                        }));
                    }

                    // We just popped a value frame, now insert it into the dynamic object
                    if let Def::DynamicValue(dyn_def) = parent_frame.allocated.shape().def {
                        // Get mutable pointers - both object and value need PtrMut
                        let object_ptr = unsafe { parent_frame.data.assume_init() };
                        let value_ptr = unsafe { popped_frame.data.assume_init() };

                        // Use insert_object_entry to add the key-value pair
                        unsafe {
                            (dyn_def.vtable.insert_object_entry)(object_ptr, key, value_ptr);
                        }

                        // Insert moved out of popped_frame
                        popped_frame.tracker = Tracker::Scalar;
                        popped_frame.is_init = false;
                        popped_frame.dealloc();

                        // Reset insert state to Idle
                        *insert_state = DynamicObjectInsertState::Idle;
                    }
                }
            }
            _ => {}
        }

        Ok(self)
    }

    /// Returns the root shape for path formatting.
    ///
    /// Use this together with [`path()`](Self::path) to format the path:
    /// ```ignore
    /// let path_str = partial.path().format_with_shape(partial.root_shape());
    /// ```
    pub fn root_shape(&self) -> &'static Shape {
        self.frames()
            .first()
            .expect("Partial should always have at least one frame")
            .allocated
            .shape()
    }

    /// Create a [`ReflectError`] with the current path context.
    ///
    /// This is a convenience method for constructing errors inside `Partial` methods
    /// that automatically captures the current traversal path.
    #[inline]
    pub fn err(&self, kind: ReflectErrorKind) -> ReflectError {
        ReflectError::new(kind, self.path())
    }

    /// Get the field for the parent frame
    pub fn parent_field(&self) -> Option<&Field> {
        self.frames()
            .iter()
            .rev()
            .nth(1)
            .and_then(|f| f.get_field())
    }

    /// Gets the field for the current frame
    pub fn current_field(&self) -> Option<&Field> {
        self.frames().last().and_then(|f| f.get_field())
    }

    /// Returns a const pointer to the current frame's data.
    ///
    /// This is useful for validation - after deserializing a field value,
    /// validators can read the value through this pointer.
    ///
    /// # Safety
    ///
    /// The returned pointer is valid only while the frame exists.
    /// The caller must ensure the frame is fully initialized before
    /// reading through this pointer.
    #[deprecated(note = "use initialized_data_ptr() instead, which checks initialization")]
    pub fn data_ptr(&self) -> Option<facet_core::PtrConst> {
        if self.state != PartialState::Active {
            return None;
        }
        self.frames().last().map(|f| {
            // SAFETY: We're in active state, so the frame is valid.
            // The caller is responsible for ensuring the data is initialized.
            unsafe { f.data.assume_init().as_const() }
        })
    }

    /// Returns a const pointer to the current frame's data, but only if fully initialized.
    ///
    /// This is the safe way to get a pointer for validation - it verifies that
    /// the frame is fully initialized before returning the pointer.
    ///
    /// Returns `None` if:
    /// - The partial is not in active state
    /// - The current frame is not fully initialized
    #[allow(unsafe_code)]
    pub fn initialized_data_ptr(&self) -> Option<facet_core::PtrConst> {
        if self.state != PartialState::Active {
            return None;
        }
        let frame = self.frames().last()?;

        // Check if fully initialized
        if frame.require_full_initialization().is_err() {
            return None;
        }

        // SAFETY: We've verified the partial is active and the frame is fully initialized.
        Some(unsafe { frame.data.assume_init().as_const() })
    }

    /// Returns a typed reference to the current frame's data if:
    /// 1. The partial is in active state
    /// 2. The current frame is fully initialized
    /// 3. The shape matches `T::SHAPE`
    ///
    /// This is the safe way to read a value from a Partial for validation purposes.
    #[allow(unsafe_code)]
    pub fn read_as<T: facet_core::Facet<'facet>>(&self) -> Option<&T> {
        if self.state != PartialState::Active {
            return None;
        }
        let frame = self.frames().last()?;

        // Check if fully initialized
        if frame.require_full_initialization().is_err() {
            return None;
        }

        // Check shape matches
        if frame.allocated.shape() != T::SHAPE {
            return None;
        }

        // SAFETY: We've verified:
        // 1. The partial is active (frame is valid)
        // 2. The frame is fully initialized
        // 3. The shape matches T::SHAPE
        unsafe {
            let ptr = frame.data.assume_init().as_const();
            Some(&*ptr.as_ptr::<T>())
        }
    }
}
