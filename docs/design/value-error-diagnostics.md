# Value Error Diagnostics

## Problem

Currently, `from_value` errors are plain text with no context:

```
number out of range: 1000 out of range for u8
```

We want miette-style diagnostics showing:
1. Where in the Value the error occurred
2. Where in the target type we were deserializing to

## Goal

```
error: number out of range
  ┌─ input value
  │
1 │ { "config": { "max_retries": 1000 } }
  │                              ^^^^ 1000 is out of range for u8 (0..255)
  │
  ┌─ target type
  │
1 │ struct Config { max_retries: u8 }
  │                              ^^ expected u8
```

## Approach

Keep it simple: `format_shape` tracks spans for everything as it formats,
returns a mapping from field paths to byte spans in the output. Shapes are
small, overhead is negligible.

### Core idea

```rust
/// A path to a location within a shape
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PathSegment {
    Field(&'static str),
    Variant(&'static str),
    Index(usize),
}

pub type Path = Vec<PathSegment>;

/// Result of formatting a shape with span tracking
pub struct FormattedShape {
    /// The formatted text (no ANSI - plain text for miette)
    pub text: String,
    /// Map from paths to their byte spans in `text`
    pub spans: HashMap<Path, (usize, usize)>,
}

pub fn format_shape_with_spans(shape: &'static Shape) -> FormattedShape {
    // Format the shape, tracking byte offsets for each field/variant
}
```

### Usage with miette

When an error occurs during deserialization:

1. We have the path where it failed (e.g., `["config", "max_retries"]`)
2. Call `format_shape_with_spans` on the target type
3. Look up the span for that path
4. Pass to miette as source + labeled span

Same approach for the Value side - format it with span tracking.

### What we need

1. **`format_shape_with_spans`** - formats a Shape, returns text + path-to-span map
2. **`format_value_with_spans`** - formats a Value, returns text + path-to-span map
3. **Path tracking in deserializer** - know where we are when error occurs
4. **`ValueError` implements `Diagnostic`** - wires it all together

## Starting Point

Add span tracking to `format_shape`. The formatter already walks the shape
recursively - just track byte offsets as we go and record them in a HashMap.
