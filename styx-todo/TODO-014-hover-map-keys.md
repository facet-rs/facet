# TODO-014: LSP Hover on Map Keys

## Status
TODO

## Description
Hovering on a map key (e.g., `captain` in `hints { captain { ... } }`) should show type information.

## Current Behavior
No hover information is shown for map keys.

## Expected Behavior
When hovering on `captain` in a `@map(@string @Hint)`:
```
`@ > hints > captain`

**Type:** `@Hint`

Defined at schema.styx:12:5
```

## Root Cause
The hover logic in `find_field_path_at_offset()` only handles object fields, not map entries. Map keys are treated as dynamic values, not schema-defined fields.

## Implementation

### 1. Detect map context
When building the field path, detect when we're inside a map:
- Parent is an object field with `@map(K V)` type in schema
- Current position is on a key

### 2. Return map value type
For `@map(@string @Hint)`, when hovering on any key:
- Breadcrumb: `@ > hints > <key>`
- Type: `@Hint` (the value type)

### 3. Handle nested maps
For `@map(@string @map(@string @Foo))`:
- First level key → type is `@map(@string @Foo)`
- Second level key → type is `@Foo`

## Files to Modify
- `crates/styx-lsp/src/server.rs`: `find_field_path_at_offset()`, `get_field_info_from_schema()`

## Notes
- The key type (`@string`) is implicit from the map definition
- Doc comments on the map field apply to all keys
