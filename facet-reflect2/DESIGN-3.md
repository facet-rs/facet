# facet-reflect2 Design (Revision 3)

Synthesis of DESIGN.md's explicit frame management with DESIGN-2's unified path model.

## Core Insight

Navigation and "where to put it" are the same thing. `Append` isn't a separate operation—it's a path segment that specifies *where* in a container the value goes.

## API

```rust
enum PathSegment {
    Field(u32),      // Struct field, tuple element, array index, enum variant
    Append,          // New list element, set element, or map entry
    Root,            // Jump to root frame (for non-recursive deserializers)
}

type Path = &[PathSegment];

enum Source {
    Imm(Imm),               // Copy bytes from existing value
    Stage(Option<usize>),   // Push a frame (optional capacity hint)
    Default,                // Call Default::default() in place
}

enum Op {
    Set { dst: Path, src: Source },
    End,
}

// Function signature
fn apply(op: Op) -> Result<(), Error>;
```

## Semantics

### Paths

A path is relative to the current frame. Each segment navigates one level:

| Segment | Meaning |
|---------|---------|
| `Field(n)` | Field/element `n` of current frame. |
| `Append` | Create new element at end of collection. |
| `Root` | Jump to root frame, regardless of current position. |

### Maps and Sets (Direct Fill Strategy)

To support incremental construction, Maps and Sets are built using a **Direct Fill** strategy.

Instead of inserting into the Hash Map/Set immediately, the frame manages a **contiguous linear buffer** (like a `Vec`).
*   **Maps**: `Append` reserves a slot for a `(Key, Value)` tuple in the buffer.
*   **Sets**: `Append` reserves a slot for the **Element** in the buffer.

Because the data sits in a stable buffer during construction:
1.  **Re-entry is supported**: You can partially build a Key or Set Element, leave, and come back later using its index in the buffer.
2.  **No Hashing yet**: Keys are not hashed until the end.

**Completion**: When the container frame is popped (`End`), the buffer is converted into the target `HashMap` or `HashSet` in one go.

### Tuple Variants

`Field(n)` selects the `n`-th variant of an enum.

*   **Unit Variant**: Select it using `Source::Default` or by entering it with `Stage` and immediately `End`-ing.
*   **Struct Variant**: `Field(n)` enters the variant. Subsequent `Field(k)` targets the struct field with index `k`.
*   **Tuple Variant**: `Field(n)` enters the variant. Subsequent `Field(k)` targets the `k`-th element of the tuple.

### Multi-Level Paths

Paths can have multiple segments.
**Rule: multi-level paths implicitly create frames for all intermediate segments.**

```rust
struct Outer { inner: Inner }
struct Inner { x: i32, y: i32 }

// This path:
Set { dst: &[Field(0), Field(1)], src: Imm(&20) }

// Is equivalent to:
Set { dst: &[Field(0)], src: Stage(None) } // create frame for inner
Set { dst: &[Field(1)], src: Imm(&20) }    // set inner.y
// cursor is now at the Inner frame
```

### Root

`Root` jumps to the root frame, regardless of the current cursor position.

### Source

| Source | Effect |
|--------|--------|
| `Imm(value)` | Copy bytes. Caller must ensure safety. |
| `Stage(cap)` | Push a new frame. `cap` hints at expected element count for Lists/Maps/Sets. |
| `Default` | Call `Default::default()` in place. |

**Modifiability**: `Imm` and `Default` mark the target as initialized. However, the value remains mutable. You can overwrite fields of a "completed" value using subsequent `Set` operations. This does not require resurrecting a full tracking frame, as the parent already considers the child "initialized".

### End

`End` pops the current frame back to its parent.

- **Strict Mode**: Frame must be complete.
- **Deferred Mode**: If incomplete, frame is preserved for re-entry.

For Maps/Sets, `End` on the **container frame** triggers the bulk conversion from the staging buffer to the final collection.

## Frame Lifecycle

### Creation

A frame is created when `Set { src: Stage }` is called or a path navigates through `Append` or uninitialized fields.

### Tracking and The Staging Area

Lists, Maps, and Sets effectively act as a **Staging Area**.
*   The container frame holds a list of "Entry Frames" (children).
*   These children might be fully or partially initialized.
*   `Append` adds a new child frame.
*   The system assigns sequential indices (0, 1, 2...) to appended elements.
*   The **Caller** is responsible for tracking these indices if they intend to re-enter specific children later (using `Field(n)`).
*   Only when `build()` is called (or the container is `End`ed in Strict Mode) are these staged children finalized into the actual destination container.

### Collapse

When all children of a frame are complete, the frame can be **collapsed** (freed), and the parent marks that field as done.

**Note on Memory Usage in Deferred Mode**:
In Deferred Mode, frames cannot be eagerly collapsed if they might be re-entered. The parent must retain the child frame (tracking its partial state) until the entire tree is finalized (`build()`). This means memory usage in Deferred Mode scales with **O(Nodes)** rather than O(Depth). This is generally acceptable as the final DOM size is also O(Nodes).

## Examples

### Structs

```rust
struct Point { x: i32, y: i32 }

Set { dst: &[Field(0)], src: Imm(&10) }  // x = 10
Set { dst: &[Field(1)], src: Imm(&20) }  // y = 20
```

### Maps (Incremental Key Construction)

```rust
struct Config { routes: HashMap<Route, Handler> }
struct Route { url: String, method: String }

// 1. Enter Map (Hint: 1 element)
Set { dst: &[Field(0)], src: Stage(Some(1)) }

// 2. Append new Entry (creates (Key, Value) tuple frame)
Set { dst: &[Append], src: Stage(None) } 
// Cursor is now at the new Entry frame.

// 3. Build Key (Route) - Field(0) of Entry
Set { dst: &[Field(0), Field(0)], src: Imm(&"/api") }   // Key.url
Set { dst: &[Field(0), Field(1)], src: Imm(&"POST") }   // Key.method

// 4. Build Value (Handler) - Field(1) of Entry
Set { dst: &[Field(1)], src: Imm(&my_handler) }

// 5. Finish Entry
End 

// 6. Finish Map (Converts buffer to HashMap)
End
```

### Sets (Direct Fill)

```rust
struct Post { tags: HashSet<String> }

// 1. Enter Set
Set { dst: &[Field(0)], src: Stage(None) }

// 2. Append Element (creates String frame)
Set { dst: &[Append], src: Stage(None) }

// 3. Fill Element Directly
Set { dst: &[], src: Imm(&"rust") }
// Alternatively: Set { dst: &[Append], src: Imm(&"rust") } would do this in one step.

// 4. Finish Element
End
```

### Lists with Re-entry (Deferred Mode)

```rust
struct Config { servers: Vec<Server> }
struct Server { host: String, port: u16 }

// 1. Enter List
Set { dst: &[Field(0)], src: Stage(None) }

// 2. Start a Server (but we only know the host right now)
Set { dst: &[Append], src: Stage(None) }
let server_idx = 0; // Caller tracks that this is the 0th element

Set { dst: &[Field(0)], src: Imm(&"localhost") }
// We don't have the port yet.
End // Pop back to List. Server frame is incomplete but stored at index `server_idx`.

// ... parse other things ...

// 3. Come back to set the port
Set { dst: &[Field(server_idx)], src: Stage(None) } // Re-enter the staged Server frame
Set { dst: &[Field(1)], src: Imm(&8080) }
End // Now complete. Frame collapses.
```

### Enums (Unit and Tuple Variants)

```rust
enum Message { 
    Quit, 
    Write(String),
    Move(i32, i32) 
}

// Quit (Unit Variant)
// Option A: Use Default
Set { dst: &[Field(0)], src: Default }

// Option B: Stage and End
Set { dst: &[Field(0)], src: Stage(None) }
End

// Write("hello") (Tuple Variant)
Set { dst: &[Field(1)], src: Stage(None) }   // Select Write (Variant 1)
Set { dst: &[Field(0)], src: Imm(&"hello") } // Set 0th element of tuple
End

// Move(10, 20)
Set { dst: &[Field(2)], src: Stage(None) }   // Select Move (Variant 2)
Set { dst: &[Field(0)], src: Imm(&10) }      // 0th element (x)
Set { dst: &[Field(1)], src: Imm(&20) }      // 1st element (y)
End
```

## Building the Final Value

### Auto-Navigation to Root

`build()` automatically navigates from the current cursor position back to root.

### Validation

In strict mode, `build()` validates that every frame along the path to root is complete.
In deferred mode, `build()` validates the entire tree—all deferred frames must be complete.

## Deferred Mode

### Why Deferred Mode

JSON object fields can arrive in any order.

### Re-entry Semantics

When a path navigates to a location that already has an incomplete frame, it re-enters the existing frame.
For Lists/Maps, this uses the index (`Field(n)`) corresponding to the order of insertion/append. The caller must track this index.

### Comparison with Previous Designs

### vs DESIGN.md

DESIGN.md had `Insert`. This design unifies insertion into `Append` with Tuple semantics for Maps.

### vs DESIGN-2.md

Retains the unified path model but clarifies `Root` behavior and memory implications. Removes opaque return values; caller tracks state.