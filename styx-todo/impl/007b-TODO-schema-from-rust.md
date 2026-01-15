# 007b: Generate Styx Schema from Rust Types

## Goal

Generate `.styx` schema files from Rust types using facet reflection, similar to how `facet-typescript` generates TypeScript definitions.

## Motivation

Currently we maintain two sources of truth:
1. `crates/styx-schema/src/types.rs` - Rust types
2. `crates/styx-schema/schema/meta.styx` - Hand-written schema

These can drift apart. The Rust types should be the canonical source, and the schema should be derived.

## Design

### New crate: `styx-schema-gen` or add to `styx-schema`

```rust
use facet::Facet;

/// Generate a Styx schema string from a Facet type
pub fn schema_from_type<T: Facet>() -> String {
    // Use facet reflection to walk the type
    // Generate styx syntax
}
```

### Mapping Rust → Styx

| Rust | Styx |
|------|------|
| `String` | `@string` |
| `i32`, `i64`, `i128` | `@int` |
| `f32`, `f64` | `@float` |
| `bool` | `@bool` |
| `()` | `@unit` |
| `Option<T>` | `@optional(@T)` |
| `Vec<T>` | `@seq(@T)` |
| `HashMap<K, V>` | `@map(@K @V)` |
| `Box<T>` | (unwrap, same as T) |
| struct with fields | `@object{ field @Type ... }` |
| enum | `@enum{ variant @Type ... }` |

### Constraints via attributes

```rust
#[derive(Facet)]
struct User {
    #[facet(styx(min_len = 1, max_len = 100))]
    name: String,
    
    #[facet(styx(min = 0, max = 150))]
    age: i32,
}
```

Generates:
```styx
User @object{
  name @string{minLen 1 maxLen 100}
  age @int{min 0 max 150}
}
```

### Usage

```rust
// In build.rs or a CLI tool
let schema = styx_schema_gen::schema_from_type::<SchemaFile>();
std::fs::write("schema/meta.styx", schema)?;
```

## Implementation Steps

1. Create schema generation module
2. Handle basic scalars (string, int, float, bool, unit)
3. Handle Option → @optional
4. Handle Vec → @seq
5. Handle HashMap → @map
6. Handle structs → @object
7. Handle enums → @enum
8. Handle Box (unwrap)
9. Handle recursive types (type references)
10. Add constraint attributes
11. Generate meta.styx from types.rs
12. Remove hand-written meta.styx

## Open Questions

- How to handle `#[facet(other)]` fallback variants?
- How to specify the root type (`@`)?
- Should we support custom type names via attributes?
- How to handle `#[facet(rename_all)]` and `#[facet(rename)]`?

## Related

- `facet-typescript` - similar concept for TypeScript
- `007-TODO-schema.md` - schema validation
- `007a-TODO-schema-extensions.md` - constraints and wrappers
