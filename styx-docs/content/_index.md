+++
title = "STYX"
insert_anchor_links = "heading"
+++

# STYX

A structured document format for humans.

STYX replaces YAML, TOML, and JSON for configuration files and data authored by people. It uses explicit delimiters instead of indentation, keeps scalars opaque until deserialization, and provides modern conveniences like heredocs and tagged values.

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

## Why STYX?

- **Explicit structure** - Braces and parentheses, not indentation
- **Two-layer processing** - Parser handles structure, deserializer handles types
- **Opaque scalars** - `42` is text until you deserialize it
- **Modern features** - Heredocs, raw strings, tagged values, schemas

## Documentation

- [Primer](/spec/primer) - Introduction by example
- [Parser Spec](/spec/parser) - Formal syntax rules
- [Schema Spec](/spec/schema) - Type system and validation
- [Examples](/examples) - Real-world usage patterns
