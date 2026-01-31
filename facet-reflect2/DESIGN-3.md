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

### Multi-Level Paths

Paths can have multiple segments: `&[Field(0), Field(1)]` means "field 1 of field 0."

**Rule: multi-level paths work only when all intermediate segments are statically laid out.**

For structs, arrays, and tuples, field access is just offset arithmetic—no allocation needed. You can write directly to nested fields:

```rust
struct Outer { inner: Inner }
struct Inner { x: i32, y: i32 }

// Direct write to nested field, no frames
Set { dst: &[Field(0), Field(1)], src: Imm(&20) }  // outer.inner.y = 20
```

But dynamic containers (Vec, HashMap, etc.) require frames because:
1. Elements aren't inline—they're heap-allocated
2. The container might be empty
3. You need `Append`/`Insert` to create slots

So paths must "stop" at dynamic containers:

```rust
struct Config { servers: Vec<Server> }

// Can't do this - Vec requires staging
Set { dst: &[Field(0), Append, Field(0)], src: Imm(&host) }  // INVALID

// Must stage the list first
Set { dst: &[Field(0)], src: Stage }        // enter servers list
Set { dst: &[Append], src: Stage }          // create element, enter it
Set { dst: &[Field(0)], src: Imm(&host) }   // set field
End                                          // back to list
End                                          // back to Config
```

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
- `End` pops current frame; cursor moves to parent
- `Set { src: Imm | Default }` writes a value; cursor stays where it is

This is explicit and unambiguous. You always know where the cursor is by counting `Stage` and `End` operations.

## Examples

### Scalars

```rust
Set { dst: &[], src: Imm(&42u32) }
```

Empty path means "current frame itself." No `End` needed—scalars don't push frames.

### Structs

```rust
struct Point { x: i32, y: i32 }

Set { dst: &[Field(0)], src: Imm(&10) }  // x = 10
Set { dst: &[Field(1)], src: Imm(&20) }  // y = 20
```

No frames, no `End`. Just direct field writes.

### Nested Structs

```rust
struct Line { start: Point, end: Point }

// Option A: Stage intermediate struct
Set { dst: &[Field(0)], src: Stage }          // enter start
Set { dst: &[Field(0)], src: Imm(&0) }        // start.x
Set { dst: &[Field(1)], src: Imm(&0) }        // start.y
End                                            // back to Line
Set { dst: &[Field(1)], src: Imm(&end_point) } // end = complete Point

// Option B: Multi-level paths (no frames)
Set { dst: &[Field(0), Field(0)], src: Imm(&0) }   // start.x
Set { dst: &[Field(0), Field(1)], src: Imm(&0) }   // start.y
Set { dst: &[Field(1), Field(0)], src: Imm(&10) }  // end.x
Set { dst: &[Field(1), Field(1)], src: Imm(&10) }  // end.y
```

Option B avoids frames entirely for pure struct nesting.

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

Set { dst: &[Field(0)], src: Stage }           // enter servers list
Set { dst: &[Append], src: Stage }             // append new Server, enter it
Set { dst: &[Field(0)], src: Imm(&host) }      // host
Set { dst: &[Field(1)], src: Imm(&port) }      // port
End                                             // back to list
Set { dst: &[Append], src: Stage }             // append another Server
Set { dst: &[Field(0)], src: Imm(&host2) }
Set { dst: &[Field(1)], src: Imm(&port2) }
End
End                                             // back to Config
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
```

### Arrays

```rust
struct Point3D { coords: [f32; 3] }

// Option A: Stage the array
Set { dst: &[Field(0)], src: Stage }       // enter coords
Set { dst: &[Field(0)], src: Imm(&1.0) }   // coords[0]
Set { dst: &[Field(1)], src: Imm(&2.0) }   // coords[1]
Set { dst: &[Field(2)], src: Imm(&3.0) }   // coords[2]
End

// Option B: Multi-level paths (arrays are statically sized)
Set { dst: &[Field(0), Field(0)], src: Imm(&1.0) }  // coords[0]
Set { dst: &[Field(0), Field(1)], src: Imm(&2.0) }  // coords[1]
Set { dst: &[Field(0), Field(2)], src: Imm(&3.0) }  // coords[2]
```

Arrays are fixed-size and inline, so multi-level paths work.

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

When `Set { dst, src: Stage }` targets a path that already has an incomplete frame:
- **Strict mode**: Error (can't re-enter)
- **Deferred mode**: Re-enter the existing frame instead of creating a new one

Re-entry works by:
- **Struct fields**: Field index
- **Enum variants**: Variant index (after selection)
- **List elements**: Element index
- **Map values**: Key lookup

### What Cannot Be Deferred

Set elements have no identity until hashed. You can't partially build a set element, `End`, then come back—there's no key to find it by. Set elements must complete immediately, even in deferred mode.

## Comparison with Previous Designs

### vs DESIGN.md

DESIGN.md had three write operations: `Set`, `Push`, `Insert`. This design has one: `Set` with path segments that include `Append` and `Insert(key)`.

Renamed:
- `Build` → `Stage` (clearer intent)
- `Push` → `Append` (path segment, not operation)

Same:
- Explicit `End` for frame management
- Clear cursor position
- Deferred mode semantics

### vs DESIGN-2.md

DESIGN-2.md unified operations but introduced ambiguity about cursor position with implicit frame creation and `Up`/`Root` navigation.

This design keeps the unified path model but retains explicit `Stage`/`End` for clarity:
- No implicit frame creation—`Stage` is always explicit
- No `Up`—use `End` to pop frames
- Cursor position is always unambiguous

DESIGN-2.md's `Root` for TOML-style random access is not included here. That's a separate concern to be addressed if needed.
