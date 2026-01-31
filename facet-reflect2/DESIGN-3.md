# facet-reflect2 Design (Revision 3)

Synthesis of DESIGN.md's explicit frame management with DESIGN-2's unified path model.

## Core Insight

Navigation and "where to put it" are the same thing. `Append` and `Insert(key)` aren't separate operations—they're path segments that specify *where* in a container the value goes.

## API

```rust
enum PathSegment {
    Field(u32),      // struct field, tuple element, array index, enum variant
    Append,          // new list/set element
    Insert(Imm),     // new map entry by key
    Root,            // jump to root frame (for non-recursive deserializers)
}

type Path = &[PathSegment];

enum Source {
    Imm(Imm),        // copy bytes from existing value
    Stage,           // push a frame for incremental construction
    Default,         // call Default::default() in place
}

enum Op {
    Set { dst: Path, src: Source },
    End,
}
```

Two operations. That's it.

- **Set** navigates to a destination and writes a value (immediate, staged, or default)
- **End** pops the current frame back to its parent

## Semantics

### Paths

A path is relative to the current frame. Each segment navigates one level:

| Segment | Meaning |
|---------|---------|
| `Field(n)` | Field/element `n` of current frame (struct field, array element, enum variant) |
| `Append` | New element at end of list/set |
| `Insert(key)` | Map entry with given key |
| `Root` | Jump to root frame, regardless of current position |

### Multi-Level Paths

Paths can have multiple segments: `&[Field(0), Field(1)]` means "field 1 of field 0."

**Rule: multi-level paths implicitly create frames for all intermediate segments.**

Every segment except the last creates a frame. This is how we track partial initialization—frames are the bookkeeping mechanism. Without them, we wouldn't know which fields are set and which aren't.

```rust
struct Outer { inner: Inner }
struct Inner { x: i32, y: i32 }

// This path:
Set { dst: &[Field(0), Field(1)], src: Imm(&20) }

// Is equivalent to:
Set { dst: &[Field(0)], src: Stage }    // create frame for inner
Set { dst: &[Field(1)], src: Imm(&20) } // set inner.y
// cursor is now at the Inner frame
```

The same applies to dynamic containers:

```rust
struct Config { servers: Vec<Server> }
struct Server { host: String, port: u16 }

// This path:
Set { dst: &[Field(0), Append, Field(0)], src: Imm(&host) }

// Is equivalent to:
Set { dst: &[Field(0)], src: Stage }     // frame for servers list
Set { dst: &[Append], src: Stage }       // frame for new Server
Set { dst: &[Field(0)], src: Imm(&host) } // set host
// cursor is now at the Server frame
```

After a multi-level path, the cursor is at the deepest frame created. You need `End` calls to pop back up.

### Root

`Root` jumps to the root frame, regardless of the current cursor position. This enables **non-recursive deserializers** like TOML that don't naturally track their position.

```toml
[server]
host = "localhost"

[database]
url = "postgres://..."

[server]    # re-opens server section!
port = 8080
```

Without `Root`, a TOML deserializer would need to:
- Track its current position in the frame tree
- Emit `End` calls to navigate back to root before each section
- Or buffer everything and emit ops in a different order

With `Root`, each section header maps directly to a path:

```rust
// [server]
// host = "localhost"
Set { dst: &[Root, Field(0), Field(0)], src: Imm(&host) }   // server.host

// [database]
// url = "postgres://..."
Set { dst: &[Root, Field(1), Field(0)], src: Imm(&url) }    // database.url

// [server] - re-entry!
// port = 8080
Set { dst: &[Root, Field(0), Field(1)], src: Imm(&port) }   // server.port
```

The deserializer stays stateless. Each `[section]` translates to `Root, Field(section_idx), ...` and the frame tree handles re-entry automatically (in deferred mode).

**Note**: `Root` should typically appear at the start of a path. `&[Field(0), Root, Field(1)]` is valid but unusual—it would enter field 0, then jump back to root, then enter field 1.

### Source

| Source | Effect |
|--------|--------|
| `Imm(value)` | Copy bytes from an existing value. Caller must `mem::forget` the source. |
| `Stage` | Push a new frame. Subsequent ops target that frame until `End`. |
| `Default` | Call the type's `Default` in place. No frame created. |

### End

`End` pops the current frame back to its parent.

- In strict mode: the frame must be complete (all required fields initialized) or `End` returns an error
- In deferred mode: incomplete frames are stored for later re-entry

After `End`, the parent's tracking for that child changes from "in-progress" to "complete."

### Cursor Position

The cursor (`current`) always points to exactly one frame. Operations are relative to it.

- `Set { src: Stage }` pushes a new frame; cursor moves to it
- Multi-level paths push frames for intermediates; cursor ends at deepest frame
- `End` pops current frame; cursor moves to parent
- `Set { src: Imm | Default }` with single-segment path writes a value; cursor stays where it is

You always know where the cursor is by tracking frame creation and `End` operations.

## Frame Lifecycle

Frames are transient bookkeeping, not permanent overhead.

### Creation

A frame is created when:
- `Set { src: Stage }` is called
- A multi-level path navigates through intermediate segments

Each frame tracks:
- Pointer to the memory being constructed
- Shape (type metadata)
- Which children are initialized (bitset for structs, count for lists, etc.)
- Parent frame reference

### Tracking

As children complete, the frame updates its tracking:
- Struct: bitset flips for each field
- List/Set: element count increments
- Map: entry count increments
- Enum: variant selection + variant data status

### Collapse

When all children of a frame are complete, the frame can be **collapsed**:

1. The frame detects it's fully initialized
2. Child frames are detached and freed
3. The parent records "this child is complete" (not "this child has frame X")

This keeps memory bounded. Even deeply nested structures don't accumulate frames—once a subtree is complete, its frames evaporate. The parent just knows "done."

```
Before collapse:

    ┌─────────────┐
    │ Line        │   fields: [→frame1, →frame2]
    └──────┬──────┘
           │
     ┌─────┴─────┐
     ▼           ▼
┌─────────┐ ┌─────────┐
│ Point   │ │ Point   │
│ [✓, ✓]  │ │ [✓, ✓]  │
└─────────┘ └─────────┘

After collapse:

    ┌─────────────┐
    │ Line        │   fields: [✓, ✓]  ← complete, no child frames
    └─────────────┘
```

### Why Frames Are Cheap

1. **Short-lived**: Frames exist only while their subtree is being built
2. **Bounded depth**: At most O(depth) frames exist at once, not O(nodes)
3. **Reusable**: Arena allocation with free list means no repeated allocations
4. **Collapse eagerly**: As soon as a frame completes, it's freed

The "optimization" of avoiding frames for simple structs happens naturally—you create the frame, set all fields, frame detects completion and collapses. No manual optimization needed.

## Examples

### Scalars

```rust
Set { dst: &[], src: Imm(&42u32) }
```

Empty path means "current frame itself." No child frames created.

### Structs

```rust
struct Point { x: i32, y: i32 }

Set { dst: &[Field(0)], src: Imm(&10) }  // x = 10
Set { dst: &[Field(1)], src: Imm(&20) }  // y = 20
```

Single-segment paths with `Imm` don't create child frames. The root frame tracks field completion directly.

### Nested Structs

```rust
struct Line { start: Point, end: Point }

// Option A: Explicit staging
Set { dst: &[Field(0)], src: Stage }          // enter start
Set { dst: &[Field(0)], src: Imm(&0) }        // start.x
Set { dst: &[Field(1)], src: Imm(&0) }        // start.y
End                                            // back to Line, start frame collapses
Set { dst: &[Field(1)], src: Imm(&end_point) } // end = complete Point

// Option B: Multi-level paths (frames created implicitly)
Set { dst: &[Field(0), Field(0)], src: Imm(&0) }   // start.x (creates start frame)
Set { dst: &[Field(0), Field(1)], src: Imm(&0) }   // start.y (re-enters start frame)
// cursor is at start frame, need to End back
End
Set { dst: &[Field(1), Field(0)], src: Imm(&10) }  // end.x (creates end frame)
Set { dst: &[Field(1), Field(1)], src: Imm(&10) }  // end.y
End
```

Both options create the same frames. Option B just does it implicitly.

### Enums

```rust
enum Message { Quit, Move { x: i32, y: i32 }, Write(String) }

// Unit variant
Set { dst: &[Field(0)], src: Default }  // Quit

// Struct variant
Set { dst: &[Field(1)], src: Stage }    // select Move, enter it
Set { dst: &[Field(0)], src: Imm(&10) } // x
Set { dst: &[Field(1)], src: Imm(&20) } // y
End

// Tuple variant
Set { dst: &[Field(2)], src: Imm(&"hello".to_string()) }  // Write("hello")
```

`Field(n)` selects variant `n`. For variants with data, `Stage` enters the variant's fields.

### Lists

```rust
struct Config { servers: Vec<String> }

Set { dst: &[Field(0)], src: Stage }           // enter servers list
Set { dst: &[Append], src: Imm(&"server1") }   // append "server1"
Set { dst: &[Append], src: Imm(&"server2") }   // append "server2"
End                                             // back to Config
```

### Lists with Complex Elements

```rust
struct Config { servers: Vec<Server> }
struct Server { host: String, port: u16 }

// Explicit staging
Set { dst: &[Field(0)], src: Stage }           // enter servers list
Set { dst: &[Append], src: Stage }             // append new Server, enter it
Set { dst: &[Field(0)], src: Imm(&host) }      // host
Set { dst: &[Field(1)], src: Imm(&port) }      // port
End                                             // back to list, Server frame collapses
End                                             // back to Config

// Or with multi-level paths
Set { dst: &[Field(0), Append, Field(0)], src: Imm(&host) }  // creates list + Server frames
Set { dst: &[Field(1)], src: Imm(&port) }                     // still in Server frame
End                                                            // back to list
End                                                            // back to Config
```

### Maps

```rust
struct Config { env: HashMap<String, String> }

Set { dst: &[Field(0)], src: Stage }                       // enter env map
Set { dst: &[Insert(key_path)], src: Imm(&"/usr/bin") }    // PATH = "/usr/bin"
Set { dst: &[Insert(key_home)], src: Imm(&"/home/user") }  // HOME = "/home/user"
End                                                         // back to Config
```

### Maps with Complex Values

```rust
struct Config { servers: HashMap<String, Server> }

Set { dst: &[Field(0)], src: Stage }               // enter servers map
Set { dst: &[Insert(key_primary)], src: Stage }    // insert "primary", enter value
Set { dst: &[Field(0)], src: Imm(&host) }          // host
Set { dst: &[Field(1)], src: Imm(&port) }          // port
End                                                 // back to map
End                                                 // back to Config

// Or with multi-level path
Set { dst: &[Field(0), Insert(key_primary), Field(0)], src: Imm(&host) }
Set { dst: &[Field(1)], src: Imm(&port) }
End  // back to map
End  // back to Config
```

### Arrays

```rust
struct Point3D { coords: [f32; 3] }

// Explicit staging
Set { dst: &[Field(0)], src: Stage }       // enter coords
Set { dst: &[Field(0)], src: Imm(&1.0) }   // coords[0]
Set { dst: &[Field(1)], src: Imm(&2.0) }   // coords[1]
Set { dst: &[Field(2)], src: Imm(&3.0) }   // coords[2]
End

// Multi-level paths
Set { dst: &[Field(0), Field(0)], src: Imm(&1.0) }  // coords[0], creates array frame
Set { dst: &[Field(1)], src: Imm(&2.0) }            // coords[1], still in array frame
Set { dst: &[Field(2)], src: Imm(&3.0) }            // coords[2]
End
```

### Sets

```rust
struct Post { tags: HashSet<String> }

Set { dst: &[Field(0)], src: Stage }          // enter tags set
Set { dst: &[Append], src: Imm(&"rust") }     // add "rust"
Set { dst: &[Append], src: Imm(&"facet") }    // add "facet"
End
```

Sets use `Append` like lists. The implementation hashes and inserts.

**Important**: Set elements cannot use `Stage`. They have no identity until hashed. You must provide a complete value via `Imm` or `Default`.

### Option

```rust
struct Config { timeout: Option<u32> }

// None
Set { dst: &[Field(0)], src: Default }

// Some with immediate value
Set { dst: &[Field(0)], src: Imm(&Some(30u32)) }

// Some with complex inner
Set { dst: &[Field(0)], src: Stage }       // enter Some(T)
Set { dst: &[Field(0)], src: Imm(&host) }  // T's field 0
Set { dst: &[Field(1)], src: Imm(&port) }  // T's field 1
End
```

When you `Stage` into an Option, you're building the `Some(T)`—fields are `T`'s fields.

### Smart Pointers

```rust
struct Config { data: Box<Point> }

Set { dst: &[Field(0)], src: Stage }       // allocate staging memory
Set { dst: &[Field(0)], src: Imm(&10) }    // x
Set { dst: &[Field(1)], src: Imm(&20) }    // y
End                                         // calls Box::new, moves value
```

`Stage` allocates temporary memory for the pointee. `End` wraps it in the pointer type.

## Building the Final Value

When construction is complete, call `build()` to extract the final value.

```rust
let value: T = partial.build()?;
```

### Auto-Navigation to Root

`build()` automatically navigates from the current cursor position back to root. You don't need to manually `End` all the way up:

```rust
Set { dst: &[Field(0), Append, Field(0)], src: Imm(&host) }
Set { dst: &[Field(1)], src: Imm(&port) }
// cursor is at Server frame, inside list, inside Config

let config: Config = partial.build()?;  // auto-pops all frames to root
```

This is equivalent to calling `End` repeatedly until reaching root, then extracting the value.

### Validation

In strict mode, `build()` validates that every frame along the path to root is complete. If any frame is incomplete, it returns an error.

In deferred mode, `build()` validates the entire tree—all deferred frames must be complete by the time `build()` is called.

## Deferred Mode

In strict mode (default), `End` requires the frame to be complete. In deferred mode, incomplete frames are stored for later completion.

### Why Deferred Mode

JSON object fields can arrive in any order. Without deferred mode, you'd need to buffer the whole object and sort by field index. With deferred mode:

```rust
// JSON: {"y": 20, "x": 10}

Set { dst: &[Field(0)], src: Stage }   // enter struct
Set { dst: &[Field(1)], src: Imm(&20) } // see "y" first, set it
End                                     // incomplete, but stored

// ... later ...
// re-enter the stored frame
Set { dst: &[Field(0)], src: Imm(&10) } // see "x", set it
End                                     // now complete
```

### Re-entry Semantics

When a path navigates to a location that already has an incomplete frame:
- **Strict mode**: Error (can't re-enter)
- **Deferred mode**: Re-enter the existing frame instead of creating a new one

Re-entry works by:
- **Struct fields**: Field index
- **Enum variants**: Variant index (after selection)
- **List elements**: Element index
- **Map values**: Key lookup

This means multi-level paths in deferred mode will re-enter existing frames as needed:

```rust
// First access creates frames
Set { dst: &[Field(0), Field(1)], src: Imm(&20) }  // creates Outer frame, sets inner.y
End  // Outer frame incomplete (inner.x not set), stored

// Second access re-enters
Set { dst: &[Field(0), Field(0)], src: Imm(&10) }  // re-enters Outer frame, sets inner.x
End  // now complete, frame collapses
```

### What Cannot Be Deferred

Set elements have no identity until hashed. You can't partially build a set element, `End`, then come back—there's no key to find it by. Set elements must complete immediately, even in deferred mode.

## Comparison with Previous Designs

### vs DESIGN.md

DESIGN.md had three write operations: `Set`, `Push`, `Insert`. This design has one: `Set` with path segments that include `Append` and `Insert(key)`.

Renamed:
- `Build` → `Stage` (clearer intent)
- `Push` → `Append` (path segment, not operation)

Added:
- Multi-level paths with implicit frame creation
- Explicit frame lifecycle documentation

Same:
- Explicit `End` for frame management
- Deferred mode semantics

### vs DESIGN-2.md

DESIGN-2.md unified operations but introduced ambiguity about cursor position with `Up` navigation in paths.

This design keeps the unified path model and adopts the good parts:
- `Root` segment for non-recursive deserializers (TOML)
- `build()` auto-navigates to root

But retains explicit `End` for clarity:
- No `Up` in paths—use `End` to pop frames
- Frames are created implicitly by paths, but popping is explicit
- Cursor position is always determinable
