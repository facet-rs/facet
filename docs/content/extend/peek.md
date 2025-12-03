+++
title = "Peek"
weight = 4
insert_anchor_links = "heading"
+++

Placeholder (to be expanded):

- Purpose: read values dynamically without knowing the concrete type.
- Core API: `Peek`, traversing fields/variants, reading scalars, sequences, maps.
- When to use: pretty-printers, diff tools, schema exporters.
- Safety: lifetime parameters tie to the backing value; no cloning unless supported by vtable.
- Examples: walk a struct, iterate a map, stringify with spans.
