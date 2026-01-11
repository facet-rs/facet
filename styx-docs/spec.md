# styx

STYX is a document language designed to replace YAML, TOML, JSON, etc. for documents authored
by humans.

## Document structure

A STYX document is an [object](#styx--objects). Top-level entries do not require braces.

> r[document.root]
> The parser MUST interpret top-level key-value pairs as entries of an implicit root object.
>
> ```compare
> /// json
> {
>   "server": {
>     "host": "localhost",
>     "port": 8080
>   },
>   "database": {
>     "url": "postgres://..."
>   }
> }
> /// styx
> server {
>   host localhost
>   port 8080
> }
> database {
>   url "postgres://..."
> }
> ```

> r[document.root.explicit]
> If the document starts with `{`, it MUST be a single block object.
> The closing `}` MUST be the end of the document.
>
> ```compare
> /// json
> {
>   "key": "value"
> }
> /// styx
> {
>   key value
> }
> ```

> r[document.root.trailing]
> The parser MUST reject tokens after the root object.
>
> ```styx
> {
>   key value
> }
> 42   // ERROR: unexpected token after root
> ```

## Comments

Line comments start with `//` and extend to the end of the line.

> r[comment.line]
> The parser MUST ignore content from `//` to the end of the line.
>
> ```compare
> /// json
> {
>   "server": {
>     "host": "localhost",
>     "port": 8080
>   }
> }
> /// styx
> server {
>   host localhost  // primary host
>   port 8080       // default port
> }
> ```

> r[comment.placement]
> The parser MUST allow comments anywhere whitespace is allowed.

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

```compare
/// json
"foo"
/// styx
foo
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

### Quoted scalars

Quoted scalars use double quotes and support escape sequences.

```compare
/// json
"hello world"
/// styx
"hello world"
```

```compare
/// json
"foo\nbar"
/// styx
"foo\nbar"
```

### Raw scalars

Raw scalars preserve content literally. JSON has no equivalent.

```compare
/// json
"no need to escape \"double quotes\" in here"
/// styx
r#"no need to escape "double quotes" in here"#
```

> r[scalar.raw.delimiter]
> The number of `#` in the closing delimiter MUST match the opening.

### Heredoc scalars

Heredocs are multiline scalars. JSON has no equivalent.

```compare
/// json
"line one\nline two"
/// styx
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
>
> ```styx
> server {
>   script <<BASH
>     #!/bin/bash
>     echo "hello"
>     BASH
> }
> ```
>
> The closing `BASH` is indented 4 spaces, so 4 spaces are stripped.
> The value of `script` is `#!/bin/bash\necho "hello"`.

> r[scalar.heredoc.indent.minimum]
> All content lines MUST be indented at least as much as the closing delimiter.
>
> ```styx
> server {
>   script <<BASH
> #!/bin/bash   // ERROR: less indented than closing delimiter
>     BASH
> }
> ```

> r[scalar.heredoc.chomp]
> The parser MUST strip the trailing newline immediately before the closing delimiter.
>
> ```styx
> msg <<EOF
>   hello
>   EOF
> ```
>
> The value of `msg` is `hello` (no trailing newline).

> r[scalar.heredoc.closing]
> The closing delimiter MUST appear on its own line.
>
> ```styx
> msg <<EOF
>   hello EOF   // ERROR: delimiter not on its own line
> ```

### Scalar interpretation

A conforming implementation MUST support interpreting scalars as the following types.

> r[scalar.interp.integer]
> A conforming implementation MUST interpret scalars matching this grammar as integers:
>
> ```
> integer = ["-" | "+"] digit+
> digit   = "0"..."9"
> ```
>
> Examples: `0`, `42`, `-10`, `+5`

> r[scalar.interp.float]
> A conforming implementation MUST interpret scalars matching this grammar as floats:
>
> ```
> float    = integer "." digit+ [exponent] | integer exponent
> exponent = ("e" | "E") ["-" | "+"] digit+
> ```
>
> Examples: `3.14`, `-0.5`, `1e10`, `2.5e-3`

> r[scalar.interp.boolean]
> A conforming implementation MUST interpret `true` and `false` as booleans.

> r[scalar.interp.null]
> A conforming implementation MUST interpret `null` as the null value.

> r[scalar.interp.duration]
> A conforming implementation MUST interpret scalars matching this grammar as durations:
>
> ```
> duration = integer unit
> unit     = "ns" | "us" | "µs" | "ms" | "s" | "m" | "h" | "d"
> ```
>
> Examples: `30s`, `10ms`, `2h`, `500µs`

> r[scalar.interp.timestamp]
> A conforming implementation MUST interpret scalars matching RFC 3339 as timestamps:
>
> ```
> timestamp = date "T" time timezone
> date      = year "-" month "-" day
> time      = hour ":" minute ":" second ["." fraction]
> timezone  = "Z" | ("+" | "-") hour ":" minute
> ```
>
> Examples: `2026-01-10T18:43:00Z`, `2026-01-10T12:00:00-05:00`

> r[scalar.interp.regex]
> A conforming implementation MUST interpret scalars matching this grammar as regular expressions:
>
> ```
> regex = "/" pattern "/" flags
> flags = ("i" | "m" | "s" | "x")*
> ```
>
> Examples: `/foo/`, `/^hello$/i`, `/\d+/`

> r[scalar.interp.bytes.hex]
> A conforming implementation MUST interpret scalars matching this grammar as byte sequences:
>
> ```
> hex_bytes = "0x" hex_digit+
> hex_digit = "0"..."9" | "a"..."f" | "A"..."F"
> ```
>
> Examples: `0xdeadbeef`, `0x00FF`

> r[scalar.interp.bytes.base64]
> A conforming implementation MUST interpret scalars matching this grammar as byte sequences:
>
> ```
> base64_bytes = "b64" '"' base64_char* '"'
> ```
>
> Examples: `b64"SGVsbG8="`, `b64""`

Implementations commonly support additional forms like paths, URLs, IPs, and semver.

## Objects

Objects are key-value maps.

### Keys

Keys are bare identifiers, quoted strings, or dotted paths.

```compare
/// json
{"foo": "value"}
/// styx
foo value
```

```compare
/// json
{"foo bar": "value"}
/// styx
"foo bar" value
```

```compare
/// json
{"foo": {"bar": "value"}}
/// styx
foo.bar value
```

```compare
/// json
{"foo.bar": "value"}
/// styx
"foo.bar" value
```

> r[object.key.bare]
> A bare key MUST match `[A-Za-z_][A-Za-z0-9_-]*`.

> r[object.key.dotted]
> The parser MUST recognize dotted paths as key segments separated by `.`.
> Each segment MUST be a bare key or a quoted string.

> r[object.key.dotted.expansion]
> A dotted path `a.b.c value` MUST expand to nested objects: `a { b { c value } }`.

### Block form

Block objects use `{ }` delimiters. Entries are separated by newlines or commas.

```compare
/// json
{
  "name": "my-app",
  "version": "1.0.0",
  "enabled": true
}
/// styx
{
  name "my-app"
  version 1.0.0
  enabled true
}
```

Nested objects:

```compare
/// json
{
  "server": {
    "host": "localhost",
    "port": 8080
  },
  "database": {
    "url": "postgres://localhost/mydb",
    "pool_size": 10
  }
}
/// styx
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

```compare
/// json
{
  "labels": {
    "app": "web",
    "tier": "frontend"
  }
}
/// styx
labels app=web tier=frontend
```

Values can be scalars, block objects, or sequences:

```compare
/// json
{
  "server": {
    "host": "localhost",
    "port": 8080
  }
}
/// styx
server host=localhost port=8080
```

```compare
/// json
{
  "build": {
    "components": ["clippy", "rustfmt", "miri"]
  }
}
/// styx
build components=(clippy rustfmt miri)
```

> r[object.attr.binding]
> `=` binds tighter than whitespace. When the parser encounters `key=` in a
> value position, it MUST parse an attribute object.

> r[object.attr.value]
> The value after `=` MUST be exactly one value.

> r[object.attr.termination]
> The parser MUST terminate an attribute object when the next token is not of the form `key=`.

Attribute objects work well for inline key-value patterns like labels,
environment variables, and options. For complex or nested structures, use block form.

### Attribute objects in sequences

Inside a sequence, use block objects:

```compare
/// json
[
  {"labels": {"app": "web", "tier": "frontend"}},
  {"labels": {"app": "api", "tier": "backend"}}
]
/// styx
(
  { labels app=web tier=frontend }
  { labels app=api tier=backend }
)
```

### Equivalence

Both forms produce the same object value:

```compare
/// styx
config host=localhost port=8080
/// styx
config {
  host localhost
  port 8080
}
```

## Enums

Enums are a schema-level concept. The core language provides structural representation
via externally tagged objects.

An enum value is represented as an object with exactly one key (the variant tag)
whose value is the payload:

> r[enum.representation]
> An enum object MUST contain exactly one key.
>
> ```compare
> /// json
> {"ok": {}}
> /// styx
> { ok {} }
> ```
>
> ```compare
> /// json
> {"err": {"message": "nope", "retry_in": "5s"}}
> /// styx
> { err { message "nope", retry_in 5s } }
> ```

Enum variants may be written using dotted path syntax:

```styx
status.ok
```

```styx
status.err { message "nope" }
```

> r[enum.dotted-path]
> The parser MUST expand `status.ok` to `status { ok {} }` and 
> `status.err { ... }` to `status { err { ... } }`.

> r[enum.singleton]
> Enum objects MUST contain exactly one variant.

> r[enum.singleton.dotted]
> Dotted paths MUST only traverse singleton objects.

A variant payload may be omitted (unit variant), or may be a scalar, block object,
or sequence:

```styx
result.ok
```

```styx
result.err message="timeout" retry_in=5s
```

Schemas define which objects are enums, valid variant names, payload shapes,
and whether unit variants are allowed. The core language only enforces structural
rules.

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
