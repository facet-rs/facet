use super::{Field, Shape};

/// Describes built-in sequence type (tuple, array, slice)
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub enum SequenceType {
    /// Tuple (`(T0, T1, ...)`)
    Tuple(TupleType),
    /// Array (`[T; N]`)
    Array(ArrayType),
    /// Slice (`[T]`)
    Slice(SliceType),
}

/// Describes a tuple (`(T0, T1, ...)`)
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct TupleType {
    /// Fields of the slice, with offsets
    pub fields: &'static [Field],
}

/// Describes a fixed-size array (`[T; N]`)
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct ArrayType {
    /// Shape of the underlying object stored on array
    pub t: &'static Shape,
    /// Constatnt length of the array
    pub n: usize,
}

/// Describes a slice (`[T]`)
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct SliceType {
    /// Shape of the underlying object stored on slice
    pub t: &'static Shape,
}
