#[cfg(test)]
mod tests;

mod iset;

use crate::{ReflectError, trace};

use core::{marker::PhantomData, num::NonZeroUsize};

mod heap_value;
use alloc::vec::Vec;
pub use heap_value::*;

use facet_core::{Facet, PtrConst, PtrUninit, Shape, Variant};
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

struct Frame<'shape> {
    /// Address of the value being initialized
    data: PtrUninit<'static>,

    /// Shape of the value being initialized
    shape: &'shape Shape<'shape>,

    /// Tracks initialized fields
    tracker: Tracker<'shape>,
}

enum Tracker<'shape> {
    /// Wholly uninitialized
    Uninit,

    /// Wholly initialized
    Init,

    /// Partially initialized array
    Array {
        /// Some array items are initialized (we only support in-order initialization)
        count: NonZeroUsize,
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
    fn new(data: PtrUninit<'static>, shape: &'shape Shape<'shape>) -> Self {
        Self {
            data,
            shape,
            tracker: Tracker::Uninit,
        }
    }

    /// Returns an error if the value is not fully initialized
    fn require_full_initialization(&self) -> Result<(), ReflectError<'shape>> {
        match self.tracker {
            Tracker::Uninit => Err(ReflectError::UninitializedValue { shape: self.shape }),
            Tracker::Init => Ok(()),
            Tracker::Array { .. } => todo!(),
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
            frames: vec![Frame::new(data, shape)],
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

    /// Puts a value wholesale into the current frame
    pub fn put<T>(&mut self, value: T) -> Result<(), ReflectError<'shape>>
    where
        T: Facet<'shape>,
    {
        // relay to put_shape — convert T into a ptr and shape, and call put_shape
        let ptr_const = PtrConst::new(&raw const value);
        self.put_shape(ptr_const, T::SHAPE)
    }

    /// Puts a value into the current frame by shape, for shape-based operations
    pub fn put_shape(
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
                    self.frames.push(Frame::new(field_ptr, field_shape));

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

        // If in Tracker::Struct and current_child is Some(idx), set the child's iset bit and clear current_child.
        let parent_frame = self.frames.last_mut().unwrap();
        if let Tracker::Struct {
            iset,
            current_child,
        } = &mut parent_frame.tracker
        {
            if let Some(idx) = *current_child {
                iset.set(idx);
                *current_child = None;
            }
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

    /// Puts a value wholesale into the current frame
    pub fn put<U>(&mut self, value: U) -> Result<(), ReflectError<'shape>>
    where
        U: Facet<'shape>,
    {
        self.wip.put(value)
    }

    /// Puts a value into the current frame by shape, for shape-based operations
    pub fn put_shape(
        &mut self,
        src_value: PtrConst<'_>,
        src_shape: &'shape Shape<'shape>,
    ) -> Result<(), ReflectError<'shape>> {
        self.wip.put_shape(src_value, src_shape)
    }

    /// Forwards field_named to the inner wip instance.
    pub fn push_field(&mut self, field_name: &str) -> Result<(), ReflectError<'shape>> {
        self.wip.push_field(field_name)
    }

    /// Forwards push_nth_field to the inner wip instance.
    pub fn push_nth_field(&mut self, idx: usize) -> Result<(), ReflectError<'shape>> {
        self.wip.push_nth_field(idx)
    }

    /// Forwards pop to the inner wip instance.
    pub fn pop(&mut self) -> Result<(), ReflectError<'shape>> {
        self.wip.pop()
    }
}

impl<'facet, 'shape> Drop for Wip<'facet, 'shape> {
    fn drop(&mut self) {
        trace!("🧹 Wip is being dropped");

        // TODO: actually clean
    }
}
