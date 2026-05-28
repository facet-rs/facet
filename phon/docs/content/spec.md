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
>
> The wire schema is the source of truth for every exchange, including ones where both ends
> happen to be Rust. The same property makes phon work as a storage format: bytes written
> today and read back years later go through the same path as two peers on different deploys
> talking now.

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
>
> Other languages receive their schemas through codegen. phon emits the type definitions
> plus the schema bytes — in self-describing phon — into Swift, TypeScript, or any other
> target. Each peer ships with the schemas it needs as constants.

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
>
> A descriptor pairs a schema with one process's memory layout for that schema. The
> engine takes a `(schema, descriptor)` pair in both directions and uses it to move
> values between memory and wire.
>
> Each language produces descriptors its own way: Rust through facet metadata, Swift by
> probing the runtime, TypeScript from codegen. The engine sees one shape.

> r[two-engines]
>
> phon has an interpreter and (optionally) a JIT. They share one intermediate
> representation: a `(schema, descriptor)` pair compiles down to the same operations
> either way.
>
> The interpreter runs everywhere — including Apple sandboxes, WebAssembly, and other
> environments that restrict allocating executable memory.
>
> The JIT specializes one `(schema, descriptor)` pair at runtime, trading compile-time
> paid once for fast per-message execution. What "JIT" means is per-target: in Rust and
> Swift, machine code via copy-and-patch; in TypeScript, generated JavaScript source
> passed to `new Function()`. The IR is shared; the lowering is per-language.
>
> Each language's JIT lives behind an opt-in — in Rust, the `phon-jit` crate — so it's
> only present where wanted.
