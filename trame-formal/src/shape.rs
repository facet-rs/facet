//! Shape descriptors for formal verification.
//!
//! Unlike `facet_core::Shape` which uses static references and can be recursive,
//! these shapes are bounded and can implement `kani::Arbitrary`.

/// Maximum nesting depth for shapes.
pub const MAX_DEPTH: usize = 3;

/// Maximum number of fields in a struct.
pub const MAX_FIELDS: usize = 8;

/// A shape descriptor that can be used with Kani.
///
/// This is a bounded representation of type structure that can be
/// generated arbitrarily for verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeDesc {
    /// A scalar type (no internal structure to track).
    Scalar,
    /// A struct with N fields.
    Struct { field_count: u8 },
    // TODO: Enum, Option, Result, List, Map, etc.
}

impl ShapeDesc {
    /// Number of slots needed for this shape (at the top level).
    pub fn slot_count(&self) -> usize {
        match self {
            ShapeDesc::Scalar => 1,
            ShapeDesc::Struct { field_count } => *field_count as usize,
        }
    }
}

#[cfg(kani)]
impl kani::Arbitrary for ShapeDesc {
    fn any() -> Self {
        let variant: u8 = kani::any();
        kani::assume(variant < 2);

        match variant {
            0 => ShapeDesc::Scalar,
            1 => {
                let field_count: u8 = kani::any();
                kani::assume(field_count > 0 && field_count <= MAX_FIELDS as u8);
                ShapeDesc::Struct { field_count }
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
        assert_eq!(ShapeDesc::Scalar.slot_count(), 1);
    }

    #[test]
    fn struct_has_field_count_slots() {
        assert_eq!(ShapeDesc::Struct { field_count: 3 }.slot_count(), 3);
    }
}
