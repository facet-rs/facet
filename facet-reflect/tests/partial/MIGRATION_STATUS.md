# Partial Test Migration Status

## Summary

This document tracks the migration of tests from the old `Wip` API to the new `Partial` API.

## Migration Changes Applied

### 1. Type Renaming
- `Wip` → `Partial`
- `Wip::alloc()` → `Partial::alloc()`

### 2. Method Renaming
- `put()` → `set()`
- `pop()` → `end()`
- `field_named()` → `begin_field()`
- `push()` → `begin_list_item()`
- `begin_pushback()` → `begin_list()`

### 3. Option Handling
- Old API: `push_some()` and implicit conversion from inner value
- New API: Explicit `set(Some(value))` or `set(None)`
- No special Option methods - just use the full Option value

### 4. Map Construction
- Must call `begin_map()` before `begin_insert()`
- Sequence: `begin_map()` → `begin_insert()` → `begin_key()` → `set(key)` → `end()` → `begin_value()` → `set(value)` → `end()`
- Note: `begin_map()` and `begin_insert()` don't push frames, they just change state

### 5. List Construction  
- Must call `begin_list()` before adding items
- Sequence: `begin_list()` → `begin_list_item()` → `set(value)` → `end()` → ...

## Test Migration Results

### ✅ Successfully Migrated (39/43 tests)
- **list_leak**: All 12 tests passing
- **option_leak**: All 6 tests passing  
- **put_vec_leak**: All 3 tests passing
- **struct_leak**: All 14 tests passing
- **map_leak**: 4/8 tests passing (tests 1, 2, 7, 8)

### ❌ Failed Tests (4/43 tests)
- **map_leak tests 3-6**: Use-after-free errors in Partial drop implementation when map insertions are partially completed

## Known Issues

1. **Memory Safety Bug**: The Partial drop implementation has a use-after-free bug when deallocating partially initialized map insertions. This affects tests where:
   - Key is set but not ended
   - Key is set and ended but value is not set
   - Value is set but not ended

2. **API Inconsistency**: Some operations push frames (e.g., `begin_key()`, `begin_value()`) while others don't (e.g., `begin_map()`, `begin_insert()`). This makes it difficult to predict how many `end()` calls are needed.

## Recommendations

1. Fix the use-after-free bug in Partial's drop implementation for partially initialized maps
2. Consider adding explicit Option support methods for clarity (e.g., `set_some()`, `set_none()`)
3. Document frame pushing behavior more clearly in the API
4. Add more comprehensive tests for partial initialization scenarios