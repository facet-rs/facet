# STYX

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

---

# Part 1: Parser

The parser converts STYX source text into a document tree. It handles syntax, structure,
and produces opaque scalar values.

## Document structure

A STYX document is an object. Top-level entries do not require braces.

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

> r[document.root.separators]
> Root entries MUST follow the same separator rules as block objects: entries
> are separated by newlines or commas.

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

The parser produces three types of values:

  * **Scalar** — an opaque text atom
  * **Object** — an ordered map of keys to values
  * **Sequence** — an ordered list of values

## Scalars

Scalars are opaque atoms. The parser assigns no meaning to them; interpretation
is deferred until deserialization. Quoted forms are lexical delimiters — they
allow spaces and special characters but don't change meaning. `foo` and `"foo"`
produce identical scalar values.

### Bare scalars

Bare scalars are delimited by whitespace and structural characters.

> r[scalar.bare.termination]
> A bare scalar is terminated by whitespace or any of: `{`, `}`, `(`, `)`, `=`, `,`.
>
> ```styx
> items(a b c)     // "items" is key, (a b c) is value
> foo{bar baz}     // "foo" is key, {bar baz} is value
> x=1              // in attribute context: "x" is key, "1" is value
> ```

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
> Elements MUST be separated by whitespace.
>
> ```styx
> (a b c)
> (
>   a
>   b
>   c
> )
> ```
>
> A single-element sequence is valid: `(foo)` contains one element.

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
> quoted  = <quoted scalar>
> ```
>
> Quoted key segments use the same syntax and escape sequences as quoted scalars
> (see r[scalar.quoted.escapes]).

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
> Trailing commas are allowed: `{ a 1, b 2, }` is valid.

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
>
> Whitespace around `=` is not allowed. `key = value` is invalid; use `key=value`.

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
>   { a 1, b 2 }
> )
> ```
>
> Or with attribute objects nested as values:
>
> ```styx
> (
>   { config a=1 b=2 }
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

---

# Part 2: Deserializer

The deserializer converts document trees into typed application values. It interprets
scalars based on target types and validates structural constraints like enum representations.

## Scalar interpretation

The deserializer interprets opaque scalar values based on the target type. A scalar
like `42` becomes an integer when the target is `u32`, but remains a string when
the target is `String`.

A conforming deserializer SHOULD recognize the following standard scalar forms:

> r[scalar.interp.integer]
> Scalars matching this grammar are eligible for integer interpretation:
>
> ```
> integer = ["-" | "+"] digit+
> digit   = "0"..."9"
> ```
>
> Examples: `0`, `42`, `-10`, `+5`

> r[scalar.interp.float]
> Scalars matching this grammar are eligible for float interpretation:
>
> ```
> float    = integer "." digit+ [exponent] | integer exponent
> exponent = ("e" | "E") ["-" | "+"] digit+
> ```
>
> Examples: `3.14`, `-0.5`, `1e10`, `2.5e-3`

> r[scalar.interp.boolean]
> `true` and `false` are eligible for boolean interpretation.

> r[scalar.interp.null]
> `null` is eligible for null/none interpretation.

> r[scalar.interp.duration]
> Scalars matching this grammar are eligible for duration interpretation:
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
> Scalars matching RFC 3339 are eligible for timestamp interpretation:
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
> Scalars matching this grammar are eligible for regular expression interpretation:
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
> Scalars matching this grammar are eligible for byte sequence interpretation:
>
> ```
> hex_bytes = "0x" hex_digit+
> hex_digit = "0"..."9" | "a"..."f" | "A"..."F"
> ```
>
> Examples: `0xdeadbeef`, `0x00FF`

> r[scalar.interp.bytes.base64]
> Scalars matching this grammar are eligible for byte sequence interpretation:
>
> ```
> base64_bytes = "b64" '"' base64_char* '"'
> ```
>
> Examples: `b64"SGVsbG8="`, `b64""`

Implementations commonly support additional forms like paths, URLs, IPs, and semver.

## Enums

Enums are represented as objects with exactly one key (the variant tag) whose value
is the variant payload.

> r[enum.representation]
> When deserializing into an enum type, the value MUST be an object with exactly one key.
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

The parser expands dotted paths into nested objects, which provides a convenient
syntax for enums:

```styx
status.ok
```

```styx
status.err { message "nope" }
```

These expand to `status { ok {} }` and `status { err { message "nope" } }` respectively.

> r[enum.singleton]
> The deserializer MUST reject enum values that are not single-key objects.
> The parser cannot enforce this — it produces the same object structure regardless
> of target type.

A variant payload may be omitted (unit variant), or may be a scalar, object, or sequence:

```styx
result.ok
```

```styx
result.err message="timeout" retry_in=5s
```

The deserializer validates:
- The value is a single-key object
- The key matches a valid variant name
- The payload matches the expected variant shape

---

# Appendix

## Usage patterns (non-normative)

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

## Design invariants (non-normative)

STYX enforces the following invariants:

- **No implicit merges**: Objects are never merged. Each key appears exactly once.
- **No reopening**: Once an object is closed, it cannot be extended with additional keys.
- **No indentation-based structure**: All structure is explicit via `{}` and `()`.
- **No semantic interpretation during parsing**: The parser produces opaque scalars; meaning is assigned during deserialization.
- **All structure is explicit**: Braces and parentheses define nesting, not whitespace or conventions.
- **Commas are separators only**: Commas have no semantic meaning; they are interchangeable with newlines for readability.
