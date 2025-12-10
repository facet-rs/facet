# Unified Diff Design

## Problem Statement

facet-diff currently has two separate diffing approaches:

1. **Regular diff** (`Diff` enum) - semantic, field-by-field comparison
   - Knows actual values (`from: Peek`, `to: Peek`)
   - Doesn't detect moves (reordering = delete + insert)

2. **Tree diff** (cinereus) - structural comparison
   - Detects moves, inserts, deletes
   - Only stores hashes, not actual values

This is confusing. We want **one unified diff** that gives you everything:
- Structural understanding (moves, inserts, deletes)
- Value information (what actually changed)

## Proposed Design

### Core Types

```rust
/// The result of diffing two values.
pub struct UnifiedDiff<'mem, 'facet> {
    pub ops: Vec<EditOp<'mem, 'facet>>,
}

/// A single edit operation.
pub enum EditOp<'mem, 'facet> {
    /// Node stayed in place, content changed.
    Update {
        path: Path,
        changes: Vec<LeafChange<'mem, 'facet>>,
    },

    /// Node was inserted (exists in new, not in old).
    Insert {
        path: Path,
        value: Peek<'mem, 'facet>,
    },

    /// Node was deleted (exists in old, not in new).
    Delete {
        path: Path,
        value: Peek<'mem, 'facet>,
    },

    /// Node moved from one location to another.
    Move {
        old_path: Path,
        new_path: Path,
        value: Peek<'mem, 'facet>,
        /// Changes within the moved node (if any)
        changes: Option<Vec<LeafChange<'mem, 'facet>>>,
    },
}

/// A leaf-level value change.
pub struct LeafChange<'mem, 'facet> {
    /// Path relative to parent EditOp
    pub relative_path: Path,
    pub kind: LeafChangeKind<'mem, 'facet>,
}

pub enum LeafChangeKind<'mem, 'facet> {
    Replace { from: Peek<'mem, 'facet>, to: Peek<'mem, 'facet> },
    Delete { value: Peek<'mem, 'facet> },
    Insert { value: Peek<'mem, 'facet> },
}
```

### Algorithm

1. **Build trees** for both values (existing `build_tree()`)
2. **Run cinereus diff** → structural ops (Move, Insert, Delete, Update)
3. **Enrich Update/Move ops** with actual value diffs:
   - For each `Update`: compute `LeafChange`s between matched nodes
   - For each `Move`: compute `LeafChange`s if the moved node also changed
4. **Return `UnifiedDiff`** with all ops

### API

```rust
pub trait UnifiedDiffExt<'facet>: Facet<'facet> {
    fn unified_diff<'a, U: Facet<'facet>>(
        &'a self,
        other: &'a U
    ) -> UnifiedDiff<'a, 'facet>;
}

// Blanket impl for all Facet types
impl<'facet, T: Facet<'facet>> UnifiedDiffExt<'facet> for T { ... }
```

## Integration with Serializers

The key insight: we want to render diffs **in the original format** (XML, JSON, etc.)
without re-serializing the entire document.

### Approach 1: DiffRenderer Trait

Serializers implement a trait that walks the diff:

```rust
pub trait DiffRenderer {
    type Output;
    type Error;

    fn render_diff<'mem, 'facet>(
        &mut self,
        diff: &UnifiedDiff<'mem, 'facet>,
    ) -> Result<Self::Output, Self::Error>;
}
```

Each format implements this differently:
- **Terminal**: colored output with `→` arrows
- **XML**: `+`/`-` line prefixes, collapsed unchanged sections
- **JSON**: similar to terminal but with JSON syntax

### Approach 2: Diff-Aware Serialization

Instead of a separate trait, extend existing serializers with diff context:

```rust
// In facet-xml
pub fn to_string_diff<'f, T: Facet<'f>>(
    old: &T,
    new: &T,
    options: DiffSerializeOptions,
) -> Result<String, Error> {
    let diff = old.unified_diff(new);
    // Serialize `new` but annotate changes based on `diff`
}

pub struct DiffSerializeOptions {
    /// How many unchanged siblings to show around changes
    pub context: usize,
    /// Whether to collapse long runs of unchanged content
    pub collapse_unchanged: bool,
    /// Style for marking changes (inline, line-prefix, etc.)
    pub style: DiffStyle,
}
```

### Approach 3: Two-Phase with Path Filter

Compute diff first, then serialize with a filter:

```rust
// Phase 1: Get changed paths
let diff = old.unified_diff(&new);
let changed_paths: HashSet<Path> = diff.changed_paths();

// Phase 2: Serialize with filter
let output = facet_xml::to_string_filtered(&new, |path| {
    if changed_paths.contains(&path) {
        FilterAction::ShowWithHighlight
    } else if changed_paths.iter().any(|p| p.starts_with(&path)) {
        FilterAction::ShowCollapsed  // ancestor of a change
    } else {
        FilterAction::Hide
    }
});
```

## Example: XML Diff Output

Given two SVGs:

```xml
<!-- OLD -->
<svg viewBox="0 0 100 100">
  <rect fill="red" x="10" y="10"/>
  <circle cx="50" cy="50" r="25"/>
  <path d="M10 10 L90 90"/>
</svg>

<!-- NEW -->
<svg viewBox="0 0 200 200">
  <rect fill="blue" x="10" y="10"/>
  <path d="M10 10 L90 90"/>
  <circle cx="50" cy="50" r="30"/>
</svg>
```

The `UnifiedDiff` would be:

```rust
UnifiedDiff {
    ops: [
        Update {
            path: [],
            changes: [
                LeafChange {
                    relative_path: [Field("view_box")],
                    kind: Replace { from: "0 0 100 100", to: "0 0 200 200" }
                },
            ],
        },
        Update {
            path: [Field("children"), Index(0)],
            changes: [
                LeafChange {
                    relative_path: [Field("fill")],
                    kind: Replace { from: "red", to: "blue" }
                },
            ],
        },
        Move {
            old_path: [Field("children"), Index(1)],  // circle was at [1]
            new_path: [Field("children"), Index(2)],  // now at [2]
            value: <Circle>,
            changes: Some([
                LeafChange {
                    relative_path: [Field("r")],
                    kind: Replace { from: "25", to: "30" }
                },
            ]),
        },
        // path stayed at same logical position (was [2], now [1])
        // but that's because circle moved - path itself didn't move
    ],
}
```

Rendered as XML diff:

```xml
<!-- 3 changes -->
<svg
- viewBox="0 0 100 100"
+ viewBox="0 0 200 200"
  xmlns="...">

  <rect
-   fill="red"
+   fill="blue"
    x="10" y="10"/>

  <!-- circle moved from [1] to [2] -->
  <circle cx="50" cy="50"
-   r="25"
+   r="30"
  />  <!-- moved + changed -->

  <path d="M10 10 L90 90"/>  <!-- unchanged -->

</svg>
```

## Implementation Plan

### Phase 1: Unify the Diff Types
- [ ] Create `UnifiedDiff`, `EditOp`, `LeafChange` types
- [ ] Implement `unified_diff()` using cinereus + value diff
- [ ] Deprecate separate `Diff` enum for external use

### Phase 2: Terminal Rendering
- [ ] Implement `Display` for `UnifiedDiff` (colored terminal output)
- [ ] Show moves explicitly: `[1] → [2]`
- [ ] Show changes within moves

### Phase 3: Format-Specific Rendering
- [ ] `DiffRenderer` trait
- [ ] XML renderer (facet-xml integration)
- [ ] JSON renderer (facet-json integration)

### Phase 4: Context Control
- [ ] Configurable context lines
- [ ] Collapse unchanged sections
- [ ] Breadcrumb paths for deep changes

## Key Design Decisions (Resolved)

### No Indices in Output

Indices shift when elements are added/removed/moved - they're confusing for humans.
Instead:
- Collapse unchanged runs: `/* 5 unchanged */`
- Show enough context that humans can orient themselves
- Let element content/ids identify what's what

### Moves Are Implicit

A moved element appears twice:
- `-` where it was (in the old position)
- `+` where it is now (in the new position)

Humans recognize it's the same element by its content. Explicit arrows or "moved from X to Y" annotations break down with multiple moves.

### Batch Queries for Serializers

Serializers need to collect children by status before emitting, not check one-at-a-time.
This enables grouping multiple changes on single lines:

```xml
<rect
- fill="red" x="10"
+ fill="blue" x="20"
  y="10" width="50" height="50"
/>
```

See `XML_DIFF_RENDERING.md` for the full `DiffContext` trait design.

## Open Questions

1. **Should `UnifiedDiff` be a flat list or a tree?**
   - Flat list is simpler to iterate
   - Tree might be more natural for nested rendering
   - Current design: flat with paths

2. **How to handle type changes?**
   - e.g., `Option<T>` going from `None` to `Some`
   - Currently: would be `Delete` + `Insert`
   - Could have special `TypeChange` variant?

3. **Performance for large documents?**
   - Tree building is O(n)
   - Cinereus diff is O(n²) worst case, usually better
   - Should we have a "quick diff" mode that skips move detection?

4. **Streaming/incremental diffs?**
   - For very large documents, might want to stream ops
   - Current design assumes everything fits in memory
