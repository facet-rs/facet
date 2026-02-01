//! Partial value construction.

// Ops appliers
mod end;
mod enum_variant;
mod list;
mod map;
mod option;
mod push;
mod result;
mod set;
mod set_collection;

use std::alloc::alloc;
use std::marker::PhantomData;
use std::ptr::NonNull;

use crate::arena::{Arena, Idx};
use crate::errors::{ReflectError, ReflectErrorKind};
use crate::frame::{Frame, FrameFlags, FrameKind, absolute_path};
use crate::ops::{Op, OpBatch, Path, PathSegment};
use crate::shape_desc::ShapeDesc;
use facet_core::{
    Def, EnumType, Facet, Field, PtrUninit, SequenceType, Shape, Type, UserType, Variant,
};

/// Manages incremental construction of a value.
pub struct Partial<'facet> {
    arena: Arena<Frame>,
    root: Idx<Frame>,
    current: Idx<Frame>,
    root_shape: &'static Shape,
    poisoned: bool,
    _marker: PhantomData<&'facet ()>,
}

impl<'facet> Partial<'facet> {
    /// Create an error at the current frame location.
    fn error(&self, kind: ReflectErrorKind) -> ReflectError {
        let frame = self.arena.get(self.current);
        ReflectError::new(frame.shape, absolute_path(&self.arena, self.current), kind)
    }

    /// Create an error at a specific frame location.
    fn error_at(&self, idx: Idx<Frame>, kind: ReflectErrorKind) -> ReflectError {
        let frame = self.arena.get(idx);
        ReflectError::new(frame.shape, absolute_path(&self.arena, idx), kind)
    }

    /// Allocate for a known type.
    pub fn alloc<T: Facet<'facet>>() -> Result<Self, ReflectError> {
        Self::alloc_shape(T::SHAPE)
    }

    /// Allocate for a dynamic shape.
    pub fn alloc_shape(shape: &'static Shape) -> Result<Self, ReflectError> {
        let shape_desc = ShapeDesc::Static(shape);
        let layout = shape.layout.sized_layout().map_err(|_| {
            ReflectError::at_root(shape_desc, ReflectErrorKind::Unsized { shape: shape_desc })
        })?;

        // Allocate memory (handle ZST case)
        let data = if layout.size() == 0 {
            PtrUninit::new(NonNull::<u8>::dangling().as_ptr())
        } else {
            // SAFETY: layout has non-zero size (checked above) and is valid from Shape
            let ptr = unsafe { alloc(layout) };
            if ptr.is_null() {
                return Err(ReflectError::at_root(
                    shape_desc,
                    ReflectErrorKind::AllocFailed { layout },
                ));
            }
            PtrUninit::new(ptr)
        };

        // Create frame with OWNS_ALLOC flag
        // Check Def first because Option/Result have Def::Option/Result
        // but are also UserType::Enum at the ty level
        let mut frame = match &shape.def {
            Def::Option(_) => Frame::new_option(data, shape),
            Def::Result(_) => Frame::new_result(data, shape),
            Def::List(list_def) => Frame::new_list(data, shape, *list_def),
            Def::Map(map_def) => Frame::new_map(data, shape, *map_def),
            Def::Set(set_def) => Frame::new_set(data, shape, *set_def),
            _ => match shape.ty {
                Type::User(UserType::Struct(ref s)) => {
                    Frame::new_struct(data, shape, s.fields.len())
                }
                Type::User(UserType::Enum(_)) => Frame::new_enum(data, shape),
                Type::Sequence(SequenceType::Array(ref a)) => {
                    // Arrays are like structs with N indexed elements
                    Frame::new_struct(data, shape, a.n)
                }
                _ => Frame::new(data, shape),
            },
        };
        frame.flags |= FrameFlags::OWNS_ALLOC;

        // Store in arena
        let mut arena = Arena::new();
        let root = arena.alloc(frame);

        Ok(Self {
            arena,
            root,
            current: root,
            root_shape: shape,
            poisoned: false,
            _marker: PhantomData,
        })
    }

    /// Poison the partial and clean up all resources.
    /// After this, any operation will return `Poisoned` error.
    fn poison(&mut self) {
        if self.poisoned {
            return;
        }
        self.poisoned = true;

        // Walk from current frame up to root, cleaning up each frame.
        // This handles in-progress child frames (e.g., list elements being built).
        let mut idx = self.current;
        while idx.is_valid() {
            let frame = self.arena.get_mut(idx);
            let parent = frame.parent_link.parent_idx();

            // Drop any initialized data in this frame
            frame.uninit();

            // Free the frame and deallocate if it owns its allocation
            // Free the frame and deallocate if it owns its allocation
            let frame = self.arena.free(idx);
            frame.dealloc_if_owned();

            // Move to parent
            idx = parent.unwrap_or(Idx::COMPLETE);
        }

        // Mark as cleaned up
        self.current = Idx::COMPLETE;
        self.root = Idx::COMPLETE;
    }

    /// Apply a sequence of operations.
    pub fn apply(&mut self, ops: &[Op<'_>]) -> Result<(), ReflectError> {
        if self.poisoned {
            return Err(ReflectError::at_root(
                ShapeDesc::Static(self.root_shape),
                ReflectErrorKind::Poisoned,
            ));
        }

        let result = self.apply_inner(ops);
        if result.is_err() {
            self.poison();
        }
        result
    }

    /// Apply a batch of operations.
    ///
    /// Consumes operations from the batch (popping from front) as they are processed.
    /// After this returns:
    /// - Ops that were consumed have been removed from the batch
    /// - Remaining ops in the batch were NOT consumed (on error or if batch wasn't empty)
    ///
    /// The caller is responsible for cleanup:
    /// - Consumed ops: their `Imm` data was moved, caller must forget source values
    /// - Remaining ops: their `Imm` data was NOT moved, caller should drop normally
    pub fn apply_batch(&mut self, batch: &mut OpBatch<'_>) -> Result<(), ReflectError> {
        if self.poisoned {
            return Err(ReflectError::at_root(
                ShapeDesc::Static(self.root_shape),
                ReflectErrorKind::Poisoned,
            ));
        }

        while let Some(op) = batch.pop() {
            let result = match &op {
                Op::Set {
                    dst: path,
                    src: source,
                } => self.apply_set(path, source),
                Op::End => self.apply_end(end::EndInitiator::Op),
            };

            if let Err(e) = result {
                // Push the failed op back so caller knows it wasn't consumed
                batch.push_front(op);
                self.poison();
                return Err(e);
            }
            // Op was consumed successfully, it's been popped and won't be in batch
        }

        Ok(())
    }

    fn apply_inner(&mut self, ops: &[Op<'_>]) -> Result<(), ReflectError> {
        for op in ops {
            match op {
                Op::Set {
                    dst: path,
                    src: source,
                } => {
                    self.apply_set(path, source)?;
                }
                Op::End => {
                    self.apply_end(end::EndInitiator::Op)?;
                }
            }
        }
        Ok(())
    }

    /// Navigate to the root frame by ending all intermediate frames.
    fn navigate_to_root(&mut self) -> Result<(), ReflectError> {
        while self.current != self.root {
            self.apply_end(end::EndInitiator::Op)?;
        }
        Ok(())
    }

    /// Apply a Set operation, dispatching based on path and current frame type.
    fn apply_set(
        &mut self,
        path: &Path,
        source: &crate::ops::Source<'_>,
    ) -> Result<(), ReflectError> {
        let segments = path.segments();

        // Handle Root segment - must be first if present
        if let Some(PathSegment::Root) = segments.first() {
            // Navigate to root first
            self.navigate_to_root()?;

            // Continue with remaining path (without Root segment)
            let remaining = Path::from_segments(&segments[1..]);
            return self.apply_set(&remaining, source);
        }

        // Check for Append segment - dispatch to collection handling
        if let Some(PathSegment::Append) = segments.first() {
            return self.apply_append_set(path, source);
        }

        // Multi-level path handling: if path has more than one segment,
        // create intermediate frames for all but the last segment
        if segments.len() > 1 {
            return self.apply_multi_level_set(path, source);
        }

        // Check if current frame is an enum frame (not inside a variant's fields)
        // and path starts with a Field - that means we're selecting a variant
        let frame = self.arena.get(self.current);

        // Get the first field index if path starts with Field
        let first_field = match path.segments().first() {
            Some(PathSegment::Field(n)) => Some(*n),
            _ => None,
        };

        let is_enum_variant_selection = first_field.is_some()
            && matches!(frame.kind, FrameKind::Enum(_))
            && matches!(frame.shape.ty(), Type::User(UserType::Enum(_)));

        // Check if current frame is an Option/Result frame with variant selection
        let is_option_variant_selection = first_field.is_some()
            && matches!(frame.kind, FrameKind::Option(_))
            && matches!(frame.shape.def(), Def::Option(_));

        let is_result_variant_selection = first_field.is_some()
            && matches!(frame.kind, FrameKind::Result(_))
            && matches!(frame.shape.def(), Def::Result(_));

        if is_enum_variant_selection {
            self.apply_enum_variant_set(path, source)
        } else if is_option_variant_selection {
            self.apply_option_variant_set(path, source)
        } else if is_result_variant_selection {
            self.apply_result_variant_set(path, source)
        } else {
            self.apply_regular_set(path, source)
        }
    }

    /// Handle multi-level paths by creating intermediate frames.
    ///
    /// For a path like `at(0).at(1).at(2)`, this:
    /// 1. Applies `at(0)` with `Stage` to create an intermediate frame
    /// 2. Recursively applies `at(1).at(2)` with the original source
    fn apply_multi_level_set(
        &mut self,
        path: &Path,
        source: &crate::ops::Source<'_>,
    ) -> Result<(), ReflectError> {
        let segments = path.segments();
        debug_assert!(segments.len() > 1, "multi-level requires > 1 segment");

        // Process all segments except the last as Stage operations
        for segment in &segments[..segments.len() - 1] {
            let intermediate_path = match segment {
                PathSegment::Field(n) => Path::field(*n),
                PathSegment::Append => Path::append(),
                PathSegment::Root => {
                    // Root in the middle of a path is invalid
                    return Err(self.error(ReflectErrorKind::RootNotAtStart));
                }
            };

            // Apply Stage to create intermediate frame
            self.apply_set(&intermediate_path, &crate::ops::Source::Stage(None))?;
        }

        // Apply the final segment with the original source
        let last_segment = &segments[segments.len() - 1];
        let final_path = match last_segment {
            PathSegment::Field(n) => Path::field(*n),
            PathSegment::Append => Path::append(),
            PathSegment::Root => {
                return Err(self.error(ReflectErrorKind::RootNotAtStart));
            }
        };

        self.apply_set(&final_path, source)
    }

    /// Apply a Set operation with Append path segment (for lists, sets, maps).
    fn apply_append_set(
        &mut self,
        path: &Path,
        source: &crate::ops::Source<'_>,
    ) -> Result<(), ReflectError> {
        // For now, we only support single-segment Append paths
        // Multi-level Append paths will be implemented in Phase 3
        let segments = path.segments();
        if segments.len() != 1 {
            return Err(self.error(ReflectErrorKind::MultiLevelPathNotSupported {
                depth: segments.len(),
            }));
        }

        // Check if we're appending to a map
        let frame = self.arena.get(self.current);
        if matches!(frame.kind, FrameKind::Map(_)) {
            self.map_append(source)
        } else {
            // Delegate to the existing push logic for lists/sets
            self.apply_push(source)
        }
    }

    /// Ensure the current collection (list, map, or set) is initialized/staged.
    ///
    /// For lists and sets: calls init_in_place_with_capacity immediately.
    /// For maps: creates a Slab for collecting entries (map stays uninitialized until End).
    fn ensure_collection_initialized(&mut self) -> Result<(), ReflectError> {
        self.ensure_collection_initialized_with_capacity(0)
    }

    /// Ensure the current collection is initialized/staged with a capacity hint.
    fn ensure_collection_initialized_with_capacity(
        &mut self,
        capacity: usize,
    ) -> Result<(), ReflectError> {
        let frame = self.arena.get(self.current);

        // Check if collection needs initialization.
        // For lists: check initialized flag.
        // For maps/sets: check slab AND frame INIT flag (collection may have been set via Imm/Default).
        let already_init = frame.flags.contains(FrameFlags::INIT);
        let needs_init = match &frame.kind {
            FrameKind::List(l) => !l.initialized,
            FrameKind::Map(m) => !m.is_staged() && !already_init,
            FrameKind::Set(s) => !s.is_staged() && !already_init,
            _ => return Ok(()), // Not a collection, nothing to do
        };

        if !needs_init {
            return Ok(());
        }

        // Initialize based on frame kind (which has the def)
        let frame = self.arena.get(self.current);
        match &frame.kind {
            FrameKind::List(list_frame) => {
                let def = list_frame.def;
                let init_fn = def.init_in_place_with_capacity().ok_or_else(|| {
                    self.error(ReflectErrorKind::ListDoesNotSupportOp { shape: frame.shape })
                })?;
                // SAFETY: frame.data points to uninitialized list memory
                let list_ptr = unsafe { init_fn(frame.data, capacity) };

                // Get actual capacity (Vec may allocate more than requested)
                let cached_capacity = if let Some(capacity_fn) = def.capacity() {
                    // SAFETY: list_ptr points to initialized list
                    unsafe { capacity_fn(list_ptr.as_const()) }
                } else {
                    // No capacity function - assume we got what we asked for
                    capacity
                };

                // Mark list as initialized and cache capacity
                let frame = self.arena.get_mut(self.current);
                if let FrameKind::List(l) = &mut frame.kind {
                    l.initialized = true;
                    l.cached_capacity = cached_capacity;
                }
                frame.flags |= FrameFlags::INIT;
            }
            FrameKind::Map(map_frame) => {
                // For maps, create a Slab to collect (K, V) tuples.
                // The map itself stays uninitialized until End.
                let entry_shape = crate::tuple2(&map_frame.def);
                let slab = crate::slab::Slab::new(
                    ShapeDesc::Tuple2(entry_shape),
                    if capacity > 0 { Some(capacity) } else { None },
                );

                let frame = self.arena.get_mut(self.current);
                if let FrameKind::Map(m) = &mut frame.kind {
                    m.slab = Some(slab);
                }
                // Note: Do NOT set INIT flag - map memory is still uninitialized
            }
            FrameKind::Set(set_frame) => {
                // For sets, create a Slab to collect elements.
                // The set itself stays uninitialized until End.
                let element_shape = set_frame.def.t;
                let slab = crate::slab::Slab::new(
                    ShapeDesc::Static(element_shape),
                    if capacity > 0 { Some(capacity) } else { None },
                );

                let frame = self.arena.get_mut(self.current);
                if let FrameKind::Set(s) = &mut frame.kind {
                    s.slab = Some(slab);
                }
                // Note: Do NOT set INIT flag - set memory is still uninitialized
            }
            _ => unreachable!(),
        }

        Ok(())
    }

    /// Resolve a path to a temporary frame for the target location.
    ///
    /// For an empty path, returns a frame pointing to the current frame's data.
    /// For a non-empty path, returns a frame pointing to the field's memory.
    fn resolve_path(&self, frame: &Frame, path: &Path) -> Result<Frame, ReflectError> {
        if path.is_empty() {
            // MapEntry frames have no single shape - must use [0] for key, [1] for value
            if matches!(frame.kind, FrameKind::MapEntry(_)) {
                return Err(self.error(ReflectErrorKind::CannotSetEntireMapEntry));
            }
            return Ok(Frame::new(frame.data, frame.shape));
        }

        let segments = path.segments();

        // For now, only support single-level Field paths
        if segments.len() != 1 {
            return Err(self.error(ReflectErrorKind::MultiLevelPathNotSupported {
                depth: segments.len(),
            }));
        }

        // Extract field index from the first segment
        let index = match &segments[0] {
            PathSegment::Field(n) => *n,
            PathSegment::Append => {
                return Err(self.error(ReflectErrorKind::AppendInResolvePath));
            }
            PathSegment::Root => {
                return Err(self.error(ReflectErrorKind::RootNotAtStart));
            }
        };

        // Check if we're inside a variant - use variant's fields for resolution
        if let FrameKind::VariantData(v) = &frame.kind {
            let field = self.get_struct_field(v.variant.data.fields, index)?;
            let field_ptr =
                unsafe { PtrUninit::new(frame.data.as_mut_byte_ptr().add(field.offset)) };
            return Ok(Frame::new(field_ptr, field.shape()));
        }

        // Check if we're inside a MapEntry - use key/value shapes
        if let FrameKind::MapEntry(ref entry) = frame.kind {
            let (shape, offset) = match index {
                0 => {
                    // Key at offset 0
                    (entry.map_def.k, 0)
                }
                1 => {
                    // Value at vtable-defined offset (matches repr(Rust) tuple layout)
                    let value_offset = entry.map_def.vtable.value_offset_in_pair;
                    (entry.map_def.v, value_offset)
                }
                _ => {
                    return Err(self.error(ReflectErrorKind::FieldIndexOutOfBounds {
                        index,
                        field_count: 2, // MapEntry has 2 fields: key and value
                    }));
                }
            };
            let field_ptr = unsafe { PtrUninit::new(frame.data.as_mut_byte_ptr().add(offset)) };
            return Ok(Frame::new(field_ptr, shape));
        }

        match *frame.shape.ty() {
            Type::User(UserType::Struct(ref s)) => {
                let field = self.get_struct_field(s.fields, index)?;
                let field_ptr =
                    unsafe { PtrUninit::new(frame.data.as_mut_byte_ptr().add(field.offset)) };
                Ok(Frame::new(field_ptr, field.shape()))
            }
            Type::User(UserType::Enum(ref e)) => {
                // Validate the variant index
                let _variant = self.get_enum_variant(e, index)?;
                // For enums, we return the shape of the whole enum (not the variant)
                // The variant's fields will be accessed in a nested frame after Build
                Ok(Frame::new(frame.data, frame.shape))
            }
            Type::Sequence(SequenceType::Array(ref a)) => {
                // Validate array index
                if index as usize >= a.n {
                    return Err(self.error(ReflectErrorKind::ArrayIndexOutOfBounds {
                        index,
                        array_len: a.n,
                    }));
                }
                // Calculate element offset: index * element_size
                // Note: Layout::size() includes trailing padding, so it equals the stride
                let element_shape = a.t;
                let element_layout = element_shape.layout.sized_layout().map_err(|_| {
                    self.error(ReflectErrorKind::Unsized {
                        shape: ShapeDesc::Static(element_shape),
                    })
                })?;
                let offset = (index as usize) * element_layout.size();
                let element_ptr =
                    unsafe { PtrUninit::new(frame.data.as_mut_byte_ptr().add(offset)) };
                Ok(Frame::new(element_ptr, element_shape))
            }
            _ => {
                // Check for Option/Result types (they have special Def but not a special Type)
                match frame.shape.def() {
                    Def::Option(_) => {
                        // Validate variant index: 0 = None, 1 = Some
                        if index > 1 {
                            return Err(
                                self.error(ReflectErrorKind::OptionVariantOutOfBounds { index })
                            );
                        }
                        // Return shape of the whole Option (like enums)
                        Ok(Frame::new(frame.data, frame.shape))
                    }
                    Def::Result(_) => {
                        // Validate variant index: 0 = Ok, 1 = Err
                        if index > 1 {
                            return Err(
                                self.error(ReflectErrorKind::ResultVariantOutOfBounds { index })
                            );
                        }
                        // Return shape of the whole Result (like enums)
                        Ok(Frame::new(frame.data, frame.shape))
                    }
                    _ => Err(self.error(ReflectErrorKind::NotAStruct)),
                }
            }
        }
    }

    /// Get a struct field by index.
    fn get_struct_field(
        &self,
        fields: &'static [Field],
        index: u32,
    ) -> Result<&'static Field, ReflectError> {
        let idx = index as usize;
        if idx >= fields.len() {
            return Err(self.error(ReflectErrorKind::FieldIndexOutOfBounds {
                index,
                field_count: fields.len(),
            }));
        }
        Ok(&fields[idx])
    }

    /// Get an enum variant by index.
    fn get_enum_variant(
        &self,
        enum_type: &EnumType,
        index: u32,
    ) -> Result<&'static Variant, ReflectError> {
        let idx = index as usize;
        if idx >= enum_type.variants.len() {
            return Err(self.error(ReflectErrorKind::VariantIndexOutOfBounds {
                index,
                variant_count: enum_type.variants.len(),
            }));
        }
        Ok(&enum_type.variants[idx])
    }

    /// Build the final value, consuming the Partial.
    pub fn build<T: Facet<'facet>>(mut self) -> Result<T, ReflectError> {
        // End all frames up to and including root.
        // This finalizes collections (maps, lists, etc.) and marks everything complete.
        while self.current.is_valid() {
            self.apply_end(end::EndInitiator::Build)?;
        }

        let frame = self.arena.get(self.root);

        // Verify shape matches
        if !frame.shape.is_shape(ShapeDesc::Static(T::SHAPE)) {
            return Err(self.error_at(
                self.root,
                ReflectErrorKind::ShapeMismatch {
                    expected: frame.shape,
                    actual: ShapeDesc::Static(T::SHAPE),
                },
            ));
        }

        // Verify initialized - check based on type
        let is_initialized = if frame.flags.contains(FrameFlags::INIT) {
            // Whole value was set (e.g., scalar or Move of entire struct)
            true
        } else {
            // For compound types, check all children are complete
            frame.kind.is_complete()
        };

        if !is_initialized {
            return Err(self.error_at(self.root, ReflectErrorKind::NotInitialized));
        }

        // SAFETY:
        // - frame.data was initialized via copy_from in apply()
        // - INIT flag is set (checked above)
        // - T::SHAPE matches frame.shape (asserted above), so reading as T is valid
        let value = unsafe { frame.data.assume_init().as_const().read::<T>() };

        // Free the frame from arena and deallocate its memory
        let frame = self.arena.free(self.root);

        // Mark as invalid so Drop doesn't try to free again
        self.root = Idx::COMPLETE;
        self.current = Idx::COMPLETE;

        frame.dealloc_if_owned();

        Ok(value)
    }
}

#[cfg(kani)]
mod kani_proofs {
    use facet_core::Facet;

    // Test 1: Can we use std::alloc?
    #[kani::proof]
    fn test_alloc_raw() {
        let layout = std::alloc::Layout::new::<u32>();
        let ptr = unsafe { std::alloc::alloc(layout) };
        kani::assert(!ptr.is_null(), "alloc succeeded");
        unsafe { std::alloc::dealloc(ptr, layout) };
    }

    // Test 2: Can we call default_in_place from vtable?
    #[kani::proof]
    fn test_vtable_default() {
        let layout = std::alloc::Layout::new::<u32>();
        let ptr = unsafe { std::alloc::alloc(layout) };
        kani::assert(!ptr.is_null(), "alloc succeeded");

        let uninit = facet_core::PtrUninit::new(ptr);
        let result = unsafe { u32::SHAPE.call_default_in_place(uninit) };
        kani::assert(result.is_some(), "default_in_place succeeded");

        // Read the value back
        let value = unsafe { *(ptr as *const u32) };
        kani::assert(value == 0, "default u32 is 0");

        unsafe { std::alloc::dealloc(ptr, layout) };
    }

    // Test 3: Can we call drop_in_place from vtable?
    #[kani::proof]
    fn test_vtable_drop() {
        let layout = std::alloc::Layout::new::<u32>();
        let ptr = unsafe { std::alloc::alloc(layout) };
        kani::assert(!ptr.is_null(), "alloc succeeded");

        // Initialize
        unsafe {
            *(ptr as *mut u32) = 42;
        }

        // Drop (no-op for u32, but tests the vtable call)
        let ptr_mut = facet_core::PtrMut::new(ptr);
        let result = unsafe { u32::SHAPE.call_drop_in_place(ptr_mut) };
        kani::assert(result.is_some(), "drop_in_place succeeded");

        unsafe { std::alloc::dealloc(ptr, layout) };
    }

    // Test 4: Can we use the Arena?
    #[kani::proof]
    fn test_arena_alloc_free() {
        use crate::arena::Arena;
        use crate::frame::Frame;

        let mut arena: Arena<Frame> = Arena::new();

        // Allocate a simple frame
        let layout = std::alloc::Layout::new::<u32>();
        let ptr = unsafe { std::alloc::alloc(layout) };
        kani::assert(!ptr.is_null(), "alloc succeeded");

        let data = facet_core::PtrUninit::new(ptr);
        let frame = Frame::new(data, u32::SHAPE);
        let idx = arena.alloc(frame);

        kani::assert(idx.is_valid(), "arena index is valid");

        // Free the frame
        let freed = arena.free(idx);
        freed.dealloc_if_owned();

        // Clean up
        unsafe { std::alloc::dealloc(ptr, layout) };
    }

    // Test 5: Can we call Partial::alloc? (forget to skip Drop)
    #[kani::proof]
    fn test_partial_alloc_u32_forget() {
        use super::Partial;

        let partial = Partial::alloc::<u32>();
        kani::assert(partial.is_ok(), "Partial::alloc succeeded");
        // Forget to avoid Drop - isolate whether alloc or Drop is the problem
        std::mem::forget(partial);
    }

    // Test 6: Partial::alloc with Drop, bounded unwind
    #[kani::proof]
    #[kani::unwind(2)]
    fn test_partial_alloc_u32_with_drop() {
        use super::Partial;
        use crate::frame::FrameKind;

        let partial = Partial::alloc::<u32>();
        kani::assert(partial.is_ok(), "Partial::alloc succeeded");

        let partial = partial.unwrap();

        // Tell Kani: this is a scalar, not a list/map/struct/enum
        // This constrains the paths Kani explores in Drop
        let frame = partial.arena.get(partial.root);
        kani::assume(matches!(frame.kind, FrameKind::Scalar));

        // Now let it drop - Kani should only explore the scalar path
    }

    // =======================================================================
    // Pathological test: minimal recursive drop with loops
    // This doesn't use Partial - it's a standalone reproduction of the pattern
    // that makes Kani explode. We'll use this to experiment with contracts.
    // =======================================================================
    // Memory model with state tracking for Kani verification
    // =======================================================================

    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    enum RegionState {
        Initialized,
        Dropped,
    }

    // Global drop counter - tracks total drops across all nodes
    static mut TOTAL_DROPS: usize = 0;

    /// A node with explicit state tracking for Kani verification
    struct Node {
        value: u32,
        children: Vec<Node>,
        state: RegionState,
    }

    impl Node {
        fn new(value: u32) -> Self {
            Self {
                value,
                children: Vec::new(),
                state: RegionState::Initialized,
            }
        }

        fn with_children(value: u32, children: Vec<Node>) -> Self {
            Self {
                value,
                children,
                state: RegionState::Initialized,
            }
        }

        /// Count total nodes in this subtree (including self)
        fn count_nodes(&self) -> usize {
            1 + self.children.iter().map(|c| c.count_nodes()).sum::<usize>()
        }
    }

    /// Drop a node and all its children, with loop invariant
    /// Returns the number of nodes dropped (for verification)
    #[kani::requires(node.state == RegionState::Initialized)]
    #[kani::ensures(|_| true)] // Can't easily express postcondition without iteration
    #[kani::recursion]
    fn drop_node(node: &mut Node) -> usize {
        // Precondition: node must be initialized (not already dropped)
        kani::assert(
            node.state == RegionState::Initialized,
            "dropping initialized node",
        );

        let num_children = node.children.len();
        let mut i: usize = 0;
        let mut dropped_in_children: usize = 0;

        // Loop invariant:
        // - i is bounded by num_children
        // - all children at indices < i have been dropped
        #[kani::loop_invariant(
            i <= num_children &&
            kani::forall!(|j in (0, i)| node.children[j as usize].state == RegionState::Dropped)
        )]
        #[kani::loop_modifies(&i, &dropped_in_children, &node.children)]
        while i < num_children {
            // Use saturating_add to avoid overflow concerns
            dropped_in_children =
                dropped_in_children.saturating_add(drop_node(&mut node.children[i]));
            i = i.wrapping_add(1);
        }

        // Mark this node as dropped
        node.state = RegionState::Dropped;
        unsafe {
            TOTAL_DROPS = TOTAL_DROPS.wrapping_add(1);
        }

        // Return total: children + self
        dropped_in_children.saturating_add(1)
    }

    impl Drop for Node {
        fn drop(&mut self) {
            // Only drop if not already dropped (avoid double-drop from explicit + implicit)
            if self.state == RegionState::Initialized {
                drop_node(self);
            }
        }
    }

    // Test 7: Single node, no children - should be fast
    #[kani::proof]
    fn test_node_leaf() {
        let mut node = Node::new(42);
        kani::assert(node.value == 42, "value is correct");

        // Explicitly drop and verify count
        let dropped = drop_node(&mut node);
        kani::assert(dropped == 1, "dropped exactly 1 node");

        // Forget to avoid double-drop from Drop impl
        std::mem::forget(node);
    }

    // Test 8: One level of children - will this work?
    #[kani::proof]
    #[kani::unwind(3)]
    fn test_node_one_level() {
        let leaf1 = Node::new(1);
        let leaf2 = Node::new(2);
        let parent = Node::with_children(0, vec![leaf1, leaf2]);
        kani::assert(parent.children.len() == 2, "has 2 children");
        // Drop - one loop iteration per child, no deeper recursion
    }

    // Test 9: Two levels - this is where it gets painful
    #[kani::proof]
    #[kani::unwind(5)]
    fn test_node_two_levels() {
        let grandchild = Node::new(100);
        let child = Node::with_children(10, vec![grandchild]);
        let root = Node::with_children(0, vec![child]);
        kani::assert(root.children.len() == 1, "has 1 child");
        // Drop - recursion depth 2, should explode without contracts
    }

    // =======================================================================
    // Now let's add dynamism - this should break things
    // =======================================================================

    // Test 10: Symbolic number of children - now with loop invariant
    #[kani::proof]
    #[kani::unwind(5)]
    fn test_node_symbolic_children_count() {
        let n: usize = kani::any();
        kani::assume(n <= 3); // Bound it, but still symbolic

        let mut children = Vec::new();
        let mut k: usize = 0;

        #[kani::loop_invariant(k <= n && children.len() == k)]
        #[kani::loop_modifies(&k, &children)]
        while k < n {
            children.push(Node::new(1));
            k = k.wrapping_add(1);
        }

        let parent = Node::with_children(0, children);
        kani::assert(parent.children.len() <= 3, "bounded children");
        // Drop with symbolic number of children - loop invariant should help!
    }

    // Test 11: Symbolic depth via function pointer (simulating vtable)
    #[kani::proof]
    #[kani::unwind(5)]
    fn test_node_dynamic_drop() {
        type DropFn = fn(&mut Node);

        fn drop_shallow(node: &mut Node) {
            // Don't recurse into children
        }

        fn drop_deep(node: &mut Node) {
            for child in &mut node.children {
                drop_deep(child);
            }
        }

        // Symbolic choice of drop function - like vtable dispatch
        let drop_fn: DropFn = if kani::any() { drop_shallow } else { drop_deep };

        let mut root = Node::with_children(0, vec![Node::new(1)]);
        drop_fn(&mut root);
        // Now let normal drop run too
    }
}

impl<'facet> Drop for Partial<'facet> {
    fn drop(&mut self) {
        // Walk from current frame up to root, cleaning up each frame.
        // This handles in-progress child frames (e.g., list elements being built).
        let mut idx = self.current;
        while idx.is_valid() {
            let frame = self.arena.get_mut(idx);
            let parent = frame.parent_link.parent_idx();

            // Drop any initialized data in this frame
            frame.uninit();

            // Free the frame and deallocate if it owns its allocation
            // Free the frame and deallocate if it owns its allocation
            let frame = self.arena.free(idx);
            frame.dealloc_if_owned();

            // Move to parent
            idx = parent.unwrap_or(Idx::COMPLETE);
        }
    }
}
