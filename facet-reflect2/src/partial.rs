//! Partial value construction with tree-based tracking.
//!
//! The `Partial` struct manages incremental construction of complex values,
//! tracking initialization state and ownership across nested structures.

use crate::arena::{Arena, FrameId};
use crate::frame::{Children, Frame, FrameFlags};
use facet_core::{Facet, PtrUninit, Shape};

/// A complete value to move into the destination.
///
/// The caller must ensure:
/// - `ptr` points to a valid, initialized value of type matching `shape`
/// - The value will not be dropped by the caller (ownership transfers)
pub struct Move {
    /// Pointer to the source value
    pub ptr: PtrUninit,

    /// Shape of the source value (must match destination)
    pub shape: &'static Shape,
}

/// Build a value incrementally by pushing a frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct Build {
    /// Hint for collection capacity (ignored for non-collections)
    pub len_hint: Option<usize>,

    /// Whether this frame can be deferred (left incomplete and re-entered)
    pub deferred: bool,
}

/// Source for a value being set.
pub enum Source {
    /// Move a complete value from the pointer into destination.
    /// No frame is pushed. The source is consumed.
    Move(Move),

    /// Build incrementally. Pushes a frame.
    Build(Build),

    /// Use the type's default value.
    /// Works for empty collections, `None`, and `#[facet(default)]` fields.
    Default,
}

/// Error during partial value construction.
#[derive(Debug)]
pub enum PartialError {
    /// Attempted operation on a poisoned Partial
    Poisoned,

    /// Shape mismatch between source and destination
    ShapeMismatch {
        expected: &'static Shape,
        got: &'static Shape,
    },

    /// Path index out of bounds
    PathOutOfBounds { index: usize, len: usize },

    /// No frame to end (already at root or root is complete)
    NoFrameToEnd,

    /// Frame is incomplete and not in deferred mode
    IncompleteFrame,

    /// Type has no default value
    NoDefault { shape: &'static Shape },

    /// Cannot push to non-list type
    NotAList { shape: &'static Shape },

    /// Cannot insert into non-map type
    NotAMap { shape: &'static Shape },

    /// Field already initialized (and not in deferred mode for replacement)
    AlreadyInitialized { index: usize },

    /// Variant already selected (must switch variant explicitly)
    VariantAlreadySelected { current: u32, requested: u32 },

    /// Multi-element path in non-deferred mode
    MultiElementPathRequiresDeferred,
}

/// Result type for Partial operations.
pub type Result<T> = std::result::Result<T, PartialError>;

/// Manages incremental construction of a value.
///
/// # Lifecycle
///
/// 1. Create with `Partial::new::<T>()` or `Partial::new_with_ptr()`
/// 2. Use `set()`, `push()`, `insert()` to fill values
/// 3. Use `end()` to complete frames
/// 4. Call `build()` to extract the final value
///
/// On any error, the Partial becomes poisoned and all resources are cleaned up.
pub struct Partial {
    /// Frame storage with free list
    arena: Arena,

    /// Root frame ID (never a sentinel for a live Partial)
    root: FrameId,

    /// Current frame being built
    current: FrameId,

    /// Whether this Partial has encountered an error
    poisoned: bool,

    /// Depth at which deferred mode started (0 = not deferred)
    deferred_depth: usize,

    /// Current frame depth (root = 1)
    current_depth: usize,
}

impl Partial {
    /// Create a new Partial for constructing a value of type `T`.
    ///
    /// Allocates memory for the root value.
    pub fn alloc<T: Facet>() -> Self {
        // Allocate memory for the root value
        let layout = std::alloc::Layout::new::<T>();
        let ptr = if layout.size() == 0 {
            // ZST - use dangling pointer
            PtrUninit::dangling::<T>()
        } else {
            // Safety: layout has non-zero size
            let raw = unsafe { std::alloc::alloc(layout) };
            if raw.is_null() {
                std::alloc::handle_alloc_error(layout);
            }
            // Safety: raw is valid, properly aligned for T
            unsafe { PtrUninit::new(raw.cast()) }
        };

        Self::new(ptr, T::SHAPE, true)
    }

    /// Create a new Partial writing into existing memory.
    ///
    /// # Safety
    ///
    /// The caller must ensure:
    /// - `ptr` points to valid, properly aligned memory for `shape`
    /// - The memory will remain valid for the lifetime of this Partial
    /// - If `owns_allocation` is true, the memory was allocated with the global allocator
    pub fn new(ptr: PtrUninit, shape: &'static Shape, owns_allocation: bool) -> Self {
        let mut arena = Arena::new();

        // Determine initial children structure based on shape
        let children = children_for_shape(shape);

        let mut root_frame = Frame::new(None, ptr, shape, children);
        if owns_allocation {
            root_frame.set_owns_allocation();
        }

        let root = arena.alloc(root_frame);

        Partial {
            arena,
            root,
            current: root,
            poisoned: false,
            deferred_depth: 0,
            current_depth: 1,
        }
    }

    /// Returns true if this Partial has encountered an error.
    pub fn is_poisoned(&self) -> bool {
        self.poisoned
    }

    /// Returns true if currently in deferred mode.
    pub fn is_deferred(&self) -> bool {
        self.deferred_depth > 0 && self.current_depth >= self.deferred_depth
    }

    /// Check if poisoned, returning error if so.
    fn check_poisoned(&self) -> Result<()> {
        if self.poisoned {
            Err(PartialError::Poisoned)
        } else {
            Ok(())
        }
    }

    /// Poison this Partial and clean up all resources.
    fn poison(&mut self) {
        if self.poisoned {
            return;
        }
        self.poisoned = true;

        // TODO: Walk tree, drop initialized values, free allocations
        // For now, just mark as poisoned
    }

    /// Set a value at a path relative to the current frame.
    pub fn set(&mut self, path: &[usize], source: Source) -> Result<()> {
        self.check_poisoned()?;

        // Multi-element paths require deferred mode
        if path.len() > 1 && !self.is_deferred() {
            return Err(PartialError::MultiElementPathRequiresDeferred);
        }

        // Empty path: set the current frame's value directly
        if path.is_empty() {
            return self.set_current(source);
        }

        // Navigate to the target, potentially through stored frames in deferred mode
        let target_index = path[0];

        // For now, only handle single-element paths
        // TODO: Handle multi-element paths in deferred mode
        if path.len() > 1 {
            todo!("multi-element path navigation in deferred mode");
        }

        self.set_child(target_index, source)
    }

    /// Set the current frame's value directly.
    fn set_current(&mut self, source: Source) -> Result<()> {
        match source {
            Source::Move(mov) => {
                let frame = self.arena.get_mut(self.current);

                // Verify shape matches
                if !std::ptr::eq(frame.shape, mov.shape) {
                    return Err(PartialError::ShapeMismatch {
                        expected: frame.shape,
                        got: mov.shape,
                    });
                }

                // Copy bytes from source to destination
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        mov.ptr.as_ptr(),
                        frame.data.as_mut_ptr(),
                        frame.shape.layout.size(),
                    );
                }

                frame.set_init();
                Ok(())
            }

            Source::Build(build) => {
                // Entering build mode for current frame
                if build.deferred && self.deferred_depth == 0 {
                    self.deferred_depth = self.current_depth;
                }

                // TODO: Handle len_hint for collections
                let _ = build.len_hint;

                Ok(())
            }

            Source::Default => {
                let frame = self.arena.get_mut(self.current);

                // Check if type has a default
                if let Some(default_fn) = frame.shape.vtable.default_fn {
                    // Safety: frame.data points to valid memory for this shape
                    unsafe {
                        default_fn(frame.data);
                    }
                    frame.set_init();
                    Ok(())
                } else {
                    Err(PartialError::NoDefault { shape: frame.shape })
                }
            }
        }
    }

    /// Set a child at the given index.
    fn set_child(&mut self, index: usize, source: Source) -> Result<()> {
        let current_frame = self.arena.get(self.current);
        let _current_shape = current_frame.shape;

        // Get child info based on current frame's children type
        match &current_frame.children {
            Children::Indexed(children) => {
                if index >= children.len() {
                    return Err(PartialError::PathOutOfBounds {
                        index,
                        len: children.len(),
                    });
                }

                let child_state = children[index];

                if child_state.is_complete() && !self.is_deferred() {
                    return Err(PartialError::AlreadyInitialized { index });
                }

                if child_state.is_in_progress() {
                    // Re-entry: make the existing frame current
                    self.current = child_state;
                    self.current_depth += 1;
                    return self.set_current(source);
                }

                // Child not started - need to create new frame
                self.create_child_frame(index, source)
            }

            Children::Variant(variant_state) => {
                // For enums, index is the variant index
                if let Some((current_variant, state)) = variant_state {
                    if *current_variant != index as u32 {
                        // TODO: Variant switching - drop current, start new
                        return Err(PartialError::VariantAlreadySelected {
                            current: *current_variant,
                            requested: index as u32,
                        });
                    }

                    if state.is_complete() && !self.is_deferred() {
                        return Err(PartialError::AlreadyInitialized { index });
                    }

                    if state.is_in_progress() {
                        // Re-entry
                        self.current = *state;
                        self.current_depth += 1;
                        return self.set_current(source);
                    }
                }

                // Create variant frame
                self.create_variant_frame(index as u32, source)
            }

            Children::Single(child_state) => {
                if index != 0 {
                    return Err(PartialError::PathOutOfBounds { index, len: 1 });
                }

                if child_state.is_complete() && !self.is_deferred() {
                    return Err(PartialError::AlreadyInitialized { index: 0 });
                }

                if child_state.is_in_progress() {
                    self.current = *child_state;
                    self.current_depth += 1;
                    return self.set_current(source);
                }

                self.create_single_child_frame(source)
            }

            Children::List(_) => {
                // Lists use Push, not Set with index
                // But in deferred mode, we might re-enter by index
                todo!("list indexing in deferred mode")
            }

            Children::Map(_) => {
                // Maps use Insert, not Set with index
                Err(PartialError::NotAMap {
                    shape: current_frame.shape,
                })
            }

            Children::None => {
                // Scalars have no children
                Err(PartialError::PathOutOfBounds { index, len: 0 })
            }
        }
    }

    /// Create a new child frame at the given index.
    fn create_child_frame(&mut self, index: usize, source: Source) -> Result<()> {
        // Get child shape and pointer
        let (child_ptr, child_shape) = {
            let frame = self.arena.get(self.current);
            get_indexed_child(frame, index)?
        };

        match source {
            Source::Move(mov) => {
                // Verify shape matches
                if !std::ptr::eq(child_shape, mov.shape) {
                    return Err(PartialError::ShapeMismatch {
                        expected: child_shape,
                        got: mov.shape,
                    });
                }

                // Copy bytes directly, no frame needed
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        mov.ptr.as_ptr(),
                        child_ptr.as_mut_ptr(),
                        child_shape.layout.size(),
                    );
                }

                // Mark as complete in parent
                let frame = self.arena.get_mut(self.current);
                if let Children::Indexed(ref mut children) = frame.children {
                    children[index] = FrameId::COMPLETE;
                }

                Ok(())
            }

            Source::Build(build) => {
                // Create child frame
                let children = children_for_shape(child_shape);
                let child_frame = Frame::new(Some(self.current), child_ptr, child_shape, children);
                let child_id = self.arena.alloc(child_frame);

                // Store in parent
                let frame = self.arena.get_mut(self.current);
                if let Children::Indexed(ref mut children) = frame.children {
                    children[index] = child_id;
                }

                // Enter the new frame
                self.current = child_id;
                self.current_depth += 1;

                if build.deferred && self.deferred_depth == 0 {
                    self.deferred_depth = self.current_depth;
                }

                Ok(())
            }

            Source::Default => {
                // Check if type has a default
                if let Some(default_fn) = child_shape.vtable.default_fn {
                    unsafe {
                        default_fn(child_ptr);
                    }

                    // Mark as complete in parent
                    let frame = self.arena.get_mut(self.current);
                    if let Children::Indexed(ref mut children) = frame.children {
                        children[index] = FrameId::COMPLETE;
                    }

                    Ok(())
                } else {
                    Err(PartialError::NoDefault { shape: child_shape })
                }
            }
        }
    }

    /// Create a variant frame for an enum.
    fn create_variant_frame(&mut self, variant_idx: u32, source: Source) -> Result<()> {
        // TODO: Get variant shape and create frame
        let _ = (variant_idx, source);
        todo!("create_variant_frame")
    }

    /// Create a single child frame (for Option, Box, etc.).
    fn create_single_child_frame(&mut self, source: Source) -> Result<()> {
        // TODO: Get inner shape and create frame
        let _ = source;
        todo!("create_single_child_frame")
    }

    /// End the current frame, returning to the parent.
    pub fn end(&mut self) -> Result<()> {
        self.check_poisoned()?;

        if self.current == self.root {
            // Can't end root frame via end() - use build()
            return Err(PartialError::NoFrameToEnd);
        }

        let frame = self.arena.get(self.current);
        let parent_id = frame.parent.expect("non-root frame must have parent");

        // Check if frame is complete
        let is_complete = self.check_frame_complete(self.current);

        if !is_complete && !self.is_deferred() {
            return Err(PartialError::IncompleteFrame);
        }

        if is_complete {
            // Frame is complete - free it and mark complete in parent
            let frame = self.arena.free(self.current);

            // Update parent's children
            let parent = self.arena.get_mut(parent_id);
            update_child_complete(parent, self.current);

            // Mark parent's value as partially/fully init
            // TODO: Check if this completes parent too

            let _ = frame; // Frame is dropped
        }
        // else: deferred mode - leave frame in place

        self.current = parent_id;
        self.current_depth -= 1;

        // Exit deferred mode if we've returned to the level that started it
        if self.deferred_depth > 0 && self.current_depth < self.deferred_depth {
            self.deferred_depth = 0;
        }

        Ok(())
    }

    /// Push an element to the current list.
    pub fn push(&mut self, source: Source) -> Result<()> {
        self.check_poisoned()?;

        let frame = self.arena.get(self.current);
        if !matches!(frame.children, Children::List(_)) {
            return Err(PartialError::NotAList { shape: frame.shape });
        }

        // TODO: Implement push
        let _ = source;
        todo!("push")
    }

    /// Insert a key-value pair into the current map.
    pub fn insert(&mut self, key: Move, value: Source) -> Result<()> {
        self.check_poisoned()?;

        let frame = self.arena.get(self.current);
        if !matches!(frame.children, Children::Map(_)) {
            return Err(PartialError::NotAMap { shape: frame.shape });
        }

        // TODO: Implement insert
        let _ = (key, value);
        todo!("insert")
    }

    /// Check if a frame is complete (all children initialized).
    fn check_frame_complete(&self, frame_id: FrameId) -> bool {
        let frame = self.arena.get(frame_id);

        match &frame.children {
            Children::Indexed(children) => children.iter().all(|c| c.is_complete()),

            Children::Variant(state) => {
                matches!(state, Some((_, id)) if id.is_complete())
            }

            Children::List(_) => {
                // Lists are complete when IS_INIT is set
                frame.is_init()
            }

            Children::Map(_) => {
                // Maps are complete when IS_INIT is set
                frame.is_init()
            }

            Children::Single(child) => child.is_complete(),

            Children::None => {
                // Scalars are complete when IS_INIT is set
                frame.is_init()
            }
        }
    }

    /// Build the final value, consuming the Partial.
    ///
    /// # Safety
    ///
    /// The caller must ensure the type `T` matches the shape used to create this Partial.
    pub unsafe fn build<T: Facet>(mut self) -> Result<T> {
        self.check_poisoned()?;

        // Verify root is complete
        if !self.check_frame_complete(self.root) {
            self.poison();
            return Err(PartialError::IncompleteFrame);
        }

        let frame = self.arena.get(self.root);
        let ptr = frame.data.as_ptr() as *const T;

        // Read the value out
        let value = std::ptr::read(ptr);

        // Prevent drop/dealloc since we're taking ownership
        // Mark as not owning allocation (we moved out the value)
        // The arena will be dropped but won't try to free this memory
        // since we're taking ownership of the value

        // Actually, we need to handle this carefully:
        // - If root owns allocation, we need to NOT deallocate
        // - The value has been moved out, so we shouldn't drop it

        // For now, just return the value
        // TODO: Proper cleanup handling
        std::mem::forget(self);

        Ok(value)
    }
}

impl Drop for Partial {
    fn drop(&mut self) {
        // Clean up any remaining frames
        // TODO: Walk tree, drop initialized values, free allocations
    }
}

/// Determine the appropriate Children variant for a shape.
fn children_for_shape(shape: &'static Shape) -> Children {
    use facet_core::Def;

    match shape.def {
        Def::Struct(struct_def) => {
            let field_count = struct_def.fields.len();
            Children::Indexed(vec![FrameId::NOT_STARTED; field_count])
        }

        Def::Enum(_) => Children::Variant(None),

        Def::Array(array_def) => Children::Indexed(vec![FrameId::NOT_STARTED; array_def.len]),

        Def::List(_) => Children::List(Vec::new()),

        Def::Map(_) => Children::Map(hashbrown::HashMap::new()),

        Def::Option(_) => Children::Single(FrameId::NOT_STARTED),

        Def::SmartPointer(_) => Children::Single(FrameId::NOT_STARTED),

        Def::Set(_) => Children::None, // Sets can't be re-entered

        Def::Scalar(_) | Def::Opaque(_) => Children::None,
    }
}

/// Get the pointer and shape for an indexed child (struct field or array element).
fn get_indexed_child(frame: &Frame, index: usize) -> Result<(PtrUninit, &'static Shape)> {
    use facet_core::Def;

    match frame.shape.def {
        Def::Struct(struct_def) => {
            let fields = struct_def.fields;
            if index >= fields.len() {
                return Err(PartialError::PathOutOfBounds {
                    index,
                    len: fields.len(),
                });
            }

            let field = &fields[index];
            let field_ptr = unsafe { frame.data.field(field.offset) };
            Ok((field_ptr, field.shape()))
        }

        Def::Array(array_def) => {
            if index >= array_def.len {
                return Err(PartialError::PathOutOfBounds {
                    index,
                    len: array_def.len,
                });
            }

            let elem_shape = array_def.vtable.item_shape();
            let elem_size = elem_shape.layout.size();
            let elem_ptr = unsafe { frame.data.byte_offset(index * elem_size) };
            Ok((elem_ptr, elem_shape))
        }

        _ => Err(PartialError::PathOutOfBounds { index, len: 0 }),
    }
}

/// Update a parent's children to mark a child as complete.
fn update_child_complete(parent: &mut Frame, completed_child_id: FrameId) {
    match &mut parent.children {
        Children::Indexed(children) => {
            for child in children.iter_mut() {
                if *child == completed_child_id {
                    *child = FrameId::COMPLETE;
                    return;
                }
            }
        }

        Children::Variant(state) => {
            if let Some((_, ref mut id)) = state {
                if *id == completed_child_id {
                    *id = FrameId::COMPLETE;
                }
            }
        }

        Children::List(elements) => {
            for elem in elements.iter_mut() {
                if *elem == completed_child_id {
                    *elem = FrameId::COMPLETE;
                    return;
                }
            }
        }

        Children::Map(map) => {
            for value in map.values_mut() {
                if *value == completed_child_id {
                    *value = FrameId::COMPLETE;
                    return;
                }
            }
        }

        Children::Single(child) => {
            if *child == completed_child_id {
                *child = FrameId::COMPLETE;
            }
        }

        Children::None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_partial_for_scalar() {
        let partial = Partial::alloc::<u32>();
        assert!(!partial.is_poisoned());
        assert!(!partial.is_deferred());
    }

    #[test]
    fn set_scalar_with_move() {
        let mut partial = Partial::alloc::<u32>();

        let value = 42u32;
        let mov = Move {
            ptr: unsafe { PtrUninit::new(&value as *const u32 as *mut u32) },
            shape: <u32 as Facet>::SHAPE,
        };

        partial.set(&[], Source::Move(mov)).unwrap();

        // Prevent value from being dropped twice
        std::mem::forget(value);

        let result = unsafe { partial.build::<u32>() }.unwrap();
        assert_eq!(result, 42);
    }

    #[test]
    fn set_scalar_with_default() {
        let mut partial = Partial::alloc::<u32>();

        // u32 has Default
        partial.set(&[], Source::Default).unwrap();

        let result = unsafe { partial.build::<u32>() }.unwrap();
        assert_eq!(result, 0);
    }
}
