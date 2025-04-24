use facet_core::{EnumDef, EnumRepr, Field, FieldAttribute, Shape, Variant};

use crate::Peek;

/// Lets you read from an enum (implements read-only enum operations)
#[derive(Clone, Copy)]
pub struct PeekEnum<'mem, 'facet_lifetime> {
    /// The internal data storage for the enum
    ///
    /// Note that this stores both the discriminant and the variant data
    /// (if any), and the layout depends on the enum representation.
    pub(crate) value: Peek<'mem, 'facet_lifetime>,

    /// The definition of the enum.
    pub(crate) def: EnumDef,
}

/// Returns the enum definition if the shape represents an enum, None otherwise
pub fn peek_enum(shape: &'static Shape) -> Option<EnumDef> {
    match shape.def {
        facet_core::Def::Enum(enum_def) => Some(enum_def),
        _ => None,
    }
}

/// Returns the enum representation if the shape represents an enum, None otherwise
pub fn peek_enum_repr(shape: &'static Shape) -> Option<EnumRepr> {
    peek_enum(shape).map(|enum_def| enum_def.repr)
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
    pub fn def(self) -> EnumDef {
        self.def
    }

    /// Returns the enum representation
    #[inline(always)]
    pub fn repr(self) -> EnumRepr {
        self.def.repr
    }

    /// Returns the enum variants
    #[inline(always)]
    pub fn variants(self) -> &'static [Variant] {
        self.def.variants
    }

    /// Returns the number of variants in this enum
    #[inline(always)]
    pub fn variant_count(self) -> usize {
        self.def.variants.len()
    }

    /// Returns the variant name at the given index
    #[inline(always)]
    pub fn variant_name(self, index: usize) -> Option<&'static str> {
        self.def.variants.get(index).map(|variant| variant.name)
    }

    /// Returns the discriminant value for the current enum value
    #[inline]
    pub fn discriminant(self) -> i64 {
        // Read the discriminant based on the enum representation
        unsafe {
            let data = self.value.data();
            match self.def.repr {
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
    pub fn variant_index(self) -> usize {
        let discriminant = self.discriminant();

        // Find the variant with matching discriminant using position method
        self.def
            .variants
            .iter()
            .position(|variant| variant.discriminant == discriminant)
            .expect("No variant found with matching discriminant")
    }

    /// Returns the active variant
    #[inline]
    pub fn active_variant(self) -> &'static Variant {
        let index = self.variant_index();
        &self.def.variants[index]
    }

    /// Returns the name of the active variant for this enum value
    #[inline]
    pub fn variant_name_active(self) -> &'static str {
        self.active_variant().name
    }

    // variant_data has been removed to reduce unsafe code exposure

    /// Returns a PeekValue handle to a field of a tuple or struct variant by index
    pub fn field(self, index: usize) -> Option<Peek<'mem, 'facet_lifetime>> {
        let variant = self.active_variant();
        let fields = &variant.data.fields;

        if index >= fields.len() {
            return None;
        }

        let field = &fields[index];
        let field_data = unsafe { self.value.data().field(field.offset) };
        Some(unsafe { Peek::unchecked_new(field_data, field.shape()) })
    }

    /// Returns the index of a field in the active variant by name
    pub fn field_index(self, field_name: &str) -> Option<usize> {
        let variant = self.active_variant();
        variant
            .data
            .fields
            .iter()
            .position(|f| f.name == field_name)
    }

    /// Returns a PeekValue handle to a field of a tuple or struct variant by name
    pub fn field_by_name(self, field_name: &str) -> Option<Peek<'mem, 'facet_lifetime>> {
        let index = self.field_index(field_name)?;
        self.field(index)
    }

    /// Iterates over all fields in this enum variant, providing both field metadata and value
    #[inline]
    pub fn fields(self) -> impl Iterator<Item = (&'static Field, Peek<'mem, 'facet_lifetime>)> {
        let variant = self.active_variant();
        let fields = &variant.data.fields;

        // Create an iterator that maps each field to a (Field, PeekValue) pair
        fields.iter().map(move |field| {
            let field_data = unsafe { self.value.data().field(field.offset) };
            let peek = unsafe { Peek::unchecked_new(field_data, field.shape()) };
            (field, peek)
        })
    }

    /// Iterates over thos fields in this struct that should be included when it is serialized.
    #[inline]
    pub fn fields_for_serialize(
        &self,
    ) -> impl Iterator<Item = (&'static Field, Peek<'mem, 'facet_lifetime>)> + '_ {
        self.fields().filter(|(field, peek)| {
            for attr in field.attributes {
                match attr {
                    FieldAttribute::SkipSerializingIf(fn_ptr) => {
                        if unsafe { fn_ptr(peek.data()) } {
                            return false;
                        }
                    }
                    FieldAttribute::SkipSerializing => {
                        return false;
                    }
                    _ => {}
                }
            }
            true
        })
    }
}
