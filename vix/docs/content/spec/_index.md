+++
title = "vix specifications"
+++

Conjoined specs: each rule here is implemented and verified by annotated
code. Coverage is queryable (`ddc coverage nav`), so "does the
implementation match the spec" is a fact, not a review impression.

- **machine** — the vix runtime: scheduler, store, identity, memo, receipts,
  primitives, persistence, observability. Governs the runtime rewrite;
  ~120 rules, confidence-tagged (SETTLED / DESIGN / OPEN).
- **solver** — the rodin resolver (distilled in `rodin/docs/`; normative
  rules here, starting with conflict learning).

Planned siblings: `lang.*` (the vix language surface and native vocabulary),
`vixen.*` (capabilities, daemons, command grammars — lives in the vixen
repo's dodeca).
