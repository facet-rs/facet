---
weight = 4
slug = "deserializer-spec"
---

# Part 3: Deserializer

The deserializer converts document trees into typed application values. It interprets
scalars based on target types and validates structural constraints like enum representations.

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

Objects in the document are validated against object schemas.

> r[deser.object.fields]
> Each key in the document must match a field defined in the schema. Required
> fields (no `?` suffix) MUST be present; optional fields MAY be absent.

> r[deser.object.unknown]
> Keys not defined in the schema are errors by default. Implementations MAY
> provide a lenient mode that ignores unknown keys.

## Optional fields

Optional fields interact with absence and unit.

> r[deser.optional.absent]
> An optional field (`key? @type`) that is absent from the document is valid.
> The application receives no value for that field.

> r[deser.optional.unit]
> An optional field explicitly set to unit (`key @`) is distinct from absence.
> Both are valid for optional fields, but applications may distinguish them.
> 
> ```styx
> // Schema: timeout? @duration
> { }                    // absent — no timeout specified
> { timeout @ }          // present but explicitly empty
> { timeout 30s }        // present with value
> ```

## Sequence deserialization

Sequences are validated element-by-element.

> r[deser.sequence]
> A sequence schema `(@type)` validates that every element matches `@type`.
> Empty sequences are valid.

## Map deserialization

Maps are objects with uniform value types.

> r[deser.map]
> A map schema `@map(@type)` validates that all values match `@type`.
> Keys are always strings. Empty maps are valid.

## Flatten

Flattening merges fields from a referenced type into a single, flat key-space in the document,
while maintaining a nested structure in the deserialized application type.

> r[deser.flatten]
> A flattened field `key @flatten(@Type)` instructs the deserializer to collect
> keys from the document that are defined in `@Type` and use them to construct
> an instance of `Type`. This instance is then assigned to the field `key` in the
> parent object. The document itself does not contain a `key` object; the fields
> are expected to be at the same level as the parent's other fields.

> r[deser.flatten.routing]
> The deserializer routes keys from the flat document to the appropriate nested structure
> based on the schema. When multiple `@flatten` directives are present, keys are
> matched to the type where they are defined.

**Example (non-normative)**

This example shows how a document with a flat structure is deserialized into a nested
`Admin` object containing a `User` object.

1.  **Schema Definition**:
    The `Admin` schema flattens the `User` schema into its `user` field.

    ```styx
    // schema.styx
    User {
      name @string
      email @string
    }

    Admin {
      // Fields from User (name, email) are expected to be flat
      // in the document, but will be collected into a User
      // object assigned to this 'user' field.
      user @flatten(@User)
      
      // Regular field belonging to Admin
      permissions (@string)
    }
    ```

2.  **STYX Document**:
    The document is flat. There is no `user` object. The keys `name` and `email`
    are peers with `permissions`.

    ```styx
    // config.styx
    name "Alice"
    email "alice@example.com"
    permissions (read write admin)
    ```

3.  **Deserialization Logic**:
    - The deserializer targets the `Admin` type.
    - It sees the `user @flatten(@User)` field in the schema.
    - It consumes `name` and `email` from the document, recognizes them as fields of `User`, and uses them to construct a `User` object.
    - It consumes `permissions` and assigns it to the `permissions` field of `Admin`.
    - The constructed `User` object is assigned to the `user` field of the `Admin` object.

4.  **Resulting Application Structure (represented as JSON)**:
    The final in-memory object has a nested structure, even though the source document was flat.

    ```json
    {
      "user": {
        "name": "Alice",
        "email": "alice@example.com"
      },
      "permissions": ["read", "write", "admin"]
    }
    ```

## Enum deserialization

Enums are represented as objects with exactly one key (the variant tag) whose value
is the variant payload.

> r[enum.representation]
> When deserializing into an enum type, the value MUST be an object with exactly one key.
> 
> ```compare
> /// json
> {"ok": null}
> /// styx
> { ok @ }
> ```
> 
> ```compare
> /// json
> {"err": {"message": "nope", "retry_in": "5s"}}
> /// styx
> { err { message "nope", retry_in 5s } }
> ```

The parser expands dotted paths into nested objects, which provides a convenient
syntax for enums:

```compare
/// styx
status.ok
/// styx
status { ok @ }
```

```compare
/// styx
status.err { message "nope" }
/// styx
status { err { message "nope" } }
```

Dotted paths expand to nested objects (see `r[object.key.dotted.expansion]`).
Keys without values get implicit unit (see `r[object.entry.implicit-unit]`).

> r[enum.singleton]
> The deserializer MUST reject enum values that are not single-key objects.
> The parser cannot enforce this — it produces the same object structure regardless
> of target type.

> r[enum.unit]
> For unit variants, the payload is the unit value `@`.
> 
> ```styx
> // All equivalent for unit variants:
> status.ok        // implicit @
> status.ok @      // explicit @
> ```

A variant payload may be unit (`@`), a scalar, an object, or a sequence:

```styx
result.ok
```

```styx
result.err message="timeout" retry_in=5s
```

The deserializer validates:
- The value is a single-key object
- The key matches a valid variant name
- The payload matches the expected variant shape
