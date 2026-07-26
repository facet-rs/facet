+++
title = "Specification"
weight = 40
+++

Conjoined specifications. Rules are implemented and verified by annotated code;
coverage is queryable with `ddc coverage`, so conformance is evidence rather
than review impression.

- **The language** defines source syntax, typing, values, codata, commands,
  placement, and tests.
- **The runtime** defines islands, demand, identity, memoization, receipts,
  scheduling, primitives, persistence, placement transport, and observability.

- **Vixen** defines the product half — capability packages, pins, delivery.
  These are product specifications; they may implement this runtime without
  becoming language semantics, and they are hosted here rather than in a
  separate tree (`vixen.spec.home`).

The Rodin solver specification lives at [/rodin](/rodin). Runner,
store-placement, and trust policy are vixen territory and remain unwritten.
