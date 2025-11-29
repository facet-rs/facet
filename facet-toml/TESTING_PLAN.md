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

1. **Implicit table merging not yet implemented**
   - TOML allows `[a]` followed by `[a.b.c]` which should navigate into existing `a`
   - Currently creates duplicate keys instead of merging
   - Need `begin_object_entry` to support "get or create" semantics

### Recently Fixed

1. ~~Empty table `[empty]` causes memory safety crash~~ ✅
2. ~~Array of tables `[[users]]` result differs from expected~~ ✅
3. **Datetime support added** ✅
   - Added `VDateTime` type to `facet_value::Value`
   - Supports all TOML datetime variants (offset, local, date, time)

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

## Phase 4: Typed Struct Codegen from Fixtures

**Key insight**: Deserializing to `Value` and deserializing to a typed struct are two different codepaths — and the typed path is the harder one! We should run every test twice.

### The Problem

Currently we only test: `TOML → Value` (dynamic path)

But the important path is: `TOML → typed struct` (via `#[derive(Facet)]`)

### The Solution: Codegen from Tagged JSON

The toml-test fixtures include tagged JSON that specifies exact types:

```json
{"type": "string", "value": "hello"}   →  field: String
{"type": "integer", "value": "42"}     →  field: i64
{"type": "float", "value": "3.14"}     →  field: f64
{"type": "bool", "value": "true"}      →  field: bool
{"type": "datetime", "value": "..."}   →  field: VDateTime
{"type": "array", "value": [...]}      →  field: Vec<T>
nested objects                         →  nested structs
```

A `build.rs` can generate Rust structs from each fixture's expected JSON:

```rust
// Generated from valid/table/nested.toml
#[derive(Facet, Debug, PartialEq)]
struct ValidTableNested {
    table: ValidTableNestedTable,
}

#[derive(Facet, Debug, PartialEq)]
struct ValidTableNestedTable {
    nested: ValidTableNestedTableNested,
}
// ...
```

### Test Strategy

Each valid fixture runs **two tests**:

1. **Dynamic path**: `facet_toml::from_str::<Value>(toml)` — compare against untagged JSON
2. **Typed path**: `facet_toml::from_str::<GeneratedStruct>(toml)` — compare against constructed expected value

### Implementation Plan

**Option A: Separate crate (`facet-toml-suite`)**
- Keeps `facet-toml` clean with no build.rs
- All test codegen and running lives in the suite crate

**Option B: Committed codegen (preferred)**
- Generate code via a script/tool (not build.rs)
- Commit the generated `.rs` files to the repo
- Benefits:
  - Generated code is versioned and diffable
  - Fixture changes show up in PRs
  - No build-time codegen overhead
  - Transparent — you can read the generated tests directly

Implementation:
1. Create a generator tool (could be a bin in facet-toml or standalone):
   - Iterates over `toml_test_data::valid()` fixtures
   - Parses the tagged JSON expected value
   - Generates a Rust struct hierarchy matching the shape
   - Generates test functions for both Value and typed paths
   - Writes to `tests/generated/` (committed to repo)

2. Test file structure:
   ```
   facet-toml/tests/
   ├── compliance.rs           # Dynamic Value tests (existing)
   ├── typed_compliance.rs     # mod generated; runs typed tests
   └── generated/
       ├── mod.rs
       ├── valid_string.rs     # Generated structs + tests for string fixtures
       ├── valid_integer.rs
       ├── valid_table.rs
       └── ...
   ```

3. Handle edge cases:
   - Rust identifier sanitization (kebab-case → snake_case, reserved words)
   - Homogeneous vs heterogeneous arrays (latter needs `Value` fallback)
   - Optional fields (if fixture has them)

## Progress Tracking

- [x] Phase 1.1: Add `begin_object()` to facet-reflect
- [x] Phase 1.2: Add `begin_object_entry()` to facet-reflect
- [x] Phase 1.3: Add DynamicValue detection to facet-toml
- [x] Phase 1.4: Implement `deserialize_dynamic_value()` in facet-toml
- [x] Phase 1.5: Basic smoke test: TOML → Value works
- [x] Phase 1.6: Fix empty table crash
- [x] Phase 1.7: Fix array of tables handling
- [ ] Phase 2.1: Port scalar tests
- [ ] Phase 2.2: Port array tests
- [ ] Phase 2.3: Port table tests
- [ ] Phase 2.4: Port nested structure tests
- [ ] Phase 2.5: Port enum tests
- [ ] Phase 2.6: Port error message tests
- [ ] Phase 2.7: Port serialization tests
- [x] Phase 3: Datetime support (VDateTime in facet-value)
- [ ] Phase 4.1: Create build.rs codegen infrastructure
- [ ] Phase 4.2: Generate struct definitions from tagged JSON
- [ ] Phase 4.3: Generate typed test functions
- [ ] Phase 4.4: Run both Value and typed tests for each fixture
