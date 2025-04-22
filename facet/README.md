<h1>
<picture>
    <source type="image/webp" media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/logo-v2/facet-b-dark.webp">
    <source type="image/png" media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/logo-v2/facet-b-dark.png">
    <source type="image/webp" srcset="https://github.com/facet-rs/facet/raw/main/static/logo-v2/facet-b-light.webp">
    <img src="https://github.com/facet-rs/facet/raw/main/static/logo-v2/facet-b-light.png" height="35" alt="Facet logo - a reflection library for Rust">
</picture>
</h1>

[![Coverage Status](https://coveralls.io/repos/github/facet-rs/facet/badge.svg?branch=main)](https://coveralls.io/github/facet-rs/facet?branch=main)
[![free of syn](https://img.shields.io/badge/free%20of-syn-hotpink)](https://github.com/fasterthanlime/free-of-syn)
[![crates.io](https://img.shields.io/crates/v/facet.svg)](https://crates.io/crates/facet)
[![documentation](https://docs.rs/facet/badge.svg)](https://docs.rs/facet)
[![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/facet.svg)](./LICENSE)

_Logo by [Misiasart](https://misiasart.com/)_

Thanks to all individual and corporate sponsors, without whom this work could not exist:

<p> <a href="https://ko-fi.com/fasterthanlime">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/kofi-dark.svg">
<img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/kofi-light.svg" height="40" alt="Ko-fi">
</picture>
</a> <a href="https://github.com/sponsors/fasterthanlime">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/github-dark.svg">
<img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/github-light.svg" height="40" alt="GitHub Sponsors">
</picture>
</a> <a href="https://patreon.com/fasterthanlime">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/patreon-dark.svg">
<img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/patreon-light.svg" height="40" alt="Patreon">
</picture>
</a> <a href="https://zed.dev">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/zed-dark.svg">
<img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/zed-light.svg" height="40" alt="Zed">
</picture>
</a> <a href="https://depot.dev?utm_source=facet">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/depot-dark.svg">
<img src="https://github.com/facet-rs/facet/raw/main/static/sponsors-v3/depot-light.svg" height="40" alt="Depot">
</picture>
</a> </p>

facet provides "const fn" reflection for Rust.

The `Facet` trait is meant to be derived for _every single type in the Rust
ecosystem_, and can be used to replace many other derive macros.

```rust,ignore
pub unsafe trait Facet: Sized {
    const SHAPE: &'static Shape;
    // (other fields ignored)
}
```

Whereas crates like `serde` derive _code_ using the heavy `syn`, `facet` derives
data with the light and fast `unsynn`.

That data does not make compile times balloon due to heavy monomorphization. It
can be used to reason about types at runtime — which even allows doing
specialization.

The `SHAPE` associated constant fully describes a type:

  * Whether it's a struct, an enum, or a scalar
  * All fields, variants, offsets, discriminants, memory layouts
  * VTable for various standard traits:
    * Display, Debug, Clone, Default, Drop etc.

## Use case: inspection, pretty printing, debugging, specialization

The `Debug` trait is severely limited because it cannot be specialized.

`facet-pretty` provides pretty printing of any type that implements `Facet`:

```rust,ignore
    let address = Address {
        street: "123 Main St".to_string(),
        city: "Wonderland".to_string(),
        country: "Imagination".to_string(),
    };

    let person = Person {
        name: "Alice".to_string(),
        age: 30,
        address,
    };

    println!("Default pretty-printing:");
    println!("{}", person.pretty());
```

```bash
facet on  main [!] via 🦀 v1.86.0
❯ cargo run --example basic_usage
   Compiling facet-pretty v0.1.2 (/Users/amos/bearcove/facet/facet-pretty)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.15s
     Running `target/debug/examples/basic_usage`
Default pretty-printing:
Person {
  name: Alice,
  age: 30,
  address: Address {
    street: 123 Main St,
    city: Wonderland,
    country: Imagination,
  },
}
```

(Note: the default pretty-printing shows ANSI colors).

Facet knows the type inside the `T`, so it's able to format it:

```rust,ignore
use facet_pretty::FacetPretty;

#[derive(Debug, Facet)]
struct Person {
    name: String,
}

# fn main() {
let alice = Person {
    name: "Alice".to_string(),
};
let bob = Person {
    name: "Bob".to_string(),
};
let carol = Person {
    name: "Carol".to_string(),
};

println!("{}", vec![alice, bob, carol].pretty());
# }
```

```bash
facet on  main [$!] via 🦀 v1.86.0
❯ cargo run --example vec_person
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.03s
     Running `target/debug/examples/vec_person`
Vec<Person> [
  Person {
    name: Alice,
  },
  Person {
    name: Bob,
  },
  Person {
    name: Carol,
  },
]
```

Because we know the shape of `T`, we can format different things differently,
if we wanted to:

```rust,ignore
    let mut file = std::fs::File::open("/dev/urandom").expect("Failed to open /dev/urandom");
    let mut bytes = vec![0u8; 128];
    std::io::Read::read_exact(&mut file, &mut bytes).expect("Failed to read from /dev/urandom");
    println!("{}", bytes.pretty());
```

```bash
facet on  main [!] via 🦀 v1.86.0
❯ cargo run --example vec_u8
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.01s
     Running `target/debug/examples/vec_u8`
Vec<u8>
  aa c5 ce 2a 79 95 a6 c6 63 ca 69 5f 12 d5 7e fc
  f4 40 60 48 c4 ee 10 7c 12 a2 67 3d 2f 9a c4 ca
  b3 7e 91 5c 67 16 41 35 92 31 22 0f 23 6a ad c1
  f4 b3 c2 60 38 13 02 47 25 7e f9 48 9b 11 b5 0e
  cb 5d c6 b1 43 23 bd a7 8c 6c 7d e6 7b 72 b7 26
  1a 2c e2 b8 e9 1a a6 e7 f6 b2 9b c7 88 76 d2 be
  59 79 27 00 0b 3e 88 a3 ce 8a 14 ec 72 f9 eb 23
  d4 36 93 a5 e9 b9 00 de 6a 3f 64 b8 49 05 3f 22
```

And because we can make this decision at runtime, it can be an option on the pretty-printer itself:

```rust,ignore
/// A formatter for pretty-printing Facet types
pub struct PrettyPrinter {
    indent_size: usize,
    max_depth: Option<usize>,
    color_generator: ColorGenerator,
    use_colors: bool,
    // ⬇️ here
    list_u8_as_bytes: bool,
}
```

This is just a pretty printer, but an imaginative mind could come up with...

  * A fully inspectable program state, through a browser interface?
  * A modern debugger, exposing all the standard traits and then some instead of a bag of pointers?



## Use case: (de)serialization

The `facet-reflect` crate allows reading (peek) and constructing/initializing/mutating (poke) arbitrary
values without knowing their concrete type until runtime. This makes it trivial to
write deserializers, see `facet-json`, `facet-yaml`, `facet-urlencoded`, etc.

Say we have this struct:

```rust,ignore
use facet::Facet;

#[derive(Debug, PartialEq, Eq, Facet)]
struct FooBar {
    foo: u64,
    bar: String,
}
```

We can build it fully through reflection using the slot-based initialization API:

```rust
use facet::Facet;
use facet_reflect::Wip;

#[derive(Debug, PartialEq, Eq, Facet)]
struct FooBar {
    foo: u64,
    bar: String,
}

# fn main() -> eyre::Result<()> {
let foo_bar = Wip::alloc::<FooBar>()
    .field_named("foo")?
    .put(42u64)?
    .pop()?
    .field_named("bar")?
    .put(String::from("Hello, World!"))?
    .pop()?
    .build()?
    .materialize::<FooBar>()?;

// Now we can use the constructed value
println!("{}", foo_bar.bar);
# Ok(())
# }
```

The reflection API maintains type safety by validating types at each step and tracks which fields have been initialized.

This approach is particularly powerful for deserializers, where you need to incrementally build objects without knowing their full structure upfront. Inside a deserializer, you would first inspect the shape to understand its structure, and then systematically initialize each field.

## Use case: parsing CLI arguments

Facet allows arbitrary attributes (WIP) so you can use it for specifying whether a CLI
argument should be positional or named, for example:

```rust,ignore
use facet::Facet;

#[derive(Facet)]
struct Args {
    #[facet(positional)]
    path: String,

    #[facet(named, short = 'v')]
    verbose: bool,

    #[facet(named, short = 'j')]
    concurrency: usize,
}

let args: Args = facet_args::from_slice(&["--verbose", "--concurrency", "14", "example.rs"]);
eprintln!("args: {}", args.pretty());
```

```bash

facet on  main [$+] via 🦀 v1.86.0
❯ cargo nextest run --no-capture test_arg_parse
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.02s
────────────
 Nextest run ID 7147e23a-815c-4a9b-8981-1c72ea8c95a7 with nextest profile: default
    Starting 1 test across 19 binaries (87 tests skipped)
       START             facet-args::simple test_arg_parse

running 1 test
✂️
args: Args {
  path: example.rs,
  verbose: true,
  concurrency: 14,
}
```

## Use cases: augmenting static analysis/debuggers

By default the `Facet` derive macro creates and exports
a global static variable `{UPPER_CASE_NAME}_SHAPE` referencing the
`Shape` of the derived `Facet` trait.

Furthermore, `Shape` and all nested fields are `#[repr(C)]`.

This information can be used by external processes (like debuggers) to access the
layout and vtable data.

For example, suppose we have:

```rust,ignore
#[derive(Debug, Facet)]
struct TestStruct {
    field: &'static str,
}

static STATIC_TEST_STRUCT: TestStruct = TestStruct {
    field: "some field I would like to see",
};
```

By default, printing this in `lldb` returns the lengthy:

```bash
(lldb) p STATIC_TEST_STRUCT
(simple_test::TestStruct) {
  field = "some field I would like to see" {
    [0] = 's'
    [1] = 'o'
    [2] = 'm'
    [3] = 'e'
    [4] = ' '
    [5] = 'f'
    [6] = 'i'
    [7] = 'e'
    [8] = 'l'
    [9] = 'd'

  ... (and so on)
}
```

However, the `TestStruct::SHAPE` constant is available at `TEST_STRUCT_SHAPE`:

```bash
(lldb) p TEST_STRUCT_SHAPE
(facet_core::types::Shape *) 0x00000001000481c8
```

And so we can instead build a simple helper function that takes in a pointer
to the object and it's debug fn and prints out the `Debug` representation:

```bash
(lldb)  p debug_print_object(&STATIC_TEST_STRUCT, &TEST_STRUCT_SHAPE->vtable->debug)
TestStruct {
    field: "some field I would like to see",
}
```

In this case, `debug_print_object` is needed because the `debug` function requires a `Formatter`
which cannot be constructed externally. But for other operations like `Eq`, you can resolve it
without needing external methods (but with some additional shenanigans to make `lldb` happy):

```bash
(lldb) p TEST_STRUCT_SHAPE->vtable->eq
(core::option::Option<unsafe fn(facet_core::ptr::PtrConst, facet_core::ptr::PtrConst) -> bool>) {
  value = {
    0 = 0x0000000100002538
  }
}
(lldb) p (*((bool (**)(simple_test::TestStruct* , simple_test::TestStruct*))(&TEST_STRUCT_SHAPE->vtable->eq)))(&STATIC_TEST_STRUCT, &STATIC_TEST_STRUCT)
(bool) true
```

## Use cases: beyond

This could be extended to allow RPC, there could be an analoguous derive for traits,
it could export statics so that binaries may be inspected — shapes would then be available
instead of / in conjunction with debug info.

HTTP routing is a form of deserialization.

This is suitable for all the things serde is bad at: binary formats (specialize
for `Vec<u8>` without a serde_bytes hack), it could be extended to support formats
like KDL/XML.

I want the derive macros to support arbitrary attributes eventually, which will also
be exposed through `Shape`.

The types are all `non_exhaustive`, so there shouldn't be churn in the
ecosystem: crates can do graceful degradation if some types don't implement the
interfaces they expect.

If you have questions or ideas, please open a GitHub issue or discussion — I'm
so excited about this.

## Ecosystem

The core crates, `facet-trait`, `facet-types` etc. are nostd-friendly.

The main `facet` crate re-exports symbols from:

- [facet-core](https://github.com/facet-rs/facet/tree/main/facet-core), which defines the main components:
  - The `Facet` trait and implementations for foreign types (mostly `libstd`)
  - The `Shape` struct along with various vtables and the whole `Def` tree
  - Type-erased pointer helpers like `PtrUninit`, `PtrConst`, and `Opaque`
  - Autoderef specialization trick needed for `facet-derive`
- [facet-derive](https://github.com/facet-rs/facet/tree/main/facet-derive), which implements the `Facet` derive attribute as a fast/light proc macro powered by [unsynn](https://docs.rs/unsynn)

For struct manipulation and reflection, the following is available:

- [facet-reflect](https://github.com/facet-rs/facet/tree/main/facet-reflect),
  allows building values of arbitrary shapes in safe code, respecting invariants.
  It also allows peeking at existing values.
- [facet-pretty](https://github.com/facet-rs/facet/tree/main/facet-pretty) is able to pretty-print Facet types.

facet supports deserialization from multiple data formats through dedicated crates:

- [facet-json](https://github.com/facet-rs/facet/tree/main/facet-json): JSON deserialization
- [facet-yaml](https://github.com/facet-rs/facet/tree/main/facet-yaml): YAML deserialization
- [facet-toml](https://github.com/facet-rs/facet/tree/main/facet-toml): TOML deserialization
- [facet-msgpack](https://github.com/facet-rs/facet/tree/main/facet-msgpack): MessagePack deserialization
- [facet-urlencoded](https://github.com/facet-rs/facet/tree/main/facet-urlencoded): URL-encoded form data deserialization
- [facet-args](https://github.com/facet-rs/facet/tree/main/facet-args): CLI arguments (a-la clap)

Internal crates include:

- [facet-codegen](https://github.com/facet-rs/facet/tree/main/facet-codegen) is internal and generates some of the code of `facet-core`
- [facet-testhelpers] a simpler log logger and color-backtrace configured with the lightweight btparse backend

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](https://github.com/facet-rs/facet/blob/main/LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](https://github.com/facet-rs/facet/blob/main/LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
