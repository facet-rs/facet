use facet_core::{EnumDef, EnumRepr, Shape, Variant, VariantKind};

#[cfg(feature = "alloc")]
extern crate alloc;
#[cfg(feature = "alloc")]
use alloc::boxed::Box;

/// Lets you read from an enum (implements read-only enum operations)
#[derive(Clone, Copy)]
pub struct PeekEnum<'mem> {
    value: crate::PeekValue<'mem>,
    def: EnumDef,
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

impl<'mem> core::ops::Deref for PeekEnum<'mem> {
    type Target = crate::PeekValue<'mem>;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<'mem> PeekEnum<'mem> {
    /// Create a new peek enum
    pub(crate) fn new(value: crate::PeekValue<'mem>, def: EnumDef) -> Self {
        Self { value, def }
    }

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

        // Find the variant with matching discriminant
        for (index, variant) in self.def.variants.iter().enumerate() {
            let variant_discriminant = match variant.discriminant {
                Some(value) => value,
                None => index as i64,
            };

            if variant_discriminant == discriminant {
                return index;
            }
        }

        // This should never happen for valid enums
        panic!("Invalid discriminant value for enum")
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

    /// Returns the kind of the active variant (Unit, Tuple, Struct)
    #[inline]
    pub fn variant_kind_active(self) -> &'static VariantKind {
        &self.active_variant().kind
    }

    /// Returns a Peek handle to a field of a tuple or struct variant
    pub fn field(self, field_name: &str) -> Option<crate::Peek<'mem>> {
        let variant = self.active_variant();

        match &variant.kind {
            VariantKind::Unit => None, // Unit variants have no fields
            VariantKind::Tuple { fields } => {
                // For tuple variants, find by name
                let field = fields.iter().find(|f| f.name == field_name)?;
                let field_data = unsafe { self.value.data().field(field.offset) };
                Some(unsafe { crate::Peek::unchecked_new(field_data, field.shape) })
            }
            VariantKind::Struct { fields } => {
                // For struct variants, find by name
                let field = fields.iter().find(|f| f.name == field_name)?;
                let field_data = unsafe { self.value.data().field(field.offset) };
                Some(unsafe { crate::Peek::unchecked_new(field_data, field.shape) })
            }
            _ => None, // Handle other variant kinds that might be added in the future
        }
    }

    /// Returns a Peek handle to a field of a tuple variant by index
    pub fn tuple_field(self, index: usize) -> Option<crate::Peek<'mem>> {
        let variant = self.active_variant();

        match &variant.kind {
            VariantKind::Tuple { fields } => {
                if index >= fields.len() {
                    return None;
                }

                let field = &fields[index];
                let field_data = unsafe { self.value.data().field(field.offset) };
                Some(unsafe { crate::Peek::unchecked_new(field_data, field.shape) })
            }
            _ => None, // Not a tuple variant
        }
    }

    /// Returns an iterator over fields of a struct or tuple variant with metadata
    #[cfg(feature = "alloc")]
    pub fn fields_with_metadata(
        self,
    ) -> Box<
        dyn Iterator<
                Item = (
                    usize,
                    &'static str,
                    crate::Peek<'mem>,
                    &'static facet_core::Field,
                ),
            > + 'mem,
    > {
        let variant = self.active_variant();
        let data = self.value.data();

        match &variant.kind {
            VariantKind::Struct { fields } => {
                Box::new(fields.iter().enumerate().map(move |(i, field)| {
                    let field_data = unsafe { data.field(field.offset) };
                    let field_peek = unsafe { crate::Peek::unchecked_new(field_data, field.shape) };
                    (i, field.name, field_peek, field)
                }))
            }
            VariantKind::Tuple { fields } => {
                Box::new(fields.iter().enumerate().map(move |(i, field)| {
                    let field_data = unsafe { data.field(field.offset) };
                    let field_peek = unsafe { crate::Peek::unchecked_new(field_data, field.shape) };
                    (i, field.name, field_peek, field)
                }))
            }
            _ => Box::new(core::iter::empty()),
        }
    }

    /// Returns an iterator over fields of a struct or tuple variant
    #[cfg(feature = "alloc")]
    pub fn fields(self) -> Box<dyn Iterator<Item = (&'static str, crate::Peek<'mem>)> + 'mem> {
        let variant = self.active_variant();
        let data = self.value.data();

        match &variant.kind {
            VariantKind::Struct { fields } => Box::new(fields.iter().map(move |field| {
                let field_data = unsafe { data.field(field.offset) };
                let peek = unsafe { crate::Peek::unchecked_new(field_data, field.shape) };
                (field.name, peek)
            })),
            VariantKind::Tuple { fields } => Box::new(fields.iter().map(move |field| {
                let field_data = unsafe { data.field(field.offset) };
                let peek = unsafe { crate::Peek::unchecked_new(field_data, field.shape) };
                (field.name, peek)
            })),
            _ => Box::new(core::iter::empty()),
        }
    }
}
