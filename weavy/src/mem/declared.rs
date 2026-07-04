//! Layout computation for LANGUAGE-DECLARED types — the second
//! authority of the two-authority model.
//!
//! Rust-authored types reach [`Descriptor`] from facet shapes, where
//! rustc already decided the layout. Types declared in a language ON
//! this substrate (fable, vix) have no rustc: the language's checker
//! hands this module the field/variant structure (as descriptors, in
//! declaration order) and THIS module decides the ABI — deterministic
//! field packing, alignment, explicit smallest-fitting tags — and
//! records it truthfully in the same descriptor vocabulary everything
//! else consumes. Consumers never know which authority produced a
//! descriptor; nesting works in both directions by construction (a
//! declared struct may contain a facet-described field and vice
//! versa, because both are just descriptors).
//!
//! Rules of the house:
//! - DETERMINISTIC: identical inputs yield identical layouts; layout
//!   is part of a declared type's content-addressed identity.
//! - RECORDED, so the strategy may evolve: today fields pack by
//!   descending alignment (stable on declaration order) and enums use
//!   an explicit leading tag; niche packing later. No consumer may
//!   assume the strategy — only the recorded offsets/tags.
//! - Field and variant NAMES are the checker's business (its symbol
//!   tables map names to indices); descriptors speak indices only.
//! - Everything is [`Construct::InPlace`]: declared values live in
//!   language-managed memory; no Rust-side thunks exist for them.

use super::{
    Access, Construct, Descriptor, EnumAccess, FieldAccess, Layout, RecordAccess,
    RecordByteOwnership, Tag, VariantAccess,
};

/// A scalar descriptor with the given process-local size/alignment.
/// Conveniences below cover the usual suspects.
#[must_use]
pub fn scalar<SchemaRef>(schema: SchemaRef, size: usize, align: usize) -> Descriptor<SchemaRef> {
    Descriptor {
        schema,
        layout: Layout { size, align },
        access: Access::Scalar,
    }
}

#[must_use]
pub fn unit<SchemaRef>(schema: SchemaRef) -> Descriptor<SchemaRef> {
    scalar(schema, 0, 1)
}

#[must_use]
pub fn bool_<SchemaRef>(schema: SchemaRef) -> Descriptor<SchemaRef> {
    scalar(schema, 1, 1)
}

#[must_use]
pub fn i64_<SchemaRef>(schema: SchemaRef) -> Descriptor<SchemaRef> {
    scalar(schema, 8, 8)
}

#[must_use]
pub fn f64_<SchemaRef>(schema: SchemaRef) -> Descriptor<SchemaRef> {
    scalar(schema, 8, 8)
}

/// An INLINE fixed-count array: `count` elements, `stride` apart,
/// living wherever the descriptor is placed (a frame, a struct field)
/// — unboxed, part of the containing layout's bytes.
#[must_use]
pub fn array_of<SchemaRef>(
    schema: SchemaRef,
    element: Descriptor<SchemaRef>,
    count: usize,
) -> Descriptor<SchemaRef> {
    let align = element.layout.align.max(1);
    let stride = align_up(element.layout.size, align);
    Descriptor {
        schema,
        layout: Layout {
            size: stride * count,
            align,
        },
        access: Access::Array {
            element: Box::new(element),
            count,
            stride,
        },
    }
}

/// Lay out a declared struct. `fields` are the field descriptors in
/// DECLARATION order; the returned record's `FieldAccess` vec keeps
/// that order (field index = declaration index, forever), while the
/// recorded OFFSETS reflect packing. Byte ownership carries full
/// field-and-padding proofs.
#[must_use]
pub fn declared_struct<SchemaRef>(
    schema: SchemaRef,
    fields: Vec<Descriptor<SchemaRef>>,
) -> Descriptor<SchemaRef> {
    let packed = pack(&fields.iter().map(|f| f.layout).collect::<Vec<_>>(), 0);
    let layout = Layout {
        size: align_up(packed.end, packed.align),
        align: packed.align,
    };
    let field_accesses: Vec<FieldAccess<SchemaRef>> = fields
        .into_iter()
        .zip(&packed.offsets)
        .map(|(descriptor, &offset)| FieldAccess {
            offset,
            descriptor,
            default: None,
        })
        .collect();
    let byte_ownership = RecordByteOwnership::from_record_layout(layout, &field_accesses);
    Descriptor {
        schema,
        layout,
        access: Access::Record(RecordAccess {
            fields: field_accesses,
            byte_ownership,
            construct: Construct::InPlace,
        }),
    }
}

/// Lay out a declared enum. `variants` are per-variant field lists in
/// DECLARATION order. Strategy (recorded, not assumed): an explicit
/// integer tag at offset 0, smallest width that fits the variant
/// count; every variant's payload starts at one shared, maximally-
/// aligned base after the tag. Variant selectors are the declaration
/// indices. Variant payloads use fields-only byte ownership — tag
/// bytes and sibling-variant bytes stay UNKNOWN to consumers (never
/// falsely provable as padding).
///
/// A zero-variant enum is uninhabited: it gets a zero-width tag and a
/// zero-size layout, and no value of it can ever exist.
#[must_use]
pub fn declared_enum<SchemaRef>(
    schema: SchemaRef,
    variants: Vec<Vec<Descriptor<SchemaRef>>>,
) -> Descriptor<SchemaRef> {
    if variants.is_empty() {
        return Descriptor {
            schema,
            layout: Layout { size: 0, align: 1 },
            access: Access::Enum(EnumAccess {
                tag: Tag::Direct {
                    offset: 0,
                    width: 0,
                },
                variants: Vec::new(),
            }),
        };
    }

    let tag_width = tag_width_for(variants.len());
    let payload_align = variants
        .iter()
        .flat_map(|fields| fields.iter().map(|f| f.layout.align))
        .max()
        .unwrap_or(1);
    let payload_base = align_up(tag_width, payload_align);
    let enum_align = tag_width.max(payload_align);

    let mut max_end = tag_width;
    let variant_accesses: Vec<VariantAccess<SchemaRef>> = variants
        .into_iter()
        .enumerate()
        .map(|(index, fields)| {
            let packed = pack(
                &fields.iter().map(|f| f.layout).collect::<Vec<_>>(),
                payload_base,
            );
            max_end = max_end.max(packed.end);
            let field_accesses: Vec<FieldAccess<SchemaRef>> = fields
                .into_iter()
                .zip(&packed.offsets)
                .map(|(descriptor, &offset)| FieldAccess {
                    offset,
                    descriptor,
                    default: None,
                })
                .collect();
            let byte_ownership = RecordByteOwnership::fields_only(&field_accesses);
            VariantAccess {
                index: u32::try_from(index).expect("variant count fits u32"),
                selector: index as u64,
                payload: RecordAccess {
                    fields: field_accesses,
                    byte_ownership,
                    construct: Construct::InPlace,
                },
            }
        })
        .collect();

    Descriptor {
        schema,
        layout: Layout {
            size: align_up(max_end, enum_align),
            align: enum_align,
        },
        access: Access::Enum(EnumAccess {
            tag: Tag::Direct {
                offset: 0,
                width: tag_width,
            },
            variants: variant_accesses,
        }),
    }
}

/// Smallest power-of-two byte width whose unsigned range covers
/// `variant_count` distinct selectors.
#[must_use]
pub fn tag_width_for(variant_count: usize) -> usize {
    if variant_count <= 1 << 8 {
        1
    } else if variant_count <= 1 << 16 {
        2
    } else if variant_count <= 1 << 32 {
        4
    } else {
        8
    }
}

struct Packed {
    /// Byte offset per field, in DECLARATION order.
    offsets: Vec<usize>,
    /// One past the last occupied byte (before final alignment).
    end: usize,
    /// The packing's alignment requirement (at least 1).
    align: usize,
}

/// Deterministic packing: place fields in descending alignment order
/// (stable on declaration index), each at its natural alignment,
/// starting at `base`. Zero-sized fields land at `base`.
fn pack(layouts: &[Layout], base: usize) -> Packed {
    let mut order: Vec<usize> = (0..layouts.len()).collect();
    order.sort_by_key(|&i| (std::cmp::Reverse(layouts[i].align), i));

    let mut offsets = vec![0usize; layouts.len()];
    let mut cursor = base;
    let mut align = 1usize;
    for &i in &order {
        let l = layouts[i];
        align = align.max(l.align);
        if l.size == 0 {
            offsets[i] = base;
            continue;
        }
        let offset = align_up(cursor, l.align.max(1));
        offsets[i] = offset;
        cursor = offset + l.size;
    }
    Packed {
        offsets,
        end: cursor,
        align,
    }
}

fn align_up(value: usize, align: usize) -> usize {
    let align = align.max(1);
    value.div_ceil(align) * align
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mem::ByteOwner;

    fn offsets<S>(descriptor: &Descriptor<S>) -> Vec<usize> {
        match &descriptor.access {
            Access::Record(record) => record.fields.iter().map(|f| f.offset).collect(),
            _ => panic!("expected a record"),
        }
    }

    #[test]
    fn structs_pack_by_descending_alignment_with_stable_declaration_indices() {
        // Declared as (i64, bool, i64, bool): naive declaration-order
        // layout would pad to 32 bytes; packing reaches 24 while field
        // INDICES stay declaration-ordered.
        let s = declared_struct((), vec![i64_(()), bool_(()), i64_(()), bool_(())]);
        assert_eq!(offsets(&s), vec![0, 16, 8, 17]);
        assert_eq!(s.layout, Layout { size: 24, align: 8 });
    }

    #[test]
    fn struct_padding_is_proven_not_assumed() {
        let s = declared_struct((), vec![i64_(()), bool_(())]);
        assert_eq!(s.layout, Layout { size: 16, align: 8 });
        let Access::Record(record) = &s.access else {
            panic!("record expected");
        };
        // Bytes 9..16 are provably padding; fields own their ranges.
        assert!(record.byte_ownership.is_padding_range(9, 7));
        assert!(!record.byte_ownership.is_padding_range(0, 9));
        assert_eq!(
            record
                .byte_ownership
                .ranges
                .iter()
                .filter(|r| r.owner == ByteOwner::Padding)
                .count(),
            1
        );
    }

    #[test]
    fn zero_sized_fields_cost_nothing() {
        let s = declared_struct((), vec![unit(()), bool_(()), unit(())]);
        assert_eq!(s.layout, Layout { size: 1, align: 1 });
        assert_eq!(offsets(&s), vec![0, 0, 0]);
    }

    #[test]
    fn enums_lead_with_the_smallest_fitting_tag() {
        // Three variants: (), (i64), (bool, bool).
        let e = declared_enum((), vec![
            vec![],
            vec![i64_(())],
            vec![bool_(()), bool_(())],
        ]);
        assert_eq!(e.layout, Layout { size: 16, align: 8 });
        let Access::Enum(access) = &e.access else {
            panic!("enum expected");
        };
        assert!(matches!(access.tag, Tag::Direct { offset: 0, width: 1 }));
        assert_eq!(access.variants.len(), 3);
        // Payloads share the aligned base after the tag.
        let v1 = &access.variants[1];
        assert_eq!(v1.selector, 1);
        assert_eq!(v1.payload.fields[0].offset, 8);
        let v2 = &access.variants[2];
        assert_eq!(v2.payload.fields[0].offset, 8);
        assert_eq!(v2.payload.fields[1].offset, 9);
    }

    #[test]
    fn enum_variant_payloads_never_prove_tag_bytes_as_padding() {
        let e = declared_enum((), vec![vec![i64_(())], vec![]]);
        let Access::Enum(access) = &e.access else {
            panic!("enum expected");
        };
        // fields_only ownership: the tag region stays UNKNOWN to
        // optimizers looking through a variant's payload record.
        assert!(!access.variants[0].payload.byte_ownership.is_padding_range(0, 1));
        assert!(!access.variants[1].payload.byte_ownership.is_padding_range(0, 1));
    }

    #[test]
    fn tag_widths_grow_with_variant_count() {
        assert_eq!(tag_width_for(1), 1);
        assert_eq!(tag_width_for(256), 1);
        assert_eq!(tag_width_for(257), 2);
        assert_eq!(tag_width_for(1 << 16), 2);
        assert_eq!(tag_width_for((1 << 16) + 1), 4);
    }

    #[test]
    fn declared_types_nest_in_both_directions() {
        // A declared struct used as a field of another declared struct:
        // interop is by-construction because everything is descriptors.
        let inner = declared_struct((), vec![bool_(()), i64_(())]);
        assert_eq!(inner.layout, Layout { size: 16, align: 8 });
        let outer = declared_struct((), vec![bool_(()), inner]);
        assert_eq!(outer.layout, Layout { size: 24, align: 8 });
        // The 16-byte inner (align 8) packs first at 0; the bool lands
        // after it (declaration index preserved in the access order).
        assert_eq!(offsets(&outer), vec![16, 0]);
    }

    #[test]
    fn uninhabited_enums_are_zero_sized() {
        let e = declared_enum((), Vec::<Vec<Descriptor<()>>>::new());
        assert_eq!(e.layout, Layout { size: 0, align: 1 });
    }

    #[test]
    fn layouts_are_deterministic() {
        let a = declared_struct((), vec![i64_(()), bool_(()), f64_(())]);
        let b = declared_struct((), vec![i64_(()), bool_(()), f64_(())]);
        assert_eq!(offsets(&a), offsets(&b));
        assert_eq!(a.layout, b.layout);
    }
}
