---
title: "facet"
date: "1969-07-20T20:17:00Z"
---

# facet

**facet** is a derive macro and a trait that gives runtime (and to some extent, const-time) knowledge
about the shape, trait implementations, and characteristics of arbitrary types.

> More information is available on [its github repository](https://github.com/facet-rs/facet)

It's intended to be "the last proc macro / the last derive you'll ever need", since it can serve a lot
of use cases typically handled by proc macros, like:

  * Pretty-printing
  * Run-time introspection
  * Debugging (incl. mutating values)
  * Serializing, and deserializing
  * Code generation (via build scripts)
  * Diffing values
  * And more!

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
    * and [ValueVTable](https://docs.rs/facet-core/latest/facet_core/struct.ValueVTable.html)

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

> *:bearsays*
>
> For example, [facet-json](https://docs.rs/facet-json) has `#[deny(unsafe_code)]` — all "format crates" do.

## What can you build with it?

The `Facet` trait lends itself to a surprisingly large number of use cases.

### A better `Debug`

You can replace `Debug` with [facet-pretty](https://docs.rs/facet-pretty) and get:

  * Nice colors via [owo-colors](https://docs.rs/owo-colors) (even in no-std)
  * Un-printable fields will just have their types printed
  * [Sensitive](https://docs.rs/facet-core/latest/facet_core/struct.FieldFlags.html#associatedconstant.SENSITIVE) fields will be redacted

Example outputs:

```term
<i class="b">Person</i><i class="l"> {</i><i class="fg-ansi1"> </i> <i class="fg-ansi4">       </i>
  <i class="fg-cyn">name</i><i class="l">: </i><i style="color:#e05185">Alice</i><i class="l">,</i><i class="fg-ansi4">   </i>
  <i class="fg-cyn">age</i><i class="l">: </i><i style="color:#5175e0">30</i><i class="l">,</i> <i class="fg-ansi4">      </i>
  <i class="fg-cyn">address</i><i class="l">: </i><i class="b">Address</i><i class="l"> {</i>
    <i class="fg-cyn">street</i><i class="l">: </i><i style="color:#e05185">123 Main St</i><i class="l">,</i>
    <i class="fg-cyn">city</i><i class="l">: </i><i style="color:#e05185">Wonderland</i><i class="l">,</i>
    <i class="fg-cyn">country</i><i class="l">: </i><i style="color:#e05185">Imagination</i><i class="l">,</i>
  <i class="l">},</i><i class="fg-ansi2">     </i>  <i class="fg-ansi4">      </i>
<i class="l">}</i>   <i class="fg-ansi1">    </i>   <i class="fg-ansi4">      </i>
```

Also:

```term
<i class="b">TestSecrets</i><i class="l"> {</i><i class="fg-ansi4">    </i>
  <i class="fg-cyn">normal_field</i><i class="l">: </i><i style="color:#e05185">This is visible</i><i class="l">,</i>
  <i class="fg-cyn">sensitive_field</i><i class="l">: </i><i class="b fg-lred">[REDACTED]</i><i class="l">,</i>
<i class="l">}</i>   <i class="fg-ansi2">     </i>  <i class="fg-ansi4">      </i>
```

### A better `assert!`

Crates like [pretty-assertions](https://docs.rs/pretty-assertions) make a diff
of the `Debug` representation of two types.

Wouldn't it be better to have access to the whole type information of both sides
and do a structural difference, knowing the affinity of every scalar, having
access to display implementations, but not just, something more like
[difftastic](https://github.com/Wilfred/difftastic). than `diff`?

### A more flexible `serde`

You can use [facet-json](https://docs.rs/facet-json), [facet-toml](https://docs.rs/facet-toml) and others to serialize and deserialize data.

> *:bearsays*
>
> Those two are the most maintained — but there are others, and [help is wanted](https://github.com/facet-rs/facet/issues)

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

> *:bearsays*
>
> But don't trust us, make your own measurements! Be aware facet is still a bit rough around the edges.

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

> *:bearsays*
>
> Nobody's has done this yet, will you be the first?

### Specialization (at runtime)

We're not talking about compiling different code based on the `T` in `Vec<T>` —
however, you can reflect on the `T` (if you're comfortable adding a `T: Facet`
bound) and dynamically call methods on it.

For example, [facet-pretty](https://docs.rs/facet-pretty) prints `Vec<u8>`
different than other Vec types:

```term
<i class="b fg-cyn">facet</i> on <i class="b fg-mag"> main</i> <i class="b fg-red">[$]</i> via <i class="b fg-red">🦀 v1.86.0</i>
<i class="b fg-grn">❯</i> cargo run --example vec_u8
<i class="b fg-grn">    Finished</i> &#96;dev&#96; profile [unoptimized + debuginfo] target(s) in 0.01s
<i class="b fg-grn">     Running</i> &#96;target/debug/examples/vec_u8&#96;
<i class="b">Vec&lt;u8&gt;</i><i class="fg-ansi1">  </i>  <i class="fg-ansi4">      </i>
  <i style="color:#51e0d4">29 </i><i style="color:#88e051">a6 </i><i style="color:#e0518d">6e </i><i style="color:#51d2e0">6a </i><i style="color:#51e0c3">8d </i><i style="color:#5f51e0">7e </i><i style="color:#51b3e0">c9 </i><i style="color:#519de0">52 </i><i style="color:#51b3e0">c9 </i><i style="color:#e051de">1e </i><i style="color:#b051e0">4d </i><i style="color:#516be0">83 </i><i style="color:#b5e051">bf </i><i style="color:#e08551">f5 </i><i style="color:#51e0c3">8d </i><i style="color:#8fe051">60</i>
<i style="color:#8fe051">  </i><i style="color:#e05351">69 </i><i style="color:#a0e051">73 </i><i style="color:#51e09b">1d </i><i style="color:#51e088">d2 </i><i style="color:#e09b51">76 </i><i style="color:#58e051">d0 </i><i style="color:#e05196">c2 </i><i style="color:#51a4e0">75 </i><i style="color:#e06451">b5 </i><i style="color:#5b51e0">a7 </i><i style="color:#51e0b3">c6 </i><i style="color:#e0ba51">f9 </i><i style="color:#518de0">af </i><i style="color:#a0e051">73 </i><i style="color:#e05196">03 </i><i style="color:#e0518f">fc</i>
<i style="color:#e0518f">  </i><i style="color:#e0b051">b0 </i><i style="color:#9151e0">65 </i><i style="color:#5183e0">b7 </i><i style="color:#e0b351">19 </i><i style="color:#51c1e0">eb </i><i style="color:#51cfe0">87 </i><i style="color:#e051a4">d3 </i><i style="color:#e08851">a3 </i><i style="color:#7e51e0">a5 </i><i style="color:#a0e051">73 </i><i style="color:#7051e0">e1 </i><i style="color:#519de0">52 </i><i style="color:#51e062">da </i><i style="color:#e05188">07 </i><i style="color:#b0e051">06 </i><i style="color:#e051c3">f3</i>
<i style="color:#e051c3">  </i><i style="color:#58e051">15 </i><i style="color:#8851e0">d6 </i><i style="color:#e05196">c2 </i><i style="color:#e07c51">f0 </i><i style="color:#e09d51">b3 </i><i style="color:#e0516b">61 </i><i style="color:#8351e0">6d </i><i style="color:#51e09b">1d </i><i style="color:#b051e0">4d </i><i style="color:#51e085">8a </i><i style="color:#51e07e">5f </i><i style="color:#e0cf51">1c </i><i style="color:#d9e051">31 </i><i style="color:#51e062">da </i><i style="color:#51a7e0">98 </i><i style="color:#e051b0">25</i>
<i style="color:#e051b0">  </i><i style="color:#e09151">b1 </i><i style="color:#b7e051">00 </i><i style="color:#e05351">94 </i><i style="color:#51c6e0">8c </i><i style="color:#e0db51">34 </i><i style="color:#e09951">ac </i><i style="color:#e0cf51">1c </i><i style="color:#51e0b3">c6 </i><i style="color:#e0a751">05 </i><i style="color:#51c8e0">09 </i><i style="color:#e05170">f7 </i><i style="color:#e0b351">19 </i><i style="color:#e09b51">76 </i><i style="color:#51e0b5">0f </i><i style="color:#51b0e0">51 </i><i style="color:#51e05d">3a</i>
<i style="color:#51e05d">  </i><i style="color:#51cae0">3d </i><i style="color:#e0cf51">1c </i><i style="color:#a051e0">e8 </i><i style="color:#51e053">d9 </i><i style="color:#e06e51">9a </i><i style="color:#e05196">03 </i><i style="color:#51cae0">3d </i><i style="color:#75e051">26 </i><i style="color:#7ee051">bb </i><i style="color:#518fe0">ed </i><i style="color:#e0e051">df </i><i style="color:#64e051">cf </i><i style="color:#e051bc">39 </i><i style="color:#5169e0">be </i><i style="color:#e05181">d4 </i><i style="color:#c1e051">0d</i>
<i style="color:#c1e051">  </i><i style="color:#51e0a2">93 </i><i style="color:#e08d51">ae </i><i style="color:#51e075">84 </i><i style="color:#8fe051">60 </i><i style="color:#e051ba">d5 </i><i style="color:#e07c51">f0 </i><i style="color:#51e09d">7c </i><i style="color:#51e0cd">4f </i><i style="color:#9de051">62 </i><i style="color:#5b51e0">a7 </i><i style="color:#51e088">22 </i><i style="color:#51c1e0">eb </i><i style="color:#e06e51">9a </i><i style="color:#e051a4">d3 </i><i style="color:#5172e0">4c </i><i style="color:#a251e0">70</i>
<i style="color:#a251e0">  </i><i style="color:#7ee051">bb </i><i style="color:#e0b751">08 </i><i style="color:#e09d51">b3 </i><i style="color:#515fe0">ea </i><i style="color:#e05158">b6 </i><i style="color:#e05169">0b </i><i style="color:#58e051">d0 </i><i style="color:#e05181">d4 </i><i style="color:#e0ba51">f9 </i><i style="color:#51e0d4">82 </i><i style="color:#5151e0">88 </i><i style="color:#51e0c8">80 </i><i style="color:#51bae0">5b </i><i style="color:#a051e0">28 </i><i style="color:#51e0d6">41 </i><i style="color:#b551e0">5d</i>
```

> *:bearsays*
>
> Whether that's a good idea or not is a different question.

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

facet gives you access to doc comments, so generating JSON-scheams

### Derive `Error`

Like [displaydoc](https://docs.rs/displaydoc/latest/displaydoc/) but without the added `syn` (see [free of syn](https://github.com/fasterthanlime/free-of-syn))?

### Much, much more

We still haven't figured everything facet can do. Come do research with us:

  * <https://github.com/facet-rs>

## Derive macro, comparison with Serde

### Container attributes

#### deny_unknown_fields

Produce an error when an unknown field is encountered during deserialization. The default behaviour
is to ignore field that are not known.

<table>
<tr>
<th>Facet</th>
<th>Serde</th>
</tr>
<tr>
<td>

```rust
#[derive(facet::Facet)]
#[facet(deny_unknown_fields)]
struct MyStruct {
    field1: i32,
    field2: Option<i32>,
}
```

</td>
<td>

```rust
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct MyStruct {
    field1: i32,
    field2: Option<i32>,
}
```

</td>
</tr>
</table>

#### default

Only allowed for `struct`s, not for `enum`s. During deserialization, any fields that are missing
from the input will be taken from the `Default::default` implementation of the struct. This is not
possible for `enum`s because they can only have a single `Default` implementation producing a single
variant.

<table>
<tr>
<th>Facet</th>
<th>Serde</th>
</tr>
<tr>
<td>

```rust
#[derive(facet::Facet)]
#[facet(default)]
struct MyStruct {
    field1: i32,
    field2: Option<i32>,
}

impl Default for MyStruct {
    fn default() -> Self {
        Self {
            field1: 1,
            field2: Some(2),
        }
    }
}
```

</td>
<td>

```rust
#[derive(serde::Deserialize)]
#[serde(default)]
struct MyStruct {
    field1: i32,
    field2: Option<i32>,
}

impl Default for MyStruct {
    fn default() -> Self {
        Self {
            field1: 1,
            field2: Some(2),
        }
    }
}
```

</td>
</tr>
</table>

<!-- #### transparent

I don't think this is used for serialization and so should not be in a comparison with serde?

/// Indicates that this is a transparent wrapper type, like `NewType(T)`
/// it should not be treated like a struct, but like something that can be built
/// from `T` and converted back to `T`
Transparent, -->

#### rename_all

Rename all fields at once using a casing convention. Supported values are

* `"PascalCase"`
* `"camelCase"`
* `"snake_case"`
* `"SCREAMING_SNAKE_CASE"`
* `"kebab-case"`
* `"SCREAMING-KEBAB-CASE"`

<table>
<tr>
<th>Facet</th>
<th>Serde</th>
</tr>
<tr>
<td>

```rust
#[derive(facet::Facet)]
#[facet(rename_all = "camelCase")]
struct MyStruct {
    field_one: i32,
}
```

</td>
<td>

```rust
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct MyStruct {
    field_one: i32,
}
```

</td>
</tr>
</table>

### Field attributes

#### skip_serializing

Skip this field during serialization.

<table>
<tr>
<th>Facet</th>
<th>Serde</th>
</tr>
<tr>
<td>

```rust
#[derive(facet::Facet)]
struct MyStruct {
    field1: i32,
    #[facet(skip_serializing)]
    field2: String,
}
```

</td>
<td>

```rust
#[derive(serde::Serialize)]
struct MyStruct {
    field1: i32,
    #[serde(skip_serializing)]
    field2: String,
}
```

</td>
</tr>
</table>


#### skip_serializing_if

Skip serializing this field when a condition is met. Typically used for `Option` fields when you
want to omit the field entirely from serialized output when the value is `None`.

<table>
<tr>
<th>Facet</th>
<th>Serde</th>
</tr>
<tr>
<td>

```rust
#[derive(facet::Facet)]
struct MyStruct {
    #[facet(skip_serializing_if = |n| n % 2 == 0)]
    field1: i32,
    #[facet(skip_serializing_if = Option::is_none)]
    field2: Option<i32>,
}
```

</td>
<td>

```rust
#[derive(serde::Serialize)]
struct MyStruct {
    #[serde(skip_serializing_if = is_even)]
    field1: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    field2: Option<i32>,
}

fn is_even(n: i32) -> bool {
    n % 2 == 0
}
```

</td>
</tr>
</table>

#### default

Use a specified function to provide a default value when deserializing if the field is missing from
input. You can either use `default` alone to use `Default::default()` for the field, or provide an
expression producing the default value.

<table>
<tr>
<th>Facet</th>
<th>Serde</th>
</tr>
<tr>
<td>

```rust
#[derive(facet::Facet)]
struct MyStruct {
    field1: i32,
    #[facet(default)]
    field2: Vec<String>,
    #[facet(default = 42))]
    field3: i32,
    #[facet(default = rand::random())]
    field4: i32,
}
```

</td>
<td>

```rust
#[derive(serde::Deserialize)]
struct MyStruct {
    field1: i32,
    #[serde(default)]
    field2: Vec<String>,
    #[serde(default = "default_value")]
    field3: i32,
    #[serde(default = "rand::random")]
    field4: i32,
}

fn default_value() -> i32 {
    42
}
```

</td>
</tr>
</table>

## How to support facet

[@fasterthanlime](https://fasterthanli.me) aka Amos Wenger is the original author and
principal maintainer of facet.

You can support them:

  * on [Ko-fi](https://ko-fi.com/fasterthanlime)
  * on [GitHub Sponsors](https://github.com/sponsors/fasterthanlime)
  * on [Patreon](https://www.patreon.com/fasterthanlime)

Watching [their videos](https://youtube.com/@fasterthanlime) (which are often Rust-focused) is also a nice way to support them.

You can adopt one of the format crates for facet, you can experiment with it,
you can tell your friends about it, you can tackle one of the many [open issues](https://github.com/facet-rs/facet/issues)!

Check out the GitHub repository and start playing with facet today!

  * <https://github.com/facet-rs/facet>
