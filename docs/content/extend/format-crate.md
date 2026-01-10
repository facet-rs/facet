+++
title = "Build a Format Crate"
weight = 6
insert_anchor_links = "heading"
+++

Outline (to be expanded):

- Architecture: reader/writer, solver integration, error strategy.
- Using `Shape`/`Peek` to read; `Partial` to construct.
- Attribute plumbing: parse extension attributes, map to behavior.
- Testing patterns: golden files, round-trips, fuzzing, span assertions.
- Performance knobs: buffering vs streaming, zero-copy where possible.
- Walkthrough target: facet-json/facet-yaml excerpts.
