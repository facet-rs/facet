use facet_core::{ListDef, ListVTable, Shape};

use super::PokeValue;

/// Allows poking a list (appending, etc.)
pub struct PokeList<'mem> {
    /// underlying data
    pub(crate) value: PokeValue<'mem>,

    pub(crate) def: ListDef,
}

impl<'mem> PokeList<'mem> {
    #[inline(always)]
    /// Shape getter
    pub fn shape(&self) -> &'static Shape {
        self.value.shape()
    }

    /// Gets the vtable for the list
    #[inline(always)]
    fn list_vtable(&self) -> &'static ListVTable {
        self.def.vtable
    }

    /// Gets the def for that list
    #[inline(always)]
    pub fn def(&self) -> &ListDef {
        &self.def
    }

    // TODO: more
}
