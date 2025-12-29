use facet_core::{Def, EnumRepr, EnumType, Facet, FieldError};

use crate::{ReflectError, peek::VariantError};

use super::Poke;

/// Lets you mutate an enum's fields.
pub struct PokeEnum<'mem, 'facet> {
    /// The internal data storage for the enum
    ///
    /// Note that this stores both the discriminant and the variant data
    /// (if any), and the layout depends on the enum representation.
    pub(crate) value: Poke<'mem, 'facet>,

    /// The definition of the enum.
    pub(crate) ty: EnumType,
}

impl core::fmt::Debug for PokeEnum<'_, '_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:?}", self.value)
    }
}

impl<'mem, 'facet> PokeEnum<'mem, 'facet> {
    /// Returns the enum definition
    #[inline(always)]
    pub fn ty(&self) -> EnumType {
        self.ty
    }

    /// Returns the enum representation
    #[inline(always)]
    pub fn enum_repr(&self) -> EnumRepr {
        self.ty.enum_repr
    }

    /// Returns the enum variants
    #[inline(always)]
    pub fn variants(&self) -> &'static [facet_core::Variant] {
        self.ty.variants
    }

    /// Returns the number of variants in this enum
    #[inline(always)]
    pub fn variant_count(&self) -> usize {
        self.ty.variants.len()
    }

    /// Returns the variant name at the given index
    #[inline(always)]
    pub fn variant_name(&self, index: usize) -> Option<&'static str> {
        self.ty.variants.get(index).map(|variant| variant.name)
    }

    /// Returns the discriminant value for the current enum value
    ///
    /// Note: For `RustNPO` (null pointer optimization) types, there is no explicit
    /// discriminant stored in memory. In this case, 0 is returned. Use
    /// [`variant_index()`](Self::variant_index) to determine the active variant for NPO types.
    #[inline]
    pub fn discriminant(&self) -> i64 {
        // Read the discriminant based on the enum representation
        match self.ty.enum_repr {
            // For RustNPO types, there is no explicit discriminant stored in memory.
            // The variant is determined by niche optimization (e.g., null pointer pattern).
            // Return 0 since that's the declared discriminant for NPO variants.
            // This also prevents UB when reading from zero-sized types.
            EnumRepr::RustNPO => 0,
            EnumRepr::U8 => unsafe { self.value.data().read::<u8>() as i64 },
            EnumRepr::U16 => unsafe { self.value.data().read::<u16>() as i64 },
            EnumRepr::U32 => unsafe { self.value.data().read::<u32>() as i64 },
            EnumRepr::U64 => unsafe { self.value.data().read::<u64>() as i64 },
            EnumRepr::USize => unsafe { self.value.data().read::<usize>() as i64 },
            EnumRepr::I8 => unsafe { self.value.data().read::<i8>() as i64 },
            EnumRepr::I16 => unsafe { self.value.data().read::<i16>() as i64 },
            EnumRepr::I32 => unsafe { self.value.data().read::<i32>() as i64 },
            EnumRepr::I64 => unsafe { self.value.data().read::<i64>() },
            EnumRepr::ISize => unsafe { self.value.data().read::<isize>() as i64 },
        }
    }

    /// Returns the variant index for this enum value
    #[inline]
    pub fn variant_index(&self) -> Result<usize, VariantError> {
        if self.ty.enum_repr == EnumRepr::RustNPO {
            // For Option<T> types with niche optimization, use the OptionVTable
            // to correctly determine if the value is Some or None.
            if let Def::Option(option_def) = self.value.shape.def {
                let is_some = unsafe { (option_def.vtable.is_some)(self.value.data()) };
                return Ok(self
                    .ty
                    .variants
                    .iter()
                    .position(|variant| {
                        let has_fields = !variant.data.fields.is_empty();
                        has_fields == is_some
                    })
                    .expect("No variant found matching Option state"));
            }

            // Fallback for other RustNPO types (e.g., Option<&T> where all-zeros means None)
            let layout = self
                .value
                .shape
                .layout
                .sized_layout()
                .expect("Unsized enums in NPO repr are unsupported");

            let data = self.value.data();
            let slice = unsafe { core::slice::from_raw_parts(data.as_byte_ptr(), layout.size()) };
            let all_zero = slice.iter().all(|v| *v == 0);

            Ok(self
                .ty
                .variants
                .iter()
                .position(|variant| {
                    // Find the maximum end bound
                    let mut max_offset = 0;

                    for field in variant.data.fields {
                        let offset = field.offset
                            + field
                                .shape()
                                .layout
                                .sized_layout()
                                .map(|v| v.size())
                                .unwrap_or(0);
                        max_offset = core::cmp::max(max_offset, offset);
                    }

                    // If we are all zero, then find the enum variant that has no size,
                    // otherwise, the one with size.
                    if all_zero {
                        max_offset == 0
                    } else {
                        max_offset != 0
                    }
                })
                .expect("No variant found with matching discriminant"))
        } else {
            let discriminant = self.discriminant();

            // Find the variant with matching discriminant using position method
            Ok(self
                .ty
                .variants
                .iter()
                .position(|variant| variant.discriminant == Some(discriminant))
                .expect("No variant found with matching discriminant"))
        }
    }

    /// Returns the active variant
    #[inline]
    pub fn active_variant(&self) -> Result<&'static facet_core::Variant, VariantError> {
        let index = self.variant_index()?;
        Ok(&self.ty.variants[index])
    }

    /// Returns the name of the active variant for this enum value
    #[inline]
    pub fn variant_name_active(&self) -> Result<&'static str, VariantError> {
        Ok(self.active_variant()?.name)
    }

    /// Returns a Poke handle to a field of a tuple or struct variant by index
    pub fn field(&mut self, index: usize) -> Result<Option<Poke<'_, 'facet>>, VariantError> {
        let variant = self.active_variant()?;
        let fields = &variant.data.fields;

        if index >= fields.len() {
            return Ok(None);
        }

        let field = &fields[index];
        let field_data = unsafe { self.value.data.field(field.offset) };
        Ok(Some(unsafe {
            Poke::from_raw_parts(field_data, field.shape())
        }))
    }

    /// Returns the index of a field in the active variant by name
    pub fn field_index(&self, field_name: &str) -> Result<Option<usize>, VariantError> {
        let variant = self.active_variant()?;
        Ok(variant
            .data
            .fields
            .iter()
            .position(|f| f.name == field_name))
    }

    /// Returns a Poke handle to a field of a tuple or struct variant by name
    pub fn field_by_name(
        &mut self,
        field_name: &str,
    ) -> Result<Option<Poke<'_, 'facet>>, VariantError> {
        let index_opt = self.field_index(field_name)?;
        match index_opt {
            Some(index) => self.field(index),
            None => Ok(None),
        }
    }

    /// Sets a field of the current variant by index.
    ///
    /// Returns an error if:
    /// - The parent enum is not POD
    /// - The index is out of bounds
    /// - The value type doesn't match the field type
    pub fn set_field<T: Facet<'facet>>(
        &mut self,
        index: usize,
        value: T,
    ) -> Result<(), ReflectError> {
        // Check that the parent enum is POD before allowing field mutation
        if !self.value.shape.is_pod() {
            return Err(ReflectError::NotPod {
                shape: self.value.shape,
            });
        }

        let variant = self
            .active_variant()
            .map_err(|_| ReflectError::OperationFailed {
                shape: self.value.shape,
                operation: "get active variant",
            })?;
        let fields = &variant.data.fields;

        let field = fields.get(index).ok_or(ReflectError::FieldError {
            shape: self.value.shape,
            field_error: FieldError::IndexOutOfBounds {
                index,
                bound: fields.len(),
            },
        })?;

        let field_shape = field.shape();
        if field_shape != T::SHAPE {
            return Err(ReflectError::WrongShape {
                expected: field_shape,
                actual: T::SHAPE,
            });
        }

        unsafe {
            let field_ptr = self.value.data.field(field.offset);
            // Drop the old value and write the new one
            field_shape.call_drop_in_place(field_ptr);
            core::ptr::write(field_ptr.as_mut_byte_ptr() as *mut T, value);
        }

        Ok(())
    }

    /// Sets a field of the current variant by name.
    ///
    /// Returns an error if:
    /// - The parent enum is not POD
    /// - No field with the given name exists
    /// - The value type doesn't match the field type
    pub fn set_field_by_name<T: Facet<'facet>>(
        &mut self,
        name: &str,
        value: T,
    ) -> Result<(), ReflectError> {
        let index = self
            .field_index(name)
            .map_err(|_| ReflectError::OperationFailed {
                shape: self.value.shape,
                operation: "get active variant",
            })?;

        let index = index.ok_or(ReflectError::FieldError {
            shape: self.value.shape,
            field_error: FieldError::NoSuchField,
        })?;

        self.set_field(index, value)
    }

    /// Gets a read-only view of a field by index.
    pub fn peek_field(
        &self,
        index: usize,
    ) -> Result<Option<crate::Peek<'_, 'facet>>, VariantError> {
        let variant = self.active_variant()?;
        let fields = &variant.data.fields;

        if index >= fields.len() {
            return Ok(None);
        }

        let field = &fields[index];
        let field_data = unsafe { self.value.data.as_const().field(field.offset) };
        Ok(Some(unsafe {
            crate::Peek::unchecked_new(field_data, field.shape())
        }))
    }

    /// Gets a read-only view of a field by name.
    pub fn peek_field_by_name(
        &self,
        field_name: &str,
    ) -> Result<Option<crate::Peek<'_, 'facet>>, VariantError> {
        let index_opt = self.field_index(field_name)?;
        match index_opt {
            Some(index) => self.peek_field(index),
            None => Ok(None),
        }
    }

    /// Converts this back into the underlying `Poke`.
    #[inline]
    pub fn into_inner(self) -> Poke<'mem, 'facet> {
        self.value
    }

    /// Returns a read-only `PeekEnum` view.
    #[inline]
    pub fn as_peek_enum(&self) -> crate::PeekEnum<'_, 'facet> {
        crate::PeekEnum {
            value: self.value.as_peek(),
            ty: self.ty,
        }
    }
}
