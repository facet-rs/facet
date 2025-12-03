+++
title = "Shape"
weight = 3
insert_anchor_links = "heading"
+++

What you can get from `Shape` (to be expanded):

- Identity: `ConstTypeId`, `type_identifier`, generics metadata.
- Layout: size/alignment, owned vs borrowed.
- Structure: `Type`/`Def` with structs/enums/collections, fields/variants, docstrings, attributes.
- VTables: operations available (`clone_into`, `debug`, `parse`, marker traits via `Characteristic`).
- Safety notes: why `Facet` is `unsafe`, invariants you must respect when consuming `Shape`.
- Examples: listing fields, checking marker traits, rendering type names.
