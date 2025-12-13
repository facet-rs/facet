# Format Suite Test Coverage Plan

This document tracks which tests from `facet-json` should be added to the format suite
to achieve comprehensive coverage of the **format abstraction layer**.

## Guiding Principles

1. **Test the abstraction, not the parser**: Unicode handling, escape sequences, and
   parser edge cases belong in format-specific tests (facet-json), not the suite.

2. **Avoid redundant coverage**: If feature A and feature B both work independently,
   we don't need to test every combination unless they interact.

3. **Focus on semantic behavior**: Test how attributes affect deserialization, type
   construction paths, error handling, and feature interactions.

## Current Coverage (65 test cases)

✅ Basic types (structs, sequences, enums, scalars, collections)
✅ Core attributes (rename, default, skip, alias, transparent)
✅ Smart pointers (Box, Arc, Rc, unsized variants, Arc<[T]>)
✅ Enum tagging (unit, complex, internally/adjacently tagged, untagged)
✅ Collections (maps, tuples, sets, arrays)
✅ Third-party types (uuid, ulid, camino, ordered_float, time, jiff, chrono)

## High Priority Additions (~15-20 tests)

### Proxy Variations (IMPORTANT)

- [x] `proxy_field_level` - Field-level `#[facet(proxy = ...)]` vs container-level
- [~] `proxy_field_overrides_container` - SKIPPED: container-level proxy changes entire serialization format, can't meaningfully combine with field-level proxies
- [x] `proxy_validation_error` - Proxy conversion error handling
- [ ] `proxy_with_enum` - Proxy on enum variants
- [ ] `proxy_with_transparent` - Interaction between proxy and transparent
- [ ] `proxy_with_option` - Proxy wrapping Option<T>

**Rationale**: Proxies are a core feature with complex precedence rules and error paths.

### Tuple Variations (IMPORTANT)

- [ ] `tuple_empty` - Empty tuple `()` as a field
- [ ] `tuple_single_element` - 1-element tuple `(T,)` (edge case in construction)
- [ ] `tuple_struct_variant` - Enum with tuple variant: `Variant(i32, String)`
- [ ] `tuple_newtype_variant` - Enum with newtype variant: `Variant(T)`

**Rationale**: Different tuple arities use different construction paths in Partial.

### Transparent Variations (IMPORTANT)

- [ ] `transparent_multilevel` - Transparent wrapping another transparent type
- [ ] `transparent_option` - Transparent wrapping Option<T>
- [ ] `transparent_nonzero` - Transparent wrapping NonZero types
- [ ] `repr_transparent_behavior` - Verify repr(transparent) implications

**Rationale**: Tests attribute composition and repr interaction.

### Flatten Variations (IMPORTANT)

- [ ] `flatten_optional_some` - Flattened field is Option<T> with Some
- [ ] `flatten_optional_none` - Flattened field is Option<T> with None
- [ ] `flatten_overlapping_fields_error` - Two flattened structs with same field name

**Rationale**: Tests flatten with Option (common pattern) and error cases.

### Untagged Enum Variations (IMPORTANT)

- [ ] `untagged_with_null` - Untagged enum with unit variant matching null
- [ ] `untagged_newtype_variant` - Discrimination with newtype variants
- [ ] `untagged_as_field` - Untagged enum as struct field (nesting)

**Rationale**: Tests variant discrimination logic and Option/null interaction.

### Error Cases (semantic level)

- [ ] `error_type_mismatch_string_to_int` - Semantic type error (not parse error)
- [ ] `error_type_mismatch_object_to_array` - Structure mismatch
- [ ] `error_missing_required_field` - Non-optional field missing
- [ ] `deny_unknown_fields_behavior` - Verify rejection works correctly

**Rationale**: Tests error reporting at the format layer, not parser errors.

### Attribute Precedence

- [ ] `rename_vs_alias_precedence` - When both are present, rename wins
- [ ] `rename_all_variations` - kebab-case, SCREAMING_SNAKE_CASE coverage

**Rationale**: Tests attribute interaction and precedence rules.

## Explicitly NOT Adding (parser concerns)

❌ Unicode field names (emoji, special characters) - parser string handling
❌ Numeric field names - parser concern
❌ Empty string rename - edge case, not valuable
❌ Invalid UTF-8 handling - parser concern
❌ Invalid surrogate sequences - parser concern
❌ Control character escaping - parser concern
❌ EOF at various positions - parser concern
❌ Long input before/after error - parser concern
❌ Extensive escape sequence testing - parser concern
❌ Borrowed &str edge cases - lifetime/parser concern

## Explicitly NOT Adding (redundant coverage)

❌ Rename with nested structures - already tested separately
❌ u128 in flatten - just tests u128 works (already covered)
❌ Tuples as map keys - BTreeMap ordering, not format layer
❌ Nested flatten beyond 2 levels - diminishing returns
❌ Every combination of transparent + other features - test key interactions only

## Implementation Strategy

1. Add high-priority tests first (proxies, tuples, transparent)
2. For each test:
   - Add method to `FormatSuite` trait
   - Add `CaseDescriptor` constant
   - Add fixture type if needed
   - Implement for JsonSlice and XmlSlice
   - Verify roundtrip works

3. Track progress by marking [ ] -> [x] in this document

## Success Metrics

- Format suite covers ~80-85 test cases (up from 65)
- All high-priority attribute interactions tested
- All type construction edge cases covered
- Error semantics well-tested
- No redundant parser-level tests

Last updated: 2025-12-13
