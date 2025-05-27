#[cfg(test)]
mod tests;

mod iset;

use crate::{ReflectError, trace};

use core::marker::PhantomData;

mod heap_value;
use alloc::vec::Vec;
pub use heap_value::*;

use facet_core::{Facet, PtrConst, PtrMut, PtrUninit, Shape, Variant};
use iset::ISet;

/// A work-in-progress heap-allocated value
///
/// # Lifetimes
///
/// * `'facet`: The lifetime of borrowed values within the structure (or 'static if it's owned)
/// * `'shape`: The lifetime of the Shape structure itself (often 'static)
pub struct Wip<'facet, 'shape> {
    /// stack of frames to keep track of deeply nested initialization
    frames: Vec<Frame<'shape>>,

    /// was this wip poisoned?
    poisoned: bool,

    invariant: PhantomData<fn(&'facet ()) -> &'facet ()>,
}

enum FrameOwnership {
    /// This frame owns the allocation and should deallocate it on drop
    Owned,
    /// This frame is a field pointer into a parent allocation
    Field,
}

struct Frame<'shape> {
    /// Address of the value being initialized
    data: PtrUninit<'static>,

    /// Shape of the value being initialized
    shape: &'shape Shape<'shape>,

    /// Tracks initialized fields
    tracker: Tracker<'shape>,

    /// Whether this frame owns the allocation or is just a field pointer
    ownership: FrameOwnership,
}

enum Tracker<'shape> {
    /// Wholly uninitialized
    Uninit,

    /// Wholly initialized
    Init,

    /// Partially initialized array
    Array {
        /// Track which array elements are initialized (up to 63 elements)
        iset: ISet,
        /// If we're pushing another frame, this is set to the array index
        current_child: Option<usize>,
    },

    /// Partially initialized struct/tuple-struct etc.
    Struct {
        /// fields need to be individually tracked — we only
        /// support up to 63 fields.
        iset: ISet,

        /// if we're pushing another frame, this is set to the
        /// index of the struct field
        current_child: Option<usize>,
    },

    /// Partially initialized enum (but we picked a variant)
    Enum {
        variant: Variant<'shape>,
        data: ISet,
    },
}

impl<'shape> Frame<'shape> {
    fn new(
        data: PtrUninit<'static>,
        shape: &'shape Shape<'shape>,
        ownership: FrameOwnership,
    ) -> Self {
        Self {
            data,
            shape,
            tracker: Tracker::Uninit,
            ownership,
        }
    }

    /// Returns an error if the value is not fully initialized
    fn require_full_initialization(&self) -> Result<(), ReflectError<'shape>> {
        match self.tracker {
            Tracker::Uninit => Err(ReflectError::UninitializedValue { shape: self.shape }),
            Tracker::Init => Ok(()),
            Tracker::Array { iset, .. } => {
                match self.shape.ty {
                    facet_core::Type::Sequence(facet_core::SequenceType::Array(array_def)) => {
                        // Check if all array elements are initialized
                        if (0..array_def.n).all(|idx| iset.get(idx)) {
                            Ok(())
                        } else {
                            Err(ReflectError::UninitializedValue { shape: self.shape })
                        }
                    }
                    _ => Err(ReflectError::UninitializedValue { shape: self.shape }),
                }
            }
            Tracker::Struct { iset, .. } => {
                if iset.all_set() {
                    Ok(())
                } else {
                    // Attempt to find the first uninitialized field, if possible
                    match self.shape.ty {
                        facet_core::Type::User(facet_core::UserType::Struct(struct_type)) => {
                            // Find index of the first bit not set
                            let first_missing_idx =
                                (0..struct_type.fields.len()).find(|&idx| !iset.get(idx));
                            if let Some(missing_idx) = first_missing_idx {
                                let field_name = struct_type.fields[missing_idx].name;
                                Err(ReflectError::UninitializedField {
                                    shape: self.shape,
                                    field_name,
                                })
                            } else {
                                // fallback, something went wrong
                                Err(ReflectError::UninitializedValue { shape: self.shape })
                            }
                        }
                        _ => Err(ReflectError::UninitializedValue { shape: self.shape }),
                    }
                }
            }
            Tracker::Enum { .. } => todo!(),
        }
    }
}

impl<'facet, 'shape> Wip<'facet, 'shape> {
    /// Allocates a new Wip instance with the given shape
    pub fn alloc_shape(shape: &'shape Shape<'shape>) -> Result<Self, ReflectError<'shape>> {
        let data = shape
            .allocate()
            .map_err(|_| ReflectError::Unsized { shape })?;

        Ok(Self {
            frames: vec![Frame::new(data, shape, FrameOwnership::Owned)],
            poisoned: false,
            invariant: PhantomData,
        })
    }

    /// Allocates a new TypedWip instance with the given shape and type
    pub fn alloc<T>() -> Result<TypedWip<'facet, 'shape, T>, ReflectError<'shape>>
    where
        T: Facet<'shape>,
    {
        Ok(TypedWip {
            wip: Self::alloc_shape(T::SHAPE)?,
            phantom: PhantomData,
        })
    }

    /// Sets a value wholesale into the current frame
    pub fn set<T>(&mut self, value: T) -> Result<(), ReflectError<'shape>>
    where
        T: Facet<'shape>,
    {
        // relay to set_shape — convert T into a ptr and shape, and call set_shape
        let ptr_const = PtrConst::new(&raw const value);
        let result = self.set_shape(ptr_const, T::SHAPE);
        // Prevent the value from being dropped since we've copied it
        core::mem::forget(value);
        result
    }

    /// Sets a value into the current frame by shape, for shape-based operations
    pub fn set_shape(
        &mut self,
        src_value: PtrConst<'_>,
        src_shape: &'shape Shape<'shape>,
    ) -> Result<(), ReflectError<'shape>> {
        let fr = self.frames.last_mut().unwrap();

        if !fr.shape.is_shape(src_shape) {
            return Err(ReflectError::WrongShape {
                expected: src_shape,
                actual: fr.shape,
            });
        }

        unsafe {
            fr.data
                .copy_from(src_value, fr.shape)
                .map_err(|_| ReflectError::Unsized { shape: fr.shape })?;
        }

        fr.tracker = Tracker::Init;
        Ok(())
    }

    /// Selects a field of a struct with a given name
    pub fn push_field(&mut self, field_name: &str) -> Result<(), ReflectError<'shape>> {
        let frame = self.frames.last_mut().unwrap();
        match frame.shape.ty {
            facet_core::Type::Primitive(_) => Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "cannot select a field from a primitive type",
            }),
            facet_core::Type::Sequence(_) => Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "cannot select a field from a sequence type",
            }),
            facet_core::Type::User(user_type) => match user_type {
                facet_core::UserType::Struct(struct_type) => {
                    let idx = struct_type.fields.iter().position(|f| f.name == field_name);
                    let idx = match idx {
                        Some(idx) => idx,
                        None => {
                            return Err(ReflectError::OperationFailed {
                                shape: frame.shape,
                                operation: "field not found",
                            });
                        }
                    };
                    self.push_nth_field(idx)
                }
                facet_core::UserType::Enum(_) => {
                    todo!("add support for selecting fields in enums")
                }
                facet_core::UserType::Union(_) => Err(ReflectError::OperationFailed {
                    shape: frame.shape,
                    operation: "unions are not supported yet",
                }),
                facet_core::UserType::Opaque => todo!(),
            },
            facet_core::Type::Pointer(_) => Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "cannot select a field from a pointer type",
            }),
            _ => todo!(),
        }
    }

    /// Selects the nth field of a struct by index
    pub fn push_nth_field(&mut self, idx: usize) -> Result<(), ReflectError<'shape>> {
        let frame = self.frames.last_mut().unwrap();
        match frame.shape.ty {
            facet_core::Type::User(user_type) => match user_type {
                facet_core::UserType::Struct(struct_type) => {
                    if idx >= struct_type.fields.len() {
                        return Err(ReflectError::OperationFailed {
                            shape: frame.shape,
                            operation: "field index out of bounds",
                        });
                    }
                    let field = &struct_type.fields[idx];

                    match &mut frame.tracker {
                        Tracker::Uninit => {
                            frame.tracker = Tracker::Struct {
                                iset: ISet::new(struct_type.fields.len()),
                                current_child: Some(idx),
                            }
                        }
                        Tracker::Struct { current_child, .. } => {
                            *current_child = Some(idx);
                        }
                        _ => unreachable!(),
                    }

                    // Push a new frame for this field onto the frames stack.
                    let field_ptr = unsafe { frame.data.field_uninit_at(field.offset) };
                    let field_shape = field.shape;
                    self.frames
                        .push(Frame::new(field_ptr, field_shape, FrameOwnership::Field));

                    Ok(())
                }
                facet_core::UserType::Enum(_) => {
                    todo!("add support for selecting fields in enums")
                }
                facet_core::UserType::Union(_) => Err(ReflectError::OperationFailed {
                    shape: frame.shape,
                    operation: "unions are not supported yet",
                }),
                facet_core::UserType::Opaque => todo!(),
            },
            _ => Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "cannot select a field from this type",
            }),
        }
    }

    /// Selects the nth element of an array by index
    pub fn push_nth_element(&mut self, idx: usize) -> Result<(), ReflectError<'shape>> {
        let frame = self.frames.last_mut().unwrap();
        match frame.shape.ty {
            facet_core::Type::Sequence(seq_type) => match seq_type {
                facet_core::SequenceType::Array(array_def) => {
                    if idx >= array_def.n {
                        return Err(ReflectError::OperationFailed {
                            shape: frame.shape,
                            operation: "array index out of bounds",
                        });
                    }

                    if array_def.n > 63 {
                        return Err(ReflectError::OperationFailed {
                            shape: frame.shape,
                            operation: "arrays larger than 63 elements are not yet supported",
                        });
                    }

                    // Ensure frame is in Array state
                    if matches!(frame.tracker, Tracker::Uninit) {
                        frame.tracker = Tracker::Array {
                            iset: ISet::default(),
                            current_child: None,
                        };
                    }

                    match &mut frame.tracker {
                        Tracker::Array {
                            iset,
                            current_child,
                        } => {
                            if iset.get(idx) {
                                return Err(ReflectError::OperationFailed {
                                    shape: frame.shape,
                                    operation: "array element already initialized",
                                });
                            }

                            *current_child = Some(idx);

                            // Calculate the offset for this array element
                            let element_layout = array_def
                                .t
                                .layout
                                .sized_layout()
                                .map_err(|_| ReflectError::Unsized { shape: array_def.t })?;
                            let offset = element_layout.size() * idx;

                            // Create a new frame for the array element
                            let element_data = unsafe { frame.data.field_uninit_at(offset) };
                            self.frames.push(Frame::new(
                                element_data,
                                array_def.t,
                                FrameOwnership::Field,
                            ));

                            Ok(())
                        }
                        _ => Err(ReflectError::OperationFailed {
                            shape: frame.shape,
                            operation: "expected array tracker state",
                        }),
                    }
                }
                _ => Err(ReflectError::OperationFailed {
                    shape: frame.shape,
                    operation: "can only select elements from arrays",
                }),
            },
            _ => Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "cannot select an element from this type",
            }),
        }
    }

    /// Pops the current frame off the stack, indicating we're done initializing the current field.
    pub fn pop(&mut self) -> Result<(), ReflectError<'shape>> {
        if self.frames.len() <= 1 {
            // Never pop the last/root frame.
            return Err(ReflectError::InvariantViolation {
                invariant: "Wip::pop() called with only one frame on the stack",
            });
        }

        // Require that the top frame is fully initialized before popping.
        let frame = self.frames.last().unwrap();
        frame.require_full_initialization()?;

        self.frames.pop();

        // Update parent frame's tracking when popping from a child
        let parent_frame = self.frames.last_mut().unwrap();
        match &mut parent_frame.tracker {
            Tracker::Struct {
                iset,
                current_child,
            } => {
                if let Some(idx) = *current_child {
                    iset.set(idx);
                    *current_child = None;
                }
            }
            Tracker::Array {
                iset,
                current_child,
            } => {
                if let Some(idx) = *current_child {
                    iset.set(idx);
                    *current_child = None;
                }
            }
            _ => {}
        }

        Ok(())
    }

    /// Builds the value
    pub fn build(mut self) -> Result<HeapValue<'facet, 'shape>, ReflectError<'shape>> {
        if self.frames.len() != 1 {
            return Err(ReflectError::InvariantViolation {
                invariant: "Wip::build() expects a single frame — pop until that's the case",
            });
        }

        let frame = self.frames.pop().unwrap();
        frame.require_full_initialization()?;

        Ok(HeapValue {
            guard: Some(Guard {
                ptr: frame.data.as_mut_byte_ptr(),
                layout: frame
                    .shape
                    .layout
                    .sized_layout()
                    .map_err(|_| ReflectError::Unsized { shape: frame.shape })?,
            }),
            shape: frame.shape,
            phantom: PhantomData,
        })
    }
}

/// A typed wrapper around `Wip`, for when you want to statically
/// ensure that `build` gives you the proper type.
pub struct TypedWip<'facet, 'shape, T> {
    wip: Wip<'facet, 'shape>,
    phantom: PhantomData<T>,
}
impl<'facet, 'shape, T> TypedWip<'facet, 'shape, T> {
    /// Builds the value and returns a Box<T>
    pub fn build(self) -> Result<Box<T>, ReflectError<'shape>>
    where
        T: Facet<'shape>,
        'facet: 'shape,
    {
        let heap_value = self.wip.build()?;
        // Safety: HeapValue was constructed from T and the shape layout is correct.
        unsafe { Ok(heap_value.into_box_unchecked::<T>()) }
    }

    /// Sets a value wholesale into the current frame
    pub fn set<U>(&mut self, value: U) -> Result<(), ReflectError<'shape>>
    where
        U: Facet<'shape>,
    {
        self.wip.set(value)
    }

    /// Sets a value into the current frame by shape, for shape-based operations
    pub fn set_shape(
        &mut self,
        src_value: PtrConst<'_>,
        src_shape: &'shape Shape<'shape>,
    ) -> Result<(), ReflectError<'shape>> {
        self.wip.set_shape(src_value, src_shape)
    }

    /// Forwards field_named to the inner wip instance.
    pub fn push_field(&mut self, field_name: &str) -> Result<(), ReflectError<'shape>> {
        self.wip.push_field(field_name)
    }

    /// Forwards push_nth_field to the inner wip instance.
    pub fn push_nth_field(&mut self, idx: usize) -> Result<(), ReflectError<'shape>> {
        self.wip.push_nth_field(idx)
    }

    /// Forwards push_nth_element to the inner wip instance.
    pub fn push_nth_element(&mut self, idx: usize) -> Result<(), ReflectError<'shape>> {
        self.wip.push_nth_element(idx)
    }

    /// Forwards pop to the inner wip instance.
    pub fn pop(&mut self) -> Result<(), ReflectError<'shape>> {
        self.wip.pop()
    }
}

impl<'facet, 'shape> Drop for Wip<'facet, 'shape> {
    fn drop(&mut self) {
        trace!("🧹 Wip is being dropped");

        // We need to properly drop all initialized fields
        while let Some(frame) = self.frames.pop() {
            match &frame.tracker {
                Tracker::Uninit => {
                    // Nothing was initialized, nothing to drop
                }
                Tracker::Init => {
                    // Fully initialized, drop it
                    if let Some(drop_fn) = (frame.shape.vtable.drop_in_place)() {
                        unsafe { drop_fn(PtrMut::new(frame.data.as_mut_byte_ptr())) };
                    }
                }
                Tracker::Array { iset, .. } => {
                    // Drop initialized array elements
                    match frame.shape.ty {
                        facet_core::Type::Sequence(facet_core::SequenceType::Array(array_def)) => {
                            let element_layout = array_def.t.layout.sized_layout().ok();
                            if let Some(layout) = element_layout {
                                for idx in 0..array_def.n {
                                    if iset.get(idx) {
                                        let offset = layout.size() * idx;
                                        let element_ptr =
                                            unsafe { frame.data.field_init_at(offset) };
                                        if let Some(drop_fn) = (array_def.t.vtable.drop_in_place)()
                                        {
                                            unsafe { drop_fn(element_ptr) };
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Tracker::Struct { iset, .. } => {
                    // Drop initialized struct fields
                    match frame.shape.ty {
                        facet_core::Type::User(facet_core::UserType::Struct(struct_type)) => {
                            for (idx, field) in struct_type.fields.iter().enumerate() {
                                if iset.get(idx) {
                                    // This field was initialized, drop it
                                    let field_ptr =
                                        unsafe { frame.data.field_init_at(field.offset) };
                                    if let Some(drop_fn) = (field.shape.vtable.drop_in_place)() {
                                        unsafe { drop_fn(field_ptr) };
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Tracker::Enum { variant, data } => {
                    // TODO: Drop initialized enum variant fields
                    let _ = (variant, data);
                }
            }

            // Only deallocate if this frame owns the allocation
            if let FrameOwnership::Owned = frame.ownership {
                if let Ok(layout) = frame.shape.layout.sized_layout() {
                    if layout.size() > 0 {
                        unsafe { alloc::alloc::dealloc(frame.data.as_mut_byte_ptr(), layout) };
                    }
                }
            }
        }
    }
}
