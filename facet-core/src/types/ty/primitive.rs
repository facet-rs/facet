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
    ///
    /// Number of bits can be found by checking the size of the shape's layout.
    Integer {
        /// Is this a signed integer (`i`) or unsigned (`u`)?
        signed: bool,
    },
    /// Floating-point (`f32`, `f64`)
    ///
    /// Number of bits can be found by checking the size of the shape's layout.
    Float,
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
