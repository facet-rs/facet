use super::Partial;
use crate::errors::{ReflectError, ReflectErrorKind};
use crate::frame::{Frame, FrameFlags, FrameKind, ListFrame, ParentLink};
use crate::ops::Source;
use crate::shape_desc::ShapeDesc;
use facet_core::{Def, ListDef, PtrConst, PtrMut, PtrUninit, Shape, Type, UserType};

/// Direct-fill operations for lists.
/// All functions are present if direct-fill is supported.
struct ListDirectFill {
    as_mut_ptr_typed: facet_core::ListAsMutPtrTypedFn,
    reserve: facet_core::ListReserveFn,
    capacity: facet_core::ListCapacityFn,
    element_shape: &'static Shape,
    element_size: usize,
}

impl ListDirectFill {
    /// Try to get direct-fill operations from a ListDef.
    /// Returns None if any required operation is missing.
    fn try_new(def: &ListDef) -> Option<Self> {
        let element_shape = def.t;
        let element_size = element_shape.layout.sized_layout().ok()?.size();

        Some(Self {
            as_mut_ptr_typed: def.as_mut_ptr_typed()?,
            reserve: def.reserve()?,
            capacity: def.capacity()?,
            element_shape,
            element_size,
        })
    }

    /// Get pointer to the slot at index `idx` in the list's buffer.
    ///
    /// # Safety
    /// - `list_ptr` must point to an initialized list
    /// - `idx` must be less than capacity
    unsafe fn slot_ptr(&self, list_ptr: PtrMut, idx: usize) -> PtrUninit {
        let buffer = unsafe { (self.as_mut_ptr_typed)(list_ptr) };
        let offset = idx * self.element_size;
        PtrUninit::new(unsafe { buffer.add(offset) })
    }

    /// Ensure the list has capacity for at least one more element.
    /// Returns the new cached capacity.
    ///
    /// # Safety
    /// - `list_ptr` must point to an initialized list
    unsafe fn ensure_capacity(&self, list_ptr: PtrMut, list_frame: &mut ListFrame) -> usize {
        let current_len = list_frame.len + list_frame.staged_len;
        if current_len >= list_frame.cached_capacity {
            // Need to grow - reserve at least 1, or double if we have elements
            let additional = if list_frame.cached_capacity == 0 {
                4 // Initial allocation
            } else {
                list_frame.cached_capacity // Double
            };
            unsafe { (self.reserve)(list_ptr, additional) };
            list_frame.cached_capacity = unsafe { (self.capacity)(list_ptr.as_const()) };
        }
        list_frame.cached_capacity
    }
}

impl<'facet> Partial<'facet> {
    /// Apply a Push operation to add an element to the current list or set.
    pub(crate) fn apply_push(&mut self, source: &Source<'_>) -> Result<(), ReflectError> {
        // Ensure collection is initialized (lazy init on first push)
        self.ensure_collection_initialized()?;

        // Check if this is a list or set
        let frame = self.arena.get(self.current);
        match &frame.kind {
            FrameKind::List(_) => self.apply_push_to_list(source),
            FrameKind::Set(_) => self.apply_push_to_set(source),
            _ => Err(self.error(ReflectErrorKind::NotAList)),
        }
    }

    /// Push to a list using direct-fill when possible, falling back to push.
    fn apply_push_to_list(&mut self, source: &Source<'_>) -> Result<(), ReflectError> {
        let frame = self.arena.get(self.current);
        let FrameKind::List(list_frame) = &frame.kind else {
            unreachable!()
        };
        let def = list_frame.def;
        let list_ptr = unsafe { frame.data.assume_init() };

        // Try to use direct-fill, fall back to push
        let direct_fill = ListDirectFill::try_new(&def);

        match source {
            Source::Imm(mov) => {
                let element_shape = def.t;
                if !element_shape.is_shape(mov.shape()) {
                    return Err(self.error(ReflectErrorKind::ShapeMismatch {
                        expected: ShapeDesc::Static(element_shape),
                        actual: ShapeDesc::Static(mov.shape()),
                    }));
                }

                if let Some(df) = &direct_fill {
                    // Direct-fill: write directly into buffer
                    self.list_direct_fill_imm(list_ptr, df, mov.ptr())?;
                } else {
                    // Fallback: use push
                    self.list_push_imm(list_ptr, &def, mov.ptr())?;
                }
            }
            Source::Stage(_capacity) => {
                if let Some(df) = &direct_fill {
                    // Direct-fill: create frame pointing into Vec's buffer
                    self.list_direct_fill_stage(list_ptr, df)?;
                } else {
                    // Fallback: allocate temp memory
                    self.list_push_stage(&def)?;
                }
            }
            Source::Default => {
                let element_shape = def.t;
                if let Some(df) = &direct_fill {
                    // Direct-fill: write default directly into buffer
                    self.list_direct_fill_default(list_ptr, df, element_shape)?;
                } else {
                    // Fallback: use push
                    self.list_push_default(list_ptr, &def, element_shape)?;
                }
            }
        }

        Ok(())
    }

    /// Direct-fill: write an immediate value into the Vec's buffer.
    fn list_direct_fill_imm(
        &mut self,
        list_ptr: PtrMut,
        df: &ListDirectFill,
        src: PtrConst,
    ) -> Result<(), ReflectError> {
        // Get mutable access to list frame
        let frame = self.arena.get_mut(self.current);
        let FrameKind::List(ref mut list_frame) = frame.kind else {
            unreachable!()
        };

        // Ensure we have capacity
        unsafe { df.ensure_capacity(list_ptr, list_frame) };

        // Get slot pointer and copy element
        let idx = list_frame.len + list_frame.staged_len;
        let slot = unsafe { df.slot_ptr(list_ptr, idx) };

        // SAFETY: slot points to uninitialized memory in Vec's buffer,
        // src points to valid initialized data of the correct type
        unsafe {
            std::ptr::copy_nonoverlapping(
                src.as_byte_ptr(),
                slot.as_mut_byte_ptr(),
                df.element_size,
            );
        }

        list_frame.staged_len += 1;
        Ok(())
    }

    /// Direct-fill: stage an element by creating a frame pointing into Vec's buffer.
    fn list_direct_fill_stage(
        &mut self,
        list_ptr: PtrMut,
        df: &ListDirectFill,
    ) -> Result<(), ReflectError> {
        // Get mutable access to list frame first
        {
            let frame = self.arena.get_mut(self.current);
            let FrameKind::List(ref mut list_frame) = frame.kind else {
                unreachable!()
            };

            // Ensure we have capacity
            unsafe { df.ensure_capacity(list_ptr, list_frame) };
        }

        // Now get the slot pointer (need to re-get frame since ensure_capacity mutated it)
        let frame = self.arena.get(self.current);
        let FrameKind::List(ref list_frame) = frame.kind else {
            unreachable!()
        };
        let idx = list_frame.len + list_frame.staged_len;
        let slot = unsafe { df.slot_ptr(list_ptr, idx) };

        // Create frame pointing into the buffer
        let element_shape = df.element_shape;
        let mut element_frame = Self::create_frame_for_shape(slot, element_shape);

        // Do NOT set OWNS_ALLOC - Vec owns this memory
        element_frame.parent_link = ParentLink::ListElement {
            parent: self.current,
        };

        let element_idx = self.arena.alloc(element_frame);
        self.current = element_idx;
        Ok(())
    }

    /// Direct-fill: write default value directly into Vec's buffer.
    fn list_direct_fill_default(
        &mut self,
        list_ptr: PtrMut,
        df: &ListDirectFill,
        element_shape: &'static Shape,
    ) -> Result<(), ReflectError> {
        // Get mutable access to list frame
        let frame = self.arena.get_mut(self.current);
        let FrameKind::List(ref mut list_frame) = frame.kind else {
            unreachable!()
        };

        // Ensure we have capacity
        unsafe { df.ensure_capacity(list_ptr, list_frame) };

        // Get slot pointer
        let idx = list_frame.len + list_frame.staged_len;
        let slot = unsafe { df.slot_ptr(list_ptr, idx) };

        // Initialize with default
        let ok = unsafe { element_shape.call_default_in_place(slot) };
        if ok.is_none() {
            return Err(self.error(ReflectErrorKind::NoDefault {
                shape: ShapeDesc::Static(element_shape),
            }));
        }

        list_frame.staged_len += 1;
        Ok(())
    }

    /// Fallback: push an immediate value using push function.
    fn list_push_imm(
        &mut self,
        list_ptr: PtrMut,
        def: &ListDef,
        src: PtrConst,
    ) -> Result<(), ReflectError> {
        let push_fn = def.push().ok_or_else(|| {
            let frame = self.arena.get(self.current);
            self.error(ReflectErrorKind::ListDoesNotSupportOp { shape: frame.shape })
        })?;

        // SAFETY: src points to valid initialized data of the element type
        unsafe { push_fn(list_ptr, src) };

        let frame = self.arena.get_mut(self.current);
        if let FrameKind::List(ref mut l) = frame.kind {
            l.len += 1;
        }
        Ok(())
    }

    /// Fallback: stage an element by allocating temp memory.
    fn list_push_stage(&mut self, def: &ListDef) -> Result<(), ReflectError> {
        let element_shape = def.t;
        let layout = element_shape.layout.sized_layout().map_err(|_| {
            self.error(ReflectErrorKind::Unsized {
                shape: ShapeDesc::Static(element_shape),
            })
        })?;

        let temp_ptr = if layout.size() == 0 {
            PtrUninit::new(std::ptr::NonNull::<u8>::dangling().as_ptr())
        } else {
            let ptr = unsafe { std::alloc::alloc(layout) };
            if ptr.is_null() {
                return Err(self.error(ReflectErrorKind::AllocFailed { layout }));
            }
            PtrUninit::new(ptr)
        };

        let mut element_frame = Self::create_frame_for_shape(temp_ptr, element_shape);

        // Mark that this frame owns its allocation
        element_frame.flags |= FrameFlags::OWNS_ALLOC;
        element_frame.parent_link = ParentLink::ListElement {
            parent: self.current,
        };

        let element_idx = self.arena.alloc(element_frame);
        self.current = element_idx;
        Ok(())
    }

    /// Fallback: push default value using push function.
    fn list_push_default(
        &mut self,
        list_ptr: PtrMut,
        def: &ListDef,
        element_shape: &'static Shape,
    ) -> Result<(), ReflectError> {
        let push_fn = def.push().ok_or_else(|| {
            let frame = self.arena.get(self.current);
            self.error(ReflectErrorKind::ListDoesNotSupportOp { shape: frame.shape })
        })?;

        let layout = element_shape.layout.sized_layout().map_err(|_| {
            self.error(ReflectErrorKind::Unsized {
                shape: ShapeDesc::Static(element_shape),
            })
        })?;

        let temp_ptr = if layout.size() == 0 {
            PtrUninit::new(std::ptr::NonNull::<u8>::dangling().as_ptr())
        } else {
            let ptr = unsafe { std::alloc::alloc(layout) };
            if ptr.is_null() {
                return Err(self.error(ReflectErrorKind::AllocFailed { layout }));
            }
            PtrUninit::new(ptr)
        };

        // Initialize with default
        let ok = unsafe { element_shape.call_default_in_place(temp_ptr) };
        if ok.is_none() {
            if layout.size() > 0 {
                unsafe { std::alloc::dealloc(temp_ptr.as_mut_byte_ptr(), layout) };
            }
            return Err(self.error(ReflectErrorKind::NoDefault {
                shape: ShapeDesc::Static(element_shape),
            }));
        }

        // SAFETY: temp_ptr now contains initialized data
        unsafe { push_fn(list_ptr, temp_ptr.assume_init().as_const()) };

        let frame = self.arena.get_mut(self.current);
        if let FrameKind::List(ref mut l) = frame.kind {
            l.len += 1;
        }

        // Deallocate temp storage
        if layout.size() > 0 {
            unsafe { std::alloc::dealloc(temp_ptr.as_mut_byte_ptr(), layout) };
        }
        Ok(())
    }

    /// Push to a set using slab-based collection.
    ///
    /// Sets use a slab to collect elements, then build the set at End via `from_slice`.
    fn apply_push_to_set(&mut self, source: &Source<'_>) -> Result<(), ReflectError> {
        let frame = self.arena.get(self.current);
        let FrameKind::Set(set_frame) = &frame.kind else {
            unreachable!()
        };
        let def = set_frame.def;
        let element_shape = def.t;
        let current_len = set_frame.len;

        match source {
            Source::Imm(mov) => {
                if !element_shape.is_shape(mov.shape()) {
                    return Err(self.error(ReflectErrorKind::ShapeMismatch {
                        expected: ShapeDesc::Static(element_shape),
                        actual: ShapeDesc::Static(mov.shape()),
                    }));
                }

                // Get element size for copying
                let element_size = element_shape
                    .layout
                    .sized_layout()
                    .map(|l| l.size())
                    .unwrap_or(0);

                // Get a slot from the slab
                let frame = self.arena.get_mut(self.current);
                let FrameKind::Set(ref mut set_frame) = frame.kind else {
                    unreachable!()
                };
                let slab = set_frame
                    .slab
                    .as_mut()
                    .expect("slab must exist after ensure_collection_initialized");
                let slot = slab.nth_slot(current_len);

                // Copy the element into the slab
                if element_size > 0 {
                    unsafe {
                        std::ptr::copy_nonoverlapping(
                            mov.ptr().as_byte_ptr(),
                            slot.as_mut_byte_ptr(),
                            element_size,
                        );
                    }
                }

                // Increment element count
                set_frame.len += 1;
            }
            Source::Stage(_capacity) => {
                // Get a slot from the slab
                let frame = self.arena.get_mut(self.current);
                let FrameKind::Set(ref mut set_frame) = frame.kind else {
                    unreachable!()
                };
                let slab = set_frame
                    .slab
                    .as_mut()
                    .expect("slab must exist after ensure_collection_initialized");
                let slot = slab.nth_slot(current_len);

                // Create frame pointing into the slab
                // Do NOT set OWNS_ALLOC - the slab owns this memory
                let mut element_frame = Self::create_frame_for_shape(slot, element_shape);
                element_frame.parent_link = ParentLink::SetElement {
                    parent: self.current,
                };

                let element_idx = self.arena.alloc(element_frame);
                self.current = element_idx;
            }
            Source::Default => {
                // Get a slot from the slab
                let frame = self.arena.get_mut(self.current);
                let FrameKind::Set(ref mut set_frame) = frame.kind else {
                    unreachable!()
                };
                let slab = set_frame
                    .slab
                    .as_mut()
                    .expect("slab must exist after ensure_collection_initialized");
                let slot = slab.nth_slot(current_len);

                // Initialize with default directly into slab
                let ok = unsafe { element_shape.call_default_in_place(slot) };
                if ok.is_none() {
                    return Err(self.error(ReflectErrorKind::NoDefault {
                        shape: ShapeDesc::Static(element_shape),
                    }));
                }

                // Increment element count
                set_frame.len += 1;
            }
        }

        Ok(())
    }

    /// Create a frame appropriate for the given shape.
    fn create_frame_for_shape(ptr: PtrUninit, shape: &'static Shape) -> Frame {
        match shape.def {
            Def::List(list_def) => Frame::new_list(ptr, shape, list_def),
            Def::Map(map_def) => Frame::new_map(ptr, shape, map_def),
            Def::Set(set_def) => Frame::new_set(ptr, shape, set_def),
            Def::Option(_) => Frame::new_option(ptr, shape),
            Def::Result(_) => Frame::new_result(ptr, shape),
            _ => match shape.ty {
                Type::User(UserType::Struct(ref s)) => {
                    Frame::new_struct(ptr, shape, s.fields.len())
                }
                Type::User(UserType::Enum(_)) => Frame::new_enum(ptr, shape),
                _ => Frame::new(ptr, shape),
            },
        }
    }
}
