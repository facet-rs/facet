use facet_core::{EnumDef, Shape, Variant};

use super::PokeStructUninit;

/// Allows poking an enum with a selected variant (setting fields, etc.)
pub struct PokeEnumUninit<'mem> {
    /// underlying value
    pub(crate) storage: PokeStructUninit<'mem>,

    /// definition for this enum
    pub(crate) def: EnumDef,

    /// index of the selected variant
    pub(crate) variant_idx: usize,
}

impl<'mem> PokeEnumUninit<'mem> {
    /// Shape getter
    #[inline(always)]
    pub fn shape(&self) -> &'static Shape {
        self.storage.shape()
    }

    /// Enum definition getter
    #[inline(always)]
    pub fn def(&self) -> &EnumDef {
        &self.def
    }

    /// Returns the currently selected variant index
    pub fn variant(&self) -> Variant {
        self.def.variants[self.variant_idx]
    }
}

/// All possible errors when getting a variant by index or by name
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum VariantError {
    /// `variant_by_index` was called with an index that is out of bounds.
    IndexOutOfBounds,

    /// `variant_by_name` or `variant_by_index` was called on a non-enum type.
    NotAnEnum,

    /// `variant_by_name` was called with a name that doesn't match any variant.
    NoSuchVariant,
}

impl core::error::Error for VariantError {}

impl core::fmt::Display for VariantError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            VariantError::IndexOutOfBounds => write!(f, "Variant index out of bounds"),
            VariantError::NotAnEnum => write!(f, "Not an enum"),
            VariantError::NoSuchVariant => write!(f, "No such variant"),
        }
    }
}
