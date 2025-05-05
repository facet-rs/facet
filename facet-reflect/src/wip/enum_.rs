use facet_core::{Def, FieldError, Variant};
#[cfg(feature = "log")]
use owo_colors::OwoColorize;

use crate::trace;
use crate::{ISet, ReflectError, Wip};

impl Wip<'_> {
    /// Selects a variant of an enum by index.
    ///
    /// # Arguments
    ///
    /// * `index` - The index of the variant to select.
    ///
    /// # Returns
    ///
    /// * `Ok(Self)` if the variant was successfully selected.
    /// * `Err(ReflectError)` if the current frame is not an enum or the variant index is out of bounds.
    pub fn variant(mut self, index: usize) -> Result<Self, ReflectError> {
        let frame = self.frames.last_mut().unwrap();
        let shape = frame.shape;
        let Def::Enum(def) = shape.def else {
            return Err(ReflectError::WasNotA {
                expected: "enum",
                actual: shape,
            });
        };

        if index >= def.variants.len() {
            return Err(ReflectError::FieldError {
                shape,
                field_error: FieldError::IndexOutOfBounds {
                    index,
                    bound: def.variants.len(),
                },
            });
        }

        let variant = def.variants[index];

        // Reset the field initialization state since we're selecting a new variant
        ISet::clear(&mut frame.istate.fields);

        // Write the discriminant value based on the enum's representation
        let discriminant = variant.discriminant;
        unsafe {
            let data_ptr = frame.data.as_mut_byte_ptr();
            match def.repr {
                facet_core::EnumRepr::U8 => *data_ptr = discriminant as u8,
                facet_core::EnumRepr::U16 => *(data_ptr as *mut u16) = discriminant as u16,
                facet_core::EnumRepr::U32 => *(data_ptr as *mut u32) = discriminant as u32,
                facet_core::EnumRepr::U64 => *(data_ptr as *mut u64) = discriminant as u64,
                facet_core::EnumRepr::USize => *(data_ptr as *mut usize) = discriminant as usize,
                facet_core::EnumRepr::I8 => *(data_ptr as *mut i8) = discriminant as i8,
                facet_core::EnumRepr::I16 => *(data_ptr as *mut i16) = discriminant as i16,
                facet_core::EnumRepr::I32 => *(data_ptr as *mut i32) = discriminant as i32,
                facet_core::EnumRepr::I64 => *(data_ptr as *mut i64) = discriminant,
                facet_core::EnumRepr::ISize => *(data_ptr as *mut isize) = discriminant as isize,
                _ => {
                    // Default to a reasonable size for other representations
                    *(data_ptr as *mut u32) = discriminant as u32;
                }
            }
        }

        // Now that we've set the discriminant, we can store the variant
        frame.istate.variant = Some(variant);

        trace!(
            "[{}] Selecting variant {} of {} with discriminant {}",
            self.frames.len(),
            variant.name.blue(),
            shape.blue(),
            discriminant
        );

        Ok(self)
    }

    /// Selects a variant of an enum by name.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the variant to select.
    ///
    /// # Returns
    ///
    /// * `Ok(Self)` if the variant was successfully selected.
    /// * `Err(ReflectError)` if the current frame is not an enum or no variant with the given name exists.
    pub fn variant_named(self, name: &str) -> Result<Self, ReflectError> {
        let frame = self.frames.last().unwrap();
        let shape = frame.shape;
        let Def::Enum(def) = shape.def else {
            return Err(ReflectError::WasNotA {
                expected: "enum",
                actual: shape,
            });
        };

        let index =
            def.variants
                .iter()
                .position(|v| v.name == name)
                .ok_or(ReflectError::FieldError {
                    shape,
                    field_error: FieldError::NoSuchField,
                })?;

        self.variant(index)
    }

    /// Finds a variant in an enum by name.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the variant to find.
    ///
    /// # Returns
    ///
    /// * `Some(index, variant)` if a variant with the given name exists.
    /// * `None` if the current frame is not an enum or no variant with the given name exists.
    pub fn find_variant(&self, name: &str) -> Option<(usize, Variant)> {
        let frame = self.frames.last()?;
        if let Def::Enum(def) = frame.shape.def {
            def.variants
                .iter()
                .enumerate()
                .find(|(_, v)| v.name == name)
                .map(|(i, &v)| (i, v))
        } else {
            None
        }
    }

    /// Returns the currently selected variant for the enum in the current frame.
    ///
    /// # Returns
    ///
    /// * `Some(variant)` if the current frame is an enum and a variant has been selected.
    /// * `None` if the current frame is not an enum or no variant has been selected yet.
    pub fn selected_variant(&self) -> Option<Variant> {
        let frame = self.frames.last()?;
        frame.istate.variant
    }
}
