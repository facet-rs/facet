#[cfg(feature = "alloc")]
extern crate alloc;

use facet_core::{EnumDef, Shape, Variant};

use super::PokeStruct;

/// Allows poking an enum with a selected variant (setting fields, etc.)
pub struct PokeEnum<'mem> {
    /// underlying value
    pub(crate) storage: PokeStruct<'mem>,

    /// definition for this enum
    pub(crate) def: EnumDef,

    /// index of the selected variant
    pub(crate) variant_idx: usize,
}

impl<'mem> PokeEnum<'mem> {
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
