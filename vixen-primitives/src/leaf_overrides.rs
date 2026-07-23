//! The embedder-supplied leaf-override table for [`vix::vir::Type::from_facet_with`].
//!
//! `vix-core`'s facet-shape → `vir::Type` walker cannot read the wire `Type` of
//! the **domain** handles off their `Facet` shape: a `BlobHandle` is a `ValueId`
//! newtype but wires as `Extern(Blob)`, a `RegistryHandle` wires as
//! `Extern(Registry)`, and an `UpstreamDigest` wires as a hex `String`. These
//! are domain facts the bare language should not hardcode, so the compiler
//! exposes a [`LeafOverrideDecl`] seam and this crate — which owns the domain —
//! supplies the mappings through [`LEAF_OVERRIDES`].
//!
//! Call [`vix::vir::Type::from_facet_with::<T>`] with [`LEAF_OVERRIDES`] wherever
//! a domain wire type's `Type`/schema is derived, so the mapping travels with the
//! host crate rather than the core walker.

use vix::runtime::{BlobHandle, RegistryHandle, UpstreamDigest};
use vix::vir::{ExternKind, LeafOverrideDecl, Type};

/// The domain wire-type leaf overrides this crate injects into
/// [`vix::vir::Type::from_facet_with`]. Each entry maps a Rust newtype's nominal
/// identity to the vix `Type` it wires as — the facts the core walker used to
/// hardcode by name and now receives from the embedder.
pub const LEAF_OVERRIDES: &[LeafOverrideDecl] = &[
    // A served Blob handle is already an interned `Extern(Blob)` store value; its
    // typed `Type` must be bit-for-bit `Extern(Blob)`.
    LeafOverrideDecl {
        type_id: facet::ConstTypeId::of::<BlobHandle>(),
        ty: || Type::Extern(ExternKind::Blob),
    },
    // A registry capability handle: a `ValueId` reused for several extern kinds,
    // disambiguated by its newtype so the wire meaning is `Extern(Registry)`.
    LeafOverrideDecl {
        type_id: facet::ConstTypeId::of::<RegistryHandle>(),
        ty: || Type::Extern(ExternKind::Registry),
    },
    // A fixed 32-byte upstream digest has no `Type` primitive; it wire-encodes as
    // a hex `String`.
    LeafOverrideDecl {
        type_id: facet::ConstTypeId::of::<UpstreamDigest>(),
        ty: || Type::String,
    },
];

#[cfg(test)]
mod tests {
    use super::LEAF_OVERRIDES;
    use vix::runtime::{
        BlobHandle, OriginHint, PinnedBlobRef, PinnedFetchRequest, RegistryHandle, UpstreamDigest,
    };
    use vix::vir::{ExternKind, Type};

    /// The injected table reproduces, for each domain handle, exactly the `Type`
    /// the core walker hardcodes today — so a future move that deletes the
    /// hardcoded domain entries and relies solely on this table stays
    /// byte-identical (`from_facet` and `from_facet_with` agree while both paths
    /// coexist).
    #[test]
    fn injected_overrides_match_the_core_leaf_table() {
        assert_eq!(
            Type::from_facet_with::<BlobHandle>(LEAF_OVERRIDES),
            Type::from_facet::<BlobHandle>(),
        );
        assert_eq!(
            Type::from_facet_with::<BlobHandle>(LEAF_OVERRIDES),
            Type::Extern(ExternKind::Blob),
        );

        assert_eq!(
            Type::from_facet_with::<RegistryHandle>(LEAF_OVERRIDES),
            Type::from_facet::<RegistryHandle>(),
        );
        assert_eq!(
            Type::from_facet_with::<RegistryHandle>(LEAF_OVERRIDES),
            Type::Extern(ExternKind::Registry),
        );

        assert_eq!(
            Type::from_facet_with::<UpstreamDigest>(LEAF_OVERRIDES),
            Type::from_facet::<UpstreamDigest>(),
        );
        assert_eq!(
            Type::from_facet_with::<UpstreamDigest>(LEAF_OVERRIDES),
            Type::String,
        );
    }

    /// The overrides must fire through nested walks, not just at the root: a
    /// request struct that embeds the domain handles resolves identically whether
    /// the mapping comes from the injected table or the core's own fallback.
    #[test]
    fn injected_overrides_match_through_nested_records() {
        for pair in [
            (
                Type::from_facet_with::<OriginHint>(LEAF_OVERRIDES),
                Type::from_facet::<OriginHint>(),
            ),
            (
                Type::from_facet_with::<PinnedBlobRef>(LEAF_OVERRIDES),
                Type::from_facet::<PinnedBlobRef>(),
            ),
            (
                Type::from_facet_with::<PinnedFetchRequest>(LEAF_OVERRIDES),
                Type::from_facet::<PinnedFetchRequest>(),
            ),
        ] {
            assert_eq!(pair.0, pair.1);
        }
    }
}
