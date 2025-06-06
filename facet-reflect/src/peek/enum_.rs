use facet_core::{EnumRepr, EnumType, Shape, UserType, Variant};

use crate::{Peek, trace};

use super::{FieldIter, HasFields};

/// Lets you read from an enum (implements read-only enum operations)
#[derive(Clone, Copy)]
pub struct PeekEnum<'mem, 'facet, 'shape> {
    /// The internal data storage for the enum
    ///
    /// Note that this stores both the discriminant and the variant data
    /// (if any), and the layout depends on the enum representation.
    pub(crate) value: Peek<'mem, 'facet, 'shape>,

    /// The definition of the enum.
    pub(crate) ty: EnumType<'shape>,
}

impl core::fmt::Debug for PeekEnum<'_, '_, '_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:?}", self.value)
    }
}

/// Returns the enum definition if the shape represents an enum, None otherwise
pub fn peek_enum<'shape>(shape: &'shape Shape) -> Option<EnumType<'shape>> {
    match shape.ty {
        facet_core::Type::User(UserType::Enum(enum_ty)) => Some(enum_ty),
        _ => None,
    }
}

/// Returns the enum representation if the shape represents an enum, None otherwise
pub fn peek_enum_repr(shape: &Shape) -> Option<EnumRepr> {
    peek_enum(shape).map(|enum_def| enum_def.enum_repr)
}

/// Returns the enum variants if the shape represents an enum, None otherwise
pub fn peek_enum_variants<'shape>(shape: &'shape Shape) -> Option<&'shape [Variant<'shape>]> {
    peek_enum(shape).map(|enum_def| enum_def.variants)
}

impl<'mem, 'facet, 'shape> core::ops::Deref for PeekEnum<'mem, 'facet, 'shape> {
    type Target = Peek<'mem, 'facet, 'shape>;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<'mem, 'facet, 'shape> PeekEnum<'mem, 'facet, 'shape> {
    /// Returns the enum definition
    #[inline(always)]
    pub fn ty(self) -> EnumType<'shape> {
        self.ty
    }

    /// Returns the enum representation
    #[inline(always)]
    pub fn enum_repr(self) -> EnumRepr {
        self.ty.enum_repr
    }

    /// Returns the enum variants
    #[inline(always)]
    pub fn variants(self) -> &'shape [Variant<'shape>] {
        self.ty.variants
    }

    /// Returns the number of variants in this enum
    #[inline(always)]
    pub fn variant_count(self) -> usize {
        self.ty.variants.len()
    }

    /// Returns the variant name at the given index
    #[inline(always)]
    pub fn variant_name(self, index: usize) -> Option<&'shape str> {
        self.ty.variants.get(index).map(|variant| variant.name)
    }

    /// Returns the discriminant value for the current enum value
    #[inline]
    pub fn discriminant(self) -> i64 {
        // Read the discriminant based on the enum representation
        unsafe {
            let data = self
                .value
                .data()
                .thin()
                .expect("discriminant must be Sized");
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
            // Check if enum is all zeros
            let layout = self
                .value
                .shape
                .layout
                .sized_layout()
                .expect("Unsized enums in NPO repr are unsupported");

            let data = self.value.data().thin().unwrap();
            let slice = unsafe { core::slice::from_raw_parts(data.as_byte_ptr(), layout.size()) };
            let all_zero = slice.iter().all(|v| *v == 0);

            trace!(
                "PeekEnum::variant_index (RustNPO): layout size = {}, all_zero = {} (slice is actually {:?})",
                layout.size(),
                all_zero,
                slice
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
                    if all_zero {
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
    pub fn active_variant(self) -> Result<&'shape Variant<'shape>, VariantError> {
        let index = self.variant_index()?;
        Ok(&self.ty.variants[index])
    }

    /// Returns the name of the active variant for this enum value
    #[inline]
    pub fn variant_name_active(self) -> Result<&'shape str, VariantError> {
        Ok(self.active_variant()?.name)
    }

    // variant_data has been removed to reduce unsafe code exposure

    /// Returns a PeekValue handle to a field of a tuple or struct variant by index
    pub fn field(self, index: usize) -> Result<Option<Peek<'mem, 'facet, 'shape>>, VariantError> {
        let variant = self.active_variant()?;
        let fields = &variant.data.fields;

        if index >= fields.len() {
            return Ok(None);
        }

        let field = &fields[index];
        let field_data = unsafe {
            self.value
                .data()
                .thin()
                .ok_or(VariantError::Unsized)?
                .field(field.offset)
        };
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
    ) -> Result<Option<Peek<'mem, 'facet, 'shape>>, VariantError> {
        let index_opt = self.field_index(field_name)?;
        match index_opt {
            Some(index) => self.field(index),
            None => Ok(None),
        }
    }
}

impl<'mem, 'facet, 'shape> HasFields<'mem, 'facet, 'shape> for PeekEnum<'mem, 'facet, 'shape> {
    fn fields(&self) -> FieldIter<'mem, 'facet, 'shape> {
        FieldIter::new_enum(*self)
    }
}

/// Error that can occur when trying to determine variant information
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum VariantError {
    /// Error indicating that enum internals are opaque and cannot be determined
    OpaqueInternals,

    /// Error indicating the enum value is unsized and cannot be accessed by field offset.
    Unsized,
}

impl core::fmt::Display for VariantError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            VariantError::OpaqueInternals => {
                write!(f, "enum layout is opaque, cannot determine variant")
            }
            VariantError::Unsized => {
                write!(
                    f,
                    "enum value is unsized and cannot be accessed by field offset"
                )
            }
        }
    }
}

impl core::fmt::Debug for VariantError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            VariantError::OpaqueInternals => {
                write!(
                    f,
                    "VariantError::OpaqueInternals: enum layout is opaque, cannot determine variant"
                )
            }
            VariantError::Unsized => {
                write!(
                    f,
                    "VariantError::Unsized: enum value is unsized and cannot be accessed by field offset"
                )
            }
        }
    }
}

impl core::error::Error for VariantError {}
