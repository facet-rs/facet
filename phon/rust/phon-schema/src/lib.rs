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

/// The schema model: `Schema`, `SchemaKind`, `SchemaRef`, `Primitive`,
/// `ChannelDirection`, `Field`, `Variant`, `VariantPayload`, `SchemaId`.
///
/// Spec: "Type system" (`r[type-system.*]`).
pub mod schema {}

/// Content-derived schema identity: the canonical structural encoding and the
/// BLAKE3-based `SchemaId` computation, including SCC-ordered assignment and
/// the depth-indexed back-reference walk for reference cycles.
///
/// Spec: "Schema identity" (`r[schema-identity.*]`).
pub mod identity {}

/// phon's dynamic value — what the self-describing codec produces and consumes,
/// and what a `Dynamic` field carries. Its mapping onto `facet_value::Value`
/// (including the cases facet has that phon doesn't) is the binding's job, not
/// this crate's.
///
/// Spec: "Value" (`r[value]`).
pub mod value {}

/// The self-describing (tag-led) codec: encode/decode a `Value` with no schema.
/// The bootstrap mode, and the backing of the `Dynamic` kind.
///
/// Spec: "Self-describing mode" (`r[self-describing.*]`).
pub mod selfdescribing {}
