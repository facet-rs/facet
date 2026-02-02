//! Shape abstractions for formal verification.
//!
//! In regular operation, trame uses `&'static Shape` from facet_core.
//! However, for testing, we use `DynShape`, which implements Arbitrary.
//! Both implement `IShape`, a common interface for shapes.
//!
//! The `Partial` type is generic over `S: IShape`, so:
//! - Production: `Partial<&'static Shape>`
//! - Kani proofs: `Partial<DynShape>`
//!
//! # Important: Layout is not the point
//!
//! Rust's struct layout is **not stable, not guaranteed, and not documented**.
//! The compiler is free to reorder fields, insert padding, and generally do
//! whatever it wants (unless you use `#[repr(C)]` or similar).
//!
//! **We are not trying to replicate real Rust layouts here.**
//!
//! For Kani verification, what matters is the **state machine**, not the memory layout:
//! - How many fields need independent initialization tracking?
//! - What are the valid state transitions? (Unallocated → Allocated → Initialized → ...)
//! - Are all invariants maintained? (no double-init, no drop-before-init, no leak)

use core::alloc::Layout;

// ============================================================================
// Traits
// ============================================================================

/// Common interface for shapes.
///
/// Implemented by:
/// - `&'static facet_core::Shape` (real shapes)
/// - `DynShape` (synthetic shapes for Kani)
pub trait IShape: Copy {
    /// The struct type returned by `as_struct()`.
    type StructType: IStructType<Field = Self::Field>;

    /// The field type used by struct types.
    type Field: IField<Shape = Self>;

    /// Get the layout (size and alignment) of this shape.
    fn layout(&self) -> Layout;

    /// Check if this is a struct type.
    fn is_struct(&self) -> bool;

    /// Get struct-specific information, if this is a struct.
    fn as_struct(&self) -> Option<Self::StructType>;
}

/// Interface for struct type information.
pub trait IStructType: Copy {
    /// The field type.
    type Field: IField;

    /// Number of fields in this struct.
    fn field_count(&self) -> usize;

    /// Get field by index.
    fn field(&self, idx: usize) -> Option<Self::Field>;
}

/// Interface for field information.
pub trait IField: Copy {
    /// The shape type.
    type Shape: IShape;

    /// Byte offset of this field within the struct.
    fn offset(&self) -> usize;

    /// Shape of this field's type.
    fn shape(&self) -> Self::Shape;
}

// ============================================================================
// DynShape - synthetic shapes for Kani
// ============================================================================

/// Maximum number of fields in a struct (for bounded verification).
pub const MAX_FIELDS: usize = 8;

/// A synthetic field for Kani verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DynField {
    /// Byte offset of this field within the struct.
    pub offset: usize,
    /// Layout of this field (we don't track nested shapes for now).
    pub layout: Layout,
}

/// A synthetic struct type for Kani verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DynStructType {
    /// Number of fields.
    pub field_count: u8,
    /// Field information (only first `field_count` entries are valid).
    pub fields: [DynField; MAX_FIELDS],
}

/// A bounded shape for Kani verification.
///
/// Unlike `facet_core::Shape` which uses static references and can be recursive,
/// these shapes are bounded and can implement `kani::Arbitrary`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DynShape {
    /// Layout of this type.
    pub layout: Layout,
    /// Type-specific information.
    pub def: DynDef,
}

/// Type-specific definition for DynShape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DynDef {
    /// A scalar type (no internal structure to track).
    Scalar,
    /// A struct with indexed fields.
    Struct(DynStructType),
    // TODO: Enum, Option, Result, List, Map, etc.
}

// ============================================================================
// IShape implementation for DynShape
// ============================================================================

impl IShape for DynShape {
    type StructType = DynStructType;
    type Field = DynField;

    #[inline]
    fn layout(&self) -> Layout {
        self.layout
    }

    #[inline]
    fn is_struct(&self) -> bool {
        matches!(self.def, DynDef::Struct(_))
    }

    #[inline]
    fn as_struct(&self) -> Option<Self::StructType> {
        match self.def {
            DynDef::Struct(s) => Some(s),
            _ => None,
        }
    }
}

impl IStructType for DynStructType {
    type Field = DynField;

    #[inline]
    fn field_count(&self) -> usize {
        self.field_count as usize
    }

    #[inline]
    fn field(&self, idx: usize) -> Option<Self::Field> {
        if idx < self.field_count as usize {
            Some(self.fields[idx])
        } else {
            None
        }
    }
}

impl IField for DynField {
    type Shape = DynShape;

    #[inline]
    fn offset(&self) -> usize {
        self.offset
    }

    #[inline]
    fn shape(&self) -> Self::Shape {
        // For now, fields are treated as scalars (no nested struct tracking)
        DynShape {
            layout: self.layout,
            def: DynDef::Scalar,
        }
    }
}

// ============================================================================
// Constructors
// ============================================================================

impl DynShape {
    /// Create a scalar shape with the given layout.
    pub const fn scalar(layout: Layout) -> Self {
        Self {
            layout,
            def: DynDef::Scalar,
        }
    }

    /// Create a struct shape with the given fields.
    pub fn struct_with_fields(fields: &[DynField]) -> Self {
        assert!(fields.len() <= MAX_FIELDS, "too many fields");

        // Calculate overall layout from fields
        let mut size = 0usize;
        let mut align = 1usize;

        for field in fields {
            align = align.max(field.layout.align());
            let field_end = field.offset + field.layout.size();
            size = size.max(field_end);
        }

        // Round up size to alignment
        size = (size + align - 1) & !(align - 1);

        let layout = Layout::from_size_align(size, align).expect("valid layout");

        let mut field_array = [DynField {
            offset: 0,
            layout: Layout::new::<()>(),
        }; MAX_FIELDS];
        for (i, f) in fields.iter().enumerate() {
            field_array[i] = *f;
        }

        Self {
            layout,
            def: DynDef::Struct(DynStructType {
                field_count: fields.len() as u8,
                fields: field_array,
            }),
        }
    }
}

impl DynField {
    /// Create a new field.
    pub const fn new(offset: usize, layout: Layout) -> Self {
        Self { offset, layout }
    }
}

// ============================================================================
// Arbitrary for Kani
// ============================================================================

#[cfg(kani)]
impl kani::Arbitrary for DynShape {
    fn any() -> Self {
        let is_struct: bool = kani::any();

        if is_struct {
            // Struct with 1-4 fields
            let field_count: u8 = kani::any();
            kani::assume(field_count > 0 && field_count <= 4);

            let mut fields = [DynField {
                offset: 0,
                layout: Layout::new::<()>(),
            }; MAX_FIELDS];

            let mut offset = 0usize;
            for i in 0..(field_count as usize) {
                let field_size: usize = kani::any();
                kani::assume(field_size > 0 && field_size <= 8);

                let layout = Layout::from_size_align(field_size, 1).unwrap();
                fields[i] = DynField::new(offset, layout);
                offset += field_size;
            }

            kani::assume(offset <= 64);

            let layout = Layout::from_size_align(offset, 1).unwrap();

            DynShape {
                layout,
                def: DynDef::Struct(DynStructType {
                    field_count,
                    fields,
                }),
            }
        } else {
            // Scalar
            let size: usize = kani::any();
            let align_pow: u8 = kani::any();
            kani::assume(size <= 64);
            kani::assume(align_pow <= 3);
            let align = 1usize << align_pow;
            kani::assume(size == 0 || size % align == 0);

            let layout = Layout::from_size_align(size, align).unwrap();
            DynShape::scalar(layout)
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalar_is_not_struct() {
        let s = DynShape::scalar(Layout::new::<u32>());
        assert!(!s.is_struct());
        assert!(s.as_struct().is_none());
    }

    #[test]
    fn struct_is_struct() {
        let fields = [
            DynField::new(0, Layout::new::<u32>()),
            DynField::new(4, Layout::new::<u32>()),
        ];
        let s = DynShape::struct_with_fields(&fields);
        assert!(s.is_struct());
        assert!(s.as_struct().is_some());
    }

    #[test]
    fn struct_field_access() {
        let fields = [
            DynField::new(0, Layout::new::<u32>()),
            DynField::new(8, Layout::new::<u64>()),
        ];
        let s = DynShape::struct_with_fields(&fields);
        let st = s.as_struct().unwrap();

        assert_eq!(st.field_count(), 2);

        let f0 = st.field(0).unwrap();
        assert_eq!(f0.offset(), 0);
        assert_eq!(f0.shape().layout().size(), 4);

        let f1 = st.field(1).unwrap();
        assert_eq!(f1.offset(), 8);
        assert_eq!(f1.shape().layout().size(), 8);

        assert!(st.field(2).is_none());
    }

    #[test]
    fn struct_layout_calculation() {
        let fields = [
            DynField::new(0, Layout::new::<u8>()),
            DynField::new(8, Layout::new::<u64>()),
        ];
        let s = DynShape::struct_with_fields(&fields);
        assert_eq!(s.layout().size(), 16);
        assert_eq!(s.layout().align(), 8);
    }
}
