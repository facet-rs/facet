# The corpus-first campaign — state and reading order

Everything in this directory serves one directive: port the essential
corpus to the language-as-designed BEFORE implementation; learn from the
feel; adjudicate from evidence.

## Reading order

1. This page.
2. `ANALYSIS-adjudication-queue.md` — the ruling queue (groups A–D).
3. `ANALYSIS-implementability.md` — 43 semantic findings, ranked.
4. `ANALYSIS-reader-feel.md` — how it reads; per-file verdicts.
5. The ports + GAPS files, annotating as you go.

## The state

Ports: rodin+index 2209→1811, crate 2060→1624, cargo_manifest 2542→2041.
Total 6811→5476 (−1335, ~20%), commentary intact, meaning preserved,
zero pop/push/_tuple/doc_string, 56 Rust probe fns → 13 native tests.

Fixed during the campaign (no ruling needed): identity.md's struck
canonical-memory rule (implementability P0 #1 — doc drift, killed);
crate.vix's surviving match pyramid; rodin's stripped commentary
restored with honest recounts.

## The two rulings that gate everything (queue items C1+C2)

**C1: is `<=>` user-overridable, and C2: what exactly is the structural
total order?** `Version` genuinely needs a custom order (semver: a
prerelease sorts BELOW its release — no structural order of the fields
gives that). But multiset observation order, map row canonicalization,
and canonical order are semantic-plane guarantees since round 5 — and map
row order feeds map CONTENT HASHES.

RECOMMENDATION (Fable): two orders, split by plane.
- The **structural order** (C2: declaration-ordered — variants by
  position, fields in declaration order, per the July-4 sketch rule)
  is machine-owned, total by construction, and owns the IDENTITY plane:
  canonical observation order, map row order, anything that feeds a
  hash. User code cannot touch it, so user code cannot destabilize
  identity or provoke cache-invalidation storms by editing a comparator.
- A **user `<=>`** (C1: yes, via `namespace`) owns the PROGRAM plane:
  `<`, `<=`, `sorted()`, `find_min`, `take_min`, `fold_ascending`.
  Pure functions mean even a weird comparator stays deterministic.
- The cost, stated honestly: for a type with a custom order, a
  multiset's observation order differs from its `sorted()` order.
  That needs a book callout and possibly a rung; the alternative —
  user code deciding identity-plane byte order — is worse in kind.

## After the rulings, in order

1. **The shadow stdlib gets chapters** — the queue's headline: ports
   call `.get()` ×153, `.unwrap()` ×135, `.insert()` ×77 against Map/
   Set/Option/Result/String/Path/Tree/Version/Doc, none of which have a
   book page. (And `.push()` ×10 against a chapter that bans it twice —
   the shadow lib even forced rule violations.)
2. **The failure surface** — three ports hit the same wall in three
   disguises (empty-map `.get().unwrap()` as an error-raise; sentinel
   rows; `assert_eq(false, true)`): there is no way to say `fail`. The
   zoo's `fail` + interpolated message is the salvage candidate;
   error.md's "governing incident" was exactly this.
3. **`find_map` and friends** — ruled into the iteration proposal,
   dropped by accident in the book, reinvented five times under four
   names in the ports. Restore with names-carry-semantics treatment
   (array: bare `find_map`, field order; multiset: min/max-named).
4. **Corpus laziness tests** — 5,476 lines and nothing asserts
   `never_demanded`: the loudest claim in the book has zero corpus
   exercises. Add demand-semantic tests to the ports (the fixtures
   already exist for index-not-fetched).
5. testing.md rewrite to the generator shape + the seven-row drift
   inventory; the nine cross-doc contradictions (stale iteration.md §4
   vs collections.md being the sharpest).
