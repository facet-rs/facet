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
> a `u32` and then an `f64`. It does not tell us how a `Point` value is represented in memory.
>
> In Rust, it might be represented as:
>
> ```rust
> struct Point { x: u32, y: f64 }
>
> // print-type-size type: `Point`: 16 bytes, alignment: 8 bytes
> // print-type-size     field `.y`: 8 bytes
> // print-type-size     field `.x`: 4 bytes
> // print-type-size     end padding: 4 bytes
> ```
>
> Or perhaps:
>
> ```rust
> #[repr(C)]
> struct PointC { x: u32, y: f64 }
>
> // print-type-size type: `PointC`: 16 bytes, alignment: 8 bytes
> // print-type-size     field `.x`: 4 bytes
> // print-type-size     padding: 4 bytes
> // print-type-size     field `.y`: 8 bytes, alignment: 8 bytes
> ```
>
> Which have different layouts.
>
> Or, in Swift, where structs are not reordered by default, the layout would match the `#[repr(C)]` variant:
>
> ```swift
> struct Point { var x: UInt32; var y: Double }
>
> // print-type-size type: `Point`: 16 bytes, alignment: 8 bytes
> // print-type-size     field `.x`: 4 bytes
> // print-type-size     padding: 4 bytes
> // print-type-size     field `.y`: 8 bytes
> ```
