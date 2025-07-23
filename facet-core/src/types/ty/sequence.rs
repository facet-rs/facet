use super::Shape;

/// Describes built-in sequence type (array, slice)
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub enum SequenceType {
    /// Array (`[T; N]`)
    Array(ArrayType),

    /// Slice (`[T]`)
    Slice(SliceType),
}

/// Describes a fixed-size array (`[T; N]`)
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct ArrayType {
    /// Shape of the underlying object stored on array
    pub t: &'static Shape,

    /// Constant length of the array
    pub n: usize,
}

/// Describes a slice (`[T]`)
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct SliceType {
    /// Shape of the underlying object stored on slice
    pub t: &'static Shape,
}
