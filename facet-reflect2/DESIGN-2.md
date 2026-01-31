# facet-reflect2 API Redesign

Exploration of a unified path-based API.

## Current API

```rust
enum Op {
    Set { dst: Path, src: Source },  // Source = Imm | Build | Default
    Append { src: Source },
    Insert { key: Imm, value: Source },
    End,
}
```

Problems:
- `Push` and `Insert` are separate from `Set`
- `End` is its own op
- Paths are indices only, no navigation

## Proposed API

```rust
struct Op {
    path: Path,
    action: Action,
}

enum PathSegment {
    Down(u32),      // child by index (struct field, array element, enum variant, etc.)
    Up,             // parent frame
    Root,           // jump to root (for non-recursive deserializers like TOML)
    Append,         // new list/set element
    Insert(Imm),    // new map entry by key
}

enum Action {
    Stage,          // ensure frame exists, current moves there
    Set(Imm),       // write immediate value
    Default,        // write default value
    Noop,           // just navigate, do nothing at destination
}
```

## Semantics

### Path Execution

A path is a sequence of navigation steps executed left-to-right.

**Rule: all path segments except the last implicitly create frames (Stage) if needed.**

The final segment's behavior depends on the action:
- `Stage` → create/re-enter frame, current stays there
- `Set(imm)` → write value, no frame created for this segment
- `Default` → write default, no frame created for this segment

### Down

`Down(n)` navigates to field/element `n` of current frame.

If this is NOT the last segment: ensures a frame exists (creates or re-enters).
If this IS the last segment: depends on action.

### Up

`Up` navigates to parent frame.

In deferred mode: always succeeds, even if current frame is incomplete.
In strict mode: validates current frame is complete before popping.

### Append

`Append` adds a new element to a list or set.

If not last segment: creates frame for the new element.
If last segment with `Set(imm)`: appends the value directly, no frame.

### Insert

`Insert(key)` accesses/creates a map entry.

If entry exists (deferred mode): re-enters the value frame.
If entry doesn't exist: creates new entry.

## Examples

### Simple Struct

```rust
struct Point { x: i32, y: i32 }
```

```rust
Op { path: &[Down(0)], action: Set(imm_10) }  // x = 10
Op { path: &[Down(1)], action: Set(imm_20) }  // y = 20
```

No frames created (besides root). Just writes values.

### Nested Struct

```rust
struct Line { start: Point, end: Point }
```

```rust
// Build start incrementally
Op { path: &[Down(0)], action: Stage }           // enter start
Op { path: &[Down(0)], action: Set(imm_0) }      // start.x = 0
Op { path: &[Down(1)], action: Set(imm_0) }      // start.y = 0
Op { path: &[Up], action: Noop }                  // back to Line

// Set end directly
Op { path: &[Down(1)], action: Set(imm_point) }  // end = complete Point
```

### List Building

```rust
struct Config { servers: Vec<String> }
```

```rust
Op { path: &[Down(0)], action: Stage }           // enter servers list
Op { path: &[Append], action: Set(imm_s1) }      // append "server1"
Op { path: &[Append], action: Set(imm_s2) }      // append "server2"
Op { path: &[Up], action: Noop }                  // back to Config
```

### List with Complex Elements

```rust
struct Config { servers: Vec<Server> }
struct Server { host: String, port: u16 }
```

```rust
Op { path: &[Down(0)], action: Stage }           // enter servers list
Op { path: &[Append], action: Stage }            // append new Server, enter it
Op { path: &[Down(0)], action: Set(imm_host) }   // host = "localhost"
Op { path: &[Down(1)], action: Set(imm_port) }   // port = 8080
Op { path: &[Up], action: Noop }                  // back to list
Op { path: &[Append], action: Stage }            // append another Server
// ...
```

### Map Building

```rust
struct Config { env: HashMap<String, String> }
```

```rust
Op { path: &[Down(0)], action: Stage }                      // enter env map
Op { path: &[Insert(key_path)], action: Set(imm_value) }    // PATH = "/usr/bin"
Op { path: &[Insert(key_home)], action: Set(imm_value) }    // HOME = "/home/user"
Op { path: &[Up], action: Noop }                             // back to Config
```

### TOML Reopened Tables

```toml
[server]
host = "localhost"

[database]
url = "postgres://..."

[server]
port = 8080
```

```rust
// [server]
// host = "localhost"
Op { path: &[Root, Down(0), Down(0)], action: Set(imm_host) }   // server.host

// [database]
// url = "postgres://..."
Op { path: &[Root, Down(1), Down(0)], action: Set(imm_url) }    // database.url

// [server] - re-entry!
// port = 8080
Op { path: &[Root, Down(0), Down(1)], action: Set(imm_port) }   // server.port (re-enters server frame)
```

With `Root`, the TOML deserializer doesn't need to track where it is. Each `[section]` maps directly to `Root, Down(section_idx), ...`

### Flattened Struct

```rust
struct Request {
    method: String,
    #[facet(flatten)]
    metadata: Metadata,
    body: String,
}
struct Metadata { trace_id: String, timestamp: u64 }
```

```json
{"method": "GET", "trace_id": "abc", "body": "...", "timestamp": 123}
```

```rust
Op { path: &[Down(0)], action: Set(imm_method) }      // method = "GET"
Op { path: &[Down(1), Down(0)], action: Set(imm_trace) }  // metadata.trace_id = "abc"
Op { path: &[Up, Down(2)], action: Set(imm_body) }    // body = "..."
Op { path: &[Up, Down(1), Down(1)], action: Set(imm_ts) } // metadata.timestamp = 123
```

Wait, this doesn't work. After the first `Down(1), Down(0)` we're at metadata.trace_id. But `Set` doesn't create a frame, so where's current?

Let me reconsider...

## Problem: Where is Current After Set?

If `Set` doesn't create a frame, current doesn't move there. So after:

```rust
Op { path: &[Down(1), Down(0)], action: Set(imm) }
```

Where is current? Options:
1. Current stays where it was (root) - path is transient navigation
2. Current is at `[1, 0]` but there's no frame there - contradiction
3. Current is at `[1]` because that's the last frame we created

Option 3 makes sense: intermediate segments create frames, final `Set` writes without creating frame, current is at the deepest frame created.

So after `&[Down(1), Down(0)], Set(imm)`:
- Frame created for field 1 (metadata)
- Value written to field 0 of metadata
- Current is at metadata frame

Then `&[Up, Down(2)], Set(imm)`:
- Up to root
- No intermediate frames (Down(2) is final segment)
- Write to field 2
- Current is at root

Then `&[Up, Down(1), Down(1)], Set(imm)`:
- Up goes nowhere (already at root)? Or error?
- Down(1) re-enters metadata frame
- Down(1) is final, write to field 1 of metadata
- Current is at metadata

Hmm, need to think about `Up` at root.

## build() Behavior

Calling `build()` automatically:
1. Navigates up from `current` to root (as if executing `Up` repeatedly)
2. Validates each frame along the way (strict) or validates the whole tree (deferred)
3. Returns the final value

So you don't need to manually navigate back to root before calling `build()`.

## Resolved Questions

1. **Up at root?** Error. It's a bug in the deserializer.

2. **Current after Set with single-segment path?** Current stays where it was. Only intermediate segments create frames; a single segment has no intermediates.

3. **Validation timing?** `Up` validates the frame being left (replaces `End`). Strict frames validate on every `Up`. Deferred frames skip validation on `Up`, deferring to `build()` or when exiting into a strict parent.
