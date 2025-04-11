use core::ptr::NonNull;
use facet_core::{FieldError, Opaque, OpaqueConst, OpaqueUninit, Shape, StructDef};

use super::{Guard, ISet, PokeValueUninit};

/// Allows poking a struct (setting fields, etc.)
pub struct PokeStruct<'mem> {
    data: OpaqueUninit<'mem>,
    shape: &'static Shape,
    def: StructDef,
    iset: ISet,
}

impl<'mem> PokeStruct<'mem> {
    #[inline(always)]
    /// Coerce back into a `PokeValue`
    pub fn into_value(self) -> PokeValueUninit<'mem> {
        unsafe { PokeValueUninit::new(self.data, self.shape) }
    }

    /// Shape getter
    #[inline(always)]
    pub fn shape(&self) -> &'static Shape {
        self.shape
    }
    /// Creates a new PokeStruct
    ///
    /// # Safety
    ///
    /// The `data`, `shape`, and `def` must match
    pub unsafe fn new(data: OpaqueUninit<'mem>, shape: &'static Shape, def: StructDef) -> Self {
        Self {
            data,
            iset: Default::default(),
            shape,
            def,
        }
    }

    /// Checks if all fields in the struct have been initialized.
    /// Panics if any field is not initialized, providing details about the uninitialized field.
    pub fn assert_all_fields_initialized(&self) {
        for (i, field) in self.def.fields.iter().enumerate() {
            if !self.iset.has(i) {
                panic!(
                    "Field '{}' was not initialized. Complete schema:\n{:?}",
                    field.name, self.shape
                );
            }
        }
    }

    /// Asserts that every field has been initialized and forgets the PokeStruct.
    ///
    /// This method is only used when the origin is borrowed.
    /// If this method is not called, all fields will be freed when the PokeStruct is dropped.
    ///
    /// # Panics
    ///
    /// This function will panic if any field is not initialized.
    pub fn build_in_place(self) -> Opaque<'mem> {
        // ensure all fields are initialized
        self.assert_all_fields_initialized();

        let data = unsafe { self.data.assume_init() };

        // prevent field drops when the PokeStruct is dropped
        core::mem::forget(self);

        data
    }

    /// Builds a value of type `T` from the PokeStruct, then deallocates the memory
    /// that this PokeStruct was pointing to.
    ///
    /// # Panics
    ///
    /// This function will panic if:
    /// - Not all the fields have been initialized.
    /// - The generic type parameter T does not match the shape that this PokeStruct is building.
    pub fn build<T: crate::Facet>(self, guard: Option<Guard>) -> T {
        let mut guard = guard;
        let this = self;
        // this changes drop order: guard must be dropped _after_ this.

        this.assert_all_fields_initialized();
        this.shape.assert_type::<T>();
        if let Some(guard) = &guard {
            guard.shape.assert_type::<T>();
        }

        let result = unsafe {
            let ptr = this.data.as_mut_bytes() as *const T;
            core::ptr::read(ptr)
        };
        guard.take(); // dealloc
        core::mem::forget(this);
        result
    }

    /// Build that PokeStruct into a boxed completed shape.
    ///
    /// # Panics
    ///
    /// This function will panic if:
    /// - Not all the fields have been initialized.
    /// - The generic type parameter T does not match the shape that this PokeStruct is building.
    pub fn build_boxed<T: crate::Facet>(self) -> alloc::boxed::Box<T> {
        self.assert_all_fields_initialized();
        self.shape.assert_type::<T>();

        let boxed = unsafe { alloc::boxed::Box::from_raw(self.data.as_mut_bytes() as *mut T) };
        core::mem::forget(self);
        boxed
    }

    /// Moves the contents of this `PokeStruct` into a target memory location.
    ///
    /// # Safety
    ///
    /// The target pointer must be valid and properly aligned,
    /// and must be large enough to hold the value.
    /// The caller is responsible for ensuring that the target memory is properly deallocated
    /// when it's no longer needed.
    pub unsafe fn move_into(self, target: NonNull<u8>, guard: Option<Guard>) {
        self.assert_all_fields_initialized();
        if let Some(guard) = &guard {
            guard.shape.assert_shape(self.shape);
        }

        unsafe {
            core::ptr::copy_nonoverlapping(
                self.data.as_mut_bytes(),
                target.as_ptr(),
                self.shape.layout.size(),
            );
        }
        core::mem::forget(self);
    }

    /// Gets a field, by name
    pub fn field_by_name(
        &self,
        name: &str,
    ) -> Result<(usize, crate::PokeUninit<'mem>), FieldError> {
        let index = self
            .def
            .fields
            .iter()
            .position(|f| f.name == name)
            .ok_or(FieldError::NoSuchStaticField)?;
        Ok((index, self.field(index)?))
    }

    /// Get a field writer for a field by index.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The shape doesn't represent a struct.
    /// - The index is out of bounds.
    pub fn field(&self, index: usize) -> Result<crate::PokeUninit<'mem>, FieldError> {
        if index >= self.def.fields.len() {
            return Err(FieldError::IndexOutOfBounds);
        }

        let field = &self.def.fields[index];

        // Get the field's address
        let field_addr = unsafe { self.data.field_uninit(field.offset) };
        let field_shape = field.shape;

        let poke = unsafe { crate::PokeUninit::unchecked_new(field_addr, field_shape) };
        Ok(poke)
    }

    /// Sets a field's value by its index, directly copying raw memory.
    ///
    /// # Safety
    ///
    /// This is unsafe because it directly copies memory without checking types.
    /// The caller must ensure that `value` is of the correct type for the field.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The index is out of bounds
    /// - The field shapes don't match
    pub unsafe fn unchecked_set(
        &mut self,
        index: usize,
        value: OpaqueConst,
    ) -> Result<(), FieldError> {
        if index >= self.def.fields.len() {
            return Err(FieldError::IndexOutOfBounds);
        }
        let field = &self.def.fields[index];
        let field_shape = field.shape;

        unsafe {
            core::ptr::copy_nonoverlapping(
                value.as_ptr(),
                self.data.field_uninit(field.offset).as_mut_bytes(),
                field_shape.layout.size(),
            );
            self.iset.set(index);
        }

        Ok(())
    }

    /// Sets a field's value by its name, directly copying raw memory.
    ///
    /// # Safety
    ///
    /// This is unsafe because it directly copies memory without checking types.
    /// The caller must ensure that `value` is of the correct type for the field.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The field name doesn't exist
    /// - The field shapes don't match
    pub unsafe fn unchecked_set_by_name(
        &mut self,
        name: &str,
        value: OpaqueConst,
    ) -> Result<(), FieldError> {
        let index = self
            .def
            .fields
            .iter()
            .position(|f| f.name == name)
            .ok_or(FieldError::NoSuchStaticField)?;
        unsafe { self.unchecked_set(index, value) }
    }

    /// Sets a field's value by its index in a type-safe manner.
    ///
    /// This method takes ownership of the value and ensures proper memory management.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The index is out of bounds
    /// - The field shapes don't match
    pub fn set<T: crate::Facet>(&mut self, index: usize, value: T) -> Result<(), FieldError> {
        let field_shape = self
            .def
            .fields
            .get(index)
            .ok_or(FieldError::IndexOutOfBounds)?
            .shape;
        if !field_shape.is_type::<T>() {
            return Err(FieldError::TypeMismatch);
        }

        unsafe {
            let opaque = OpaqueConst::new(&value);
            let result = self.unchecked_set(index, opaque);
            if result.is_ok() {
                core::mem::forget(value);
            }
            result
        }
    }

    /// Sets a field's value by its name in a type-safe manner.
    ///
    /// This method takes ownership of the value and ensures proper memory management.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The field name doesn't exist
    /// - The field shapes don't match
    pub fn set_by_name<T: crate::Facet>(&mut self, name: &str, value: T) -> Result<(), FieldError> {
        let index = self
            .def
            .fields
            .iter()
            .position(|f| f.name == name)
            .ok_or(FieldError::NoSuchStaticField)?;

        self.set(index, value)
    }

    /// Marks a field as initialized.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the field is initialized. Only call this after writing to
    /// an address gotten through [`Self::field`] or [`Self::field_by_name`].
    pub unsafe fn mark_initialized(&mut self, index: usize) {
        self.iset.set(index);
    }

    /// Gets the struct definition
    pub fn def(&self) -> StructDef {
        self.def
    }
}

impl Drop for PokeStruct<'_> {
    fn drop(&mut self) {
        self.def
            .fields
            .iter()
            .enumerate()
            .filter_map(|(i, field)| {
                if self.iset.has(i) {
                    Some((field, field.shape.vtable.drop_in_place?))
                } else {
                    None
                }
            })
            .for_each(|(field, drop_fn)| unsafe {
                drop_fn(self.data.field_init(field.offset));
            });
    }
}
