use facet_core::{MapDef, Shape};

use super::{PokeMap, PokeValue, PokeValueUninit};

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

    /// Initializes the map with an optional size hint
    pub fn init(self, size_hint: Option<usize>) -> Result<PokeMap<'mem>, Self> {
        let data = self.value.data;
        let shape = self.value.shape;
        let def = self.def;

        let res = if let Some(capacity) = size_hint {
            let init_in_place_with_capacity = self.def.vtable.init_in_place_with_capacity_fn;
            unsafe { init_in_place_with_capacity(self.value.data, capacity) }.map(|data| {
                PokeValue {
                    data,
                    shape: self.value.shape(),
                }
            })
        } else {
            self.value.default_in_place().map_err(|_| ())
        };

        let data = res.map_err(|_| PokeMapUninit {
            value: PokeValueUninit { data, shape },
            def,
        })?;
        Ok(PokeMap {
            value: data,
            def: self.def,
        })
    }
}
