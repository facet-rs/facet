//! The codegen tool.
//!
//! phon schemas originate from Rust types via facet; codegen turns those schemas
//! into source for the other languages a system speaks. For each target it emits
//! two things: the type definitions a programmer writes against, and the schema
//! itself as a constant — the self-describing phon bytes of the `Schema` value,
//! which the peer parses at startup. A non-Rust peer never re-derives a schema
//! from its generated types; the emitted bytes are the source of truth, so its
//! `SchemaId` matches the Rust origin exactly
//! (`r[codegen.schema-is-source-of-truth]`).
//!
//! This is the *data plane* codegen: types + schemas + ids. The RPC layer
//! (services, clients, dispatchers, channels) is generated separately by the
//! framework on top of this output.
//!
//! Spec: `docs/content/spec.md` — "Codegen".

pub mod source;
pub mod typescript;

pub use source::{Builder, Module, Root};

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;
    use phon_schema::{schema_from_bytes, schema_to_bytes};

    #[derive(Facet)]
    struct Point {
        x: u32,
        y: u32,
    }

    #[derive(Facet)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Shape {
        Circle(f64),
        Rectangle { width: f64, height: f64 },
        Point,
    }

    #[derive(Facet)]
    #[allow(dead_code)]
    struct Person {
        name: String,
        age: u32,
        email: Option<String>,
        tags: Vec<String>,
        home: Point,
        favorite: Shape,
        big: u64,
    }

    fn module() -> Module {
        Builder::new().add::<Person>().build().unwrap()
    }

    #[test]
    // r[verify codegen.schema-is-source-of-truth]
    fn every_schema_byte_blob_round_trips() {
        let m = module();
        assert!(!m.schemas.is_empty());
        for s in &m.schemas {
            let bytes = schema_to_bytes(s);
            let back = schema_from_bytes(&bytes).expect("schema round-trips");
            assert_eq!(&back, s, "schema {:#x} must round-trip", s.id.0);
        }
    }

    #[test]
    // r[verify codegen.emits]
    // r[verify codegen.schema-is-source-of-truth]
    fn typescript_renders_the_expected_shapes() {
        let ts = typescript::render(&module());

        // Struct -> interface with the ergonomic scalar mapping.
        assert!(
            ts.contains("export interface Person {"),
            "missing Person:\n{ts}"
        );
        assert!(ts.contains("name: string;"));
        assert!(ts.contains("age: number;")); // u32 -> number
        assert!(ts.contains("big: bigint;")); // u64 -> bigint
        assert!(ts.contains("email: string | null;")); // Option<String>
        assert!(ts.contains("tags: string[];")); // Vec<String>
        assert!(ts.contains("home: Point;")); // nested struct by name
        assert!(ts.contains("favorite: Shape;")); // enum by name

        // Enum -> { tag, … } discriminated union (unit / newtype / struct variant).
        assert!(ts.contains("export type Shape ="), "missing Shape:\n{ts}");
        assert!(ts.contains(r#"{ tag: "Circle"; value: number }"#)); // newtype f64
        assert!(ts.contains(r#"{ tag: "Rectangle"; width: number; height: number }"#));
        assert!(ts.contains(r#"{ tag: "Point" }"#)); // unit

        // The data plane: a registry built from schema-bytes + a root id constant.
        assert!(ts.contains("export const registry = new Registry("));
        assert!(ts.contains("export const schemaId = {"));
        assert!(ts.contains("Person: 0x"));
    }
}
