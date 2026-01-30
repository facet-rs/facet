# Partial API v2 - Revised Operation Design

## Core insight

Most operations should decide whether to push a frame based on their **payload**, not the operation type.

## Source enum

```rust
/// A complete value to move into the destination
struct Move {
    ptr: *const (),
    shape: &'static Shape,
}

/// Build incrementally - pushes a frame
struct Build {
    len_hint: Option<usize>,
}

enum Source {
    /// Move a complete value from ptr into destination
    Move(Move),
    
    /// Build incrementally - pushes a frame
    Build(Build),
    
    /// Use the type's default value (empty collections, None, #[facet(default)] fields)
    Default,
}
```

For `Move`: move the bytes from ptr into destination, mark as initialized. No frame pushed. The source is consumed (caller must not drop it).

For `Build`: push a frame. Subsequent operations are relative to this frame until `End`.

For `Default`: call the type's default vtable function. No frame pushed. Errors if the type has no default.

The `len_hint` is used when entering collections (Vec, HashMap, etc.) - it allows pre-allocation. Ignored for non-collections.

If the caller needs to clone a value, that's their responsibility - clone first, then move the clone.

## Operations

### Set

The workhorse operation. Sets a value at a path relative to the current frame.

```rust
Set { path: &[usize], source: Source }
```

### End

Pops the current frame. Validates completeness (unless deferred - see below).

```rust
End
```

### Push

Append an element to a list or set.

```rust
Push { source: Source }
```

**When `source` is `Move`**: Element is complete, add it directly.

**When `source` is `Build`**: Push a frame for building the element. `End` completes and appends/inserts.

For lists, the implementation decides whether to use direct-fill or staging:

- **Direct-fill** (Vec, VecDeque - has `reserve`/`set_len`):
  1. `reserve(len + 1)`
  2. Frame points into `vec.as_ptr().add(len)`
  3. On `End`: `set_len(len + 1)`

- **Staging** (LinkedList, etc. - only has `push`):
  1. Allocate staging buffer
  2. Frame points at staging
  3. On `End`: call `push` vtable, deallocate staging

For sets, always uses staging (build element, then insert).

The caller doesn't know or care which path is taken.

### Insert

Insert a key-value pair into a map.

```rust
Insert { key: Move, value: Source }
```

The key must always be complete (`Move`) - this enables deferred map entries. If we need to defer mid-value, we can store the incomplete value frame keyed by the (complete) key and re-enter later.

The value can be `Move` (complete) or `Build` (incremental).

## Examples

### Scalar

```rust
Set { path: &[], source: Source::Move(Move { ptr: &42u32, shape: <u32>::SHAPE }) }
```

### Default values

```rust
// Empty Vec
Set { path: &[0], source: Source::Default }

// Option::None
Set { path: &[1], source: Source::Default }

// Struct field with #[facet(default)]
Set { path: &[2], source: Source::Default }
```

### Struct (field by field)

```rust
// struct Point { x: i32, y: i32 }
Set { path: &[0], source: Source::Move(...) }  // x
Set { path: &[1], source: Source::Move(...) }  // y
```

### Nested struct (incremental)

```rust
// struct Line { start: Point, end: Point }
Set { path: &[0], source: Source::Build(Build { len_hint: None }) }  // start - pushes frame
  Set { path: &[0], source: Source::Move(...) }  // start.x
  Set { path: &[1], source: Source::Move(...) }  // start.y
End
Set { path: &[1], source: Source::Build(Build { len_hint: None }) }  // end - pushes frame
  Set { path: &[0], source: Source::Move(...) }  // end.x
  Set { path: &[1], source: Source::Move(...) }  // end.y
End
```

### Nested struct (you have the whole thing)

```rust
// If you already have a Point value
Set { path: &[0], source: Source::Move(Move { ptr: &start_point, shape: <Point>::SHAPE }) }
Set { path: &[1], source: Source::Move(Move { ptr: &end_point, shape: <Point>::SHAPE }) }
```

### Enum (variant selection via path)

```rust
// enum Message { Quit, Move { x: i32, y: i32 }, Write(String) }

// Set Message::Move { x: 10, y: 20 }
Set { path: &[1], source: Source::Build(Build { len_hint: None }) }  // select variant 1, push frame
  Set { path: &[0], source: Source::Move(...) }  // x
  Set { path: &[1], source: Source::Move(...) }  // y
End
```

### List (Vec)

```rust
// Vec<Server> where Server { host: String, port: u16 }

// Enter Vec field - len_hint enables pre-allocation
Set { path: &[0], source: Source::Build(Build { len_hint: Some(2) }) }
  
  // Element 0 - built incrementally
  Push { source: Source::Build(Build { len_hint: None }) }
    Set { path: &[0], source: Source::Move(...) }  // host
    Set { path: &[1], source: Source::Move(...) }  // port
  End
  
  // Element 1 - have the whole thing
  Push { source: Source::Move(Move { ptr: &server2, shape: <Server>::SHAPE }) }
End
```

### Set (HashSet)

```rust
// HashSet<Server> where Server { host: String, port: u16 }

// Enter HashSet field
Set { path: &[0], source: Source::Build(Build { len_hint: Some(2) }) }
  
  // Element 0 - built incrementally
  Push { source: Source::Build(Build { len_hint: None }) }
    Set { path: &[0], source: Source::Move(...) }  // host
    Set { path: &[1], source: Source::Move(...) }  // port
  End
  
  // Element 1 - have the whole thing
  Push { source: Source::Move(Move { ptr: &server2, shape: <Server>::SHAPE }) }
End
```

### Map (HashMap)

```rust
// HashMap<String, Server>

// Enter HashMap field
Set { path: &[0], source: Source::Build(Build { len_hint: Some(2) }) }

  // Entry 1 - value built incrementally
  Insert { 
    key: Move { ptr: &"server1", shape: <String>::SHAPE },
    value: Source::Build(Build { len_hint: None })
  }
    Set { path: &[0], source: Source::Move(...) }  // host
    Set { path: &[1], source: Source::Move(...) }  // port
  End

  // Entry 2 - complete value
  Insert {
    key: Move { ptr: &"server2", shape: <String>::SHAPE },
    value: Source::Move(Move { ptr: &server2, shape: <Server>::SHAPE })
  }
End
```

## Deferred frames

For `#[facet(flatten)]` support, frames can be deferred - left incomplete and re-entered later.

```rust
struct Build {
    len_hint: Option<usize>,
    deferred: bool,
}
```

When `deferred: true`, an incomplete frame at `End` is stored (by path for structs, by key for maps) and can be re-entered later. When `deferred: false`, an incomplete frame at `End` is an error.

For maps, deferred entries are keyed by the (complete) key. Re-entering looks up the incomplete value frame by key.

## Arrays

Fixed-size `[T; N]`. Use `Set` with path index - no `Push` since size is fixed:

```rust
// [i32; 3]
Set { path: &[0], source: Source::Move(...) }
Set { path: &[1], source: Source::Move(...) }
Set { path: &[2], source: Source::Move(...) }
```

## Error handling

On any error, the Partial is poisoned. All initialized fields are dropped, all allocations are freed, and you're done. No partial recovery, no "undo just this operation".

This keeps the implementation simple and matches fail-fast semantics.
