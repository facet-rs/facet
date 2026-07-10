+++
title = "Persistence"
weight = 4
+++

Cross-process persistence of values, memo entries, and claims. Today the
machine persists nothing (memo) and values only via bundles; this page is
the interface those facts grow into — designed now, backed by vx-store at
R8. Building cargo without a persistent cache is building cargo without a
cache.

> r[machine.persistence.trait-boundary]
>
> [SETTLED] The persistence seam is a trait defined purely in vix semantic terms
> (get value by (schema, hash) — only realized-tier values persist, so no tier
> axis is needed; look up memo by demand key; enumerate projection candidates).
> Open Vix depends on no product crate; Vixen implements the trait against
> vx-store. This is the open/product seam applied to persistence.

> r[machine.persistence.value-vs-claim]
>
> [SETTLED] Two persisted object classes with different trust rules: BYTES
> (values, chunks) are self-verifying — request hash, hash what arrives,
> compare, no trust needed. CLAIMS (memo mappings, tier-2 candidates,
> observation pins) carry policy: tenant trust, verify-by-sampling,
> recompute-from-pins. The machine never conflates them; solver facts are
> claims under the same policy machinery, not a bespoke path.

> r[machine.persistence.reverify-on-load]
>
> [SETTLED] Persistence changes residency, never the proof obligation. A
> durable read-set-gated claim re-verifies against the CURRENT world on every
> load before acceptance — same rule for exec tier-2, projection candidates,
> and warm solver facts, stated once.

> r[machine.persistence.lookup-order]
>
> [DESIGN] Demand lookup order with persistence: process-local exact memo →
> persistent exact claim (accepted only after receipt/policy check) → local
> projection candidates → persistent projection candidates (each verified
> before acceptance) → spawn. Persistent EXACT claims are read-set-gated too,
> not accepted on `DemandKey` equality alone, unless the function class is
> proven pure over only content-addressed arguments (then the key is the
> proof). This prevents a stale or cross-tenant persisted exact claim from
> serving without verification.

> r[machine.persistence.ephemeral-stays-ephemeral]
>
> [DESIGN] Scheduler-internal state — waiters, pending runs, trace clocks,
> connection state, presence caches — is never serialized. Only exact pure
> memo entries and read-witnessed claims persist. The line is drawn here so
> no future implementor tries to persist the demand map.

> r[machine.persistence.gc-claim-rooted]
>
> [DESIGN] Any future collector traces liveness from claims and roots (tenant
> roots, memo/index claims, run leases, published artifacts, policy-retained
> provenance) — never from bare object presence. CAS bytes are claimless;
> deleting an object a receipt still names breaks the product.

> r[machine.persistence.memo-interface-day-one]
>
> [DESIGN] The memo and store expose persistability interfaces from the first
> commit (the trait, not the backing). LED's "no serialization exists" is the
> gap being closed, not a license to design in-memory-only.

> r[machine.persistence.four-lifetimes]
>
> [SETTLED] Identity, claim, replica, and lease are distinct. An identity is an
> immutable fact and never expires. A claim maps a recipe/nomination to result
> identity plus receipt and may expire. A replica says one store currently has
> the body and may be evicted. A lease/root promises availability for a policy
> lifetime. No expiration policy changes an identity.

> r[machine.persistence.value-and-claim-interfaces]
>
> [SETTLED] The persistence seam exposes a typed value CAS and a separately
> trusted claim index even when one service implements both. Open Vix verifies
> claims. Vixen governs tenant namespaces, provenance, signatures, sampling,
> residency, leases, and collection. A claim may outlive its value body: resolve
> another replica, recompute and verify the named identity, or report
> unavailable without inventing a new value.

> r[machine.persistence.replica-eviction]
>
> [SETTLED] Every individual CAS replica and every memo candidate is evictable.
> Policy may evict the globally last body only when a live source, permitted
> recomputation path, or accepted-loss policy exists. Unsaved editor snapshots
> retain a live source lease; pinned upstream blobs may rely on admissible
> origins/peers; published artifacts with archival SLAs retain durable roots;
> irreproducible observations require retained flesh for as long as their claims
> promise availability.

> r[machine.persistence.store-placement]
>
> [SETTLED] Store placement first applies hard admissibility filters: tenant and
> ACL/grant, sovereignty jurisdiction, encryption/key domain, secret/proprietary
> policy, execution contract, self-hosting requirement, and observation trust.
> Knowing a hash grants no authority. Among admissible stores, policy minimizes
> measured/predicted cost: current residency, read-set bytes, memory/disk/LAN
> latency, egress price, queueing, recomputation cost, hotness, durability, and
> fanout. Sovereignty changes permitted residency and execution, never semantic
> identity.

> r[machine.persistence.enterprise-shards]
>
> [SETTLED] Enterprise shards are storage and trust domains, not alternate hash
> spaces. The same identity may exist in several tenants without cross-tenant
> discoverability. Physical cross-tenant dedup is optional policy because
> equality itself can leak information. Claim sources are ordered by trust
> (tenant-owned, vendor-signed, public/untrusted, editor-ephemeral) and do not
> become mutually trusted by sharing blob hashes.

> r[machine.persistence.data-plane]
>
> [SETTLED] Orchestrator/control traffic carries demands, identities, grants,
> receipts, leases, cancellations, placement decisions, and residency hints.
> Bulk value bytes move store-to-store or source-provider-to-store on the data
> plane. A Vox connection may multiplex a store lane and small verified bodies
> may inline as an optimization, but the orchestrator is never the mandatory
> payload bottleneck. If the orchestrator itself demands diagnostics or codata,
> those demanded bytes naturally flow to it.

> r[machine.persistence.editor-snapshots]
>
> [SETTLED] An editor buffer is first an in-memory source provider with content
> identity, not an eagerly uploaded shared-CAS object. Remote consumers pull
> only demanded files/ranges. Fanout policy may publish changed chunks once to
> an ephemeral tenant/regional shard, use peer transfer, colocate demands, or
> reconstruct from a prior snapshot plus deltas/CDC, always verifying the final
> content identity. A small edit creates a new identity without requiring a
> full-file disk write or full-file network upload.
