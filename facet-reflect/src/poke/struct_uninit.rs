use facet_core::{Facet, FieldError, Shape, Struct};

use crate::ReflectError;

use super::slot::Parent;
use super::{HeapVal, ISet, PokeStruct, PokeValue, PokeValueUninit, Slot};

#[cfg(feature = "alloc")]
extern crate alloc;
#[cfg(feature = "alloc")]
use alloc::boxed::Box;

/// Allows gradually initializing a struct (setting fields, etc.)
///
/// This also works for tuples, and tuple structs.
#[derive(Debug)]
pub struct PokeStructUninit<'mem> {
    /// underlying value
    pub(crate) value: PokeValueUninit<'mem>,

    /// fields' shape, etc.
    pub(crate) def: Struct,

    /// tracks initialized fields
    pub(crate) iset: ISet,
}

impl<'mem> PokeStructUninit<'mem> {
    /// Shape getter
    #[inline(always)]
    pub fn shape(&self) -> &'static Shape {
        self.value.shape()
    }

    /// Gets the struct definition
    #[inline(always)]
    pub fn def(&self) -> Struct {
        self.def
    }

    pub(crate) fn assert_can_build(&self) -> Result<(), ReflectError> {
        // are all fields initialized?
        for (i, field) in self.def.fields.iter().copied().enumerate() {
            if !self.iset.has(i) {
                return Err(ReflectError::PartiallyInitialized { field });
            }
        }

        // do we have any invariants to check?
        if let Some(invariants) = self.shape().vtable.invariants {
            let value = unsafe { self.value.data.assume_init().as_const() };
            if !unsafe { invariants(value) } {
                return Err(ReflectError::InvariantViolation);
            }
        }

        Ok(())
    }

    /// Asserts that every field has been initialized and gives a [`PokeStruct`]
    ///
    /// If one of the field was not initialized, all fields will be dropped in place.
    pub fn build_in_place(self) -> Result<PokeStruct<'mem>, ReflectError> {
        self.assert_can_build()?;
        let ps = PokeStruct {
            def: self.def,
            value: PokeValue {
                data: unsafe { self.value.data.assume_init() },
                shape: self.value.shape,
            },
        };
        core::mem::forget(self); // prevent field double-drops
        Ok(ps)
    }

    /// Gets a slot for a given field, by index
    pub fn field(self, index: usize) -> Slot<'mem> {
        if index >= self.def.fields.len() {
            panic!("Index out of bounds");
        }

        let field = self.def.fields[index];
        let value = PokeValueUninit {
            data: unsafe { self.value.data.field_uninit_at(field.offset) },
            shape: field.shape,
        };

        Slot {
            parent: Parent::StructUninit(self),
            value,
            index,
        }
    }

    /// Gets a slot for a given field, by name
    pub fn field_by_name(self, name: &str) -> Result<Slot<'mem>, FieldError> {
        for (index, field) in self.def.fields.iter().enumerate() {
            if field.name == name {
                return Ok(self.field(index));
            }
        }

        Err(FieldError::NoSuchField)
    }
}

impl Drop for PokeStructUninit<'_> {
    fn drop(&mut self) {
        for (i, field) in self.def.fields.iter().enumerate() {
            // for every set field...
            if self.iset.has(i) {
                // that has a drop function...
                if let Some(drop_fn) = field.shape.vtable.drop_in_place {
                    unsafe {
                        // call it
                        drop_fn(self.value.data.field_init(field.offset));
                    }
                }
            }
        }
    }
}

impl<'mem> HeapVal<PokeStructUninit<'mem>> {
    /// Builds a Poke Struct out of this
    pub fn build(self) -> Result<HeapVal<PokeStruct<'mem>>, ReflectError> {
        self.map_res(|this| {
            this.assert_can_build()?;
            let ps = PokeStruct {
                def: this.def,
                value: PokeValue {
                    data: unsafe { this.value.data.assume_init() },
                    shape: this.value.shape,
                },
            };
            core::mem::forget(this);
            Ok(ps)
        })
    }

    /// Builds a value of type `U` out of this
    pub fn materialize<U: Facet>(self) -> Result<U, ReflectError> {
        self.build()?.into_value().materialize::<U>()
    }

    pub fn materialize_boxed<U: Facet>(self) -> Result<Box<U>, ReflectError> {
        self.build()?.into_value().materialize_boxed::<U>()
    }
}
