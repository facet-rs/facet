---
weight = 3
slug = "schema-spec"
---

# Part 2: Schemas

Schemas define the expected structure of STYX documents. They specify what keys exist,
what types values must have, and whether fields are required or optional.

STYX schemas are themselves STYX documents. They can be inline (embedded in a document)
or external (separate files). Schema constructs use tagged sequences and tagged objects
(see `r[sequence.tagged]` and `r[object.tagged]`).

## Schema declaration

A document SHOULD declare its schema using the `@schema` directive at the root.
This allows tools like editors and validators to provide immediate feedback.

> r[schema.declaration]
> A document MAY declare its schema using the reserved key `@schema` at the document root.
> The value MUST be either an inline schema object or a path/URL to an external schema file.
>
> ```styx
> // Inline schema declaration
> @schema {
>   server {
>     host @string
>     port @u16
>   }
> }
>
> server {
>   host localhost
>   port 8080
> }
> ```
>
> ```styx
> // External schema (local path)
> @schema ./server.schema.styx
>
> server {
>   host localhost
>   port 8080
> }
> ```
>
> ```styx
> // External schema (URL)
> @schema https://example.com/schemas/server.styx
>
> server {
>   host localhost
>   port 8080
> }
> ```

> r[schema.resolution]
> Implementations MUST support resolving local paths relative to the document's location.
> Implementations SHOULD support resolving remote schemas via HTTPS URLs.
> If no `@schema` directive is present, schema application is determined by the processing tool.

## Schema metadata

Standalone schema files declare metadata using the `@meta` directive.

> r[schema.meta]
> A schema file MAY include an `@meta` directive at the root to declare metadata.
>
> ```styx
> @meta {
>   id https://example.com/schemas/server
>   version 2026-01-11
>   description "Server configuration schema"
> }
>
> server {
>   host @string
>   port @u16
> }
> ```

> r[schema.meta.id]
> Standalone schema files (not inline schemas) MUST include an `id` field in `@meta`.
> The ID SHOULD be a URL that uniquely identifies this schema.
> Implementations MUST warn or error when multiple schemas with the same ID are loaded.

> r[schema.meta.version]
> The `version` field is a date in `YYYY-MM-DD` format indicating when the schema
> was last modified. This is informational and does not affect validation.

> r[schema.meta.description]
> The `description` field is a human-readable summary of the schema's purpose.
> This is informational and MAY be displayed by tooling.

## Imports

Schemas can import type definitions from other schema files using the `@import` directive.

> r[schema.import]
> The `@import` directive imports types from external schema files into namespaces.
> Each key is a namespace name, and the value is a path or URL to the schema file.
>
> ```styx
> @import {
>   types ./common/types.styx
>   config https://example.com/schemas/config.styx
> }
>
> server {
>   host @string
>   port @u16
>   tls @types.TlsConfig       // from ./common/types.styx
>   logging @config.Logging    // from the URL
> }
> ```

> r[schema.import.namespace]
> Imported types MUST be referenced using their namespace prefix: `@namespace.TypeName`.
> This avoids collisions between types with the same name from different sources.

> r[schema.import.resolution]
> Import paths follow the same resolution rules as `@schema` (see `r[schema.resolution]`).

## Type references

Type references use the `@` prefix to distinguish types from literal values:

```styx
/// A server configuration schema
server {
  host @string
  port @u16
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
> version @u32       // must be an unsigned 32-bit integer
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
> Integer types are sized and match their Rust equivalents:
>
> | Type | Range |
> |------|-------|
> | `@u8` | 0 to 255 |
> | `@u16` | 0 to 65,535 |
> | `@u32` | 0 to 4,294,967,295 |
> | `@u64` | 0 to 18,446,744,073,709,551,615 |
> | `@u128` | 0 to 2¹²⁸−1 |
> | `@i8` | −128 to 127 |
> | `@i16` | −32,768 to 32,767 |
> | `@i32` | −2,147,483,648 to 2,147,483,647 |
> | `@i64` | −2⁶³ to 2⁶³−1 |
> | `@i128` | −2¹²⁷ to 2¹²⁷−1 |
> | `@usize` | Platform-dependent (32 or 64 bits, unsigned) |
> | `@isize` | Platform-dependent (32 or 64 bits, signed) |
>
> Integer syntax:
> ```
> integer = ["-" | "+"] digit+
> digit   = "0"..."9"
> ```
>
> Examples: `0`, `42`, `-10`, `+5`
>
> Schema validation MUST reject values outside the type's range.

> r[schema.type.float]
> Float types are sized and match their Rust equivalents:
>
> | Type | Description |
> |------|-------------|
> | `@f32` | 32-bit IEEE 754 floating point |
> | `@f64` | 64-bit IEEE 754 floating point |
>
> Float syntax:
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
> time      = hour ":" minute ":" second [ "." fraction]
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
> metadata @map(@string @any)   // arbitrary key-value pairs
> extensions @any               // any structure
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
  port @u16         // required
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
id @union(@u64 @string)

// Duration, integer, or unit
timeout @union(@duration @u64 @unit)
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
> For overlapping types (e.g., `@union(@u64 @string)`), more specific types
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
  port @u16
})
```

> r[schema.sequence]
> A sequence schema `(@type)` matches a sequence where every element matches `@type`.

## Maps

Maps are objects with typed keys and values. They use the `@map` tagged sequence:

```styx
/// Environment variables (string → string)
env @map(@string)

/// Port mappings (string → u16)
ports @map(@string @u16)

/// Generic counts (string → u64)
counts @map(@string @u64)
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
> `@map(@K @V)` matches an object where keys match `@K` and values match `@V`.
> The one-argument form `@map(@T)` is shorthand for `@map(@T @T)`.
>
> ```styx
> env @map(@string)           // string → string (shorthand)
> env @map(@string @string)   // equivalent explicit form
> ports @map(@string @u16)    // string → u16
> ```
>
> The value type can be an inline object schema:
>
> ```styx
> /// Named server configurations
> servers @map(@string {
>   host @string
>   port @u16
>   timeout? @duration
> })
> ```

## Nested objects

Object schemas can be nested inline:

```styx
server {
  host @string
  port @u16
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
  port @u16
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
    code? @i32
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
  port @u16

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
