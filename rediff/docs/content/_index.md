+++
title = "rediff"
weight = 36
insert_anchor_links = "heading"
+++

`rediff` compares `Facet` values structurally and reports detailed diffs.

It is useful in tests where `PartialEq` is unavailable, too blunt, or gives too
little context when values differ. Rediff walks the reflected shape of both
values and renders the difference with paths, structure, and terminal-friendly
formatting.

```rust
use facet::Facet;
use rediff::assert_same;

#[derive(Facet)]
struct Config {
    host: String,
    port: u16,
}

let expected = Config {
    host: "localhost".to_owned(),
    port: 8080,
};

let actual = Config {
    host: "localhost".to_owned(),
    port: 8080,
};

assert_same!(expected, actual);
```

Source: [`rediff`](https://github.com/facet-rs/facet/tree/main/rediff)
