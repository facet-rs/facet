+++
title = "Specification"
weight = 40
+++

Conjoined specifications.

- **The language** defines source syntax, typing, values, codata, commands,
  placement, and tests.
- **The runtime** defines islands, demand, identity, memoization, receipts,
  scheduling, primitives, persistence, placement transport, and observability.
- **Vixen** defines the product half — capability packages, the executor
  manifest, packages, pins, delivery, and the registry. These are product
  specifications; they may implement this runtime without becoming language
  semantics, and they are hosted here rather than in a separate tree
  (`vixen.spec.home`).

## What this specification claims, and what it does not

**Coherence is the bar; coverage is not.** The goal is a specification that
holds together, with an honestly-shaded subset that is actually built — not a
demonstration that appears complete by having each layer stand in for the one
below it. A rule that is coherent and unimplemented costs nothing. Two rules
that cannot both be true cost everything.

So the implemented fraction is small, and is *reported* rather than asserted.
Rules carry `r[impl …]` and `r[verify …]` references from the code and tests
that answer to them, and `ddc coverage` reads them:

```
ddc coverage status      # how much is referenced, implemented, verified
ddc coverage untested    # rules no test names
ddc coverage invalid     # references that resolve to nothing
```

Those references are **hints, not claims of completeness**. They say "the code
for this rule is here," which keeps the map between a rule and its
implementation in the source, moving when the source moves and going visibly
stale in a diff when it does not. They do not replace reading the code to find
out whether it does what the rule says.

Three kinds of negative statement appear throughout, and they mean different
things: **Explicitly out** (deferred — not in 0.1), **Retracted** (specified,
found wrong, recorded so it is not rebuilt), and **Deliberately not** (never —
naming the real system that does it and the reason we refuse).

Undecided questions live in prose `## Open` sections, deliberately **outside**
the rule-ID system, so that nothing can cite an open question and no rule ID is
spent on one.

The Rodin solver specification lives at [/rodin](/rodin). Runner,
store-placement, and trust policy are vixen territory and remain unwritten.
