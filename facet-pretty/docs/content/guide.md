+++
title = "Pretty-printing"
description = "Render Facet values as readable structured text, including redaction for sensitive fields."
weight = 1
insert_anchor_links = "heading"
+++

`facet-pretty` renders any `Facet` value as readable, structured text without
requiring `Debug`. Reach for it when you want friendly diagnostics, snapshots,
logs, or quick inspection with the same shape information the rest of facet uses.

## Install

Add `facet` and `facet-pretty` to your crate.

## Minimal example

```rust
use facet::Facet;
use facet_pretty::FacetPretty;

#[derive(Facet)]
struct Person {
    name: String,
    age: u32,
}

let person = Person {
    name: "Alice".to_string(),
    age: 30,
};

let rendered = person.pretty().to_string();
assert!(rendered.contains("Person"));
assert!(rendered.contains("Alice"));
```

`.pretty()` returns a `Display` wrapper, so it works anywhere `{}` formatting
does: diagnostics, tracing fields, test output, or a tiny debug view.

## Redaction

Mark a field `#[facet(sensitive)]` and `facet-pretty` replaces its value with
`[REDACTED]` while keeping the field name visible:

```rust
use facet::Facet;
use facet_pretty::FacetPretty;

#[derive(Facet)]
struct Config {
    host: String,

    #[facet(sensitive)]
    api_key: String,
}

let config = Config {
    host: "example.com".to_string(),
    api_key: "secret".to_string(),
};

let rendered = config.pretty().to_string();
assert!(rendered.contains("api_key"));
assert!(rendered.contains("[REDACTED]"));
assert!(!rendered.contains("secret"));
```

That makes it a good fit for operational output where secrets must never take a
surprise stroll through the logs.

## Custom output

Use `PrettyPrinter` when you want explicit formatting settings:

```rust
use facet::Facet;
use facet_pretty::{FacetPretty, PrettyPrinter};

#[derive(Facet)]
struct Point {
    x: i32,
    y: i32,
}

let point = Point { x: 1, y: 2 };
let printer = PrettyPrinter::new().with_indent_size(4);

let rendered = point.pretty_with(printer).to_string();
assert!(rendered.contains("Point"));
```

`PrettyPrinter::format(&value)` is available too, if you prefer asking the
printer for a `String` directly.

## Related

- [facet-error](/facet-error/guide/) — derive displayable error types from doc comments
- [facet-json](/facet-json/guide/) — serialize the same values as JSON
- [facet-validate](/facet-validate/guide/) — reject invalid values before printing them
- [Ecosystem](/ecosystem/) — diagnostics, derive plugins, and format crates
