use facet_core::{Facet, FieldError, Fields, Shape, Struct};

use crate::ReflectError;

use super::{Guard, ISet, PokeStruct, PokeValue, PokeValueUninit, Slot};

#[cfg(feature = "alloc")]
extern crate alloc;
#[cfg(feature = "alloc")]
use alloc::boxed::Box;

/// Allows gradually initializing a struct (setting fields, etc.)
///
/// This also works for tuples, and tuple structs.
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

    pub(crate) fn assert_all_fields_initialized(&self) -> Result<(), ReflectError> {
        for (i, field) in self.def.fields.iter().copied().enumerate() {
            if !self.iset.has(i) {
                return Err(ReflectError::PartiallyInitialized { field });
            }
        }
        Ok(())
    }

    /// Asserts that every field has been initialized and gives a [`PokeStruct`]
    ///
    /// If one of the field was not initialized, all fields will be dropped in place.
    pub fn build_in_place(self) -> Result<PokeStruct<'mem>, ReflectError> {
        self.assert_all_fields_initialized()?;

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
        let (mut guard, this) = (guard, self);

        this.shape().assert_type::<T>();
        if let Some(guard) = &guard {
            guard.shape.assert_type::<T>();
        }

        let ps = this.build_in_place()?;
        let t = unsafe { ps.value.data.read::<T>() };
        ps.value.data();
        Ok(t)
    }

    /// Build that PokeStruct into a boxed completed shape.
    ///
    /// # Panics
    ///
    /// This function will panic if:
    /// - Not all the fields have been initialized.
    /// - The generic type parameter T does not match the shape that this PokeStruct is building.
    #[cfg(feature = "alloc")]
    pub fn build_boxed<T: Facet>(self) -> Box<T> {
        self.assert_all_fields_initialized();
        self.shape().assert_type::<T>();

        let boxed = unsafe { Box::from_raw(self.value.data.as_mut_bytes() as *mut T) };
        core::mem::forget(self);
        boxed
    }

    pub(crate) unsafe fn field_uninit_by_name(
        &self,
        name: &str,
    ) -> Result<(usize, PokeValueUninit<'mem>), FieldError> {
        let index = self
            .def
            .fields
            .iter()
            .position(|f| f.name == name)
            .ok_or(FieldError::NoSuchField)?;
        Ok((index, self.field(index)?))
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

        Slot {
            parent: Parent::StructUninit(self),
            value: self.value,
            field,
            index,
        }
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
