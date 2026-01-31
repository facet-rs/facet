//! Frame for tracking partial value construction.

use crate::arena::{Arena, Idx};
use crate::errors::{ErrorLocation, ReflectError, ReflectErrorKind};
use crate::ops::Path;
use facet_core::{ListDef, MapDef, PtrConst, PtrUninit, SequenceType, SetDef, Shape, Variant};

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
/// Tracks whether the list is initialized and count of pushed elements.
pub struct ListFrame {
    /// The list definition (element type, vtable).
    pub def: &'static ListDef,
    /// Whether the list has been initialized (via init_in_place_with_capacity).
    /// The actual pointer is always frame.data.
    pub initialized: bool,
    /// Number of elements that have been pushed.
    pub len: usize,
}

impl ListFrame {
    /// Create an uninitialized list frame.
    pub fn new(def: &'static ListDef) -> Self {
        Self {
            def,
            initialized: false,
            len: 0,
        }
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
/// Tracks whether the map is initialized and count of inserted entries.
pub struct MapFrame {
    /// The map definition (key/value types, vtable).
    pub def: &'static MapDef,
    /// Whether the map has been initialized (via init_in_place_with_capacity).
    /// The actual pointer is always frame.data.
    pub initialized: bool,
    /// Number of entries that have been inserted.
    pub len: usize,
}

impl MapFrame {
    /// Create an uninitialized map frame.
    pub fn new(def: &'static MapDef) -> Self {
        Self {
            def,
            initialized: false,
            len: 0,
        }
    }

    /// Maps are always "complete" since they have variable size.
    /// Completion just means End was called.
    pub fn is_complete(&self) -> bool {
        true
    }
}

/// Set frame data (building a HashSet, BTreeSet, etc.).
/// Tracks whether the set is initialized and count of inserted elements.
pub struct SetFrame {
    /// The set definition (element type, vtable).
    pub def: &'static SetDef,
    /// Whether the set has been initialized (via init_in_place_with_capacity).
    /// The actual pointer is always frame.data.
    pub initialized: bool,
    /// Number of elements that have been inserted.
    pub len: usize,
}

impl SetFrame {
    /// Create an uninitialized set frame.
    pub fn new(def: &'static SetDef) -> Self {
        Self {
            def,
            initialized: false,
            len: 0,
        }
    }

    /// Sets are always "complete" since they have variable size.
    /// Completion just means End was called.
    pub fn is_complete(&self) -> bool {
        true
    }
}

/// Option frame data (building Option<T>).
/// Tracks whether None or Some is selected and if we're building the inner value.
pub struct OptionFrame {
    /// The selected variant: None = 0, Some = 1 (or None if not yet selected)
    pub selected: Option<u32>,
    /// Frame index for the inner value being built (for Some)
    pub inner: Idx<Frame>,
}

impl OptionFrame {
    pub fn new() -> Self {
        Self {
            selected: None,
            inner: Idx::NOT_STARTED,
        }
    }

    pub fn is_complete(&self) -> bool {
        match self.selected {
            None => false,
            Some(0) => true,                     // None variant is always complete
            Some(1) => self.inner.is_complete(), // Some needs inner to be complete
            Some(_) => false,                    // Invalid
        }
    }
}

impl Default for OptionFrame {
    fn default() -> Self {
        Self::new()
    }
}

/// Result frame data (building Result<T, E>).
/// Tracks whether Ok or Err is selected and if we're building the inner value.
pub struct ResultFrame {
    /// The selected variant: Ok = 0, Err = 1 (or None if not yet selected)
    pub selected: Option<u32>,
    /// Frame index for the inner value being built
    pub inner: Idx<Frame>,
}

impl ResultFrame {
    pub fn new() -> Self {
        Self {
            selected: None,
            inner: Idx::NOT_STARTED,
        }
    }

    pub fn is_complete(&self) -> bool {
        match self.selected {
            None => false,
            Some(_) => self.inner.is_complete(), // Both Ok and Err need inner to be complete
        }
    }
}

impl Default for ResultFrame {
    fn default() -> Self {
        Self::new()
    }
}

/// Map entry frame data (building a (Key, Value) tuple for a map entry).
/// This is created when Append is called on a map frame.
/// Field 0 = key, Field 1 = value.
pub struct MapEntryFrame {
    /// The map definition (for key/value shapes).
    pub map_def: &'static MapDef,
    /// Key status: NOT_STARTED, COMPLETE, or valid frame index.
    pub key: Idx<Frame>,
    /// Value status: NOT_STARTED, COMPLETE, or valid frame index.
    pub value: Idx<Frame>,
}

impl MapEntryFrame {
    pub fn new(map_def: &'static MapDef) -> Self {
        Self {
            map_def,
            key: Idx::NOT_STARTED,
            value: Idx::NOT_STARTED,
        }
    }

    pub fn is_complete(&self) -> bool {
        self.key.is_complete() && self.value.is_complete()
    }

    /// Mark a field (0=key, 1=value) as complete.
    pub fn mark_field_complete(&mut self, idx: usize) {
        match idx {
            0 => self.key = Idx::COMPLETE,
            1 => self.value = Idx::COMPLETE,
            _ => {}
        }
    }

    /// Mark a field (0=key, 1=value) as not started.
    pub fn mark_field_not_started(&mut self, idx: usize) {
        match idx {
            0 => self.key = Idx::NOT_STARTED,
            1 => self.value = Idx::NOT_STARTED,
            _ => {}
        }
    }

    /// Check if a field (0=key, 1=value) is complete.
    pub fn is_field_complete(&self, idx: usize) -> bool {
        match idx {
            0 => self.key.is_complete(),
            1 => self.value.is_complete(),
            _ => false,
        }
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

    /// Building an Option<T>.
    Option(OptionFrame),

    /// Building a Result<T, E>.
    Result(ResultFrame),

    /// Building a map entry (key, value) tuple.
    /// Created by Append on a map frame.
    MapEntry(MapEntryFrame),
}

/// Describes the relationship between a frame and its parent.
///
/// This captures not just "who is my parent" but also "what to do when this frame completes".
/// Each variant knows exactly what completion action is needed, eliminating the need for
/// runtime checks like `has_pending_key && is_map_parent`.
pub enum ParentLink {
    /// Root frame - no parent.
    Root,

    /// Field of a struct, tuple, or array.
    /// On End: mark field complete in parent.
    StructField { parent: Idx<Frame>, field_idx: u32 },

    /// Element being pushed to a list.
    /// On End: call push_fn to add element to parent list.
    ListElement { parent: Idx<Frame> },

    /// Element being inserted into a set.
    /// On End: call set's insert_fn to add element.
    SetElement { parent: Idx<Frame> },

    /// Pointee of a smart pointer (Box/Rc/Arc).
    /// On End: call new_into_fn to create the pointer.
    PointerInner { parent: Idx<Frame> },

    /// Inner value of Option (always the Some case).
    /// On End: call init_some to wrap the value.
    OptionInner { parent: Idx<Frame> },

    /// Inner value of Result (Ok or Err).
    /// On End: call init_ok or init_err based on is_ok.
    ResultInner { parent: Idx<Frame>, is_ok: bool },

    /// Variant data frame for an enum.
    /// On End: mark variant complete in parent enum frame.
    EnumVariant {
        parent: Idx<Frame>,
        variant_idx: u32,
    },

    /// Map entry frame (new API using Append).
    /// On End: insert the (key, value) pair into parent map.
    MapEntry { parent: Idx<Frame> },

    /// Field of a map entry (0=key, 1=value).
    /// On End: mark field complete in parent MapEntryFrame.
    MapEntryField { parent: Idx<Frame>, field_idx: u32 },
}

impl ParentLink {
    /// Get the parent frame index, if any.
    pub fn parent_idx(&self) -> Option<Idx<Frame>> {
        match self {
            ParentLink::Root => None,
            ParentLink::StructField { parent, .. } => Some(*parent),
            ParentLink::ListElement { parent } => Some(*parent),
            ParentLink::SetElement { parent } => Some(*parent),
            ParentLink::PointerInner { parent } => Some(*parent),
            ParentLink::OptionInner { parent } => Some(*parent),
            ParentLink::ResultInner { parent, .. } => Some(*parent),
            ParentLink::EnumVariant { parent, .. } => Some(*parent),
            ParentLink::MapEntry { parent } => Some(*parent),
            ParentLink::MapEntryField { parent, .. } => Some(*parent),
        }
    }

    /// Get the field index for path building (only meaningful for some variants).
    /// Returns the index used in error paths.
    pub fn field_idx_for_path(&self) -> Option<u32> {
        match self {
            ParentLink::Root => None,
            ParentLink::StructField { field_idx, .. } => Some(*field_idx),
            ParentLink::ListElement { .. } => None, // Lists don't use indexed paths
            ParentLink::SetElement { .. } => None,  // Sets don't use indexed paths
            ParentLink::PointerInner { .. } => Some(0), // Pointer inner is "field 0"
            ParentLink::OptionInner { .. } => Some(1), // Some is variant 1
            ParentLink::ResultInner { is_ok, .. } => Some(if *is_ok { 0 } else { 1 }),
            ParentLink::EnumVariant { variant_idx, .. } => Some(*variant_idx),
            ParentLink::MapEntry { .. } => None, // Map entries don't use indexed paths
            ParentLink::MapEntryField { field_idx, .. } => Some(*field_idx),
        }
    }
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
            FrameKind::Option(o) => o.is_complete(),
            FrameKind::Result(r) => r.is_complete(),
            FrameKind::MapEntry(e) => e.is_complete(),
        }
    }

    /// Mark a child field as complete (for Struct, VariantData, and MapEntry).
    pub fn mark_field_complete(&mut self, idx: usize) {
        match self {
            FrameKind::Struct(s) => s.mark_field_complete(idx),
            FrameKind::VariantData(v) => v.mark_field_complete(idx),
            FrameKind::MapEntry(e) => e.mark_field_complete(idx),
            _ => {}
        }
    }

    /// Mark a child field as not started (after dropping it before overwriting).
    pub fn mark_field_not_started(&mut self, idx: usize) {
        match self {
            FrameKind::Struct(s) => s.mark_field_not_started(idx),
            FrameKind::VariantData(v) => v.mark_field_not_started(idx),
            FrameKind::MapEntry(e) => e.mark_field_not_started(idx),
            _ => {}
        }
    }

    /// Check if a child field is complete (for Struct, VariantData, and MapEntry).
    pub fn is_field_complete(&self, idx: usize) -> bool {
        match self {
            FrameKind::Struct(s) => s.fields.is_complete(idx),
            FrameKind::VariantData(v) => v.fields.is_complete(idx),
            FrameKind::MapEntry(e) => e.is_field_complete(idx),
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

    /// Relationship to parent frame (or Root if this is the root).
    pub parent_link: ParentLink,
}

/// Build the absolute path from root to the given frame by walking up the parent chain.
pub fn absolute_path(arena: &Arena<Frame>, mut idx: Idx<Frame>) -> Path {
    let mut indices = Vec::new();
    while idx.is_valid() {
        let frame = arena.get(idx);
        if let Some(field_idx) = frame.parent_link.field_idx_for_path() {
            indices.push(field_idx);
        }
        match frame.parent_link.parent_idx() {
            Some(parent_idx) => idx = parent_idx,
            None => break,
        }
    }
    indices.reverse();
    let mut path = Path::empty();
    for i in indices {
        path = path.then_field(i);
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
            parent_link: ParentLink::Root,
        }
    }

    /// Create a frame for a struct with the given number of fields.
    pub fn new_struct(data: PtrUninit, shape: &'static Shape, field_count: usize) -> Self {
        Frame {
            data,
            shape,
            kind: FrameKind::Struct(StructFrame::new(field_count)),
            flags: FrameFlags::empty(),
            parent_link: ParentLink::Root,
        }
    }

    /// Create a frame for an enum (variant not yet selected).
    pub fn new_enum(data: PtrUninit, shape: &'static Shape) -> Self {
        Frame {
            data,
            shape,
            kind: FrameKind::Enum(EnumFrame::new()),
            flags: FrameFlags::empty(),
            parent_link: ParentLink::Root,
        }
    }

    /// Create a frame for an enum variant's fields.
    pub fn new_variant(data: PtrUninit, shape: &'static Shape, variant: &'static Variant) -> Self {
        Frame {
            data,
            shape,
            kind: FrameKind::VariantData(VariantFrame::new(variant)),
            flags: FrameFlags::empty(),
            parent_link: ParentLink::Root,
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
            parent_link: ParentLink::Root,
        }
    }

    /// Create a frame for a list (Vec, etc.).
    /// `data` points to the list memory (uninitialized).
    /// The list will be lazily initialized on first Push.
    pub fn new_list(data: PtrUninit, shape: &'static Shape, def: &'static ListDef) -> Self {
        Frame {
            data,
            shape,
            kind: FrameKind::List(ListFrame::new(def)),
            flags: FrameFlags::empty(),
            parent_link: ParentLink::Root,
        }
    }

    /// Create a frame for a map (HashMap, BTreeMap, etc.).
    /// `data` points to the map memory (uninitialized).
    /// The map will be lazily initialized on first Insert.
    pub fn new_map(data: PtrUninit, shape: &'static Shape, def: &'static MapDef) -> Self {
        Frame {
            data,
            shape,
            kind: FrameKind::Map(MapFrame::new(def)),
            flags: FrameFlags::empty(),
            parent_link: ParentLink::Root,
        }
    }

    /// Create a frame for a set (HashSet, BTreeSet, etc.).
    /// `data` points to the set memory (uninitialized).
    /// The set will be lazily initialized on first Push.
    pub fn new_set(data: PtrUninit, shape: &'static Shape, def: &'static SetDef) -> Self {
        Frame {
            data,
            shape,
            kind: FrameKind::Set(SetFrame::new(def)),
            flags: FrameFlags::empty(),
            parent_link: ParentLink::Root,
        }
    }

    /// Create a frame for an Option<T>.
    pub fn new_option(data: PtrUninit, shape: &'static Shape) -> Self {
        Frame {
            data,
            shape,
            kind: FrameKind::Option(OptionFrame::new()),
            flags: FrameFlags::empty(),
            parent_link: ParentLink::Root,
        }
    }

    /// Create a frame for a Result<T, E>.
    pub fn new_result(data: PtrUninit, shape: &'static Shape) -> Self {
        Frame {
            data,
            shape,
            kind: FrameKind::Result(ResultFrame::new()),
            flags: FrameFlags::empty(),
            parent_link: ParentLink::Root,
        }
    }

    /// Create a frame for a map entry (key, value) tuple.
    /// `data` points to temporary memory for key and value staging.
    /// `shape` is the parent map's shape (for error reporting).
    pub fn new_map_entry(data: PtrUninit, shape: &'static Shape, map_def: &'static MapDef) -> Self {
        Frame {
            data,
            shape,
            kind: FrameKind::MapEntry(MapEntryFrame::new(map_def)),
            flags: FrameFlags::empty(),
            parent_link: ParentLink::Root,
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
    ///
    /// Note: This does NOT clean up the ParentLink. If this frame is a MapValue child,
    /// the key in ParentLink::MapValue will be cleaned up when the frame is freed.
    pub fn uninit(&mut self) {
        use crate::enum_helpers::drop_variant_fields;
        use facet_core::{Type, UserType};

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
        frame.parent_link = ParentLink::StructField {
            parent,
            field_idx: index,
        };
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
        use crate::ops::PathSegment;
        let mut arena = Arena::new();
        let root = arena.alloc(dummy_frame());
        let child = arena.alloc(dummy_frame_with_parent(root, 3));

        let path = absolute_path(&arena, child);
        assert_eq!(path.segments(), &[PathSegment::Field(3)]);
    }

    #[test]
    fn absolute_path_two_levels() {
        use crate::ops::PathSegment;
        let mut arena = Arena::new();
        let root = arena.alloc(dummy_frame());
        let child = arena.alloc(dummy_frame_with_parent(root, 1));
        let grandchild = arena.alloc(dummy_frame_with_parent(child, 2));

        let path = absolute_path(&arena, grandchild);
        assert_eq!(
            path.segments(),
            &[PathSegment::Field(1), PathSegment::Field(2)]
        );
    }

    #[test]
    fn absolute_path_three_levels() {
        use crate::ops::PathSegment;
        let mut arena = Arena::new();
        let root = arena.alloc(dummy_frame());
        let a = arena.alloc(dummy_frame_with_parent(root, 0));
        let b = arena.alloc(dummy_frame_with_parent(a, 5));
        let c = arena.alloc(dummy_frame_with_parent(b, 10));

        let path = absolute_path(&arena, c);
        assert_eq!(
            path.segments(),
            &[
                PathSegment::Field(0),
                PathSegment::Field(5),
                PathSegment::Field(10)
            ]
        );
    }

    #[test]
    fn absolute_path_sibling_frames() {
        use crate::ops::PathSegment;
        let mut arena = Arena::new();
        let root = arena.alloc(dummy_frame());
        let child0 = arena.alloc(dummy_frame_with_parent(root, 0));
        let child1 = arena.alloc(dummy_frame_with_parent(root, 1));
        let child2 = arena.alloc(dummy_frame_with_parent(root, 2));

        assert_eq!(
            absolute_path(&arena, child0).segments(),
            &[PathSegment::Field(0)]
        );
        assert_eq!(
            absolute_path(&arena, child1).segments(),
            &[PathSegment::Field(1)]
        );
        assert_eq!(
            absolute_path(&arena, child2).segments(),
            &[PathSegment::Field(2)]
        );
    }

    #[test]
    fn absolute_path_deep_nesting() {
        use crate::ops::PathSegment;
        let mut arena = Arena::new();
        let mut current = arena.alloc(dummy_frame());

        for i in 0..10 {
            current = arena.alloc(dummy_frame_with_parent(current, i));
        }

        let path = absolute_path(&arena, current);
        let expected: Vec<PathSegment> = (0..10).map(PathSegment::Field).collect();
        assert_eq!(path.segments(), expected.as_slice());
    }
}
