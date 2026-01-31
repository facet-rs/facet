# facet-reflect2 TODO

Remaining work from the Unified Path Model upgrade.

## Multi-level Path Resolution

Currently, only single-segment paths are supported. Multi-segment paths error with `MultiLevelPathNotSupported`.

**Goal:** Support paths like `Path::field(0).then_field(1).then_append()` with implicit frame creation for intermediate segments.

```rust
// Current: errors
Op::set().at(0).at(1).imm(&mut value)

// Goal: creates intermediate frames automatically
```

**Implementation:**
- `resolve_path` should iterate through segments
- For intermediate segments, create child frame via Stage logic
- Only the last segment determines the action (Imm/Stage/Default)

## Root Path Segment

`PathSegment::Root` exists but isn't processed. The builder method `SetBuilder::root()` exists but does nothing useful.

**Goal:** `Path::root().then_field(0)` should navigate to root first, then descend.

```rust
fn navigate_to_root(&mut self) -> Result<(), ReflectError> {
    while self.current != self.root {
        self.apply_end()?;
    }
    Ok(())
}
```

**Constraint:** Root must be the first segment in a path, otherwise error.

## Default Application at End

Currently, ending a struct with missing fields errors with `EndWithIncomplete`.

**Goal:** Automatically apply defaults for missing fields that have `#[facet(default)]` or are `Option<T>`.

```rust
fn apply_defaults_on_end(&mut self) -> Result<(), ReflectError> {
    let frame = self.arena.get(self.current);
    if let FrameKind::Struct(s) = &frame.kind {
        for (i, field) in fields.iter().enumerate() {
            if s.fields[i] == Idx::NOT_STARTED {
                if field.has_default() || is_option_field(field) {
                    self.apply_default_at_field(i)?;
                } else {
                    return Err(self.error(ReflectErrorKind::MissingRequiredField { index: i }));
                }
            }
        }
    }
    Ok(())
}
```

## build() Auto-Navigation

Currently, `build()` requires being at the root frame. If called while inside a nested frame, it errors.

**Goal:** `build()` should automatically navigate to root (applying End semantics).

```rust
pub fn build<T: Facet<'facet>>(mut self) -> Result<T, ReflectError> {
    // Navigate to root
    while self.current != self.root {
        self.apply_end()?;
    }
    // ... existing validation and extraction
}
```

## Cleanup

Remove deprecated builders from `src/ops/builder.rs`:
- `PushBuilder`
- `InsertBuilder`
- `Op::push()`, `Op::insert()`
