# XML Diff Rendering Plan

## Goal

Make `facet-xml` able to render diffs between two values as XML with `-`/`+` annotations.

## Output Format

```xml
<rect
- fill="red"   x="10"
+ fill="blue"  x="20"
  y="10" width="50" height="50"
/>
```

Key decisions:
- `-` = deleted, `+` = inserted (red/green)
- `←` = moved from here, `→` = moved to here (blue)
- Keys stay white, only VALUES are colored
- Multiple changed attrs aligned in columns
- Wrap into groups if too many attrs changed
- Collapse unchanged runs: `<!-- N unchanged -->`

## Step 1: Write Test Scenarios

Create actual Rust structs for each scenario, compute the diff, see what `EditOp`s we get.

```rust
#[derive(Facet, Clone)]
struct Rect {
    fill: String,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

// Scenario: single attr change
let old = Rect { fill: "red".into(), x: 10, y: 10, width: 50, height: 50 };
let new = Rect { fill: "blue".into(), x: 10, y: 10, width: 50, height: 50 };
let diff = old.diff(&new);
// What does diff look like? What EditOps?
```

Scenarios to test:
1. Single attribute change
2. Multiple attribute changes
3. Attribute added
4. Attribute removed
5. Child element added (simple)
6. Child element added (large, with children)
7. Child element removed
8. Children reordered (move detection)
9. Element moved AND modified
10. Deep nested change
11. Many unchanged siblings (collapse)

## Step 2: Design DiffContext Trait

The serializer needs to query what changed. Batch queries are better than one-at-a-time:

```rust
trait DiffContext<'mem, 'facet> {
    /// Attrs that changed (need -/+ lines)
    fn changed_children(&self, path: &Path) -> Vec<ChangedChild>;

    /// Attrs that stayed same (inline, dimmed)
    fn unchanged_children(&self, path: &Path) -> Vec<UnchangedChild>;

    /// Elements removed (- prefix, red)
    fn deleted_children(&self, path: &Path) -> Vec<DeletedChild>;

    /// Elements added (+ prefix, green)
    fn inserted_children(&self, path: &Path) -> Vec<InsertedChild>;

    /// Elements relocated (← old pos, → new pos, blue)
    fn moved_children(&self, path: &Path) -> Vec<MovedChild>;

    /// Can we skip this subtree?
    fn should_collapse(&self, path: &Path) -> bool;

    /// How many siblings collapsed at this point?
    fn collapsed_count(&self, path: &Path) -> Option<usize>;
}
```

## Step 3: Modify XML Serializer

Add a diff-aware mode:

```rust
// Normal serialization
pub fn to_string<T: Facet>(value: &T) -> Result<String>;

// Diff-aware serialization
pub fn to_string_diff<T: Facet>(
    old: &T,
    new: &T,
    options: DiffOptions,
) -> Result<String>;

pub struct DiffOptions {
    /// How many unchanged siblings to show around changes
    pub context: usize,
    /// Collapse runs longer than this
    pub collapse_threshold: usize,
    /// Enable colors (ANSI escape codes)
    pub colors: bool,
}
```

## Step 4: Serializer Layout Logic

```
for each element:
    if should_collapse(path):
        emit "<!-- N unchanged -->"
        continue

    // Collect children by status
    changed = ctx.changed_children(path)
    unchanged = ctx.unchanged_children(path)
    deleted = ctx.deleted_children(path)
    inserted = ctx.inserted_children(path)
    moved = ctx.moved_children(path)

    emit "<tagname"

    if no changes:
        emit attrs inline, dimmed
        emit "/>" or ">"
    else:
        emit newline

        // Group changed attrs, align columns
        for group in changed.chunks_fitting_line_width():
            emit "- " + group.map(|c| key=old_value).aligned()
            emit "+ " + group.map(|c| key=new_value).aligned()

        // Deleted attrs
        for d in deleted:
            emit "- " + d.name + "=" + d.value

        // Inserted attrs
        for i in inserted:
            emit "+ " + i.name + "=" + i.value

        // Unchanged attrs inline (dimmed)
        emit "  " + unchanged.inline()

        emit ">"

    // Child elements with same logic...
    for child in children:
        if child.is_moved_from():
            emit "← " + serialize(child, blue)
        elif child.is_moved_to():
            emit "→ " + serialize(child, blue)
        elif child.is_deleted():
            emit "- " + serialize(child, red)
        elif child.is_inserted():
            emit "+ " + serialize(child, green)
        else:
            recurse(child)
```

## Open Questions

1. **Line width for wrapping**: hardcode 80? configurable?
2. **Alignment calculation**: need to know max width of old values before emitting
3. **Moved + modified**: show change within moved element? (current mockup does)
4. **Streaming vs buffered**: probably need to buffer for alignment calculations
