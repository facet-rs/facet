+++
title = "Why facet?"
weight = 1
+++

facet is a reflection library for Rust. A single `#[derive(Facet)]` provides serialization, pretty-printing, diffing, CLI argument parsing, and more — all from the same type information.

```rust,noexec
#[derive(Facet)]
struct Config {
    name: String,
    port: u16,
    #[facet(sensitive)]
    api_key: String,
}
```

That single derive works with `facet-json`, `facet-yaml`, `figue`, `facet-pretty`, `facet-diff`, `facet-assert`, and any future tools built on facet.

## Data, not code

[serde](https://docs.rs/serde) generates code. When you derive `Serialize`, the compiler generates a function that knows how to serialize your specific type. That function is optimized, inlined, and fast.

facet generates data. When you derive `Facet`, the compiler generates a static description of your type — its fields, their names, their types, their attributes. Format crates read this description at runtime and decide what to do.

This is a real tradeoff. facet is slower than serde. But it enables things serde fundamentally cannot do.

## What facet enables

### Introspection without running code

With serde, the only way to answer questions about a type is to run a serializer against it. Want to know what fields a struct has? Write a dummy serializer that records them. Want to check if a field has a certain attribute? You can't — serde doesn't preserve that information.

With facet, the shape of your type is static data. You can query it directly: what fields exist, what are their types, what attributes do they have, are any marked sensitive.

This is why crates like [minijinja](https://docs.rs/minijinja) have built reflection capabilities on top of serde. facet provides this introspection natively.

### Rich error messages

When deserialization fails, facet can tell you exactly where and why:

```
  × unknown field `emial`, expected one of: ["username", "email"]
   ╭────
 1 │ {"username": "alice", "emial": "alice@example.com"}
   ·                       ───┬───
   ·                          ╰── unknown field 'emial' - did you mean 'email'?
   ╰────
```

The error points to the exact location in the input, suggests corrections for typos, and explains what went wrong.

serde's generated code is optimized for speed. Tracking source locations would require allocating memory and recording state, which adds overhead in the common case where nothing goes wrong.

facet's shape data is static. Keeping references to it is cheap. Errors store byte offsets and pointers to shapes, and formatting happens lazily when you actually render the error.

### Specialization

With serde, `Vec<T>` serializes the same way for any `T`. The trait implementation is fixed at compile time.

With facet, you can compare shapes at runtime. A format crate can treat `Vec<u8>` as raw bytes and `Vec<u32>` as an array of integers. The decision happens at runtime based on the actual type information.

### A richer value type

serde's de facto dynamic value type is [`serde_json::Value`](https://docs.rs/serde_json/latest/serde_json/enum.Value.html) — designed for JSON. It doesn't natively support bytes, datetimes, or `u128`.

[`facet_value::Value`](https://docs.rs/facet-value/latest/facet_value/struct.Value.html) is format-agnostic and richer: bytes and datetimes are first-class, not encoded as strings or arrays. Source spans can be preserved for error reporting.

### flatten without precision loss

serde's `#[serde(flatten)]` and `#[serde(untagged)]` require the deserializer to buffer into an intermediate representation, which can lose precision for large integers.

facet's [`facet-solver`](https://docs.rs/facet-solver) handles flatten and enum disambiguation differently. It builds the space of possibilities from the type structure and narrows it down as fields arrive. No lossy intermediate buffer.

## One derive, many uses

With serde, you derive `Serialize` and `Deserialize`. For CLI parsing, you derive `Parser` from [clap](https://docs.rs/clap). For debug output, you implement or derive `Debug`. Each one generates its own code.

If you want to reduce binary size, you might use `#[cfg_attr(...)]` to conditionally enable derives only where needed. That's complexity you have to manage.

With facet, you derive once:

```rust,noexec
#[derive(Facet)]
struct Args {
    #[facet(args::named, args::short = 'v')]
    verbose: bool,

    #[facet(args::positional)]
    input: String,

    #[facet(sensitive)]
    token: String,
}
```

That type works with `facet-json` for config files, `figue` for CLI parsing, and `facet-pretty` for debug output (with `token` redacted). Same derive, same attributes, many uses.

## Extension attributes without proc-macros

Writing a proc-macro is hard. With facet, if you want custom attributes for your format crate, you call `define_attr_grammar!` and you're done:

```rust,noexec
facet::define_attr_grammar! {
    ns "myformat";
    crate_path ::my_format_crate;

    pub enum Attr {
        /// Marks a field as special
        Special,
    }
}
```

Users write `#[facet(myformat::special)]`, typos are caught at compile time with suggestions, and you query the attributes at runtime with full type safety. No proc-macro expertise required.

## The tradeoff

facet is slower than serde. You're making decisions at runtime instead of at compile time. For many applications — CLI tools, desktop apps, web services where serialization isn't the bottleneck — this doesn't matter.

If you've profiled your application and serialization is actually your bottleneck, serde might be the better choice. That's a valid tradeoff.

facet's goal is not speed. It's expressiveness, diagnostics, and flexibility. If the computer has information, it should present it clearly. If you can avoid generating separate code for every tool, you should.

## Current state

- The **attribute grammar system** allows extension crates to define custom attributes without proc-macros
- Format crates cover JSON, YAML, TOML, MessagePack, CSV, XDR, and more
- figue handles CLI parsing, facet-pretty handles debug output, facet-diff handles structural comparison

## Next steps

- Browse the [Showcases](/showcases/) for examples
- Check the [format support matrix](/reference/format-crate-matrix/) for available formats
- Read the [Guide](/guide/) to get started
- Source: [github.com/facet-rs/facet](https://github.com/facet-rs/facet)
