use facet_core::{ListDef, Shape};

use super::{PokeList, PokeValue, PokeValueUninit};

/// Allows initializing an uninitialized list
pub struct PokeListUninit<'mem> {
    pub(crate) value: PokeValueUninit<'mem>,

    pub(crate) def: ListDef,
}

impl<'mem> PokeListUninit<'mem> {
    /// Shape getter
    #[inline(always)]
    pub fn shape(&self) -> &'static Shape {
        self.value.shape()
    }

    /// Returns the list definition.
    #[inline(always)]
    pub fn def(&self) -> &ListDef {
        &self.def
    }

    /// Initializes the list with an optional size hint
    pub fn init(self, size_hint: Option<usize>) -> Result<PokeList<'mem>, Self> {
        if let Some(capacity) = size_hint {
            let init_in_place_with_capacity = self.def.vtable.init_in_place_with_capacity;
            let res = unsafe { init_in_place_with_capacity(self.value.data, capacity) };
            match res {
                Ok(data) => Ok(PokeList {
                    value: PokeValue {
                        data,
                        shape: self.shape(),
                    },
                    def: self.def,
                }),
                Err(_) => Err(self),
            }
        } else {
            match self.value.default_in_place() {
                Ok(val) => Ok(PokeList {
                    value: val,
                    def: self.def,
                }),
                Err(uninit_val) => Err(PokeListUninit {
                    value: uninit_val,
                    def: self.def,
                }),
            }
        }
    }
}
