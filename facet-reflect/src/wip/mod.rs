extern crate alloc;
use crate::{ReflectError, ValueId};
use core::{alloc::Layout, marker::PhantomData};
use facet_ansi::Stylize;
use facet_core::{Def, Facet, FieldError, Opaque, OpaqueConst, OpaqueUninit, Shape, Variant};
use indexmap::IndexMap;

/// Represents a frame in the initialization stack
pub struct Frame {
    /// The value we're initializing
    data: OpaqueUninit<'static>,

    /// The shape of the value
    shape: &'static Shape,

    /// If set, when we're initialized, we must mark the
    /// parent's indexth field as initialized.
    index: Option<usize>,

    /// Tracking which of our fields are initialized
    /// TODO: I'm not sure we should track "ourselves" as initialized — we always have the
    /// parent to look out for, right now we're tracking children in two states, which isn't ideal
    istate: IState,
}

impl Frame {
    /// Returns the value ID for a frame
    fn id(&self) -> ValueId {
        ValueId::new(self.shape, self.data.as_byte_ptr())
    }

    /// Returns true if the frame is fully initialized
    fn is_fully_initialized(&self) -> bool {
        match self.shape.def {
            Def::Struct(sd) => self.istate.fields.are_all_set(sd.fields.len()),
            Def::Enum(_) => match self.istate.variant.as_ref() {
                None => false,
                Some(v) => self.istate.fields.are_all_set(v.data.fields.len()),
            },
            _ => self.istate.fields.are_all_set(1),
        }
    }

    /// Marks the frame as fully initialized
    unsafe fn mark_fully_initialized(&mut self) {
        match self.shape.def {
            Def::Struct(sd) => {
                self.istate.fields = ISet::all(sd.fields);
            }
            Def::Enum(_) => {
                if let Some(variant) = &self.istate.variant {
                    self.istate.fields = ISet::all(variant.data.fields);
                }
            }
            _ => {
                self.istate.fields.set(0);
            }
        }
    }
}

/// Initialization state
struct IState {
    /// Variant chosen — for everything except enums, this stays None
    variant: Option<Variant>,

    /// Fields that were initialized. For scalars, we only track 0
    fields: ISet,

    /// The depth of the frame in the stack
    depth: usize,
}

impl IState {
    /// Creates a new `IState` with the given depth.
    pub fn new(depth: usize) -> Self {
        Self {
            variant: None,
            fields: Default::default(),
            depth,
        }
    }
}

/// A work-in-progress heap-allocated value
pub struct Wip<'a> {
    /// frees the memory when dropped
    guard: Option<Guard>,

    /// stack of frames to keep track of deeply nested initialization
    frames: alloc::vec::Vec<Frame>,

    /// keeps track of initialization of out-of-tree frames
    istates: IndexMap<ValueId, IState>,

    /// lifetime of the shortest reference we hold
    phantom: PhantomData<&'a ()>,
}

impl<'a> Wip<'a> {
    /// Allocates a new value of the given shape
    pub fn alloc_shape(shape: &'static Shape) -> Self {
        let data = shape.allocate();
        let guard = Guard {
            ptr: data.as_mut_byte_ptr(),
            layout: shape.layout,
        };
        Self {
            guard: Some(guard),
            frames: vec![Frame {
                data,
                shape,
                index: None,
                istate: IState::new(0),
            }],
            istates: Default::default(),
            phantom: PhantomData,
        }
    }

    /// Allocates a new value of type `S`
    pub fn alloc<S: Facet>() -> Self {
        Self::alloc_shape(S::SHAPE)
    }

    fn pop_inner(&mut self) -> Option<Frame> {
        let frame = self.frames.pop()?;
        let frame_shape = frame.shape;

        let init = frame.is_fully_initialized();
        log::trace!(
            "[{}] {} popped, {} initialized",
            self.frames.len(),
            frame_shape.blue(),
            if init {
                "✅ fully".green()
            } else {
                "🚧 partially".red()
            }
        );
        if init {
            if let Some(parent) = self.frames.last_mut() {
                if let Some(index) = frame.index {
                    parent.istate.fields.set(index);
                }
            }
        }

        Some(frame)
    }

    fn track(&mut self, frame: Frame) {
        self.istates.insert(frame.id(), frame.istate);
    }

    /// Returns the shape of the current frame
    pub fn shape(&self) -> &'static Shape {
        self.frames.last().unwrap().shape
    }

    /// Asserts everything is initialized and that invariants are upheld (if any)
    pub fn build(mut self) -> Result<HeapValue<'a>, ReflectError> {
        let mut root: Option<Frame> = None;
        while let Some(frame) = self.pop_inner() {
            if let Some(old_root) = root.replace(frame) {
                self.track(old_root);
            }
        }
        let Some(root) = root else {
            return Err(ReflectError::OperationFailed {
                shape: <()>::SHAPE,
                operation: "tried to build a value but there was no root frame",
            });
        };

        let shape = root.shape;
        let data = unsafe { root.data.assume_init() };

        self.istates.insert(root.id(), root.istate);

        for (id, is) in &self.istates {
            let field_count = match id.shape.def {
                Def::Struct(def) => def.fields.len(),
                Def::Enum(_) => todo!(),
                _ => 1,
            };
            if !is.fields.are_all_set(field_count) {
                match id.shape.def {
                    Def::Struct(sd) => {
                        for (i, field) in sd.fields.iter().enumerate() {
                            if !is.fields.has(i) {
                                panic!("Field '{}::{}' was not initialized", id.shape, field.name);
                            }
                        }
                    }
                    Def::Enum(_) => {
                        todo!()
                    }
                    Def::Scalar(_) => {
                        panic!("Field was not initialized for scalar {}", id.shape);
                    }
                    _ => {}
                }
            }
        }

        if let Some(invariant_fn) = shape.vtable.invariants {
            if !unsafe { invariant_fn(OpaqueConst::new(data.as_byte_ptr())) } {
                return Err(ReflectError::InvariantViolation {
                    invariant: "Custom validation function returned false",
                });
            }
        }

        let guard = self.guard.take().unwrap();

        // don't double-drop the fields!
        self.istates.clear();

        Ok(HeapValue {
            guard: Some(guard),
            shape,
            phantom: PhantomData,
        })
    }

    /// Selects a field of a struct by index and pushes it onto the frame stack.
    ///
    /// # Arguments
    ///
    /// * `index` - The index of the field to select.
    ///
    /// # Returns
    ///
    /// * `Ok(Self)` if the field was successfully selected and pushed.
    /// * `Err(ReflectError)` if the current frame is not a struct or the field doesn't exist.
    pub fn field(mut self, index: usize) -> Result<Self, ReflectError> {
        let frame = self.frames.last_mut().unwrap();
        let shape = frame.shape;
        let Def::Struct(def) = shape.def else {
            return Err(ReflectError::WasNotA { name: "struct" });
        };
        if index >= def.fields.len() {
            return Err(ReflectError::FieldError {
                shape,
                field_error: FieldError::NoSuchField,
            });
        }
        let field = &def.fields[index];
        let field_data = unsafe { frame.data.field_uninit_at(field.offset) };

        let mut frame = Frame {
            data: field_data,
            shape: field.shape,
            index: Some(index),
            istate: IState::new(self.frames.len()),
        };
        log::trace!(
            "[{}] Selecting field {} ({}#{}) of {}",
            self.frames.len(),
            field.name.blue(),
            field.shape.green(),
            index.yellow(),
            shape.blue(),
        );
        if let Some(iset) = self.istates.shift_remove(&frame.id()) {
            log::trace!(
                "[{}] Restoring saved state for {}",
                self.frames.len(),
                frame.id().shape.blue()
            );
            frame.istate = iset;
        }
        self.frames.push(frame);
        Ok(self)
    }

    /// Finds the index of a field in a struct by name.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the field to find.
    ///
    /// # Returns
    ///
    /// * `Some(usize)` if the field was found.
    /// * `None` if the current frame is not a struct or the field doesn't exist.
    pub fn field_index(&self, name: &str) -> Option<usize> {
        let frame = self.frames.last()?;
        if let Def::Struct(def) = frame.shape.def {
            def.fields.iter().position(|f| f.name == name)
        } else {
            None
        }
    }

    /// Selects a field of a struct by name and pushes it onto the frame stack.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the field to select.
    ///
    /// # Returns
    ///
    /// * `Ok(Self)` if the field was successfully selected and pushed.
    /// * `Err(ReflectError)` if the current frame is not a struct or the field doesn't exist.
    pub fn field_named(self, name: &str) -> Result<Self, ReflectError> {
        let frame = self.frames.last().unwrap();
        let shape = frame.shape;
        let index = self.field_index(name).ok_or(ReflectError::FieldError {
            shape,
            field_error: FieldError::NoSuchField,
        })?;
        self.field(index)
    }

    /// Puts a value of type `T` into the current frame.
    ///
    /// # Arguments
    ///
    /// * `t` - The value to put into the frame.
    ///
    /// # Returns
    ///
    /// * `Ok(Self)` if the value was successfully put into the frame.
    /// * `Err(ReflectError)` if there was an error putting the value into the frame.
    pub fn put<'val, T: Facet + 'val>(mut self, t: T) -> Result<Wip<'val>, ReflectError>
    where
        'a: 'val,
    {
        let Some(frame) = self.frames.last_mut() else {
            return Err(ReflectError::OperationFailed {
                shape: T::SHAPE,
                operation: "tried to put a T but there was no frame to put T into",
            });
        };

        // check that the type matches
        if !frame.shape.is_type::<T>() {
            return Err(ReflectError::WrongShape {
                expected: frame.shape,
                actual: T::SHAPE,
            });
        }

        // de-initialize partially initialized fields
        if frame.istate.variant.is_some() || frame.istate.fields.is_any_set() {
            todo!(
                "we should de-initialize partially initialized fields for {}",
                frame.shape
            );
        }

        unsafe {
            frame.data.put(t);
            frame.mark_fully_initialized();
        }

        let shape = frame.shape;
        let index = frame.index;

        // mark the field as initialized
        self.mark_field_as_initialized(shape, index)?;

        Ok(self)
    }

    /// Tries to parse the current frame's value from a string
    pub fn parse(mut self, s: &str) -> Result<Self, ReflectError> {
        let Some(frame) = self.frames.last_mut() else {
            return Err(ReflectError::OperationFailed {
                shape: <()>::SHAPE,
                operation: "tried to parse value but there was no frame",
            });
        };

        let shape = frame.shape;
        let index = frame.index;

        let Some(parse_fn) = frame.shape.vtable.parse else {
            return Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "type does not implement Parse",
            });
        };
        match unsafe { (parse_fn)(s, frame.data) } {
            Ok(_res) => {
                unsafe {
                    frame.mark_fully_initialized();
                }

                // mark the field as initialized
                self.mark_field_as_initialized(shape, index)?;

                Ok(self)
            }
            Err(_) => Err(ReflectError::OperationFailed {
                shape,
                operation: "parsing",
            }),
        }
    }

    /// Puts the default value in the currrent frame.
    pub fn put_default(mut self) -> Result<Self, ReflectError> {
        let Some(frame) = self.frames.last_mut() else {
            return Err(ReflectError::OperationFailed {
                shape: <()>::SHAPE,
                operation: "tried to put default value but there was no frame",
            });
        };

        let vtable = frame.shape.vtable;

        let Some(default_in_place) = vtable.default_in_place else {
            return Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "type does not implement Default",
            });
        };
        unsafe {
            default_in_place(frame.data);
            frame.mark_fully_initialized();
        }

        let shape = frame.shape;
        let index = frame.index;

        // mark the field as initialized
        self.mark_field_as_initialized(shape, index)?;

        Ok(self)
    }

    /// Marks a field as initialized in the parent frame.
    fn mark_field_as_initialized(
        &mut self,
        shape: &'static Shape,
        index: Option<usize>,
    ) -> Result<(), ReflectError> {
        if let Some(index) = index {
            let parent_index = self.frames.len().saturating_sub(2);
            let num_frames = self.frames.len();
            let Some(parent) = self.frames.get_mut(parent_index) else {
                return Err(ReflectError::OperationFailed {
                    shape,
                    operation: "was supposed to mark a field as initialized, but there was no parent frame",
                });
            };
            let parent_shape = parent.shape;
            log::trace!(
                "[{}] {}.{} initialized with {}",
                num_frames,
                parent_shape.blue(),
                index.yellow(),
                shape.green()
            );

            if matches!(parent.shape.def, Def::Enum(_)) && parent.istate.variant.is_none() {
                return Err(ReflectError::OperationFailed {
                    shape,
                    operation: "was supposed to mark a field as initialized, but the parent frame was an enum and didn't have a variant chosen",
                });
            }

            if parent.istate.fields.has(index) {
                return Err(ReflectError::OperationFailed {
                    shape,
                    operation: "was supposed to mark a field as initialized, but the parent frame already had it marked as initialized",
                });
            }

            parent.istate.fields.set(index);
        }
        Ok(())
    }

    /// Pops the current frame — goes back up one level
    pub fn pop(mut self) -> Result<Self, ReflectError> {
        let frame = match self.frames.len() {
            0 => Err(ReflectError::InvariantViolation {
                invariant: "No frame to pop",
            }),
            1 => Err(ReflectError::InvariantViolation {
                invariant: "The last frame should be popped through build",
            }),
            _ => Ok(self.pop_inner().unwrap()),
        }?;
        self.track(frame);
        Ok(self)
    }
}

impl Drop for Wip<'_> {
    fn drop(&mut self) {
        while let Some(frame) = self.frames.pop() {
            self.track(frame);
        }

        log::trace!(
            "[{}] 🚯 Dropping, was tracking {} istates",
            self.frames.len(),
            self.istates.len()
        );
        for (id, is) in &self.istates {
            log::trace!(
                "[{}]: variant={:?} initialized={:016b} {}",
                is.depth.yellow(),
                is.variant.green(),
                is.fields.0.bright_magenta(),
                id.shape.blue(),
            );
        }

        let mut depths: Vec<usize> = self.istates.values().map(|is| is.depth).collect();
        depths.sort_unstable();
        depths.dedup();

        for depth in depths.iter().rev() {
            log::trace!("Dropping istates with depth {}", depth.yellow(),);

            // Find and drop values at this depth level
            let mut to_remove = Vec::new();
            self.istates.retain(|id, is| {
                if is.depth == *depth {
                    log::trace!(
                        "Dropping value ID with shape {} and fields {:016b}",
                        id.shape.blue(),
                        is.fields.0.bright_magenta()
                    );

                    if !is.fields.is_any_set() {
                        log::trace!("  Skipping drop: no fields were initialized");
                        to_remove.push(*id);
                        return false;
                    }

                    if matches!(id.shape.def, Def::Struct(_) | Def::Enum(_)) {
                        // if it's a composite, rely on the fact that each individual field was deinitialized
                        log::trace!("  Skipping composite type drop: individual fields already deinitialized");
                        to_remove.push(*id);
                        return false;
                    }

                    if let Some(drop_fn) = id.shape.vtable.drop_in_place {
                        // Only drop if some fields were initialized
                        if is.fields.is_any_set() {
                            log::trace!("  Calling drop_in_place function for {}", id.shape.green());
                            unsafe {
                                drop_fn(Opaque::new(id.ptr as *mut u8));
                            }
                        }
                    } else {
                        log::trace!("  No drop_in_place function available for {}", id.shape.red());
                    }

                    to_remove.push(*id);
                    return false;
                }
                true
            });
        }
    }
}

/// A guard structure to manage memory allocation and deallocation.
///
/// This struct holds a raw pointer to the allocated memory and the layout
/// information used for allocation. It's responsible for deallocating
/// the memory when dropped.
pub struct Guard {
    /// Raw pointer to the allocated memory.
    ptr: *mut u8,
    /// Layout information of the allocated memory.
    layout: Layout,
}

impl Drop for Guard {
    fn drop(&mut self) {
        if self.layout.size() != 0 {
            // SAFETY: `ptr` has been allocated via the global allocator with the given layout
            unsafe { alloc::alloc::dealloc(self.ptr, self.layout) };
        }
    }
}

use facet_core::Field;

/// Keeps track of which fields were initialized, up to 64 fields
#[derive(Clone, Copy, Default, Debug)]
pub struct ISet(u64);

impl ISet {
    /// The maximum index that can be tracked.
    pub const MAX_INDEX: usize = 63;

    /// Creates a new ISet with all (given) fields set.
    pub fn all(fields: &[Field]) -> Self {
        let mut iset = ISet::default();
        for (i, _field) in fields.iter().enumerate() {
            iset.set(i);
        }
        iset
    }

    /// Sets the bit at the given index.
    pub fn set(&mut self, index: usize) {
        if index >= 64 {
            panic!("ISet can only track up to 64 fields. Index {index} is out of bounds.");
        }
        self.0 |= 1 << index;
    }

    /// Unsets the bit at the given index.
    pub fn unset(&mut self, index: usize) {
        if index >= 64 {
            panic!("ISet can only track up to 64 fields. Index {index} is out of bounds.");
        }
        self.0 &= !(1 << index);
    }

    /// Checks if the bit at the given index is set.
    pub fn has(&self, index: usize) -> bool {
        if index >= 64 {
            panic!("ISet can only track up to 64 fields. Index {index} is out of bounds.");
        }
        (self.0 & (1 << index)) != 0
    }

    /// Checks if all bits up to the given count are set.
    pub fn are_all_set(&self, count: usize) -> bool {
        if count > 64 {
            panic!("ISet can only track up to 64 fields. Count {count} is out of bounds.");
        }
        let mask = (1 << count) - 1;
        self.0 & mask == mask
    }

    /// Checks if any bit in the ISet is set.
    pub fn is_any_set(&self) -> bool {
        self.0 != 0
    }

    /// Clears all bits in the ISet.
    pub fn clear(&mut self) {
        self.0 = 0;
    }
}

/// A type-erased value stored on the heap
pub struct HeapValue<'a> {
    guard: Option<Guard>,
    shape: &'static Shape,
    phantom: PhantomData<&'a ()>,
}

impl Drop for HeapValue<'_> {
    fn drop(&mut self) {
        if let Some(guard) = self.guard.take() {
            if let Some(drop_fn) = self.shape.vtable.drop_in_place {
                unsafe { drop_fn(Opaque::new(guard.ptr)) };
            }
            drop(guard);
        }
    }
}

impl<'a> HeapValue<'a> {
    /// Turn this heapvalue into a concrete type
    pub fn materialize<T: Facet + 'a>(mut self) -> Result<T, ReflectError> {
        if self.shape != T::SHAPE {
            return Err(ReflectError::WrongShape {
                expected: self.shape,
                actual: T::SHAPE,
            });
        }

        let guard = self.guard.take().unwrap();
        let data = OpaqueConst::new(guard.ptr);
        let res = unsafe { data.read::<T>() };
        drop(guard); // free memory (but don't drop in place)
        Ok(res)
    }
}

impl HeapValue<'_> {
    /// Formats the value using its Display implementation, if available
    pub fn fmt_display(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if let Some(display_fn) = self.shape.vtable.display {
            unsafe { display_fn(OpaqueConst::new(self.guard.as_ref().unwrap().ptr), f) }
        } else {
            write!(f, "⟨{}⟩", self.shape)
        }
    }

    /// Formats the value using its Debug implementation, if available
    pub fn fmt_debug(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if let Some(debug_fn) = self.shape.vtable.debug {
            unsafe { debug_fn(OpaqueConst::new(self.guard.as_ref().unwrap().ptr), f) }
        } else {
            write!(f, "⟨{}⟩", self.shape)
        }
    }
}

impl core::fmt::Display for HeapValue<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.fmt_display(f)
    }
}

impl core::fmt::Debug for HeapValue<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.fmt_debug(f)
    }
}

impl PartialEq for HeapValue<'_> {
    fn eq(&self, other: &Self) -> bool {
        if self.shape != other.shape {
            return false;
        }
        if let Some(eq_fn) = self.shape.vtable.eq {
            unsafe {
                eq_fn(
                    OpaqueConst::new(self.guard.as_ref().unwrap().ptr),
                    OpaqueConst::new(other.guard.as_ref().unwrap().ptr),
                )
            }
        } else {
            false
        }
    }
}

impl PartialOrd for HeapValue<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        if self.shape != other.shape {
            return None;
        }
        if let Some(partial_ord_fn) = self.shape.vtable.partial_ord {
            unsafe {
                partial_ord_fn(
                    OpaqueConst::new(self.guard.as_ref().unwrap().ptr),
                    OpaqueConst::new(other.guard.as_ref().unwrap().ptr),
                )
            }
        } else {
            None
        }
    }
}
