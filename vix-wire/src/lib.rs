//! The `vix::` facet extension-attribute grammar — the facet-native seam for
//! declaring how a Rust wire type maps onto the vix type system.
//!
//! A wire type annotates itself instead of being registered in a table:
//!
//! ```ignore
//! #[derive(facet::Facet)]
//! #[facet(vix::wire_extern = "Registry")]
//! pub struct RegistryHandle(pub ValueId);
//! ```
//!
//! `vix::vir::Type::from_facet` honors the attribute wherever the shape
//! appears, so the mapping travels with the type definition. This is the seam
//! that lets an embedder introduce a new host-typed handle without touching
//! `vix-core` (issue 2520): any name that is not a core extern maps to
//! `Extern(Host(name))`, and the walker's rule is generic — there is no
//! per-type table in the core and no injected override list.
//!
//! This grammar lives in its own crate (re-exported at the `vix` root) because
//! the macros `define_attr_grammar!` generates are `#[macro_export]`ed through
//! a macro expansion, and Rust forbids referring to such macros by absolute
//! path (`vix::…`) from inside the crate that expanded them — while `vix-core`
//! annotates its own wire types (`RegistryHandle`, `BlobHandle`, `SchemaRef`).
//! The same layout `dibs` / `dibs-db-schema` uses.

facet::define_attr_grammar! {
    ns "vix";
    crate_path ::vix;

    /// Vix wire-mapping attributes.
    pub enum Attr {
        /// This type wires as the named vix extern: an axiom extern (`Blob`,
        /// `Registry`, `PinnedUrl`, `Schema`) or, for any other name, the
        /// declared host type `Extern(Host(name))`.
        ///
        /// Usage: `#[facet(vix::wire_extern = "Registry")]`
        WireExtern(&'static str),
    }
}
