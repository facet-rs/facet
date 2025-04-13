use facet_core::{OpaqueUninit, Shape, Struct};

use super::{ISet, PokeStructUninit, PokeValue, PokeValueUninit};

/// Allows mutating a fully-initialized struct
pub struct PokeStruct<'mem> {
    /// pointer to the partially-initialized struct
    pub(crate) value: PokeValue<'mem>,

    /// field list, with offsets and shapes
    pub(crate) def: Struct,
}

impl<'mem> PokeStruct<'mem> {
    /// Shape getter
    #[inline(always)]
    pub fn shape(&self) -> &'static Shape {
        self.value.shape()
    }

    /// Gets the struct definition
    pub fn def(&self) -> Struct {
        self.def
    }

    /// Coerce back into a value
    #[inline(always)]
    pub fn into_value(self) -> PokeValue<'mem> {
        self.value
    }

    /// Coerce back into a partially-initialized struct
    ///
    /// This will allow mutating fields, and the invariants can then be re-checked
    /// before going back to a fully-initialized struct
    pub fn into_uninit(self) -> PokeStructUninit<'mem> {
        PokeStructUninit {
            value: PokeValueUninit {
                data: OpaqueUninit::new(self.value.data.as_mut_byte_ptr()),
                shape: self.value.shape,
            },
            def: self.def,
            iset: ISet::all(self.def.fields),
        }
    }
}
