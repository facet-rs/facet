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
> A bare scalar is terminated by whitespace or any of: `{`, `}`, `(`, `)`, `=`, `,`, `//`.
>
> ```styx
> items(a b c)     // "items" is key, (a b c) is value
> foo{bar baz}     // "foo" is key, {bar baz} is value
> x=1              // in attribute context: "x" is key, "1" is value
> foo// comment    // "foo" is the scalar, comment follows
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
> | `\@` | Literal `@` (see r[schema.type-ref.escape]) |
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
> r##"contains "# in the middle"##
> r###"contains "## in the middle"###
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
> Elements MUST be separated by whitespace. Commas are NOT allowed in sequences.
>
> ```styx
> (a b c)
> (
>   a
>   b
>   c
> )
> (a, b, c)    // ERROR: commas not allowed in sequences
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

> r[object.order]
> Objects preserve insertion order. Parsers MUST yield entries in the order
> they appear in the source. This guarantee enables configuration files where
> order matters (e.g., processing pipelines, rule precedence).

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

> r[object.key.reserved]
> Keys starting with `@` are reserved for directives (e.g., `@schema`).
> Reserved keys do not follow the standard key grammar — they are recognized
> as special tokens by the parser at specific positions (e.g., document root).
> To use a literal key starting with `@` in a document, quote it: `"\@foo"`.

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

> r[object.entry.implicit-unit]
> If a key is not followed by a value, the value is implicitly `()` (empty sequence).
>
> ```styx
> enabled           // equivalent to: enabled ()
> status.ok         // equivalent to: status { ok () }
> server {
>   debug           // equivalent to: debug ()
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

> r[object.block.no-equals]
> In block objects, entries use `key value` syntax, not `key=value`.
> Attribute objects may appear as *values* within block objects.
>
> ```styx
> { a=1 b=2 }              // ERROR: entries cannot use =
> { a 1, b 2 }             // OK: block form entries
> { labels app=web }       // OK: "labels" is key, "app=web..." is attribute value
> { config { a=1 } }       // ERROR: nested block cannot use =
> ```

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
> Comments are treated as whitespace and do not affect termination.

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
> {"ok": []}
> /// styx
> { ok () }
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

The first expands to `status { ok () }` (using implicit unit, see r[object.entry.implicit-unit]).
The second expands to `status { err { message "nope" } }`.

> r[enum.singleton]
> The deserializer MUST reject enum values that are not single-key objects.
> The parser cannot enforce this — it produces the same object structure regardless
> of target type.

> r[enum.unit]
> For unit variants, the deserializer MUST accept both `()` and `{}` as valid payloads.
> Both represent "no payload."
>
> ```styx
> // All equivalent for unit variants:
> status.ok        // implicit ()
> status.ok ()     // explicit ()
> status.ok {}     // explicit {}
> ```

A variant payload may be a unit (omitted, `()`, or `{}`), a scalar, an object, or a sequence:

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

# Part 3: Schemas

Schemas define the expected structure of STYX documents. They specify what keys exist,
what types values must have, and whether fields are required or optional.

STYX schemas are themselves STYX documents. They can be inline (embedded in a document)
or external (separate files).

## Type references

Type references use the `@` prefix to distinguish types from literal values:

```styx
/// A server configuration schema
server {
  host @string
  port @integer
  timeout @duration?
}
```

> r[schema.type-ref]
> A type reference is a scalar starting with `@`. The remainder names a type
> from the standard type vocabulary or a user-defined type.
>
> Type names MUST match the grammar:
> ```
> type-ref  = "@" type-name ["?"]
> type-name = [A-Za-z_][A-Za-z0-9_-]*
> ```
>
> Examples: `@string`, `@TlsConfig`, `@my-type`, `@my_type`

> r[schema.type-ref.literal]
> A scalar without `@` is a literal value constraint. The document value must
> be exactly that scalar.
>
> ```styx
> version 1          // must be exactly the scalar "1"
> version @integer   // must be an integer (1, 2, 42, etc.)
> ```

> r[schema.type-ref.escape]
> To represent a literal value starting with `@`, use the `\@` escape in a quoted scalar.
> The parser produces the scalar text; the schema layer interprets `@` prefixes.
>
> ```styx
> // In a schema:
> tag @string        // type reference: any string
> tag "\@mention"    // literal: must be exactly "@mention"
> ```
>
> Raw scalars and heredocs are always literal values — they never produce type references:
>
> ```styx
> pattern r#"@user"#   // literal: the string "@user"
> ```

## Standard types

The schema type vocabulary matches the deserializer's scalar interpretation rules:

> r[schema.type.string]
> `@string` — any scalar value.

> r[schema.type.integer]
> `@integer` — a scalar matching the integer grammar.

> r[schema.type.float]
> `@float` — a scalar matching the float grammar.

> r[schema.type.boolean]
> `@boolean` — `true` or `false`.

> r[schema.type.null]
> `@null` — the scalar `null`.

> r[schema.type.duration]
> `@duration` — a scalar matching the duration grammar (`30s`, `10ms`, etc.).

> r[schema.type.timestamp]
> `@timestamp` — a scalar matching RFC 3339.

> r[schema.type.regex]
> `@regex` — a scalar matching the regex grammar (`/pattern/flags`).

> r[schema.type.bytes]
> `@bytes` — a scalar matching hex (`0x...`) or base64 (`b64"..."`) grammar.

> r[schema.type.any]
> `@any` — any value (scalar, object, or sequence). Useful for arbitrary metadata:
>
> ```styx
> metadata @map(@any)   // arbitrary key-value pairs
> extensions @any       // any structure
> ```

## Optional types

A `?` suffix marks a type as optional:

```styx
server {
  host @string       // required
  port @integer      // required  
  timeout @duration? // optional
}
```

> r[schema.optional]
> A type reference followed by `?` indicates the field may be omitted.
> If present, the value must match the type.
>
> The `?` MUST immediately follow the type name with no whitespace:
>
> ```styx
> timeout @duration?     // OK: optional duration
> timeout @duration ?    // ERROR: space before ?
> ```

## Sequences

Sequences use `()` containing a type reference:

```styx
/// List of hostnames
hosts (@string)

/// List of server configurations
servers ({
  host @string
  port @integer
})
```

> r[schema.sequence]
> A sequence schema `(@type)` matches a sequence where every element matches `@type`.

## Maps

Maps are objects with arbitrary string keys and uniform value types:

```styx
/// Environment variables (string to string)
env @map(@string)

/// Port mappings (string to integer)
ports @map(@integer)
```

> r[schema.map]
> `@map(@type)` matches an object where all values match `@type`.
> Keys are always strings.

## Nested objects

Object schemas can be nested inline:

```styx
server {
  host @string
  port @integer
  tls {
    cert @string
    key @string
    enabled @boolean?
  }
}
```

Or reference named types:

```styx
server {
  host @string
  port @integer
  tls @TlsConfig
}

TlsConfig {
  cert @string
  key @string
  enabled @boolean?
}
```

> r[schema.object.inline]
> An inline object schema `{ ... }` defines the expected structure directly.

> r[schema.object.ref]
> A type reference like `@TlsConfig` refers to a named schema defined elsewhere
> in the schema document.

> r[schema.type.definition]
> Named types are defined at the schema root as a key (the type name) with an
> object value (the type's structure). Type definitions do NOT use the `@` prefix;
> `@` is only used when *referencing* a type.
>
> ```styx
> // Type definition (no @):
> TlsConfig {
>   cert @string
>   key @string
> }
>
> // Type reference (with @):
> server {
>   tls @TlsConfig
> }
> ```

## Enums

Enum schemas list the valid variants:

```styx
status @enum {
  ok
  pending
  err {
    message @string
    code @integer?
  }
}
```

> r[schema.enum]
> An enum schema `@enum { ... }` defines valid variant names and their payloads.
> Unit variants use implicit `()` (see r[object.entry.implicit-unit]).
> Variants with payloads specify their schema as the value.

## Limitations (non-normative)

This schema language intentionally omits some features common in other systems:

**Union types**: There is no syntax for "string or integer" or "duration or null".
Use `@any` for polymorphic fields, or model variants explicitly with `@enum`.

**Nullable vs optional**: `@type?` means the field may be omitted. To express
"required but may be null", use `@null` as a valid type or model with `@enum`:

```styx
// Option 1: accept null explicitly
value @any  // allows null among other values

// Option 2: model as enum
value @enum {
  some { inner @string }
  none
}
```

**Recursive types**: Self-referential types are supported:

```styx
TreeNode {
  value @any
  children (@TreeNode)
}
```

## Doc comments

Doc comments use `///` and attach to the following definition:

```styx
/// Server configuration for the web tier
server {
  /// Hostname or IP address to bind to
  host @string
  
  /// Port number (1-65535)
  port @integer
  
  /// Request timeout; defaults to 30s if not specified
  timeout @duration?
}
```

> r[schema.doc]
> A comment starting with `///` is a doc comment. It attaches to the immediately
> following key or type definition.
>
> Doc comments take precedence: `////` is a doc comment with content `/ ...`,
> not a regular comment. Multiple consecutive doc comments are concatenated.

## Schema location

Schemas can be:

1. **External**: A separate `.styx` file referenced by the document
2. **Inline**: Embedded in the document itself

> r[schema.inline]
> An inline schema uses the reserved key `@schema` at the document root:
>
> ```styx
> @schema {
>   server {
>     host @string
>     port @integer
>   }
> }
> 
> server {
>   host localhost
>   port 8080
> }
> ```

> r[schema.external]
> External schema resolution is implementation-defined. Common patterns include
> file extensions (`.schema.styx`), sidecar files, or registry lookups.

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
- **Commas in objects only**: Commas are optional separators in objects (interchangeable with newlines). Sequences use whitespace only.
- **Implicit unit values**: Keys without values produce `()` (empty sequence). This enables concise unit variants (`status.ok`) and flag-like entries (`enabled`).
