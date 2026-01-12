---
weight = 2
slug = "parser-spec"
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
>   url postgres://...
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
> url https://example.com/path?query=1    // bare scalar includes = and /
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
> r#"contains \"quotes\""#
> r##"contains \"# in the middle"##
> r###"contains \"## in the middle"###
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
> To use a literal key starting with `@` in a document, quote it: `"\\@foo"`.

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
