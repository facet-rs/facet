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

Deferred mode allows incomplete frames to persist, with validation deferred to `build()`.

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

Currently, `IndexedFields` stores `Idx<Frame>` per slot but only uses sentinels:
- `NOT_STARTED` (0)
- `COMPLETE` (u32::MAX)

For deferred mode, we use the full range:
- `NOT_STARTED` → field not touched
- Valid index → in-progress child frame
- `COMPLETE` → field fully initialized

### Creating Deferred Frames

When navigating path `&[a, b]`:

```rust
fn ensure_child_frame(&mut self, field_idx: u32) -> Idx<Frame> {
    let frame = self.arena.get(self.current);
    
    match &frame.kind {
        FrameKind::Struct(s) => {
            let child_idx = s.fields.0[field_idx as usize];
            
            if child_idx.is_valid() {
                // Re-enter existing frame
                child_idx
            } else if child_idx.is_not_started() {
                // Create new deferred frame
                let child_frame = /* create frame for field */;
                let new_idx = self.arena.alloc(child_frame);
                
                // Link parent to child
                let frame = self.arena.get_mut(self.current);
                if let FrameKind::Struct(s) = &mut frame.kind {
                    s.fields.0[field_idx as usize] = new_idx;
                }
                
                new_idx
            } else {
                // COMPLETE - field already done, error or overwrite?
                todo!()
            }
        }
        // ... other frame kinds
    }
}
```

### End Behavior

In deferred mode, `End` on an incomplete frame:
1. Does NOT error
2. Keeps the frame in the arena
3. Parent's `IndexedFields` keeps the valid frame index
4. Returns to parent

```rust
fn apply_end_deferred(&mut self) -> Result<(), ReflectError> {
    let frame = self.arena.get(self.current);
    let parent_idx = frame.parent_link.parent_idx()
        .ok_or_else(|| self.error(ReflectErrorKind::EndAtRoot))?;
    
    // Don't check completeness - just pop
    // Frame stays in arena, parent still references it
    
    self.current = parent_idx;
    Ok(())
}
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

```rust
struct DynKey {
    data: TempAlloc,
    // For lookup, we need hash + eq from the shape's vtable
}

impl DynKey {
    fn eq(&self, other: &DynKey) -> bool {
        // Use shape vtable for comparison
        todo!()
    }
    
    fn hash(&self) -> u64 {
        // Use shape vtable for hashing
        todo!()
    }
}
```

### MapFrame Changes

```rust
struct MapFrame {
    def: &'static MapDef,
    initialized: bool,
    len: usize,
    // NEW: track in-progress value frames by key
    pending: HashMap<DynKey, Idx<Frame>>,
}
```

On `Insert { key, value: Build }`:
1. Check if `pending` has an entry for this key
2. If yes, re-enter that frame
3. If no, create new frame and add to `pending`

On `End` of a map value frame:
1. If complete, call `insert_fn` and remove from `pending`
2. If incomplete (deferred), keep in `pending`

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

```rust
fn validate_complete(&self, idx: Idx<Frame>) -> Result<(), ReflectError> {
    let frame = self.arena.get(idx);
    
    if frame.flags.contains(FrameFlags::INIT) {
        return Ok(());
    }
    
    match &frame.kind {
        FrameKind::Struct(s) => {
            for (i, child_idx) in s.fields.0.iter().enumerate() {
                if child_idx.is_not_started() {
                    return Err(/* field i not initialized */);
                } else if child_idx.is_valid() {
                    // Recurse into child frame
                    self.validate_complete(*child_idx)?;
                }
                // COMPLETE is fine
            }
            Ok(())
        }
        // ... other kinds
    }
}
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
