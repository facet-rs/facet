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
> If the document starts with `{`, it MUST be parsed as a single block object.
> The closing `}` MUST be the final token; content after it is an error.
>
> ```styx
> {
>   key value
> }
> extra   // ERROR: unexpected token after root object
> ```


## Comments

Line comments start with `//` and extend to the end of the line.

> r[comment.line]
> Line comments start with `//` and extend to the end of the line.
> Comments MUST be preceded by whitespace (space or newline). The sequence `//`
> without preceding whitespace is not recognized as a comment start.
>
> ```styx
> server {
>   host localhost  // OK: space before //
>   port 8080       // OK: space before //
> }
> url https://example.com  // OK: space before //
> foo bar// comment        // ERROR: "bar//" is part of the scalar
> ```

> r[comment.placement]
> Comments are allowed anywhere whitespace is allowed.

## Value types

The parser produces six types of values:

  * **Scalar** — an opaque text atom
  * **Object** — an ordered map of keys to values
  * **Tagged object** — an object with an associated scalar tag
  * **Sequence** — an ordered list of values
  * **Tagged sequence** — a sequence with an associated scalar tag
  * **Unit** — the absence of a meaningful value (`@`)

## Unit

The unit value represents the absence of a meaningful value, analogous to `()` in Rust
or `None` in Python. JSON has no direct equivalent; `null` is the closest approximation.

```compare
/// json
{"enabled": null}
/// styx
enabled @
```

> r[value.unit]
> The token `@` not immediately followed by an identifier character is the **unit value**.
> Identifier characters are `[A-Za-z_]` for the first character, `[A-Za-z0-9_-]` thereafter
> (see `r[object.key.syntax]`).
>
> ```styx
> field @              // unit value (@ followed by whitespace)
> field @              // unit value (@ at end of line)
> field @string        // type reference (@ followed by identifier)
> field @123           // unit value followed by scalar "123" — ERROR: unexpected token
> ```
>
> The parser resolves `@` vs `@identifier` by checking the immediately following character.
> If no identifier character follows, the `@` is the unit value.

> r[value.unit.sequence]
> The unit value is valid as a sequence element.
>
> ```styx
> (a @ c)              // 3-element sequence: "a", unit, "c"
> (@)                  // 1-element sequence containing unit
> ()                   // 0-element sequence (empty, distinct from unit)
> ```

## Scalars

Scalars are opaque atoms. The parser assigns no meaning to them; interpretation
is deferred until deserialization.

> r[scalar.form]
> The parser MUST record the lexical form of each scalar: **bare**, **quoted**,
> **raw**, or **heredoc**. All forms produce identical text content, but the
> form is preserved for use by the schema layer.
>
> ```styx
> foo          // bare scalar
> "foo"        // quoted scalar
> r#"foo"#     // raw scalar
> <<EOF        // heredoc scalar
> foo
> EOF
> ```
>
> The schema layer uses this distinction: only bare scalars starting with `@`
> are type references. Quoted, raw, and heredoc forms are always literal values
> (see `r[schema.type-ref]`).

### Bare scalars

Bare scalars are delimited by whitespace and structural characters.

> r[scalar.bare.termination]
> A bare scalar is terminated by whitespace or any of: `{`, `}`, `(`, `)`, `,`.
>
> When `(` or `{` terminates a bare scalar, the preceding characters form a tag
> and the result is a tagged sequence or tagged object (see `r[sequence.tagged]`
> and `r[object.tagged]`).
>
> ```styx
> url https://example.com/path?query=1   // bare scalar includes = and /
> items (a b c)                           // whitespace before ( — two tokens
> rgb(255 0 0)                            // no whitespace — tagged sequence
> config { host localhost }               // whitespace before { — two tokens
> point{ x 1, y 2 }                       // no whitespace — tagged object
> ```
>
> ```styx
> items tag(a b c)   // "items" is key, tag(a b c) is a tagged sequence
> foo data{bar baz}  // "foo" is key, data{bar baz} is a tagged object
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
> The delimiter MUST match the pattern `[A-Z][A-Z0-9_]*` (uppercase letters,
> digits, and underscores only; must start with an uppercase letter).
> Single-character delimiters are valid.
>
> Examples: `E`, `EOF`, `SQL`, `EOF2`, `BASE64_DATA`

> r[scalar.heredoc.delimiter.length]
> The delimiter MUST NOT exceed 16 characters.

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
> The resulting value is:
> ```
> #!/bin/bash
> echo "hello"
> ```

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

> r[scalar.heredoc.literal]
> Heredoc content is literal. Comments (`//`) and escape sequences (`\n`) are
> not processed within heredoc content.
>
> ```styx
> script <<BASH
>   echo "hello"  // this is not a comment
>   echo "line\nbreak"  // \n is literal, not a newline
>   BASH
> ```
>
> The value includes the literal text `// this is not a comment` and `\n`.

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
> An empty sequence `()` is valid and distinct from unit (`@`).

> r[sequence.no-commas]
> Commas are NOT allowed in sequences.
>
> ```styx
> (a, b, c)    // ERROR: commas not allowed in sequences
> ```
>
> **Rationale**: Sequences are visually clean with whitespace alone:
>
> ```styx
> (a b c)
> ```
>
> Objects need commas because `key value` pairs are harder to group visually:
>
> ```styx
> { key value, key value }   // commas help group key-value pairs
> key=value key=value        // or use attribute form
> ```

> r[sequence.elements]
> Sequence elements MAY be scalars, block objects, nested sequences, or unit.
> Attribute objects are NOT allowed as direct sequence elements.
>
> ```styx
> ((1 2) (3 4))           // nested sequences
> ({ name alice } { name bob })  // block objects
> (a @ c)                 // unit as element
> (a=1 b=2)               // ERROR: attribute object ambiguous
> ```
>
> **Rationale**: Given `(a=1 b=2)`, it is unclear whether this is one object
> `{a: 1, b: 2}` or two objects `{a: 1}` and `{b: 2}`. Block objects make
> structure explicit.

### Tagged sequences

A tagged sequence is a sequence with an associated tag. The tag is a scalar that
immediately precedes the opening `(` with no whitespace. JSON has no equivalent;
tagged values are a STYX extension.

```compare
/// json
{"colors": {"$tag": "rgb", "$values": [255, 128, 0]}}
/// styx
colors rgb(255 128 0)
```

> r[sequence.tagged]
> When a bare scalar is immediately followed by `(` (no intervening whitespace),
> the parser MUST produce a **tagged sequence** value. The scalar becomes the tag.
>
> ```styx
> colors rgb(255 128 0)
> point vec3(1.0 2.0 3.0)
> ```
>
> The value of `colors` is a tagged sequence with tag `rgb` and elements `(255 128 0)`.

> r[sequence.tagged.nested]
> Tagged sequences may be nested.
>
> ```styx
> transform scale(translate(10 20) rotate(45))
> ```
>
> This is a tagged sequence `scale(...)` containing two tagged sequences
> `translate(...)` and `rotate(...)`.

> r[sequence.tagged.quoted]
> The tag may be a quoted scalar.
>
> ```styx
> data "my-tag"(a b c)
> ```

> r[sequence.tagged.empty]
> A tagged empty sequence is valid.
>
> ```styx
> empty tag()
> ```

### Tagged objects

A tagged object is an object with an associated tag. The tag is a scalar that
immediately precedes the opening `{` with no whitespace.

> r[object.tagged]
> When a bare scalar is immediately followed by `{` (no intervening whitespace),
> the parser MUST produce a **tagged object** value. The scalar becomes the tag.
>
> ```styx
> status @enum{
>   ok
>   pending
>   err { message @string }
> }
> ```
>
> The value of `status` is a tagged object with tag `@enum` and the object contents.

> r[object.tagged.quoted]
> The tag may be a quoted scalar.
>
> ```styx
> data "my-tag"{ key value }
> ```

> r[object.tagged.empty]
> A tagged empty object is valid.
>
> ```styx
> empty tag{}
> ```

## Objects

Objects are key-value maps.

> r[object.order]
> Parsers MUST yield object entries in the order they appear in the source.
> This enables stable round-tripping and predictable diffs.

### Keys

Keys are dotted paths composed of one or more segments.

> r[object.key.syntax]
> A key MUST match the following grammar:
>
> ```
> key     = segment ("." segment)* "?"?
> segment = bare | quoted
> bare    = [A-Za-z_][A-Za-z0-9_-]*
> quoted  = <quoted scalar>
> ```
>
> A trailing `?` marks the key as optional (see `r[schema.optional]`).
>
> Quoted key segments use the same syntax and escape sequences as quoted scalars
> (see `r[scalar.quoted.escapes]`).

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
> If a key is not followed by a value, the value is implicitly `@` (unit).
>
> ```styx
> enabled           // equivalent to: enabled @
> status.ok         // equivalent to: status { ok @ }
> server {
>   debug           // equivalent to: debug @
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
    url postgres://localhost/mydb
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

> r[object.block.separators.no-mixing]
> An object MUST use either commas or newlines as separators, never both.
> An object uses comma separation if any comma appears between entries.
> An object uses newline separation if entries are separated only by newlines.
>
> ```styx
> { a 1, b 2, c 3 }      // OK: comma-separated
> {
>   a 1
>   b 2
>   c 3
> }                       // OK: newline-separated
> {
>   a 1,
>   b 2
> }                       // ERROR: comma on line 1, newline separates from line 2
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
> When the parser expects a value and encounters a token matching `key=value`,
> it MUST parse an attribute object. The `=` is recognized within the token
> because `=` does not terminate bare scalars (see `r[scalar.bare.termination]`).
>
> The parser scans for `=` within the token: characters before `=` form the key,
> characters after form the value (which may itself be a scalar, sequence, or object).
>
> Whitespace around `=` is not allowed. `key = value` is three tokens; use `key=value`.

> r[object.attr.value]
> The value after `=` MUST be exactly one value: a scalar, sequence, or block object.
> Block objects may span multiple lines; the attribute object continues after the
> closing `}`.
>
> ```styx
> config foo={
>   a long
>   object block
> } bar=123 baz=hey
> ```
>
> This is equivalent to:
>
> ```styx
> config {
>   foo { a long, object block }
>   bar 123
>   baz hey
> }
> ```

> r[object.attr.termination]
> An attribute object ends when the next token is not `key=...`.
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
> // both attributes belong to server
> server host=localhost port=8080
> ```
>
> ```compare
> /// json
> {
>   "server": {
>     "host": "localhost"
>   },
>   "port": 8080
> }
> /// styx
> // newline ends the attribute object — port is a separate root entry
> server host=localhost
> port 8080
> ```
>
> ```styx
> // ERROR: cannot follow attribute object with block object
> server host=localhost { port 8080 }
> ```

Attribute objects work well for inline key-value patterns like labels,
environment variables, and options. For complex or nested structures, use block form.

> r[object.block.no-equals]
> Block objects use `key value` syntax. Attribute `key=value` syntax is only
> valid as a *value* within a block object, not as an entry.
>
> ```styx
> { a=1 b=2 }              // ERROR: block entries cannot use =
> { a 1, b 2 }             // OK: block form entries
> { labels app=web }       // OK: "labels" is key, attribute object is value
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

# Part 2: Schemas

Schemas define the expected structure of STYX documents. They specify what keys exist,
what types values must have, and whether fields are required or optional.

STYX schemas are themselves STYX documents. They can be inline (embedded in a document)
or external (separate files). Schema constructs use tagged sequences and tagged objects
(see `r[sequence.tagged]` and `r[object.tagged]`).

## Type references

Type references use the `@` prefix to distinguish types from literal values:

```styx
/// A server configuration schema
server {
  host @string
  port @integer
  timeout? @duration
}
```

> r[schema.type-ref]
> A type reference is a scalar starting with `@`. The remainder names a type
> from the standard type vocabulary or a user-defined type.
>
> Type names MUST match the grammar:
> ```
> type-ref  = "@" type-name
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
> To represent a literal value starting with `@`, use any non-bare scalar form.
> Only bare scalars are interpreted as type references (see `r[scalar.form]`).
>
> ```styx
> // In a schema:
> tag @string        // type reference: any string
> tag "@mention"     // literal: must be exactly "@mention"
> tag r#"@user"#     // literal: the string "@user"
> ```

## Standard types

The schema type vocabulary matches the deserializer's scalar interpretation rules:

> r[schema.type.string]
> `@string` — any scalar value.

> r[schema.type.integer]
> `@integer` — a scalar matching:
>
> ```
> integer = ["-" | "+"] digit+
> digit   = "0"..."9"
> ```
>
> Examples: `0`, `42`, `-10`, `+5`

> r[schema.type.float]
> `@float` — a scalar matching:
>
> ```
> float    = integer "." digit+ [exponent] | integer exponent
> exponent = ("e" | "E") ["-" | "+"] digit+
> ```
>
> Examples: `3.14`, `-0.5`, `1e10`, `2.5e-3`

> r[schema.type.boolean]
> `@boolean` — `true` or `false`.

> r[schema.type.duration]
> `@duration` — a scalar matching:
>
> ```
> duration = integer unit
> unit     = "ns" | "us" | "µs" | "ms" | "s" | "m" | "h" | "d"
> ```
>
> Both `us` and `µs` are accepted for microseconds, for ASCII compatibility.
> Units are case-sensitive; `30S` is not a valid duration.
>
> Examples: `30s`, `10ms`, `2h`, `500µs`, `500us`

> r[schema.type.timestamp]
> `@timestamp` — a scalar matching RFC 3339:
>
> ```
> timestamp = date "T" time timezone
> date      = year "-" month "-" day
> time      = hour ":" minute ":" second ["." fraction]
> timezone  = "Z" | ("+" | "-") hour ":" minute
> ```
>
> Examples: `2026-01-10T18:43:00Z`, `2026-01-10T12:00:00-05:00`

> r[schema.type.regex]
> `@regex` — a scalar matching:
>
> ```
> regex = "/" pattern "/" flags
> flags = [a-zA-Z]*
> ```
>
> The set of valid flags is implementation-defined. Common flags include `i` (case-insensitive),
> `m` (multiline), `s` (dotall), and `x` (extended).
>
> Examples: `/foo/`, `/^hello$/i`, `/\d+/`

> r[schema.type.bytes]
> `@bytes` — a scalar matching hex or base64:
>
> ```
> hex_bytes    = "0x" hex_digit+
> base64_bytes = "b64" '"' base64_char* '"'
> ```
>
> Examples: `0xdeadbeef`, `0x00FF`, `b64"SGVsbG8="`, `b64""`

> r[schema.type.any]
> `@any` — any value (scalar, object, sequence, or unit). Useful for arbitrary metadata:
>
> ```styx
> metadata @map(@any)   // arbitrary key-value pairs
> extensions @any       // any structure
> ```

> r[schema.type.unit]
> `@unit` — the unit value `@`. Useful for sentinel fields or nullable types:
>
> ```styx
> // Field that must be unit (sentinel/marker)
> enabled @unit
>
> // Nullable field using union
> value @union(@string @unit)
> ```
>
> Use `@unit` for nullable fields. The unit value `@` represents structural absence,
> distinct from any scalar value.

## Optional types

A trailing `?` on a key marks the field as optional:

```styx
server {
  host @string      // required
  port @integer     // required  
  timeout? @duration // optional
}
```

> r[schema.optional]
> A key ending with `?` indicates the field may be omitted from the document.
> If present, the value must match the type.
>
> ```styx
> timeout? @duration   // may be absent; if present, must be duration
> ```

## Union types

Union types allow a value to match any of several types using a tagged sequence:

```styx
// String or unit (nullable string)
name @union(@string @unit)

// Integer or string
id @union(@integer @string)

// Duration, integer, or unit
timeout @union(@duration @integer @unit)
```

> r[schema.union.syntax]
> A union type uses the `@union` tagged sequence containing type references:
>
> ```
> union = "@union" "(" type-ref+ ")"
> ```
>
> The union must contain at least one type reference.

> r[schema.union]
> `@union(@type1 @type2 ...)` matches a value if it matches any of the
> listed types.
>
> ```styx
> // Nullable string: required, but may be unit
> name @union(@string @unit)
>
> // Optional nullable: may be absent, or string, or unit
> name? @union(@string @unit)
> ```

> r[schema.union.disambiguation]
> When validating a value against a union, types are checked in order.
> The first matching type determines the interpretation.
>
> For overlapping types (e.g., `@union(@integer @string)`), more specific types
> should appear first to ensure correct matching.

**Common patterns:**

```styx
// Nullable field (required but may be unit)
value @union(@string @unit)

// Optional field (may be absent)
value? @string

// Optional nullable field (may be absent, or present as string or unit)
value? @union(@string @unit)
```

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

Maps are objects with arbitrary string keys and uniform value types. They use the
`@map` tagged sequence:

```styx
/// Environment variables (string to string)
env @map(@string)

/// Port mappings (string to integer)
ports @map(@integer)
```

Example document matching `env @map(@string)`:

```compare
/// json
{"env": {"HOME": "/home/user", "PATH": "/usr/bin"}}
/// styx
env {
  HOME "/home/user"
  PATH "/usr/bin"
}
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
    enabled? @boolean
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
  enabled? @boolean
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

> r[schema.type.unknown]
> A type reference to an undefined type degenerates to `@any`. This allows schemas
> to reference types from external sources (imports, registries) that the validator
> may not have access to. Implementations MAY issue a warning for unknown types.
>
> ```styx
> // If ExternalConfig is not defined in this schema:
> config @ExternalConfig   // treated as @any during validation
> ```

## Flatten

The `@flatten` modifier inlines fields from another type into the current object:

```styx
User {
  name @string
  email @string
}

Admin {
  user @flatten(@User)
  permissions (@string)
}
```

> r[schema.flatten]
> `@flatten(@Type)` inlines all fields from the referenced type into the current
> object. The field name (`user` in the example) is used for deserialization into
> nested structures, but the data is flat.
>
> Given the schema above, this data:
>
> ```styx
> name Alice
> email alice@example.com
> permissions (read write admin)
> ```
>
> deserializes into an `Admin` with `name` and `email` routed to the nested `User`.

> r[schema.flatten.collision]
> Key collisions between flattened fields and the containing object's own fields
> are forbidden. The schema validator MUST detect collisions statically at schema
> validation time, before any documents are validated. This requires resolving all
> type references in the flattened type.
>
> ```styx
> Base { name @string }
> 
> Derived {
>   base @flatten(@Base)
>   name @string            // ERROR: "name" collides with Base.name
> }
> ```
>
> For recursive types, the validator MUST detect cycles and report an error rather
> than entering infinite recursion.

## Enums

Enum schemas list the valid variants using a tagged object:

```styx
status @enum{
  ok
  pending
  err {
    message @string
    code? @integer
  }
}
```

> r[schema.enum]
> `@enum{ ... }` defines valid variant names and their payloads.
> Unit variants use implicit `@` (see `r[object.entry.implicit-unit]`).
> Variants with payloads specify their schema as the value.

## Notes (non-normative)

**Nullable vs optional**: These are distinct concepts:

- `key? @type` — *optional*: field may be absent from the document
- `key @union(@type @unit)` — *nullable*: field must be present but may be unit (`@`)
- `key? @union(@type @unit)` — *optional nullable*: may be absent, or present as value or unit

```styx
// Required string
name @string

// Optional string (may be absent)
name? @string

// Nullable string (present, but may be @)
name @union(@string @unit)

// Optional nullable string
name? @union(@string @unit)
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
  timeout? @duration
}
```

> r[schema.doc]
> A comment starting with `///` is a doc comment. It attaches to the immediately
> following key or type definition.
>
> Doc comments take precedence: `////` is a doc comment with content `/ ...`,
> not a regular comment. Multiple consecutive doc comments are concatenated.

> r[schema.doc.unattached]
> A doc comment not followed by a key or type definition is a syntax error.
> Blank lines between doc comments break the sequence.
>
> ```styx
> /// This comment
> /// attaches to foo
> foo @string
>
> /// This comment
>
> /// ERROR: previous doc comment has no attachment (blank line broke sequence)
> bar @string
> ```

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

# Part 3: Deserializer

The deserializer converts document trees into typed application values. It interprets
scalars based on target types and validates structural constraints like enum representations.

For performance, implementations may deserialize directly from source text without
materializing an intermediate document tree. The behavior must be indistinguishable
from first parsing into a tree, then deserializing from that tree.

## Scalars are opaque

The parser treats all scalars as opaque text. The deserializer assigns meaning
based on the target type.

> r[deser.scalar.opaque]
> A scalar has no inherent type. `42` is not "an integer" — it is text that
> *can be interpreted as* an integer when the target type requires one.
>
> ```styx
> port 42        // if target is u16: integer 42
>                // if target is String: string "42"
> ```

> r[deser.scalar.no-coercion]
> There is no implicit coercion between scalar forms. A quoted scalar `"42"`
> and a bare scalar `42` both contain the text `42`, but neither is "more numeric"
> than the other. The target type determines interpretation, not the lexical form.

See Part 2 for the grammars of `@integer`, `@float`, `@duration`, etc.

## Object deserialization

Objects in the document are validated against object schemas.

> r[deser.object.fields]
> Each key in the document must match a field defined in the schema. Required
> fields (no `?` suffix) MUST be present; optional fields MAY be absent.

> r[deser.object.unknown]
> Keys not defined in the schema are errors by default. Implementations MAY
> provide a lenient mode that ignores unknown keys.

## Optional fields

Optional fields interact with absence and unit.

> r[deser.optional.absent]
> An optional field (`key? @type`) that is absent from the document is valid.
> The application receives no value for that field.

> r[deser.optional.unit]
> An optional field explicitly set to unit (`key @`) is distinct from absence.
> Both are valid for optional fields, but applications may distinguish them.
>
> ```styx
> // Schema: timeout? @duration
> { }                    // absent — no timeout specified
> { timeout @ }          // present but explicitly empty
> { timeout 30s }        // present with value
> ```

## Sequence deserialization

Sequences are validated element-by-element.

> r[deser.sequence]
> A sequence schema `(@type)` validates that every element matches `@type`.
> Empty sequences are valid.

## Map deserialization

Maps are objects with uniform value types.

> r[deser.map]
> A map schema `@map(@type)` validates that all values match `@type`.
> Keys are always strings. Empty maps are valid.

## Flatten

Flattening merges fields from a referenced type.

> r[deser.flatten]
> A flattened field `key @flatten(@Type)` expects the referenced type's fields
> at the same level, not nested under `key`.
>
> ```styx
> // Schema:
> // User { name @string, email @string }
> // Admin { user @flatten(@User), role @string }
>
> // Document (fields are flat):
> name Alice
> email alice@example.com
> role superuser
> ```

> r[deser.flatten.routing]
> The deserializer routes flattened keys to the appropriate nested structure
> based on the schema. Keys are matched in declaration order when multiple
> flattened types could apply.

## Enum deserialization

Enums are represented as objects with exactly one key (the variant tag) whose value
is the variant payload.

> r[enum.representation]
> When deserializing into an enum type, the value MUST be an object with exactly one key.
>
> ```compare
> /// json
> {"ok": null}
> /// styx
> { ok @ }
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

```compare
/// styx
status.ok
/// styx
status { ok @ }
```

```compare
/// styx
status.err { message "nope" }
/// styx
status { err { message "nope" } }
```

Dotted paths expand to nested objects (see `r[object.key.dotted.expansion]`).
Keys without values get implicit unit (see `r[object.entry.implicit-unit]`).

> r[enum.singleton]
> The deserializer MUST reject enum values that are not single-key objects.
> The parser cannot enforce this — it produces the same object structure regardless
> of target type.

> r[enum.unit]
> For unit variants, the payload is the unit value `@`.
>
> ```styx
> // All equivalent for unit variants:
> status.ok        // implicit @
> status.ok @      // explicit @
> ```

A variant payload may be unit (`@`), a scalar, an object, or a sequence:

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

# Part 4: Diagnostics

This section specifies the format and content of error messages. Clear, actionable
diagnostics are essential for a human-authored format.

## Diagnostic format

STYX implementations SHOULD emit diagnostics in the following format:

```
level: message
  --> file:line:column
   |
NN | source line
   | ^^^ annotation
   |
   = note: additional context
   = help: suggested fix
```

> r[diagnostic.format]
> A diagnostic SHOULD include:
>
> - **Level**: `error`, `warning`, or `note`
> - **Message**: A concise description of the problem
> - **Location**: File path, line number, and column
> - **Source context**: The relevant source line(s) with underline annotations
> - **Help**: When applicable, a concrete suggestion for fixing the problem
>
> Secondary locations (e.g., "first defined here") use `------` underlines.
> Primary locations (the actual error site) use `^^^^^` underlines.

> r[diagnostic.actionable]
> Error messages SHOULD be actionable. When a fix is known, the diagnostic
> SHOULD show the corrected code, not just describe the problem.

## Parser errors

### Unexpected token

> r[diagnostic.parser.unexpected]
> When the parser encounters an unexpected token, the message SHOULD identify
> what was found and what was expected.
>
> ```
> error: unexpected token
>   --> config.styx:3:5
>   |
> 3 |     = value
>   |     ^ expected key or '}'
> ```

### Unclosed delimiter

> r[diagnostic.parser.unclosed]
> When a delimiter is not closed, the message SHOULD show where the opening
> delimiter was and where the parser expected the closing delimiter.
>
> ```
> error: unclosed '{'
>   --> config.styx:1:8
>   |
> 1 | server {
>   |        ^ unclosed delimiter
>   |
> ...
>   |
> 5 | database {
>   | -------- this '{' might be the problem (missing '}' before it?)
> ```

### Invalid escape sequence

> r[diagnostic.parser.escape]
> When a quoted scalar contains an invalid escape sequence, the message SHOULD
> identify the specific invalid escape.
>
> ```
> error: invalid escape sequence '\q'
>   --> config.styx:2:12
>   |
> 2 |   name "foo\qbar"
>   |            ^^ invalid escape
>   |
>   = help: valid escapes are: \\, \", \n, \r, \t, \0, \uXXXX, \u{X...}
> ```

### Unterminated string

> r[diagnostic.parser.unterminated-string]
> When a quoted scalar is not terminated, the message SHOULD show where the
> string started.
>
> ```
> error: unterminated string
>   --> config.styx:2:8
>   |
> 2 |   name "hello
>   |        ^ string starts here
> 3 |   port 8080
>   |
>   = help: add closing '"' or use a heredoc for multiline strings
> ```

### Unterminated heredoc

> r[diagnostic.parser.unterminated-heredoc]
> When a heredoc is not terminated, the message SHOULD show the expected
> delimiter and where the heredoc started.
>
> ```
> error: unterminated heredoc, expected 'EOF'
>   --> config.styx:2:10
>   |
> 2 |   script <<EOF
>   |          ^^^^^ heredoc starts here
>   |
>   = note: reached end of file while looking for 'EOF'
>   = help: the closing delimiter must appear on its own line
> ```

### Heredoc delimiter too long

> r[diagnostic.parser.heredoc-delimiter-length]
> When a heredoc delimiter exceeds 16 characters, the message SHOULD state
> the limit.
>
> ```
> error: heredoc delimiter too long
>   --> config.styx:2:10
>   |
> 2 |   script <<THIS_DELIMITER_IS_WAY_TOO_LONG
>   |          ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ 35 characters
>   |
>   = help: delimiter must be at most 16 characters
> ```

### Heredoc indentation error

> r[diagnostic.parser.heredoc-indent]
> When a heredoc content line is less indented than the closing delimiter,
> the message SHOULD show both locations.
>
> ```
> error: heredoc line less indented than closing delimiter
>   --> config.styx:4:1
>   |
> 3 |     script <<BASH
> 4 | echo "hello"
>   | ^^^^ this line has no indentation
> 5 |     BASH
>   |     ---- closing delimiter is indented 4 spaces
>   |
>   = help: indent content to at least column 5, or dedent the closing delimiter
> ```

### Comment without preceding whitespace

> r[diagnostic.parser.comment-whitespace]
> When `//` appears without preceding whitespace (making it part of a scalar),
> the parser cannot distinguish user intent. If subsequent parsing fails, the
> message SHOULD note the potential comment issue.
>
> ```
> error: unexpected token 'comment'
>   --> config.styx:2:13
>   |
> 2 |   url foo// comment
>   |             ^^^^^^^ unexpected token
>   |
>   = note: '//' without preceding space is part of the scalar 'foo//'
>   = help: add a space before '//' to start a comment
> ```

### Duplicate key

> r[diagnostic.parser.duplicate-key]
> When a key appears twice in the same object, the message SHOULD show both
> locations.
>
> ```
> error: duplicate key 'port'
>   --> config.styx:4:3
>   |
> 2 |   port 8080
>   |   ---- first defined here
>   |
> 4 |   port 9090
>   |   ^^^^ duplicate key
> ```

### Cannot reopen object

> r[diagnostic.parser.no-reopen]
> When a dotted path attempts to add a key to an already-closed singleton
> object, the message SHOULD show both locations and suggest block form.
>
> ```
> error: cannot add key 'port' to 'server': object was already closed
>   --> config.styx:2:1
>   |
> 1 | server.host localhost
>   | ------ 'server' first defined here as a singleton object
> 2 | server.port 8080
>   | ^^^^^^^^^^^ cannot reopen 'server'
>   |
>   = help: use block form to define multiple keys:
>   |
>   | server {
>   |   host localhost
>   |   port 8080
>   | }
> ```

### Mixed separators

> r[diagnostic.parser.mixed-separators]
> When an object mixes comma and newline separators, the message SHOULD
> identify both styles and suggest picking one.
>
> ```
> error: mixed separators in object
>   --> config.styx:2:7
>   |
> 1 | {
> 2 |   a 1,
>   |      ^ comma here
> 3 |   b 2
>   |
>   = help: use either commas or newlines, not both:
>   |
>   | { a 1, b 2 }        // comma-separated
>   |
>   | {                   // newline-separated
>   |   a 1
>   |   b 2
>   | }
> ```

### Comma in sequence

> r[diagnostic.parser.sequence-comma]
> When a comma appears in a sequence, the message SHOULD explain that
> sequences use whitespace separation.
>
> ```
> error: unexpected ',' in sequence
>   --> config.styx:1:3
>   |
> 1 | (a, b, c)
>   |   ^ commas not allowed in sequences
>   |
>   = help: use whitespace to separate elements: (a b c)
> ```

### Attribute object in sequence

> r[diagnostic.parser.attr-in-sequence]
> When an attribute object appears as a direct sequence element, the message
> SHOULD explain the ambiguity and suggest block form.
>
> ```
> error: attribute object not allowed as sequence element
>   --> config.styx:2:3
>   |
> 2 |   a=1 b=2
>   |   ^^^^^^^ attribute object
>   |
>   = note: ambiguous whether this is one object {a:1, b:2} or two {a:1} {b:2}
>   = help: use block form: { a 1, b 2 }
> ```

### Trailing content after root

> r[diagnostic.parser.trailing-content]
> When content appears after a closed root object, the message SHOULD note
> that explicit root objects cannot have siblings.
>
> ```
> error: unexpected token after root object
>   --> config.styx:4:1
>   |
> 1 | {
>   | - root object starts here
> 2 |   key value
> 3 | }
>   | - root object ends here
> 4 | extra
>   | ^^^^^ unexpected token
>   |
>   = help: remove the '{ }' to allow multiple top-level entries
> ```

## Deserializer errors

### Type mismatch

> r[diagnostic.deser.type-mismatch]
> When a scalar cannot be interpreted as the target type, the message SHOULD
> identify the expected type and the actual value.
>
> ```
> error: type mismatch
>   --> config.styx:2:8
>   |
> 2 |   port "eight thousand"
>   |        ^^^^^^^^^^^^^^^^ expected integer, found string
>   |
>   = help: use a numeric value: port 8080
> ```

### Invalid integer

> r[diagnostic.deser.invalid-integer]
> When a scalar looks like an integer but is invalid (overflow, invalid chars),
> the message SHOULD be specific.
>
> ```
> error: integer out of range
>   --> config.styx:2:8
>   |
> 2 |   port 99999999999999999999
>   |        ^^^^^^^^^^^^^^^^^^^^ value exceeds u16 maximum (65535)
> ```

### Invalid duration

> r[diagnostic.deser.invalid-duration]
> When a scalar cannot be parsed as a duration, the message SHOULD show
> valid duration formats.
>
> ```
> error: invalid duration
>   --> config.styx:2:11
>   |
> 2 |   timeout 30 seconds
>   |           ^^ expected duration with unit
>   |
>   = help: valid formats: 30s, 10ms, 2h, 500us
>   = help: valid units: ns, us, µs, ms, s, m, h, d
> ```

### Invalid timestamp

> r[diagnostic.deser.invalid-timestamp]
> When a scalar cannot be parsed as an RFC 3339 timestamp, the message SHOULD
> identify the problem.
>
> ```
> error: invalid timestamp
>   --> config.styx:2:12
>   |
> 2 |   created 2026-13-01T00:00:00Z
>   |                ^^ month must be 01-12
>   |
>   = help: expected RFC 3339 format: YYYY-MM-DDTHH:MM:SSZ
> ```

### Invalid boolean

> r[diagnostic.deser.invalid-boolean]
> When a value is expected to be boolean but isn't `true` or `false`, the
> message SHOULD list the valid values.
>
> ```
> error: invalid boolean
>   --> config.styx:2:11
>   |
> 2 |   enabled yes
>   |           ^^^ expected 'true' or 'false'
> ```

### Enum not a single-key object

> r[diagnostic.deser.enum-not-singleton]
> When deserializing an enum and the value is not a single-key object, the
> message SHOULD explain enum representation.
>
> ```
> error: expected enum variant (single-key object)
>   --> config.styx:2:10
>   |
> 2 |   status { ok, err }
>   |          ^^^^^^^^^^^ object has 2 keys, expected 1
>   |
>   = help: enum values are represented as single-key objects:
>   |
>   | status.ok              // unit variant
>   | status.err { msg "x" } // variant with payload
> ```

### Unknown enum variant

> r[diagnostic.deser.unknown-variant]
> When an enum variant name doesn't match any defined variant, the message
> SHOULD list the valid variants.
>
> ```
> error: unknown variant 'unknown'
>   --> config.styx:2:10
>   |
> 2 |   status.unknown
>   |          ^^^^^^^ not a valid variant
>   |
>   = help: valid variants are: ok, pending, err
> ```

### Missing required field

> r[diagnostic.deser.missing-field]
> When a required field is missing during deserialization, the message SHOULD
> identify the field and the containing object.
>
> ```
> error: missing required field 'port'
>   --> config.styx:1:1
>   |
> 1 | server {
>   | ^^^^^^ in this object
> 2 |   host localhost
> 3 | }
>   |
>   = help: add the required field: port 8080
> ```

### Unknown field

> r[diagnostic.deser.unknown-field]
> When a field is present but not expected by the target type, the message
> SHOULD suggest similar field names if available.
>
> ```
> error: unknown field 'prot'
>   --> config.styx:3:3
>   |
> 3 |   prot 8080
>   |   ^^^^ unknown field
>   |
>   = help: did you mean 'port'?
>   = note: expected fields: host, port, timeout
> ```

### Expected object, found scalar

> r[diagnostic.deser.expected-object]
> When an object is expected but a scalar is found.
>
> ```
> error: expected object, found scalar
>   --> config.styx:2:10
>   |
> 2 |   server localhost
>   |          ^^^^^^^^^ expected object
>   |
>   = help: use braces for object: server { host localhost }
> ```

### Expected sequence, found scalar

> r[diagnostic.deser.expected-sequence]
> When a sequence is expected but a scalar is found.
>
> ```
> error: expected sequence, found scalar
>   --> config.styx:2:9
>   |
> 2 |   hosts localhost
>   |         ^^^^^^^^^ expected sequence
>   |
>   = help: use parentheses for sequence: hosts (localhost)
> ```

## Schema validation errors

### Type constraint violation

> r[diagnostic.schema.type-violation]
> When a value doesn't match the schema's type constraint.
>
> ```
> error: schema violation: expected @integer, found string
>   --> config.styx:2:8
>   |
> 2 |   port "8080"
>   |        ^^^^^^ expected integer
>   |
>   --> schema.styx:3:8
>   |
> 3 |   port @integer
>   |        -------- required by schema
> ```

### Literal mismatch

> r[diagnostic.schema.literal-mismatch]
> When a value doesn't match a literal constraint in the schema.
>
> ```
> error: schema violation: expected literal 'v1', found 'v2'
>   --> config.styx:1:9
>   |
> 1 | version v2
>   |         ^^ expected 'v1'
>   |
>   --> schema.styx:1:9
>   |
> 1 | version v1
>   |         -- literal value required by schema
> ```

### Missing required field (schema)

> r[diagnostic.schema.missing-required]
> When a required field per the schema is missing.
>
> ```
> error: missing required field 'host'
>   --> config.styx:1:1
>   |
> 1 | server {
>   | ^^^^^^ missing 'host'
> 2 |   port 8080
> 3 | }
>   |
>   --> schema.styx:2:3
>   |
> 2 |   host @string
>   |   ---- required field defined here (no '?' suffix)
> ```

### Unexpected field (schema)

> r[diagnostic.schema.unexpected-field]
> When a field is present but not defined in the schema.
>
> ```
> error: unexpected field 'debug'
>   --> config.styx:4:3
>   |
> 4 |   debug true
>   |   ^^^^^ not defined in schema
>   |
>   --> schema.styx:1:1
>   |
> 1 | server {
>   | ------ schema for 'server' defined here
>   |
>   = note: schema defines: host, port, timeout
> ```

### Union type mismatch

> r[diagnostic.schema.union-mismatch]
> When a value doesn't match any type in a union.
>
> ```
> error: value matches no type in union
>   --> config.styx:2:10
>   |
> 2 |   timeout (30 seconds)
>   |           ^^^^^^^^^^^^^ none of the union types match
>   |
>   --> schema.styx:3:11
>   |
> 3 |   timeout? @union(@duration @integer)
>   |            -------------------------- expected one of these types
>   |
>   = note: tried @duration: invalid duration format
>   = note: tried @integer: expected scalar, found sequence
> ```

### Flatten collision

> r[diagnostic.schema.flatten-collision]
> When flattened fields collide with the containing object's fields.
>
> ```
> error: field collision in @flatten
>   --> schema.styx:8:3
>   |
> 3 |   name @string
>   |   ---- 'name' defined in Base
>   |
> ...
>   |
> 8 |   name @string
>   |   ^^^^ 'name' also defined in Derived
>   |
>   --> schema.styx:7:3
>   |
> 7 |   base @flatten(@Base)
>   |   ---- Base is flattened here
> ```

### Unknown type reference

> r[diagnostic.schema.unknown-type]
> When a type reference cannot be resolved. This is a warning, not an error,
> since the type may come from an external source.
>
> ```
> warning: unknown type '@ExternalConfig'
>   --> schema.styx:5:10
>   |
> 5 |   config @ExternalConfig
>   |          ^^^^^^^^^^^^^^^ type not defined in this schema
>   |
>   = note: treating as @any; validation will be skipped for this field
> ```

### Invalid type name

> r[diagnostic.schema.invalid-type-name]
> When a type reference doesn't match the type name grammar.
>
> ```
> error: invalid type name
>   --> schema.styx:2:8
>   |
> 2 |   port @123-invalid
>   |        ^^^^^^^^^^^^ type names must start with a letter or underscore
>   |
>   = help: valid examples: @string, @MyType, @my_type, @my-type
> ```

### Doc comment without attachment

> r[diagnostic.schema.unattached-doc]
> When a doc comment is not followed by a definition.
>
> ```
> error: doc comment has no attachment
>   --> schema.styx:3:1
>   |
> 2 | /// This comment is orphaned
> 3 |
>   | ^ blank line breaks doc comment attachment
> 4 | /// This attaches to 'bar'
> 5 | bar @string
>   |
>   = help: remove the blank line, or delete the orphaned comment
> ```

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

#[derive(Facet)]
struct Config {
    server: Server,
}

#[derive(Facet)]
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

### Enum deserialization

Enums use externally-tagged representation. The dotted path syntax provides ergonomic shorthand:

```rust
#[derive(Facet)]
enum Status {
    Ok,
    Pending,
    Err { message: String, code: Option<i32> },
}

#[derive(Facet)]
struct Response {
    status: Status,
}

// Unit variant (all equivalent)
let r: Response = styx::from_str("status.ok")?;
let r: Response = styx::from_str("status.ok @")?;
let r: Response = styx::from_str("status { ok @ }")?;

// Variant with payload
let r: Response = styx::from_str(r#"
    status.err {
        message "connection timeout"
        code 504
    }
"#)?;

// Using attribute syntax for payload
let r: Response = styx::from_str(r#"
    status.err message="timeout" code=504
"#)?;
```

## Design invariants (non-normative)

STYX enforces the following invariants:

- **No implicit merges**: Objects are never merged. Each key appears exactly once.
- **No reopening**: Once an object is closed, it cannot be extended with additional keys.
- **No indentation-based structure**: All structure is explicit via `{}` and `()`.
- **No semantic interpretation during parsing**: The parser produces opaque scalars; meaning is assigned during deserialization.
- **All structure is explicit**: Braces and parentheses define nesting, not whitespace or conventions.
- **Commas in objects only**: Commas are optional separators in objects (interchangeable with newlines). Sequences use whitespace only.
- **Explicit unit value**: `@` is the unit value, distinct from `()` (empty sequence). Keys without values implicitly produce `@`. This enables concise unit variants (`status.ok`) and flag-like entries (`enabled`).
