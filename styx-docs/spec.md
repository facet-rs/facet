# styx

STYX is a document language designed to replaced YAML, TOML, JSON, etc. for documents authored
by humans.

## Value types

STYX values are one of:

  * Scalar
  * Object
  * Sequence
  
## Scalars

> r[scalar.interpretation]
> All scalars are lexically parsed, never interpreted by the core language.
> Meaning is assigned only by conventions or schemas.

### Bare scalar tokens

> r[scalar.bare.definition]
> A bare scalar is any non-whitespace token that does not contain spaces.
>
> ```styx
> foo
> web
> RUST_LOG
> ghcr.io/acme/web:1.2.3
> ```

> r[scalar.bare.opaque]
> Bare scalars are opaque atoms to the core language.

### Quoting

> r[scalar.quoting]
> Quoted forms (`"..."`, `r#"..."#`, heredocs) are lexical delimiters that allow scalars
> to contain spaces and special characters. They do not change the scalar's type or meaning.
> `foo` and `"foo"` produce identical scalar values.

### Quoted strings

> r[scalar.string.quoted]
> Quoted strings allow spaces and escape sequences:
>
> ```styx
> "hello world"
> "foo\nbar"
> ```

### Raw strings

> r[scalar.string.raw]
> Raw strings do not process escape sequences:
>
> ```styx
> r#"no need to escape "double quotes" in here"#
> ```

> r[scalar.string.raw.nesting]
> Multiple `#` characters allow embedding `"#` sequences:
>
> ```styx
> r##"can contain "# without closing"##
> r###"can contain "## without closing"###
> ```
>
> The number of `#` in the closing delimiter MUST match the opening.

### Multiline strings (heredoc)

> r[scalar.string.heredoc]
> Multiline strings are explicitly delimited and preserve content:
>
> ```styx
> <<EOF
> line one
> line two
> EOF
> ```

### Scalar interpretation

> r[scalar.interpretation.deferred]
> The parser treats all scalars as opaque atoms. Interpretation is deferred
> until deserialization.

> r[scalar.interpretation.required]
> A conforming STYX implementation MUST support interpreting scalars as the
> following types when requested:
>
> - Integers (signed and unsigned, various widths)
> - Floating point numbers
> - Booleans (`true`, `false`)
> - Null (`null`)
> - Strings (the scalar's raw text)
> - Durations (e.g., `30s`, `10ms`, `2h`)
> - Timestamps (RFC 3339)
> - Regular expressions (e.g., `/foo/i`)
> - Byte sequences (hex `0xdeadbeef`, base64 `b64"..."`)

> r[scalar.interpretation.extensible]
> Implementations MAY support additional forms beyond this list.
> Enums, paths, URLs, email addresses, IP addresses, semantic versions, and other
> domain-specific types are commonly supported as extensions.

## Objects

There are several object forms in STYX.

### Block objects

> r[object.block.delimiters]
> Block objects MUST start with `{` and end with `}`:
> 
> ```styx
> {
>   key value
>   key value
> }
> ```

> r[object.block.separators]
> In block objects, keys and values MUST separated by spaces, and key-value pairs MUST be separated by newlines (`\n`) or commas (`,`):
> 
> ```styx
> // this is fine, too!
> {
>   key value, key value
> }
> ```
> 
> ```styx
> { key value, key value } // and so is this
> ```

> r[object.block.separators.trailing]
>
> Trailing commas in a block object MUST be treated as a syntax error:
>
> ```styx
> {
>   key value, // <- ERROR: expected another key
> }
> ```

> r[object.key.bare]
> Bare keys MUST only contain /[A-Za-z0-9-_]/.
>
> Any key that contains a space, a unicode character, etc., must be double-quoted.

> r[object.block.scope]
> Block objects MAY appear anywhere a value is allowed, including:
>
> - as the document root
> - as object values
> - inside sequences

### Attribute objects

Attribute objects are syntactic sugar for block objects, providing a compact inline form.

> r[object.attr.definition]
> An attribute object is an object introduced implicitly by one or more assignments using `=`:
>
> ```styx
> labels app=web tier=frontend
> ```
>
> This is equivalent to:
>
> ```styx
> labels {
>   app web
>   tier frontend
> }
> ```

> r[object.attr.binding]
> When parsing a value position, if the next token is an assignment token (`key=`),
> the value is parsed as an attribute object. `=` binds tighter than whitespace.

> r[object.attr.assignment]
> An assignment has the form `key=value`, where:
>
> - `=` separates the key and value with no surrounding whitespace
> - The value MUST be exactly one value
> - The value MAY be a block object (`{ ... }`) or a sequence (`( ... )`)
>
> ```styx
> config server={ host localhost, port 8080 }
> with toolchain=stable components=(clippy rustfmt)
> ```

> r[object.attr.grouping]
> Multi-value data in an assignment MUST be explicitly grouped:
>
> ```styx
> // Valid: grouped with ()
> components=(clippy rustfmt)
>
> // Invalid: value is not grouped
> components=clippy rustfmt
> ```

> r[object.attr.termination]
> An attribute object consists of a contiguous run of assignments.
> It ends when:
>
> - a token appears that cannot start an assignment, or
> - the surrounding block (`{}`) closes

> r[object.attr.no-comma]
> Attribute objects do not use commas between assignments.

> r[object.attr.no-reopen]
> Attribute objects do not support implicit merging and cannot be reopened or extended later.

> r[object.attr.sequence-restriction]
> Attribute objects MUST NOT appear as direct elements of a sequence.
>
> ```styx
> // Invalid: attribute object as direct sequence element
> (
>   a=1 b=2
> )
>
> // Valid: block object containing attribute object
> (
>   { labels app=web tier=frontend }
>   { labels app=api tier=backend }
> )
> ```

> r[object.attr.sequence-restriction.scope]
> This restriction does not apply to attribute objects nested within object literals
> that are themselves sequence elements.

> r[object.attr.direct-element]
> A "direct element of a sequence" is a value parsed immediately within `( )`
> without being contained inside an explicit block object `{ }` or sequence `( )`.

### Object equivalence

> r[object.equivalence]
> Block objects and attribute objects are semantically equivalent.
> Both forms produce the same object value at the data model level.
>
> ```styx
> // These are identical:
> labels app=web tier=frontend
>
> labels {
>   app web
>   tier frontend
> }
> ```

> r[object.block.intent]
> Block objects prioritize clarity, explicit structure, and ease of navigation and editing.

> r[object.attr.intent]
> Attribute objects prioritize compactness and inline readability for common map-like patterns
> (labels, env, options). They are intentionally constrained to remain local, predictable,
> and visually obvious.

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
