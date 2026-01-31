//! Frame for tracking partial value construction.

use crate::arena::{Arena, Idx};
use crate::errors::{ErrorLocation, ReflectError, ReflectErrorKind};
use crate::ops::Path;
use facet_core::{PtrConst, PtrMut, PtrUninit, SequenceType, Shape, Variant};

bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct FrameFlags: u8 {
        /// The value is initialized (for scalars)
        const INIT = 1 << 0;
        /// This frame owns its allocation
        const OWNS_ALLOC = 1 << 1;
    }
}

/// Indexed fields for structs, arrays, and variant data.
/// Each slot is either NOT_STARTED, COMPLETE, or a valid frame index.
pub struct IndexedFields(Vec<Idx<Frame>>);

impl IndexedFields {
    /// Create indexed fields with the given count, all NOT_STARTED.
    pub fn new(count: usize) -> Self {
        Self(vec![Idx::NOT_STARTED; count])
    }

    /// Mark a field as complete.
    pub fn mark_complete(&mut self, idx: usize) {
        self.0[idx] = Idx::COMPLETE;
    }

    /// Mark a field as not started (e.g., after dropping it before overwriting).
    pub fn mark_not_started(&mut self, idx: usize) {
        self.0[idx] = Idx::NOT_STARTED;
    }

    /// Check if all fields are complete.
    pub fn all_complete(&self) -> bool {
        self.0.iter().all(|id| id.is_complete())
    }

    /// Check if a field is complete.
    pub fn is_complete(&self, idx: usize) -> bool {
        self.0[idx].is_complete()
    }
}

/// Struct frame data.
pub struct StructFrame {
    pub fields: IndexedFields,
}

impl StructFrame {
    pub fn new(field_count: usize) -> Self {
        Self {
            fields: IndexedFields::new(field_count),
        }
    }

    pub fn is_complete(&self) -> bool {
        self.fields.all_complete()
    }

    pub fn mark_field_complete(&mut self, idx: usize) {
        self.fields.mark_complete(idx);
    }

    /// Mark a field as not started (after dropping it before overwriting).
    ///
    /// NOTE: This "leaks" an arena slot if the field was an in-progress child frame.
    /// TODO: Add tests for frame arena leaking scenarios.
    pub fn mark_field_not_started(&mut self, idx: usize) {
        self.fields.mark_not_started(idx);
    }
}

/// Enum frame data.
/// `selected` is None if no variant selected yet,
/// or Some((variant_idx, state)) where state is NOT_STARTED/COMPLETE/valid frame idx.
pub struct EnumFrame {
    pub selected: Option<(u32, Idx<Frame>)>,
}

impl EnumFrame {
    pub fn new() -> Self {
        Self { selected: None }
    }

    pub fn is_complete(&self) -> bool {
        matches!(self.selected, Some((_, idx)) if idx.is_complete())
    }
}

impl Default for EnumFrame {
    fn default() -> Self {
        Self::new()
    }
}

/// Variant data frame (inside an enum variant, building its fields).
pub struct VariantFrame {
    pub variant: &'static Variant,
    pub fields: IndexedFields,
}

impl VariantFrame {
    pub fn new(variant: &'static Variant) -> Self {
        Self {
            variant,
            fields: IndexedFields::new(variant.data.fields.len()),
        }
    }

    pub fn is_complete(&self) -> bool {
        self.fields.all_complete()
    }

    pub fn mark_field_complete(&mut self, idx: usize) {
        self.fields.mark_complete(idx);
    }

    pub fn mark_field_not_started(&mut self, idx: usize) {
        self.fields.mark_not_started(idx);
    }
}

/// Pointer frame data (inside a Box/Rc/Arc, building the pointee).
/// `inner` is NOT_STARTED, COMPLETE, or a valid frame index for the inner value.
pub struct PointerFrame {
    pub inner: Idx<Frame>,
}

impl PointerFrame {
    pub fn new() -> Self {
        Self {
            inner: Idx::NOT_STARTED,
        }
    }

    pub fn is_complete(&self) -> bool {
        self.inner.is_complete()
    }
}

impl Default for PointerFrame {
    fn default() -> Self {
        Self::new()
    }
}

/// List frame data (building a Vec or similar).
/// Tracks the initialized list and count of pushed elements.
pub struct ListFrame {
    /// Pointer to the initialized list (after init_in_place_with_capacity).
    pub list_ptr: PtrMut,
    /// Number of elements that have been pushed.
    pub len: usize,
}

impl ListFrame {
    pub fn new(list_ptr: PtrMut) -> Self {
        Self { list_ptr, len: 0 }
    }

    /// Lists are always "complete" since they have variable size.
    /// Completion just means End was called.
    pub fn is_complete(&self) -> bool {
        // Lists don't have a fixed number of elements to track,
        // they're complete when End is called
        true
    }
}

/// Map frame data (building a HashMap, BTreeMap, etc.).
/// Tracks the initialized map and count of inserted entries.
pub struct MapFrame {
    /// Pointer to the initialized map (after init_in_place_with_capacity).
    pub map_ptr: PtrMut,
    /// Number of entries that have been inserted.
    pub len: usize,
}

impl MapFrame {
    pub fn new(map_ptr: PtrMut) -> Self {
        Self { map_ptr, len: 0 }
    }

    /// Maps are always "complete" since they have variable size.
    /// Completion just means End was called.
    pub fn is_complete(&self) -> bool {
        true
    }
}

/// Set frame data (building a HashSet, BTreeSet, etc.).
/// Tracks the initialized set and count of inserted elements.
pub struct SetFrame {
    /// Pointer to the initialized set (after init_in_place_with_capacity).
    pub set_ptr: PtrMut,
    /// Number of elements that have been inserted.
    pub len: usize,
}

impl SetFrame {
    pub fn new(set_ptr: PtrMut) -> Self {
        Self { set_ptr, len: 0 }
    }

    /// Sets are always "complete" since they have variable size.
    /// Completion just means End was called.
    pub fn is_complete(&self) -> bool {
        true
    }
}

/// What kind of value this frame is building.
pub enum FrameKind {
    /// Scalar or opaque value - no children.
    Scalar,

    /// Struct with indexed fields.
    Struct(StructFrame),

    /// Enum - variant may or may not be selected.
    Enum(EnumFrame),

    /// Inside a variant, building its fields.
    VariantData(VariantFrame),

    /// Inside a pointer (Box/Rc/Arc), building the pointee.
    Pointer(PointerFrame),

    /// Building a list (Vec, etc.).
    List(ListFrame),

    /// Building a map (HashMap, BTreeMap, etc.).
    Map(MapFrame),

    /// Building a set (HashSet, BTreeSet, etc.).
    Set(SetFrame),
}

impl FrameKind {
    /// Check if this frame is complete.
    pub fn is_complete(&self) -> bool {
        match self {
            FrameKind::Scalar => false, // scalars use INIT flag instead
            FrameKind::Struct(s) => s.is_complete(),
            FrameKind::Enum(e) => e.is_complete(),
            FrameKind::VariantData(v) => v.is_complete(),
            FrameKind::Pointer(p) => p.is_complete(),
            FrameKind::List(l) => l.is_complete(),
            FrameKind::Map(m) => m.is_complete(),
            FrameKind::Set(s) => s.is_complete(),
        }
    }

    /// Mark a child field as complete (for Struct and VariantData).
    pub fn mark_field_complete(&mut self, idx: usize) {
        match self {
            FrameKind::Struct(s) => s.mark_field_complete(idx),
            FrameKind::VariantData(v) => v.mark_field_complete(idx),
            _ => {}
        }
    }

    /// Mark a child field as not started (after dropping it before overwriting).
    pub fn mark_field_not_started(&mut self, idx: usize) {
        match self {
            FrameKind::Struct(s) => s.mark_field_not_started(idx),
            FrameKind::VariantData(v) => v.mark_field_not_started(idx),
            _ => {}
        }
    }

    /// Check if a child field is complete (for Struct and VariantData).
    pub fn is_field_complete(&self, idx: usize) -> bool {
        match self {
            FrameKind::Struct(s) => s.fields.is_complete(idx),
            FrameKind::VariantData(v) => v.fields.is_complete(idx),
            _ => false,
        }
    }

    /// Get as mutable enum frame, if this is an enum.
    pub fn as_enum_mut(&mut self) -> Option<&mut EnumFrame> {
        match self {
            FrameKind::Enum(e) => Some(e),
            _ => None,
        }
    }
}

/// Pending key for map insertion.
/// When building a value for a map insert, this holds the key until End.
/// Uses TempAlloc which handles cleanup automatically.
pub type PendingKey = crate::temp_alloc::TempAlloc;

/// A frame tracking construction of a single value.
pub struct Frame {
    /// Pointer to the memory being written.
    pub data: PtrUninit,

    /// Shape (type metadata) of the value.
    pub shape: &'static Shape,

    /// What kind of value we're building.
    pub kind: FrameKind,

    /// State flags.
    pub flags: FrameFlags,

    /// Parent frame (if any) and our index within it.
    pub parent: Option<(Idx<Frame>, u32)>,

    /// Pending key for map insertion (only set when building a value for Insert).
    pub pending_key: Option<PendingKey>,
}

/// Build the absolute path from root to the given frame by walking up the parent chain.
pub fn absolute_path(arena: &Arena<Frame>, mut idx: Idx<Frame>) -> Path {
    let mut indices = Vec::new();
    while idx.is_valid() {
        let frame = arena.get(idx);
        if let Some((parent_idx, field_idx)) = frame.parent {
            indices.push(field_idx);
            idx = parent_idx;
        } else {
            break;
        }
    }
    indices.reverse();
    let mut path = Path::default();
    for i in indices {
        path.push(i);
    }
    path
}

impl Frame {
    pub fn new(data: PtrUninit, shape: &'static Shape) -> Self {
        Frame {
            data,
            shape,
            kind: FrameKind::Scalar,
            flags: FrameFlags::empty(),
            parent: None,
            pending_key: None,
        }
    }

    /// Create a frame for a struct with the given number of fields.
    pub fn new_struct(data: PtrUninit, shape: &'static Shape, field_count: usize) -> Self {
        Frame {
            data,
            shape,
            kind: FrameKind::Struct(StructFrame::new(field_count)),
            flags: FrameFlags::empty(),
            parent: None,
            pending_key: None,
        }
    }

    /// Create a frame for an enum (variant not yet selected).
    pub fn new_enum(data: PtrUninit, shape: &'static Shape) -> Self {
        Frame {
            data,
            shape,
            kind: FrameKind::Enum(EnumFrame::new()),
            flags: FrameFlags::empty(),
            parent: None,
            pending_key: None,
        }
    }

    /// Create a frame for an enum variant's fields.
    pub fn new_variant(data: PtrUninit, shape: &'static Shape, variant: &'static Variant) -> Self {
        Frame {
            data,
            shape,
            kind: FrameKind::VariantData(VariantFrame::new(variant)),
            flags: FrameFlags::empty(),
            parent: None,
            pending_key: None,
        }
    }

    /// Create a frame for a pointer's pointee (Box, Rc, Arc, etc.).
    /// `data` points to the allocated pointee memory, `shape` is the pointee's shape.
    pub fn new_pointer(data: PtrUninit, shape: &'static Shape) -> Self {
        Frame {
            data,
            shape,
            kind: FrameKind::Pointer(PointerFrame::new()),
            flags: FrameFlags::empty(),
            parent: None,
            pending_key: None,
        }
    }

    /// Create a frame for a list (Vec, etc.).
    /// `data` points to the list memory, `list_ptr` is the initialized list,
    /// `shape` is the list's shape.
    pub fn new_list(data: PtrUninit, shape: &'static Shape, list_ptr: PtrMut) -> Self {
        Frame {
            data,
            shape,
            kind: FrameKind::List(ListFrame::new(list_ptr)),
            flags: FrameFlags::empty(),
            parent: None,
            pending_key: None,
        }
    }

    /// Create a frame for a map (HashMap, BTreeMap, etc.).
    /// `data` points to the map memory, `map_ptr` is the initialized map,
    /// `shape` is the map's shape.
    pub fn new_map(data: PtrUninit, shape: &'static Shape, map_ptr: PtrMut) -> Self {
        Frame {
            data,
            shape,
            kind: FrameKind::Map(MapFrame::new(map_ptr)),
            flags: FrameFlags::empty(),
            parent: None,
            pending_key: None,
        }
    }

    /// Create a frame for a set (HashSet, BTreeSet, etc.).
    /// `data` points to the set memory, `set_ptr` is the initialized set,
    /// `shape` is the set's shape.
    pub fn new_set(data: PtrUninit, shape: &'static Shape, set_ptr: PtrMut) -> Self {
        Frame {
            data,
            shape,
            kind: FrameKind::Set(SetFrame::new(set_ptr)),
            flags: FrameFlags::empty(),
            parent: None,
            pending_key: None,
        }
    }

    /// Assert that the given shape matches this frame's shape.
    pub fn assert_shape(&self, actual: &'static Shape, path: &Path) -> Result<(), ReflectError> {
        if self.shape.is_shape(actual) {
            Ok(())
        } else {
            Err(ReflectError {
                location: ErrorLocation {
                    shape: self.shape,
                    path: path.clone(),
                },
                kind: ReflectErrorKind::ShapeMismatch {
                    expected: self.shape,
                    actual,
                },
            })
        }
    }

    /// Drop any initialized value, returning frame to uninitialized state.
    ///
    /// This is idempotent - calling on an uninitialized frame is a no-op.
    pub fn uninit(&mut self) {
        use crate::enum_helpers::drop_variant_fields;
        use facet_core::{Type, UserType};

        // Clean up pending key if present (TempAlloc handles drop + dealloc)
        let _ = self.pending_key.take();

        if self.flags.contains(FrameFlags::INIT) {
            // SAFETY: INIT flag means the value is fully initialized
            unsafe {
                self.shape.call_drop_in_place(self.data.assume_init());
            }
            self.flags.remove(FrameFlags::INIT);

            // Also clear enum selected state
            if let FrameKind::Enum(ref mut e) = self.kind {
                e.selected = None;
            }
        } else if let FrameKind::Struct(ref mut s) = self.kind {
            // Struct or array may have some fields/elements initialized - drop them individually
            if let Type::User(UserType::Struct(ref struct_type)) = self.shape.ty {
                for (idx, field) in struct_type.fields.iter().enumerate() {
                    if s.fields.is_complete(idx) {
                        // SAFETY: field is marked complete, so it's initialized
                        unsafe {
                            let field_ptr = self.data.assume_init().field(field.offset);
                            field.shape().call_drop_in_place(field_ptr);
                        }
                    }
                }
                // Reset all fields to NOT_STARTED
                s.fields = IndexedFields::new(struct_type.fields.len());
            } else if let Type::Sequence(SequenceType::Array(ref array_type)) = self.shape.ty {
                // Array elements - all have the same shape
                // Note: Layout::size() includes trailing padding, so it equals the stride
                // For ZSTs, size=0, so all elements have offset 0 (correct for ZSTs)
                let element_shape = array_type.t;
                // Arrays of unsized types (!Sized) can't exist in Rust, so unwrap_or is defensive
                let element_size = element_shape
                    .layout
                    .sized_layout()
                    .map(|l| l.size())
                    .unwrap_or(0);
                for idx in 0..array_type.n {
                    if s.fields.is_complete(idx) {
                        // SAFETY: element is marked complete, so it's initialized
                        unsafe {
                            let offset = idx * element_size;
                            let element_ptr = self.data.assume_init().field(offset);
                            element_shape.call_drop_in_place(element_ptr);
                        }
                    }
                }
                // Reset all elements to NOT_STARTED
                s.fields = IndexedFields::new(array_type.n);
            }
        } else if let FrameKind::Enum(ref mut e) = self.kind {
            // Enum variant may be complete even if INIT flag isn't set
            // (e.g., when variant was set via apply_enum_variant_set)
            if let Some((variant_idx, status)) = e.selected {
                if status.is_complete()
                    && let Type::User(UserType::Enum(ref enum_type)) = self.shape.ty
                {
                    let variant = &enum_type.variants[variant_idx as usize];
                    // SAFETY: the variant was marked complete, so its fields are initialized
                    unsafe {
                        drop_variant_fields(self.data.assume_init().as_const(), variant);
                    }
                }
                e.selected = None;
            }
        }
    }

    /// Prepare a struct field for overwriting by dropping any existing value.
    ///
    /// This handles two cases:
    /// 1. If the whole struct has INIT flag (set via Imm move): drop the field,
    ///    clear INIT, and mark all OTHER fields as complete.
    /// 2. If just this field was previously set individually: drop it and mark
    ///    as not started (so uninit() won't try to drop again on failure).
    ///
    /// NOTE: This may "leak" an arena slot if the field was an in-progress child frame.
    /// TODO: Add tests for frame arena leaking scenarios.
    pub fn prepare_field_for_overwrite(&mut self, field_idx: usize) {
        use facet_core::{Type, UserType};

        if self.flags.contains(FrameFlags::INIT) {
            // The whole struct was previously initialized via Imm.
            // We need to:
            // 1. Drop the old field value
            // 2. Clear INIT flag
            // 3. Mark all OTHER fields as complete (they're still valid)

            if let Type::User(UserType::Struct(ref struct_type)) = self.shape.ty {
                // Drop the old field value
                let field = &struct_type.fields[field_idx];
                // SAFETY: INIT means field is initialized
                unsafe {
                    let field_ptr = self.data.assume_init().field(field.offset);
                    field.shape().call_drop_in_place(field_ptr);
                }

                // Clear INIT and switch to field tracking
                self.flags.remove(FrameFlags::INIT);

                // Mark all OTHER fields as complete
                if let FrameKind::Struct(ref mut s) = self.kind {
                    for i in 0..struct_type.fields.len() {
                        if i != field_idx {
                            s.mark_field_complete(i);
                        }
                    }
                }
            }
        } else if self.kind.is_field_complete(field_idx) {
            // Field was previously set individually - drop the old value
            if let Type::User(UserType::Struct(ref struct_type)) = self.shape.ty {
                let field = &struct_type.fields[field_idx];
                // SAFETY: field is marked complete, so it's initialized
                unsafe {
                    let field_ptr = self.data.assume_init().field(field.offset);
                    field.shape().call_drop_in_place(field_ptr);
                }
                // Mark the field as not started - if we fail before completing,
                // uninit() shouldn't try to drop it again
                self.kind.mark_field_not_started(field_idx);
            }
        }
    }

    /// Copy a value into this frame, marking it as initialized.
    ///
    /// Returns an error if the frame is already initialized.
    /// Call [`uninit()`](Self::uninit) first to clear it.
    ///
    /// # Safety
    ///
    /// - `src` must point to a valid, initialized value matching `shape`
    /// - `shape` must match `self.shape`
    pub unsafe fn copy_from(
        &mut self,
        src: PtrConst,
        shape: &'static Shape,
    ) -> Result<(), ReflectErrorKind> {
        if self.flags.contains(FrameFlags::INIT) {
            return Err(ReflectErrorKind::AlreadyInitialized);
        }
        debug_assert!(self.shape.is_shape(shape), "shape mismatch");

        // SAFETY: caller guarantees src points to valid data matching shape,
        // and shape matches self.shape (debug_assert above)
        unsafe {
            self.data.copy_from(src, self.shape).unwrap();
        }
        self.flags |= FrameFlags::INIT;
        Ok(())
    }

    /// Deallocate the frame's memory if it owns the allocation.
    ///
    /// This should be called after the value has been moved out or dropped.
    pub fn dealloc_if_owned(self) {
        if self.flags.contains(FrameFlags::OWNS_ALLOC) {
            let layout = self.shape.layout.sized_layout().unwrap();
            if layout.size() > 0 {
                // SAFETY: we allocated this memory with this layout
                unsafe {
                    std::alloc::dealloc(self.data.as_mut_byte_ptr(), layout);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet_core::Facet;
    use std::ptr::NonNull;

    fn dummy_frame() -> Frame {
        Frame::new(
            PtrUninit::new(NonNull::<u8>::dangling().as_ptr()),
            u32::SHAPE,
        )
    }

    fn dummy_frame_with_parent(parent: Idx<Frame>, index: u32) -> Frame {
        let mut frame = dummy_frame();
        frame.parent = Some((parent, index));
        frame
    }

    #[test]
    fn absolute_path_root_frame() {
        let mut arena = Arena::new();
        let root = arena.alloc(dummy_frame());

        let path = absolute_path(&arena, root);
        assert!(path.is_empty());
    }

    #[test]
    fn absolute_path_one_level() {
        let mut arena = Arena::new();
        let root = arena.alloc(dummy_frame());
        let child = arena.alloc(dummy_frame_with_parent(root, 3));

        let path = absolute_path(&arena, child);
        assert_eq!(path.as_slice(), &[3]);
    }

    #[test]
    fn absolute_path_two_levels() {
        let mut arena = Arena::new();
        let root = arena.alloc(dummy_frame());
        let child = arena.alloc(dummy_frame_with_parent(root, 1));
        let grandchild = arena.alloc(dummy_frame_with_parent(child, 2));

        let path = absolute_path(&arena, grandchild);
        assert_eq!(path.as_slice(), &[1, 2]);
    }

    #[test]
    fn absolute_path_three_levels() {
        let mut arena = Arena::new();
        let root = arena.alloc(dummy_frame());
        let a = arena.alloc(dummy_frame_with_parent(root, 0));
        let b = arena.alloc(dummy_frame_with_parent(a, 5));
        let c = arena.alloc(dummy_frame_with_parent(b, 10));

        let path = absolute_path(&arena, c);
        assert_eq!(path.as_slice(), &[0, 5, 10]);
    }

    #[test]
    fn absolute_path_sibling_frames() {
        let mut arena = Arena::new();
        let root = arena.alloc(dummy_frame());
        let child0 = arena.alloc(dummy_frame_with_parent(root, 0));
        let child1 = arena.alloc(dummy_frame_with_parent(root, 1));
        let child2 = arena.alloc(dummy_frame_with_parent(root, 2));

        assert_eq!(absolute_path(&arena, child0).as_slice(), &[0]);
        assert_eq!(absolute_path(&arena, child1).as_slice(), &[1]);
        assert_eq!(absolute_path(&arena, child2).as_slice(), &[2]);
    }

    #[test]
    fn absolute_path_deep_nesting() {
        let mut arena = Arena::new();
        let mut current = arena.alloc(dummy_frame());

        for i in 0..10 {
            current = arena.alloc(dummy_frame_with_parent(current, i));
        }

        let path = absolute_path(&arena, current);
        assert_eq!(path.as_slice(), &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
    }
}
