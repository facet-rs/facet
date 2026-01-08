//! Types for tracking source span information during deserialization.

use core::{mem, ops::Deref};

use facet_core::{
    Def, Facet, FieldBuilder, Shape, StructKind, TypeOpsDirect, type_ops_direct, vtable_direct,
};

/// Source span with offset and length.
///
/// This type tracks a byte offset and length within a source document,
/// useful for error reporting that can point back to the original source.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Span {
    /// Byte offset from start of source.
    pub offset: usize,
    /// Length in bytes.
    pub len: usize,
}

impl Span {
    /// Create a new span with the given offset and length.
    pub const fn new(offset: usize, len: usize) -> Self {
        Self { offset, len }
    }

    /// Check if this span is unknown (zero offset and length).
    pub fn is_unknown(&self) -> bool {
        self.offset == 0 && self.len == 0
    }

    /// Get the end offset (offset + len).
    pub fn end(&self) -> usize {
        self.offset + self.len
    }
}

#[cfg(feature = "miette")]
impl From<Span> for miette::SourceSpan {
    fn from(span: Span) -> Self {
        miette::SourceSpan::new(span.offset.into(), span.len)
    }
}

#[cfg(feature = "miette")]
impl From<miette::SourceSpan> for Span {
    fn from(span: miette::SourceSpan) -> Self {
        Self {
            offset: span.offset(),
            len: span.len(),
        }
    }
}

// SAFETY: Span is a simple struct with two usize fields, properly laid out
unsafe impl Facet<'_> for Span {
    const SHAPE: &'static Shape = &const {
        static FIELDS: [facet_core::Field; 2] = [
            FieldBuilder::new(
                "offset",
                facet_core::shape_of::<usize>,
                mem::offset_of!(Span, offset),
            )
            .build(),
            FieldBuilder::new(
                "len",
                facet_core::shape_of::<usize>,
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

/// A value with source span information.
///
/// This struct wraps a value along with the source location (offset and length)
/// where it was parsed from. This is useful for error reporting that can point
/// back to the original source.
#[derive(Debug)]
pub struct Spanned<T> {
    /// The wrapped value.
    pub value: T,
    /// The source span (offset and length).
    pub span: Span,
}

impl<T> Spanned<T> {
    /// Create a new spanned value.
    pub const fn new(value: T, span: Span) -> Self {
        Self { value, span }
    }

    /// Get the source span.
    pub fn span(&self) -> Span {
        self.span
    }

    /// Get a reference to the inner value.
    pub fn value(&self) -> &T {
        &self.value
    }

    /// Unwrap into the inner value, discarding span information.
    pub fn into_inner(self) -> T {
        self.value
    }
}

impl<T> Deref for Spanned<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T: Default> Default for Spanned<T> {
    fn default() -> Self {
        Self {
            value: T::default(),
            span: Span::default(),
        }
    }
}

impl<T: Clone> Clone for Spanned<T> {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
            span: self.span,
        }
    }
}

impl<T: PartialEq> PartialEq for Spanned<T> {
    fn eq(&self, other: &Self) -> bool {
        // Only compare the value, not the span
        self.value == other.value
    }
}

impl<T: Eq> Eq for Spanned<T> {}

// SAFETY: Spanned<T> is a simple struct with a value and span field, properly laid out
unsafe impl<'a, T: Facet<'a>> Facet<'a> for Spanned<T> {
    const SHAPE: &'static Shape = &const {
        use facet_core::{TypeOpsIndirect, TypeParam, VTableIndirect};

        unsafe fn drop_in_place<T>(ox: facet_core::OxPtrMut) {
            // SAFETY: The caller guarantees ox points to a valid Spanned<T>
            unsafe { core::ptr::drop_in_place(ox.ptr().as_byte_ptr() as *mut Spanned<T>) };
        }

        Shape::builder_for_sized::<Spanned<T>>("Spanned")
            .decl_id(facet_core::DeclId::new(facet_core::decl_id_hash("Spanned")))
            .vtable_indirect(&VTableIndirect::EMPTY)
            .type_ops_indirect(
                &const {
                    TypeOpsIndirect {
                        drop_in_place: drop_in_place::<T>,
                        default_in_place: None,
                        clone_into: None,
                        is_truthy: None,
                    }
                },
            )
            .type_params(
                &const {
                    [TypeParam {
                        name: "T",
                        shape: T::SHAPE,
                    }]
                },
            )
            .ty(facet_core::Type::struct_builder(
                StructKind::Struct,
                &const {
                    [
                        FieldBuilder::new(
                            "value",
                            facet_core::shape_of::<T>,
                            mem::offset_of!(Spanned<T>, value),
                        )
                        .build(),
                        FieldBuilder::new(
                            "span",
                            facet_core::shape_of::<Span>,
                            mem::offset_of!(Spanned<T>, span),
                        )
                        // Mark span as metadata - excluded from structural hashing/equality
                        // Deserializers that support span metadata will populate this field
                        .metadata("span")
                        .build(),
                    ]
                },
            )
            .build())
            .def(Def::Undefined)
            .type_name(|_shape, f, opts| {
                write!(f, "Spanned")?;
                if let Some(opts) = opts.for_children() {
                    write!(f, "<")?;
                    if let Some(type_name_fn) = T::SHAPE.type_name {
                        type_name_fn(T::SHAPE, f, opts)?;
                    } else {
                        write!(f, "{}", T::SHAPE.type_identifier)?;
                    }
                    write!(f, ">")?;
                } else {
                    write!(f, "<…>")?;
                }
                Ok(())
            })
            .build()
    };
}

/// Check if a shape represents a type with span metadata (like `Spanned<T>`).
///
/// Returns `true` if the shape is a struct with:
/// - At least one non-metadata field (the actual value)
/// - A field with `#[facet(metadata = span)]` for storing source location
///
/// This allows any struct to be "spanned" by adding the metadata attribute,
/// not just the built-in `Spanned<T>` wrapper.
pub fn is_spanned_shape(shape: &Shape) -> bool {
    use facet_core::{Type, UserType};

    if let Type::User(UserType::Struct(struct_def)) = &shape.ty {
        let has_span_metadata = struct_def
            .fields
            .iter()
            .any(|f| f.metadata_kind() == Some("span"));
        let has_value_field = struct_def.fields.iter().any(|f| !f.is_metadata());
        return has_span_metadata && has_value_field;
    }
    false
}

/// Find the span metadata field in a struct shape.
///
/// Returns the field with `#[facet(metadata = span)]` if present.
pub fn find_span_metadata_field(shape: &Shape) -> Option<&'static facet_core::Field> {
    use facet_core::{Type, UserType};

    if let Type::User(UserType::Struct(struct_def)) = &shape.ty {
        return struct_def
            .fields
            .iter()
            .find(|f| f.metadata_kind() == Some("span"));
    }
    None
}

/// Extract the inner value shape from a Spanned-like struct.
///
/// For a struct with span metadata, this returns the shape of the first
/// non-metadata field (typically the `value` field in `Spanned<T>`).
///
/// This is useful when you need to look through a Spanned wrapper to
/// determine the actual type being wrapped, such as when matching
/// untagged enum variants against scalar values.
///
/// Returns `None` if the shape is not spanned or has no value fields.
pub fn get_spanned_inner_shape(shape: &Shape) -> Option<&'static Shape> {
    use facet_core::{Type, UserType};

    if !is_spanned_shape(shape) {
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
