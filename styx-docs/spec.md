# styx

STYX is a document language designed to replaced YAML, TOML, JSON, etc. for documents authored
by humans.

## Value types

STYX values are one of:

  * Scalar
  * Object
  * Sequence
  
## Scalars

Scalars are opaque atoms. The parser assigns no meaning to them; interpretation
is deferred until deserialization. Quoted forms are lexical delimiters — they
allow spaces and special characters but don't change meaning. `foo` and `"foo"`
produce identical values.

### Bare scalars

Bare scalars are delimited by whitespace.

```styx
foo
42
true
https://example.com/path
```

### Quoted scalars

Quoted scalars use double quotes and support escape sequences.

```styx
"hello world"
"foo\nbar"
```

### Raw scalars

Raw scalars preserve content literally.

```styx
r#"no need to escape "double quotes" in here"#
```

> r[scalar.raw.delimiter]
> The number of `#` in the closing delimiter MUST match the opening.

### Heredoc scalars

Heredocs are multiline scalars.

```styx
<<EOF
line one
line two
EOF
```

> r[scalar.heredoc.delimiter]
> The delimiter MUST match the pattern `[A-Z_]+`.

> r[scalar.heredoc.indent]
> The parser MUST strip leading whitespace from content lines up to the
> closing delimiter's indentation level.

> r[scalar.heredoc.indent.minimum]
> All content lines MUST be indented at least as much as the closing delimiter.

> r[scalar.heredoc.chomp]
> The parser MUST strip the trailing newline immediately before the closing delimiter.

> r[scalar.heredoc.closing]
> The closing delimiter MUST appear on its own line.

### Scalar interpretation

> r[scalar.interpretation]
> A conforming implementation MUST support interpreting scalars as:
>
> - Integers (signed/unsigned, various widths)
> - Floating point numbers
> - Booleans (`true`, `false`)
> - Null (`null`)
> - Strings
> - Durations (`30s`, `10ms`, `2h`)
> - Timestamps (RFC 3339)
> - Regular expressions (`/foo/i`)
> - Byte sequences (hex `0xdeadbeef`, base64 `b64"..."`)

Implementations commonly support additional forms like paths, URLs, IPs, and semver.

## Objects

Objects are key-value maps.

### Keys

Keys are bare identifiers, quoted strings, or dotted paths.

```styx
foo value             // bare key
"foo bar" value       // quoted key (contains space)
foo.bar value         // dotted path: foo -> bar
"foo".bar value       // dotted path: "foo" -> bar
"foo.bar" value       // quoted key (literal dot, no path expansion)
```

> r[object.key.bare]
> A bare key MUST match `[A-Za-z_][A-Za-z0-9_-]*`.

> r[object.key.dotted]
> A dotted path is a sequence of key segments separated by `.`.
> Each segment MUST be a bare key or a quoted string.

> r[object.key.dotted.expansion]
> A dotted path `a.b.c value` MUST expand to nested objects: `a { b { c value } }`.

### Block form

Block objects use `{ }` delimiters. Entries are separated by newlines or commas.

```styx
{
  name "my-app"
  version 1.0.0
  enabled true
}
```

```styx
{ name "my-app", version 1.0.0, enabled true }
```

Nested objects:

```styx
{
  server {
    host localhost
    port 8080
  }
  database {
    url "postgres://localhost/mydb"
    pool_size 10
  }
}
```

> r[object.block.delimiters]
> Block objects MUST start with `{` and end with `}`.

> r[object.block.separators]
> Entries MUST be separated by newlines or commas.

### Attribute form

Attribute objects use `key=value` syntax. They are sugar for block objects.

```styx
labels app=web tier=frontend
```

is equivalent to:

```styx
labels {
  app web
  tier frontend
}
```

Values can be scalars, block objects, or sequences:

```styx
server host=localhost port=8080
server config={ host localhost, port 8080 }
build components=(clippy rustfmt miri)
```

> r[object.attr.binding]
> `=` binds tighter than whitespace. When the parser encounters `key=` in a
> value position, it MUST parse an attribute object.

> r[object.attr.value]
> The value after `=` MUST be exactly one value.

> r[object.attr.termination]
> An attribute object ends when a token is not of the form `key=`.

Attribute objects work well for inline key-value patterns like labels,
environment variables, and options. For complex or nested structures, use block form.

### Attribute objects in sequences

Inside a sequence, use block objects:

```styx
(
  { labels app=web tier=frontend }
  { labels app=api tier=backend }
)
```

### Equivalence

Both forms produce the same object value:

```styx
config host=localhost port=8080

config {
  host localhost
  port 8080
}
```

## Enums

Enums are a schema-level concept. The core language provides structural representation
via externally tagged objects.

> r[enum.representation]
> An enum value is represented as an object with exactly one key (the variant tag)
> whose value is the payload:
>
> ```styx
> { ok {} }
> ```
>
> ```styx
> { err { message "nope", retry_in 5s } }
> ```

> r[enum.dotted-path]
> Enum variants may be written using dotted path syntax:
>
> ```styx
> status.ok
> ```
>
> ```styx
> status.err { message "nope" }
> ```
>
> This is syntactic sugar for:
>
> ```styx
> status { ok {} }
> ```
>
> ```styx
> status { err { message "nope" } }
> ```

> r[enum.singleton]
> Enum objects MUST contain exactly one variant. Dotted paths may only traverse
> singleton objects.

> r[enum.no-reopen]
> Enum objects MUST NOT be reopened or merged:
>
> ```styx
> // Invalid: attempts to add second variant
> status.ok
> status.err { message "nope" }
> ```

> r[enum.payload]
> A variant payload may be omitted (unit variant), or may be a scalar, block object,
> or sequence:
>
> ```styx
> result.ok
> ```
>
> ```styx
> result.err message="timeout" retry_in=5s
> ```

> r[enum.explicit]
> Enum interpretation must be structurally explicit. The following is ambiguous
> and not a valid enum representation:
>
> ```styx
> // Ambiguous: string value or enum variant?
> status ok
> ```

> r[enum.schema]
> Schemas define which objects are enums, valid variant names, payload shapes,
> and whether unit variants are allowed. The core language only enforces structural
> rules (singleton objects, no reopening).

## Usage patterns (non-normative)

This section illustrates how applications interact with STYX documents. Since the core
language treats scalars as opaque atoms, interpretation happens at the application layer.

### Dynamic access

Parse into a generic document tree and interpret values on demand:

```rust
let doc: styx::Document = styx::parse(r#"
    server {
        host localhost
        port 8080
        timeout 30s
    }
"#)?;

// Caller decides how to interpret each scalar
let host = doc["server"]["host"].as_str()?;
let port = doc["server"]["port"].as_u16()?;
let timeout = doc["server"]["timeout"].as_duration()?;
```

This approach is useful for:
- Tools that process arbitrary STYX documents
- Exploratory parsing where the schema is unknown
- Gradual migration from other formats

### Typed deserialization

Deserialize directly into concrete types. The type system guides scalar interpretation:

```rust
use std::time::Duration;

#[derive(styx::Deserialize)]
struct Config {
    server: Server,
}

#[derive(styx::Deserialize)]
struct Server {
    host: String,
    port: u16,
    timeout: Duration,
}

let config: Config = styx::from_str(r#"
    server {
        host localhost
        port 8080
        timeout 30s
    }
"#)?;

assert_eq!(config.server.port, 8080);
assert_eq!(config.server.timeout, Duration::from_secs(30));
```

This approach is useful for:
- Application configuration with known schemas
- Type-safe access with compile-time guarantees
- Automatic validation via Rust's type system

### Schema as the interpreter

In both patterns, the "schema" — whether explicit types or runtime `.as_*()` calls —
determines how scalars are interpreted. The STYX parser produces the same document tree
regardless of how it will be consumed.
