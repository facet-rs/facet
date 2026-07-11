# Analysis: completeness/consistency + the adjudication queue

> **Post-migration note.** The four ports have since been migrated onto the
> ratified immutable-collection surface (`SURFACE.md` §6): `.push`/`Set.insert`
> became `+`; map `.insert` became `+ (k, v)` for known-new keys or `.with`
> for insert-or-replace; `Set.contains` became `.has`; and `map.get(k)` is now
> addressed (`V`, absence is typed `MissingKey`), so `map.get(k).unwrap()`
> collapsed to `map.get(k)` and optional lookups became `map.has(k)` then an
> addressed `map.get(k)`. Current counts: `.push` 0, mutation `.insert` 0,
> `map/set +` 73, `.with` 5, `.has` 41; the only surviving `.get(...).unwrap()`
> are the two unbanked `Doc` reads in `rodin.vix`. The retired-spelling counts,
> the `push` contradiction (1C-1, C4), and the `unwrap`-as-error questions
> below describe the **pre-migration snapshot** that motivated the ratified
> surface; they are retained as the audit trail, not as the ports' current
> state.

Seat: analysis (corpus-first campaign, round 6 directive). Inputs read: the
book (`vix/docs/content/**`, incl. `spec/machine/*`), `SURFACE.md`, the four
ports, the three GAPS files, `spec-changelog.md` rounds 1–6, and the design
notes (`combinators`, `iteration`, `exec-observers`, `innovation-points`,
`test-surface-forms`, `heritage-comparison`, `lesson-two-decisions`,
`reconcile-decisions`).

Two jobs, two parts. Part 1 is the audit: what the ports use that no chapter
defines, what the book claims that no port exercises, and where the corpus
contradicts itself. Part 2 is the ranked queue for Amos, deduped across the
three GAPS files.

**The headline.** `SURFACE.md` was the porter's law, but it is not the book,
and the gap between them is not small: `SURFACE.md` says "Sets per book" and
"Collections per the book chapter" — the book chapter (`std/collections.md`)
defines **arrays and multisets only**. It has no `Map`, no `Set`, no `Option`,
no `Result`, no `String`, no `Path`, no `Tree`, no `Version`, no `Doc`. The
ports call `.get()` 153 times, `.unwrap()` 135 times, `.insert()` 77 times and
`.push()` 10 times — every one of those against a type or a method the reader's
authority never introduces, and `push` is a method `std/collections.md`
explicitly says **does not exist**. The corpus does not overrun the surface at
the edges. It runs on a shadow standard library roughly the size of the
documented one.

---

## Part 1A — The shadow surface: constructs the ports use that no chapter specifies

Occurrence counts are total across the four ports; the per-port figures are
line counts. "Book" = `docs/content/**` excluding `spec/machine` (the spec
binds the implementation, not the language surface).

### The unspecified standard library

| Construct | Uses | Where the book mentions it | Verdict |
|---|---|---|---|
| `Map<K,V>` (`get`/`insert`/`keys`/`len`/`contains`) | `get` ×153, `insert` ×77, `keys` ×5 | `building-a-solver.md:43` uses `Map<PkgId,VersionSet>` in prose; `values.md:48` calls `.insert` | **No chapter.** The single most-used type in the corpus is undefined. |
| `Option<T>` + `.unwrap()` | `unwrap` ×135 | `testing.md:44` names `Option<T>`; `collections.md` returns it | **No chapter.** `unwrap` appears nowhere in the book. |
| `Result<T,E>`, `Ok`/`Err` | `cargo_manifest.vix:2023–2032` | nowhere | **No chapter, no type name.** `try_json_decode<T>` returns it; `e.message` is a `String`. |
| `String` ops: `before`, `after`, `contains`, `join`, `strip_prefix`, `parse_int`, `to_string` | `before` ×18, `after` ×17, `join` ×18, others ×8 | nowhere | **No chapter.** `.contains` is overloaded String/Array/Multiset with no note. |
| `Tree`/`Path` ops: `/`, `.text()`, `.glob()`, `.with_ext()`, `.collect()` | `p""` ×84 lines, `collect` ×3 | `demand.md:101` shows `tree / p"Cargo.toml"` once | **No chapter.** `Tree::collect` flagged by GAPS-crate; `glob`/`with_ext`/`text` flagged by nobody. |
| `Version` fields + `VersionSet` algebra (`from_req`, `contains`, `intersect`, `union`, `subset`, `complement`) | 9 sites | `building-a-solver.md:58` calls `.intersect`/`.is_empty` | **No chapter.** `v.major/.minor/.patch` used at `rodin.vix:27–33`; no `pre`. |
| `Doc` (dynamic JSON) + `json(text)` | `Doc` ×16 lines, `json(` ×5 | nowhere | **No chapter.** Only `spec/machine/primitive.md:128` gestures at "generic Doc access… for dynamic use." |
| `Arg::Str` / `Arg::Interpolation { tree, subpath }` | `crate.vix:562–575, 594` | nowhere | **No chapter.** `capability.md:44` bans the machine knowing argv dialects; it never gives the user an argv value type. |
| `Target::host()`, `Target` | 9 sites | nowhere | `spec/machine/primitive.md:136` promises "first-class value with **literal syntax**"; no literal syntax exists, ports call `Target::host()`. |
| `Rustc::acquire(...)` + `rustc! { … }` / `build_script! { … }` | `rodin.vix:452–453`; `crate.vix:588, 598, 610, 624, 636, 1211, 1304` | nowhere | **An entire unspecified sublanguage**: bare flags, `{expr}` splices, and mixed literal/splice (`--emit=metadata={metadata},link={link}`). GAPS-rodin:447 calls it "the blessed escape hatch"; nothing blesses it. |
| `fetch(url: …)` — **named arguments** | `index.vix:82` | `demand.md:101` and `building-a-solver.md:28` call `fetch(x)` positionally | Round 6 parked "positional application & `partial`" as OPEN. The port silently shipped keyword arguments. |
| `use vix::{…}` / `use caps::Rustc;` | 6 sites | `testing.md`/ratchet cite rung 106 `import geometry::{…}`; `heritage-comparison.md:131` records a third form `import("path")` | **Three spellings, no ruling.** |
| `try_json_decode<SparseIndexRow>(…)` — explicit type args at a call | `cargo_manifest.vix:2023–2024` | nowhere | Turbofish-without-`::` syntax, invented at the port. |
| `fixture_tree("name")` | `cargo_manifest.vix:1921–1988`, 9 sites | nowhere | Test fixtures have no mechanism (GAPS-cargo-manifest:1927). |
| `#[decode(rename = "…")]` | 10 sites in `cargo_manifest.vix`, mirrored in `crate.vix` | `SURFACE.md:31` says "decode annotations (shape per book)"; the book has no decode section | The surface deferred to a chapter that doesn't exist. |
| `namespace T { }` / `fn <=>(self, other)` | `rodin.vix:40, 1279`; `crate.vix:216` | nowhere in the book (ratified in changelog round 6) | Methods on user types have no chapter. `extend T { }`: **0 uses**. |
| `Stream<T>`, `yield` | `cargo_manifest.vix` ×13 tests, ×36 yields | `_index.md:97` ("Streams deserve their six lines"), `testing.md`: nothing | Generators are the test shape and have no chapter. |
| `must_use` | 0 uses | nowhere | Adopted round 6; no surface, no book text, no port evidence. |
| `.push(x)` returning a new array | `index.vix:96, 104, 203, 220, 284, 285`; `rodin.vix:337, 353, 1098, 1151` | `collections.md:33` and `:230`: **"there is no `push`"**, listed under *What deliberately does not exist* | **Direct contradiction.** See 1C-1. |

### The sharpest one: failure has no surface, so the ports weaponized `unwrap`

`crate.vix:1327` and `crate.vix:222` both do this:

```vix
fn malformed_build_script_directive(line: String) -> String {
    let missing: Map<String, String> = %{};
    missing.get(line).unwrap()
}
```

That is a deliberate lookup-miss-then-unwrap **used as the language's only way
to raise an error**. `GAPS-crate:220` names it ("no typed error construction");
`GAPS-rodin` and `GAPS-cargo-manifest` never see it, because they hit the same
wall differently — `index.vix:359` returns an empty-row sentinel instead.

The spec already ruled against every part of this, in a page whose opening line
is the incident report:

- `spec/machine/error.md:6` — *"The governing incident: a solve failing with the string `unwrap on None` and no location, no subject, no demand chain."*
- `error.md:21` — `Option` is not an error channel.
- `error.md:38` — a structural impossibility "is never folded into a legitimate-miss path."
- `value.md:33` — *"An 'impossible by construction' match arm holding a string error means the construction was never encoded in types."*

`heritage-comparison.md:214` lists `fail` (zoo-era, with interpolated messages)
as salvage item #3: *"the book's failure story covers machine failures only."*
The ports are the proof that the user-level failure story is load-bearing and
absent. This is C4 in the queue and I rank it highest of the semantic rulings.

---

## Part 1B — Untested doctrine: book claims no port exercises

`combinators.md:127` asked the right question — *"Which removal variants
actually earn v1? The corpus rewrite will show demand."* Here is the answer,
and it generalizes: the corpus rewrite also shows which doctrine has **zero**
demand.

| Book surface | Chapter | Port uses |
|---|---|---|
| `never_demanded` / `demanded` / `demanded_once` | `testing.md:80–84` | **0** |
| `expect` / `expect_eq` / `expect_ne` / `expect_some` / `expect_none` | `testing.md:41–46` | **0** (ports use `assert_eq` ×38) |
| `expect_snapshot` | `testing.md:46` | **0** |
| `.reject.vix` compile-fail files | `testing.md:93–106` | **0 files** |
| `take_min` / `take_max` | `collections.md:209` | **0** |
| `find_max` | `collections.md:201` | **0** (`find_min` ×2) |
| `sorted_by` | `collections.md:215` | **0** (`sorted()` ×7) |
| `flat_map` (multiset) | `collections.md:182` | **0** |
| `Indexed<T>` (the alias, named) | `collections.md:109` | **0** (`enumerate()` ×2) |
| `extend Type { }` | `SURFACE.md:23` | **0** |
| `#[test(...)]` with arguments | `SURFACE.md:31` | **0** |
| Multiset `map` | `collections.md:165` | **0** |

Three of these are worth stopping on.

1. **`never_demanded` is the flagship.** The laziness law is the book's first
   and loudest claim (`_index.md:31`, `demand.md:36`, `machine.scheduler.no-in-program-forcing`).
   `SURFACE.md:28` explicitly ratified trace checks as ordinary calls. The
   entire ported corpus — 2041 lines of `cargo_manifest.vix` with 13 tests —
   places **zero** trace expectations. Nothing in the corpus tests that
   anything is *not* computed. That is the single largest hole between the
   doctrine and its evidence.

2. **`expect` is missing where the ports needed it.** `cargo_manifest.vix`
   writes `assert_eq(x, true)` **11 times** (e.g. `:1952, :2017, :2038–2040`)
   and `assert_eq(false, true)` as a deliberate fail (`:2026, :2030`). Both are
   the shape of a missing `expect(cond)` and a missing `fail(msg)`.

3. **The removal-variant question is answered.** `split_last` ×3 (all in
   `rodin.vix`, all in the genuinely order-sensitive search); `find_min` ×2;
   `take_min`/`take_max`/`find_max` ×0. `combinators.md`'s lean — *"start with
   none and let a real site ask"* — should now be: keep `split_last` and
   `find_min`; `take_min`/`take_max` have not earned v1 and should be cut from
   `collections.md` until a site asks.

---

## Part 1C — Contradictions

### 1C-1. `push` — the book says it doesn't exist; the corpus uses it 10 times

`std/collections.md:33` ("there is no growth, no capacity, no `push`/`pop`
mutation family") and `:230` ("`pop`, `push`, `insert`, `remove` as
mutations — nothing mutates") vs `index.vix:203` (`packages:
state.packages.push(state.next_pkg)`) and nine siblings.

The escape is that `.push()` here *returns a new array* and is used as a value,
so it never mutates. But then `innovation-points.md:25` has already ruled on
exactly this shape: *"`insert`/`push` return the new value — DEFENDED: no
expression statements, so the fire-and-forget shape is a compile error."* So
`push` **is** meant to exist and return. `collections.md` says the opposite in
two places. One of these is the language.

Note the second-order damage: `SURFACE.md:20` says "NO `pop`" and lists the
collections chapter as law, so the porters believed `push` was legal (it isn't
in their law) and used it anyway — while `crate.vix` and `cargo_manifest.vix`,
porting the same idiom, used `[..out, x]` spread instead and reached **0**
`push` calls. The corpus is internally inconsistent because the book is.

### 1C-2. Array `map`: positions preserved, or canonical order?

- `std/collections.md:95` — *"the result's field `i` is `f(self[i])`. **positions are preserved**, partial dependency is preserved."*
- `content-design/iteration.md:260` — *"**`ys[i] = f(xs[i])` is dead.** `ys = xs.map(f)` observes the f-images in *their* canonical order; index `i` means 'i-th smallest'."*

`spec-changelog.md:336` (round 5) rules "positional order dies for derived
aggregates; construction stays positional," and `combinators.md:42` sides with
the book (field-wise mapping "has no ordering problem at all"). So the book is
right and `iteration.md` §4 is superseded — but `iteration.md` is still in
`content-design/`, still unmarked, and its §4 is the *only* place the
canonical-order doctrine is argued in full. Two of its downstream claims are
also stranded: *"`fold` runs in canonical element order"* (the book renamed it
`fold_ascending` and banned bare `fold` on multisets) and *"`find_map` returns
the first hit in canonical order"* (see 1C-3).

### 1C-3. `find_map` was ruled, then silently dropped — and all three ports want it

`iteration.md:168` puts `find_map` in the **v1 combinator set**;
`iteration.md:274` gives it a semantics ruling ("first hit in canonical order —
deterministic, parallelizable"). `combinators.md` (which "supersedes the flat
v1 list") does not list it. `std/collections.md` does not have it.

The three GAPS files then independently reinvent it: `GAPS-rodin:648, :988,
:1199` (three sites, plus `find_last_map`, `try_find_rev`, `find_map_rev`
proposals), `GAPS-rodin:359` on `index.vix` (`find_rev`),
`GAPS-cargo-manifest:641` (`find_last`). Five sites, three ports, four proposed
names. `last()` suffered the same fate (`iteration.md:171`, dropped, unused).

This is a dropped ruling, not an open question. It is B2 in the queue.

### 1C-4. `<=>` is ambient — and the corpus overrides it

- `values.md:113` — *"Every value supports `==`, `<=>`… **by construction**. You never derive them, never implement a trait."*
- `collections.md:65` — *"You never implement an ordering trait; a total order over all values of a type always exists."*
- `rodin.vix:40–58` — `namespace Version { fn <=>(self, other) -> Ordering { … } }`, ratified by `SURFACE.md:23`.

If `<=>` is overridable, then `sorted()`, `sorted_by`'s tie-breaks, multiset
canonical observation order, `find_min`, and `Map` iteration order all become
user-definable — and `spec-changelog.md:345` ("canonical order is CONTENT order…
defined over value order `<=>`") makes that a **semantic-plane** consequence,
not a convenience. Meanwhile round 5 says *"`Version` in std must implement
`<=>` — fix on contact"*, which presumes overriding is how std does it.

Nobody has said whether a user `<=>` replaces the structural order or is merely
a named comparator. This is C1 and it is the deepest one in the queue.

### 1C-5. The structural order itself is never pinned

`heritage-comparison.md:147` — *"Old declaration-ordered total order (variants
by position, fields in declaration order — 'reordering fields is a semantic
change') vs the new corpus, which **never pinned WHAT the structural order
is**. The old rule is probably right and should just be adopted explicitly."*
`innovation-points.md:41` independently files it as a trap needing "at least a
book callout." Everything in 1C-4 rests on it.

### 1C-6. The ratchet is 100 rungs, and the docs cite rungs 106, 128, and 134

`testing.md:110` — *"a numbered ladder: `001` through `100`… Rung 100 is a
working miniature of the solver chapter."* But `heritage-comparison.md:133`
cites rung 106 (`import`), `:46` cites rung 128 (progressive trees), `:23`
cites rung 134 (ordered-float); `exec-observers.md:79` cites rung 128;
`innovation-points.md:49` cites the "diagnostics band 111–122" and rung 013.
Either the ladder is longer than the chapter says, or those rungs don't exist.
Cheap fix, but it is the reader's authority disagreeing with five design notes.

### 1C-7. Tree-file → String: coerced in one port, `.text()`-ed in another

`index.vix:83` passes `snapshot / p"index.jsonl"` straight into
`fn parse_sparse_jsonl(text: String)`. `cargo_manifest.vix:187` writes
`(manifest / p"Cargo.toml").text()`. `crate.vix` uses `.text()` too. Either the
projection of a tree file coerces to `String` or it doesn't; the book's only
example (`demand.md:101`) binds it to `config` and never uses it.

### 1C-8. `%{}` map literals vs the book's silence

Round 6 ratified `%{ k => v }` and swept the ratchet's `{}`. Ports use `%{}`
51 times. `std/collections.md` — the only collections chapter — never mentions
map literals, because it never mentions `Map`. `heritage-comparison.md:216`
called this out as needing *"ONE ruling"*; the ruling happened, and the book
never heard.

### 1C-9. `combinators.md` still says `fold` on multisets

`combinators.md:76` lists `fold(init, f)` on `Multiset<T>` ("canonical element
order"). `collections.md:186` renamed it `fold_ascending` and states *"There is
no bare `fold` on multisets."* `combinators.md:83` also types `split_last()` as
`(T, [T])` where the book returns `Option<(T, [T])>`. Design notes drift; that
is expected — but `combinators.md` is the document `iteration.md` says
supersedes it, and neither is marked superseded by the book.

---

## Part 1D — The testing chapter drift inventory

`testing.md` is the chapter furthest from the corpus. It teaches, in order:

| `testing.md` teaches | Round-6 / port reality | Status |
|---|---|---|
| `test NAME { … }` block declaration (`:20`) | `#[test] fn name() -> Stream<Check>` (`cargo_manifest.vix:1919`, ×13) | **Dead.** `test-surface-forms.md` Form D adopted; changelog round 6 addenda. |
| `expect_eq` / `expect_ne` / `expect` / `expect_some` / `expect_none` (`:41–46`) | `assert_eq` only (×38); zero `expect_*` | **Vocabulary fork.** `SURFACE.md:27` hedges "`assert_eq`, `expect`-family"; `test-surface-forms.md:57` writes `assert_eq`. Pick one. |
| *"The test block is the one place expression-lines are allowed"* (`:29`) | Generator blocks contain bindings and `yield`s (`test-surface-forms.md:78`) | **Superseded.** The refined no-statements law is unwritten in the book. |
| `expecting { never_demanded expensive; }` mini-grammar (`:66–84`) | *"The expecting mini-grammar and header directives are dead"* (changelog `:445`); trace checks are ordinary calls | **Dead, unreplaced.** And unexercised (Part 1B). |
| `.reject.vix` + `//! reject:` / `//! at:` header directives (`:93–106`) | *"header directives are dead"* (same ruling) | **Dead, unreplaced.** Compile-fail testing now has **no** specified mechanism, while `innovation-points.md:25, :31, :51` leans on reject rungs as the primary defense for three traps. |
| ratchet `001`–`100` (`:110`) | rungs 106/111–122/128/134 cited elsewhere | Contradiction (1C-6). |
| — | `must_use` on `Check` (changelog `:464`) | **Missing.** The whole reason Form D is safe. |
| — | `fixture_tree(...)` (`cargo_manifest.vix:1921`, ×9) | **Missing.** No fixture mechanism exists. |
| — | Failure assertions written as `match` + `assert_eq(false,true)` (`:2022–2033`) | **Missing.** `expect_err_contains` (GAPS-cargo-manifest:2011). |
| — | Oracles/differential fixtures (`cargo_manifest.vix:1911–1917`, two text-pin "gap" functions) | **Missing.** GAPS-cargo-manifest:1911 and its "Missing Or Deferred Meaning" section. |

`testing.md` needs a rewrite, not an edit. The rewrite is D10 below and it is
the only chapter edit I would call urgent, because the conformance ladder is
*written against this page* (`testing.md:8`).

---

## Part 2 — The adjudication queue

Deduped across the three GAPS files. Evidence is `file:line` with a demand
count = how many distinct ports asked. Effort classes: **ruling** (one
sentence, no design), **std addition** (a function/type + a chapter section),
**chapter edit**, **design pass** (needs a document).

Ranked within each group by demand × blockage.

### (a) Already ruled — needs surface

| # | What | Ruled where | Evidence (demand) | Proposed resolution | Effort |
|---|---|---|---|---|---|
| **A1** | **Newtypes** | `iteration.md:369` — *"return 'bigly', and must be markedly more ergonomic than Rust's. Design pass owed."* Also `spec/machine/value.md:13` for the machine. | **3/3 ports.** `rodin.vix:107, 133, 155` (bare `Int` across pkg/feature/guard/clause/version namespaces); `index.vix:331`; `cargo_manifest.vix:32`. The book *already writes* `PkgId` at `values.md:48` and `building-a-solver.md:43, 94` — a type it does not define. | Do the design pass. Then `PkgId`/`FeatureId`/… land in rodin and the two chapters stop lying. Blocks A2's `Set<PkgId>` and B8's typed `Map<Id,T>`. | design pass |
| **A2** | **`Set<T>`** | `iteration.md:373` — *"first-class structure, obviously."* | **3/3 ports.** `rodin.vix:322, 635, 814` (three separate `[..ms.sorted(), x].values()` round-trips to insert one element); `cargo_manifest.vix:810` (linear `Array.contains` de-dup). | Add `Set<T>` with `.insert`/`.contains`/`.union`; give `collections.md` a third section. Kills B4 outright. | std addition + chapter edit |
| **A3** | **Per-field record defaults** | `iteration.md:374` — *"go further than Rust — per-field defaults."* Prior art in-house: `heritage-comparison.md:141` (`opt: Int = 2` in the types sketch). | **1 port, high volume.** `cargo_manifest.vix:1643` — `workspace_profile()` writes 13 fields; `registry_profile` repeats them; `crate.vix:40` mirrors the shape. | Import the sketch's `field: T = expr` shape. `registry_profile` becomes a one-field spread. | design pass (small) + chapter edit |
| **A4** | **Const maps / string-match tables** | `iteration.md:373` — *"agreed."* Machine twin: `spec/machine/value.md:19` bans string tag comparison. | **3/3 ports.** `crate.vix:1370` (7-deep `match key == "…"` ladder); `crate.vix:1333`; `cargo_manifest.vix:689` (`"normal"`/`"build"`/`"dev"`); `rodin.vix:490` (`doc_string(expr,"tag")` compared to `"all"`/`"any"`/`"not"`). | Ratify a match-on-string-literal table or a const-map dispatch form. Then C-side rulings on `DependencyKind` and `CfgExpr` become mechanical. | design pass |
| **A5** | ~~**Exec observers**~~ → **stdout as codata** | **SETTLED, round 12.** `r[machine.primitive.exec-outcome]`, `r[machine.primitive.exec-is-placement-agnostic]`. `exec-observers.md`'s *finding* stands (readiness is a protocol fact); its *API* is retired. | **2/3 ports still un-ported.** `crate.vix:1442`, `:1583` and `rodin.vix:497` route stdout **through a fake file in a Tree** (`--stdout {p"build.stdout"}`, `{p"cfg.stdout"}`) — written when stdout had no home. | **Stdout has a home**: `exec` returns `ExecOutcome { tree, stdout: Stream<Int,String>, stderr }`, and codata fields are consumed as the process runs. There is **no observer surface** — no `observer:` parameter, no capability-level default, no per-call override. To consume a stream remotely you **place the surrounding block**; the March observer closure is that block's *lowering*. **Owed:** re-port the three sites onto `out.stdout`. | re-port (mechanical) |
| **A5b** | Stream observation granularity | open | — | Does a placed block's stream consumption enter the read-set as **one** journaled stream observation, or **per-message**? Decides replay granularity and whether early cutoff can act on logs. Unchanged by round 12. | ruling |
| **A6** | **Typed argv fragments (`Arg`)** | `capability.md:44` (`no-argv-dialect`) rules the machine side. The *user* side is unruled. `heritage-comparison.md:105` flags V29's missing degradation clause. | **1 port, structural.** `crate.vix:566, 572` (`Arg::Str("dependency=")` + empty-subpath interpolation); `crate.vix:596–648` — **four near-identical `rustc!` arms** existing only because a path-valued `--emit=` fragment is not a value. | Ratify `Arg` (it already exists in the ports, imported from `vix::`) with a path/tree payload, or let `rustc!` take a typed argv record. Deletes ~50 lines of `crate.vix` and one duplication verdict. | design pass |
| **A7** | **Generators / `Stream<T>` chapter** | Round 6 addenda: *"a generator is an ordinary function whose return type is `Stream<T>`; `yield` in the body."* Streams are codata. | `cargo_manifest.vix` ×13 signatures, ×36 `yield`s. The book's total coverage: six lines of prose at `_index.md:97`. | New chapter, or a section in `values.md`. It is also the prerequisite for A5 (observers consume streams) and D10 (tests produce them). | chapter edit |
| **A8** | **`must_use`** | Round 6 addenda: *"ADOPTED as a type marker."* | 0 uses, 0 mentions. It is the safety net for the ratified test shape. | One paragraph in the (new) testing chapter + the type marker in `Check`/`Outcome`. | chapter edit |
| **A9** | **`namespace` / `extend` chapter** | Round 6: *"accepted as the direction."* `SURFACE.md:23`. | `rodin.vix:40, 1279`; `crate.vix:216`. `extend`: **0 uses** — import-scoped extension is entirely untested. | A methods chapter. `heritage-comparison.md:210` lists it as salvage #1: *"a user defining `fn area(self: Shape)` has no home."* | chapter edit |
| **A10** | **`Version::<=>` in std** | Round 5: *"`Version` in std must implement it (std currently advertises the contract and doesn't eat it — fix on contact)."* | `rodin.vix:40–58` hand-writes it, and it ignores prerelease. `crate.vix:1178` hard-codes `CARGO_PKG_VERSION_MAJOR/MINOR/PATCH/PRE` because `Version` exposes no accessors. | Land `<=>` + accessors + `pre` in std `Version`. Blocked on C1 (is a user `<=>` even a thing?). | std addition |

### (b) New std / combinator asks, with demand counts

| # | What | Demand | Evidence | Proposed resolution | Effort |
|---|---|---|---|---|---|
| **B1** | **`String::lines() -> [String]`** | **3/3 ports** | `index.vix:90–107` (recursive `before`/`after` walker, 18 lines); `crate.vix:1411` (same); `cargo_manifest.vix:1878` (same). All three GAPS files propose it independently. | Add it. The three walkers dissolve. Consider `Stream<String>` per GAPS-cargo-manifest:1878 so `filter_map(json_decode)` composes. | std addition |
| **B2** | **`find_map` family** | **3/3 ports, 5 sites** | `rodin.vix:648, 988, 1199`; `index.vix:359`; `cargo_manifest.vix:641`. Four names proposed across GAPS (`find_map`, `find_last_map`, `find_map_rev`, `try_find_rev`, `find_last`). | **This was already ruled in** (`iteration.md:168, :274`) and dropped. Restore `find_map` with the canonical-order semantics already written. The order-sensitive variants (`rodin.vix:988`'s demand-selective search) need C11 answered first. | std addition + ruling |
| **B3** | **Early-exit fold (`try_fold_ascending`)** | **2/3 ports** — and the **book teaches the workaround** | `rodin.vix:915` (`Step` accumulator carrying conflict short-circuit). `building-a-solver.md:55–72` — the chapter's centerpiece `propagate` is *exactly* this workaround, presented as idiomatic. | Either add `try_fold_ascending`, or accept the `Step`-accumulator idiom and say in the chapter that it is the idiom. Today the book teaches a workaround without knowing it is one. | std addition **or** chapter edit |
| **B4** | **`Multiset::insert_one` / `union_one`** | 1 port, 3 sites | `rodin.vix:322, 635, 814`. | **Subsumed by A2** for the unique cases. If genuine multisets need one-element insert, add it; otherwise close as duplicate of A2. | ruling |
| **B5** | **`Array::reversed()` / `fold_descending`** | 1 port, 2 sites | `index.vix:147, 150` — `rows.fold([], \|rev, row\| [row, ..rev])` written **twice** to preserve the old `pop` order, each an O(n²) spread chain. | Add `reversed()`. Cheap, and the current shape is a performance trap the reader will copy. | std addition |
| **B6** | **`Doc::as_array` / `Doc::object_empty` / `Doc::null`** | 2/3 ports | `rodin.vix:438` (rustc cfg output is a `Doc` linked list → forced recursion); `index.vix:378` (`json("{}")`, `json("null")` — stringly construction of empty docs, ×5 `json(` sites). | Mostly obviated by C13 (typed decode for these sites). What survives needs a `Doc` chapter — currently `Doc` is used in 16 lines and defined nowhere. | std addition + chapter edit |
| **B7** | **`Map` surface + `keys()` order** | **4/4 ports** | `get` ×153, `insert` ×77, `keys` ×5 (`cargo_manifest.vix:508, 516, 677, 709, 969` — three of them **fold state through `keys()`**, so determinism depends on an unstated order). | Write the `Map` chapter. State whether `keys()` yields `[K]` in canonical value order or a `Multiset<K>`. Folding over it (as three port sites do) is only deterministic if you answer. | chapter edit + ruling |
| **B8** | **`String` surface** | **4/4 ports** | `before` ×18, `after` ×17, `join` ×18, `contains`, `strip_prefix` ×3 (`crate.vix:1333` relies on `strip_prefix` returning the **unchanged string** on no-match — a silent, lossy contract), `parse_int` ×3, `to_string` ×2. | Write the `String` chapter. Make `strip_prefix` return `Option<String>`; `crate.vix:1333–1341`'s `match double_colon == line` is comparing strings to detect failure. | chapter edit + std addition |
| **B9** | **`Option` / `Result` / decode errors** | 4/4 (Option), 1 (Result) | `unwrap` ×135. `cargo_manifest.vix:2022–2033`: `try_json_decode<T>` → `Ok`/`Err`, `e.message.contains("missing field `vers`")`. | `Result` needs a name, a chapter, and a decision on `e.message: String` — `spec/machine/error.md:9` forbids `Result<_, String>` in the machine; a stringly `DecodeError.message` in the language is the same mistake one plane up. See C4/C5. | chapter edit + ruling |
| **B10** | **`Tree` / `Path` surface incl. `Tree::collect`** | 3/3 ports | `crate.vix:798, 867, 1067` (`[Tree].collect()` — unratified, GAPS-crate:867); `cargo_manifest.vix:294` (`glob`); `crate.vix:1572` (`with_ext`); `.text()` ×2. | Write the chapter. `Tree::collect([Tree])` needs a *name* that says "union with dependency semantics"; `collect` says nothing (names-carry-semantics, round 5). | chapter edit + std addition |
| **B11** | **Cargo-domain helpers** | 2/3 ports | `crate.vix:1563` (proc-macro dylib naming via local target-OS match); `crate.vix:1178` (`CARGO_PKG_VERSION_*` hard-coded). | Not language gaps — std/caps gaps. Park below the line, but they *are* real duplication (GAPS-crate's "duplication verdict"). | design pass (low) |

**Dropped from the queue as answered by the corpus:** `take_min`/`take_max`
(0 sites — `combinators.md:127` asked; the answer is no), `flat_map` on arrays
(`combinators.md:120` open question — 0 sites), `for` sugar (`iteration.md:309`
— the folds read fine; no site asks).

### (c) Semantic rulings needed (one sentence each)

Ordered by blast radius.

| # | Question | Why it blocks | Evidence |
|---|---|---|---|
| **C1** | **Can a user `<=>` (via `namespace T`) replace a type's structural order, and if so does canonical observation order follow it?** | If yes: multiset order, `sorted()`, `find_min`, `Map` iteration and *canonical order itself* become user-definable — and round 5 made canonical order a semantic-plane guarantee. If no: `rodin.vix:40` is illegal and `Version` needs a different mechanism. | `values.md:113` ("never implement"), `collections.md:65`, `SURFACE.md:23` (ratifies it), `rodin.vix:40–58` (does it), changelog `:345`, `:413`. |
| **C2** | **What is the structural total order — declaration order of fields/variants?** | Everything in C1 rests on it, and `..base` spread + field reordering silently changes sort order. | `heritage-comparison.md:147` (*"the old rule is probably right and should just be adopted explicitly"*), `innovation-points.md:41` (files it as a trap). |
| **C3** | **Does the language have a failure construct (`fail`, or `panic`, or a typed error value)?** | Two ports raise errors by `%{}.get(k).unwrap()`. A third returns a sentinel. The spec's error chapter opens with the incident this produces. | `crate.vix:222, 1327`; `index.vix:359`; `spec/machine/error.md:6, 21, 38`; `spec/machine/value.md:33`; `heritage-comparison.md:214` (salvage #3, `fail` with interpolation). |
| **C4** | **Does `push` exist?** (If it returns a new array, `collections.md` must stop saying it doesn't.) | 10 sites in two ports; 0 in the other two, which used `[..a, x]`. The corpus is split because the book is. | `collections.md:33, :230` vs `innovation-points.md:25` vs `index.vix:203`, `rodin.vix:337`. |
| **C5** | **Is `.unwrap()` a language surface, and what does it do on `None`?** | 135 uses. `spec/machine/error.md:32` designs the *machine's* answer (a `MachineError` with a span). The language chapter says nothing about `unwrap` existing. | ports, everywhere; `crate.vix:209` chains three. |
| **C6** | **Named arguments: yes or no?** | `fetch(url: …)` shipped in a port while round 6 has "positional application & `partial`" as OPEN. | `index.vix:82`; changelog `:451`. |
| **C7** | **`use` vs `import` vs `import("path")` — one spelling.** | Three in flight; `heritage-comparison.md:131` already asked and got no answer. | ports ×6 (`use`), ratchet rung 106 (`import`), grammar.js (`import("…")`). |
| **C8** | **Does a tree-file projection coerce to `String`, or is `.text()` required?** | Two ports disagree. | `index.vix:83` vs `cargo_manifest.vix:187`, `crate.vix`. |
| **C9** | **What order does `Map::keys()` yield, and is folding over it deterministic?** | Three port sites thread solver state through `keys().fold(...)`. | `cargo_manifest.vix:508, 516, 969`. |
| **C10** | **Is `find_map` order-sensitive on arrays (field order) or canonical?** | `iteration.md:274` ruled canonical. `rodin.vix:988`'s candidate search and `:648`'s `exact_version_set` both need *last-match / demand-one-branch-at-a-time*, which canonical order cannot give. B2 is blocked on this. | `rodin.vix:648, 988, 1199`; `GAPS-rodin` proposes three different order-carrying names. |
| **C11** | **Untagged decode: how does `"2021"` pick `CargoEdition::Literal` and a table pick `Workspace`?** Plus: `#[decode(rename)]` spelling, and variants with required literal fields (`Inherited { workspace: true }`). | Blocks the whole typed-decode story, which is the ports' biggest win. | `crate.vix:20`; `cargo_manifest.vix:48, 57, 78`; ratchet 062–066 is cited by `SURFACE.md:30` but no chapter exists. |
| **C12** | **Is `Option<String>` equality the right gate-effect semantics?** (Port preserved "only both absent are equal.") | A behavior preserved out of fear, not decided. | `rodin.vix:750`, `GAPS-rodin:750`. |
| **C13** | **When input order matters after `filter_map`, is `enumerate().values().filter_map(...).sorted()` the blessed idiom?** | `crate.vix:1082, 1092` produce `--extern` flags whose order Cargo sometimes preserves in diagnostics. | `GAPS-crate:1082`. |
| **C14** | **Is the `rustc! { }` / `Rustc::acquire` capability-macro grammar part of the language surface?** | `GAPS-rodin:447` calls it *"the blessed escape hatch"* on no authority. 7 sites across two ports. | `rodin.vix:452`; `crate.vix:588, 598–646, 1211, 1304`. |

### (d) Book-example imports — the ports' best idioms the book should teach

Each of these is a "win" the GAPS files named. They are the cheapest work in
this document and they are how the book stops being an aspiration.

| # | Import | From | Into |
|---|---|---|---|
| **D1** | **The scalar-or-table decode exemplar.** `CargoDependencySpec { Req(String), Detailed(CargoDependencyTable) }` — one typed path for all dependency projections. | `cargo_manifest.vix:78` (GAPS: *"make this the ratchet exemplar"*) | a new decode chapter (with C11's ruling) |
| **D2** | **Glob member expansion as one `fold` + array spread**, replacing a recursive entry/glob walker pair. | `cargo_manifest.vix:300` | `std/collections.md`, as the worked `fold` example |
| **D3** | **"Last matching element" as `fold(None, …)`.** | `cargo_manifest.vix:641` | `std/collections.md` — *or* delete it by landing `find_last` (B2) |
| **D4** | **State threading through folds** — direct dependency clause registration as folds over members × dependency names. | `cargo_manifest.vix:1014` | `building-a-solver.md`, which currently teaches the `Step`-accumulator (B3) without acknowledging it |
| **D5** | **The typed builder that dissolves a combinatorial matrix.** `RustUnit` + `externs`/`deps_tree`/`BuildMode`/`CargoUnitKind` replaces the old no-deps/one-dep/two-deps/dynamic-deps helper family — **194 fns → 135**, and the duplication does not reappear under new names. | `crate.vix:146` | a "modeling with records" section; this is the single best argument in the corpus for the language's ergonomics |
| **D6** | **Typed decode kills `Doc` walking.** `doc_string`/`doc_as_string`/`doc_as_bool`/`doc_as_strings` — 52 uses in the old `crate.vix` — are **gone**. | `crate.vix:196, 231` | the decode chapter; and `_index.md`'s "cache hit is a proof" argument gets a concrete companion |
| **D7** | **Paths as `p""` + `/`**, never embedded slashes in strings. | `crate.vix:712, 725, 807, 815, 1613` (79 `p""` lines) | the (missing) Tree/Path chapter, B10 |
| **D8** | **`namespace Version { fn <=>(self, other) }`** and call sites that then use `<`/`>` operators. | `rodin.vix:40` | the (missing) methods chapter, A9 — **pending C1** |
| **D9** | **The three deletions**, quantified: `stored_*` one-entry-map laundering **entirely gone**; boolean-match pyramids → `if`/`else`/`&&`/`\|\|`/`!`; 78+ `_tuple` trampolines → tuple destructuring in `split_last` matches and closure params. `.pop()` 54→0, `.push()` 23→0 in `cargo_manifest`. | `GAPS-rodin` Wins; `GAPS-cargo-manifest:12–13` | `iteration.md`'s "wound, quantified" section was a *prediction* (800+ lines). Measured: **−1424 lines** across the four ports (2209→1716 rodin+index, 2060→1630 crate, 2542→2041 cargo_manifest). The prediction was low by 78%. Put the real number in the book. |
| **D10** | **Rewrite `testing.md` to Form D.** 13 generator tests replace 56 `pub fn` Rust-demand probes; 0 `pub fn` probes remain. | `cargo_manifest.vix:1918–2041`; `GAPS-cargo-manifest:11` | `testing.md` — see Part 1D for the full drift list. Needs: the `Stream<Check>` shape, `assert_eq` vs `expect_eq` (C-adjacent, pick one), `must_use`, trace checks as ordinary calls, a fixture mechanism, `expect_err_contains`, a replacement for `.reject.vix` headers, and the ratchet's real length. |

---

## What I would do first

1. **C1 + C2** (`<=>` overridability, structural order). One conversation, two
   sentences of output. Everything about canonical order, `Set`, `Map` key
   order, `sorted`, and `find_min` is downstream, and `rodin.vix:40` is
   currently either legal or illegal with no way to tell.
2. **C3** (failure). The ports are raising errors with the exact construct the
   spec's error chapter was written to eliminate, and no one noticed because
   each GAPS file saw only its own workaround.
3. **A1 + A2** (newtypes, `Set<T>`). Both ruled, both blocking the largest
   single class of port ugliness, both named by all three GAPS files.
4. **D10** (rewrite `testing.md`). It is the chapter the conformance ladder is
   written against, and every one of its seven sections is stale.
5. **B1, B2, B5** (`String::lines`, `find_map`, `reversed`). Three std
   additions that delete four hand-rolled recursive walkers across three ports.
   `find_map` was already ruled in and lost.

The rest of Part 2 can proceed in parallel. The chapters that do not exist —
`Map`, `String`, `Option`/`Result`, `Tree`/`Path`, `Version`, `Doc`, methods,
generators, decode — are the real measure of the gap: `SURFACE.md:40` told the
porters *"anything else the book doesn't say"* is not banked, and they wrote
5,387 lines anyway, because a build language cannot be written without those
nine chapters. The port campaign worked exactly as intended.
