+++
title = "Primer"
weight = 1
slug = "primer"
insert_anchor_links = "heading"
+++

STYX is a structured document format designed to replace YAML, TOML, JSON, etc. for documents
authored by humans.

## Overview

STYX processing happens in two distinct layers:

- **Parser**: Converts text into a document tree of scalars, objects, and sequences.
  The parser treats all scalars as opaque text — it assigns no semantic meaning to values
  like `42` or `true`. The parser's job is purely structural.

- **Deserializer**: Converts the document tree into typed application values.
  The deserializer interprets scalars based on target types — `42` becomes an integer
  when deserializing into `u32`, a string when deserializing into `String`.

This separation keeps the parser simple and predictable while allowing flexible
interpretation at the application layer.

**Key design choice**: STYX uses explicit delimiters (`{}` and `()`) for all structure.
There is no indentation-based nesting — whitespace is purely for separation, never for structure.
This eliminates an entire class of formatting bugs common in YAML.

## Primer

This section introduces STYX by example. Formal rules follow in subsequent sections.

### Basic values

Scalars are atoms — strings, numbers, booleans. STYX treats them all as opaque text;
interpretation happens during deserialization.

```compare
/// json
"hello"
/// styx
hello
```

```compare
/// json
42
/// styx
42
```

```compare
/// json
true
/// styx
true
```

### Objects

Objects map keys to values. Use `{}` braces.

```compare
/// json
{
  "name": "alice",
  "age": 30
}
/// styx
{ name alice, age 30 }
```

Objects can be nested:

```compare
/// json
{
  "server": {
    "host": "localhost",
    "port": 8080
  }
}
/// styx
{
  server {
    host localhost
    port 8080
  }
}
```

At the top level, braces are optional — a STYX document *is* an object:

```styx
server {
  host localhost
  port 8080
}
database {
  url "postgres://..."
}
```

### Sequences

Sequences are ordered lists. Use `()` parentheses.

```compare
/// json
["a", "b", "c"]
/// styx
(a b c)
```

```compare
/// json
[1, 2, 3]
/// styx
(1 2 3)
```

### Tagged values

A tag immediately before `(` or `{` (no space) creates a tagged value.

```compare
/// json
{"$tag": "rgb", "$values": [255, 128, 0]}
/// styx
rgb(255 128 0)
```

```compare
/// json
{"$tag": "point", "x": 1, "y": 2}
/// styx
point{ x 1, y 2 }
```

### Unit

The unit value `@` represents absence — like `null` but structural.

```compare
/// json
{"enabled": null}
/// styx
enabled @
```

Keys without values implicitly get `@`:

```styx
enabled        // equivalent to: enabled @
```
