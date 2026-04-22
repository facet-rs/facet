+++
title = "Formatting"
weight = 7
slug = "format"
insert_anchor_links = "heading"
+++

# Formatting Requirements

This document specifies the formatting behavior for Styx documents.

## Line-Length Based Pretty Printing

> format[format.line-length]
> When formatting Styx documents, lines SHOULD not exceed a maximum width.
> When a structure would exceed this width if formatted inline, it SHOULD be
> expanded to multiline format to maintain readability.
>
> The default maximum line width SHOULD be 80 characters, consistent with
> common coding standards.

> format[format.line-length.default]
> The default maximum line width for pretty printing SHOULD be 80 characters.

> format[format.line-length.preserve]
> Existing line breaks in the input SHOULD be preserved in the output to
> maintain document structure and intentional formatting.

## Implementation

The formatting behavior is controlled through the following mechanisms:

### CLI Flags

- `--pretty`: Enable line-length based pretty printing
- `--line-length <N>`: Customize the maximum line length (default: 80)
- `--multiline`: Force all structures to be multiline (aggressive expansion)
- `--compact`: Force all structures to be inline (minimal expansion)

### Programmatic API

```rust
use facet_styx::{to_string_pretty, SerializeOptions};

// Use default pretty printing (80 character limit)
let pretty_output = to_string_pretty(&value)?;

// Customize line length
let options = SerializeOptions::default().pretty(100);
let custom_output = to_string_with_options(&value, &options)?;
```

## Examples

### Simple Structure (Fits Within Line Limit)

**Input:**
```styx
config {name "test", port 8080}
```

**Output with `--pretty`:**
```styx
config {name "test", port 8080}
```

### Complex Structure (Exceeds Line Limit)

**Input:**
```styx
server {host "localhost", port 8080, timeout 30, max_connections 100, ssl_enabled true}
```

**Output with `--pretty`:**
```styx
server {
    host "localhost"
    port 8080
    timeout 30
    max_connections 100
    ssl_enabled true
}
```

### Nested Structures

**Input:**
```styx
config {server {host "localhost", port 8080}, database {url "postgres://localhost/mydb"}}
```

**Output with `--pretty`:**
```styx
config {
    server {
        host "localhost"
        port 8080
    }
    database {
        url "postgres://localhost/mydb"
    }
}
```

## Behavior Details

### Object Expansion

An object is expanded to multiline format when:
- Its inline representation would exceed the maximum line width
- It contains doc comments or line comments
- It contains nested block objects
- It's explicitly forced with `--multiline`

### Sequence Expansion

A sequence is expanded to multiline format when:
- Its inline representation would exceed the maximum line width
- It contains comments
- It's explicitly forced with `--multiline`

### Preservation Rules

- Existing line breaks in the input are preserved
- Comments are always preserved and affect formatting
- Simple structures that fit within line limits remain inline
- The formatter is idempotent (running it multiple times produces the same output)

## Configuration

The formatting behavior can be configured through:

1. **CLI flags** (as shown above)
2. **Programmatic options** via `SerializeOptions`
3. **Environment variables** (future enhancement)
4. **Configuration files** (future enhancement)

## Relationship to Other Specifications

The formatting specification builds upon:

- **Parser specification**: Defines how Styx source is parsed
- **Schema specification**: Defines schema structure and validation
- **Diagnostics specification**: Defines error reporting format

The formatter operates on the parsed CST (Concrete Syntax Tree) and preserves all semantic information while applying formatting rules.