# Mapping

Mapping of apiv1 Partial methods to apiv2 ops.

## Scalar (u32)

**apiv1:**
```rust
partial.set(42u32)?;
```

**apiv2:**
```rust
Set { path: &[], source: Source::Move(Move { ptr: &42u32, shape: <u32>::SHAPE }) }
```

## Simple struct

```rust
struct Point { x: i32, y: i32 }
```

**apiv1:**
```rust
partial
    .begin_nth_field(0)?.set(10i32)?.end()?
    .begin_nth_field(1)?.set(20i32)?.end()?;
```

**apiv2:**
```rust
Set { path: &[0], source: Source::Move(Move { ptr: &10i32, shape: <i32>::SHAPE }) }
Set { path: &[1], source: Source::Move(Move { ptr: &20i32, shape: <i32>::SHAPE }) }
```

No `End` needed - fields are set directly without pushing frames.

## Nested struct

```rust
struct Line { start: Point, end: Point }
struct Point { x: i32, y: i32 }
```

**apiv1:**
```rust
partial
    .begin_nth_field(0)?  // start
        .begin_nth_field(0)?.set(0i32)?.end()?  // start.x
        .begin_nth_field(1)?.set(0i32)?.end()?  // start.y
    .end()?
    .begin_nth_field(1)?  // end
        .begin_nth_field(0)?.set(10i32)?.end()?  // end.x
        .begin_nth_field(1)?.set(10i32)?.end()?  // end.y
    .end()?;
```

**apiv2:**
```rust
Set { path: &[0], source: Source::Build(Build { len_hint: None }) }  // start - push frame
  Set { path: &[0], source: Source::Move(...) }  // start.x
  Set { path: &[1], source: Source::Move(...) }  // start.y
End
Set { path: &[1], source: Source::Build(Build { len_hint: None }) }  // end - push frame
  Set { path: &[0], source: Source::Move(...) }  // end.x
  Set { path: &[1], source: Source::Move(...) }  // end.y
End
```

`Build` pushes a frame, `End` pops it.

**apiv2 (if you have the whole Point):**
```rust
Set { path: &[0], source: Source::Move(Move { ptr: &start_point, shape: <Point>::SHAPE }) }
Set { path: &[1], source: Source::Move(Move { ptr: &end_point, shape: <Point>::SHAPE }) }
```

No frames needed when you have the complete value.

## Enum

```rust
enum Message { Quit, Move { x: i32, y: i32 }, Write(String) }
```

**apiv1:**
```rust
// Message::Quit
partial.select_nth_variant(0)?.set_default()?;

// Message::Move { x: 10, y: 20 }
partial
    .select_nth_variant(1)?
    .begin_nth_field(0)?.set(10i32)?.end()?
    .begin_nth_field(1)?.set(20i32)?.end()?;

// Message::Write("hello")
partial
    .select_nth_variant(2)?
    .begin_nth_field(0)?.set("hello".to_string())?.end()?;
```

**apiv2:**
```rust
// Message::Quit (unit variant)
Set { path: &[0], source: Source::Default }

// Message::Move { x: 10, y: 20 }
Set { path: &[1], source: Source::Build(Build { len_hint: None }) }  // select variant 1, push frame
  Set { path: &[0], source: Source::Move(...) }  // x
  Set { path: &[1], source: Source::Move(...) }  // y
End

// Message::Write("hello")
Set { path: &[2], source: Source::Move(Move { ptr: &hello_string, shape: <String>::SHAPE }) }
```

Path index selects the variant. `Build` for struct variants (need to set fields), `Move` for tuple variants with a complete value, `Default` for unit variants.

## Vec

```rust
struct Config { servers: Vec<String> }
```

**apiv1:**
```rust
partial
    .begin_nth_field(0)?  // servers
    .init_list_with_capacity(2)?
    .begin_list_item()?.set("server1".to_string())?.end()?
    .begin_list_item()?.set("server2".to_string())?.end()?
    .end()?;
```

**apiv2:**
```rust
Set { path: &[0], source: Source::Build(Build { len_hint: Some(2) }) }  // servers - push frame
  Push { source: Source::Move(Move { ptr: &s1, shape: <String>::SHAPE }) }
  Push { source: Source::Move(Move { ptr: &s2, shape: <String>::SHAPE }) }
End
```

`len_hint` enables pre-allocation. `Push` adds elements - no frame pushed when source is `Move`.

**Empty Vec:**
```rust
Set { path: &[0], source: Source::Default }
```

## Vec with complex elements

```rust
struct Config { servers: Vec<Server> }
struct Server { host: String, port: u16 }
```

**apiv1:**
```rust
partial
    .begin_nth_field(0)?  // servers
    .init_list()?
    .begin_list_item()?
        .begin_nth_field(0)?.set("localhost".to_string())?.end()?
        .begin_nth_field(1)?.set(8080u16)?.end()?
    .end()?
    .end()?;
```

**apiv2:**
```rust
Set { path: &[0], source: Source::Build(Build { len_hint: Some(1) }) }  // servers
  Push { source: Source::Build(Build { len_hint: None }) }  // element - push frame
    Set { path: &[0], source: Source::Move(...) }  // host
    Set { path: &[1], source: Source::Move(...) }  // port
  End
End
```

## Option

```rust
struct Config { timeout: Option<u32> }
```

**apiv1:**
```rust
// Some(30)
partial
    .begin_nth_field(0)?  // timeout
    .begin_some()?
    .set(30u32)?
    .end()?
    .end()?;

// None
partial
    .begin_nth_field(0)?
    .set_default()?  // or just leave it, fill_defaults handles it
    .end()?;
```

**apiv2:**
```rust
// Some(30)
Set { path: &[0], source: Source::Move(Move { ptr: &Some(30u32), shape: <Option<u32>>::SHAPE }) }

// None
Set { path: &[0], source: Source::Default }
```

For `Option`, you typically have the whole value. If you need to build the inner incrementally:

```rust
Set { path: &[0], source: Source::Build(Build { len_hint: None }) }  // push Option frame
  // ... build inner value ...
End
```

## Option with complex inner

```rust
struct Config { server: Option<Server> }
struct Server { host: String, port: u16 }
```

**apiv1:**
```rust
partial
    .begin_nth_field(0)?  // server
    .begin_some()?
        .begin_nth_field(0)?.set("localhost".to_string())?.end()?
        .begin_nth_field(1)?.set(8080u16)?.end()?
    .end()?
    .end()?;
```

**apiv2:**
```rust
Set { path: &[0], source: Source::Build(Build { len_hint: None }) }  // server (Option) - push frame
  Set { path: &[0], source: Source::Move(...) }  // host (inside the Some)
  Set { path: &[1], source: Source::Move(...) }  // port
End
```

When you `Build` into an Option field, the frame is for `Some(T)` - you set fields of `T` directly.

## Box / Arc / Rc

```rust
struct Config { data: Box<Server> }
struct Server { host: String, port: u16 }
```

**apiv1:**
```rust
partial
    .begin_nth_field(0)?  // data
    .begin_smart_ptr()?
        .begin_nth_field(0)?.set("localhost".to_string())?.end()?
        .begin_nth_field(1)?.set(8080u16)?.end()?
    .end()?
    .end()?;
```

**apiv2:**
```rust
Set { path: &[0], source: Source::Build(Build { len_hint: None }) }  // data (Box) - push frame
  Set { path: &[0], source: Source::Move(...) }  // host
  Set { path: &[1], source: Source::Move(...) }  // port
End
```

Smart pointers are transparent - `Build` into them and set the inner value's fields directly. The implementation allocates and wraps appropriately.

## HashSet

```rust
struct Config { tags: HashSet<String> }
```

**apiv1:**
```rust
partial
    .begin_nth_field(0)?  // tags
    .init_set()?
    .begin_set_item()?.set("production".to_string())?.end()?
    .begin_set_item()?.set("us-east".to_string())?.end()?
    .end()?;
```

**apiv2:**
```rust
Set { path: &[0], source: Source::Build(Build { len_hint: Some(2) }) }  // tags
  Push { source: Source::Move(Move { ptr: &tag1, shape: <String>::SHAPE }) }
  Push { source: Source::Move(Move { ptr: &tag2, shape: <String>::SHAPE }) }
End
```

Sets use `Push` just like lists. The implementation knows it's a set and will hash+insert.

## Array

```rust
struct Point3D { coords: [f32; 3] }
```

**apiv1:**
```rust
partial
    .begin_nth_field(0)?  // coords
    .init_array()?
    .begin_nth_field(0)?.set(1.0f32)?.end()?
    .begin_nth_field(1)?.set(2.0f32)?.end()?
    .begin_nth_field(2)?.set(3.0f32)?.end()?
    .end()?;
```

**apiv2:**
```rust
Set { path: &[0, 0], source: Source::Move(...) }  // coords[0]
Set { path: &[0, 1], source: Source::Move(...) }  // coords[1]
Set { path: &[0, 2], source: Source::Move(...) }  // coords[2]
```

Arrays use `Set` with index paths - no `Push` since size is fixed. Multi-element paths work in deferred mode.

## HashMap

```rust
struct Config { env: HashMap<String, String> }
```

**apiv1:**
```rust
partial
    .begin_nth_field(0)?  // env
    .init_map()?
    .begin_key()?.set("PATH".to_string())?.end()?
    .begin_value()?.set("/usr/bin".to_string())?.end()?
    .end()?;
```

**apiv2:**
```rust
Set { path: &[0], source: Source::Build(Build { len_hint: Some(1) }) }  // env
  Insert {
    key: Move { ptr: &path_key, shape: <String>::SHAPE },
    value: Source::Move(Move { ptr: &path_value, shape: <String>::SHAPE })
  }
End
```

`Insert` takes a complete key and either a complete value (`Move`) or incremental (`Build`).

## HashMap with complex values

```rust
struct Config { servers: HashMap<String, Server> }
struct Server { host: String, port: u16 }
```

**apiv1:**
```rust
partial
    .begin_nth_field(0)?  // servers
    .init_map()?
    .begin_key()?.set("primary".to_string())?.end()?
    .begin_value()?
        .begin_nth_field(0)?.set("localhost".to_string())?.end()?
        .begin_nth_field(1)?.set(8080u16)?.end()?
    .end()?
    .end()?;
```

**apiv2:**
```rust
Set { path: &[0], source: Source::Build(Build { len_hint: Some(1) }) }  // servers
  Insert {
    key: Move { ptr: &key, shape: <String>::SHAPE },
    value: Source::Build(Build { len_hint: None })  // push frame for value
  }
    Set { path: &[0], source: Source::Move(...) }  // host
    Set { path: &[1], source: Source::Move(...) }  // port
  End
End
```
