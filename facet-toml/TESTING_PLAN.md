# facet-toml Testing Plan

This document tracks the plan to enable comprehensive TOML serialization/deserialization testing.

## Goal

Run the toml-rs compatibility tests and TOML compliance fixtures against facet-toml, using `facet_value::Value` and the `value!{}` macro for expected values.

## Current Status

### Completed

1. **TOML → Value deserialization implemented** ✅
   - facet-reflect has `Def::DynamicValue` support for:
     - ✅ Scalars (via `set()`)
     - ✅ Arrays (via `begin_list()`, `begin_list_item()`)
     - ✅ Objects (via `begin_map()`, `begin_object_entry()`)
   - facet-toml detects `Def::DynamicValue` and routes to dynamic value handling
   - 9 basic tests passing (scalars, arrays, inline tables, nested objects)

### Known Issues

1. **Empty table `[empty]` causes memory safety crash**
   - When a table header has no key-value pairs
   - Likely missing initialization of the nested object value

2. **Array of tables `[[users]]` result differs from expected**
   - The root object wrapper isn't being created correctly
   - Need to investigate frame management

### Deferred

1. **Datetime support**
   - TOML has native datetime types
   - `facet_value::Value` doesn't have a Datetime variant
   - Will deserialize datetimes to `time::DateTime` or similar (out of scope for Value)
   - Skip datetime-related tests for now

### Reference Test Files

Located in `tests-reference/toml-rs-compat/`:

| Directory | Files | Purpose |
|-----------|-------|---------|
| `serde/` | 10 | Serde compatibility (de_enum, de_errors, de_key, general, ser_enum, ser_key, ser_tables_last, ser_to_string, ser_to_string_pretty, spanned) |
| `compliance/` | 3 | TOML spec compliance |
| `testsuite/` | 4 | toml-rs test suite |
| root | 6 | Encoder/decoder tests |
| `fixtures/` | 64 TOML files | Test fixtures for invalid TOML |

## Implementation Plan

### Phase 1: Enable TOML → Value Deserialization

1. **Add object support to facet-reflect's DynamicValue handling**
   - Implement `begin_object()` method on `Partial`
   - Implement `begin_object_entry(key: &str)` method on `Partial`
   - Handle `end()` for object entries

2. **Add DynamicValue support to facet-toml**
   - In `deserialize_value()`, detect `Def::DynamicValue`
   - Create `deserialize_dynamic_value()` that handles:
     - Scalars → `partial.set()`
     - Arrays → `partial.begin_list()` + recurse
     - Inline tables → `partial.begin_object()` + recurse

### Phase 2: Port Tests Incrementally

Tests will be ported to use:
- `facet_toml::from_str::<Value>()`
- `value!{}` macro for expected values
- `facet-assert` for comparisons

Priority order:
1. Basic scalar tests (integers, floats, booleans, strings)
2. Array tests
3. Table/object tests
4. Nested structure tests
5. Enum tests
6. Error message tests
7. Serialization tests

### Phase 3: Datetime Support (Future)

- Add datetime types that work with `time` crate
- Implement deserialization of TOML datetime → `time::DateTime`
- Enable datetime-related tests

## Test Harness

Simple harness pattern:

```rust
#[test]
fn test_basic_table() {
    let toml = r#"
        name = "Alice"
        age = 30
    "#;

    let result: Value = facet_toml::from_str(toml).unwrap();
    let expected = value!({
        "name": "Alice",
        "age": 30
    });

    assert_eq!(result, expected);
}
```

## Progress Tracking

- [x] Phase 1.1: Add `begin_object()` to facet-reflect
- [x] Phase 1.2: Add `begin_object_entry()` to facet-reflect
- [x] Phase 1.3: Add DynamicValue detection to facet-toml
- [x] Phase 1.4: Implement `deserialize_dynamic_value()` in facet-toml
- [x] Phase 1.5: Basic smoke test: TOML → Value works
- [ ] Phase 1.6: Fix empty table crash
- [ ] Phase 1.7: Fix array of tables handling
- [ ] Phase 2.1: Port scalar tests
- [ ] Phase 2.2: Port array tests
- [ ] Phase 2.3: Port table tests
- [ ] Phase 2.4: Port nested structure tests
- [ ] Phase 2.5: Port enum tests
- [ ] Phase 2.6: Port error message tests
- [ ] Phase 2.7: Port serialization tests
- [ ] Phase 3: Datetime support
