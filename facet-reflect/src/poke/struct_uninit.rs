use facet_core::{Facet, FieldError, Shape, Struct};

use crate::ReflectError;

use super::slot::Parent;
use super::{Guard, ISet, PokeStruct, PokeValue, PokeValueUninit, Slot};

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

        let data = unsafe { self.value.data.assume_init() };
        let shape = self.value.shape;
        let def = self.def;

        // prevent field drops
        core::mem::forget(self);

        Ok(PokeStruct {
            def,
            value: PokeValue { data, shape },
        })
    }

    /// Builds a value of type `T` from the PokeStruct, then deallocates the memory
    /// that this PokeStruct was pointing to.
    ///
    /// # Panics
    ///
    /// This function will panic if:
    /// - Not all the fields have been initialized.
    /// - The generic type parameter T does not match the shape that this PokeStruct is building.
    pub fn build<T: Facet>(self, guard: Option<Guard>) -> Result<T, ReflectError> {
        // change drop order: we want to drop guard _after_ this.
        let (guard, this) = (guard, self);

        this.shape().assert_type::<T>();
        if let Some(guard) = &guard {
            guard.shape.assert_type::<T>();
        }

        let ps = this.build_in_place()?;
        Ok(unsafe { ps.value.data.read::<T>() })
    }

    /// Build that PokeStruct into a boxed completed shape.
    ///
    /// # Panics
    ///
    /// This function will panic if:
    /// - Not all the fields have been initialized.
    /// - The generic type parameter T does not match the shape that this PokeStruct is building.
    #[cfg(feature = "alloc")]
    pub fn build_boxed<T: Facet>(self) -> Result<Box<T>, ReflectError> {
        self.assert_can_build()?;
        self.shape().assert_type::<T>();

        let boxed = unsafe { Box::from_raw(self.value.data.as_mut_bytes() as *mut T) };
        core::mem::forget(self);
        Ok(boxed)
    }

    pub(crate) unsafe fn field_uninit(
        &self,
        index: usize,
    ) -> Result<PokeValueUninit<'mem>, FieldError> {
        if index >= self.def.fields.len() {
            return Err(FieldError::IndexOutOfBounds);
        }
        let field = &self.def.fields[index];

        Ok(PokeValueUninit {
            data: unsafe { self.value.data.field_uninit_at(field.offset) },
            shape: field.shape,
        })
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
