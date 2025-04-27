/// Describes built-in primitives (u32, bool, str, etc.)
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub enum PrimitiveType {
    /// Boolean (`bool`)
    Boolean,
    /// Numeric (integer/float)
    Numeric(NumericType),
    /// Textual (`char`/`str`)
    Textual(TextualType),
    /// Never type (`!`)
    Never,
}

/// Describes numeric types (integer/float)
///
/// Numeric types have associated `Scalar` `Def`, which includes additional information for the
/// given type.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub enum NumericType {
    /// Integer (`u16`, `i8`, `usize`, etc.)
    Integer(IntegerType),
    /// Floating-point (`f32`, `f64`)
    Float(FloatType),
}

/// Describes textual types (char/string)
///
/// Textual types have associated `Scalar` `Def`, which includes additional information for the
/// given type.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub enum TextualType {
    /// UCS-16 `char` type
    Char,
    /// UTF-8 string (`str`)
    Str,
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct IntegerType {
    pub signed: bool,
    pub bits: usize,
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct FloatType {
    pub bits: usize,
}
