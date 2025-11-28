# Partial Ownership Redesign

## Problem Statement

The current `Partial` implementation has multiple sources of truth for tracking whether data is initialized:

1. `frame.is_init: bool` on child frames
2. Parent's `Tracker::Struct { iset, .. }` marking which fields are initialized

This leads to aliasing problems where both parent and child believe they're responsible for dropping the same memory, causing double-frees. The complexity is compounded in deferred mode where frames can be stored and revisited.

## Current Issues

### Double-Free Scenario
```
Parent Frame (FuzzTarget)
├── tracker: Struct { iset: [name=✓, count=✓, ...] }
│
Child Frame (name field)
├── data: ptr to FuzzTarget.name  ← SAME memory
├── is_init: true
├── ownership: Field
```

On drop, both parent (via iset) and child (via is_init) may try to drop the same data.

### Deferred Mode Complexity

When `end()` stores a frame in `stored_frames`:
- The frame is detached from the stack
- Parent's iset may or may not reflect the field's state
- Drop logic must reconcile these states with ~200 lines of complex code

## Proposed Design

### Core Invariant

**For any memory location, exactly ONE entity is responsible for dropping it at any time.**

This is achieved through explicit ownership transfer: when navigating into a field, the parent temporarily relinquishes responsibility; when returning, responsibility is restored.

### Data Structures

```rust
/// Whether a frame's data is initialized and the frame is responsible for it
#[derive(Debug, Clone, Copy)]
pub(crate) enum FrameState {
    /// Data is not initialized, nothing to drop
    Uninitialized,
    /// Data is initialized, this frame is responsible for dropping it
    Initialized,
}

/// Who owns the allocation and how responsibility is tracked
#[derive(Debug)]
pub(crate) enum FrameOwnership {
    /// This frame owns the heap allocation itself.
    /// On drop: deallocate the memory.
    /// Parent's iset: N/A (no parent tracks this)
    Owned,

    /// This frame points to a field within a parent's allocation.
    /// The parent's iset[field_idx] was CLEARED when this frame was created.
    /// On drop: deinit if Initialized, but do NOT deallocate.
    /// On successful end(): parent's iset[field_idx] will be SET.
    ///
    /// Note: Currently `Field` is a unit variant and `current_child` tracks the index.
    /// After migration, `Field { field_idx }` replaces `current_child` - the child
    /// frame knows its own index, enabling `end()` to set the right iset bit.
    Field {
        field_idx: usize,
    },

    /// This frame's allocation is managed elsewhere (e.g., map key/value buffers).
    /// On drop: deinit if Initialized, but do NOT deallocate.
    /// Parent's iset: not modified (these are temporary buffers, not struct fields)
    ManagedElsewhere,
}

pub(crate) struct Frame {
    /// Address of the value
    pub(crate) data: PtrUninit<'static>,

    /// Shape of the value
    pub(crate) shape: &'static Shape,

    /// Whether this frame is responsible for initialized data
    pub(crate) state: FrameState,

    /// Tracks partial initialization (which struct fields, list state, etc.)
    pub(crate) tracker: Tracker,

    /// Ownership model for this frame
    pub(crate) ownership: FrameOwnership,
}
```

### FrameState Semantics by Type

**Scalars**: `state = Initialized` means the value has been written via `put_*`.

**Structs**: `state = Initialized` means "drop glue should run". The drop glue iterates
`iset` and drops only fields where `iset[idx] = true`. A struct can be `Initialized`
even with only some fields set - it will correctly drop just those fields.

**Collections (List, Map, Set)**: `state = Initialized` means the collection itself
exists (e.g., `Vec::new()` was called). Elements are tracked separately via vtable
functions, not iset.

**Arrays**: Like structs, use `iset` to track which elements are initialized.
`state = Initialized` means drop glue should run (which respects iset).

### Tracker Simplification

The current `Tracker::Struct` has:
```rust
Tracker::Struct {
    iset: ISet,
    current_child: Option<usize>,  // ← Can be removed
}
```

The `current_child` field exists only for drop logic to determine "which field are we
currently inside". With the new model, this is unnecessary:
- When we enter a field, parent's `iset[idx]` is cleared
- Parent can never drop a field we're currently in
- No lookup needed at drop time

After migration, `Tracker::Struct` simplifies to just `{ iset: ISet }`. Same applies
to `Tracker::Array` and `Tracker::Enum`.

### Operation Semantics

#### `begin_nth_field(idx)`

```
Before:
  Parent iset[idx] = ✓ (or ✗ if field wasn't init)

Action:
  1. was_init = parent.iset[idx]
  2. parent.iset[idx] = ✗           ← Parent relinquishes responsibility
  3. Push child Frame {
       state: if was_init { Initialized } else { Uninitialized },
       ownership: Field { field_idx: idx },
       ...
     }

After:
  Parent iset[idx] = ✗
  Child is solely responsible for the field
```

#### `end()` in Strict Mode

```
Before:
  Child frame on stack, parent iset[idx] = ✗

Action:
  1. Validate child is fully initialized
  2. Pop child frame
  3. parent.iset[idx] = ✓           ← Parent reclaims responsibility

After:
  Parent iset[idx] = ✓
  Child frame gone, parent is responsible
```

#### `end()` in Deferred Mode

```
Before:
  Child frame on stack, parent iset[idx] = ✗

Action:
  1. Pop child frame
  2. Store in stored_frames
  3. parent.iset[idx] stays ✗       ← Parent still NOT responsible

After:
  Parent iset[idx] = ✗
  Stored frame is responsible (tracked by its FrameState)
```

#### `finish_deferred()`

```
Before:
  Stored frames with their FrameState
  Parent iset has those fields cleared

Action:
  For each stored frame (deepest first):
    1. Validate fully initialized
    2. parent.iset[field_idx] = ✓   ← Transfer responsibility to parent
    3. Discard stored frame (no deinit needed, parent now owns)

After:
  All stored frames processed
  Parent iset reflects all initialized fields
```

#### Drop (Partial dropped without proper cleanup)

```
1. Process stored_frames:
   For each stored frame:
     - If state == Initialized: deinit (drop the data)
     - Parent's iset[field_idx] is ✗, so parent won't touch it

2. Process stack frames (top to bottom):
   For each frame:
     Match ownership:
       Owned:
         - If state == Initialized: deinit
         - Deallocate memory

       Field { field_idx }:
         - If state == Initialized: deinit
         - Parent's iset[field_idx] is ✗, so parent won't touch it
         - Do NOT deallocate (parent owns allocation)

       ManagedElsewhere:
         - If state == Initialized: deinit
         - Do NOT deallocate

3. When parent frame is processed:
   - Its deinit() only drops fields where iset[idx] = ✓
   - All fields we navigated into have iset = ✗
   - No double-free possible
```

### Deferred Mode Specifics

#### Stored Frame Structure

```rust
struct StoredFrame {
    frame: Frame,
    path: KeyPath,  // For nested field navigation
}
```

The `Frame::ownership` contains `Field { field_idx }` which tells us which field of the parent this was. The `path` tells us how to find the parent (for nested fields).

#### Restoring a Stored Frame

When `begin_nth_field` is called and finds a stored frame:

```
Action:
  1. Remove frame from stored_frames
  2. Push frame to stack
  3. (Parent's iset[idx] is already ✗ from when frame was first created)

The frame's FrameState is preserved, so we know if it was Initialized.
```

### Invariants

1. **Single Responsibility**: For any field, either:
   - Parent's iset has it marked (parent responsible), OR
   - A frame exists for it on stack or in stored_frames (frame responsible)
   - Never both.

2. **Ownership Transfer**:
   - `begin_*` clears parent's iset → child responsible
   - `end()` (strict) sets parent's iset → parent responsible
   - `end()` (deferred) keeps parent's iset clear → stored frame responsible
   - `finish_deferred()` sets parent's iset → parent responsible

3. **Drop Safety**:
   - Frames deinit based on their own FrameState
   - Parents deinit based on their own iset
   - These never overlap due to the ownership transfer protocol

## Migration Path

### Step 1: Add `iset.unset(idx)` in `begin_nth_struct_field`

In `internal.rs`, after reading `was_field_init`:
```rust
let was_field_init = match &mut frame.tracker {
    Tracker::Struct { iset, current_child } => {
        *current_child = Some(idx);
        let was_init = iset.get(idx);
        iset.unset(idx);  // ← ADD THIS: parent relinquishes responsibility
        was_init
    }
    _ => unreachable!(),
};
```

Same pattern for `begin_nth_array_element` and `begin_nth_enum_field`.

### Step 2: Verify `end()` already sets iset

The current code (misc.rs:632-634) already does:
```rust
iset.set(idx);
*current_child = None;
```

This is correct - parent reclaims responsibility on successful `end()`.

### Step 3: Simplify drop logic

The current ~200 lines of drop logic (mod.rs:1154-1351) can be replaced with:
```rust
impl Drop for Partial {
    fn drop(&mut self) {
        // 1. Deinit stored frames (deferred mode)
        if let FrameMode::Deferred { stored_frames, .. } = &mut self.mode {
            for (_, mut frame) in stored_frames.drain(..) {
                if frame.state == FrameState::Initialized {
                    frame.deinit();
                }
                // Don't deallocate - Field ownership means parent owns memory
            }
        }

        // 2. Pop and deinit stack frames
        while let Some(mut frame) = self.mode.stack_mut().pop() {
            if frame.state == FrameState::Initialized {
                frame.deinit();
            }
            if let FrameOwnership::Owned = frame.ownership {
                frame.dealloc();
            }
        }
    }
}
```

No `parent_will_drop` checks. No `is_field_marked_in_parent`. No special cases.
Each frame's `state` is the sole authority on whether it should deinit.

### Step 4: Remove `current_child` from Tracker variants

Once drop logic doesn't need it:
```rust
// Before
Tracker::Struct { iset: ISet, current_child: Option<usize> }
// After
Tracker::Struct { iset: ISet }
```

### Step 5: Rename `is_init` to `state`

Change `Frame::is_init: bool` to `Frame::state: FrameState` for clarity.
Could keep as bool if preferred - the semantics matter more than the name.

## Type-Specific Behavior

### Structs and Arrays (iset-based)

The ownership transfer protocol applies directly:
- `begin_nth_field` / `begin_nth_element`: clear parent's `iset[idx]`
- `end()`: set parent's `iset[idx]`
- Drop: each side only drops what their iset says

### Collections (List, Map, Set)

Collections use a **different mechanism** - not iset-based ownership transfer:
- Element frames have `FrameOwnership::Owned` (they allocate element memory)
- On `end()`, ownership transfers via vtable function (`push_fn`, `insert_fn`)
- The vtable function **moves** the data out of the element frame
- Element frame is then deallocated (but not deinited - data was moved)
- Collection's own drop glue handles dropping all elements

No iset involved. The "ownership transfer" is a move into the collection.

### DynamicValue Arrays

Same as collections - elements are pushed via `push_array_element` vtable function.
Element frames have `Owned` ownership. No special handling needed.

### Option Types

`begin_some()` doesn't use iset. The `Tracker::Option` tracks whether we're building
the inner value. On `end()`, the option is marked as `Some`. Similar transfer protocol
but simpler (only one "slot").

### Nested Deferred Fields

Path `["a", "b"]` means frame "b" is a field of frame "a", which is stored.
- When navigating into "a": parent's `iset["a"]` cleared
- When navigating into "b": "a"'s `iset["b"]` cleared
- When "b" is stored: "a"'s `iset["b"]` stays clear
- When "a" is stored: parent's `iset["a"]` stays clear

Ownership is clean at every level.

## Code Removal Summary

After migration, the following can be removed:

### From `mod.rs` Drop impl (~150 lines)
- `is_field_marked_in_parent()` helper
- `unmark_field_in_parent()` helper
- All `parent_will_drop` logic
- All `is_initialized_collection_with_partial_state` special cases
- The entire stored_frames cleanup loop complexity

### From Tracker variants
- `current_child: Option<usize>` from `Tracker::Struct`
- `current_child: Option<usize>` from `Tracker::Array`
- `current_child: Option<usize>` from `Tracker::Enum`

### From deferred mode
- Path-based parent lookup for drop
- Complex depth calculations for "should we deinit"

## Alternatives Considered

### Keep is_init, fix drop logic
We've been doing this. Each fix reveals new edge cases. The fundamental aliasing problem remains.

### Remove is_init entirely, only use parent's iset
Doesn't work for `Owned` frames which have no parent. Also complicates deferred mode where frames are detached.

### Store parent reference in each frame
Complicates memory management. Parent indices change as stack grows/shrinks.

## Conclusion

The ownership transfer model provides a clean invariant: at any moment, exactly one entity
is responsible for each piece of data.

**The key insight**: `begin_field` should immediately clear the parent's iset, not wait
until `end()` or drop time to figure out who's responsible.

**Why this works**: The current code tries to maintain two sources of truth (`is_init` and
`iset`) and reconcile them at drop time. This is fundamentally broken because:
1. Both can be true for the same memory (aliasing)
2. Drop logic must guess which one "really" owns the data
3. Deferred mode adds stored frames as a third source of truth

The fix is simple: make `iset` authoritative for struct fields. When you enter a field,
clear the bit. When you leave, set it. At any point, exactly one of these is true:
- Parent's `iset[idx] = true` → parent drops it
- Child frame exists → child drops it (based on its own state)

No reconciliation needed. No guessing. No special cases.
