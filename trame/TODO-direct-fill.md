# trame Direct Fill TODO

Performance optimization to minimize vtable calls and data movement during collection construction.

## Current State

- `init_in_place_with_capacity` is called for initial allocation
- But still calls `push_fn` per element (in `apply_push` for `Source::Imm`)
- `MapEntryFrame` is per-entry staging (for building ONE key+value pair), not bulk staging

## Lists (Vec, etc.)

**Current:** `init_in_place_with_capacity(cap)` + repeated `push_fn` per element in `apply_push`.

**Goal:**
1. Allocate staging buffer (grows dynamically like Vec if capacity unknown)
2. Write elements directly into staging buffer at `buffer + (index * stride)`
3. `set_len(n)` at list End - vtable method exists in facet-core

**Hook:** `apply_push` with `Source::Imm` (line ~60 in push.rs) - currently calls `push_fn`, should write to staging instead.

**Implementation:**
- `ListFrame` needs staging buffer pointer + capacity + count
- `apply_push` writes to staging buffer, grows if needed
- On list End: `set_len(count)` on the actual list

## Maps (HashMap, etc.)

**Current:** `init_in_place_with_capacity(cap)` + repeated `insert_fn` per entry.

**Goal:**
1. Allocate staging buffer for entries (grows dynamically)
2. Build entries into staging buffer slots
3. Single `from_iter` call at map End

**Implementation:**
- `MapFrame` needs staging buffer pointer + capacity + count
- `MapEntryFrame` builds into a slot in that staging buffer (growing if needed, not separate allocation per entry)
- On entry End: increment count (entry already written to staging)
- On map End: call `from_iter` with staging buffer
- Need: `from_iter` vtable method for maps

## Sets (HashSet, etc.)

Same as lists - staging buffer, write directly, `from_iter` at end.

## Vtable Requirements

facet-core already has everything needed:

**Lists:**
- `init_in_place_with_capacity` - used
- `set_len` - exists for Vec

**Maps:**
- `from_pair_slice(uninit, pairs_ptr, count)` - builds HashMap from contiguous (K, V) pairs
- `pair_stride` - `size_of::<(K, V)>()`
- `value_offset_in_pair` - `offset_of!((K, V), 1)`

**Sets:**
- No `from_slice` yet - `SetVTable` only has `insert`
- Would need to add `from_slice(uninit, elements_ptr, count)` to facet-core
