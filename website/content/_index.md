+++
title = "facet"
insert_anchor_links = "heading"
+++

**facet** is a derive macro and a trait that gives runtime (and to some extent, const-time) knowledge
about the shape, trait implementations, and characteristics of arbitrary types.

If you're coming from serde and just want to see the differences, check out the [Serde Comparison](/serde-comparison/).

It can serve a lot of use cases typically handled by proc macros, like:

  * Pretty-printing
  * Run-time introspection
  * Debugging (incl. mutating values)
  * Serializing, and deserializing
  * Code generation (via build scripts)
  * Diffing values

## Crash course

You derive it like `Serialize` or `Deserialize` except there's only one macro:

```rust
#[derive(Facet)]
struct FooBar {
    foo: u32,
    bar: String,
}
```

Now, `FooBar::SHAPE`, of type [Shape](https://docs.rs/facet-core/latest/facet_core/struct.Shape.html),
lets us know:

  * Whether it's a struct, an enum, a list, a map, an array, a slice, a scalar (see [Def](https://docs.rs/facet-core/latest/facet_core/enum.Def.html))
  * What fields it has (if it's a struct, an enum, a tuple, etc.)
    * Also, which offset they're at and _their_ shape, of course
  * What variants it has if it's an enum...

But also:

  * Which traits are implemented for this shape
    * via [Characteristic](https://docs.rs/facet-core/latest/facet_core/enum.Characteristic.html)
    * and [ValueVTable](https://docs.rs/facet-core/latest/facet_core/enum.ValueVTable.html)

This takes into account type parameters (which you can also inspect at runtime), so:

  * For example, `<Vec<i32>>::SHAPE.vtable.debug` is `Some(_)`
  * For example, `<Vec<MyNonDebugStruct>>::SHAPE.vtable.debug` is `None(_)`

### Reflection

However, vtables are low-level and unsafe, and you would normally invoke stuff through
[facet-reflect](https://docs.rs/facet-reflect) types like:

  * [Peek](https://docs.rs/facet-reflect/latest/facet_reflect/struct.Peek.html) when reading from a value
  * [Partial](https://docs.rs/facet-reflect/latest/facet_reflect/struct.Partial.html) when building values from scratch

These two abstractions are used by serializers and deserializers respectively,
and are fully safe, despite dealing with partially-initialized values under the hood.

{% bearsays() %}
For example, [facet-json](https://docs.rs/facet-json) has `#[deny(unsafe_code)]` — all "format crates" do.
{% end %}

## What can you build with it?

The `Facet` trait lends itself to a surprisingly large number of use cases.

### A better `Debug`

You can replace `Debug` with [facet-pretty](https://docs.rs/facet-pretty) and get:

  * Nice colors via [owo-colors](https://docs.rs/owo-colors) (even in no-std)
  * Un-printable fields will just have their types printed
  * [Sensitive](https://docs.rs/facet-core/latest/facet_core/struct.FieldFlags.html#associatedconstant.SENSITIVE) fields will be redacted

### A better `assert!`

Crates like [pretty-assertions](https://docs.rs/pretty-assertions) make a diff
of the `Debug` representation of two types.

Wouldn't it be better to have access to the whole type information of both sides
and do a structural difference, knowing the affinity of every scalar, having
access to display implementations, but not just, something more like
[difftastic](https://github.com/Wilfred/difftastic). than `diff`?

### A more flexible `serde`

You can use [facet-json](https://docs.rs/facet-json), [facet-toml](https://docs.rs/facet-toml) and others to serialize and deserialize data.

{% bearsays() %}
Those two are the most maintained — but there are others, and [help is wanted](https://github.com/facet-rs/facet/issues)
{% end %}

Those are bound to be slower than [serde](https://serde.rs), which generates optimized code. So why bother?

Well, serde generates a _lot_ of code. And it depends on heavy packages like [syn](https://docs.rs/syn).

Cold build times (and often, hot build times) suffer, in the presence of a lot
of large data structures. If runtime performance is not the bottleneck, facet can help by:

  * Deriving _data_, not code
  * Avoiding combinatorial explosion due to monomorphization

What does that last point mean? serde generates different code for `Vec<T>`, `Vec<U>`, `Vec<W>`, etc.

What's more, it generates different code (via generics, too) for every
serializer and deserializer. This may be very efficient at runtime, but it makes
some projects' compile time very, very long.

With `facet`, serialization and deserialization is implemented:

  * Once per type (`Vec<T>` for any `T`)
  * Once per data format (JSON, TOML, etc.)

You can have `mycrate-types` crates, with every struct deriving `Facet`, with no worries. No need
to put it behind a feature flag even, the main `facet` crate is relatively light, thanks to its use
of the lightweight [unsynn](https://docs.rs/unsynn) instead of `syn`.

{% bearsays() %}
But don't trust us, make your own measurements!
{% end %}

`facet` has a lot more information about your types than `serde` does, which
means it's able to generate better errors, and decide things about deserialization
that can't really be done with serde without breaking its interface, like:

  * Deciding at runtime what to do about duplicate fields
  * Deciding at runtime what to fill a missing field with?
  * Only deserializing _part_ of the data, with JSONPath-like selectors

Additionally, deserializers like [facet-json](https://docs.rs/facet-json)'s are
designed to be iterative, not recursive. You can deserialize very very deep data
structures without blowing up the stack. As long as you got enough heap, you're good
to go.

### Code generation

If you don't mind building your types crates as a build dependency, too, you
could then use reflection to generate Rust code and thus reach serde-level
speeds, if you generate serialization/deserialization code, for example.

### Specialization (at runtime)

We're not talking about compiling different code based on the `T` in `Vec<T>` —
however, you can reflect on the `T` (if you're comfortable adding a `T: Facet`
bound) and dynamically call methods on it.

For example, [facet-pretty](https://docs.rs/facet-pretty) prints `Vec<u8>`
different than other Vec types.

### Better debuggers

See [this issue](https://github.com/facet-rs/facet/issues/102) for an interesting discussion.

### Diffing?

See [this issue](https://github.com/facet-rs/facet/issues/145) for talk about diffing

### Better support for XML/KDL

Those don't fit the serde data model so well. More discussion over at:

  * [the XML issue](https://github.com/facet-rs/facet/issues/150)
  * [the KDL issue](https://github.com/facet-rs/facet/issues/151)

Other data formats (protobuf? postcard?) would also probably benefit from additional attributes.

### Better JSON schemas

facet gives you access to doc comments, so JSON-schemas generated from that information could in theory be more complete than those from, say, serde.

### Derive `Error`

Like [displaydoc](https://docs.rs/displaydoc/latest/displaydoc/) but without the added `syn` (see [free of syn](https://github.com/fasterthanlime/free-of-syn))?

### Much, much more

We still haven't figured everything facet can do. Come do research with us:

  * <https://github.com/facet-rs>

See also: [Comparison with Serde](/serde-comparison) for a side-by-side look at derive macro attributes.

## Contributing

Contributions are welcome! Check out the [GitHub repository](https://github.com/facet-rs/facet) to get started.
