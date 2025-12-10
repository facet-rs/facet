# Diff Layout Design

## Overview

This document describes the data structures and algorithms for rendering diffs as formatted XML (and eventually JSON, TOML, etc.) with proper alignment, coloring, and collapsing.

## Goals

- Format values once, measure once, emit once (no redundant work)
- Proper column alignment for changed attributes on -/+ lines
- Configurable line width with automatic wrapping
- Collapse long runs of unchanged elements
- No per-value allocations - use arenas

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         Diff<'mem, 'facet>                       │
│                    (from facet-diff-core)                        │
└─────────────────────────────────────────────────────────────────┘
                                │
                                ▼
┌─────────────────────────────────────────────────────────────────┐
│                      Phase 1: Format                             │
│                                                                  │
│  Walk the Diff, format all scalar values into FormatArena.       │
│  Each value becomes a Span + width measurement.                  │
└─────────────────────────────────────────────────────────────────┘
                                │
                                ▼
┌─────────────────────────────────────────────────────────────────┐
│                      Phase 2: Layout                             │
│                                                                  │
│  Build LayoutNode tree (in indextree Arena).                     │
│  Group changed attrs into lines, calculate alignment.            │
│  Decide what to collapse.                                        │
└─────────────────────────────────────────────────────────────────┘
                                │
                                ▼
┌─────────────────────────────────────────────────────────────────┐
│                      Phase 3: Render                             │
│                                                                  │
│  Walk LayoutNode tree, emit to writer with proper                │
│  indentation, prefixes (-/+/←/→), colors, and padding.          │
└─────────────────────────────────────────────────────────────────┘
```

## Data Structures

### FormatArena

Single buffer for all formatted strings. Values are written once and referenced by `Span`.

```rust
pub struct Span {
    pub start: u32,
    pub end: u32,
}

pub struct FormatArena {
    buf: String,  // Pre-allocated with estimated capacity
}

impl FormatArena {
    pub fn with_capacity(cap: usize) -> Self;

    /// Format into arena, return span and display width
    pub fn format<F>(&mut self, f: F) -> (Span, usize)
    where
        F: FnOnce(&mut String) -> fmt::Result;

    /// Retrieve string for a span
    pub fn get(&self, span: Span) -> &str;
}
```

### FormattedValue

A formatted scalar value with its measurements.

```rust
pub struct FormattedValue {
    pub span: Span,       // Into FormatArena
    pub width: usize,     // Display width (unicode-aware)
}
```

### Attr and AttrStatus

Attributes with their change status.

```rust
pub enum AttrStatus {
    Unchanged { value: FormattedValue },
    Changed { old: FormattedValue, new: FormattedValue },
    Deleted { value: FormattedValue },
    Inserted { value: FormattedValue },
}

pub struct Attr {
    pub name: &'static str,
    pub name_width: usize,
    pub status: AttrStatus,
}
```

### ChangedGroup

A group of changed attributes that fit on one -/+ line pair, with alignment info.

```rust
pub struct ChangedGroup {
    pub attr_indices: Vec<usize>,  // Into parent's attrs vec
    pub max_name_width: usize,
    pub max_old_width: usize,
    pub max_new_width: usize,
}
```

### LayoutNode

Nodes in the layout tree, stored in `indextree::Arena`.

```rust
pub enum LayoutNode {
    Element {
        tag: &'static str,
        attrs: Vec<Attr>,
        changed_groups: Vec<ChangedGroup>,
        change: ElementChange,
    },
    Collapsed { count: usize },
    Text { value: FormattedValue, change: ElementChange },
}

pub enum ElementChange {
    None,
    Deleted,
    Inserted,
    MovedFrom,
    MovedTo,
}
```

### Layout

The complete layout ready for rendering.

```rust
pub struct Layout {
    pub strings: FormatArena,
    pub tree: Arena<LayoutNode>,
    pub root: NodeId,
}
```

## Algorithms

### Attribute Grouping

Group changed attributes into lines that fit within max line width.

```
Input:  [fill: red→blue, x: 10→20, y: 5→15, stroke: black→white, ...]
Output: [
    Group { attrs: [fill, x], max_name=6, max_old=5, max_new=5 },
    Group { attrs: [y, stroke], max_name=6, max_old=5, max_new=5 },
]
```

Algorithm:
1. For each changed attr, calculate: `name_width + 2 (="") + max(old_width, new_width) + 1 (space)`
2. Greedy bin-pack into lines that fit `max_width - indent - 2 (prefix)`
3. For each group, compute max widths for alignment

### Collapse Detection

Collapse runs of unchanged siblings when `count > threshold`.

```
Input:  [unchanged, unchanged, unchanged, unchanged, unchanged, changed, unchanged]
         └──────────────────┬─────────────────────┘            └──┬───┘
                     collapse to "5 unchanged"              keep (context=1)
```

Algorithm:
1. Scan children, identify runs of unchanged elements
2. Keep `context` elements before/after each change
3. Collapse runs longer than `collapse_threshold`

### Alignment

For a group of changed attrs, the -/+ lines are aligned:

```
- fill="red"   x="10"
+ fill="blue"  x="20"
  ^^^^         ^
  name         name aligned, values padded to max width
```

Padding calculation:
- Name column: pad to `max_name_width`
- Old value: pad to `max_old_width` (only matters for - line alignment with + line)
- Between attrs: single space

## Rendering

### Element with Changed Attrs

```xml
<rect
- fill="red"   x="10"
+ fill="blue"  x="20"
  y="5" width="100" height="50"
/>
```

- Opening tag on its own line
- Each ChangedGroup emits a - line and a + line
- Unchanged attrs on a single line (dimmed)
- Self-closing or with children

### Deleted/Inserted Elements

```xml
- <circle cx="50" cy="50" r="25"/>
+ <ellipse cx="50" cy="50" rx="30" ry="20"/>
```

Entire element in red/green, prefix on every line of multi-line elements.

### Moved Elements

```xml
← <circle id="a" cx="50" cy="50" r="25"/>
  ... other elements ...
→ <circle id="a" cx="50" cy="50" r="25"/>
```

Blue color, ← at old position, → at new position.

### Collapsed Runs

```xml
<!-- 5 unchanged -->
```

Gray/dimmed comment.

## Dependencies

- `unicode-width` - for display width calculation
- `indextree` - for arena-allocated tree
- `owo-colors` - for ANSI coloring

## Files

- `facet-diff-core/src/layout/mod.rs` - Main types and Layout struct
- `facet-diff-core/src/layout/arena.rs` - FormatArena and Span
- `facet-diff-core/src/layout/attrs.rs` - Attr, AttrStatus, ChangedGroup
- `facet-diff-core/src/layout/node.rs` - LayoutNode, ElementChange
- `facet-diff-core/src/layout/build.rs` - Build Layout from Diff
- `facet-diff-core/src/layout/render.rs` - Render Layout to writer

## Current Status

**Implemented:**
- `FormatArena` - arena for pre-formatted strings with span tracking
- `Span` - reference into the arena
- `FormattedValue` - span + display width
- `Attr`, `AttrStatus` - attribute types with change status
- `ChangedGroup` - group of changed attrs for alignment
- `group_changed_attrs()` - bin-packing algorithm for grouping
- `LayoutNode`, `Layout` - tree structure using indextree
- `ElementChange` - enum for deleted/inserted/moved
- `render()`, `render_to_string()` - rendering to writer/String
- `RenderOptions` - colors, symbols, indent config

**Not yet implemented:**
- Build phase (Diff -> Layout conversion)
- Collapse detection for unchanged runs
- Move detection (←/→ markers)
- XML-specific escaping in format phase

## Test Plan

### Unit Tests

- `arena.rs`: format values, retrieve spans, verify widths
- `attrs.rs`: grouping algorithm with various widths
- `build.rs`: convert simple Diff to Layout
- `render.rs`: render Layout to string, verify output

### Integration Tests

- Single attr change
- Multiple attr changes (fit on one line)
- Multiple attr changes (wrap to multiple lines)
- Attr added/removed
- Child element added/removed
- Element moved
- Element moved AND modified
- Deep nesting
- Collapse unchanged runs
- Full XML document diff

### Snapshot Tests

Use `insta` for snapshot testing of rendered output.
