# Handoff: Flatten deserialization with deferred mode - COMPLETE

## Current State

Branch: `fix-untagged-enum-deserialization`

All 2095 tests pass.

## What Was Done

### 1. facet-solver
Changed `seen_keys` from `BTreeSet<&'a str>` to `BTreeSet<Cow<'a, str>>` - eliminates `Box::leak` in callers.

### 2. facet-reflect

**Path tracking for deferred mode:**
- Added path tracking for `begin_some()` in deferred mode - pushes "Some" onto `current_path`
- Modified `end()` to skip full init check when frame will be stored in deferred mode
- Added `complete_option_frame()` to handle Option completion in `finish_deferred()`

**Error messages:**
- Updated error messages to mention deferred mode
- Updated snapshots for new error messages

**Tests:**
- Added `deferred_option_struct_interleaved_fields`
- Added `deferred_option_struct_deeply_nested_interleaved`

### 3. facet-json

**Flatten deserialization rewrite:**
- Rewrote `deserialize_struct_with_flatten` to use rewind pattern with `at_offset`
- Properly handles externally tagged enums in flatten context:
  - When path ends with `Variant` segment, selects variant then calls `deserialize_variant_struct_content`
  - This avoids the bug where `deserialize_into` would call `deserialize_enum` expecting `{"VariantName": data}` again

**Cleanup:**
- Removed unused `deserialize_at_path` function (logic inlined in `deserialize_struct_with_flatten`)

## Key Insight: Variant Selection

`select_variant_named` does NOT push a stack frame - it modifies the current frame's `Tracker` to track the selected variant. Only `begin_field()` on variant fields pushes frames.

This means after selecting a variant, you don't call `deserialize_into` (which would try to deserialize the enum from scratch). Instead, you call `deserialize_variant_struct_content` to deserialize the variant's fields directly.

## Files Modified

- `facet-json/src/deserialize.rs` - flatten handling, removed dead code
- `facet-reflect/src/error.rs` - deferred mode error messages
- `facet-reflect/src/partial/partial_api/misc.rs` - `complete_option_frame`, deferred handling in `end()`
- `facet-reflect/src/partial/partial_api/option.rs` - path tracking for `begin_some()`
- `facet-reflect/tests/partial/deferred.rs` - new tests
- Various snapshot files

## Ready to Commit

All tests pass. Changes are ready to be committed and pushed for PR.
