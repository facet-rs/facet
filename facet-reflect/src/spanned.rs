//! Types for tracking source span information during deserialization.

use core::{mem, ops::Deref};

use facet_core::{
    Facet, Field, MarkerTraits, Shape, StructType, Type, TypeParam, UserType, ValueVTable,
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
        Shape::builder_for_sized::<Self>()
            .vtable(
                ValueVTable::builder::<Self>()
                    .type_name(|f, _opts| write!(f, "Span"))
                    .marker_traits(
                        MarkerTraits::SEND
                            .union(MarkerTraits::SYNC)
                            .union(MarkerTraits::EQ)
                            .union(MarkerTraits::COPY)
                            .union(MarkerTraits::UNPIN),
                    )
                    .default_in_place(Some(|target| unsafe { target.put(Span::default()).into() }))
                    .clone_into(Some(|src, dst| unsafe { dst.put(*src.get()).into() }))
                    .debug(Some(|this, f| {
                        let span = this.get();
                        write!(f, "Span {{ offset: {}, len: {} }}", span.offset, span.len)
                    }))
                    .partial_eq(Some(|a, b| a.get() == b.get()))
                    .build(),
            )
            .type_identifier("Span")
            .ty(Type::User(UserType::Struct(
                StructType::builder()
                    .kind(facet_core::StructKind::Struct)
                    .repr(facet_core::Repr::default())
                    .fields(
                        &const {
                            [
                                Field::builder()
                                    .name("offset")
                                    .shape(|| <usize as Facet>::SHAPE)
                                    .offset(mem::offset_of!(Span, offset))
                                    .build(),
                                Field::builder()
                                    .name("len")
                                    .shape(|| <usize as Facet>::SHAPE)
                                    .offset(mem::offset_of!(Span, len))
                                    .build(),
                            ]
                        },
                    )
                    .build(),
            )))
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
        Shape::builder_for_sized::<Self>()
            .vtable(
                ValueVTable::builder::<Self>()
                    .type_name(|f, opts| {
                        write!(f, "Spanned")?;
                        if let Some(opts) = opts.for_children() {
                            write!(f, "<")?;
                            T::SHAPE.vtable.type_name()(f, opts)?;
                            write!(f, ">")?;
                        } else {
                            write!(f, "<…>")?;
                        }
                        Ok(())
                    })
                    .build(),
            )
            .type_identifier("Spanned")
            .type_params(&[TypeParam {
                name: "T",
                shape: T::SHAPE,
            }])
            .ty(Type::User(UserType::Struct(
                StructType::builder()
                    .kind(facet_core::StructKind::Struct)
                    .repr(facet_core::Repr::default())
                    .fields(
                        &const {
                            [
                                Field::builder()
                                    .name("value")
                                    .shape(|| T::SHAPE)
                                    .offset(mem::offset_of!(Spanned<T>, value))
                                    .build(),
                                Field::builder()
                                    .name("span")
                                    .shape(|| Span::SHAPE)
                                    .offset(mem::offset_of!(Spanned<T>, span))
                                    .build(),
                            ]
                        },
                    )
                    .build(),
            )))
            .build()
    };
}

/// Check if a shape represents a `Spanned<T>` type.
///
/// This function checks the type identifier rather than duck-typing
/// based on field names, ensuring correct identification.
pub fn is_spanned_shape(shape: &Shape) -> bool {
    shape.type_identifier == "Spanned"
}
