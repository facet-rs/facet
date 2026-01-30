# Partial API v2 Design Notes

## What do we need to track?

1. **Allocations** - avoid use-after-free, double-free, leaks
2. **Granular initialization state** - for correct drop while partially initialized (must drop initialized fields, not uninitialized ones)
3. **Validation** - facet-validate attrs (min/max, regex, custom validators)

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
1. Begin inner       2. Set a            3. End (store)       4. Set other
   deferred: true                           frame by path
   ┌─────────┐          ┌─────────┐                              ┌─────────┐
   │  Inner  │          │  Inner  │       stored["inner"]        │  Outer  │
   │ a: ?    │          │ a: 1 ✓  │        = Inner{a:1}          │other:hi✓│
   │ b: ?    │          │ b: ?    │                              └─────────┘
   └─────────┘          └─────────┘
   ┌─────────┐          ┌─────────┐          ┌─────────┐
   │  Outer  │          │  Outer  │          │  Outer  │
   └─────────┘          └─────────┘          └─────────┘


5. Re-enter inner    6. Set b            7. End               
   (restore frame)                          validates, complete!

   ┌─────────┐          ┌─────────┐          ┌─────────┐
   │  Inner  │          │  Inner  │          │  Outer  │
   │ a: 1 ✓  │          │ a: 1 ✓  │          │inner: ✓ │
   │ b: ?    │          │ b: 2 ✓  │          │other: ✓ │
   └─────────┘          └─────────┘          └─────────┘
   ┌─────────┐          ┌─────────┐
   │  Outer  │          │  Outer  │
   └─────────┘          └─────────┘
```

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
