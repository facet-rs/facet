# Tracking

How does Partial track what's initialized?

## Why we track

### 1. Drop safety / cleanup on error

If something fails partway through, we need to drop exactly what's initialized and nothing else. If `inner.x` is set but `inner.y` isn't, and we error, we must:
- Drop the value at `inner.x`
- NOT touch uninitialized memory at `inner.y`
- Deallocate the allocation

Without tracking: leak, double-free, or UB.

### 2. Validation at finish

When finalizing, we check: is everything initialized (or defaultable)? We need to know which fields are missing to either fill defaults or error.

### 3. Re-entry in deferred mode

When re-entering a stored frame, we need to know what's already set. Is setting `inner.x` again an error? An overwrite? Depends on knowing it was already set.

### 4. Replacement / overwrite

If a field is already set and someone sets it again:
- Drop the old value first
- Write the new one
- Tracking still shows "initialized"

### 5. Enum variant switching

If we select variant A, set some fields, then select variant B:
- Drop all initialized fields from variant A
- Reset tracking for the new variant
- Update discriminant

### 6. Staging / temporaries

For `Arc<T>`, we build a temporary `T`, then wrap it. The staging `T` needs tracking - if we error mid-build, we drop what's initialized in staging and deallocate, but don't touch the never-created Arc.

### 7. Map key/value staging

For `Insert { key, value: Build }` in deferred mode:
- Key is complete (moved in)
- Value is incomplete (stored frame)
- Re-entry is by key lookup

### 8. Memory ownership

Who owns what? A Vec with 3 elements owns those elements. If we store that frame and re-enter to push more, the Vec grows. If we error, Vec's drop handles its elements - but only because we know the Vec itself is initialized.

## What can be deferred?

Not everything can be re-entered:

| Type | Re-enterable by |
|------|-----------------|
| Struct fields | path index |
| Enum variant fields | path index (after variant selected) |
| Map values | key |
| List elements | index |
| Array elements | index |
| **Set elements** | **NOT re-enterable** - must complete |

Set elements have no identity (no key, no stable index) until they're hashed and inserted. An incomplete set element at `End` is an error even in deferred mode.

## Frame identity

A frame needs a unique identity for storage/lookup in deferred mode:

- **Struct field**: parent frame + field index
- **Enum field**: parent frame + variant index + field index  
- **List element**: parent frame + element index
- **Array element**: parent frame + element index
- **Map value**: parent frame + key (the actual key value)
- **Set element**: N/A (not storable)
- **Option inner**: parent frame + "some" marker
- **Smart pointer inner**: parent frame + "inner" marker

## Open questions

1. **Frame storage structure**: BTreeMap<Path, Frame>? Arena with indices? 

2. **Key storage for maps**: We need to keep the complete key around to identify the incomplete value. Where does it live?

3. **Bitset vs explicit tracking**: For structs, a bitset is compact. For maps with arbitrary keys, we need something else.

4. **Nested incomplete**: If `a.b.c` is incomplete, do we store three frames or one frame with nested tracking?
