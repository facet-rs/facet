# facet-diff Design Notes

## Current State

### What we have:

1. **Recursive structural diff** (`Diff` enum)
   - `Diff::Equal` - values are identical
   - `Diff::Replace` - leaf-level value change
   - `Diff::User` - struct/enum with nested changes
   - `Diff::Sequence` - list/array with element changes

2. **Tree-based diff** (cinereus integration)
   - `build_tree()` - converts Peek to cinereus Tree
   - `tree_diff()` - returns `Vec<EditOp>` (Update/Insert/Delete/Move)
   - Good for detecting moves, but operates at node level not leaf level

3. **Leaf change collection** (`collect_leaf_changes()`)
   - Walks the `Diff` tree recursively
   - Returns `Vec<LeafChange>` with full paths
   - Each `LeafChange` has path + kind (Replace/Delete/Insert)

4. **Adaptive formatting** (`DiffFormat`)
   - Compact mode: `path: old → new` for each change
   - Truncation: shows first N changes + "... and M more"
   - Colored output with tokyo_night theme

### Output examples:

```
settings.theme: "dark" → "light"
level1.level2.value: "original" → "modified"
[15]: 15 → 999
email: "bob@example.com" → "bob@company.com"
... and 4 more changes
```

## What's Missing / Future Work

### 1. Better context display

Currently we show just the changed values. Could show surrounding context:
```
User {
  name: "Alice",        // unchanged, shown for context
  email: "a@old.com" → "a@new.com",
  age: 30,              // unchanged, shown for context
}
```

### 2. Unified diff-style output

For sequences, could show unified diff format:
```
[
  1,
  2,
- 3,
+ 300,
  4,
]
```

### 3. Side-by-side mode

```
OLD                          NEW
User {                       User {
  email: "a@old.com",          email: "a@new.com",
  age: 30,                     age: 31,
}                            }
```

### 4. Integration with assertion frameworks

For test assertions, want something like:
```rust
assert_eq_facet!(actual, expected);
// On failure, shows nice diff
```

### 5. Move detection at leaf level

The cinereus tree diff can detect moves, but we're not surfacing that well yet. Example:
```
items[2] moved to items[5]
```

### 6. Configurable path display

- Full paths: `user.settings.theme`
- Short paths: `theme` (when unambiguous)
- Bracketed: `user["settings"]["theme"]`

## API Design

```rust
// Current
let diff = old.diff(&new);
println!("{}", diff.format_default());

// Future possibilities
let diff = old.diff(&new);
println!("{}", diff.format(&DiffFormat {
    style: DiffStyle::Unified,  // or Compact, SideBySide, Tree
    context_lines: 2,
    max_changes: 10,
    colors: true,
    path_style: PathStyle::Full,
}));
```

## Miette Integration (Experimental)

We tried using miette for visual diff display with arrows pointing to changes.
It works but has limitations:
- Designed for 1-3 errors, not many diffs
- Labels stack vertically, gets messy
- Better suited for "here's what's wrong" than "here's what changed"

The compact format we built is likely better for most use cases.
