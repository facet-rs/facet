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
            Tracker::Struct { .. } => todo!(),
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
    pub fn put<T>(self, value: T) -> Result<Self, ReflectError<'shape>>
    where
        T: Facet<'shape>,
    {
        // relay to put_shape — convert T into a ptr and shape, and call put_shape
        let ptr_const = PtrConst::new(&raw const value);
        self.put_shape(ptr_const, T::SHAPE)
    }

    /// Puts a value into the current frame by shape, for shape-based operations
    pub fn put_shape(
        mut self,
        src_value: PtrConst<'_>,
        src_shape: &'shape Shape<'shape>,
    ) -> Result<Self, ReflectError<'shape>> {
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
        Ok(self)
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
    pub fn put<U>(self, value: U) -> Result<Self, ReflectError<'shape>>
    where
        U: Facet<'shape>,
    {
        Ok(Self {
            wip: self.wip.put(value)?,
            phantom: PhantomData,
        })
    }

    /// Puts a value into the current frame by shape, for shape-based operations
    pub fn put_shape(
        self,
        src_value: PtrConst<'_>,
        src_shape: &'shape Shape<'shape>,
    ) -> Result<Self, ReflectError<'shape>> {
        Ok(Self {
            wip: self.wip.put_shape(src_value, src_shape)?,
            phantom: PhantomData,
        })
    }
}

impl<'facet, 'shape> Drop for Wip<'facet, 'shape> {
    fn drop(&mut self) {
        trace!("🧹 Wip is being dropped");

        // TODO: actually clean
    }
}
