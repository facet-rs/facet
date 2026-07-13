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

The Rodin solver specification lives at [/rodin](/rodin). Vixen capability,
runner, store-placement, and trust policy remain product specifications; they
may implement this runtime without becoming language semantics.
