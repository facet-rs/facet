---
weight = 4
slug = "deserializer-spec"
---

# Deserializer

The deserializer converts document trees into typed application values. It interprets
scalars based on target types (e.g., Rust structs) and validates structural constraints.

The deserializer does not require a schema — it works directly with target types.
Schemas (Part 2) are a separate, optional validation layer.

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

See Part 2 for the grammars of integer types, float types, `@duration`, etc.

## Object deserialization

Objects are deserialized based on the target type (e.g., a Rust struct).

> r[deser.object.fields]
> Each key in the document must match a field defined in the target type.
> Required fields MUST be present; optional fields MAY be absent.

> r[deser.object.unknown]
> Keys not defined in the target type are errors by default. Implementations MAY
> provide a lenient mode that ignores unknown keys.

## Optional fields

Optional fields interact with absence and unit.

> r[deser.optional.absent]
> An optional field (e.g., `Option<T>` in Rust) that is absent from the document is valid.
> The application receives `None` or equivalent for that field.

> r[deser.optional.unit]
> An optional field explicitly set to unit (`key @`) is distinct from absence.
> Both are valid for optional fields, but applications may distinguish them.
> 
> ```styx
> // Target type has: timeout: Option<Duration>
> { }                    // absent — None
> { timeout @ }          // present but explicitly empty
> { timeout 30s }        // present with value
> ```

## Sequence deserialization

Sequences are deserialized element-by-element.

> r[deser.sequence]
> Each element is deserialized according to the target element type (e.g., `Vec<T>` → each element as `T`).
> Empty sequences are valid.

## Map deserialization

Maps are objects with uniform value types.

> r[deser.map]
> Each value is deserialized according to the target value type (e.g., `HashMap<String, T>` → each value as `T`).
> Keys are always strings. Empty maps are valid.

## Flatten

Flattening merges fields from a referenced type into a single, flat key-space in the document,
while maintaining a nested structure in the deserialized application type.

> r[deser.flatten]
> A flattened field (e.g., `#[facet(flatten)]` in Rust) instructs the deserializer to collect
> keys from the document that belong to the nested type and use them to construct
> an instance of that type. The document itself is flat; the fields appear at the
> same level as the parent's other fields.

> r[deser.flatten.routing]
> The deserializer routes keys from the flat document to the appropriate nested structure
> based on the target type. When multiple flattened fields are present, keys are
> matched to the type where they are defined.

**Example (non-normative)**

This example shows how a flat document is deserialized into a nested structure.

1.  **Target types** (Rust):

    ```rust
    #[derive(Facet)]
    struct User {
        name: String,
        email: String,
    }

    #[derive(Facet)]
    struct Admin {
        #[facet(flatten)]
        user: User,
        permissions: Vec<String>,
    }
    ```

2.  **STYX Document** (flat):

    ```styx
    name "Alice"
    email "alice@example.com"
    permissions (read write admin)
    ```

3.  **Deserialization**:
    - The deserializer targets the `Admin` type.
    - It sees `user` is flattened, so it collects `name` and `email` to construct a `User`.
    - It deserializes `permissions` into the `Vec<String>`.
    - The `User` is assigned to `admin.user`.

4.  **Result** (as JSON):

    ```json
    {
      "user": { "name": "Alice", "email": "alice@example.com" },
      "permissions": ["read", "write", "admin"]
    }
    ```

## Enum deserialization

Enums use tag syntax. The tag names the variant; the payload is the variant's data.

> r[enum.representation]
> An enum value is a tag whose identifier matches a variant name.
> 
> ```compare
> /// json
> {"ok": null}
> /// styx
> @ok
> ```
> 
> ```compare
> /// json
> {"err": {"message": "nope", "retry_in": "5s"}}
> /// styx
> @err{message "nope", retry_in 5s}
> ```

> r[enum.unit-variant]
> Unit variants use implicit unit: `@ok` means `@ok@`.

> r[enum.payload]
> Variant payloads may be unit (`@`), object (`{}`), sequence (`()`), or quoted scalar.
> 
> ```styx
> @ok                              // unit variant
> @err{message "timeout"}          // struct variant
> @values(1 2 3)                   // tuple variant
> @message"hello"                  // newtype variant (quoted scalar)
> ```

The deserializer validates that the tag matches a defined variant and the payload matches the expected shape.
