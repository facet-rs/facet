use facet_core::{EnumRepr, EnumType, Field, Shape, UserType, Variant};

use crate::{Peek, trace};

use super::HasFields;

/// Lets you read from an enum (implements read-only enum operations)
#[derive(Clone, Copy)]
pub struct PeekEnum<'mem, 'facet_lifetime> {
    /// The internal data storage for the enum
    ///
    /// Note that this stores both the discriminant and the variant data
    /// (if any), and the layout depends on the enum representation.
    pub(crate) value: Peek<'mem, 'facet_lifetime>,

    /// The definition of the enum.
    pub(crate) ty: EnumType,
}

impl core::fmt::Debug for PeekEnum<'_, '_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if let Some(debug_fn) = self.vtable().debug {
            unsafe { debug_fn(self.data, f) }
        } else {
            write!(f, "⟨{}⟩", self.shape)
        }
    }
}

/// Returns the enum definition if the shape represents an enum, None otherwise
pub fn peek_enum(shape: &'static Shape) -> Option<EnumType> {
    match shape.ty {
        facet_core::Type::User(UserType::Enum(enum_ty)) => Some(enum_ty),
        _ => None,
    }
}

/// Returns the enum representation if the shape represents an enum, None otherwise
pub fn peek_enum_repr(shape: &'static Shape) -> Option<EnumRepr> {
    peek_enum(shape).map(|enum_def| enum_def.enum_repr)
}

/// Returns the enum variants if the shape represents an enum, None otherwise
pub fn peek_enum_variants(shape: &'static Shape) -> Option<&'static [Variant]> {
    peek_enum(shape).map(|enum_def| enum_def.variants)
}

impl<'mem, 'facet_lifetime> core::ops::Deref for PeekEnum<'mem, 'facet_lifetime> {
    type Target = Peek<'mem, 'facet_lifetime>;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<'mem, 'facet_lifetime> PeekEnum<'mem, 'facet_lifetime> {
    /// Returns the enum definition
    #[inline(always)]
    pub fn ty(self) -> EnumType {
        self.ty
    }

    /// Returns the enum representation
    #[inline(always)]
    pub fn enum_repr(self) -> EnumRepr {
        self.ty.enum_repr
    }

    /// Returns the enum variants
    #[inline(always)]
    pub fn variants(self) -> &'static [Variant] {
        self.ty.variants
    }

    /// Returns the number of variants in this enum
    #[inline(always)]
    pub fn variant_count(self) -> usize {
        self.ty.variants.len()
    }

    /// Returns the variant name at the given index
    #[inline(always)]
    pub fn variant_name(self, index: usize) -> Option<&'static str> {
        self.ty.variants.get(index).map(|variant| variant.name)
    }

    /// Returns the discriminant value for the current enum value
    #[inline]
    pub fn discriminant(self) -> i64 {
        // Read the discriminant based on the enum representation
        unsafe {
            let data = self.value.data();
            match self.ty.enum_repr {
                EnumRepr::U8 => data.read::<u8>() as i64,
                EnumRepr::U16 => data.read::<u16>() as i64,
                EnumRepr::U32 => data.read::<u32>() as i64,
                EnumRepr::U64 => data.read::<u64>() as i64,
                EnumRepr::USize => data.read::<usize>() as i64,
                EnumRepr::I8 => data.read::<i8>() as i64,
                EnumRepr::I16 => data.read::<i16>() as i64,
                EnumRepr::I32 => data.read::<i32>() as i64,
                EnumRepr::I64 => data.read::<i64>(),
                EnumRepr::ISize => data.read::<isize>() as i64,
                _ => {
                    // Default to a reasonable size for other representations that might be added in the future
                    data.read::<u32>() as i64
                }
            }
        }
    }

    /// Returns the variant index for this enum value
    #[inline]
    pub fn variant_index(self) -> Result<usize, VariantError> {
        if self.ty.enum_repr == EnumRepr::RustNPO {
            let data = self.value.data();
            let ptr_value = unsafe { data.read::<usize>() & 0x0fff_ffff };
            let niche_zero = ptr_value == 0;

            trace!(
                "PeekEnum::variant_index (RustNPO): niche_zero = {} (masked ptr_value is 0x{:x})",
                niche_zero, ptr_value
            );

            Ok(self
                .ty
                .variants
                .iter()
                .enumerate()
                .position(|#[allow(unused)] (variant_idx, variant)| {
                    // Find the maximum end bound
                    let mut max_offset = 0;

                    for field in variant.data.fields {
                        let offset = field.offset
                            + field
                                .shape
                                .layout
                                .sized_layout()
                                .map(|v| v.size())
                                .unwrap_or(0);
                        max_offset = core::cmp::max(max_offset, offset);
                    }

                    trace!(
                        "  variant[{}] = '{}', max_offset = {}",
                        variant_idx, variant.name, max_offset
                    );

                    // If we are all zero, then find the enum variant that has no size,
                    // otherwise, the one with size.
                    if niche_zero {
                        max_offset == 0
                    } else {
                        max_offset != 0
                    }
                })
                .expect("No variant found with matching discriminant"))
        } else {
            let discriminant = self.discriminant();

            trace!(
                "PeekEnum::variant_index: discriminant = {} (repr = {:?})",
                discriminant, self.ty.enum_repr
            );

            // Find the variant with matching discriminant using position method
            Ok(self
                .ty
                .variants
                .iter()
                .enumerate()
                .position(|#[allow(unused)] (variant_idx, variant)| {
                    variant.discriminant == Some(discriminant)
                })
                .expect("No variant found with matching discriminant"))
        }
    }

    /// Returns the active variant
    #[inline]
    pub fn active_variant(self) -> Result<&'static Variant, VariantError> {
        let index = self.variant_index()?;
        Ok(&self.ty.variants[index])
    }

    /// Returns the name of the active variant for this enum value
    #[inline]
    pub fn variant_name_active(self) -> Result<&'static str, VariantError> {
        Ok(self.active_variant()?.name)
    }

    // variant_data has been removed to reduce unsafe code exposure

    /// Returns a PeekValue handle to a field of a tuple or struct variant by index
    pub fn field(self, index: usize) -> Result<Option<Peek<'mem, 'facet_lifetime>>, VariantError> {
        let variant = self.active_variant()?;
        let fields = &variant.data.fields;

        if index >= fields.len() {
            return Ok(None);
        }

        let field = &fields[index];
        let field_data = unsafe { self.value.data().field(field.offset) };
        Ok(Some(unsafe {
            Peek::unchecked_new(field_data, field.shape())
        }))
    }

    /// Returns the index of a field in the active variant by name
    pub fn field_index(self, field_name: &str) -> Result<Option<usize>, VariantError> {
        let variant = self.active_variant()?;
        Ok(variant
            .data
            .fields
            .iter()
            .position(|f| f.name == field_name))
    }

    /// Returns a PeekValue handle to a field of a tuple or struct variant by name
    pub fn field_by_name(
        self,
        field_name: &str,
    ) -> Result<Option<Peek<'mem, 'facet_lifetime>>, VariantError> {
        let index_opt = self.field_index(field_name)?;
        match index_opt {
            Some(index) => self.field(index),
            None => Ok(None),
        }
    }
}

impl<'mem, 'facet_lifetime> HasFields<'mem, 'facet_lifetime> for PeekEnum<'mem, 'facet_lifetime> {
    fn fields(&self) -> impl DoubleEndedIterator<Item = (Field, Peek<'mem, 'facet_lifetime>)> {
        // Get the active variant and its fields
        let variant = match self.active_variant() {
            Ok(v) => v,
            Err(e) => panic!("Cannot get active variant: {:?}", e),
        };
        let fields = &variant.data.fields;

        // Create an iterator that yields the field definition and field value
        (0..fields.len()).filter_map(move |i| {
            // Get the field definition
            let field = fields[i];
            // Get the field value
            let field_value = match self.field(i) {
                Ok(Some(v)) => v,
                Ok(None) => return None,
                Err(e) => panic!("Cannot get field: {:?}", e),
            };
            // Return the field definition and value
            Some((field, field_value))
        })
    }
}

/// Error that can occur when trying to determine variant information
pub enum VariantError {
    /// Error indicating that enum internals are opaque and cannot be determined
    OpaqueInternals,
}

impl core::fmt::Display for VariantError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "enum layout is opaque, cannot determine variant")
    }
}

impl core::fmt::Debug for VariantError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "VariantError::OpaqueInternals: enum layout is opaque, cannot determine variant"
        )
    }
}

impl core::error::Error for VariantError {}
