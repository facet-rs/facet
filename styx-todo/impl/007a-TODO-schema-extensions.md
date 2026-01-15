# Phase 007a: Schema Extensions

Extended schema capabilities: constraints, defaults, deprecation, and better documentation.

## Current State

The basic schema validation is working:
- Type references (`@string`, `@int`, `@bool`, etc.)
- Objects with required/optional fields
- Sequences, unions, maps, enums
- Named type definitions and references

## Proposed Extensions

### 1. Type Constraints

Type-specific constraint objects as payloads on built-in types:

```styx
schema {
  @ @object{
    // Simple type reference (no constraints)
    name @string
    
    // Type with constraints (object payload)
    email @string{minLen 5, maxLen 100, pattern "^[^@]+@[^@]+$"}
    age @int{min 0, max 150}
    rating @float{min 0.0, max 5.0}
  }
}
```

**Constraint types per built-in:**

```styx
StringConstraints @object{
  minLen @optional(@int)
  maxLen @optional(@int)
  pattern @optional(@string)  // regex
}

IntConstraints @object{
  min @optional(@int)
  max @optional(@int)
}

FloatConstraints @object{
  min @optional(@float)
  max @optional(@float)
}
```

**Design question:** How to represent in meta-schema?

Currently `@string` falls through to `Schema::Type { name: Some("string") }`. If we want
`@string` vs `@string{...}` to both work, we have two options:

**Option A:** Built-in types become explicit enum variants with optional constraint payloads:
```rust
enum Schema {
    String(Option<StringConstraints>),  // @string or @string{...}
    Int(Option<IntConstraints>),        // @int or @int{...}
    // ...
    Type { name: Option<String> },      // fallback for user types
}
```

**Option B:** Keep type refs as-is, add separate constrained type:
```styx
email @constrained(@string {minLen 5})  // ugly
```

Option A is cleaner - built-in types become first-class.

### 2. Default Values

Wrapper type that specifies a default when field is missing:

```styx
schema {
  @ @object{
    port @default(8080 @int)
    timeout @optional(@default(30 @int))
  }
}
```

Syntax: `@default(value @type)` - sequence with default value then inner type.

In meta-schema:
```styx
Schema @enum{
  // ...
  default @seq(@any @Schema)  // (defaultValue innerType)
}
```

### 3. Deprecated Fields

Wrapper type for marking fields as deprecated (validation warns but doesn't fail):

```styx
schema {
  @ @object{
    newField @string
    oldField @deprecated("use newField instead" @string)
  }
}
```

Syntax: `@deprecated("reason" @type)` - sequence with message then inner type.

In meta-schema:
```styx
Schema @enum{
  // ...
  deprecated @seq(@string @Schema)  // ("reason" innerType)
}
```

### 4. Documentation Preservation

Currently doc comments (`/// ...`) on schema entries are lost during deserialization.
We should preserve them for:
- LSP hover documentation
- Generated documentation
- Schema introspection

Options:
- Add `doc` field to schema types
- Use attributes (`#[doc("...")]` style) - but styx doesn't have this syntax yet

## Tree Structure Investigation (RESOLVED)

Using `styx tree` CLI command, we verified all syntax patterns:

### Basic type reference
```styx
name @string
```
```
Tagged { tag: "string", payload: None }
```

### Type with constraints
```styx
name @string{minLen 1}
```
```
Tagged { tag: "string", payload: Some(Object { minLen: "1" }) }
```

### Default wrapper
```styx
port @default(8080 @int)
```
```
Tagged { tag: "default", payload: Some(Sequence ["8080", Tagged @int]) }
```

### Deprecated wrapper
```styx
oldField @deprecated("use newField" @string)
```
```
Tagged { tag: "deprecated", payload: Some(Sequence ["use newField", Tagged @string]) }
```

**All syntax patterns work naturally!** The tag payload can be:
- None (simple type ref)
- Object (constraints)
- Sequence (wrapper with args)

## Implementation Order

1. ~~**CLI tool** - `styx tree` / `styx canonicalize` to visualize parsing~~ ✓ DONE
2. ~~**Test constraint syntax** - verify `@type{...}` parses as expected~~ ✓ DONE
3. **Update meta-schema** - add constraint types, default, deprecated
4. **Update Rust types** - match new meta-schema
5. **Update validator** - handle constraints, defaults, deprecation warnings

## Open Questions

1. Should constraints be validated at schema-load time or document-validation time?
2. How to handle constraint inheritance with type aliases?
3. Should `@default` apply the default during validation or just document it?
4. Pattern syntax - PCRE? Rust regex? Something simpler?
