# CRITIC: reading the book as a reader, against the corpus as ground truth

> [!NOTE]
> This is a pre-round-13 reader snapshot. Collection examples using optional
> `Map.get`, `.get(...).unwrap()`, `.push`, or `.insert` document the old
> contradiction and have been superseded by addressed `get`, `has`, `+`, `++`,
> and `with` in `SURFACE.md` and the migrated corpus.

*Seat: a competent Rust/Haskell/JS programmer, reading the book in its own weight
order for the first time, then opening the ported corpus cold. Every claim below
is grounded in a `file:line` I opened this session. Section 7 says what I did not
read.*

## Outcome, up front

The book teaches its two hardest ideas (demand-not-action, yield-is-availability-
order) well. It does not teach — anywhere, in any of the eight chapters read in
order — the calling convention the language actually uses. That is not a phrasing
complaint: five of eight chapters' code samples, including the entire capstone,
are written in a calling convention SURFACE.md calls superseded, and would not
parse under the grammar SURFACE.md ratifies. This is the number one finding.
Ranked list follows; per-chapter verdicts and three rewritten passages after that.

---

## 1. The book never teaches `where { }` — and mostly doesn't use it

`grep -rn "named argument\|positional argument\|juxtaposition\|calling convention"`
across all eight chapters: **zero hits.** SURFACE.md §1–2 states the calling
convention as foundational — "at most one positional argument," "application is
juxtaposition," parens are grouping, never call syntax, lineage Swift/Smalltalk —
and it is the very first thing SURFACE.md establishes. No chapter in the reading
order explains it as a concept. A reader meets `where {…}` used, unexplained, cold,
for the first time at `effects.md:37` (`fetch "https://…" where { sha256: "…" }`)
with no note saying what `where` is, why it exists, or that it replaces ordinary
parenthesized arguments.

Worse: most of the book doesn't follow it either.

| chapter | convention used |
|---|---|
| `values.md:48` `fn put_domain(state: State, pkg: PkgId, domain: Domain) -> State` | old (3 positional, no `where`) |
| `demand.md:16` `let solution = solve(manifest, index);` | old (2 positional) |
| `testing.md:24` `yield expect_eq(p.x, 3);` (sig at `testing.md:47`: `expect_eq(a: T, b: T) -> Check`) | old |
| `effects.md:37,142` `fetch url where { sha256: … }` | **v2, correct** |
| `std/collections.md:91` `rows.sorted where { order: by_key(…) }` | **v2, correct** (one line) |
| `building-a-solver.md:27,55,94` `index_row(registry, pkg, version)`, `propagate(state, row)`, `try_candidates(state, pkg, cands)` | old, throughout |

Only `effects.md` and one line of `collections.md` use the ratified grammar.
`building-a-solver.md` — the capstone, "the postcard" of the real solver — never
does. Its own `propagate`, `search`, `try_candidates`, `try_candidate` all take
2–3 bare positional arguments in parens with no `where`. Compare the ground
truth it claims to describe: `rodin.vix:1012` is
`fn propagate(index: Index) where { state: State, target: Target } -> SearchResult`,
and `rodin.vix:988-1001` (`try_candidates`/`try_candidate`, quoted verbatim in
`ANALYSIS-reader-feel.md:16-31` as "the book's own 'Backtracking is free' section
... in the real corpus") actually has *six* bare positional parameters
(`index, state, target, pkg, version, rest`) — itself a violation of "at most one
positional," logged by the porter as painful at `GAPS-rodin.md:18`
("try_candidate ... reads like a continuation record. PROPOSAL: introduce
CandidateAttempt").

So: the rule is stated once (SURFACE.md), taught nowhere, followed inconsistently
even by the book, and violated by the ground-truth code the book claims is
"about fifteen hundred lines... the real one" (`building-a-solver.md:162`). A
reader who typed the book's own capstone code into a vix file would, per the
language's own ratified grammar, get a parse or arity error on nearly every
function in it.

## 2. The book's central undocumented behavior: `.unwrap()`

Corpus-wide counts (`rg -oE` across all four `.vix` files):

```
.get(       152      Option<     109      Result<       0
.unwrap()   135      .insert(     85      .filter(      0
.fold(       91      .contains(   42      .sorted(      0
.len()       28      .keys()       5      .rem(         0
```

`.get(k) -> Option<V>` is documented once (`std/collections.md:239`).
`.unwrap()` is used **135 times** in the real corpus and appears **nowhere** in
the book — not defined, not mentioned, not contrasted with anything. The book's
entire premise is "no effects, no exceptions, no statements that 'do' without
denoting" (`_index.md:49-50`) and "a failing check renders both sides... you get
every failure in one run" (`testing.md:54-64`) — but `.unwrap()` on `None` is a
partial function firing mid-evaluation, in ordinary (non-test) code, throughout
production-shaped corpus files, and the book gives a reader no model for what
that *is* under demand-driven, replayable, memoized evaluation. Does it poison
the memo entry? Does it fail the enclosing demand as a value (some `Result`-
shaped thing) or does it abort the evaluator? `Result<` has **zero** occurrences
in the whole corpus, so whatever `.unwrap()` unwraps from `None` is not being
routed through a `Result`.

This is not a documentation oversight the porters missed — it's an admitted,
open design hole: `SURFACE.md:271-273` lists `fail` under "NOT banked... the
failure surface is queue item C3 — **three ports weaponized `.get().unwrap()`
for want of it**." All three GAPS files confirm it independently:
`GAPS-crate.md:76` ("impossible/malformed cases still use empty-map
`.get(...).unwrap()` to raise an error"), `GAPS-cargo-manifest.md:57`
(`assert_eq(false, true)` in place of a real failure primitive). A reader hits
this construct 135 times before ever finding a single sentence about it, in
any chapter, at any weight.

## 3. Reading order: `effects.md` (w25) forward-references `collections.md` (w30)

Confirmed exactly as the mission suspected. `effects.md:21`:

```vix
let objects = src.glob("*.c").map(|c| exec cc!{ … } ).collect();
```

uses `.glob()`, `.map()`, `.collect()`, and (`effects.md:101`) the `Tree` type —
none defined before this point in the reading order. `Stream`, `Map`, `.collect()`,
`.values()`, and `Tree = Map<Path, Blob>` are all first *defined* in
`std/collections.md` (site weight 30, one slot later). `grep` confirms
`three-planes.md` (w20) makes no such forward reference — only `effects.md` does.
A reader hits a keyed-stream pipeline they have no vocabulary for, one chapter
before the chapter that supplies it.

Separately, `three-planes.md` (w20) is a difficulty cliff regardless of forward
references: it opens with Merkle-root two-step reasoning and closes discussing
reversed-path tries, ribbon filters, Nominal Adapton, Skyframe's SkyKey, and
Watchman's clockspec (`three-planes.md:203-246`) — six pieces of outside prior
art — before the reader has seen `exec` or `fetch` even once (those arrive in
the *next* chapter). The chapter is correct and probably necessary, but it is
paced as if for a compiler-team audience, sandwiched between `testing.md` and
`effects.md` in a reading order pitched at first contact.

## 4. The stream/array prevalence claim doesn't survive the corpus

`std/collections.md:104`: "**Most things are streams**... Almost nothing else
should be an array" (`:189`). Counting the actual 6,005 lines:

```
[T] array-type annotations:  rodin 32 + index 23 + cargo_manifest 96 + crate 56 = 207
Stream<...> annotations:     cargo_manifest 13 (ALL are `-> Stream<Check>` test fns), 0 elsewhere
.stream() calls:             crate.vix 2, 0 elsewhere
.collect() calls:            cargo_manifest 1 + crate 3 = 4 total
```

Arrays outnumber streams roughly 16:1 by type-annotation count, and a real
`glob → map → collect → values` pipeline — the book's own running example,
repeated in both `effects.md:21` and `std/collections.md:127-141,171-173` —
occurs **exactly once** in the entire corpus, at `cargo_manifest.vix:375-378`.
Every other `.collect()`/`.stream()` site (`crate.vix:727,1268,1278,1289,1295`)
is calling `.collect()` directly on an already-existing array or using an
invented `.filter_map()` that appears in no book chapter — both logged as gaps
by the porter (`GAPS-crate.md:87-92`), not endorsed usage.

Also worth flagging: `Stream<Check>` (one type parameter, `testing.md:22` and
13 sites in `cargo_manifest.vix`) and `Stream<K, V>` (two type parameters,
`std/collections.md` throughout, also `SURFACE.md` §7) are never reconciled
anywhere. A reader who reads `testing.md` first (w15) meets a single-parameter
`Stream<Check>`; by the time they reach `collections.md` (w30) `Stream` has
grown a second parameter with no chapter explaining the relationship (sugar?
different type? `Stream<Unit, Check>`?).

**Verdict: this is a lie by the book's own standard** ("this is idiomatic ... not
history" — the book claims a present-tense fact about how vix code looks, and
5,000+ real lines say otherwise, 16:1).

## 5. `.values()`: mostly punctuation, but the porters say "ceremony" too — read their answer

The mission asks to check the porters' own verdict before forming one. All
three GAPS files answer directly:

- `GAPS-rodin.md:16`: all 7 sites read as **punctuation** ("the call is useful
  because it marks the one compaction").
- `GAPS-crate.md:38-42`: punctuation at its 2 real sites, but explicitly:
  "It would read as **ceremony** if it appeared on ordinary authored arrays;
  this file now has only those two real v2 compaction sites" — i.e. the porter
  is reporting they *avoided* the ceremony case, which means it's a real
  failure mode, not a hypothetical one.
- `GAPS-cargo-manifest.md:21`: "It reads as **ceremony** in
  `cargo_manifest.vix:911` and `:915`, where a `Set<String>` is immediately
  compacted back to `[String]` only because later text renderers still want
  arrays."

Combined count: 12 real `.values()` call sites across the corpus, of which the
porters themselves call 2 "ceremony." The book's framing ("This is the only
compaction in the language... happens once, on a map that already exists" —
`std/collections.md:164-165`) holds up as *mechanism*, but its implicit claim
that this always reads as clean punctuation is contradicted by a quarter of
the porters' own examples. The book should say when it turns to ceremony
(compacting a `Set` that only exists to compact it back to an array for a text
renderer), not just when it doesn't.

## 6. Two voice violations against the book's own stated standard, plus one invented, undocumented construct

Voice (`grep -n "we \(decided\|built\|measured\|tried\)\|predates\|used to"`):

- `_index.md:73-75`: "(This doctrine predates the current design — it was
  already the rule in the old vixen language, before vix was called vix.)"
  — pure history/attribution, in the *first chapter a reader ever opens.*
- `_index.md:111`: "This is not hypothetical: **we built one and measured**
  the drowning at four orders of magnitude." — first-person design-process
  residue.
- `three-planes.md:36`: "...it stayed hidden until **we tried to formalize**
  the dance" — same pattern.

Invented syntax: `building-a-solver.md:60` — `s.domains[dep.pkg].intersect(…)`
uses `Map` bracket-indexing. `std/collections.md` defines exactly one Map
read operation, `.get(k) -> Option<V>` (`:239`), never `m[k]`. The real corpus
never does this either — `grep -oE '\.[a-z_]+\[[a-z_]+\]'` across all four
`.vix` files returns **zero** matches. The capstone's one indexing expression
is syntax that exists nowhere else in the language's own worked examples or
its 6,000 lines of ground truth, and there's no divergence note warning a
Rust reader that this might (unlike `.get()`) panic on a missing key the way
`HashMap::index` does.

## 7. Missing chapters, ranked by "reader cannot proceed" vs "reader can guess"

Corpus-wide occurrence counts, cross-referenced against `grep -rln` for a
book page:

| symbol | corpus occurrences | book page? | verdict |
|---|---|---|---|
| `.unwrap()` | 135 | **none, anywhere** | **cannot proceed** — see §2 |
| `where { }` / named args | pervasive (316 sites in rodin+index alone, `GAPS-rodin.md:17`) | **none** — only demonstrated, never taught | **cannot proceed** — see §1 |
| `String` | 695 | none | can mostly guess (familiar type), but `.lines()` is used nowhere and its absence is a logged gap (`GAPS-cargo-manifest.md:55`) with no book resolution |
| `Tree` | 247 | one line, `Tree = Map<Path, Blob>` (`std/collections.md:48`); no chapter on `.glob()`, which the book's own flagship examples call | can guess shape, cannot guess operations |
| `Version` | 106 | none (only appears as a *worked example of structural order*, `std/collections.md:73-82`, and `SURFACE.md` §5 — never as "here is the `Version` type and its API") | can guess fields, cannot guess parsing/`VersionSet` API used everywhere in the solver |
| `Option<` | 109 | one line (`.get(k) -> Option<V>`) | can guess from Rust background, but `.unwrap()` (its main real-corpus consumer) is undocumented |
| `Path` | 77 | one line (`p""` literal, `std/collections.md` doesn't exist for it; values.md doesn't cover it either) | can guess literal syntax, cannot guess `.with_ext()` (used in `effects.md:21` itself!) |
| `Doc` | 17 | **none** | **cannot proceed** — used at `cargo_manifest.vix:14,2032` with no definition in book or corpus; a reader has no idea what this type is or does |

Ranked: `.unwrap()` and `where` are the two "reader cannot proceed" gaps that
matter most because they are not narrow domain vocabulary (`Doc`, `Version`) —
they are core-language mechanisms used in nearly every function, and both are
either entirely undocumented or actively contradicted by the book's own code.

## 8. `fetch`'s flagship example doesn't match its one real use

`effects.md:34-58` builds a careful, well-taught story: `fetch` is pinned, its
identity is known before evaluation, a checksum is right there in the source.
`building-a-solver.md:26-30` extends it: `fetch(registry.row_url(pkg, version))`,
presented as an ordinary pinned fetch. The corpus's one actual `fetch` call,
`index.vix:95`, is:

```vix
let snapshot: Tree = fetch(url: `${base_url}/${sparse_index_path name}`);
```

No checksum. It fetches a **mutable, unpinned sparse-index snapshot** — exactly
the case `effects.md:53-58` says is *not* a fetch: "A read whose result identity
is unknown until you perform it is a different thing — an observation, not a
fetch... **An ambient read is an observation. An input is a pin.**" The porter
says so directly: `GAPS-rodin.md:31`, "`fetch(url: ...)` is inherited from the
old corpus; SURFACE.md mentions fetch but not the final named-only expression
spelling... PROPOSAL: ratify `fetch where { url, sha256 }`." The book's cleanest
worked example (pinned `fetch`, checksum in source, "verifiable by a stranger")
describes a primitive that, when the corpus needed it for the one thing a
package-index solver actually needs to fetch, doesn't apply — and the missing
"observation" primitive effects.md itself calls for (`SURFACE.md:222-224`,
"Primitive TBD — log it") still doesn't exist. A reader who internalizes
`effects.md`'s fetch story will be surprised by `index.vix:95`.

---

## Per-chapter verdicts

- **`_index.md`** — Strong opening thesis, mostly present-tense. Two voice
  leaks (§6). The Salsa/Bazel/Adapton/GHC/Nix lineage section (`:196-213`) is
  legitimate technical contrast, not attribution residue — kept it out of §6.
- **`values.md`** — Clean, well-scaffolded, best "Coming from" coverage in the
  book (Rust/JS/Haskell all addressed for the same idea, `:98-110`). Its own
  code (`:48`) already breaks from the ratified calling convention (§1).
- **`demand.md`** — The best-taught chapter for the hardest idea (3a in the
  mission): `never_demanded` is set up two chapters early via the "eager
  arguments" JS-vs-vix contrast (`:54-71`) before `testing.md` needs it. Also
  uses the old calling convention throughout (`solve(manifest, index)`, `:16`).
- **`testing.md`** (rewritten today) — The strongest chapter in the book.
  Teaches yield-is-availability-order (3b) with a concrete example, a named
  contrast to "every other generator," and a two-kinds-of-Check framing that
  explains *why* the rule must be true, not just that it is. Its
  `Stream<Check>` single-param usage is never reconciled with collections.md's
  two-param `Stream<K,V>` (§4).
- **`three-planes.md`** — Correct and important, but a difficulty spike placed
  right after the easiest chapter and right before the concrete one (§3). A
  first-time reader gets Nominal Adapton and ribbon filters before `exec`.
- **`effects.md`** (brand new today) — The best-argued chapter for 3c: "So
  where does the fetch happen?" (`:139-151`) is a genuinely satisfying, concrete
  answer, and the divergence note on `Target::host()` (`:109-111`) is exactly
  the right shape. Undercut by two things: it forward-references
  `collections.md` (§3), and its own flagship `fetch` example doesn't match the
  corpus's one real use (§8).
- **`std/collections.md`** (rewritten today) — Precise, well-organized
  reference prose ("one collection structure, four names" is a genuinely good
  organizing idea). Its central prevalence claim about streams doesn't survive
  the corpus (§4), and it never mentions `.unwrap()`, `where`, `Doc`, `Path`
  methods, or `Version`'s actual API despite being the natural home for at
  least the first two.
- **`building-a-solver.md`** — Rhetorically the best chapter in the book (the
  backtracking-is-free argument is genuinely persuasive, and
  `ANALYSIS-reader-feel.md:16-31` independently confirms the real
  `try_candidates`/`try_candidate` in `rodin.vix:988-1001` delivers on it). But
  its code is written in a calling convention the language's own ratified
  surface superseded, uses undocumented indexing syntax not found anywhere
  else in the language (§6), and its `fetch` example elides the one case the
  real solver actually needed (§8). The chapter that should be the strongest
  proof that the book describes real code is the one most out of sync with it.

---

## Three rewrites

### A. `building-a-solver.md:55-108` — calling convention and Map access

Original uses bare 2-3-positional calls throughout and one invented `m[k]`
index. Rewritten to the convention `effects.md` and `SURFACE.md` §1-2 actually
ratify, and to the one documented Map read (`.get` + `.unwrap`, which — until
§2 above is fixed — is at least what the real corpus does):

```vix
fn propagate(row: Row) where { state: State } -> Step {
    row.deps.values().fold(Step::Pass(state), |step, dep| {
        match step {
            Step::Conflict(ng) => Step::Conflict(ng),
            Step::Pass(s) => {
                let narrowed = s.domains.get(dep.pkg).unwrap().intersect(dep.req);
                if narrowed.is_empty() {
                    Step::Conflict(no_good_for(s, dep))
                } else {
                    Step::Pass(State {
                        domains: s.domains.insert(dep.pkg, narrowed),
                        ..s
                    })
                }
            },
        }
    })
}
```

```vix
fn search(pkg_pick: State) -> Outcome {
    match pick_undecided(pkg_pick) {
        None => Outcome::Solved(pkg_pick.selected),
        Some(pkg) => try_candidates(candidates(pkg_pick, pkg))
            where { state: pkg_pick, pkg },
    }
}

fn try_candidates(cands: [Version]) where { state: State, pkg: PkgId } -> Outcome {
    match cands.split_last() {
        None => Outcome::Exhausted(no_good_for_exhaustion(state, pkg)),
        Some((v, rest)) => match attempt(v) where { state, pkg } {
            Outcome::Solved(sel) => Outcome::Solved(sel),
            Outcome::Exhausted(ng) =>
                try_candidates(rest) where { state: learn(state, ng), pkg },
            //              ^^^^^^^^^^^^^^^^^ backtracking is THIS:
            //              we still have `state`. The failed branch's
            //              hundred domain-narrowings belong to a value
            //              we simply stopped using.
        },
    }
}
```

### B. `_index.md:106-112` — remove design-conversation residue

Original:

> Taken literally, "everything is lazy" is unimplementable at any speed worth
> having, and "everything is memoized" is worse — you cannot memoize `a + b`,
> and it would be senseless to: the memo entry costs more than the addition,
> the identity hash costs more than the addition, the receipt costs more than
> the addition. A naive demand-driven evaluator drowns in its own
> bookkeeping. This is not hypothetical: we built one and measured the
> drowning at four orders of magnitude.

Rewritten:

> Taken literally, "everything is lazy" is unimplementable at any speed worth
> having, and "everything is memoized" is worse — you cannot memoize `a + b`,
> and it would be senseless to: the memo entry costs more than the addition,
> the identity hash costs more than the addition, the receipt costs more than
> the addition. A naive demand-driven evaluator drowns in its own
> bookkeeping — measured at four orders of magnitude slower than the islands
> design below.

(And delete `_index.md:73-75` — "(This doctrine predates the current design —
it was already the rule in the old vixen language, before vix was called
vix.)" — entirely. It tells a reader nothing about vix; it tells them about
vix's git history.)

### C. `std/collections.md:104-106` — the stream-prevalence opening

Original:

> Most things are streams. A stream is what you get when you ask the world for
> something — the files in a directory, the results of two hundred compiles
> running at once.

Rewritten, honest about what the corpus actually does (§4):

> Streams show up at the seam where the outside world hands you rows whose
> order you don't control — the files in a directory, the results of two
> hundred compiles running at once. Most collections you construct yourself
> are arrays or maps; a stream exists only to carry those outside rows safely
> to a `collect()`, before anything downstream can depend on an order nobody
> chose.

---

## What I did not verify

- I did not read `spec/*.md` (the formal spec tree) beyond `grep`-checking it
  for `Option`/`Result`/`Version` page existence — it's a different, more
  formal audience than this mission's reader persona, and the mission's
  reading order excludes it.
- I did not read `ANALYSIS-implementability.md` or
  `ANALYSIS-adjudication-queue.md` at all, and only skimmed the first ~60
  lines of `ANALYSIS-reader-feel.md` (used once, at §1, to corroborate — not
  originate — the `try_candidates` finding independently reached via my own
  `rodin.vix` read).
- I did not compile or run anything — `vx test --ratchet` was not invoked;
  all findings are static, textual, and grep/read-based against the corpus
  and book as committed.
- I did not read `vix/tests/ratchet/*.vix` (the numbered conformance ladder
  mentioned in `testing.md:159-164`) despite it being visibly modified in git
  status at the start of this session — out of scope for this mission, which
  targets the book and the four corpus-next ports specifically.
- Line counts for `[T]`, `.get(`, etc. are `grep -oE`/`grep -ohE` occurrence
  counts, not AST-verified call-site counts; a small number could be inside
  comments or strings. Spot-checked several (`.values()`, `fetch(`,
  the Map-bracket-index pattern) by reading surrounding context directly.
