+++
title = "Receipts"
weight = 10
+++

Read-sets and receipts — the product's central claim, stated as machine
law: a receipt is the observed read-set, complete by construction, exposable
as a value, sealable as an instrument. "SLSA provenance is a signed claim
about what a build did; a vixen receipt is the observed read-set."

> r[machine.receipt.witness-reads]
>
> [SETTLED] Store-backed bytes are obtainable ONLY through an accessor that
> records the read — witness-typed, compiler-enforced. Interposition, not
> politeness: if any path can read without recording, receipts are a hope.
> (The FSKit guarantee, applied internally. The old machine's ~19 call-site
> opt-in recording sites and their empirically demonstrated hole rate are the
> counter-example this rule exists to kill.)

> r[machine.receipt.complete-by-construction]
>
> [SETTLED] Receipts are complete by construction: an undeclared input is a
> build failure, not a silent omission. A retroactive audit proves every
> legacy read path is witness-typed before the rewrite claims this rule.

> r[machine.receipt.granularity]
>
> [DESIGN] Reads are recorded at the finest shape the access has: whole,
> field, tag, map-key (by key hash), tree-path, doc-key, and artifact
> projections (ELF/AST/OCI). Recording dedupes by (argument, path) with
> last-write-wins. Reads through not-yet-realized trees (merge/exec) widen to
> whole-tree structurally — the narrower bytes don't exist yet; this widening
> is correct, not a defect. (Preserved from `ProjectionPath` and its
> verification arms.)

> r[machine.receipt.misses-recorded]
>
> [DESIGN] Absence is an observation: reads that find nothing are recorded as
> first-class read-set entries — and this extends to PATH RESOLUTION, not just
> direct lookups. Search-path/PATH/include/symlink/enumeration/mount-boundary
> decisions that can affect the chosen path each record their candidate misses
> and listings (distinguishing "directory present and empty" from "directory
> absent"). The command grammar declares which arguments trigger resolution.
> Tier-2 reuse is sound against "a file appeared" only because every candidate
> miss along the resolution is a receipt. (Extends
> `ReadObservation::Absent`/`Listing`.)

> r[machine.receipt.read-set-as-value]
>
> [DESIGN] A read-set is a first-class vix value: typed, content-addressed,
> hashable like everything else. Receipts are not host-side bookkeeping.

> r[machine.receipt.exposed-to-programs]
>
> [DESIGN] The machine exposes a demanded function's read-set to vix code (the
> demand's certificate is itself demandable). This closes doc-90's gap 1 by
> construction and is what makes read-set widening (doc 50) expressible.
> Field-level consumption tracking (which *fields* of an index row were read,
> warm-facts §5) is the forward-looking requirement on this same surface.
> Demanding a certificate is a machine-meta demand: it does NOT itself produce a
> second-order receipt (no receipt-of-receipt recursion), and reading a receipt
> value is not an input observation of the reader's own computation.

> r[machine.receipt.journal]
>
> [DESIGN] Mandatory observations — secret reveals, provenance facts,
> capability acquisitions — live in a JOURNAL: a persistent observation store
> with a defined lifetime, distinct from the no-op-able event sink
> (`machine.obs.event-sink`) and distinct from the banned "fetch journal cache"
> (a naming collision — that was a private result cache, this is a receipt
> authority). Journal observations survive reload and are not elided when the
> event sink is off; "provable by trace absence" (reveal) means absence in the
> journal, not in the transient event stream.

> r[machine.receipt.certificate-vs-derivation]
>
> [OPEN] The read-set certificate is sufficient for sound reuse [SETTLED, from
> doc 90]. Whether the machine ALSO retains a walkable derivation (the demand
> trace as queryable values, for human-facing explanation — "why was this
> selected") or explanation stays resolver-built is undecided. The design
> lean: retention-policy-gated trace-as-values, justified by the cachet/audit
> product story; the cost is memory and a second retained structure. Decide
> before R5.

> r[machine.receipt.sealable-as-cachet]
>
> [DESIGN] A receipt can be sealed into a cachet: the signed, portable
> attestation instrument (the recipe-embedded-artifact vehicle; signing
> excludes the signature region). [OPEN sub-question: whether cachet sealing
> and secret-value sealing (`sops()`-style, identity from ciphertext) share
> one machine primitive or are two operations that happen to share an English
> word. Two docs use "seal" for different things; unify or rename.]

> r[machine.receipt.fetch-observation-pin]
>
> [SETTLED] A fetch without a declared checksum is an observation: the machine
> pins what it saw, and the pin becomes the receipt's authority. Recompute is
> the audit, not the source of truth; divergence under re-fetch is a visible
> diagnostic, never a silent change. (vix-spec V14–V16.)

> r[machine.receipt.reveal-is-journaled]
>
> [SETTLED] Every secret reveal is a journal observation — provable by trace
> absence when not revealed. Per-leaf lazy reveal rides the existing
> pinned-observation machinery; reveal is a capability-gated coercion, closed
> by default (the current tree wires exactly one recipient and traps all
> others — preserved behavior).

> r[machine.receipt.provenance-facts-consultable]
>
> [DESIGN] Journal-side provenance facts (how a package version was published,
> publish-method transitions) are consultable at resolution time through the
> solve seam, so policy can fail/warn/quarantine on provenance events. The
> receipts system records them; the machine exposes them like any other
> observation.
