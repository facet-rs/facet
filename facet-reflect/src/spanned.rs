//! Types for tracking source span information during deserialization.

use core::{fmt, mem};

use facet_core::{
    Def, Facet, FieldBuilder, Shape, StructKind, TypeOpsDirect, type_ops_direct, vtable_direct,
};

/// Source span with offset and length.
///
/// This type tracks a byte offset and length within a source document,
/// useful for error reporting that can point back to the original source.
///
/// To use span tracking in your own types, define a wrapper struct with
/// `#[facet(metadata_container)]` and a span field marked with `#[facet(metadata = "span")]`:
///
/// ```rust
/// use facet::Facet;
/// use facet_reflect::Span;
///
/// #[derive(Debug, Clone, Facet)]
/// #[facet(metadata_container)]
/// pub struct Spanned<T> {
///     pub value: T,
///     #[facet(metadata = "span")]
///     pub span: Option<Span>,
/// }
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Span {
    /// Byte offset from start of source (max 4GB).
    pub offset: u32,
    /// Length in bytes (max 4GB).
    pub len: u32,
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // use half-closed notation
        write!(f, "[{}..{})", self.offset, self.offset + self.len)
    }
}

impl Span {
    /// Create a new span with the given offset and length.
    ///
    /// Values larger than `u32::MAX` are saturated.
    pub const fn new(offset: usize, len: usize) -> Self {
        Self {
            offset: if offset > u32::MAX as usize {
                u32::MAX
            } else {
                offset as u32
            },
            len: if len > u32::MAX as usize {
                u32::MAX
            } else {
                len as u32
            },
        }
    }

    /// Check if this span is unknown (zero offset and length).
    pub const fn is_unknown(&self) -> bool {
        self.offset == 0 && self.len == 0
    }

    /// Get the end offset (offset + len).
    pub const fn end(&self) -> usize {
        self.offset as usize + self.len as usize
    }
}

// SAFETY: Span is a simple struct with two u32 fields, properly laid out
unsafe impl Facet<'_> for Span {
    const SHAPE: &'static Shape = &const {
        static FIELDS: [facet_core::Field; 2] = [
            FieldBuilder::new(
                "offset",
                facet_core::shape_of::<u32>,
                mem::offset_of!(Span, offset),
            )
            .build(),
            FieldBuilder::new(
                "len",
                facet_core::shape_of::<u32>,
                mem::offset_of!(Span, len),
            )
            .build(),
        ];

        const VTABLE: facet_core::VTableDirect = vtable_direct!(Span => Debug, PartialEq);
        const TYPE_OPS: TypeOpsDirect = type_ops_direct!(Span => Default, Clone);

        Shape::builder_for_sized::<Span>("Span")
            .vtable_direct(&VTABLE)
            .type_ops_direct(&TYPE_OPS)
            .ty(facet_core::Type::struct_builder(StructKind::Struct, &FIELDS).build())
            .def(Def::Undefined)
            .build()
    };
}

/// Extract the inner value shape from a metadata container.
///
/// For a struct marked with `#[facet(metadata_container)]`, this returns
/// the shape of the first non-metadata field (the actual value being wrapped).
///
/// This is useful when you need to look through a metadata wrapper (like
/// a user-defined `Spanned<T>` or `Documented<T>`) to determine the actual type
/// being wrapped, such as when matching untagged enum variants against scalar values.
///
/// Returns `None` if the shape is not a metadata container or has no value fields.
pub fn get_metadata_container_value_shape(shape: &Shape) -> Option<&'static Shape> {
    use facet_core::{Type, UserType};

    if !shape.is_metadata_container() {
        return None;
    }

    if let Type::User(UserType::Struct(struct_def)) = &shape.ty {
        // Find the first non-metadata field (the actual value)
        struct_def
            .fields
            .iter()
            .find(|f| !f.is_metadata())
            .map(|f| f.shape.get())
    } else {
        None
    }
}
