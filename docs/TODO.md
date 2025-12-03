# Documentation TODO (remaining)

Only the unfinished or still-stale items are tracked here.

## Learn
- Add per-format guides: JSON, YAML, TOML, KDL, MessagePack, etc.
- Finish migration guide: step-by-step process, pitfalls, testing checklist.
- Ship generated comparison showcase for “Why facet?” (binary size, diagnostics, flexibility) with scripts and real numbers.

## Extend
- Deep dives: Shape, Peek, Partial (concepts, examples, gotchas).
- “Build a format crate” tutorial with testing strategy and scaffolding.
- Runtime attribute querying examples tied to real crates.

## Contribute
- Detailed chapters on derive internals, vtables/unsafe invariants, memory layout.
- “Adding a new type” guide with feature-gated std/external types.
- Link and surface existing design docs (e.g., value-error-diagnostics).

## Reference & Accuracy
- Refresh `format-crate-matrix.md` with current test-backed status; date-stamp the matrix.
- Verify GitHub issue links/statuses referenced in docs (formerly 102/145/150/151).
- Fix showcase generator glitches (Target Type rendering) and add “when to use” context per scenario.

## Site polish
- Add visible doc version and “last updated” metadata (footer/header).
- Add troubleshooting/FAQ expansion focused on migration and performance tradeoffs.
