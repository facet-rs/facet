//! Layout descriptions and value-semantic primitives.
//!
//! The two-authority ruling (constitution A5/A6): a type defined in
//! Rust is described by facet; a type defined in vix owns an optimized
//! ABI — recorded here as a [`Layout`] so Rust can introspect any vix
//! value without a Rust definition of its type existing anywhere.
//! Layouts are COMPILE-TIME knowledge and debug-side metadata: lowering
//! consults them to pick instructions and offsets; running code never
//! dispatches on them. There are no tagged operands anywhere in this
//! machine (A6) — a sum type's discriminant lives in its own layout,
//! matched by code that statically knows the type.
//!
//! Byte-exact packing (niches, payload overlap — "what a Rust enum
//! would do") is computed by the lowering slice and recorded per
//! variant/field; the description vocabulary below is what it fills
//! in. Field kinds name compile-time storage classes, not runtime
//! checks.

use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};

/// `f64` under the IEEE totalOrder relation, NaN canonicalized at
/// construction so equality, ordering, and hashing agree. The language
/// deliberately trades IEEE comparison semantics for a total order
/// (invariant: every vix value is hashable AND totally ordered).
#[derive(Clone, Copy)]
pub struct TotalF64(f64);

impl TotalF64 {
    pub fn new(value: f64) -> Self {
        Self(if value.is_nan() { f64::NAN } else { value })
    }

    pub fn get(self) -> f64 {
        self.0
    }
}

impl fmt::Debug for TotalF64 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl PartialEq for TotalF64 {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_bits() == other.0.to_bits()
    }
}

impl Eq for TotalF64 {}

impl PartialOrd for TotalF64 {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TotalF64 {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.total_cmp(&other.0)
    }
}

impl Hash for TotalF64 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.0.to_bits());
    }
}

/// A registered [`Layout`]'s identity within a [`Registry`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LayoutId(u32);

/// The compile-time storage class of a field. This is TYPE information
/// consumed by lowering (instruction selection, offsets) and by
/// introspection — never consulted by running code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind {
    Unit,
    Bool,
    I64,
    F64,
    /// A reference to another composite (tree, closure, string,
    /// user type) — typed refinement (WHICH layout it must point at)
    /// arrives with the checking slice.
    Ref,
}

/// The recorded ABI of a composite type: the artifact that makes a
/// vix-authored type observable from Rust. A struct is a layout with
/// exactly one variant; an enum's discriminant is part of THIS record,
/// i.e. of the type itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout {
    pub name: String,
    pub variants: Vec<Variant>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Variant {
    pub name: String,
    pub fields: Vec<Field>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    pub kind: FieldKind,
}

/// Where layouts live. Registration order is not identity-bearing;
/// content-addressed layout identity joins with the lowering slice
/// (a vix type's layout hashes like everything else the type-closure
/// hash covers).
#[derive(Debug, Default)]
pub struct Registry {
    layouts: Vec<Layout>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, layout: Layout) -> LayoutId {
        let id = LayoutId(u32::try_from(self.layouts.len()).expect("layout count fits u32"));
        self.layouts.push(layout);
        id
    }

    pub fn get(&self, id: LayoutId) -> &Layout {
        &self.layouts[id.0 as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floats_are_totally_ordered_and_hash_consistently() {
        let mut values: Vec<TotalF64> = [f64::NAN, 1.0, -0.0, 0.0, -1.0]
            .into_iter()
            .map(TotalF64::new)
            .collect();
        values.sort();
        let ordered: Vec<f64> = values.iter().map(|v| v.get()).collect();
        assert_eq!(ordered[0], -1.0);
        assert_eq!(ordered[1].to_bits(), (-0.0f64).to_bits());
        assert_eq!(ordered[2].to_bits(), 0.0f64.to_bits());
        assert_eq!(ordered[3], 1.0);
        assert!(ordered[4].is_nan());

        // NaN equals NaN after canonicalization: memo keys must agree.
        assert_eq!(TotalF64::new(f64::NAN), TotalF64::new(-f64::NAN));
    }

    #[test]
    fn vix_authored_layouts_are_recorded_and_introspectable() {
        let mut registry = Registry::new();
        // No Rust type named Verdict exists anywhere; this record IS
        // the type's ABI as far as Rust observation is concerned.
        let id = registry.register(Layout {
            name: "Verdict".to_owned(),
            variants: vec![
                Variant {
                    name: "Pass".to_owned(),
                    fields: vec![Field {
                        name: "score".to_owned(),
                        kind: FieldKind::I64,
                    }],
                },
                Variant {
                    name: "Fail".to_owned(),
                    fields: vec![
                        Field {
                            name: "code".to_owned(),
                            kind: FieldKind::I64,
                        },
                        Field {
                            name: "cause".to_owned(),
                            kind: FieldKind::Ref,
                        },
                    ],
                },
            ],
        });

        let layout = registry.get(id);
        assert_eq!(layout.name, "Verdict");
        assert_eq!(layout.variants.len(), 2);
        let fail = &layout.variants[1];
        assert_eq!(fail.name, "Fail");
        assert_eq!(fail.fields[1].name, "cause");
        assert_eq!(fail.fields[1].kind, FieldKind::Ref);
    }
}
