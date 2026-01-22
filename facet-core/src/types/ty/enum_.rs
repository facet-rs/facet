use super::{Field, Repr, StructKind, StructType};

/// Fields for enum types
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct EnumType {
    /// Representation of the enum's data
    pub repr: Repr,

    /// representation of the enum's discriminant (u8, u16, etc.)
    pub enum_repr: EnumRepr,

    /// all variants for this enum
    pub variants: &'static [Variant],
}

/// Describes a variant of an enum
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct Variant {
    /// Name of the variant, e.g. `Foo` for `enum FooBar { Foo, Bar }`
    pub name: &'static str,

    /// Renamed variant name for serialization/deserialization.
    ///
    /// Set by `#[facet(rename = "name")]` or container-level `#[facet(rename_all = "...")]`.
    /// When present, serializers/deserializers should use this name instead of the variant's actual name.
    pub rename: Option<&'static str>,

    /// Discriminant value (if available). Might fit in a u8, etc.
    pub discriminant: Option<i64>,

    /// Attributes set for this variant via the derive macro
    pub attributes: &'static [VariantAttribute],

    /// Fields for this variant (empty if unit, number-named if tuple).
    /// IMPORTANT: the offset for the fields already takes into account the size & alignment of the
    /// discriminant.
    pub data: StructType,

    /// Doc comment for the variant
    pub doc: &'static [&'static str],
}

impl Variant {
    /// Checks whether the `Variant` has an attribute with the given namespace and key.
    ///
    /// Use `None` for builtin attributes, `Some("ns")` for namespaced attributes.
    #[inline]
    pub fn has_attr(&self, ns: Option<&str>, key: &str) -> bool {
        self.attributes
            .iter()
            .any(|attr| attr.ns == ns && attr.key == key)
    }

    /// Gets an attribute by namespace and key.
    ///
    /// Use `None` for builtin attributes, `Some("ns")` for namespaced attributes.
    #[inline]
    pub fn get_attr(&self, ns: Option<&str>, key: &str) -> Option<&super::Attr> {
        self.attributes
            .iter()
            .find(|attr| attr.ns == ns && attr.key == key)
    }

    /// Checks whether the `Variant` has a builtin attribute with the given key.
    #[inline]
    pub fn has_builtin_attr(&self, key: &str) -> bool {
        self.has_attr(None, key)
    }

    /// Gets a builtin attribute by key.
    #[inline]
    pub fn get_builtin_attr(&self, key: &str) -> Option<&super::Attr> {
        self.get_attr(None, key)
    }

    /// Returns true if this variant has the `#[facet(html::text)]` or `#[facet(xml::text)]` attribute.
    ///
    /// When serializing to HTML/XML, variants marked as text should be serialized as
    /// text content rather than as elements.
    #[inline]
    pub fn is_text(&self) -> bool {
        self.has_builtin_attr("text")
            || self.has_attr(Some("html"), "text")
            || self.has_attr(Some("xml"), "text")
    }

    /// Returns true if this variant has the `#[facet(custom_element)]`,
    /// `#[facet(html::custom_element)]` or `#[facet(xml::custom_element)]` attribute.
    ///
    /// When deserializing HTML/XML, variants marked as custom_element act as a catch-all
    /// for unknown element names. The element's tag name is stored in the variant's `tag` field.
    #[inline]
    pub fn is_custom_element(&self) -> bool {
        self.has_builtin_attr("custom_element")
            || self.has_attr(Some("html"), "custom_element")
            || self.has_attr(Some("xml"), "custom_element")
    }

    /// Returns true if this variant has the `#[facet(other)]` attribute.
    ///
    /// When deserializing, variants marked as `other` act as a catch-all
    /// for unknown variant names. This is useful for extensible enums where
    /// unknown tags should be captured rather than rejected.
    #[inline]
    pub fn is_other(&self) -> bool {
        self.has_builtin_attr("other")
    }

    /// Returns the effective name for serialization/deserialization.
    ///
    /// Returns `rename` if set, otherwise returns the variant's actual name.
    #[inline]
    pub fn effective_name(&self) -> &'static str {
        self.rename.unwrap_or(self.name)
    }
}

/// An attribute that can be set on an enum variant.
/// This is now just an alias for `ExtensionAttr` - all attributes use the same representation.
pub type VariantAttribute = super::Attr;

/// All possible representations for Rust enums — ie. the type/size of the discriminant
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
pub enum EnumRepr {
    /// Special-case representation discriminated by zeros under non-nullable pointer
    ///
    /// See: <https://rust-lang.github.io/unsafe-code-guidelines/layout/enums.html#discriminant-elision-on-option-like-enums>
    RustNPO,
    /// u8 representation (#[repr(u8)])
    U8,
    /// u16 representation (#[repr(u16)])
    U16,
    /// u32 representation (#[repr(u32)])
    U32,
    /// u64 representation (#[repr(u64)])
    U64,
    /// usize representation (#[repr(usize)])
    USize,
    /// i8 representation (#[repr(i8)])
    I8,
    /// i16 representation (#[repr(i16)])
    I16,
    /// i32 representation (#[repr(i32)])
    I32,
    /// i64 representation (#[repr(i64)])
    I64,
    /// isize representation (#[repr(isize)])
    ISize,
}

impl EnumRepr {
    /// Returns the enum representation for the given discriminant type
    ///
    /// NOTE: only supports unsigned discriminants
    ///
    /// # Panics
    ///
    /// Panics if the size of the discriminant size is not 1, 2, 4, or 8 bytes.
    pub const fn from_discriminant_size<T>() -> Self {
        match core::mem::size_of::<T>() {
            1 => EnumRepr::U8,
            2 => EnumRepr::U16,
            4 => EnumRepr::U32,
            8 => EnumRepr::U64,
            _ => panic!("Invalid enum size"),
        }
    }
}

/// Builder for constructing [`Variant`] instances in const contexts.
///
/// This builder enables shorter derive macro output by providing a fluent API
/// for constructing variants with default values for optional fields.
///
/// # Example
///
/// ```
/// use facet_core::{VariantBuilder, StructTypeBuilder, StructKind, Variant};
///
/// const VARIANT: Variant = VariantBuilder::new(
///     "Foo",
///     StructTypeBuilder::new(StructKind::Unit, &[]).build()
/// )
/// .discriminant(42)
/// .build();
/// ```
#[derive(Clone, Copy, Debug)]
pub struct VariantBuilder {
    name: &'static str,
    rename: Option<&'static str>,
    discriminant: Option<i64>,
    attributes: &'static [VariantAttribute],
    data: StructType,
    doc: &'static [&'static str],
}

impl VariantBuilder {
    /// Creates a new `VariantBuilder` with the required fields.
    ///
    /// # Parameters
    ///
    /// - `name`: The name of the variant
    /// - `data`: The struct type representing the variant's fields
    #[inline]
    pub const fn new(name: &'static str, data: StructType) -> Self {
        Self {
            name,
            rename: None,
            discriminant: None,
            attributes: &[],
            data,
            doc: &[],
        }
    }

    /// Creates a unit variant (no fields).
    ///
    /// # Example
    /// ```ignore
    /// VariantBuilder::unit("None").build()
    /// ```
    #[inline]
    pub const fn unit(name: &'static str) -> Self {
        Self::new(name, StructType::UNIT)
    }

    /// Creates a tuple variant with the given fields.
    ///
    /// # Example
    /// ```ignore
    /// VariantBuilder::tuple("Some", &[FieldBuilder::new("0", T::SHAPE, 0).build()]).build()
    /// ```
    #[inline]
    pub const fn tuple(name: &'static str, fields: &'static [Field]) -> Self {
        Self::new(
            name,
            StructType {
                repr: Repr::default(),
                kind: StructKind::TupleStruct,
                fields,
            },
        )
    }

    /// Sets the renamed name for this variant.
    ///
    /// Defaults to `None` if not called.
    #[inline]
    pub const fn rename(mut self, rename: &'static str) -> Self {
        self.rename = Some(rename);
        self
    }

    /// Sets the discriminant value for this variant.
    ///
    /// Defaults to `None` if not called.
    #[inline]
    pub const fn discriminant(mut self, discriminant: i64) -> Self {
        self.discriminant = Some(discriminant);
        self
    }

    /// Sets the attributes for this variant.
    ///
    /// Defaults to an empty slice if not called.
    #[inline]
    pub const fn attributes(mut self, attributes: &'static [VariantAttribute]) -> Self {
        self.attributes = attributes;
        self
    }

    /// Sets the documentation for this variant.
    ///
    /// Defaults to an empty slice if not called.
    #[inline]
    pub const fn doc(mut self, doc: &'static [&'static str]) -> Self {
        self.doc = doc;
        self
    }

    /// Builds the final [`Variant`] instance.
    #[inline]
    pub const fn build(self) -> Variant {
        Variant {
            name: self.name,
            rename: self.rename,
            discriminant: self.discriminant,
            attributes: self.attributes,
            data: self.data,
            doc: self.doc,
        }
    }
}

/// Builder for constructing [`EnumType`] instances in const contexts.
///
/// This builder enables shorter derive macro output by providing a fluent API
/// for constructing enum types with default values for optional fields.
///
/// # Example
///
/// ```
/// use facet_core::{EnumTypeBuilder, EnumRepr, Repr, EnumType};
///
/// const ENUM: EnumType = EnumTypeBuilder::new(EnumRepr::U8, &[])
///     .repr(Repr::c())
///     .build();
/// ```
#[derive(Clone, Copy, Debug)]
pub struct EnumTypeBuilder {
    repr: Repr,
    enum_repr: EnumRepr,
    variants: &'static [Variant],
}

impl EnumTypeBuilder {
    /// Creates a new `EnumTypeBuilder` with the required fields.
    ///
    /// # Parameters
    ///
    /// - `enum_repr`: The representation of the enum's discriminant
    /// - `variants`: All variants for this enum
    #[inline]
    pub const fn new(enum_repr: EnumRepr, variants: &'static [Variant]) -> Self {
        Self {
            repr: Repr::c(),
            enum_repr,
            variants,
        }
    }

    /// Sets the representation of the enum's data.
    ///
    /// Defaults to `Repr::c()` if not called.
    #[inline]
    pub const fn repr(mut self, repr: Repr) -> Self {
        self.repr = repr;
        self
    }

    /// Builds the final [`EnumType`] instance.
    #[inline]
    pub const fn build(self) -> EnumType {
        EnumType {
            repr: self.repr,
            enum_repr: self.enum_repr,
            variants: self.variants,
        }
    }
}
