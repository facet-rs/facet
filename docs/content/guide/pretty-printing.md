+++
title = "Pretty-printing"
weight = 5
insert_anchor_links = "heading"
+++

[`facet-pretty`](https://docs.rs/facet-pretty) renders any `Facet` value as
readable, colored, structured text — without requiring `Debug` to be derived,
and with first-class redaction of sensitive fields.

## Setup

```bash
cargo add facet facet-pretty
```

Bring the extension trait into scope; it's implemented for every `Facet` type:

```rust,noexec
use facet::Facet;
use facet_pretty::FacetPretty;

#[derive(Facet)]
struct Person {
    name: String,
    age: u32,
    address: Address,
}

#[derive(Facet)]
struct Address {
    street: String,
    city: String,
}

let p = Person {
    name: "Alice".into(),
    age: 30,
    address: Address { street: "123 Main St".into(), city: "Wonderland".into() },
};

println!("{}", p.pretty());
```

```text
Person {
  name: "Alice",
  age: 30,
  address: Address {
    street: "123 Main St",
    city: "Wonderland",
  },
}
```

`.pretty()` returns a `Display` wrapper, so it works anywhere `{}` does —
`println!`, `tracing` fields, error messages.

## Redacting sensitive fields

Mark a field `#[facet(sensitive)]` and pretty-printing replaces its value with a
placeholder. The secret never reaches your logs:

```rust,noexec
#[derive(Facet)]
struct Config {
    host: String,
    #[facet(sensitive)]
    api_key: String,
}
```

```text
Config {
  host: "example.com",
  api_key: [REDACTED],
}
```

## Customizing output

`.pretty_with(printer)` takes a `PrettyPrinter` for indentation and styling:

```rust,noexec
use facet_pretty::PrettyPrinter;

let printer = PrettyPrinter::new().with_indent("    ");
println!("{}", value.pretty_with(printer));
```

Why not just `#[derive(Debug)]`? Because the same `Facet` derive also gives you
serialization, diffing, schema generation, and CLI parsing — and because
`sensitive` redaction is enforced for free. See the
[pretty showcase](/showcases/pretty) for live output.

## Related

- [rediff](https://docs.rs/rediff) — structural diffing with the same no-`PartialEq` philosophy
- [Ecosystem](@/ecosystem/_index.md) — diagnostics & derive plugins
