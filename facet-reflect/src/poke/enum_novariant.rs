use facet_core::{EnumDef, EnumRepr, FieldError, Shape, Variant};

use super::{PokeEnumUninit, PokeValueUninit};

/// Represents an enum before a variant has been selected
pub struct PokeEnumNoVariant<'mem> {
    /// underlying value
    pub(crate) value: PokeValueUninit<'mem>,

    /// definition for this enum
    pub(crate) def: EnumDef,
}

impl<'mem> PokeEnumNoVariant<'mem> {
    /// Shape getter
    #[inline(always)]
    pub fn shape(&self) -> &'static Shape {
        self.value.shape
    }

    /// Enum definition getter
    #[inline(always)]
    pub fn def(&self) -> &EnumDef {
        &self.def
    }

    /// Sets the variant of an enum by name.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    ///
    /// - No variant with the given name exists.
    pub fn set_variant_by_name(
        self,
        variant_name: &str,
    ) -> Result<PokeEnumUninit<'mem>, FieldError> {
        let variant_index = self
            .def
            .variants
            .iter()
            .enumerate()
            .find(|(_, v)| v.name == variant_name)
            .map(|(i, _)| i)
            .ok_or(FieldError::NoSuchField)?;

        self.set_variant_by_index(variant_index)
    }

    /// Sets the variant of an enum by index.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The index is out of bounds.
    pub fn set_variant_by_index(
        self,
        variant_idx: usize,
    ) -> Result<PokeEnumUninit<'mem>, FieldError> {
        if variant_idx >= self.def.variants.len() {
            return Err(FieldError::IndexOutOfBounds);
        }

        // Get the current variant info
        let variant = &self.def.variants[variant_idx];

        // Prepare memory for the enum
        unsafe {
            // Zero out the memory first to ensure clean state
            core::ptr::write_bytes(
                self.value.data.as_mut_byte_ptr(),
                0,
                self.shape().layout.size(),
            );

            // Set up the discriminant (tag)
            // For enums in Rust, the first bytes contain the discriminant
            let discriminant_value = variant.discriminant;
            let ptr = self.value.data.as_mut_byte_ptr();

            // Write the discriminant value based on the representation
            match self.def.repr {
                EnumRepr::U8 => {
                    let tag_ptr = ptr;
                    *tag_ptr = discriminant_value as u8;
                }
                EnumRepr::U16 => {
                    let tag_ptr = ptr as *mut u16;
                    *tag_ptr = discriminant_value as u16;
                }
                EnumRepr::U32 => {
                    let tag_ptr = ptr as *mut u32;
                    *tag_ptr = discriminant_value as u32;
                }
                EnumRepr::U64 => {
                    let tag_ptr = ptr as *mut u64;
                    *tag_ptr = discriminant_value as u64;
                }
                EnumRepr::USize => {
                    let tag_ptr = ptr as *mut usize;
                    *tag_ptr = discriminant_value as usize;
                }
                EnumRepr::I8 => {
                    let tag_ptr = ptr as *mut i8;
                    *tag_ptr = discriminant_value as i8;
                }
                EnumRepr::I16 => {
                    let tag_ptr = ptr as *mut i16;
                    *tag_ptr = discriminant_value as i16;
                }
                EnumRepr::I32 => {
                    let tag_ptr = ptr as *mut i32;
                    *tag_ptr = discriminant_value as i32;
                }
                EnumRepr::I64 => {
                    let tag_ptr = ptr as *mut i64;
                    *tag_ptr = discriminant_value;
                }
                EnumRepr::ISize => {
                    let tag_ptr = ptr as *mut isize;
                    *tag_ptr = discriminant_value as isize;
                }
                _ => {
                    panic!("Unsupported enum representation: {:?}", self.def.repr);
                }
            }
        }

        // Create PokeEnum with the selected variant
        Ok(PokeEnumUninit {
            def: self.def,
            variant_idx,
            storage: super::PokeStructUninit {
                value: self.value,
                def: self.def.variants[variant_idx].data,
                iset: Default::default(),
            },
        })
    }

    /// Try to find the variant by name.
    pub fn variant_by_name(&self, name: &str) -> Option<&Variant> {
        self.def
            .variants
            .iter()
            .find(|variant| variant.name == name)
    }
}
