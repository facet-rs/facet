+++
title = "phon"
description = "Typed binary format and execution engine"
+++

# Base concepts

> r[purpose]
> 
> phon is a typed binary format meant to support exchanges between two peers, whose idea of
> the schema may have drifted over time.
>
> it gracefully supports adding fields, removing fields, re-ordering fields, similar evolutions
> with enum variants, etc.

> r[two-forms]
>
> phon has a self-describing mode (no schema required, think JSON), and a compact mode (you
> have to know what you're deserializing, think postcard).
>
> phon schemas are themselves encoded as self-describing phon, which allows bootstrapping
> an exchange.

> r[no-idl]
> 
> phon doesn't have an IDL (Interface Definition Language): phon schemas are
  derived from Rust types, through introspection via the [facet](https://crates.io/crates/facet) crate.
>
> For example, the following Rust type:
>
> ```rust
> #[derive(Facet)]
> struct Point { x: u32, y: f64 }
> ```
>
> Would result in a phon schema of something like (JSON representation):
>
> ```json
> {
>   "type": "struct",
>   "name": "Point",
>   "fields": [
>     { "name": "x", "type": "u32" },
>     { "name": "y", "type": "f64" }
>   ]
> }
> ```

> r[schemas-vs-descriptors]
> 
> Schemas describe what goes on the wire: in the case of the `Point` struct from earlier,
> a `u32` and then an `f64`.
