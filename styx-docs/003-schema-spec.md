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

In schema definitions, `@` (unit) represents "any tag" — a type reference to a built-in or user-defined type.

## Schema file structure

> r[schema.file]
> A schema file has two top-level keys: `meta` (required) and `schema` (required).
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
> | `@bytes` | hex `0xdeadbeef` or base64 `b64"SGVsbG8="` |
> | `@any` | any value |
> | `@unit` | the unit value `@` |
> | `@optional(@T)` | value of type `@T` or absent |

## Optional fields

> r[schema.optional]
> `@optional(@T)` matches either a value of type `@T` or absence of a value.
> For object fields, `key?` is shorthand for `key @optional(...)`.
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
  @ {
    meta @Meta
    schema @map(@union(@string @unit) @Schema)
  }

  Meta {
    id @string
    version @string
    description? @string
  }

  Schema @union(
    @string                    // literal value constraint
    @                          // type reference (any tag with unit payload)
    @Object                    // { field @type }
    @Sequence                  // (@type)
    @Union                     // @union(@type @type)
    @Optional                  // @optional(@type)
    @Enum                      // @enum{ a, b { x @type } }
    @Map                       // @map(@K @V)
    @Flatten                   // @flatten(@Type)
  )

  Object @map(@string @Schema) // keys to schemas (keys ending in ? are optional)

  Sequence (@Schema)           // homogeneous sequence

  // @union(@A @B @C) — matches any of the listed types
  Union (@Schema)

  // @optional(@T) — value or absent
  Optional @Schema

  // @enum{ a, b { x @type } } — variant name → optional payload
  Enum @map(@string @union(@unit @Object))

  // @map(@V) — string keys, value type V
  // @map(@K @V) — explicit key and value types
  Map @union(
    (@Schema)
    (@Schema @Schema)
  )

  // @flatten(@Type) — inline fields from another type
  Flatten @
}
```
