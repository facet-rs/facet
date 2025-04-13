use facet_core::{ListDef, Shape};

use crate::ReflectError;

use super::{HeapVal, PokeList, PokeValue, PokeValueUninit};

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
}

impl<'mem> HeapVal<PokeListUninit<'mem>> {
    /// Initializes the list with an optional size hint
    pub fn init(self, size_hint: Option<usize>) -> Result<HeapVal<PokeList<'mem>>, ReflectError> {
        if let Some(capacity) = size_hint {
            let init_in_place_with_capacity = self.def.vtable.init_in_place_with_capacity;
            self.map_res(|this| {
                let data = match init_in_place_with_capacity {
                    Some(init_fn) => unsafe { init_fn(this.value.data, capacity) },
                    None => {
                        return Err(ReflectError::NoDefault {
                            shape: this.shape(),
                        });
                    }
                };
                Ok(PokeList {
                    value: PokeValue {
                        data,
                        shape: this.shape(),
                    },
                    def: this.def,
                })
            })
        } else {
            let def = self.def;
            Ok(self
                .into_value()
                .default_in_place()?
                .map(|val| PokeList { value: val, def }))
        }
    }

    pub fn into_value(self) -> HeapVal<PokeValueUninit<'mem>> {
        self.map(|list| list.value)
    }
}
