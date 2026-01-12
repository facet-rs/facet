+++
title = "STYX"
insert_anchor_links = "heading"
+++

# STYX

A structured document format for humans.

STYX replaces YAML, TOML, and JSON for configuration files and data authored by people.
It uses explicit delimiters instead of indentation, keeps scalars opaque until deserialization,
and provides modern conveniences like heredocs and tagged values.

```styx
server {
  host localhost
  port 8080
  tls {
    cert /etc/ssl/cert.pem
    key /etc/ssl/key.pem
  }
}

features (auth logging metrics)
```

## Quick examples

### Objects

```compare
/// json
{
  "name": "alice",
  "age": 30
}
/// styx
{name alice, age 30}
```

### Sequences

```compare
/// json
["a", "b", "c"]
/// styx
(a b c)
```

### Tagged values

```compare
/// json
{"$tag": "rgb", "$values": [255, 128, 0]}
/// styx
@rgb(255 128 0)
```

## Why STYX?

- **Explicit structure** — Braces and parentheses, not indentation
- **Two-layer processing** — Parser handles structure, deserializer handles types
- **Opaque scalars** — `42` is text until you deserialize it
- **Modern features** — Heredocs, raw strings, tagged values, schemas

## Documentation

- [Parser Spec](/spec/parser) — Formal syntax rules
- [Schema Spec](/spec/schema) — Type system and validation
- [Diagnostics](/spec/diagnostics) — Error message standards
- [Rust Bindings](/bindings/rust) — How Rust types map to STYX
- [Comparisons](/comparisons) — vs JSON, YAML, TOML, KDL
