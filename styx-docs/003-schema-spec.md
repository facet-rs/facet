---
weight = 3
slug = "schema-spec"
---

# Schemas

Schemas define the expected structure of STYX documents for validation purposes.
They are optional — deserialization works with target types directly (e.g., Rust structs).
Schemas are useful for text editors, CLI tools, and documentation.

## Why STYX works for schemas

STYX schemas are themselves STYX documents. This works because of tags and implicit unit:

- A tag like `@string` is shorthand for `@string@` — a tag with unit payload
- In schema context, tags name types: `@string`, `@u64`, `@MyCustomType`
- Built-in tags like `@union`, `@map`, `@enum` take payloads describing composite types
- User-defined type names are just tags referencing definitions elsewhere in the schema

For example:

```styx
host @string           // field "host" must match type @string
port @u16              // field "port" must match type @u16
id @union(@u64 @string) // @union tag with sequence payload
```

The `@union(@u64 @string)` is:
- Tag `@union` with payload `(@u64 @string)`
- The payload is a sequence of two tagged unit values
- Semantically: "id must match @u64 or @string"

This uniformity means schemas require no special syntax — just STYX with semantic interpretation of tags as types.

In schema definitions, the unit value `@` (not a tag) is used as a wildcard meaning “any type reference” —
that is, any tagged unit value like `@string` or `@MyType`.

## Schema file structure

> r[schema.file]
> A schema file has three top-level keys: `meta` (required), `imports` (optional), and `schema` (required).
>
> ```styx
> meta {
>   id https://example.com/schemas/server
>   version 2026-01-11
>   description "Server configuration schema"
> }
>
> schema {
>   @ {
>     server @Server
>   }
>
>   Server {
>     host @string
>     port @u16
>   }
> }
> ```

> r[schema.meta]
> The `meta` block contains schema metadata: `id` (required), `version` (required), and `description` (optional).

> r[schema.root]
> Inside `schema`, the key `@` defines the expected structure of the document root.
> Other keys define named types that can be referenced with `@TypeName`.

## Imports

> r[schema.imports]
> The `imports` block maps namespace prefixes to external schema locations (URLs or paths).
> Paths are resolved relative to the importing schema file.
> Imported types are referenced as `@namespace.TypeName`.
>
> ```styx
> meta {
>   id https://example.com/schemas/app
>   version 2026-01-11
> }
>
> imports {
>   common https://example.com/schemas/common.styx
>   auth https://example.com/schemas/auth.styx
> }
>
> schema {
>   @ {
>     user @auth.User
>     settings @common.Settings
>   }
> }
> ```

## Schema declaration in documents

> r[schema.declaration]
> A document MAY declare its schema inline or reference an external schema file.
>
> ```styx
> // Inline schema
> @ {
>   schema {
>     @ { server { host @string, port @u16 } }
>   }
> }
>
> server { host localhost, port 8080 }
> ```
>
> ```styx
> // External schema reference
> @ "https://example.com/schemas/server.styx"
>
> server { host localhost, port 8080 }
> ```

## Types and literals

> r[schema.type]
> A tagged unit denotes a type constraint.
>
> ```styx
> version @u32     // type: must be an unsigned 32-bit integer
> host @string     // type: must be a string
> ```
>
> Since unit payloads are implicit, `@u32` is shorthand for `@u32@` — which makes STYX schemas valid STYX.

> r[schema.literal]
> A scalar denotes a literal value constraint.
>
> ```styx
> version 1        // literal: must be exactly "1"
> enabled true     // literal: must be exactly "true"
> tag "@mention"   // literal: must be exactly "@mention" (quoted)
> ```

## Standard types

> r[schema.type.primitives]
> These tags are built-in type constraints:
>
> | Type | Description |
> |------|-------------|
> | `@string` | any scalar |
> | `@boolean` | `true` or `false` |
> | `@u8`, `@u16`, `@u32`, `@u64`, `@u128` | unsigned integers |
> | `@i8`, `@i16`, `@i32`, `@i64`, `@i128` | signed integers |
> | `@f32`, `@f64` | floating point |
> | `@duration` | e.g., `30s`, `10ms`, `2h` |
> | `@timestamp` | RFC 3339, e.g., `2026-01-10T18:43:00Z` |
> | `@regex` | e.g., `/^hello$/i` |
> | `@bytes` | hex `0xdeadbeef` or base64 `SGVsbG8=` |
> | `@any` | any value |
> | `@unit` | the unit value `@` |
> | `@optional(@T)` | value of type `@T` or absent |

## Optional fields

> r[schema.optional]
> `@optional(@T)` matches either a value of type `@T` or absence of a value.
> For object fields, `key?` is shorthand for `key @optional(...)`.
> Absence means the field key is not present in the object (it does not mean the field value is `@`).
>
> ```compare
> /// styx
> // Shorthand
> server {
>   host @string
>   timeout? @duration
> }
> /// styx
> // Canonical
> server {
>   host @string
>   timeout @optional(@duration)
> }
> ```

## Objects

> r[schema.object]
> An object schema is written as an object mapping field names (scalars) to schemas.
> By default, object schemas are **closed**: keys not mentioned in the schema are forbidden.
>
> To allow additional keys, use a special entry with key `@` (unit key) to define the schema for
> all additional fields. If present, any key not explicitly listed MUST match the `@` entry's schema.
> The key `@` is reserved for this purpose and cannot be used to describe a literal unit-key field.
>
> ```styx
> // Closed object (default): only host and port allowed
> Server {
>   host @string
>   port @u16
> }
>
> // Open object: allow any extra string fields
> Labels {
>   @ @string
> }
>
> // Mixed: known fields plus additional string→string
> Config {
>   name @string
>   @ @string
> }
> ```

## Unions

> r[schema.union]
> `@union(...)` matches if the value matches any of the listed types.
>
> ```styx
> id @union(@u64 @string)           // integer or string
> value @union(@string @unit)       // nullable string
> ```

## Sequences

> r[schema.sequence]
> A sequence schema matches a sequence where every element matches the inner schema.
> The sequence schema MUST contain exactly one element: `(@T)`.
> Tuple/positional schemas like `(@A @B)` are not supported; use `(@union(@A @B))` for heterogeneous lists.
>
> ```styx
> hosts (@string)                   // sequence of strings
> servers ({                        // sequence of objects
>   host @string
>   port @u16
> })
> ids (@union(@u64 @string))        // sequence of ids
> ```

## Maps

> r[schema.map]
> `@map(@K @V)` matches an object where all keys match `@K` and all values match `@V`.
> `@map(@V)` is shorthand for `@map(@string @V)`.
>
> ```styx
> env @map(@string)              // string → string
> ports @map(@u16)               // string → u16
> ```

## Named types

> r[schema.type.definition]
> Named types are defined inside the `schema` block. Use `@TypeName` to reference them.
>
> ```styx
> TlsConfig {
>   cert @string
>   key @string
> }
>
> server {
>   tls @TlsConfig
> }
> ```

## Flatten

> r[schema.flatten]
> `@flatten(@Type)` inlines fields from another type into the current object.
> The document is flat; deserialization reconstructs the nested structure.
>
> ```styx
> User { name @string, email @string }
>
> Admin {
>   user @flatten(@User)
>   permissions (@string)
> }
> ```
>
> Document: `name Alice, email alice@example.com, permissions (read write)`

## Enums

> r[schema.enum]
> `@enum{...}` defines valid variant names and their payloads.
>
> ```styx
> status @enum{
>   ok
>   pending
>   err { message @string }
> }
> ```
>
> Values use the tag syntax: `@ok`, `@pending`, `@err{message "timeout"}`.

## Meta schema

The schema for STYX schema files:

```styx
meta {
  id https://styx-lang.org/schemas/schema
  version 2026-01-11
  description "Schema for STYX schema files"
}

schema {
  /// The root structure of a schema file.
  @ {
    /// Schema metadata (required).
    meta @Meta
    /// External schema imports (optional).
    imports? @map(@string @string)
    /// Type definitions: @ for document root, strings for named types.
    schema @map(@union(@string @unit) @Schema)
  }

  /// Schema metadata.
  Meta {
    /// Unique identifier for the schema (URL recommended).
    id @string
    /// Schema version (date or semver).
    version @string
    /// Human-readable description.
    description? @string
  }

  /// A type constraint.
  Schema @union(
    @string      /// Literal value constraint.
    @            /// Type reference (any tag with unit payload).
    @Object      /// Object schema: { field @type }
    @Sequence    /// Sequence schema: (@type)
    @Union       /// Union: @union(@A @B)
    @Optional    /// Optional: @optional(@T)
    @Enum        /// Enum: @enum{ a, b { x @type } }
    @Map         /// Map: @map(@K @V)
    @Flatten     /// Flatten: @flatten(@Type)
  )

  /// Object schema: maps keys to type constraints. The unit key (@) is reserved for "additional fields".
  Object @map(@union(@string @unit) @Schema)

  /// Sequence schema: all elements match the inner type.
  Sequence (@Schema)

  /// Union: matches any of the listed types.
  Union (@Schema)

  /// Optional: value of type T or absent.
  Optional @Schema

  /// Enum: variant names with optional payloads.
  Enum @map(@string @union(@unit @Object))

  /// Map: @map(@V) for string keys, @map(@K @V) for explicit key type.
  Map @union(
    (@Schema)
    (@Schema @Schema)
  )

  /// Flatten: inline fields from another type.
  Flatten @
}
```
