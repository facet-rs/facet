//! Shape abstractions for formal verification.
//!
//! This module provides:
//! - `ShapeDesc`: bounded synthetic shapes for Kani proofs
//!
//! The key insight is that trame needs from shapes:
//! - Layout (size, align) for allocation
//! - Field count for structs
//! - Field offsets for pointer math
//! - Drop/default vtable functions
//!
//! For verification, we don't need the vtable functions - we just track state transitions.
//! What matters is the structure: how many slots (fields) need tracking.

use core::alloc::Layout;

/// Maximum number of fields in a struct (for bounded verification).
pub const MAX_FIELDS: usize = 8;

/// Information about a single field within a struct.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldInfo {
    /// Byte offset of this field within the struct.
    pub offset: usize,
    /// Layout (size and alignment) of this field.
    pub layout: Layout,
}

impl FieldInfo {
    /// Create a new field info.
    pub const fn new(offset: usize, layout: Layout) -> Self {
        Self { offset, layout }
    }
}

/// A bounded shape descriptor for Kani verification.
///
/// Unlike `facet_core::Shape` which uses static references and can be recursive,
/// these shapes are bounded and can implement `kani::Arbitrary`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeDesc {
    /// A scalar type (no internal structure to track).
    /// Examples: u32, String, any opaque type.
    Scalar(Layout),

    /// A struct with indexed fields.
    /// Each field is a slot that needs independent init tracking.
    Struct {
        /// Layout of the whole struct.
        layout: Layout,
        /// Number of fields/slots.
        field_count: u8,
        /// Field information (only first `field_count` entries are valid).
        fields: [FieldInfo; MAX_FIELDS],
    },
    // TODO: Enum, Option, Result, List, Map, etc.
}

impl ShapeDesc {
    /// Create a scalar shape with the given layout.
    pub const fn scalar(layout: Layout) -> Self {
        Self::Scalar(layout)
    }

    /// Create a struct shape with the given fields.
    ///
    /// Calculates the overall layout from the fields.
    pub fn struct_with_fields(fields: &[FieldInfo]) -> Self {
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

        let mut field_array = [FieldInfo::new(0, Layout::new::<()>()); MAX_FIELDS];
        for (i, f) in fields.iter().enumerate() {
            field_array[i] = *f;
        }

        Self::Struct {
            layout,
            field_count: fields.len() as u8,
            fields: field_array,
        }
    }

    /// Get the layout of this shape.
    pub const fn layout(&self) -> Layout {
        match self {
            Self::Scalar(layout) => *layout,
            Self::Struct { layout, .. } => *layout,
        }
    }

    /// Number of slots (fields) this shape has.
    ///
    /// - Scalars have 1 slot (the whole value)
    /// - Structs have N slots (one per field)
    pub const fn slot_count(&self) -> usize {
        match self {
            Self::Scalar(_) => 1,
            Self::Struct { field_count, .. } => *field_count as usize,
        }
    }

    /// Get field info by index.
    ///
    /// Returns `Some(&FieldInfo)` if index is valid, `None` otherwise.
    pub const fn field(&self, idx: usize) -> Option<&FieldInfo> {
        match self {
            Self::Scalar(_) => None,
            Self::Struct {
                field_count,
                fields,
                ..
            } => {
                if idx < *field_count as usize {
                    Some(&fields[idx])
                } else {
                    None
                }
            }
        }
    }

    /// Check if this is a struct (has multiple fields).
    pub const fn is_struct(&self) -> bool {
        matches!(self, Self::Struct { .. })
    }
}

#[cfg(kani)]
impl kani::Arbitrary for ShapeDesc {
    fn any() -> Self {
        let variant: u8 = kani::any();
        kani::assume(variant < 2);

        match variant {
            0 => {
                // Scalar with reasonable size/align
                let size: usize = kani::any();
                let align_pow: u8 = kani::any();
                kani::assume(size <= 64);
                kani::assume(align_pow <= 3); // align up to 8
                let align = 1usize << align_pow;
                // Size must be zero or a multiple of align for valid Layout
                kani::assume(size == 0 || size % align == 0);
                let layout = Layout::from_size_align(size, align).unwrap();
                ShapeDesc::Scalar(layout)
            }
            1 => {
                // Struct with 1-4 fields
                let field_count: u8 = kani::any();
                kani::assume(field_count > 0 && field_count <= 4);

                let mut fields = [FieldInfo::new(0, Layout::new::<()>()); MAX_FIELDS];

                let mut offset = 0usize;
                for i in 0..(field_count as usize) {
                    let field_size: usize = kani::any();
                    kani::assume(field_size > 0 && field_size <= 8);

                    // All fields use align=1 for simplicity in verification
                    let layout = Layout::from_size_align(field_size, 1).unwrap();
                    fields[i] = FieldInfo::new(offset, layout);
                    offset += field_size;
                }

                // Keep total size bounded
                kani::assume(offset <= 64);

                let layout = Layout::from_size_align(offset, 1).unwrap();

                ShapeDesc::Struct {
                    layout,
                    field_count,
                    fields,
                }
            }
            _ => unreachable!(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalar_has_one_slot() {
        let s = ShapeDesc::scalar(Layout::new::<u32>());
        assert_eq!(s.slot_count(), 1);
        assert_eq!(s.layout().size(), 4);
        assert_eq!(s.layout().align(), 4);
    }

    #[test]
    fn struct_has_field_count_slots() {
        let fields = [
            FieldInfo::new(0, Layout::new::<u32>()),
            FieldInfo::new(4, Layout::new::<u32>()),
            FieldInfo::new(8, Layout::new::<u32>()),
        ];
        let s = ShapeDesc::struct_with_fields(&fields);
        assert_eq!(s.slot_count(), 3);
    }

    #[test]
    fn struct_field_access() {
        let fields = [
            FieldInfo::new(0, Layout::new::<u32>()),
            FieldInfo::new(8, Layout::new::<u64>()),
        ];
        let s = ShapeDesc::struct_with_fields(&fields);

        let f0 = s.field(0).unwrap();
        assert_eq!(f0.offset, 0);
        assert_eq!(f0.layout.size(), 4);

        let f1 = s.field(1).unwrap();
        assert_eq!(f1.offset, 8);
        assert_eq!(f1.layout.size(), 8);

        assert!(s.field(2).is_none());
    }

    #[test]
    fn struct_layout_calculation() {
        // u8 at 0, u64 at 8 (with padding)
        let fields = [
            FieldInfo::new(0, Layout::new::<u8>()),
            FieldInfo::new(8, Layout::new::<u64>()),
        ];
        let s = ShapeDesc::struct_with_fields(&fields);
        assert_eq!(s.layout().size(), 16); // 8 + 8
        assert_eq!(s.layout().align(), 8);
    }

    #[test]
    fn scalar_has_no_fields() {
        let s = ShapeDesc::scalar(Layout::new::<u64>());
        assert!(s.field(0).is_none());
        assert!(s.field(1).is_none());
    }
}
