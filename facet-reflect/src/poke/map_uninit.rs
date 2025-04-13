use facet_core::{MapDef, Shape};

use crate::ReflectError;

use super::{HeapVal, PokeMap, PokeValue, PokeValueUninit};

/// Allows initializing an uninitialized map
pub struct PokeMapUninit<'mem> {
    pub(crate) value: PokeValueUninit<'mem>,
    pub(crate) def: MapDef,
}

impl<'mem> PokeMapUninit<'mem> {
    #[inline(always)]
    /// Shape getter
    pub fn shape(&self) -> &'static Shape {
        self.value.shape()
    }
}

impl<'mem> HeapVal<PokeMapUninit<'mem>> {
    /// Initializes the map with an optional size hint
    pub fn init(self, size_hint: Option<usize>) -> Result<HeapVal<PokeMap<'mem>>, ReflectError> {
        if let Some(capacity) = size_hint {
            let init_in_place_with_capacity = self.def.vtable.init_in_place_with_capacity_fn;
            self.map_res(|this| {
                let data = unsafe { init_in_place_with_capacity(this.value.data, capacity) };
                Ok(PokeMap {
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
                .map(|val| PokeMap { value: val, def }))
        }
    }

    pub fn into_value(self) -> HeapVal<PokeValueUninit<'mem>> {
        self.map(|map| map.value)
    }
}
