use facet_core::{Def, EnumRepr, EnumType, Shape, UserType, Variant};

use crate::{Peek, trace};

use super::{FieldIter, HasFields};

/// Lets you read from an enum (implements read-only enum operations)
#[derive(Clone, Copy)]
pub struct PeekEnum<'mem, 'facet> {
    /// The internal data storage for the enum
    ///
    /// Note that this stores both the discriminant and the variant data
    /// (if any), and the layout depends on the enum representation.
    pub(crate) value: Peek<'mem, 'facet>,

    /// The definition of the enum.
    pub(crate) ty: EnumType,
}

impl core::fmt::Debug for PeekEnum<'_, '_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:?}", self.value)
    }
}

/// Returns the enum definition if the shape represents an enum, None otherwise
#[inline]
pub const fn peek_enum(shape: &'static Shape) -> Option<EnumType> {
    match shape.ty {
        facet_core::Type::User(UserType::Enum(enum_ty)) => Some(enum_ty),
        _ => None,
    }
}

/// Returns the enum representation if the shape represents an enum, None otherwise
#[inline]
pub fn peek_enum_repr(shape: &'static Shape) -> Option<EnumRepr> {
    peek_enum(shape).map(|enum_def| enum_def.enum_repr)
}

/// Returns the enum variants if the shape represents an enum, None otherwise
#[inline]
pub fn peek_enum_variants(shape: &'static Shape) -> Option<&'static [Variant]> {
    peek_enum(shape).map(|enum_def| enum_def.variants)
}

impl<'mem, 'facet> core::ops::Deref for PeekEnum<'mem, 'facet> {
    type Target = Peek<'mem, 'facet>;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<'mem, 'facet> PeekEnum<'mem, 'facet> {
    /// Returns the enum definition
    #[inline(always)]
    pub const fn ty(self) -> EnumType {
        self.ty
    }

    /// Returns the enum representation
    #[inline(always)]
    pub const fn enum_repr(self) -> EnumRepr {
        self.ty.enum_repr
    }

    /// Returns the enum variants
    #[inline(always)]
    pub const fn variants(self) -> &'static [Variant] {
        self.ty.variants
    }

    /// Returns the number of variants in this enum
    #[inline(always)]
    pub const fn variant_count(self) -> usize {
        self.ty.variants.len()
    }

    /// Returns the variant name at the given index
    #[inline(always)]
    pub fn variant_name(self, index: usize) -> Option<&'static str> {
        self.ty.variants.get(index).map(|variant| variant.name)
    }

    /// Returns the discriminant value for the current enum value
    ///
    /// Note: For `RustNPO` (null pointer optimization) types, there is no explicit
    /// discriminant stored in memory. In this case, 0 is returned. Use
    /// [`variant_index()`](Self::variant_index) to determine the active variant for NPO types.
    #[inline]
    pub fn discriminant(self) -> i64 {
        // Read the discriminant based on the enum representation
        match self.ty.enum_repr {
            // For Rust enums with unspecified layout, we cannot read the discriminant.
            // Panic since the caller should check the repr before calling this.
            EnumRepr::Rust => {
                panic!("cannot read discriminant from Rust enum with unspecified layout")
            }
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
    pub fn variant_index(self) -> Result<usize, VariantError> {
        // For Option<T> types, use the OptionVTable to correctly determine if the value is Some or None.
        // This handles both RustNPO (niche-optimized) and Rust (non-niche) representations.
        if let Def::Option(option_def) = self.value.shape.def {
            let is_some = unsafe { (option_def.vtable.is_some)(self.value.data()) };
            trace!("PeekEnum::variant_index (Option): is_some = {is_some}");
            // Find the variant by checking which has fields (Some) vs no fields (None)
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

        if self.ty.enum_repr == EnumRepr::RustNPO {
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
                                .shape()
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
    pub fn field(self, index: usize) -> Result<Option<Peek<'mem, 'facet>>, VariantError> {
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
    ) -> Result<Option<Peek<'mem, 'facet>>, VariantError> {
        let index_opt = self.field_index(field_name)?;
        match index_opt {
            Some(index) => self.field(index),
            None => Ok(None),
        }
    }
}

impl<'mem, 'facet> HasFields<'mem, 'facet> for PeekEnum<'mem, 'facet> {
    #[inline]
    fn fields(&self) -> FieldIter<'mem, 'facet> {
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
