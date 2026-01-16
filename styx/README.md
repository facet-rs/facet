# Styx

At least it's not YAML!

## Styx the straightforward

Imagine JSON

```json
{
  "key": "value"
}
```

But you remove everything that's getting in the way: the double quotes, the
the colon, even the comma:

```styx
{
  key value
}
```

Of course you can have the comma back if you want to put everything in a single line:

```styx
{key value, koi tuvalu}
```

Not far enough? Wanna get rid of the brackets? Okay, but only for the top-level object:

```styx
key value
koi tuvalu
```

What about arrays? They're called sequences and they use parentheses:

```styx
methods (GET POST PUT)
```

They're always whitespace-separated, never comma-separated.

## Styx the typed

```styx
name "John Doe"
age 97
retired true
```

Hey, which type are those values? Any type you want them to.

Scalars are just text atoms, `97` is not any more a number
than `https://example.org/` is.

Types matter at exactly two times:

- Validation via [schemas](https://styx.bearcove.eu/spec/schema/) (which are also Styx documents)
- Deserialization, in either flavor (dynamic or static typing)

In dynamic typing flavor, your Styx document gets parsed into a tree,
and then you get to request "field name as type string" — and if it can't
be coerced into a string, you get an error at that point.

In static typing flavor, you may for example deserialize to:

```rust
#[derive(Facet)]
struct Does {
    name: String,
    age: number,
    retired: bool,
}
```

And then the type mapping is, well, what you'd expect.

This solves the Norway problem:

```styx
country no
```

This `no` is not a boolean, not a string, not a number, it's everything, everywhere,
all at once, until you _need_ it to be something.

## Styx the nerd

Sometimes a value isn't quite enough, and you want to tag it:

```styx
this (is an untagged list)
that @special(list I hold dear)
```

Remember `()` are for sequences. They're not for grouping/precedence/calls.

You can tag objects, too:

```styx
rule @path_prefix{
  prefix /api
  route_to localhost:9000 // still no need to double-quote anything
  // oh yeah also comments just work
}
```

That's because Styx was designed to play nice with sum types,
like Rust enums:

```rust
enum Alternatives {
    NoPayload
    TuplePayload(u32, u32)
    StructPayload { name: String }
}
```

And so, tags are a natural way to _select_ a variant:

```styx
alts (
    @no_payload@
    @tuple_payload(3, 7)
    @struct_payload{name Gisèle}
)
```

Did you notice the `@` at the end of `@no_payload@`? Not a typo:
that's the unit value. It means "nothing", "none", kinda like "null"
but a little superior.

`@` is a value like any other:

```styx
sparse_seq (1 2 @ 8 9)
```

And in fact, wanna know a secret? `@` is not even the canonical form
of unit: `@@` is.

An empty tag degenerates to `@`, and a tag without a paylod defaults to
a payload of `@`.

Therefore:

```styx
@            // tag=@,   payload=@ (implied)
@@           // tag=@,   payload=@
@tag         // tag=tag, payload=@ (implied)
@tag@        // tag=tag, payload=@
@tag"must"   // tag=tag, payload=must
@tag()       // tag=tag, payload=() aka empty sequence
```

Importantly, there is NEVER ANY SPACE between a tag and its payload.
Spaces separate seq elements or key-value pairs in object context:

```styx
// this is a key-value pair:
@tag ()     // key(tag=tag, payload=@) value(tag=@, payload=())

// this is a DIFFERENT key-value pair
@tag()      // key(tag=tag, payload=()) value(tag=@, payload=@)
```

Does it confusing? Maybe. Little bit.

## Styx the objective

We've just seen this in the last gotcha:

```styx
@tag()      // key(tag=tag, payload=()) value(tag=@, payload=@)
```

Which, okay, `@tag()` is the entire key. But where's the value?

It's omitted. It defaults to `@`:

```styx
key @ // explicitly set to unit
koi   // implicitly set to unit
```

So, key-value pairs can be missing a value, and... they can also
have more than one key.

```styx
fee fi foe fum
// equivalent to
fee {fi {foe fum}}
```

And that's /it/ with the weirdness. Some unfamiliar bits, but hopefully
not too many, which lets us...

## Styx the schematic

...define Styx schemas in Styx itself.

```styx
schema {
  /// The root structure of a schema file.
  @ @object{
    /// Schema metadata (required).
    meta @Meta
    /// External schema imports (optional).
    imports @optional(@map(@string @string))
    /// Type definitions: @ for document root, strings for named types.
    schema @map(@union(@string @unit) @Schema)
  }
  
  // etc.
}
```

Are those doc comments? Yes. Parsers are taught to keep them and attach them to
the next element. This means your styx documents can be validated against a
schema:

  * by a CLI, locally, in CI
  * by an LSP, in your code editor
  * honestly anytime for any reason

And that your code editor (mine's [Zed](https://zed.dev)) can have the full
code editing experience: autocomplete, documentation on hover, jump to definition
(in schema), hover for field documentation, etc.

It's... so nice.

## Styx the one last thing

Oh! Also, HEREDOCs:

```styx
examples (
    {
        name hello.rs
        source <<SRC,rust
        fn main() {
          println!("Hello from Rust!")
        }
        SRC
    }
)
```

The `,rust` is just a hint which is used by your editor to inject syntax
highlighting from the embedded language :)

## Implementations

There is a spec for parsing, schema validation, and error reporting,
tracked with [Tracey](https://github.com/bearcove/tracey) and available
on the [styx website](https://styx.bearcove.eu).

The flagship implementation is, of course, the Rust one — across multiple
crates like `facet-styx` and `serde_styx`, but not just.

There's a TypeScript implementation in the repository, and probably more
to come.

## Editor Support

<p>
<a href="https://zed.dev">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="./static/sponsors/zed-dark.svg">
<img src="./static/sponsors/zed-light.svg" height="40" alt="Zed">
</picture>
</a>
</p>

Styx has first-class support for [Zed](https://zed.dev) with syntax highlighting, LSP integration, and more.

## Documentation

See [styx.bearcove.eu](https://styx.bearcove.eu) for full documentation.

## Sponsors

Thanks to all individual sponsors:

<p>
<a href="https://github.com/sponsors/fasterthanlime">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="./static/sponsors/github-dark.svg">
<img src="./static/sponsors/github-light.svg" height="40" alt="GitHub Sponsors">
</picture>
</a>
<a href="https://patreon.com/fasterthanlime">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="./static/sponsors/patreon-dark.svg">
<img src="./static/sponsors/patreon-light.svg" height="40" alt="Patreon">
</picture>
</a>
</p>

...along with corporate sponsors:

<p>
<a href="https://zed.dev">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="./static/sponsors/zed-dark.svg">
<img src="./static/sponsors/zed-light.svg" height="40" alt="Zed">
</picture>
</a>
<a href="https://depot.dev?utm_source=styx">
<picture>
<source media="(prefers-color-scheme: dark)" srcset="./static/sponsors/depot-dark.svg">
<img src="./static/sponsors/depot-light.svg" height="40" alt="Depot">
</picture>
</a>
</p>

CI runs on [Depot](https://depot.dev/) runners.

## License

MIT OR Apache-2.0
