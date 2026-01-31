# facet-reflect2 Design

Partial value construction for facet. Build values incrementally when you don't have all the data upfront.

## 1. Overview

The core abstraction is a **frame tree**. Each frame represents a value being constructed - it holds a pointer to memory, the shape (type metadata), and tracks which parts are initialized.

A `Partial` manages this tree. It has a `current` pointer that acts like a cursor. Operations navigate and mutate the tree. When you're done, `build()` validates everything is initialized and returns the value.

### Example: Building a Nested Struct

```rust
struct Line { start: Point, end: Point }
struct Point { x: i32, y: i32 }
```

Here's how to build a `Line` step by step, with the frame tree shown after each operation:

**Initial state** - `Partial::alloc::<Line>()`:

```
    ┌─────────────┐
───►│ Line        │   fields: [_, _]
    │ (current)   │
    └─────────────┘
```

**Op: `Set { dst: &[0], src: Build }`** - push frame for `start`:

```
    ┌─────────────┐
    │ Line        │   fields: [→, _]
    └──────┬──────┘            │
           │                   │
           ▼                   │
    ┌─────────────┐            │
───►│ Point       │◄───────────┘
    │ (current)   │   fields: [_, _]
    └─────────────┘
```

**Op: `Set { dst: &[0], src: Imm(&0) }`** - set `start.x`:

```
    ┌─────────────┐
    │ Line        │   fields: [→, _]
    └──────┬──────┘
           │
           ▼
    ┌─────────────┐
───►│ Point       │   fields: [✓, _]
    │ (current)   │
    └─────────────┘
```

**Op: `Set { dst: &[1], src: Imm(&0) }`** - set `start.y`:

```
    ┌─────────────┐
    │ Line        │   fields: [→, _]
    └──────┬──────┘
           │
           ▼
    ┌─────────────┐
───►│ Point       │   fields: [✓, ✓]  ← complete!
    │ (current)   │
    └─────────────┘
```

**Op: `End`** - pop back to parent, mark field complete:

```
    ┌─────────────┐
───►│ Line        │   fields: [✓, _]
    │ (current)   │
    └─────────────┘
```

The Point frame is freed. Now repeat for `end`:

**Op: `Set { dst: &[1], src: Build }`** - push frame for `end`:

```
    ┌─────────────┐
    │ Line        │   fields: [✓, →]
    └──────┬──────┘               │
           │                      │
           ▼                      │
    ┌─────────────┐               │
───►│ Point       │◄──────────────┘
    │ (current)   │   fields: [_, _]
    └─────────────┘
```

**Ops: set `end.x`, `end.y`, then `End`**:

```
    ┌─────────────┐
───►│ Line        │   fields: [✓, ✓]  ← complete!
    │ (current)   │
    └─────────────┘
```

**`build()`** - validate and return the value.

### Core Operations

There are three operations that write values: **Set**, **Push**, and **Insert**.

| Op | Target | Path |
|----|--------|------|
| `Set { dst, src }` | struct fields, enum variants, scalars | by index |
| `Push { src }` | list elements | appends |
| `Insert { key, value }` | map entries | by key |

All three take a **src** that determines how the value is written:

| Source | Effect |
|--------|--------|
| `Imm` | Copy bytes from an existing value (caller must `mem::forget` the original) |
| `Build` | Push a new frame - subsequent ops target that frame until `End` |
| `Default` | Call the type's `Default` in place |

**End** pops the current frame back to its parent. In immediate mode, the frame must be complete (all required fields initialized) or `End` returns an error.

**dst** is a sequence of indices relative to the current frame. `&[]` means the current frame itself. `&[0]` means field 0 (or variant 0 for enums). For Insert, the key serves as the path.

## 2. Building Each Type

### Scalars

```rust
// Direct set
Set { dst: &[], src: Imm(&42u32) }

// Or with default
Set { dst: &[], src: Default }
```

No `End` needed - scalars don't push frames.

### Structs

```rust
struct Point { x: i32, y: i32 }
```

**Option A: Set fields individually (common case)**

```rust
Set { dst: &[0], src: Imm(&10i32) }  // x
Set { dst: &[1], src: Imm(&20i32) }  // y
```

No frames pushed, no `End` needed.

**Option B: Push a frame for a nested struct**

```rust
struct Line { start: Point, end: Point }

Set { dst: &[0], src: Build }  // push frame for start
  Set { dst: &[0], src: Imm(&0i32) }  // start.x
  Set { dst: &[1], src: Imm(&0i32) }  // start.y
End
Set { dst: &[1], src: Build }  // push frame for end
  Set { dst: &[0], src: Imm(&10i32) }  // end.x
  Set { dst: &[1], src: Imm(&10i32) }  // end.y
End
```

**Option C: Move a complete struct**

```rust
Set { dst: &[0], src: Imm(&start_point) }
Set { dst: &[1], src: Imm(&end_point) }
```

### Enums

```rust
#[repr(u8)]
enum Message { Quit, Move { x: i32, y: i32 }, Write(String) }
```

Path index selects the variant.

**Unit variant**:

```rust
Set { dst: &[0], src: Default }  // Quit
```

**Struct variant** (needs frame for fields):

```rust
Set { dst: &[1], src: Build }  // select Move, push frame
  Set { dst: &[0], src: Imm(&10i32) }  // x
  Set { dst: &[1], src: Imm(&20i32) }  // y
End
```

**Tuple variant with single field** (Imm directly):

```rust
Set { dst: &[2], src: Imm(&"hello".to_string()) }  // Write
```

**Moving a complete enum**:

```rust
Set { dst: &[], src: Imm(&Message::Quit) }
```

### Lists (Vec, etc.)

```rust
Set { dst: &[], src: Build }  // initialize empty list, push frame
  Push { src: Imm(&1u32) }
  Push { src: Imm(&2u32) }
  Push { src: Imm(&3u32) }
// No End needed for root-level list
```

**List as a struct field**:

```rust
Set { dst: &[0], src: Build }  // push frame for list field
  Push { src: Imm(&"server1") }
  Push { src: Imm(&"server2") }
End
```

**List with complex elements**:

```rust
Set { dst: &[], src: Build }
  Push { src: Build }  // push frame for element
    Set { dst: &[0], src: Imm(&"host") }
    Set { dst: &[1], src: Imm(&8080u16) }
  End
End
```

`Build.len_hint` enables pre-allocation.

### Maps (HashMap, etc.)

```rust
Set { dst: &[], src: Build }  // initialize empty map, push frame
  Insert { key: Imm(&"PATH"), value: Imm(&"/usr/bin") }
  Insert { key: Imm(&"HOME"), value: Imm(&"/home/user") }
// No End needed for root-level map
```

**Map with complex values**:

```rust
Set { dst: &[], src: Build }
  Insert { key: Imm(&"primary"), value: Build }  // push frame for value
    Set { dst: &[0], src: Imm(&"localhost") }  // host
    Set { dst: &[1], src: Imm(&8080u16) }      // port
  End
End
```

### Arrays

```rust
struct Point3D { coords: [f32; 3] }
```

Arrays are fixed-size. Push a frame for the array, then set elements by index:

```rust
Set { dst: &[0], src: Build }  // push frame for coords
  Set { dst: &[0], src: Imm(&1.0f32) }  // coords[0]
  Set { dst: &[1], src: Imm(&2.0f32) }  // coords[1]
  Set { dst: &[2], src: Imm(&3.0f32) }  // coords[2]
End
```

Or set the whole array at once:

```rust
Set { dst: &[0], src: Imm(&[1.0f32, 2.0f32, 3.0f32]) }
```

### Sets (HashSet, etc.)

```rust
Push { src: Imm(&"tag1") }
Push { src: Imm(&"tag2") }
```

Sets use `Push` like lists. The implementation hashes and inserts.

**Important**: Set elements cannot be partially constructed. They have no identity until hashed. `Push { src: Build }` for a set element must complete before `End`.

### Option

**Complete Option via Imm**:

```rust
Set { dst: &[0], src: Imm(&Some(30u32)) }  // Some
Set { dst: &[0], src: Imm(&None::<u32>) }  // None
Set { dst: &[0], src: Default }            // None (Option's default)
```

**Building Some with complex inner**:

```rust
Set { dst: &[0], src: Build }  // push frame for Some(T)
  Set { dst: &[0], src: Imm(&"host") }  // T's field 0
  Set { dst: &[1], src: Imm(&8080u16) } // T's field 1
End
```

When you `Build` into an Option, the frame is for the `Some(T)` - you set fields of `T` directly.

### Smart Pointers (Box, Rc, Arc)

```rust
Set { dst: &[], src: Build }  // allocate staging memory, push frame
  Set { dst: &[0], src: Imm(&10i32) }  // x
  Set { dst: &[1], src: Imm(&20i32) }  // y
End  // calls new_into_fn to create the pointer
```

`Build` allocates staging memory for the pointee. `End` wraps it in the pointer type and deallocates the staging memory.

## 3. Frames and Tracking

A **frame** represents a value under construction:

```rust
struct Frame {
    data: PtrUninit,           // pointer to the memory
    shape: &'static Shape,     // type metadata
    kind: FrameKind,           // how to track children
    flags: FrameFlags,         // INIT, OWNS_ALLOC
    parent: Option<(Idx, u32)>, // parent frame and our index in it
    pending_key: Option<...>,  // for map value frames
}
```

### Frame Kinds

- **Scalar**: No children. Complete when `INIT` flag is set.

- **Struct**: Tracks each field as NOT_STARTED, in-progress, or COMPLETE. Complete when all fields are COMPLETE.

- **Enum**: Tracks selected variant as `Option<(variant_idx, status)>`. Status is NOT_STARTED, in-progress frame idx, or COMPLETE.

- **VariantData**: Inside an enum variant, building its fields. Same tracking as Struct.

- **Pointer**: Tracks inner as NOT_STARTED, in-progress frame idx, or COMPLETE.

- **List**: Holds the initialized list pointer and element count. Always "complete" (variable size).

- **Map**: Holds the initialized map pointer and entry count. Always "complete" (variable size).

### Completeness

A frame is complete when:

- **Scalar**: `INIT` flag is set
- **Struct/VariantData**: All fields are COMPLETE
- **Enum**: A variant is selected AND its status is COMPLETE
- **List/Map**: Always complete (they own their elements)
- **Pointer**: Inner is COMPLETE

`End` checks completeness. If incomplete, returns error.

### The Arena

Frames live in an arena with a free list for reuse:

```rust
struct Idx(u32);  // 0 = NOT_STARTED, MAX = COMPLETE, else valid index

struct Arena<Frame> {
    slots: Vec<Option<Frame>>,
    free_list: Vec<u32>,
}
```

When a frame completes, its slot can be reused. The parent's tracking changes from a valid index to `COMPLETE`.

## 4. Safety

### Drop on Error

If an operation fails, `poison()` is called:

1. Walk from current frame up to root
2. For each frame, call `uninit()` to drop any initialized values
3. Free the frame and deallocate if it owns its allocation

After poisoning, any further operation returns `Poisoned` error.

### Overwriting Values

When setting a field that already has a value:

1. If the whole struct has `INIT` flag (set via Imm of complete struct):
   - Drop the old field value
   - Clear `INIT` flag
   - Mark all OTHER fields as COMPLETE (they're still valid)

2. If just this field was set individually:
   - Drop the old field value
   - Mark field as NOT_STARTED (so cleanup won't double-drop on error)

This is handled by `prepare_field_for_overwrite()`.

### Variant Switching

When selecting a different enum variant:

1. If `INIT` is set (whole enum was moved in): call `drop_in_place` on the whole value
2. If a variant was selected and complete: drop that variant's fields
3. Clear the selected variant
4. Write the new discriminant

## 5. Deferred Mode

**Current behavior (immediate mode)**: `End` requires the frame to be complete. All children must be initialized before you can pop back to the parent.

**Deferred mode** (not yet implemented): `End` on an incomplete frame stores it for later. You can re-enter that frame to finish initialization.

### Why Deferred Mode

Consider parsing JSON into a struct. JSON objects have no guaranteed field order. With immediate mode, you'd need to buffer the entire object, sort by field, then apply ops. With deferred mode:

```rust
Set { dst: &[0], src: Build }  // push frame for struct field
  // ... see field "y" first in JSON ...
  Set { dst: &[1], src: Imm(&20) }  // set y
End  // frame is incomplete, but that's OK - store it

// ... later, see field "x" ...
// re-enter the stored frame
Set { dst: &[0], src: Imm(&10) }  // set x
End  // now complete
```

### How It Changes Semantics

**End behavior**:
- Immediate: incomplete → error
- Deferred: incomplete → store frame, pop to parent anyway

**Set with Build behavior**:
- Immediate: always creates new frame
- Deferred: if frame already exists for that path, re-enter it

**Validation**:
- Immediate: checked at each `End`
- Deferred: checked at final `build()` or explicit flush

### Re-entry by Path

For struct fields: re-enter by field index.
For enum variants: re-enter by variant index (after variant is selected).
For list elements: re-enter by element index.
For map values: re-enter by key.

### What Cannot Be Deferred

**Set elements** have no identity until hashed and inserted. You can't partially build a set element, `End`, then come back - there's no key to find it by. Set elements must complete immediately.

### Frame Storage

Incomplete frames stay in the arena. The parent's child tracking holds the frame index (not COMPLETE). Re-entry looks up the child, finds a valid index, and sets `current` to that frame.

For maps, re-entry by key requires storing the key with the frame. This needs a type-erased key (`DynKey`) that can hash and compare using the shape's vtable.
