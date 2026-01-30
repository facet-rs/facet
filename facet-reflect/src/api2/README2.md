# Partial API v2 - Revised Operation Design

## Core insight

Most operations should decide whether to push a frame based on their **payload**, not the operation type.

## Value enum

```rust
enum Value<'a> {
    /// Complete value ready to copy (memcpy from ptr)
    Literal { ptr: *const (), shape: &'static Shape },
    
    /// "I'm going to build this incrementally" - pushes a frame
    Staged { len_hint: Option<usize> },
}
```

For `Literal`: copy the bytes, mark as initialized. No frame pushed.

For `Staged`: push a frame. Subsequent operations are relative to this frame until `End`.

The `len_hint` is used when entering collections (Vec, HashMap, etc.) - it allows pre-allocation. Ignored for non-collections.

## Operations

### Set

The workhorse operation. Sets a value at a path relative to the current frame.

```rust
Set { path: &[usize], value: Value }
```

### End

Pops the current frame. Validates completeness (unless deferred - see below).

```rust
End
```

### Push

Append an element to a list.

```rust
Push { value: Value }
```

**When `value` is `Literal`**: Element is complete, add it directly.

**When `value` is `Staged`**: Push a frame for building the element. `End` completes and appends.

The implementation decides whether to use direct-fill or staging:

- **Direct-fill** (Vec, VecDeque - has `reserve`/`set_len`):
  1. `reserve(len + 1)`
  2. Frame points into `vec.as_ptr().add(len)`
  3. On `End`: `set_len(len + 1)`

- **Staging** (LinkedList, etc. - only has `push`):
  1. Allocate staging buffer
  2. Frame points at staging
  3. On `End`: call `push` vtable, deallocate staging

The caller doesn't know or care which path is taken.

## Examples

### Scalar

```rust
Set { path: &[], value: Value::Literal { ptr: &42u32, shape: <u32>::SHAPE } }
```

### Struct (field by field)

```rust
// struct Point { x: i32, y: i32 }
Set { path: &[0], value: Value::Literal { ... } }  // x
Set { path: &[1], value: Value::Literal { ... } }  // y
```

### Nested struct (incremental)

```rust
// struct Line { start: Point, end: Point }
Set { path: &[0], value: Value::Staged { len_hint: None } }  // start - pushes frame
  Set { path: &[0], value: Value::Literal { ... } }  // start.x
  Set { path: &[1], value: Value::Literal { ... } }  // start.y
End
Set { path: &[1], value: Value::Staged { len_hint: None } }  // end - pushes frame
  Set { path: &[0], value: Value::Literal { ... } }  // end.x
  Set { path: &[1], value: Value::Literal { ... } }  // end.y
End
```

### Nested struct (you have the whole thing)

```rust
// If you already have a Point value
Set { path: &[0], value: Value::Literal { ptr: &start_point, shape: <Point>::SHAPE } }
Set { path: &[1], value: Value::Literal { ptr: &end_point, shape: <Point>::SHAPE } }
```

### Enum (variant selection via path)

```rust
// enum Message { Quit, Move { x: i32, y: i32 }, Write(String) }

// Set Message::Move { x: 10, y: 20 }
Set { path: &[1], value: Value::Staged { len_hint: None } }  // select variant 1, push frame
  Set { path: &[0], value: Value::Literal { ... } }  // x
  Set { path: &[1], value: Value::Literal { ... } }  // y
End
```

### List (Vec)

```rust
// Vec<Server> where Server { host: String, port: u16 }

// Enter Vec field - len_hint enables pre-allocation
Set { path: &[0], value: Value::Staged { len_hint: Some(2) } }
  
  // Element 0 - built incrementally
  Push { value: Value::Staged { len_hint: None } }
    Set { path: &[0], value: Value::Literal { ... } }  // host
    Set { path: &[1], value: Value::Literal { ... } }  // port
  End
  
  // Element 1 - have the whole thing
  Push { value: Value::Literal { ptr: &server2, shape: <Server>::SHAPE } }
End
```

## Deferred frames

For `#[facet(flatten)]` support, frames can be deferred - left incomplete and re-entered later.

TODO: How does this interact with the new model? Probably:
- `Set { path, value: Value::Staged { ... } }` could take an optional `deferred: bool`
- Or a separate `SetDeferred { path }` operation
- Or deferred is always implicit and determined by whether the frame is complete at `End` time

## Maps

TODO: Maps need key + value. Options:
1. Separate `Insert { key: Value, value: Value }` operation
2. Build key and value in scratch Partials, then insert
3. Something else?

Keeping maps for later discussion.

## Open questions

1. **Pop for error recovery** - Do we need `Pop` to undo an in-progress `Push`?

2. **Arrays** - Fixed-size `[T; N]`. Probably just `Set` with path index: `Set { path: &[0], ... }`, `Set { path: &[1], ... }`. No `Push` since size is fixed.
