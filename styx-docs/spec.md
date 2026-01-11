# styx

STYX is a structured document format designed to replace YAML, TOML, JSON, etc. for documents
authored by humans.

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

> r[document.root.empty]
> An empty document (containing only whitespace and comments) is valid and
> represents an empty object `{}`.

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

## Sequences

Sequences are ordered collections of values. They use `( )` delimiters.

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

> r[sequence.delimiters]
> Sequences MUST start with `(` and end with `)`.

> r[sequence.separators]
> Elements MUST be separated by whitespace or commas.
>
> ```styx
> (a b c)
> (a, b, c)
> (
>   a
>   b
>   c
> )
> ```

> r[sequence.elements]
> Sequence elements MAY be scalars, block objects, or nested sequences.
>
> ```compare
> /// json
> [[1, 2], [3, 4]]
> /// styx
> ((1 2) (3 4))
> ```
>
> ```compare
> /// json
> [{"name": "alice"}, {"name": "bob"}]
> /// styx
> (
>   { name alice }
>   { name bob }
> )
> ```

> r[sequence.empty]
> An empty sequence `()` is valid and represents an empty list.

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

> r[scalar.quoted.escapes]
> Quoted scalars MUST support the following escape sequences:
>
> | Escape | Meaning |
> |--------|---------|
> | `\\` | Backslash |
> | `\"` | Double quote |
> | `\n` | Newline |
> | `\r` | Carriage return |
> | `\t` | Tab |
> | `\0` | Null |
> | `\uXXXX` | Unicode code point (4 hex digits) |
> | `\u{X...}` | Unicode code point (1-6 hex digits) |
>
> Invalid escape sequences are an error.

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
> Any number of `#` characters is allowed (including zero: `r"..."`).
>
> ```styx
> r"simple"
> r#"contains "quotes""#
> r##"contains "# sequence"##
> ```

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
> The closing delimiter MUST appear on its own line, with only optional
> leading whitespace before it. Trailing whitespace after the delimiter is allowed.
>
> ```styx
> msg <<EOF
>   hello EOF   // ERROR: delimiter not on its own line
> ```

> r[scalar.heredoc.empty]
> A heredoc with no content lines produces an empty string.
>
> ```styx
> empty <<EOF
> EOF
> ```
>
> The value of `empty` is `""` (empty string).

### Scalar interpretation

The parser produces opaque scalar values. Interpretation is a separate layer.

A conforming implementation MUST provide standard interpretations for the following
scalar forms. These interpretations are applied during deserialization, not parsing.
The parser itself MUST NOT assign semantic meaning to scalars.

> r[scalar.interp.integer]
> A conforming implementation MUST recognize scalars matching this grammar as
> eligible for integer interpretation:
>
> ```
> integer = ["-" | "+"] digit+
> digit   = "0"..."9"
> ```
>
> Examples: `0`, `42`, `-10`, `+5`

> r[scalar.interp.float]
> A conforming implementation MUST recognize scalars matching this grammar as
> eligible for float interpretation:
>
> ```
> float    = integer "." digit+ [exponent] | integer exponent
> exponent = ("e" | "E") ["-" | "+"] digit+
> ```
>
> Examples: `3.14`, `-0.5`, `1e10`, `2.5e-3`

> r[scalar.interp.boolean]
> A conforming implementation MUST recognize `true` and `false` as eligible for
> boolean interpretation.

> r[scalar.interp.null]
> A conforming implementation MUST recognize `null` as eligible for null interpretation.

> r[scalar.interp.duration]
> A conforming implementation MUST recognize scalars matching this grammar as
> eligible for duration interpretation:
>
> ```
> duration = integer unit
> unit     = "ns" | "us" | "µs" | "ms" | "s" | "m" | "h" | "d"
> ```
>
> Units are case-sensitive; `30S` is not a valid duration.
>
> Examples: `30s`, `10ms`, `2h`, `500µs`

> r[scalar.interp.timestamp]
> A conforming implementation MUST recognize scalars matching RFC 3339 as
> eligible for timestamp interpretation:
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
> A conforming implementation MUST recognize scalars matching this grammar as
> eligible for regular expression interpretation:
>
> ```
> regex = "/" pattern "/" flags
> flags = ("i" | "m" | "s" | "x")*
> ```
>
> Flag order is insignificant (`/foo/im` equals `/foo/mi`). Duplicate flags
> are allowed but have no additional effect.
>
> Examples: `/foo/`, `/^hello$/i`, `/\d+/`

> r[scalar.interp.bytes.hex]
> A conforming implementation MUST recognize scalars matching this grammar as
> eligible for byte sequence interpretation:
>
> ```
> hex_bytes = "0x" hex_digit+
> hex_digit = "0"..."9" | "a"..."f" | "A"..."F"
> ```
>
> Examples: `0xdeadbeef`, `0x00FF`

> r[scalar.interp.bytes.base64]
> A conforming implementation MUST recognize scalars matching this grammar as
> eligible for byte sequence interpretation:
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

Keys are dotted paths composed of one or more segments.

> r[object.key.syntax]
> A key MUST match the following grammar:
>
> ```
> key     = segment ("." segment)*
> segment = bare | quoted
> bare    = [A-Za-z_][A-Za-z0-9_-]*
> quoted  = '"' ... '"'
> ```

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

Mixed dotted paths with quoted segments:

```compare
/// json
{"key with spaces": {"still": {"dotted": "value"}}}
/// styx
"key with spaces".still.dotted value
```

> r[object.key.dotted.expansion]
> A dotted path MUST expand to nested singleton objects.
> `a.b.c value` expands to `a { b { c value } }`.

> r[object.key.dotted.no-reopen]
> A dotted path MUST NOT introduce a key whose parent object already contains a different key.
>
> ```styx
> server.host localhost
> server.port 8080   // ERROR: cannot reopen server to add port
> ```
>
> Use block form instead:
>
> ```styx
> server {
>   host localhost
>   port 8080
> }
> ```

> r[object.key.duplicate]
> Duplicate keys within the same object are forbidden.
>
> ```styx
> server {
>   port 8080
>   port 9090   // ERROR: duplicate key
> }
> ```

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

> r[object.block.empty]
> An empty object `{}` is valid and represents an object with no entries.

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

> r[object.attr.key]
> Attribute keys follow the same grammar as object keys. Quoted keys are valid:
>
> ```styx
> config "quoted key"=value foo=bar
> ```
>
> Dotted paths in attribute keys expand as expected:
>
> ```styx
> server.host=localhost
> ```
>
> expands to `server { host localhost }`.

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

> r[object.attr.sequence.forbidden]
> Attribute objects MUST NOT appear as direct elements of a sequence.
>
> ```styx
> (
>   a=1 b=2   // ERROR: attribute object as sequence element
> )
> ```
>
> Use block objects instead:
>
> ```styx
> (
>   { a=1 b=2 }
> )
> ```

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
> An enum object MUST contain exactly one key. This constraint is enforced
> during deserialization when the target type is known to be an enum.
> The parser itself cannot distinguish enum objects from regular objects.

> r[enum.singleton.dotted]
> Dotted paths used for enum syntax MUST only traverse singleton objects.
> This is a structural consequence of the expansion rule, not a parser constraint.

A variant payload may be omitted (unit variant), or may be a scalar, block object,
or sequence:

```styx
result.ok
```

```styx
result.err message="timeout" retry_in=5s
```

The parser produces the same object structure regardless of whether the target
is an enum. Schema validation (at deserialization time) enforces:
- Which objects are enums
- Valid variant names
- Payload shapes
- Whether unit variants are allowed

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

## Design invariants (non-normative)

STYX enforces the following invariants:

- **No implicit merges**: Objects are never merged. Each key appears exactly once.
- **No reopening**: Once an object is closed, it cannot be extended with additional keys.
- **No indentation-based structure**: All structure is explicit via `{}` and `()`.
- **No semantic interpretation during parsing**: The parser produces opaque scalars; meaning is assigned during deserialization.
- **All structure is explicit**: Braces and parentheses define nesting, not whitespace or conventions.
- **Commas are separators only**: Commas have no semantic meaning; they are interchangeable with newlines for readability.
