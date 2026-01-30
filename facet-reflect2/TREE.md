# Tree-based frame tracking

Instead of `BTreeMap<Path, Frame>`, we keep an actual tree of frames in an arena.

## Structure

```rust
struct FrameId(u32);

impl FrameId {
    const NOT_STARTED: FrameId = FrameId(0);
    const COMPLETE: FrameId = FrameId(u32::MAX);
    
    fn is_not_started(self) -> bool { self.0 == 0 }
    fn is_complete(self) -> bool { self.0 == u32::MAX }
    fn is_in_progress(self) -> bool { self.0 != 0 && self.0 != u32::MAX }
    // Valid arena indices: 1..MAX-1
}

struct Arena {
    slots: Vec<Option<Frame>>,
    free_list: Vec<u32>,
}

impl Arena {
    fn alloc(&mut self, frame: Frame) -> FrameId {
        if let Some(idx) = self.free_list.pop() {
            self.slots[idx as usize] = Some(frame);
            FrameId(idx)
        } else {
            let idx = self.slots.len() as u32;
            self.slots.push(Some(frame));
            FrameId(idx)
        }
    }
    
    fn free(&mut self, id: FrameId) {
        debug_assert!(id.is_in_progress());
        self.slots[id.0 as usize] = None;
        self.free_list.push(id.0);
    }
    
    fn get(&self, id: FrameId) -> &Frame {
        self.slots[id.0 as usize].as_ref().unwrap()
    }
    
    fn get_mut(&mut self, id: FrameId) -> &mut Frame {
        self.slots[id.0 as usize].as_mut().unwrap()
    }
}
```

**No generational indices needed**: The only place a FrameId is stored is in `parent.children[idx]`. When a frame completes, we overwrite that slot with `COMPLETE` and free the arena slot. There are no stale references - the parent held the only reference and we just replaced it.

bitflags! {
    struct FrameFlags: u8 {
        const OWNS_ALLOCATION = 1 << 0;
        const IS_INIT = 1 << 1;
    }
}

/// Type-erased key for map lookups using shape vtables
struct DynKey {
    ptr: PtrUninit,
    shape: &'static Shape,
}

impl Hash for DynKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        unsafe { self.shape.call_hash(self.ptr.assume_init(), state) }
    }
}

impl PartialEq for DynKey {
    fn eq(&self, other: &Self) -> bool {
        if !self.shape.is_shape(other.shape) {
            return false;
        }
        unsafe { self.shape.call_eq(self.ptr.assume_init(), other.ptr.assume_init()) }
    }
}

impl Eq for DynKey {}

/// Children structure varies by container type
enum Children {
    /// Structs, arrays: indexed by field/element index
    /// FrameId::NOT_STARTED, COMPLETE, or valid index
    Indexed(Vec<FrameId>),
    
    /// Enums: at most one variant active at a time
    /// None = no variant selected, Some = (variant_idx, frame state)
    Variant(Option<(u32, FrameId)>),
    
    /// Lists: can grow dynamically via Push
    List(Vec<FrameId>),
    
    /// Maps: keyed by actual key values for O(1) re-entry
    Map(HashMap<DynKey, FrameId>),
    
    /// Option inner, smart pointer inner: single child
    Single(FrameId),  // NOT_STARTED, COMPLETE, or valid index
    
    /// Scalars, sets: no children (sets can't be re-entered)
    None,
}

struct Frame {
    parent: Option<FrameId>,
    children: Children,
    
    data: PtrUninit,
    shape: &'static Shape,
    flags: FrameFlags,
}

struct Partial {
    arena: Arena<Frame>,
    root: FrameId,
    current: FrameId,
}
```

## Child states

For `Children::Indexed`, `Children::List`, and `Children::Single`:

| Value | Meaning |
|-------|---------|
| `NOT_STARTED` (0) | Not started - no value, no frame |
| `COMPLETE` (MAX) | Complete - value is in place, frame was discarded |
| `1..MAX-1` | In progress - frame exists at this arena index |

For `Children::Variant`:
- `None` → no variant selected
- `Some((idx, COMPLETE))` → variant idx selected and complete
- `Some((idx, frame_id))` → variant idx in progress

For `Children::Map`:
- Key absent → not started
- Key present with `COMPLETE` → complete
- Key present with valid id → in progress

For `Children::None`:
- No children to track (scalars, set elements)

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

## Enum variant handling

Enum variants ARE frames. When you do:
```rust
Set { path: &[1], source: Build }  // select variant 1
```

This creates a variant frame. The structure is:
```
Enum frame (Children::Variant)
  └── Variant frame (children for variant's fields)
        ├── field 0
        ├── field 1
        └── ...
```

**Reading the discriminant**: If we need to know which variant is selected (e.g., after a `Move` that wrote the whole enum), we read the discriminant from memory using the Shape's vtable. We don't track `variant_idx` separately - the memory is the source of truth.

**Variant switching**: If variant 1 is in progress and someone selects variant 2:
1. Drop all initialized fields of variant 1
2. Deallocate variant 1's frame
3. Create new frame for variant 2

## Open questions

1. **Lists in deferred mode**: Elements are indexed, but indices can be sparse if we're re-entering. `Vec<FrameId>` with `NOT_STARTED` slots should handle this.

2. **Memory for children**: Each frame may allocate a Vec/HashMap for children. Could use arena slices for the common case?

3. **DynKey ownership**: The `DynKey` owns the key allocation. On cleanup, we need to drop the key value and deallocate. On successful insert into the actual map, we move the key out.
