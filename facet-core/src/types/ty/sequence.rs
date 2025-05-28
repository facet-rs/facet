use super::Shape;

/// Describes built-in sequence type (array, slice)
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub enum SequenceType<'shape> {
    /// Array (`[T; N]`)
    Array(ArrayType<'shape>),

    /// Slice (`[T]`)
    Slice(SliceType<'shape>),
}

/// Describes a fixed-size array (`[T; N]`)
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct ArrayType<'shape> {
    /// Shape of the underlying object stored on array
    pub t: &'shape Shape<'shape>,

    /// Constant length of the array
    pub n: usize,
}

/// Describes a slice (`[T]`)
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct SliceType<'shape> {
    /// Shape of the underlying object stored on slice
    pub t: &'shape Shape<'shape>,
}
