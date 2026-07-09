+++
title = "heritage: the vixen-era language design vs the book"
+++

Comparison of the previous design round (`~/vixenware/vixen/docs/design/`:
`vix-spec.md` V1–V32, `vix-language-design.md`, `sketches/types.vix.md`,
`tree-sitter-vixen/grammar.js`) against the current book + machine spec.
Three buckets: independently re-derived (convergence = evidence the design
is real), genuinely improved since, and things the old design had that the
new corpus has NOT re-derived — the import candidates, which are the
payload.

## Independently re-derived — the convergences

These were arrived at again this round without consulting the old texts,
sometimes verbatim:

- **Everything hashable, equatable, totally ordered — no derive layer.**
  Old rule: "the language cannot express a type that can't be a memo key."
  V12 even states *"collect yields the canonical total order, never
  arrival order; scheduling nondeterminism is unobservable by
  construction"* — the canonical-order doctrine, word for word, including
  **ordered-float semantics** (re-derived yesterday as ratchet rung 134).
- **Values, not places.** No references, lifetimes, `mut`, `Box`;
  `env.insert(k, v)` returns a new map — the Values chapter, five days
  early.
- **Purity; effects only in primitives; no async surface ever; demand IS
  the await** (V1, V3) — Description-not-action, older phrasing.
- **Nothing forces locally** (V4, with the war note "killed attempts 1–4
  when violated") — the no-in-program-forcing law, banked again in
  changelog round 3 as if new.
- **Projections are free; identity forces** (V5) — partial dependency.
- **Two-tier identity: fingerprint × read-set, the anti-Nix pillar** (V9,
  round 4: "LOAD-BEARING design pillar"), including tier 2 skipping work
  *"even when the tree changed, if changes miss the read-set"* — this IS
  the two-step dance, formalized in the old corpus after all; the piece
  genuinely missing then AND now was candidate nomination, which the
  location plane now supplies.
- **Conservative caching: false positives never** (V8).
- **Capability daemon: advertise ⇒ watch ⇒ poison** (V28); **command
  grammars ride with the capability** (V29); both survived into the
  machine spec near-verbatim.
- **The result pin is the authority; recompute is the audit** (V16) —
  today's `fetch-observation-pin` rule.
- **Pull-based streams where "rustc .rmeta mid-compile → downstream
  fires"** — progressive exec trees, and ratchet rung 128.
- **Iteration** (old round 3): map/filter = parallel fan-out, fold =
  sequential chain, "cost model legible in source" — refined this round
  into the array/multiset type split and names-carry-semantics.
- **One implementation, typed instructions over untagged operands,
  interp==JIT differential incl. suspension traces** (V24) — the weavy
  doctrine and gate discipline.
- **Enforcement-per-invariant** (each V names its harness) — the ratchet's
  ethos; the old spec's §4 "what correct means, mechanically" is the
  foundation contract's ancestor.

## Genuinely improved since

- **Partition-as-filter fixes V11.** Old: "aggregation unit = memo unit" —
  which couples cache keys to the compiler's grouping, so repartitioning
  would strand caches. New: memo keys are partition-independent; the
  partition only filters which values are observed. This is the one place
  the old spec was *wrong* rather than early, and the new design knows why.
- **Islands are a theory where "auto-aggregate via cost heuristics" was a
  hope** — semantic cuts (effects, unprovable demand) vs cost-model cuts,
  the materialized-strictness-analysis argument, no programmer knob.
- **The location plane** — the old two tiers had no stable name for "the
  same computation across runs"; nomination was the unformalized hole.
- **Replay-is-semantics** — old had suspendable nodes and an enumerable
  frontier (V26); new makes restart the semantic model and suspension a
  discardable cache, with the chaos oracle enforcing it.
- **Content-hash definition** (schema-specialized walk, entry-carried
  identity, carried midstate) — deeper than V13's "canonicalized parse,
  everywhere."
- **Spec-as-textbook + the ratchet** — the old spec is an implementer's
  constitution; the new corpus also teaches, and its conformance ladder is
  executable.

## NOT re-derived — import candidates (the payload)

1. **V10, the blast-radius rule — the sharpest missing piece.** "The
   closure hash joins every memo key — covering the canonical ASTs of
   EVERYTHING the code references, transitively: functions AND type
   declarations." Proven live: a warm reload served a stale result after a
   leaf edit; an `#[ignore]`d blast-radius test was written as the fix's
   oracle. The new spec's recipe identity says "operation identity +
   inputs' identities" but never pins what *operation identity* covers —
   transitive callees? referenced type declarations? The old design paid
   blood for the answer. **Needs a rule, and a ratchet rung.**
2. **Observation upgrades.** Pins are addressable; *upgrade = re-evaluate
   while ignoring this observation*. This is the entire
   `cargo update`/lockfile-refresh story in one mechanism, and the new
   journal/receipt rules don't have it.
3. **Cargo.lock as a bidirectional journal backend** (V17) — "imported,
   not imitated"; source of truth, not a view. Product-critical for the
   drop-in wedge; absent from the new persistence spec.
4. **V22: the resolution result is a queryable artifact** — per-node
   derivations, provenance queries as reads, *counterfactuals as ordinary
   re-solves returning diffs + manifest amendments*, UNSAT returns its
   impossibility derivation. This is a decided shape for what the new
   corpus still tracks as the OPEN certificate-vs-derivation question —
   the old answer should at least be on that question's table.
5. **V29's degradation clause**: an argv item the grammar can't parse
   degrades to the widest assumption — *unparseable is slow, never
   unsound*. The new `no-argv-dialect` rule bans sniffing but never states
   the graceful floor.
6. **V25: the vix crate stays WASM-clean** — playground/browser story;
   nothing in the new spec protects it.
7. **Ecosystem closures** (V-vocabulary): the registry *ships vix code*
   (`versions_of` + `deps_of`); the solver doesn't know it's resolving
   crates. Current rodin work does this de facto (index.vix); the framing
   that it's a REGISTRY capability's payload is stronger and unstated.
8. **Prefetch-PGO** ("statistical model from previous runs prewarns tree
   materialization") and **`vix wtf`** (time-travel debugging UI, REPL) —
   product affordances worth keeping on a list somewhere they can be found.

## Divergences needing adjudication

- **RETRACTED after closer reading: generics brackets are NOT a fork.**
  tree-sitter-vixen's `[]` type arguments are an older layer that the
  types sketch (07-04) itself superseded — the sketch writes
  `Pair<Int, String>`, `Map<String, String>`, bans `T?` sugar, and uses
  `..base` spread. The sketch and the book agree on nearly the whole
  surface. The stale-vs-current split inside the OLD corpus is:
  grammar.js carries arrow closures `(x) => e`, pipes `|> _`, postfix `!`
  (invocation) and `?` (fallible), `value with {…}` updates, `${…}`
  interpolation, `is` pattern-tests, `#capability` tags on `Fn` types,
  `[]` generics, `T?` — while the sketch has already moved to the
  Rust-shaped surface the book uses.
- **`import("path")` function-style** (grammar.js) vs the book's bare
  `import geometry::{…}` (ratchet rung 106) — the sketch is silent;
  adjudication still wanted, low stakes.
- **`{}` as the empty-map literal is a book-side invention.** The sketch
  ruled "no map literal syntax v0: `Map::from([(k, v), …])` — boring on
  purpose"; ratchet rung 041 casually wrote `let m: Map<String, Int> = {};`.
  One of these is now the language.
- **Per-field struct defaults were already designed** (`opt: Int = 2` in
  the sketch) — the "go further than Rust" companion decision has prior
  art in-house; import the sketch's shape.
- **Dropped grammar features worth a deliberate verdict rather than
  silence**: `|>` pipelines with `_` placeholder, `${…}` string
  interpolation (the book concatenates), `is` pattern-test expressions,
  postfix `?` (this round PARKED ?-propagation; the old grammar had
  already committed a surface for it), and `#capability` effect tags on
  function types (the concrete surface for the still-open effect-system
  fork; round-4 verdict was "mhhaaaybe").
- **Old declaration-ordered total order** (variants by position, fields in
  declaration order — "reordering fields is a semantic change") vs the new
  corpus, which never pinned WHAT the structural order is. The old rule is
  probably right and should just be adopted explicitly.

## Feel

The old spec reads like an engine constitution — numbered invariants, each
with provenance and an enforcing harness, ruthless about the boundary
(open semantics / proprietary trust). The new corpus reads like a
language that expects to be learned. They are the same design DAYS apart (the
old round's rulings are dated 2026-07-01..04; this round ran 07-06..09) —
so the convergence is not independent validation but proof the convictions
are stable across restarts and that this round's agents, who never read
the old texts, re-derived them faithfully from conversation alone. The deltas that
matter are the four numbered imports above — especially V10, which is a
correctness hole with a recorded incident, not a taste question.
