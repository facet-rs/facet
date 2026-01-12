---
weight = 3
slug = "schema-spec"
---

# Schemas

Schemas define the expected structure of STYX documents for validation purposes.
They are optional — deserialization works with target types directly (e.g., Rust structs).
Schemas are useful for text editors, CLI tools, and documentation.

STYX schemas are themselves STYX documents.

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

## Type references

In schemas, tags name types rather than enum variants.

> r[schema.type-ref]
> A tag like `@string` or `@u16` references a type from the standard vocabulary or a user-defined type.
> A bare scalar without `@` is a literal value constraint.
>
> ```styx
> version 1        // must be exactly "1"
> version @u32     // must be an unsigned 32-bit integer
> ```

> r[schema.type-ref.literal]
> To constrain a field to a literal value starting with `@`, use a quoted scalar.
>
> ```styx
> tag "@mention"   // literal: must be exactly "@mention"
> ```

## Standard types

> r[schema.type.primitives]
> Primitive types:
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

## Optional fields

> r[schema.optional]
> A key ending with `?` indicates the field may be omitted.
>
> ```styx
> server {
>   host @string
>   timeout? @duration   // optional
> }
> ```

## Unions

> r[schema.union]
> `@union(...)` matches if the value matches any listed type. Types are checked in order.
>
> ```styx
> id @union(@u64 @string)           // integer or string
> value @union(@string @unit)       // nullable string
> ```

## Sequences

> r[schema.sequence]
> `(@type)` matches a sequence where every element matches `@type`.
>
> ```styx
> hosts (@string)
> servers ({
>   host @string
>   port @u16
> })
> ```

## Maps

> r[schema.map]
> `@map(@K @V)` matches an object where all values match `@V`.
> Keys in STYX are always strings. `@map(@V)` is shorthand for `@map(@string @V)`.
>
> ```styx
> env @map(@string)              // string → string
> ports @map(@string @u16)       // string → u16
> ```

## Named types

> r[schema.type.definition]
> Named types are defined at the schema root. Use `@TypeName` to reference them.
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
    schema @map(@string @Schema)
  }

  Meta {
    id @string
    version @string
    description? @string
  }

  Schema @union(
    @string                    // literal value constraint
    @TypeRef                   // @string, @u64, @MyType, etc.
    @Object                    // { field @type }
    @Sequence                  // (@type)
    @Union                     // @union(@type @type)
    @Enum                      // @enum{ a, b { x @type } }
    @Map                       // @map(@K @V)
    @Flatten                   // @flatten(@Type)
  )

  TypeRef @string              // tag like @string, @u64, @CustomType

  Object @map(@string @Schema) // keys to schemas (keys ending in ? are optional)

  Sequence (@Schema)           // homogeneous sequence

  Union {
    @union (@Schema)           // list of alternative schemas
  }

  Enum {
    @enum @map(@string @union(@unit @Object))  // variant name → optional payload
  }

  Map {
    @map (@Schema @Schema)     // key schema, value schema (key usually @string)
  }

  Flatten {
    @flatten @TypeRef          // inline fields from another type
  }
}
```
