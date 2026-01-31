# Deferred Mode Design

Extension to facet-reflect2 for re-entry into incomplete structures.

## Motivation

Immediate mode requires completing each nested structure before moving on:

```rust
Set { dst: &[0], src: Build }  // enter server
  Set { dst: &[0], src: Imm }  // host
  Set { dst: &[1], src: Imm }  // port
End  // must be complete!
```

This doesn't work for:

**TOML tables that can be reopened:**

```toml
[server]
host = "localhost"

[database]
url = "postgres://..."

[server]
port = 8080
```

**Flattened structs where fields are interleaved:**

```rust
struct Request {
    method: String,
    #[facet(flatten)]
    metadata: Metadata,
    body: String,
}

struct Metadata {
    trace_id: String,
    timestamp: u64,
}
```

```json
{
    "method": "GET",
    "trace_id": "abc123",
    "body": "...",
    "timestamp": 1234567890
}
```

Parsing left-to-right:
1. `method` → set `Request.method`
2. `trace_id` → enter `Request.metadata`, set field, but can't End (incomplete)
3. `body` → need to set `Request.body`, but we're stuck inside `metadata`
4. `timestamp` → re-enter `metadata`, finish it

Without deferred mode, the deserializer must buffer all flattened fields until it's seen them all.

Deferred mode allows incomplete frames to persist, with validation deferred until the deferred subtree is finalized.

## When is a Frame Deferred?

A struct frame is deferred if it has any `#[facet(flatten)]` fields.

That's it. The deserializer can determine this from the schema before parsing begins.

When a deferred frame is `End`ed:
- If incomplete → frame stays in arena, can be re-entered later
- When finally complete and `End`ed → subtree is validated, parent's slot becomes COMPLETE

For TOML's reopened tables, the root itself would be marked deferred (or the deserializer uses deferred mode globally).

## Multi-Level Paths

The key API change: paths can have multiple indices.

```rust
Set { dst: &[0, 0], src: Imm("localhost") }  // server.host
Set { dst: &[1, 0], src: Imm("postgres://") } // database.url
Set { dst: &[0, 1], src: Imm(8080) }          // server.port
```

### Semantics

A multi-level path `&[a, b, c]` means:
1. From `current`, navigate to child `a`
2. From there, navigate to child `b`
3. Set value at child `c`
4. Return `current` to where it started

Each intermediate step **ensures a frame exists**:
- If no frame for that child → create one (deferred/incomplete)
- If frame already exists → re-enter it

After the operation, `current` is unchanged. The intermediate frames remain in the arena, tracked by their parents.

### Example: TOML Config

```rust
struct Config {
    server: Server,      // field 0
    database: Database,  // field 1
}

struct Server {
    host: String,        // field 0
    port: u16,           // field 1
    ssl: SslConfig,      // field 2
}

struct SslConfig {
    cert: String,        // field 0
    key: String,         // field 1
}
```

```toml
[server]
host = "localhost"

[database]
url = "postgres://..."

[server]
port = 8080

[server.ssl]
cert = "/path/to/cert"

[server.ssl]
key = "/path/to/key"
```

Operations (all from root, `current` stays at root):

```rust
Set { dst: &[0, 0], src: Imm("localhost") }   // server.host
Set { dst: &[1, 0], src: Imm("postgres://") } // database.url  
Set { dst: &[0, 1], src: Imm(8080) }          // server.port
Set { dst: &[0, 2, 0], src: Imm("/path/cert") } // server.ssl.cert
Set { dst: &[0, 2, 1], src: Imm("/path/key") }  // server.ssl.key
```

Frame tree after all operations:

```
    ┌─────────────┐
───►│ Config      │   fields: [→, →]
    │ (current)   │
    └──────┬──────┘
           │
     ┌─────┴─────┐
     ▼           ▼
┌─────────┐  ┌──────────┐
│ Server  │  │ Database │
│ fields: │  │ fields:  │
│ [✓,✓,→] │  │ [✓]      │
└────┬────┘  └──────────┘
     │
     ▼
┌───────────┐
│ SslConfig │
│ fields:   │
│ [✓, ✓]    │
└───────────┘
```

At `build()` time, we validate the entire tree is complete.

## Parent-Child Linking

`IndexedFields` is a `Vec<Idx<Frame>>` - it could store frame indices, but today we only use sentinels:
- `NOT_STARTED` (0) - via `mark_not_started()`
- `COMPLETE` (u32::MAX) - via `mark_complete()`

When we `Build` into a field, we allocate a child frame but never store its index in the parent. We only store `COMPLETE` after `End` succeeds.

For deferred mode, we use the full range:
- `NOT_STARTED` → field not touched
- Valid index → in-progress child frame
- `COMPLETE` → field fully initialized

### Creating Deferred Frames

When navigating into a child:

```
ensure_child_frame(field_idx):
    child_idx = parent.fields[field_idx]
    
    if child_idx is valid     → re-enter existing frame
    if child_idx is NOT_STARTED → create frame, store index in parent
    if child_idx is COMPLETE  → field already done (error? overwrite?)
```

### End Behavior

In deferred mode, `End` on an incomplete frame:
1. Does NOT error
2. Keeps the frame in the arena
3. Parent's `IndexedFields` keeps the valid frame index
4. Returns to parent

```
apply_end_deferred():
    if at root → error
    pop to parent (frame stays in arena, parent keeps reference)
```

## Map Re-entry

Maps are trickier - we need to find existing entries by key.

```toml
[servers.primary]
host = "localhost"

[servers.secondary]
host = "backup"

[servers.primary]
port = 8080
```

This requires:
1. A way to look up "do we have an in-progress frame for key X?"
2. Type-erased key comparison using the shape's vtable

### DynKey

Type-erased key that can hash and compare using the shape's vtable. Wraps a `TempAlloc` with the key data.

### MapFrame Changes

Add a `pending: HashMap<DynKey, Idx<Frame>>` to track in-progress value frames by key.

On `Insert { key, value: Build }`:
- If `pending` has this key → re-enter that frame
- Otherwise → create new frame, add to `pending`

On `End` of a map value frame:
- If complete → call `insert_fn`, remove from `pending`
- If incomplete → keep in `pending` for re-entry

## What Cannot Be Deferred

**Set elements** - they have no identity until hashed. You can't partially build a set element, End, then find it again. Set elements must complete immediately, even in deferred mode.

```rust
// This must complete before End:
Push { src: Build }
  Set { dst: &[0], src: Imm }
  Set { dst: &[1], src: Imm }
End  // element gets hashed and inserted here
```

## Validation at build()

In deferred mode, `build()` must recursively validate:

```
validate_complete(frame):
    if frame.INIT → ok
    
    for each child slot:
        if NOT_STARTED → error (missing field)
        if valid index → recurse into child frame
        if COMPLETE   → ok
```

## Mode Selection

Options:
1. **Compile-time**: `Partial<Immediate>` vs `Partial<Deferred>` (zero-cost but two types)
2. **Runtime flag**: `Partial::new_deferred()` sets a flag checked in `End`
3. **Always deferred**: Immediate mode is just "deferred where you happen to complete everything"

Option 3 is simplest - the only behavior change is `End` not erroring on incomplete. Callers who want immediate-mode semantics can check completeness explicitly.

## Open Questions

1. **Overwriting deferred frames**: If field 0 has an in-progress frame and you do `Set { dst: &[0], src: Imm(complete_value) }`, do we:
   - Error? (can't overwrite in-progress)
   - Drop the in-progress frame and all its children, then write?
   
2. **Multi-level paths with Build**: What does `Set { dst: &[0, 1], src: Build }` mean?
   - Navigate to [0], then push frame for [1], leave `current` at the new frame?
   - Or navigate to [0, 1], push frame for that location, return `current` to root?

3. **Error recovery**: If an op fails partway through a multi-level path, what state are we in?
