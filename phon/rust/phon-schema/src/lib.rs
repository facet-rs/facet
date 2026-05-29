//! phon's wire contract — the most widely depended-on layer, with no engine and
//! no language binding.
//!
//! This crate owns everything portable about a phon schema: the schema model,
//! content-derived schema identity, the dynamic [`value`] type, and the
//! self-describing codec that bootstraps schema exchange. Every other phon crate
//! depends on this one; it depends on nothing phon-specific.
//!
//! Spec: `docs/content/spec.md` — "Type system", "Schema identity",
//! "Self-describing mode", and `r[crates.concern-separation]`.

pub mod identity;
pub mod schema;

/// phon's dynamic value. In Rust this *is* `facet_value::Value`, re-exported
/// rather than wrapped — a `Dynamic` field carries one directly. The
/// self-describing codec maps the cases facet carries beyond the wire tag table
/// (null, date/time, qname, uuid) onto phon kinds.
///
/// Spec: "Value" (`r[value]`).
pub mod value {
    pub use facet_value::Value;
}

/// The self-describing (tag-led) codec: encode/decode a `Value` with no schema.
/// The bootstrap mode, and the backing of the `Dynamic` kind.
///
/// Spec: "Self-describing mode" (`r[self-describing.*]`).
pub mod selfdescribing {}

pub use identity::{primitive_id, resolve_ids};
pub use schema::{
    ChannelDirection, Field, Primitive, Schema, SchemaId, SchemaKind, SchemaRef, Variant,
    VariantPayload,
};
pub use value::Value;
