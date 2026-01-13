+++
title = "Schema"
weight = 3
slug = "schema"
insert_anchor_links = "heading"
+++

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

In schema definitions, the unit value `@` (not a tag) is used as a wildcard meaning "any type reference" —
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
>   @ @object{
>     server @Server
>   }
>
>   Server @object{
>     host @string
>     port @u16
>   }
> }
> ```

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
>   @ @object{
>     user @auth.User
>     settings @common.Settings
>   }
> }
> ```

## Schema declaration in documents

> r[schema.declaration]
> A document MAY declare its schema using a tagged unit key `@` at the document root.
> The value is either a URL/path string (external reference) or an inline schema object.
> Inline schemas use a simplified form: only the `schema` block is required; `meta` and `imports` are optional.
>
> ```styx
> // External schema reference
> @ https://example.com/schemas/server.styx
>
> server {host localhost, port 8080}
> ```
>
> ```styx
> // Inline schema (simplified form)
> @ {
>   schema {
>     @ @object{server @object{host @string, port @u16}}
>   }
> }
>
> server {host localhost, port 8080}
> ```

## Types and constraints

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
> A scalar denotes a literal value constraint. The unit value `@` is also a literal constraint.
>
> ```styx
> version 1        // literal: must be exactly "1"
> enabled true     // literal: must be exactly "true"
> tag "@mention"   // literal: must be exactly "@mention" (quoted)
> nothing @        // literal: must be exactly @ (unit)
> ```

### Standard types

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
> | `@datetime` | RFC 3339 date-time, e.g., `2026-01-10T18:43:00Z` |
> | `@date` | RFC 3339 full-date, e.g., `2026-01-10` |
> | `@regex` | e.g., `/^hello$/i` |
> | `@hex` | hex-encoded bytes, e.g., `deadbeef` |
> | `@b64` | base64-encoded bytes, e.g., `SGVsbG8=` |
> | `@any` | any value |
>
> Composite type constructors (`@optional`, `@union`, `@map`, `@enum`, `@flatten`) are described in their own sections.

### Optional fields

> r[schema.optional]
> `@optional(@T)` matches either a value of type `@T` or absence of a value.
> Absence means the field key is not present in the object (it does not mean the field value is `@`).
>
> ```styx
> server @object{
>   host @string
>   timeout @optional(@duration)
> }
> ```

## Composite types

### Objects

> r[schema.object]
> `@object{...}` defines an object schema mapping field names (scalars) to schemas.
> By default, object schemas are **closed**: keys not mentioned in the schema are forbidden.
>
> To allow additional keys, use a special entry with key `@` (unit key) to define the schema for
> all additional fields. If present, any key not explicitly listed MUST match the `@` entry's schema.
> The key `@` is reserved for this purpose and cannot be used to describe a literal unit-key field.
>
> ```styx
> // Closed object (default): only host and port allowed
> Server @object{
>   host @string
>   port @u16
> }
>
> // Open object: allow any extra string fields
> Labels @object{
>   @ @string
> }
>
> // Mixed: known fields plus additional string→string
> Config @object{
>   name @string
>   @ @string
> }
> ```

### Unions

> r[schema.union]
> `@union(...)` matches if the value matches any of the listed types.
>
> ```styx
> id @union(@u64 @string)           // integer or string
> value @union(@string @unit)       // nullable string
> ```

### Sequences

> r[schema.sequence]
> `@seq(@T)` defines a sequence schema where every element matches type `@T`.
>
> ```styx
> hosts @seq(@string)               // sequence of strings
> servers @seq(@object{             // sequence of objects
>   host @string
>   port @u16
> })
> ids @seq(@union(@u64 @string))    // sequence of ids
> ```

### Maps

> r[schema.map]
> `@map(@K @V)` matches an object where all keys match `@K` and all values match `@V`.
> `@map(@V)` is shorthand for `@map(@string @V)`.
>
> ```styx
> env @map(@string)              // string → string
> ports @map(@u16)               // string → u16
> ```
>
> r[schema.map.keys]
> Valid key types are scalar types that can be parsed from the key's text representation:
> `@string`, integer types (`@u8`, `@i32`, etc.), and `@boolean`.
> Non-scalar key types (objects, sequences) are not allowed.
> Key uniqueness is determined by the parsed key value per `r[key.equality]` in the parser spec,
> not by the typed interpretation — `"1"` and `"01"` are distinct keys even if both parse as integer 1.

## Named types

> r[schema.type.definition]
> Named types are defined inside the `schema` block. Use `@TypeName` to reference them.
> By convention, named types use PascalCase (e.g., `TlsConfig`, `UserProfile`).
> This is not enforced but aids readability and distinguishes user types from built-in types.
>
> ```styx
> TlsConfig @object{
>   cert @string
>   key @string
> }
>
> server @object{
>   tls @TlsConfig
> }
> ```

### Recursive types

> r[schema.type.recursive]
> Recursive types are allowed. A type may reference itself directly or indirectly.
>
> ```styx
> Node @object{
>   value @string
>   children @seq(@Node)
> }
> ```

### Flatten

> r[schema.flatten]
> `@flatten(@Type)` inlines fields from another type into the current object.
> The document is flat; deserialization reconstructs the nested structure.
>
> ```styx
> User @object{name @string, email @string}
>
> Admin @object{
>   user @flatten(@User)
>   permissions @seq(@string)
> }
> ```
>
> Document: `name Alice, email alice@example.com, permissions (read write)`
>
> r[schema.flatten.constraints]
> The argument to `@flatten` MUST be a named object type or a type alias to an object.
> Flattening unions or primitives is not allowed.
> If flattened fields conflict with explicitly declared fields in the same object, validation MUST fail.
> Multiple `@flatten` entries are allowed; their fields MUST NOT overlap with each other or with explicit fields.

### Enums

> r[schema.enum]
> `@enum{...}` defines valid variant names and their payloads.
> Unlike other composite types that use sequence payloads (`@union(...)`, `@map(...)`),
> `@enum` uses an object payload because variants have names.
>
> ```styx
> status @enum{
>   ok
>   pending
>   err @object{message @string}
> }
> ```
>
> Values use the tag syntax: `@ok`, `@pending`, `@err{message "timeout"}`.

## Validation

> r[schema.validation]
> Schema validation checks that a document conforms to a schema.
> Validation produces a list of errors; an empty list means the document is valid.
>
> r[schema.validation.errors]
> Validation errors MUST include:
> - The path to the invalid value (e.g., `server.port`)
> - The expected constraint (e.g., `@u16`)
> - The actual value or its type
>
> Common error conditions:
> - **Type mismatch**: value doesn't match the expected type (e.g., `"abc"` for `@u16`)
> - **Missing required field**: a non-optional field is absent
> - **Unknown field**: a field not in the schema (for closed objects)
> - **Literal mismatch**: value doesn't match a literal constraint
> - **Union failure**: value doesn't match any variant in a union

## Meta schema

The schema for STYX schema files.

> r[schema.meta.wildcard]
> In the meta schema, the unit value `@` is used as a wildcard meaning "any type reference" —
> that is, any tagged unit value like `@string` or `@MyType`.
> This is a semantic convention for the meta schema; it leverages the fact that `@` (unit) is a valid
> STYX value, and in schema context represents "match any type tag here".

```styx
meta {
  id https://styx-lang.org/schemas/schema
  version 2026-01-11
  description "Schema for STYX schema files"
}

schema {
  /// The root structure of a schema file.
  @ @object{
    /// Schema metadata (required).
    meta @Meta
    /// External schema imports (optional).
    imports @optional(@map(@string @string))
    /// Type definitions: @ for document root, strings for named types.
    schema @map(@union(@string @unit) @Schema)
  }

  /// Schema metadata.
  Meta @object{
    /// Unique identifier for the schema (URL recommended).
    id @string
    /// Schema version (date or semver).
    version @string
    /// Human-readable description.
    description @optional(@string)
  }

  /// A type constraint.
  Schema @enum{
    /// Literal value constraint (a scalar).
    literal @string
    /// Type reference (any tag with unit payload, e.g., @string, @MyType).
    type @
    /// Object schema: @object{field @type, @ @type}.
    object @object{@ @Schema}
    /// Sequence schema: @seq(@type).
    seq @seq(@Schema)
    /// Union: @union(@A @B ...).
    union @seq(@Schema)
    /// Optional: @optional(@T).
    optional @Schema
    /// Enum: @enum{variant, variant @object{...}}.
    enum @object{@ @union(@unit @object{@ @Schema})}
    /// Map: @map(@V) or @map(@K @V).
    map @seq(@Schema)
    /// Flatten: @flatten(@Type).
    flatten @
  }
}
```
