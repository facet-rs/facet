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

## Next Steps / Experiments

### 1. Test all scalar types

We need comprehensive tests for how all scalar types diff:
- Integers: i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize
- Floats: f32, f64
- bool
- char
- Strings: &str, String, Cow<str>
- Bytes: &[u8], Vec<u8>
- Unit type ()

### 2. Deep tree vs wide tree scenarios

Test and visualize:
- **Deep tree**: child removal/addition/move several levels down
- **Wide tree**: many siblings, showing how we skip/collapse unchanged children

### 3. Alternative markup rendering

Instead of Rust-style output, render diffs as:
- JSON
- TOML
- XML
- YAML

This requires:
- Knowing how to collapse/skip many unchanged children in a row
- Format-specific syntax for showing changes inline

### 4. Side-by-side rendering

```
OLD                              NEW
─────────────────────────────    ─────────────────────────────
User {                           User {
  name: "Alice",                   name: "Alice",
  email: "a@old.com",      ←→      email: "a@new.com",
  age: 30,                 ←→      age: 31,
}                                }
```

Challenges:
- Terminal width constraints
- Aligning corresponding lines
- Handling insertions/deletions (one side empty)
- Wrapping long values

### 5. Context-aware display

Show unchanged fields around changes for context:
```
User {
  name: "Alice",              // unchanged, shown for context
  email: "a@old.com" → "a@new.com",
  age: 30,                    // unchanged, shown for context
}
```

Configurable context lines (like `diff -C`).

## API Ideas

```rust
// Current
let diff = old.diff(&new);
println!("{}", diff.format_default());

// Future possibilities
let diff = old.diff(&new);

// Different output formats
println!("{}", diff.format_as_json());
println!("{}", diff.format_as_toml());

// Side-by-side
println!("{}", diff.format_side_by_side(80)); // terminal width

// With context
println!("{}", diff.format(&DiffFormat {
    style: DiffStyle::Unified,
    context_lines: 2,
    max_changes: 10,
    colors: true,
}));
```

## Decided Against

### Miette integration

We tried using miette for visual diff display with arrows pointing to changes.
It works but has limitations:
- Designed for 1-3 errors, not many diffs
- Labels stack vertically, gets messy
- Better suited for "here's what's wrong" than "here's what changed"

The compact format we built is likely better for most use cases.
