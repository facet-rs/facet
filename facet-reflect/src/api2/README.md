# Partial API v2 Design Notes

## What is Partial?

Partial is a type-erased builder for any type that derives `Facet`. You give it a shape (runtime type info), and it lets you construct values field by field, element by element, without knowing the concrete type at compile time.

This is what powers format parsers: `facet-json` reads JSON tokens, calls Partial methods, and out comes a fully-initialized `T`.

### Why is this hard?

Let's build up from simple to complex.

#### Scalars

A `u32` is either initialized or not. One bit of tracking.

```
u32: ✓ or ✗
```

Easy. Write the bytes, mark as initialized. Drop might be a no-op (primitives) or might do real work (`String`, `PathBuf`, or any type with heap allocations).

#### Structs

A struct with N fields needs N bits - one per field.

```rust
struct Point { x: i32, y: i32 }
```

```
Point
├── x: i32   ✓ or ✗
└── y: i32   ✓ or ✗
```

We use an `ISet` (initialization set) - a bitset that tracks which fields are done. For structs with ≤64 fields, this fits in a single `u64`.

On drop, we iterate fields in declaration order, dropping only the initialized ones.

#### Tuples

Same as structs - just fields without names.

```
(String, i32, bool)
├── .0: String   ✓ or ✗
├── .1: i32      ✓ or ✗
└── .2: bool     ✓ or ✗
```

#### Enums

Enums are trickier. First you must select a variant, then initialize its payload.

```rust
enum Message {
    Quit,                      // variant 0
    Move { x: i32, y: i32 },   // variant 1
    Write(String),             // variant 2
}
```

The variant index is part of the path. To set `Message::Move { x: 10, y: 20 }`:

```
Begin { path: &[1] }      // select variant 1 (Move)
Set { path: &[0], ... }   // x
Set { path: &[1], ... }   // y
End
```

When you `Begin` into a variant:
1. The discriminant is written to memory (e.g., `1` for `Move`)
2. The frame tracks: which variant is selected + ISet for its payload fields

The variant selection **persists** even after End. This is part of the enum's state, not just navigation.

**Switching variants**: If you Begin a different variant, the previous payload (if any) must be dropped first. This is destructive - you can't have a "partial Move" and then switch to Write without cleaning up.

```
Message (tracking state)
├── selected_variant: None | 0 | 1 | 2
└── payload_iset: (depends on variant)
    - variant 0 (Quit): nothing to track
    - variant 1 (Move): ISet for {x, y}
    - variant 2 (Write): single value tracking
```

#### Options

`Option<T>` is really just an enum: `None` or `Some(T)`.

```
Option<String>
├── variant: None | Some
└── payload: (if Some) String ✓ or ✗
```

#### Results

Same pattern: `Ok(T)` or `Err(E)`.

```
Result<String, io::Error>
├── variant: Ok | Err
└── payload: String or io::Error
```

#### Lists (Vec, VecDeque, etc.)

Lists have two completely different paths for adding elements.

##### Push path

Used for: linked lists, or any collection without contiguous storage.

```
                    ┌──────────────┐
                    │ staging buf  │
                    │ ┌──────────┐ │
                    │ │ Server   │ │  ← build here first
                    │ │ host: ✓  │ │
                    │ │ port: ✓  │ │
                    │ └──────────┘ │
                    └──────┬───────┘
                           │ push (move)
                           ▼
┌─────────────────────────────────────┐
│ Vec<Server>                         │
│ [0]: Server ✓  [1]: Server ✓  ...   │
└─────────────────────────────────────┘
```

1. Allocate a temporary staging buffer
2. Build the element completely in staging
3. Call `push` vtable - moves from staging into collection
4. Deallocate staging buffer

The collection only ever contains fully-initialized elements. If we fail while building in staging, we drop the partial staging buffer - the collection is untouched.

##### Direct-fill path

Used for: Vec, VecDeque, anything with contiguous storage and a `reserve`/`set_len` API.

```
┌─────────────────────────────────────────────────────┐
│ Vec<Server>  (len=2, cap=4)                         │
│                                                     │
│ [0]: Server ✓  [1]: Server ✓  [2]: ????  [3]: ????  │
│                                    ▲                │
│                                    │                │
│                            building here!           │
│                            ┌──────────┐             │
│                            │ Server   │             │
│                            │ host: ✓  │             │
│                            │ port: ✗  │ ← partial!  │
│                            └──────────┘             │
└─────────────────────────────────────────────────────┘
```

1. `reserve(len + 1)` - ensure capacity
2. Get pointer to `vec.as_ptr().add(len)` - the slot past the end
3. Build element directly in that slot
4. On success: `set_len(len + 1)`
5. On failure: drop partial element, do NOT call `set_len`

This is faster (no intermediate copy) but more dangerous. The element is being built inside the Vec's buffer but the Vec doesn't "know" about it yet (len hasn't changed). If we panic or error:
- We must drop initialized fields of the partial element
- We must NOT call `set_len` (element isn't complete)
- The Vec's drop will deallocate the buffer (safe - it only drops `[0..len]`)

##### Tracking for lists

```
List frame:
├── collection initialized? ✓ (empty Vec is valid)
├── current_element: Option<ElementFrame>
│   └── if Some: tracking the in-progress element
└── (elements already in collection are owned by the collection)
```

We don't track individual elements with a bitset - once pushed/set_len'd, they're the collection's responsibility. We only track the *currently building* element.

#### Maps (HashMap, BTreeMap, etc.)

Maps need TWO temporary values: key and value.

```
HashMap<String, Server>
├── {"localhost": Server} ✓
├── {"remote": Server}    ✓
└── (currently building):
    ├── key: String     ✓
    └── value: Server
        ├── host: String ✓
        └── port: u16    ✗  ← partial!
```

Both key and value must be fully initialized before insertion. We build them in temporary buffers, then call the map's insert.

#### Smart Pointers (Box, Arc, Rc)

These allocate their inner value on the heap.

```
Box<Config>
└── inner: Config
    ├── name: String ✓
    └── port: u16    ✗
```

The Box itself isn't "initialized" until its inner value is complete. If we fail partway, we must drop the partial inner value AND deallocate the Box's heap memory.

#### Nested complexity

Now combine everything:

```rust
struct Config {
    name: String,
    servers: Vec<Server>,
    timeout: Option<Duration>,
}
```

```
Config
├── name: String           ✓
├── servers: Vec<Server>   ✓ (vec initialized)
│   ├── [0]: Server        ✓
│   └── [1]: Server        partial!
│       ├── host: String   ✓
│       └── port: u16      ✗
└── timeout: Option        ✗ (no variant selected yet)
```

If we error here, cleanup must:
1. Drop `servers[1].host` (initialized String)
2. NOT touch `servers[1].port` (uninitialized)
3. Drop `servers[0]` entirely
4. Drop the Vec (deallocates buffer, drops elements)
5. Drop `name`
6. NOT touch `timeout`
7. Deallocate Config

This is why Partial exists: **tracking all of this so cleanup is always correct**.

### Summary: what we track

1. **Allocations** - top-level T, Vec buffers, HashMap buckets, Box/Arc/Rc heap allocations, temporary buffers for map keys/values
2. **Initialization state** - per-field bitsets for structs, variant selection for enums, element tracking for collections
3. **Completeness** - before returning a finished T, verify everything is initialized (separate from facet-validate attribute validation)

### Defaults: the escape hatch

Not everything needs explicit initialization. Partial can fill in defaults:

- `Option<T>` → defaults to `None` (implicitly, always)
- `#[facet(default)]` on a field → use `Default::default()`
- `#[facet(default)]` on a struct → all fields get their defaults
- `#[facet(default = expr)]` → custom default value

When we validate completeness, uninitialized fields with defaults get filled in automatically. Only fields WITHOUT defaults cause an error if unset.

## Why deferred? The flatten problem

```rust
struct Outer {
    #[facet(flatten)]
    inner: Inner,
    other: String,
}

struct Inner {
    a: i32,
    b: i32,
}
```

In JSON this can look like:

```json
{ "a": 1, "other": "hi", "b": 2 }
```

With `Begin { deferred: false }`, you can't handle this:

```
1. Begin inner         2. Set a              3. End inner ← BOOM!
                                                b isn't set yet!
   ┌─────────┐            ┌─────────┐
   │  Inner  │            │  Inner  │
   │ a: ?    │            │ a: 1 ✓  │
   │ b: ?    │            │ b: ?    │
   └─────────┘            └─────────┘
   ┌─────────┐            ┌─────────┐
   │  Outer  │            │  Outer  │
   └─────────┘            └─────────┘
```

You can't End because `inner` isn't complete, but you need to go back up to set `other`, then come back down to set `b`.

With `Begin { deferred: true }`, frames are stored and you can re-enter:

```
1. Begin inner       2. Set a            3. End (incomplete)  4. Set other
   deferred: true                           store frame id
   ┌─────────┐          ┌─────────┐                              ┌─────────┐
   │ Inner   │          │ Inner   │       stored[path] = id      │  Outer  │
   │ path: 0 │          │ path: 0 │       frame stays in arena   │other:hi✓│
   │ a: ?    │          │ a: 1 ✓  │                              └─────────┘
   │ b: ?    │          │ b: ?    │
   └─────────┘          └─────────┘
   ┌─────────┐          ┌─────────┐          ┌─────────┐
   │  Outer  │          │  Outer  │          │  Outer  │
   └─────────┘          └─────────┘          └─────────┘


5. Re-enter inner    6. Set b            7. End (complete)
   lookup by path,                          validates!
   push frame id
   ┌─────────┐          ┌─────────┐          ┌─────────┐
   │ Inner   │          │ Inner   │          │  Outer  │
   │ path: 0 │          │ path: 0 │          │inner: ✓ │
   │ a: 1 ✓  │          │ a: 1 ✓  │          │other: ✓ │
   │ b: ?    │          │ b: 2 ✓  │          └─────────┘
   └─────────┘          └─────────┘
   ┌─────────┐          ┌─────────┐
   │  Outer  │          │  Outer  │
   └─────────┘          └─────────┘
```

## Data structures

```rust
struct Frame {
    path: Path,           // where this frame lives
    // ... data, iset, etc.
}

struct Partial {
    arena: Arena<Frame>,
    stack: Vec<FrameId>,
    stored_frames: BTreeMap<Path, FrameId>,  // lookup by path
}
```

On `End` of a deferred incomplete frame:
1. Frame stays in arena (not deallocated)
2. `stored_frames.insert(frame.path.clone(), frame_id)`
3. Pop from stack

On `Begin` with a path that exists in `stored_frames`:
1. Look up `frame_id = stored_frames.get(&path)`
2. Push that `frame_id` back onto stack
3. Continue where we left off

## Core ops

```rust
Begin { path: &[usize], deferred: bool }
End
Set { path: &[usize], ptr: *const (), shape: &'static Shape }
```

Path is relative to current frame.

**End behavior** depends on how the frame was begun:

- `deferred: false` → validate, must be complete, pop
- `deferred: true` → if complete, validate and pop; if incomplete, store frame by path, pop (can re-enter later)

Re-entering a path that has a stored frame restores it to the stack.

## The core tension

**Set** can copy any value - scalars, structs, enums, anything. Just memcpy the bytes.

But sometimes you **want** to build in-place:
- Avoid allocating a temp buffer just to copy it later
- Write directly into the final destination (a field inside a struct inside a Vec, etc.)

Hence **Begin/End**: begin work on a nested location, let subsequent ops write there, end when done.

## Examples by type

### Scalars

```rust
// Building a u32 at root
Set { path: &[], ptr: &42u32, shape: <u32 as Facet>::SHAPE }
```

### Structs

```rust
struct Point { x: i32, y: i32 }

// Option 1: Set entire struct at once (if you have it)
Set { path: &[], ptr: &point, shape: <Point as Facet>::SHAPE }

// Option 2: Build in-place, field by field
Set { path: &[0], ptr: &10i32, ... }  // x
Set { path: &[1], ptr: &20i32, ... }  // y

// Nested struct
struct Line { start: Point, end: Point }

// Need Begin/End for each nested struct
Begin { path: &[0], deferred: false }  // start
Set { path: &[0], ... }                // start.x
Set { path: &[1], ... }                // start.y
End                                    // validates start is complete
Begin { path: &[1], deferred: false }  // end  
Set { path: &[0], ... }                // end.x
Set { path: &[1], ... }                // end.y
End                                    // validates end is complete
```

### Lists

Two paths depending on what the list supports:

**Direct-fill** (Vec-like with contiguous storage):
1. Reserve capacity
2. Get pointer to `vec.as_ptr().add(len)`
3. Construct in-place there
4. `set_len(len + 1)`

```rust
Begin { path: &[0], deferred: false }  // enter the Vec field
InitList
BeginSlot { index: 0 }                 // points directly into vec's buffer
Set { path: &[], ... }                 // write the element
End                                    // done with slot
BeginSlot { index: 1 }
Set { path: &[], ... }
End
End                                    // done with list
```

**Push path** (linked lists, or when direct-fill unavailable):
1. Allocate staging buffer
2. Construct value there
3. Call `push` vtable which moves from staging
4. Dealloc staging buffer

```rust
Begin { path: &[0], deferred: false }
InitList
BeginItem                              // allocates staging
Set { path: &[], ... }
End                                    // calls push, frees staging
BeginItem
Set { path: &[], ... }
End
End
```

The format parser **must** know which path to use.

### Maps

Maps are special: build key and value in **separate Partials**, then insert.

```rust
Begin { path: &[0], deferred: false }  // enter the HashMap field
InitMap

// Build key in scratch partial, build value in scratch partial
// Then:
Insert { key_ptr: ..., value_ptr: ... }

End
```

Format parser builds key/value in scratch Partials, emits Insert with pointers to finished values. Map's insert vtable moves them in.

### Sets

Same as maps but only element:

```rust
Begin { path: &[0], deferred: false }
InitSet
Insert { element_ptr: ... }
End
```

## Open questions

### Enums

How do variant selection work with paths? Probably:
```rust
SelectVariant { index: usize }  // then fields are accessible by index
```

### Init* ops

Do we need separate `InitList`/`InitMap`/`InitSet`/`InitArray`? Or just `Init` and the shape tells us what to do?
