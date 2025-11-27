# Deferred Materialization

This document explains the design of deferred materialization in `facet-reflect`.

## How facet-reflect Works: In-Place Initialization

`facet-reflect` initializes values **in place**, directly in their final memory
location. As you set fields, the actual struct in memory is being written to.

This means you can have partially initialized values at any point:

- A struct where only some fields are set
- An enum where no variant has been selected (discriminant not yet written)
- An array with gaps (elements 0 and 2 set, but not 1)
- A smart pointer whose inner value is half-built

This approach is more efficient (no intermediate allocations) but requires
careful tracking of what's initialized and what isn't.

## The Frame Stack

`facet-reflect` uses a stack of `Frame`s to track nested initialization:

```rust
struct Frame {
    data: PtrUninit<'static>,    // Pointer to the memory being initialized
    shape: &'static Shape,        // Type information
    tracker: Tracker,             // What's initialized within this value
    ownership: FrameOwnership,    // Who owns this memory
    // ...
}
```

When you navigate into a nested field, you push a frame. When you're done
with that field, you pop. The `Tracker` inside each frame knows the state:

- `Tracker::Struct { iset }` - bitset of which fields are initialized
- `Tracker::Enum { variant, data }` - selected variant + field bitset
- `Tracker::Array { iset }` - bitset of which elements are initialized
- etc.

## Strict Mode: Validate on Pop

The default mode is strict mode. It enforces a critical invariant:

**When you pop a frame, the value must be fully initialized.**

When you pop a child frame, its initialization status is recorded in the
parent. If the parent is a struct and the child was a field, the parent's
`ISet` (initialization bitset) marks that field as initialized.

If you try to visit that child again, one of two things happens:

1. **Error**: The field is already initialized, operation rejected.
2. **Overwrite**: The old value is dropped and you initialize fresh.

Which behavior occurs depends on the API used and the type involved.

Strict mode is designed for simplicity and speed:

1. **Simplicity**: Once popped, you don't revisit. The parent's ISet is
   the single source of truth for "is this field done?"

2. **Speed**: No bookkeeping for "maybe we'll come back to this later."

3. **Finalization**: Some types need processing at pop time. For example,
   when building `Box<[T]>`, we actually build a `Vec<T>` as scratch space,
   then convert it to a boxed slice on pop.

This works perfectly when you control the initialization order.

## The Problem: Deserialization

Consider this type:

```rust
struct Outer {
    name: String,
    inner: Inner,
    count: u64,
}

struct Inner {
    x: u32,
    y: String,
}
```

In well-formed JSON, objects are self-contained. Let's trace through parsing:

```json
{ "name": "test", "inner": { "x": 42, "y": "hello" }, "count": 100 }
```

1. See `{` → we're deserializing `Outer`, push frame for `Outer`
2. See `"name": "test"` → set `outer.name`
3. See `"inner": {` → push frame for `Inner`
4. See `"x": 42` → set `inner.x`
5. See `"y": "hello"` → set `inner.y`
6. See `}` → pop `Inner` frame (all fields set, validation passes!)
7. See `"count": 100` → set `outer.count`
8. See `}` → pop `Outer` frame (all fields set, done!)

This works perfectly. `inner` is fully initialized before we pop it.
So where does interleaving come from?

### Case 1: TOML Dotted Keys

TOML allows dotted keys that can interleave nested fields:

```toml
name = "test"
inner.x = 42
count = 100
inner.y = "hello"
```

This is valid TOML! All four lines are at the root level, but `inner.x` and
`inner.y` both write to the nested `inner` struct. A streaming parser sees:

1. `name = "test"` → set `outer.name`
2. `inner.x = 42` → push `inner` frame, set `x`... but we can't pop yet!
3. `count = 100` → need to set `outer.count`, must leave `inner` first
4. `inner.y = "hello"` → back to `inner`, set `y`

In strict mode, step 3 requires popping the `inner` frame to get back to
`outer`, but `inner.y` isn't set yet. We're stuck.

This is why TOML parsers typically deserialize to an intermediate tree (a DOM,
document object model) first—they can't know if a table is truly "done" or if
more dotted keys will add to it later. Building a DOM avoids the problem, but
it means extra allocations and no streaming. Deferred mode lets us initialize
in place without that intermediate step.

### Case 2: Flattened Structs

With `#[facet(flatten)]`, nested fields appear at the same level:

```rust
struct Outer {
    name: String,
    #[facet(flatten)]
    inner: Inner,
    count: u64,
}

struct Inner {
    x: u32,
    y: String,
}
```

Now valid JSON looks like:

```json
{
  "name": "test",
  "x": 42,
  "count": 100,
  "y": "hello"
}
```

All four fields are siblings! The parser sees them in document order:
`name`, `x`, `count`, `y`. But `x` and `y` both belong to `inner`.

From facet-reflect's perspective:

1. `outer.name = "test"`
2. `outer.inner.x = 42` (push inner frame, set x)
3. `outer.count = 100` (need to pop inner frame first!)
4. `outer.inner.y = "hello"` (push inner frame again)

Step 3 requires popping the `inner` frame to get back to `outer`, but
`inner.y` isn't set yet. Strict mode fails.

### The Pattern

Both cases produce the same fundamental problem: **sibling fields of a
nested struct are interleaved with fields of the parent**. The frame stack
model assumes depth-first traversal, but real-world formats don't guarantee it.

## The Solution: Deferred Mode

Deferred mode relaxes the "fully initialized on pop" rule. Instead:

1. **Frames are stored, not discarded** - When you pop a frame, it's saved
   (keyed by its path) rather than validated and dropped.

2. **Frames are restored on re-entry** - When you navigate back to the same
   path, the stored frame is retrieved, preserving all its state.

3. **Validation happens at the end** - A final `finish_deferred()` call
   validates that everything is properly initialized.

### Why Store Entire Frames?

We store entire `Frame`s, not just `Tracker`s, because frames may own memory:

```rust
enum FrameOwnership {
    Owned,           // Frame allocated this memory - can't lose it!
    Field,           // Pointer into parent's allocation
    ManagedElsewhere // Memory managed by something else
}
```

If `ownership == Owned`, the frame's `data` pointer references heap memory.
Storing only the tracker would leak that memory.

### Data Structures

```rust
enum PartialMode {
    /// Normal mode: validate on each pop
    Strict,

    /// Deferred mode: store frames, validate at finish
    Deferred {
        /// Expected structure (from facet-solver)
        resolution: Resolution,

        /// Current path as we navigate (e.g., ["inner", "x"])
        current_path: KeyPath,  // Vec<&'static str>

        /// Frames saved when popped, restored when re-entered
        stored_frames: BTreeMap<KeyPath, Frame>,
    },
}
```

### The Flow

**Entering a field** (`begin_field("inner")`):
1. Push `"inner"` onto `current_path`
2. Look up `current_path` in `stored_frames`
3. If found: take that frame, push onto stack (restoring previous state)
4. If not found: create fresh frame as usual

**Leaving a field** (`end()`):
1. Pop frame from stack
2. Store frame at `current_path` (preserving state for potential re-entry)
3. Pop last segment from `current_path`

**Finishing** (`finish_deferred()`):
1. Walk all stored frames
2. Verify all required fields are initialized
3. Apply defaults for fields with `#[facet(default)]`
4. Initialize `Option<T>` fields to `None` if not set
5. Report errors for any missing required fields

## What Can Be Re-entered?

Everything. That's the whole point.

The frames are preserved with all their state, including heap allocations
used as scratch space:

| Type | What's Preserved |
|------|------------------|
| Struct | ISet tracking which fields are set |
| Enum | Selected variant + field ISet |
| Array | ISet tracking which indices are set |
| List (Vec, etc.) | The Vec being built |
| Map | The map being built + any pending key |
| `Arc<[T]>`, `Box<[T]>` | The Vec being built (transformation deferred!) |

**Example: YAML with interleaved list items**

```yaml
items:
  - first
other_field: 42
items:
  - second
```

With deferred mode, we push "first", exit to set `other_field`, then re-enter
`items` and push "second". The Vec is preserved across the exit.

### Deferred vs Eager Materialization

In strict mode, some types transform on pop. For example, `Arc<[T]>` is built
using a `Vec<T>` as scratch space, then converted to an `Arc<[T]>` when you
call `end()`. Once that transformation happens, you can't go back.

In deferred mode, we *defer* that transformation. The `Vec` stays a `Vec`.
You can re-enter and push more items. Only when you call `finish_deferred()`
does the actual `Vec→Arc<[T]>` transformation happen.

This is why it's called deferred *materialization*—we defer the final
materialization of these types until the very end.

## Example

```rust
let mut partial = Partial::alloc::<Outer>()?;
partial.begin_deferred(resolution);

// Set outer.name
partial.set_field("name", "test")?;

// Enter inner, set x
partial.begin_field("inner")?;    // path: ["inner"]
partial.set_field("x", 42)?;       // inner.iset now has bit 0 set
partial.end()?;                    // frame stored at ["inner"], path: []

// Set outer.count (we left inner with y unset - that's ok in deferred mode!)
partial.set_field("count", 100)?;

// Re-enter inner, set y
partial.begin_field("inner")?;    // frame restored! iset still has bit 0
partial.set_field("y", "hello")?;  // now iset has bits 0 and 1
partial.end()?;                    // frame stored again

// Validate everything is initialized
partial.finish_deferred()?;

let result = partial.build()?;
```
