+++
title = "Specification"
weight = 40
sort_by = "weight"
+++

Conjoined specs: each rule here is implemented and verified by annotated
code. Coverage is queryable (`ddc coverage nav`), so "does the
implementation match the spec" is a fact, not a review impression.

- **The runtime** — scheduler, store, identity, memo, receipts, primitives,
  persistence, observability. Confidence-tagged rules
  (SETTLED / DESIGN / OPEN).

Related specifications live with their components: the rodin solver at
[/rodin](/rodin), the daemon and capability packages under `vixd.*` when
that spec lands. The language surface and native vocabulary (`lang.*`)
is a planned sibling here.
