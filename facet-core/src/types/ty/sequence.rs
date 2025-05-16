use super::{Field, Shape};

/// Describes built-in sequence type (tuple, array, slice)
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub enum SequenceType<'shape> {
    /// Tuple (`(T0, T1, ...)`)
    Tuple(TupleType<'shape>),

    /// Array (`[T; N]`)
    Array(ArrayType<'shape>),

    /// Slice (`[T]`)
    Slice(SliceType<'shape>),
}

/// Describes a tuple (`(T0, T1, ...)`)
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct TupleType<'shape> {
    /// Fields of the slice, with offsets
    pub fields: &'shape [Field<'shape>],
}

/// Describes a fixed-size array (`[T; N]`)
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct ArrayType<'shape> {
    /// Shape of the underlying object stored on array
    pub t: &'shape Shape<'shape>,

    /// Constatnt length of the array
    pub n: usize,
}

/// Describes a slice (`[T]`)
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct SliceType<'shape> {
    /// Shape of the underlying object stored on slice
    pub t: &'shape Shape<'shape>,
}
