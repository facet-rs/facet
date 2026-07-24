//! The facet-native wire seam: a Rust type annotated
//! `#[facet(vix::wire_extern = "…")]` declares — on its own shape — which vix
//! extern it wires as. The walker's rule is generic: a core spelling maps to
//! the axiom extern, any other name to the declared host type `Host(name)`.
//! No table in `vix-core`, no injected override list — this is how an embedder
//! introduces a new host-typed handle without touching the language core
//! (issue 2520).

use facet::Facet;
use vix::vir::{ExternKind, Type};

/// Stands in for an embedder-owned handle of a *new* host type the core has
/// never heard of — the walker maps it by the generic rule alone.
#[derive(Facet)]
#[facet(vix::wire_extern = "Tree")]
struct EmbedderTreeHandle(u64);

/// A core extern spelling maps to the axiom extern, not a host type.
#[derive(Facet)]
#[facet(vix::wire_extern = "Blob")]
struct EmbedderBlobHandle(u64);

/// An annotated field inside an ordinary request record is honored mid-walk,
/// exactly how a primitive contract struct uses the seam.
#[derive(Facet)]
struct EmbedderRequest {
    tree: EmbedderTreeHandle,
    pattern: String,
}

#[test]
fn wire_extern_maps_a_host_name_to_the_declared_host_type() {
    assert_eq!(
        Type::from_facet::<EmbedderTreeHandle>(),
        Type::Extern(ExternKind::Host("Tree")),
    );
}

#[test]
fn wire_extern_maps_a_core_spelling_to_the_axiom_extern() {
    assert_eq!(
        Type::from_facet::<EmbedderBlobHandle>(),
        Type::Extern(ExternKind::Blob),
    );
}

#[test]
fn wire_extern_is_honored_inside_a_walked_record() {
    let Type::Record(record) = Type::from_facet::<EmbedderRequest>() else {
        panic!("a struct walks to a record");
    };
    assert_eq!(record.name, "EmbedderRequest");
    assert_eq!(
        record.fields[0].ty,
        Type::Extern(ExternKind::Host("Tree")),
        "the field's annotation travels with its type"
    );
    assert_eq!(record.fields[1].ty, Type::String);
}
