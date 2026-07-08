+++
title = "machine: persistence"
+++

Cross-process persistence of values, memo entries, and claims. Today the
machine persists nothing (memo) and values only via bundles; this page is
the interface those facts grow into — designed now, backed by vx-store at
R8. Building cargo without a persistent cache is building cargo without a
cache.

r[machine.persistence.trait-boundary]

[DESIGN] The persistence seam is a trait defined purely in vix semantic terms
(get value by (schema, hash) — only realized-tier values persist, so no tier
axis is needed; look up memo by demand key; enumerate projection candidates).
Open vix depends on no product crate; the proprietary side implements the
trait against vx-store. This is the open/proprietary seam applied to
persistence. [DESIGN not SETTLED: the interface is load-bearing but its shape
comes from a doc with its own open questions.]

r[machine.persistence.value-vs-claim]

[SETTLED] Two persisted object classes with different trust rules: BYTES
(values, chunks) are self-verifying — request hash, hash what arrives,
compare, no trust needed. CLAIMS (memo mappings, tier-2 candidates,
observation pins) carry policy: tenant trust, verify-by-sampling,
recompute-from-pins. The machine never conflates them; solver facts are
claims under the same policy machinery, not a bespoke path.

r[machine.persistence.reverify-on-load]

[SETTLED] Persistence changes residency, never the proof obligation. A
durable read-set-gated claim re-verifies against the CURRENT world on every
load before acceptance — same rule for exec tier-2, projection candidates,
and warm solver facts, stated once.

r[machine.persistence.lookup-order]

[DESIGN] Demand lookup order with persistence: process-local exact memo →
persistent exact claim (accepted only after receipt/policy check) → local
projection candidates → persistent projection candidates (each verified
before acceptance) → spawn. Persistent EXACT claims are read-set-gated too,
not accepted on `DemandKey` equality alone, unless the function class is
proven pure over only content-addressed arguments (then the key is the
proof). This prevents a stale or cross-tenant persisted exact claim from
serving without verification.

r[machine.persistence.ephemeral-stays-ephemeral]

[DESIGN] Scheduler-internal state — waiters, pending runs, trace clocks,
connection state, presence caches — is never serialized. Only exact pure
memo entries and read-witnessed claims persist. The line is drawn here so
no future implementor tries to persist the demand map.

r[machine.persistence.gc-claim-rooted]

[DESIGN] Any future collector traces liveness from claims and roots (tenant
roots, memo/index claims, run leases, published artifacts, policy-retained
provenance) — never from bare object presence. CAS bytes are claimless;
deleting an object a receipt still names breaks the product.

r[machine.persistence.memo-interface-day-one]

[DESIGN] The memo and store expose persistability interfaces from the first
commit (the trait, not the backing). LED's "no serialization exists" is the
gap being closed, not a license to design in-memory-only.
