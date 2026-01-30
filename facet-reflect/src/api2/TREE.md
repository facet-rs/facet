# Tree-based frame tracking

Instead of `BTreeMap<Path, Frame>`, we keep an actual tree of frames in an arena.

## Structure

```rust
struct FrameId(NonZeroU32);

impl FrameId {
    // Special sentinels (using Option<FrameId> niche)
    // None (0)         → not started
    // Some(MAX)        → complete, value in place
    // Some(1..MAX-1)   → in progress, frame exists at this id
}

bitflags! {
    struct FrameFlags: u8 {
        const OWNS_ALLOCATION = 1 << 0;
        const IS_INIT = 1 << 1;
    }
}

struct Frame {
    parent: Option<FrameId>,
    children: Vec<Option<FrameId>>,  // indexed by field_idx / element_idx
    
    data: PtrUninit,
    shape: &'static Shape,
    flags: FrameFlags,
    
    // For enums: which variant is selected
    variant_idx: Option<u32>,
}

struct Partial {
    arena: Arena<Frame>,
    root: FrameId,
    current: FrameId,
}
```

## Child states

`children[idx]` can be:

| Value | Meaning |
|-------|---------|
| `None` | Not started - no value, no frame |
| `Some(COMPLETE)` | Complete - value is in place, frame was discarded |
| `Some(id)` | In progress - frame exists, possibly incomplete |

## Operations

### Set with Build

1. Allocate new frame in arena
2. Set `parent.children[idx] = Some(new_id)`
3. Set `current = new_id`

### End (complete)

1. Validate frame is fully initialized
2. Set `parent.children[idx] = Some(COMPLETE)`
3. Frame can be reused/freed in arena
4. Set `current = parent`

### End (deferred, incomplete)

1. Leave frame in place (children[idx] still points to it)
2. Set `current = parent`

### Re-enter

1. Look at `current.children[idx]`
2. If `Some(id)` where id != COMPLETE → set `current = id`
3. If `Some(COMPLETE)` → error or overwrite (need to drop first)
4. If `None` → create new frame (same as Set with Build)

### Drop / cleanup

1. Walk tree to find leaves (frames with no in-progress children)
2. Process leaves first: drop initialized values, dealloc if owned
3. Work up toward root

## Completeness is contagious

When a frame becomes complete, we check: is parent now complete too?

A frame is complete when:
- Scalars: `IS_INIT` flag is set
- Structs: all children are `Some(COMPLETE)` (or have defaults)
- Enums: variant selected AND all variant fields complete
- Lists: `IS_INIT` (the list itself) - elements are owned by the list
- Maps: `IS_INIT` - entries are owned by the map

If completing a child makes the parent complete, propagate up.

## Benefits

- No path computation for lookup - just follow tree edges
- Re-entry is O(1): `children[field_idx]`
- Natural deepest-first traversal for cleanup
- Could parallelize validation (subtrees are independent)
- Arena allocation - cache friendly, no per-frame heap alloc

## Open questions

1. **Arena reuse**: When a frame is "complete" and discarded, can we reuse that slot? Need generation counters?

2. **Maps**: Children are keyed by actual key values, not indices. Need a different structure - `HashMap<HeapValue, FrameId>` or similar?

3. **Lists in deferred mode**: Elements are indexed, but indices can be sparse if we're re-entering. `Vec<Option<FrameId>>` or sparse map?

4. **Memory for children vec**: Each frame allocates a Vec for children. Could use arena slices instead?
