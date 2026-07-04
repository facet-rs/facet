//! The value model: scalars unboxed in [`Slot`]s, composites behind
//! [`Handle`]s, every composite's ABI recorded as a [`Layout`] that
//! Rust can introspect.
//!
//! The ruling this implements (constitution A5): types come from Rust
//! or from vix. Rust-authored types are described by facet (bridge
//! lands with the lowering slice). Vix-authored types own their ABI —
//! optimized, enum-shaped, never boxed-forever — but that ABI must be
//! recorded and observable from Rust. [`Layout`] is that record;
//! [`ValueRef`] is that observation.
//!
//! Storage today keeps composite fields as a slot row behind the
//! handle; the recorded layout is the contract, so byte-exact packing
//! (niches, payload overlap) can evolve behind it without touching any
//! consumer. That dial is an implementation choice, not a shape choice
//! — nothing outside this file may depend on how a composite is stored.

use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};

/// One operand word. This is what lives on the machine's operand stack
/// and in a composite's field row: small scalars directly, everything
/// else as a [`Handle`]. `Copy`, totally ordered, hashable — every vix
/// value is (invariant: canonical total order).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Slot {
    Unit,
    Bool(bool),
    I64(i64),
    F64(TotalF64),
    Handle(Handle),
}

/// A reference to a composite in a [`Store`]. Opaque outside this
/// module; meaningful only with the store that issued it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Handle(u32);

/// A registered [`Layout`]'s identity within a [`Registry`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LayoutId(u32);

/// `f64` under the IEEE totalOrder relation, NaN canonicalized at
/// construction so equality, ordering, and hashing agree. The language
/// deliberately trades IEEE comparison semantics for a total order
/// (invariant: every value hashable AND totally ordered).
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

/// The kind of value a field holds, at slot granularity. Typed
/// refinement (which layout a handle field must point at) arrives with
/// the type-checking slice; the vocabulary already leaves room for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotTy {
    Unit,
    Bool,
    I64,
    F64,
    Handle,
}

impl SlotTy {
    fn admits(self, slot: &Slot) -> bool {
        matches!(
            (self, slot),
            (SlotTy::Unit, Slot::Unit)
                | (SlotTy::Bool, Slot::Bool(_))
                | (SlotTy::I64, Slot::I64(_))
                | (SlotTy::F64, Slot::F64(_))
                | (SlotTy::Handle, Slot::Handle(_))
        )
    }
}

/// The recorded ABI of a composite type: the artifact that makes a
/// vix-authored type observable from Rust. A struct is a layout with
/// exactly one variant.
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
    pub ty: SlotTy,
}

/// Where layouts live. Registration order is not identity-bearing;
/// content-addressed layout identity joins with the lowering slice
/// (layouts of vix types hash like everything else the type-closure
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

/// Composite storage. Allocation validates against the recorded layout
/// — a value that disagrees with its own description cannot exist.
#[derive(Debug, Default)]
pub struct Store {
    composites: Vec<Composite>,
}

#[derive(Debug)]
struct Composite {
    layout: LayoutId,
    variant: u32,
    fields: Box<[Slot]>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum LayoutError {
    NoSuchVariant {
        type_name: String,
        variant: String,
    },
    FieldArity {
        type_name: String,
        variant: String,
        expected: usize,
        got: usize,
    },
    FieldKind {
        type_name: String,
        variant: String,
        field: String,
        expected: SlotTy,
        got: Slot,
    },
}

impl fmt::Display for LayoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LayoutError::NoSuchVariant { type_name, variant } => {
                write!(f, "{type_name} has no variant named {variant}")
            }
            LayoutError::FieldArity {
                type_name,
                variant,
                expected,
                got,
            } => write!(
                f,
                "{type_name}::{variant} takes {expected} field(s), got {got}"
            ),
            LayoutError::FieldKind {
                type_name,
                variant,
                field,
                expected,
                got,
            } => write!(
                f,
                "{type_name}::{variant}.{field} expects {expected:?}, got {got:?}"
            ),
        }
    }
}

impl std::error::Error for LayoutError {}

impl Store {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn alloc(
        &mut self,
        registry: &Registry,
        layout: LayoutId,
        variant: &str,
        fields: &[Slot],
    ) -> Result<Handle, LayoutError> {
        let desc = registry.get(layout);
        let (variant_ix, variant_desc) = desc
            .variants
            .iter()
            .enumerate()
            .find(|(_, v)| v.name == variant)
            .ok_or_else(|| LayoutError::NoSuchVariant {
                type_name: desc.name.clone(),
                variant: variant.to_owned(),
            })?;
        if variant_desc.fields.len() != fields.len() {
            return Err(LayoutError::FieldArity {
                type_name: desc.name.clone(),
                variant: variant.to_owned(),
                expected: variant_desc.fields.len(),
                got: fields.len(),
            });
        }
        for (field_desc, slot) in variant_desc.fields.iter().zip(fields) {
            if !field_desc.ty.admits(slot) {
                return Err(LayoutError::FieldKind {
                    type_name: desc.name.clone(),
                    variant: variant.to_owned(),
                    field: field_desc.name.clone(),
                    expected: field_desc.ty,
                    got: *slot,
                });
            }
        }
        let handle = Handle(u32::try_from(self.composites.len()).expect("store fits u32"));
        self.composites.push(Composite {
            layout,
            variant: u32::try_from(variant_ix).expect("variant index fits u32"),
            fields: fields.into(),
        });
        Ok(handle)
    }

    /// Observe a composite through its recorded layout — the
    /// introspection half of the ruling: Rust reads any vix-authored
    /// value without a Rust definition of its type existing anywhere.
    pub fn read<'s>(&'s self, registry: &'s Registry, handle: Handle) -> ValueRef<'s> {
        let composite = &self.composites[handle.0 as usize];
        let layout = registry.get(composite.layout);
        ValueRef {
            layout,
            variant: &layout.variants[composite.variant as usize],
            fields: &composite.fields,
        }
    }
}

/// A composite viewed through its recorded layout.
#[derive(Clone, Copy)]
pub struct ValueRef<'s> {
    layout: &'s Layout,
    variant: &'s Variant,
    fields: &'s [Slot],
}

impl<'s> ValueRef<'s> {
    pub fn type_name(&self) -> &'s str {
        &self.layout.name
    }

    pub fn variant_name(&self) -> &'s str {
        &self.variant.name
    }

    pub fn field(&self, name: &str) -> Option<Slot> {
        self.variant
            .fields
            .iter()
            .position(|f| f.name == name)
            .map(|ix| self.fields[ix])
    }

    pub fn fields(&self) -> impl Iterator<Item = (&'s str, Slot)> + '_ {
        self.variant
            .fields
            .iter()
            .map(|f| f.name.as_str())
            .zip(self.fields.iter().copied())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_stay_small() {
        assert!(
            std::mem::size_of::<Slot>() <= 16,
            "Slot grew past two words: {}",
            std::mem::size_of::<Slot>()
        );
    }

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

    fn verdict_layout(registry: &mut Registry) -> LayoutId {
        registry.register(Layout {
            name: "Verdict".to_owned(),
            variants: vec![
                Variant {
                    name: "Pass".to_owned(),
                    fields: vec![Field {
                        name: "score".to_owned(),
                        ty: SlotTy::I64,
                    }],
                },
                Variant {
                    name: "Fail".to_owned(),
                    fields: vec![
                        Field {
                            name: "code".to_owned(),
                            ty: SlotTy::I64,
                        },
                        Field {
                            name: "cause".to_owned(),
                            ty: SlotTy::Handle,
                        },
                    ],
                },
            ],
        })
    }

    #[test]
    fn vix_authored_layout_is_recorded_and_observable_from_rust() {
        let mut registry = Registry::new();
        let verdict = verdict_layout(&mut registry);
        let mut store = Store::new();

        // No Rust type named Verdict exists anywhere; the recorded
        // layout alone makes the value buildable and readable.
        let pass = store
            .alloc(&registry, verdict, "Pass", &[Slot::I64(42)])
            .expect("valid Pass");
        let fail = store
            .alloc(
                &registry,
                verdict,
                "Fail",
                &[Slot::I64(3), Slot::Handle(pass)],
            )
            .expect("valid Fail");

        let read = store.read(&registry, fail);
        assert_eq!(read.type_name(), "Verdict");
        assert_eq!(read.variant_name(), "Fail");
        assert_eq!(read.field("code"), Some(Slot::I64(3)));

        // Introspection walks handles: observe the nested composite.
        let Some(Slot::Handle(cause)) = read.field("cause") else {
            panic!("cause should be a handle");
        };
        let nested = store.read(&registry, cause);
        assert_eq!(nested.variant_name(), "Pass");
        assert_eq!(nested.field("score"), Some(Slot::I64(42)));
        assert_eq!(
            nested.fields().collect::<Vec<_>>(),
            vec![("score", Slot::I64(42))]
        );
    }

    #[test]
    fn alloc_validates_against_the_recorded_layout() {
        let mut registry = Registry::new();
        let verdict = verdict_layout(&mut registry);
        let mut store = Store::new();

        assert!(matches!(
            store.alloc(&registry, verdict, "Maybe", &[]),
            Err(LayoutError::NoSuchVariant { .. })
        ));
        assert!(matches!(
            store.alloc(&registry, verdict, "Pass", &[]),
            Err(LayoutError::FieldArity { expected: 1, got: 0, .. })
        ));
        assert!(matches!(
            store.alloc(&registry, verdict, "Pass", &[Slot::Bool(true)]),
            Err(LayoutError::FieldKind { .. })
        ));
    }
}
