+++
title = "lesson: the two decisions, from zero"
+++


Self-contained. No term is used before it is explained. Each part ends with
the actual decision in one line. (Supersedes the "decision 1" section of
`reconcile-decisions.md`, which was framed before the census and before the
one-mechanism decree.)

---

## Part 1 — warm facts: what rodin learns, and what it means to keep it warm

**What rodin does.** Rodin resolves versions: given manifests and a package
index, pick one version per package so every requirement is satisfied. It
does this the way modern SAT solvers work: try assignments, propagate
consequences, and when a contradiction appears, *learn from it*.

**What a learned fact is.** When a solve hits a dead end, the solver derives
a compact reason for the failure before backtracking. Example: "`app`
needs `tokio ≥1.40` and `legacy-io` needs `mio <0.8`, and every `tokio`
≥1.40 requires `mio ≥0.8` — therefore `app` and `legacy-io` can never
coexist." That sentence is a **learned fact** (solver literature calls it a
*no-good*). For the rest of the solve, the solver never wastes time in that
corner again. Learning is why hard solves finish.

**What "warm" means.** Within one solve, learned facts are just memory.
The prize is reusing them *across* solves — tomorrow, after the index
changed a little, most of yesterday's learned facts are still true, and
starting with them makes tomorrow's solve dramatically cheaper. A fact
carried across solves is a **warm fact**.

**Why that's dangerous.** A learned fact is only true *because of specific
things about the world*. The example fact above rests on: the dependency
rows of `app`, `legacy-io`, and every `tokio` ≥1.40 — and, sneakily, on an
*absence*: "there exists NO tokio ≥1.40 that works with old mio." Each of
these supporting world-facts is called a **premise**. If any premise
changes, the fact may be false. And the absence premise is the treacherous
one: publishing a *new* tokio version tomorrow can invalidate it — a fact
can be killed by something that didn't exist when it was learned. Reusing a
stale fact doesn't make the solver slow; it makes the resolution *wrong*.

**The old design** (`warm-facts-spec.md`, written 2026-07-07 — a design
document, never implemented; not to be confused with the retired Rust
resolver). It made warm reuse safe with a bespoke subsystem:

- every fact carries its **premise list**, including first-class absence
  premises ("no version of X in range Y");
- a **proof digest** — a content hash of the derivation, so a fact can be
  checked for integrity;
- a **WarmFactVerifier** — the component that, before letting a persisted
  fact into a new solve, *replays* the check: are all its premises still
  true in today's index? Only verified facts install;
- **quarantine / trust policy** — facts offered by another machine or
  tenant aren't trusted just because they parse;
- an **inverted index** mapping `premise → facts that rest on it` — so
  that when the world changes (say, a publish into range Y), the machine
  can look up *which* warm facts just died, instead of re-checking all of
  them one by one.

**What made this a fork.** One day later, the machine spec collapsed all of
that into one sentence: a learned fact is just a *memoized value* — its
recipe is the derivation, its read-set is its premises — and the machine's
generic reuse machinery already provides everything the bespoke subsystem
provided. The mapping, mechanism by mechanism (this is what the research
pass verified, and several generic rules cite warm-facts *by name*, so the
collapse was deliberate):

| Old mechanism | Generic machinery that subsumes it |
|---|---|
| premise list | the **read-set**: the machine records everything a computation reads, automatically — a fact's premises ARE its recorded reads |
| absence premises | `misses-recorded`: reads that find *nothing* are recorded as first-class read-set entries — absence is already an observation |
| proof digest | `hashing-is-ambient`: every value already has a content hash for free |
| WarmFactVerifier / replay-before-install | `reverify-on-load`: *anything* loaded from persistence re-verifies its read-set against the current world before being trusted — "same rule for exec, projection candidates, and warm solver facts, stated once" |
| quarantine / trust | `value-vs-claim`: persisted BYTES verify themselves by hash; persisted CLAIMS ("this fact holds") pass a trust policy — solver facts are claims, "not a bespoke path" |

**The one thing the generic machine does NOT have: the inverted index.**
Note what it's for: not correctness — re-verification covers that — but
*finding*. Two finding problems, actually: "the world changed; which stored
facts should I even re-check?" (push invalidation) and "I'm starting a
solve; which stored facts are worth offering it?" (candidate nomination).
The generic machine currently answers both with a shrug — and that shrug is
the same hole the **location plane** (`three-planes.md`) is being designed
to fill for build reuse in general. So the recommendation "keep the index
as nomination precedent" means, in plain words: *don't build the solver its
own bespoke index; but when we build the generic finding machinery, the old
spec is the only place anyone in this corpus has actually worked that
problem — steal its lessons.* One lesson in particular: it chose an exact
index, and at solver scale (a fact rests on one or two premises; absence
keys number in the hundreds) exact is free. The build-graph analog is a
hundred thousand runs — three orders bigger — which is why approximate
filters are even on the table there. The crossover is a measurement.

**The decision, one line:** retire the bespoke subsystem — a warm fact is a
memoized value and rides the generic memo/read-set/re-verify/trust
machinery — and fold its finding-problem (the inverted index) into the
generic nomination design as its worked precedent. (Everything above says
this loses nothing; the research pass found zero contradictions and zero
soundness residue.)

*Queued alongside, per Amos: rename the `solver.*` rule namespace →
`rodin.*` — rodin is one specific solver, not the genus.*

---

## Part 2 — the content-hash definition: what exactly we're choosing

**Which hash this is about.** We ratified three planes of identity. The
**location hash** (where a value lives in the demand graph — nomination
only) and the **recipe hash** (what would be computed — operation +
inputs' identities) are settled and *not* part of this decision. This
decision is only about the third: the **content hash** — the hash of a
materialized value's actual contents, the thing memo keys, store dedup,
receipts, and bundles ultimately ground in. One definition of it must
exist; the current code accidentally contains two; one must win.

**No epochs.** Nothing is deployed, no caches exist outside our own dev
machines, nobody loses anything if the definition changes next month —
we'd recompute some hashes. The "identity epoch" vocabulary (breaks,
gates, committee ratification) belonged to a future with persisted caches
in other people's hands; it is deleted from this decision. We pick the
best definition on the merits. (When persistence ships to real users,
*re*-defining it becomes expensive. That is a reason to think now, not a
ceremony to perform now.)

**What the content hash must serve** (from the consumption census, which
walked every consumer in the current code): it is consumed only at
**edges** — when arguments form a memo key, when a value is interned into
the store, when a receipt's recorded reads are verified, when a bundle
crosses a process boundary. Nothing hashes values in straight-line code;
values that never cross an edge are never hashed under either definition.
So the choice is not about how often we hash — it's about what the hash
*is*.

**Definition A — hash a walked encoding.** To hash a value, visit it
according to its schema and feed the hasher a description: "this is a
record of schema Point; field 0: Int, these 8 bytes; field 1: Int, these
8 bytes." References to other stored values (handles) contribute the
referent's content hash, not the reference word. The stream is *framed* —
structural markers make it self-delimiting, so no two different values can
produce the same bytes by coincidence of concatenation.

*Corrected framing, per Amos — and it matters:* the walk is **schema-const**.
For any given schema, the entire walk structure — which fields, what
order, what framing bytes — is known at lowering time. So the walk does
not have to be (and should not stay) the hand-written, reflection-shaped,
descriptor-interpreting Rust it is today: it can be **lowered as weavy
IR, specialized per schema**, with the constant framing bytes precomputed
and folded, leaving straight-line code that is essentially "hash these N
bytes, splice these referent hashes at known offsets." Today's slowness is
an implementation choice, not the definition's nature. Definition A must
be judged in its specialized form.

**Definition B — hash the canonical memory image.** A value's hash input
*is* its store memory: the zero-padded canonical bytes, with each handle
word substituted by the referent's 32-byte content hash. No framing bytes;
unambiguity comes from the schema (which is part of the identity pair) and
the layout it fully determines. For a flat value — no handles — the image
is literally the raw bytes, and hashing is one blake3 over memory.

**Where they genuinely differ, once A is specialized.** Strip away the
performance folklore and the real differences are these:

1. **Coupling to layout.** B ties identity to the weavy-declared memory
   layout: change a struct's field order or width and every hash of every
   value of that schema changes. A's hashes survive layout changes (the
   encoding is its own definition, independent of memory). Under B, layout
   canonicality — and the zero-padding law — become *identity-load-bearing*
   forever, not just hygiene.
2. **The facet question — the sharpest one.** Vix values exist under two
   ABIs by constitutional doctrine: weavy-declared (we control layout,
   padding zeroed, canonical) and **facet-discovered** (rustc dictates the
   layout of Rust-side types; padding bytes are arbitrary garbage; layout
   is not canonical across compiler versions). B's "hash the memory" is
   simply not computable for facet-discovered values as they sit — they'd
   need a re-encode into a canonical image first, and a re-encode *is a
   walk*. So under B, the walk doesn't die; it survives as the canonical-
   image *producer* for the facet ABI. The one-mechanism question becomes:
   is "one mechanism" the hash *definition* (blake3 over a canonical
   image, with two ways of producing the image — weavy values already ARE
   theirs, facet values get encoded into one), or must the *procedure*
   also be one (in which case A, the walk, is the only candidate that
   covers both ABIs uniformly)?
3. **Incrementality.** The carried hashers (the incremental array/map
   hashing that made the store-append path fast) compose naturally with B
   — appending to a molten array extends a hash over a memory region. A
   can do the same in its specialized form, but the framing bytes make
   the incremental state fiddlier.

**What does NOT differ**: speed at the flat case (both are ~one blake3
over bytes once A is specialized); edge-only consumption; the ordering
question (already ruled: canonical order is content order via `<=>`, hash
bytes never ordering-visible — so ordering is mechanism-blind either way);
receipts and bundles (they re-derive/verify whatever the one definition
says — they need *a* definition, not a particular one).

**The decision, one line:** the content hash's single definition is either
**(A) the schema-specialized walked encoding** — uniform across both ABIs,
layout-independent, framing bytes as the price — or **(B) blake3 over the
canonical memory image** — layout-coupled, framing-free, native to weavy
values, with the walk surviving as the image-producer for facet-discovered
values. The facet question in (2) is the fulcrum: if "one mechanism" must
mean one *procedure*, A wins by default; if it means one *definition of
the hashed bytes*, B is coherent and the flat case gets its speed by
construction rather than by specialization.
