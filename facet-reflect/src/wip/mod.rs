extern crate alloc;
use crate::{ReflectError, ValueId};
use core::{alloc::Layout, marker::PhantomData};
use facet_core::{Def, Facet, FieldError, Opaque, OpaqueConst, OpaqueUninit, Shape, Variant};
use std::collections::HashMap;

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
#[derive(Default)]
struct IState {
    /// Variant chosen — for everything except enums, this stays None
    variant: Option<Variant>,

    /// Fields that were initialized. For scalars, we only track 0
    fields: ISet,
}

/// A work-in-progress heap-allocated value
pub struct Wip<'a> {
    /// frees the memory when dropped
    guard: Guard,

    /// stack of frames to keep track of deeply nested initialization
    frames: alloc::vec::Vec<Frame>,

    /// keeps track of initialization of out-of-tree frames
    istates: HashMap<ValueId, IState>,

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
            guard,
            frames: vec![Frame {
                data,
                shape,
                index: None,
                istate: Default::default(),
            }],
            istates: HashMap::new(),
            phantom: PhantomData,
        }
    }

    /// Allocates a new value of type `S`
    pub fn alloc<S: Facet>() -> Self {
        Self::alloc_shape(S::SHAPE)
    }

    fn pop_inner(&mut self) -> Option<Frame> {
        let frame = self.frames.pop()?;
        if frame.is_fully_initialized() {
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

    /// Asserts everything is initialized
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

        for (id, is) in self.istates.drain() {
            let field_count = match id.shape.def {
                Def::Struct(def) => def.fields.len(),
                Def::Enum(_) => todo!(),
                _ => 1,
            };
            if !is.fields.are_all_set(field_count) {
                match id.shape.def {
                    Def::Struct(sd) => {
                        eprintln!("fields were not initialized for struct {}", id.shape);
                        for (i, field) in sd.fields.iter().enumerate() {
                            if !is.fields.has(i) {
                                eprintln!("  {}", field.name);
                            }
                        }
                    }
                    Def::Enum(_) => {
                        todo!()
                    }
                    Def::Scalar(_) => {
                        eprintln!("fields were not initialized for scalar {}", id.shape);
                    }
                    _ => {}
                }
                panic!("some fields were not initialized")
            }
        }

        let shape = root.shape;
        let _data = unsafe { root.data.assume_init() };

        Ok(HeapValue {
            guard: Some(self.guard),
            shape,
            phantom: PhantomData,
        })
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
    pub fn field_named(mut self, name: &str) -> Result<Self, ReflectError> {
        let frame = self.frames.last_mut().unwrap();
        let shape = frame.shape;
        let Def::Struct(def) = shape.def else {
            return Err(ReflectError::WasNotA { name: "struct" });
        };
        let (index, field) = def
            .fields
            .iter()
            .enumerate()
            .find(|(_, f)| f.name == name)
            .ok_or(ReflectError::FieldError {
                shape,
                field_error: FieldError::NoSuchField,
            })?;
        let field_data = unsafe { frame.data.field_uninit_at(field.offset) };

        let mut frame = Frame {
            data: field_data,
            shape: field.shape,
            index: Some(index),
            istate: Default::default(),
        };
        if let Some(iset) = self.istates.remove(&frame.id()) {
            frame.istate = iset;
        }
        self.frames.push(frame);
        Ok(self)
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
            let Some(parent) = self.frames.get_mut(parent_index) else {
                return Err(ReflectError::OperationFailed {
                    shape,
                    operation: "was supposed to mark a field as initialized, but there was no parent frame",
                });
            };

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
        let Some(frame) = self.pop_inner() else {
            return Err(ReflectError::InvariantViolation);
        };
        self.track(frame);
        Ok(self)
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
