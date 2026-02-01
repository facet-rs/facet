# trame TODO

Unified Path Model upgrade is **complete**. All items have been implemented.

## Completed

- **Multi-level Path Resolution**: `at(0).at(1).at(2)` creates intermediate frames automatically via `apply_multi_level_set()`
- **Root Path Segment**: `navigate_to_root()` navigates to root when path starts with `PathSegment::Root`
- **Default Application at End**: `apply_defaults_and_ensure_complete()` auto-fills `#[facet(default)]` and `Option<T>` fields
- **build() Auto-Navigation**: `build()` automatically navigates to root
- **Cleanup**: Removed deprecated `PushBuilder`, `InsertBuilder`, `Op::push()`, `Op::insert()`

## Next Steps

Ready for deferred mode implementation.
