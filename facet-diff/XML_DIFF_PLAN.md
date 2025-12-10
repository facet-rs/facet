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

## Current Status (2025-12-10)

### Done

- [x] **facet-diff-core crate created** - Core types split out for sharing across serializers
  - `Diff<'mem, 'facet>` enum: `Equal`, `Replace`, `User`, `Sequence`
  - `Value<'mem, 'facet>` enum: `Tuple`, `Struct` (with updates/deletions/insertions/unchanged)
  - `Updates<'mem, 'facet>` - sequence diff with `Interspersed<UpdatesGroup, Vec<Peek>>`
  - `Path` / `PathSegment` - navigation types for diff trees
  - `DiffSymbols` - configurable symbols (`-`, `+`, `←`, `→`)
  - `DiffTheme` - Tokyo Night colors for deletions/insertions/moves
  - `ChangeKind` enum: `Unchanged`, `Deleted`, `Inserted`, `MovedFrom`, `MovedTo`, `Modified`
  - `Display` impl for `Diff` with tree-style output

- [x] **facet-xml/src/diff_serialize.rs** - Scaffolding in place
  - `DiffSerializeOptions` struct with all config fields
  - Re-exports `DiffSymbols`, `DiffTheme`, `ChangeKind` from facet-diff-core

- [x] **facet-diff showcase** - Real diff examples working
  - Struct field changes, nested structures, sequences
  - Enums, Options, scalar types
  - Confusable string detection with Unicode codepoints
  - Deep/wide tree diffing

### In Progress

- [ ] **Implement `diff_to_string`** in facet-xml
  - The stub exists but the TODO remains:
    ```rust
    // TODO: Implement diff_to_string and diff_to_writer
    ```

### Not Started

- [ ] Test scenarios with actual XML output
- [ ] DiffContext trait (may not be needed - can walk `Diff` directly)
- [ ] Alignment calculation for multiple changed attrs
- [ ] Collapse logic for unchanged runs
- [ ] Move detection integration (cinereus provides this)

## Architecture

```
facet-diff-core (no facet dep, just types)
    ├── Diff, Value, Updates, Interspersed
    ├── Path, PathSegment
    ├── DiffSymbols, DiffTheme, ChangeKind
    └── Display impl for Diff

facet-diff (depends on facet-core, facet-reflect)
    ├── FacetDiff trait - .diff() method
    ├── tree.rs - EditOp, FacetTree, tree_diff (uses cinereus)
    └── re-exports facet-diff-core types

facet-xml (depends on facet-diff-core)
    └── diff_serialize.rs - XML-specific diff rendering
```

## Next Steps

1. **Walk the `Diff` type directly** - No need for `DiffContext` trait; just pattern match on `Diff`:
   ```rust
   match diff {
       Diff::Equal { .. } => { /* emit dimmed */ }
       Diff::Replace { from, to } => { /* emit -/+ lines */ }
       Diff::User { value, .. } => { /* handle struct/tuple */ }
       Diff::Sequence { updates, .. } => { /* handle list */ }
   }
   ```

2. **Implement basic XML diff output** for structs (attributes):
   - Unchanged fields: emit inline, dimmed
   - Changed fields (in `updates`): emit `-`/`+` lines
   - Deleted fields: emit `-` line
   - Inserted fields: emit `+` line

3. **Add alignment** - Calculate max key/value widths before emitting

4. **Add collapse logic** - Track unchanged runs, emit `<!-- N unchanged -->`

## Open Questions

1. **Line width for wrapping**: hardcode 80? configurable? (currently: 80 in DiffSerializeOptions)
2. **Alignment calculation**: need to know max width of old values before emitting (buffer required)
3. **Moved + modified**: show change within moved element? (current mockup does)
4. **Streaming vs buffered**: probably need to buffer for alignment calculations

## See Also

- `examples/diff_format_mockups.rs` - Visual mockups of desired output
- `examples/diff_showcase.rs` - Working demo of current diff engine
