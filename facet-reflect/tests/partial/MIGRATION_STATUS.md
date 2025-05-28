# Partial Test Migration Status

## Summary

This document tracks the migration of tests from the old `Wip` API to the new `Partial` API.

## Tests Still Needing Migration

### Files still using old Wip API (3 files remaining):
- **invariant.rs**: Tests for variance behavior
- **map.rs**: Map construction tests
- **misc.rs**: Miscellaneous tests including enums, lists, and edge cases

### Files already migrated:
- **arc.rs**: All 4 tests passing ✅
- **array_building.rs**: Array construction tests ✅
- **list_leak.rs**: List memory leak tests ✅
- **map_leak.rs**: Map memory leak tests (all 8 tests now passing) ✅
- **no_uninit.rs**: Tests for uninitialized value handling ✅
- **option_leak.rs**: Option memory leak tests ✅
- **put_vec_leak.rs**: Vec memory leak tests ✅
- **struct_leak.rs**: Struct memory leak tests ✅
- **variance.rs**: Variance tests ✅

## Key API Changes

1. **Type renaming**: `Wip` → `Partial`
2. **Method renaming**:
   - `put()` → `set()`
   - `pop()` → `end()`
   - `field_named()` → `begin_field()`
   - `push()` → `begin_list_item()`
   - `begin_pushback()` → `begin_list()`
   - `push_pointee()` → `begin_smart_ptr()`
   - `variant()` → `select_variant()`
   - `variant_named()` → `select_variant_named()`
   - `field()` → `begin_nth_field()` or `begin_nth_element()`
3. **API returns `&mut self`**: No need to reassign variables
4. **Build return type**: `TypedPartial::build()` returns `Box<T>`
5. **No implicit Option conversion**: Must use explicit `Some(value)` or `None`