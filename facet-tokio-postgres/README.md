# facet-tokio-postgres

Deserialize tokio-postgres Rows into any type implementing Facet.

This crate provides a bridge between tokio-postgres and facet, allowing you to
deserialize database rows directly into Rust structs that implement `Facet`.

Part of the [dibs](https://github.com/bearcove/dibs) project.
