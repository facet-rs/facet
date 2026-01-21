# Chawathe Edit Script Semantics

This document describes how the Chawathe algorithm's edit operations work,
particularly the **non-shifting semantics** that differ from typical array operations.

## Background

The Chawathe algorithm (from "Change Detection in Hierarchically Structured Information", 
Chawathe et al., 1996) generates an edit script to transform one tree into another using
four operations: **INSERT**, **DELETE**, **UPDATE**, and **MOVE**.

A key insight is that Chawathe edit scripts are designed for **tree structures**, not arrays.
The semantics are fundamentally different from array splice operations.

## The Displacement Model (Slots)

In Chawathe semantics, **INSERT and MOVE operations do NOT shift sibling nodes**.
Instead, they **displace** whatever node currently occupies the target position.

The displaced node is conceptually detached from the tree and stored in a **slot**
for potential later reinsertion (via MOVE or another INSERT).

### Why Slots?

Consider transforming:
```
[A, B, C] → [X, A, B, C]
```

**Array splice approach** (NOT Chawathe):
1. Insert X at position 0
2. All existing nodes shift: A→1, B→2, C→3

**Chawathe approach**:
1. INSERT X at position 0, displacing A to slot #1
2. INSERT A at position 1, displacing B to slot #2  
3. INSERT B at position 2, displacing C to slot #3
4. INSERT C at position 3 (no displacement, appending)

Or more efficiently with MOVE:
1. INSERT X at position 0, displacing A to slot #1
2. MOVE slot #1 to position 1, displacing B to slot #2
3. MOVE slot #2 to position 2, displacing C to slot #3
4. MOVE slot #3 to position 3

The key insight: **each operation specifies an exact position**, and any existing
node at that position is saved for later, not lost.

## Operation Semantics

### INSERT

```
INSERT node at parent[position], detach_to_slot: Option<slot_id>
```

1. Navigate to `parent`
2. If there's a child at `position`:
   - If `detach_to_slot` is Some, store the existing child in that slot
   - Use `replaceChild(new_node, existing_child)` for atomic replacement
3. Otherwise, insert at that position (or append if beyond current children)

### MOVE

```
MOVE from source to parent[position], detach_to_slot: Option<slot_id>
```

1. Retrieve the node from `source` (either a tree path or a slot number)
2. If there's a node at `parent[position]`:
   - If `detach_to_slot` is Some, store it in that slot
   - Use `replaceChild(moving_node, existing_child)` for atomic replacement
3. Otherwise, insert at that position

### DELETE

```
DELETE node
```

1. Locate the node (by path or slot)
2. Remove it from the tree (or simply discard if in a slot)

### UPDATE

```
UPDATE node, old_value → new_value
```

1. Locate the matched node
2. Update its label/value in place

## Implementation Notes

### DOM Implementation (replaceChild)

The DOM's `replaceChild(newChild, oldChild)` method is perfect for Chawathe semantics:
- It atomically replaces `oldChild` with `newChild`
- It **returns** the removed `oldChild`, which can be stored in a slot
- It handles all the DOM parent/child bookkeeping

```javascript
// Chawathe INSERT with displacement
const displaced = parent.replaceChild(newNode, existingChild);
slots.set(slotId, displaced);
```

### Path Interpretation for MOVE

When processing a MOVE operation with target path like `[0, 2, 1]`:
- The path represents: `root → child[0] → child[2] → child[1]`
- The **last segment** (1) is the position within the parent
- The **parent path** is `[0, 2]`

```rust
let parent_path = to.0[..to.0.len() - 1];
let target_position = to.0[to.0.len() - 1];
```

### Slot Management

Slots are a simple map from slot ID to node reference:

```rust
struct Slots {
    map: HashMap<u32, Node>,
}

impl Slots {
    fn store(&mut self, id: u32, node: Node) { ... }
    fn take(&mut self, id: u32) -> Option<Node> { ... }
}
```

Slots should be cleared/validated at the end of applying an edit script - any
remaining nodes in slots represent an error (orphaned nodes).

## Why Not Just Shift?

The Chawathe model has several advantages:

1. **Edit script is position-independent**: Each operation fully specifies its effect
   without reference to what other operations have done. This makes parallel
   application theoretically possible.

2. **Matches tree semantics**: Trees have parent-child relationships, not array indices.
   A node's "position" is relative to its current siblings, not a global index.

3. **Minimal moves**: By using displacement, the algorithm can express "swap these two"
   efficiently rather than requiring a sequence of remove/insert operations.

4. **Easier verification**: When applying A→B, you can verify by checking the final
   structure matches B, without tracking intermediate states.

## References

- Chawathe, S.S. et al. "Change Detection in Hierarchically Structured Information"
  SIGMOD 1996
- GumTree (uses Chawathe for edit script): Falleri et al. "Fine-grained and accurate
  source code differencing" ASE 2014
- facet-html-diff: Our DOM implementation of Chawathe semantics
- facet-html-diff-wasm: Browser-based validation using real DOM operations
