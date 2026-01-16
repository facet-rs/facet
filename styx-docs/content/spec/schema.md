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
> | `@bool` | `@true` or `@false` |
> | `@int` | any integer |
> | `@float` | any floating point number |
> | `@unit` | the unit value `@` |
> | `@any` | any value |
>
> Composite type constructors (`@optional`, `@union`, `@map`, `@enum`, `@flatten`) are described in their own sections.
> Modifiers (`@default`, `@deprecated`) are described in their own sections.

### Type constraints

> r[schema.constraints]
> Scalar types can have constraints specified in an object payload.

> r[schema.constraints.string]
> `@string` accepts optional constraints:
>
> | Constraint | Description |
> |------------|-------------|
> | `minLen` | minimum length (inclusive) |
> | `maxLen` | maximum length (inclusive) |
> | `pattern` | regex pattern the string must match |
>
> ```styx
> name @string{minLen 1, maxLen 100}
> slug @string{pattern "^[a-z0-9-]+$"}
> ```

> r[schema.constraints.int]
> `@int` accepts optional constraints:
>
> | Constraint | Description |
> |------------|-------------|
> | `min` | minimum value (inclusive) |
> | `max` | maximum value (inclusive) |
>
> ```styx
> port @int{min 1, max 65535}
> age @int{min 0}
> ```

> r[schema.constraints.float]
> `@float` accepts optional constraints:
>
> | Constraint | Description |
> |------------|-------------|
> | `min` | minimum value (inclusive) |
> | `max` | maximum value (inclusive) |
>
> ```styx
> ratio @float{min 0.0, max 1.0}
> temperature @float{min -273.15}
> ```

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

### Default values

> r[schema.default]
> `@default(value @T)` specifies a default value for optional fields.
> If the field is absent, validation treats it as if the default value were present.
> The first element is the default value, the second is the type constraint.
>
> ```styx
> server @object{
>   host @string
>   port @default(8080 @int{min 1, max 65535})
>   timeout @default(30s @duration)
>   enabled @default(@true @bool)
> }
> ```
>
> Note: `@default` implies the field is optional. Using `@optional(@default(...))` is redundant.

### Deprecation

> r[schema.deprecated]
> `@deprecated("reason" @T)` marks a field as deprecated.
> Validation produces a warning (not an error) when deprecated fields are used.
> The first element is the deprecation message, the second is the type constraint.
>
> ```styx
> server @object{
>   host @string
>   // Old field, use 'host' instead
>   hostname @deprecated("use 'host' instead" @string)
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
>   port @int
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
> id @union(@int @string)           // integer or string
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
>   port @int
> })
> ids @seq(@union(@int @string))    // sequence of ids
> ```

### Maps

> r[schema.map]
> `@map(@K @V)` matches an object where all keys match `@K` and all values match `@V`.
> `@map(@V)` is shorthand for `@map(@string @V)`.
>
> ```styx
> env @map(@string)              // string → string
> ports @map(@int)               // string → int
> ```
>
> r[schema.map.keys]
> Valid key types are scalar types that can be parsed from the key's text representation:
> `@string`, `@int`, and `@bool`.
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
> Validation produces a list of errors and warnings; an empty error list means the document is valid.
>
> r[schema.validation.errors]
> Validation errors MUST include:
> - The path to the invalid value (e.g., `server.port`)
> - The expected constraint (e.g., `@int{min 1, max 65535}`)
> - The actual value or its type
>
> Common error conditions:
> - **Type mismatch**: value doesn't match the expected type (e.g., `"abc"` for `@int`)
> - **Constraint violation**: value doesn't meet constraints (e.g., `0` for `@int{min 1}`)
> - **Missing required field**: a non-optional field is absent
> - **Unknown field**: a field not in the schema (for closed objects)
> - **Literal mismatch**: value doesn't match a literal constraint
> - **Union failure**: value doesn't match any variant in a union
>
> r[schema.validation.warnings]
> Validation warnings are non-fatal issues:
> - **Deprecated field**: a field marked with `@deprecated` is present

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
  version 2026-01-16
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

  /// String type constraints.
  StringConstraints @object{
    minLen @optional(@int{min 0})
    maxLen @optional(@int{min 0})
    pattern @optional(@string)
  }

  /// Integer type constraints.
  IntConstraints @object{
    min @optional(@int)
    max @optional(@int)
  }

  /// Float type constraints.
  FloatConstraints @object{
    min @optional(@float)
    max @optional(@float)
  }

  /// A type constraint.
  Schema @enum{
    /// String type with optional constraints.
    string @optional(@StringConstraints)
    /// Integer type with optional constraints.
    int @optional(@IntConstraints)
    /// Float type with optional constraints.
    float @optional(@FloatConstraints)
    /// Boolean type.
    bool
    /// Unit type (the value must be @).
    unit
    /// Any type (accepts any value).
    any
    /// Object schema: @object{field @type, @ @type}.
    object @object{@ @Schema}
    /// Sequence schema: @seq(@type).
    seq(@Schema)
    /// Union: @union(@A @B ...).
    union @seq(@Schema)
    /// Optional: @optional(@T).
    optional(@Schema)
    /// Enum: @enum{variant, variant @object{...}}.
    enum @object{@ @Schema}
    /// Map: @map(@V) or @map(@K @V).
    map @seq(@Schema)
    /// Flatten: @flatten(@Type).
    flatten @
    /// Default value: @default(value @type).
    default @seq(@union(@string @Schema))
    /// Deprecated: @deprecated("reason" @type).
    deprecated @seq(@union(@string @Schema))
    /// Type reference (user-defined type).
    type @
  }
}
```
