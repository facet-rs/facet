use facet_core::{EnumDef, Field, Shape};

#[derive(Debug)]
pub enum ReflectError {
    /// Tried to `build` or `build_in_place` a struct/enum without initializing all fields.
    PartiallyInitialized {
        field: Field,
    },

    /// Tried to set an enum to a variant that does not exist
    NoSuchVariant {
        enum_def: EnumDef,
    },

    /// Tried to get the wrong shape out of a value — e.g. we were manipulating
    /// a `String`, but `.get()` was called with a `u64` or something.
    WrongShape {
        expected: &'static Shape,
        actual: &'static Shape,
    },

    WasNotAStruct,
}

impl core::fmt::Display for ReflectError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ReflectError::PartiallyInitialized { field } => {
                write!(
                    f,
                    "Value partially initialized: field {} was not set",
                    field.name
                )
            }
            ReflectError::NoSuchVariant { enum_def } => {
                write!(f, "No such variant in enum. Known variants: ")?;
                for v in enum_def.variants {
                    write!(f, ", {}", v.name)?;
                }
                write!(f, ", that's it.")
            }
            ReflectError::WrongShape { expected, actual } => {
                write!(f, "Wrong shape: expected {}, but got {}", expected, actual)
            }
            ReflectError::WasNotAStruct => write!(f, "Was not a struct"),
        }
    }
}

impl core::error::Error for ReflectError {}
