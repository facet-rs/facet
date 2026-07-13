# CRITIC: contradictions in the vix corpus

**Pinned to `2e81266db2f6073c1bb2ebce82b8a89aa3abf997`.** Every `file:line` below was
opened in this session and re-verified at that commit. The working tree moved four
times while I read (`1dcccff81`, `23fd9c38f`, `c41ad53ef`, `2e81266db`); three findings
I had drafted were fixed under me mid-session and have been demoted to §V rather than
reported as live. Line numbers rot fast here — I re-derived all of them at the pin.

**Headline:** the identity chapter contains two pairs of rules that assert P and ¬P,
and one of them (`tier-not-in-hash` vs `pending-identity`) is cited *from both sides in
a single sentence* of `store.dedup`. Below that, the memo is specified with two
different indexes in two different files. Below that, the error the mission names —
a conclusion outliving its reason — was committed **again**, inside the very commit
that claimed to fix an instance of it.

---

## A. Direct contradictions, ranked by blast radius

### A1. `Pending` and its realized value both do and do not share identity
**Reaches: memo keys, waiter wakeup, store slot allocation.**

> `identity.md:123-127` — `r[machine.identity.tier-not-in-hash]` **[SETTLED]**
> "`HandleTier` (pending/realized scheduling state) never enters hash bytes. A
> `Pending<T>` and its eventual realized value **share declared identity, computed as
> if resolved** — this is what lets a waiter recognize the value it awaited without
> re-deriving identity."

> `identity.md:136-141` — `r[machine.identity.pending-identity]` **[DESIGN]**
> "This is NOT the realized value's `ContentHash` […] so a pending value and its
> eventual realized value **do NOT share a content hash**. The 'recognize the value I
> awaited' property is served by the memo (`DemandKey → result`), **not by identity
> collision**."

Both rules claim the same mechanism (`waiter recognizes awaited value`) and assign it
to opposite causes. This is not a wording drift: `tier-not-in-hash` is the *reason* the
old driver was believed correct, and `pending-identity` is the round-1 correction.

The sweep was half-done, and `store.md` proves it — the rule cites **both**, two lines
apart, and only notices one:

> `store.md:41-46` — `r[machine.store.dedup]`
> "Tier is a dedup-key axis (pending and realized slots are distinct) even though tier
> never enters hash bytes (`machine.identity.tier-not-in-hash`); a pending slot's
> `ContentHash` component is its promise identity, a realized slot's is its value
> identity, and **they are NOT equal** (`machine.identity.pending-identity`)"

The changelog records exactly which rules were swept and `tier-not-in-hash` is not
among them:

> `spec-changelog.md:26-28` — "`machine.identity.pending-identity` rewritten;
> `machine.store.dedup` no longer claims shared-hash pending/realized slots."

**Also:** `pending-identity` closes with "`Pending<T>`/`Realized<T>` remain distinct
schema wrappers." Under `r[machine.identity.value-identity-pair]` (`identity.md:41`),
identity is `(SchemaRef, ContentHash)` — so distinct wrappers already force distinct
identity, independent of the hash. `tier-not-in-hash`'s claim is unreachable twice over.

**Fix:** strike sentence two of `tier-not-in-hash`. Keep "tier never enters hash bytes";
delete "share declared identity … recognize the value it awaited."

---

### A2. Flat-byte hashing is simultaneously the definition, rejected, and banned
**Reaches: the definition of `ContentHash`, i.e. everything.**

> `identity.md:59-68` — `r[machine.identity.zero-padding]` **[SETTLED]**
> "Consequence: **flat-byte hashing is valid unconditionally for canonical layouts**"

> `identity.md:26-37` — `r[machine.identity.canonical-memory]` **[STRUCK]**
> "The one content-hash definition is the schema-specialized framed walked encoding
> […] **Flat-memory hashing was rejected** because the structural hash of a value must
> never depend on the ABI"

> `identity.md:49-57` — `r[machine.identity.framed-encoding]` **[SETTLED]**
> "**Raw `hasher.update(user_bytes)` outside these APIs is banned.**"

Flat-byte hashing of a record *is* a raw update of user bytes. Two `[SETTLED]` rules
therefore contradict each other, and the surviving one (`zero-padding`) carries the
conclusion of the rule that was struck. `canonical-memory` even anticipates this —
"Zero-padding remains hygiene (and a canary), **not identity-load-bearing**" — but
`zero-padding` itself was never edited to agree.

The book teaches the struck definition as law:

> `content/_index.md:178` — "**Value identity** (tier 2): what came out —
> `(schema, blake3(memory))` of the result"

`blake3(memory)` is Definition B verbatim, amputated at `spec-changelog.md:363-374`.
Note `three-planes.md:59`, written later, gets it right: "`(SchemaRef, blake3)`".

**Fix:** `zero-padding`'s last sentence should read "flat-byte *comparison* is valid
for canonical layouts (a dedup fast path and a canary), and is never an identity
definition." `_index.md:178` → `(schema, ContentHash)`.

---

### A3. The memo has two different indexes
**Reaches: reuse, early cutoff, the whole location-plane story.**

> `spec/machine/_index.md:31-34` — `r[machine.cache.three-kinds]` **[SETTLED]**
> "the **memo** (demand results, **keyed by `DemandKey`**)"

> `memo.md:12` — `r[machine.memo.demand-key]` **[DESIGN]**
> "The memo is keyed by `DemandKey`: a fixed-size digest formed by a framed combine of
> the closure identity and argument identities"

> `three-planes.md:63` — "The memo store is **indexed by location** and **carries**
> recipe, read-set, and result."

> `placement.md:79-81` — `r[machine.placement.trees-cross-as-grants]` **[DESIGN]**
> "changing a file nobody read invalidates nothing — **the memo is indexed by location,
> which is content-free**, and the entry it finds carries a read-set the change misses."

And a third rule, at the persistence seam:

> `persistence.md:14-16` — `r[machine.persistence.trait-boundary]` **[DESIGN]**
> "The persistence seam is a trait defined purely in vix semantic terms (get value by
> (schema, hash) […]; **look up memo by demand key**; enumerate projection candidates)."

The book states the location claim to the reader as settled fact —

> `effects.md:170` — "**And nothing reruns** — because the memo is indexed by *location*,
> which is content-free, and the entry it finds carries a read-set that the README is
> not in."

— and so does the changelog (`spec-changelog.md:858-860`, round 10). This is not two
compatible views of one store: `three-planes.md` argues that content-keying is *the
bug*, by name —

> `three-planes.md:37-41` — "The memo key contains the input's content hash. The content
> changed. The key changed. By content alone, yesterday's run is unfindable […]
> Candidate nomination was hand-waved everywhere this dance was described, and it is
> the load-bearing missing piece."

— and `memo.md` and `persistence.md` are where it was hand-waved. Both
`machine.memo.three-tier-reuse` (`memo.md:36-42`) and `machine.persistence.lookup-order`
(`persistence.md:38-43`) name the projection tier without saying how a candidate is
found; `trait-boundary` reduces it to the bare verb "enumerate projection candidates."
That is exactly the hole `three-planes.md:205-209` says the location plane fills.

**No rule anywhere in `spec/machine/` mentions the location plane.** I checked
mechanically: `rg -i location spec/machine/` returns ten hits, of which eight are the
substring in "allocation", one is `placement.md:44` in an unrelated sense, and one is
`placement.md:80` — the claim itself. Round 5 promised the extraction
(`spec-changelog.md:334-335`, "Rules to be extracted in the rewrite pass") and it never
happened, so a `[SETTLED]` charter rule and two `[DESIGN]` rules now contradict three
prose chapters and the changelog.

**Fix:** this is the single largest missing-rule hole in the spec. Either
`cache.three-kinds` and `memo.demand-key` get amended to "indexed by location, carrying
recipe", or the three chapters get corrected. It cannot stay ambiguous — `placement.md`
already depends on the location answer for its central "nothing reruns" claim.

---

### A4. `carried-hasher` [SETTLED] asserts the map encoding that `map-order-independence` [OPEN] forbids anyone to assert

> `identity.md:93-96` — `r[machine.identity.carried-hasher]` **[SETTLED]**
> "This rule is scoped to ordered aggregates: **maps use sort-first-then-stream**
> because insertion order is not semantic order"

> `identity.md:185` — `r[machine.identity.map-order-independence]` **[OPEN]**
> "**Nothing may cite this rule as settled until it is.**"

This is the instance the mission names, and it has been *half* fixed. Commit
`1dcccff81` ("Fix carried-hasher resting on an OPEN rule") removed the citation and
kept the claim:

> `identity.md:99-103` — "CITATION FIX (round 10): this rule previously cited
> `machine.identity.map-order-independence` as its authority for map row order. That
> rule is OPEN, not settled — a settled rule may not rest on an unratified one. The
> mechanism it names (rows in key order, keys unique) **is sound independently** […]
> only the rule's *status* is unsettled."

Removing a citation does not remove a dependency. A `[SETTLED]` rule still asserts, as
law, the map encoding that the map-encoding rule says is unratified and "needs Amos"
(`identity.md:180`). And the replacement authority it now cites —
`r[machine.identity.merkle-tree]` (`identity.md:187-194`) — is **[DESIGN]**, not
settled, and is scoped to `Tree` = `Map<Path, Blob>`, not to `Map<K,V>`:

> `identity.md:190` — "A `Tree` (`Map<Path, Blob>`) is therefore identified as a
> **Merkle map**"

So `carried-hasher` [SETTLED] now rests on a [DESIGN] rule about a *different, narrower
type*. The status problem was relabelled, not resolved.

**This is finding F1 and finding C1 in one place.** See §F.

---

### A5. `observation-inside-a-place` [SETTLED] cites a rule that declares itself struck

> `placement.md:97-99` — `r[machine.placement.observation-inside-a-place]` **[SETTLED, round 10]**
> "An observation performed inside a `place` was performed by another evaluator, and
> **by `machine.receipt.fetch-observation-pin`** its pin becomes the receipt's authority."

> `receipt.md:96-100` — `r[machine.receipt.fetch-observation-pin]`
> "[RENAMED, round 10 […]] **The rule id is retained struck so stale references fail
> loudly.**"

The stale reference was written the same day, in the same round, by the same author,
and it does not fail loudly — it reads as authority. Note also that the rename never
happened: the id is unchanged, so "renamed in place" (`spec-changelog.md:883`) means
"kept, with a note."

Compounding it: the entire `[SETTLED]` rule is about a primitive that does not exist and
is explicitly OPEN —

> `primitive.md:140-141` — "The observation primitive's name and shape are OPEN; until
> it lands, checksumless retrieval has no surface."

A settled rule about the placement semantics of a primitive whose shape is undecided is
premature by its own chapter's standard.

---

### A6. `Set<T>` cannot be a real alias for `Map<T,()>` and also have `Set<T>.map(f: T -> U)`

> `SURFACE.md:165` — "| `Set<T>` | the elements | `%[a, b]` | **alias for `Map<T, ()>`** |"
> `SURFACE.md:202` — "`Set<T>.map(f)                     -> Map<T, U>           element -> image`"

> `collections.md:48` — "`Set<T>` is `Map<T, ()>` and `Tree` is `Map<Path, Blob>` —
> **real aliases, not analogies**."
> `collections.md:302` — "### `Set<T>.map(f: T -> U) -> Map<T, U>`"

If the alias is real, `Set<T>.map` *is* `Map<T,()>.map`, whose `f` receives the value
`()`, not the element `T`. It cannot receive `T`. The tell: `collections.md`'s "Map
operations" section (`:255-266`) defines **no `.map` at all** — only the alias has one.
A method that exists on the alias and not on the aliased type is the definition of not
being an alias.

The changelog knows this is unresolved and SURFACE prints it as law:

> `spec-changelog.md:784-786` — "**OPEN**: a set's stream wants `Stream<T,T>` […] so
> that `set.map(f)` means 'the map from each element to its image' — which is exactly
> the gcc build. But `Set<T> = Map<T, ()>` says `Stream<T,()>`. Lean: `Stream<T,T>`"

This is the porters' law carrying an OPEN item as ratified. It is also the one that
bites in real code — the gcc build in `collections.md:194-201` depends on it.

---

### A7. Yield ordinals are the key, and yield position does not survive

Introduced by the newest commit (`2e81266db`, "Stream<T> is sugar for Stream<Int,T>
keyed by yield ordinal"), which fixed the `Stream<Check>` arity hole (§V) and opened this:

> `collections.md:134-136` — "`Stream<T>` is sugar for `Stream<Int, T>`. A generator's
> keys are **the ordinals of its `yield` sites** — static, known before anything runs,
> and *not* the order the elements arrive in."

> `testing.md:122-123` — "A stream's order is *availability* order. **Nothing about the
> source position of a `yield` survives into the stream.**"

> `SURFACE.md:217` — "**Write no code that depends on yield position.**"

The yield-site ordinal *is* the source position. Keys survive `map`/`filter`
(`SURFACE.md:196-197`) and land in `collect()`, so `gen().collect()` yields a
`Map<Int, T>` keyed by source position, and `.values()` returns it in source order.
Yield position is not merely observable — it is now the *only* deterministic thing
about a generator.

Two further consequences nothing addresses:

1. **A yield site reached twice produces a duplicate key.** vix has no loops, so
   generators recurse; a recursive generator hits one `yield` site N times. `collect()`
   fails on duplicates — and `SURFACE.md:207-208` says "a duplicate is always
   attributable to one `rekey`", which is false here. There is no `rekey`.
2. `testing.md:116-123` ("Checks come in two kinds, and the order you yield them is not
   real") is now arguing against its own type.

The reconciliation the text wants is probably: *arrival order* is a scheduling
artifact; the *key* is static. Then `testing.md:122` should say "nothing about the
*arrival* order of a yield…" and `SURFACE.md:217` should say "write no code that
depends on yield *arrival*." Both currently say "position."

---

### A8. Struck vocabulary is back, as a first-class type

> `spec-changelog.md:507-513` (round 8) — "**Vocabulary struck.** 'Wire' is rejected for
> an undemanded value: it collides with `vix-wire` […] and, worse, **it reinvents the
> noun that `promise` was banned for. There is no object.** There are values; values are
> not computed until something demands them."

Against that:

- `demand.md:69` — "In strict languages, passing an expression to a function is a
  promise to compute it. **In vix it's a wire.**" (both banned nouns, one sentence)
- `testing.md:147` — "`fn never_demanded<T>(d: Demand<T>) -> Check`"
- `testing.md:151` — "`Demand<T>` is what an un-demanded expression already is."
- `scheduler.md:14` — `r[machine.scheduler.no-in-program-forcing]` **[SETTLED — Vix 101]**
  "construction binds **promises**"
- `identity.md:133-135` — "identified by its `DemandKey` (its **`PromiseId`**) […]
  identical pending invocations share one **promise** and one waiter set"

`Demand<T>` is the sharpest: it is not a slipped word but a generic type constructor for
exactly the object round 8 said does not exist, introduced deliberately (commit
`8d56651c6`: "trace checks take Demand<T> (the harness algorithm needed a type)").
And SURFACE denies a type is needed:

> `SURFACE.md:266-268` — "Trace checks are ordinary calls (`never_demanded(expensive())`)
> — they work **because passing an expression describes a value without demanding it**."

Either the harness needs the type (and round 8's "there is no object" is wrong), or it
doesn't (and `testing.md:147` is wrong). Both are currently law. `demand.md:69` and the
`promise` residue are ordinary sweep misses; `Demand<T>` is a design question.

---

## B. Stale documents, one verdict each

| document | verdict |
|---|---|
| `combinators.md` | **DEAD.** Whole spine. |
| `iteration.md` | **§4 DEAD** (the doctrine); §"what exists today" verified accurate. |
| `maps-all-the-way-down.md` | **HALF-DEAD.** Teaches a deleted type; cites a known-buggy struct as proof. |
| `where-a-build-runs.md` | **CONCLUSION ALIVE, EVIDENCE ROTTEN.** All §2 citations dangle. |
| `innovation-points.md` | **STALE ENTRIES + MISSING THE TWO THAT CITE IT.** |
| `test-surface-forms.md` | **SUPERSEDED, correctly.** Keep as decision record. Origin of "wires". |
| `exec-observers.md` | **ALIVE in substance**, resting on an OPEN primitive. |
| `reconcile-decisions.md` | **SUPERSEDED**, and says so via `lesson-two-decisions.md:7-9`. |
| `lesson-two-decisions.md` | **PART 1 STALE, UNMARKED.** |
| `heritage-comparison.md` | **ALIVE.** (One convergence bullet is now false.) |

### B1. `combinators.md` — dead root and branch
Every row of both tables is a deleted construct: `Multiset<T>` (`:25`, `:53`, `:67`,
`:69-80`, `:119`), `enumerate()` (`:33`, `:63`, `:86`), `Indexed<T>` (`:63`, `:87`),
`sorted_by(cmp)` (`:79`). Its central claim —

> `combinators.md:19-25` — "Operations that destroy positions return a different kind of
> value […] the result is an **unordered collection**: an immutable multiset *value* […]
> whenever it is observed, it is whole, in canonical (increasing) value order."

— is exactly what round 9 deleted, and the replacement (keep the key, nothing compacts)
is its negation. It also inverts `.values()`: `combinators.md:67` has
`values(): [T] -> Multiset<T>` (forget positions); today `.values()` is
`Map<K,V> -> [V]` (*create* positions). `SURFACE.md:297` records the flip. Nothing in
this file survives. **Delete or stamp SUPERSEDED at the top.**

### B2. `iteration.md` §4 — the fight the mission asks about is over, and §4 lost
It is no longer a live fight with `collections.md`. It is a rout, and the document is
unmarked. §4 is the *only* place the canonical-value-order doctrine is written as
Amos-attributed doctrine, which makes it the most dangerous stale page in the tree.

> `iteration.md:239-241` — "**All aggregate observation is in canonical value order**:
> increasing (lexicographic) order *of the values themselves*."

> `collections.md:170-174` — "A row's structural order compares the key first and never
> reaches the value. So **sorting object files by their contents** […] **is not
> something you must remember not to do. It is unreachable.**"

Ordering by the values themselves is precisely what is now unreachable. And directly:

> `iteration.md:260-263` — "**`ys[i] = f(xs[i])` is dead.** `ys = xs.map(f)` observes the
> f-images in *their* canonical order; index `i` means 'i-th smallest'"

> `collections.md:222-224` — "`.map(f: fn(T) -> U) -> [U]` — Field-wise: **the result's
> field `i` is `f(self[i])`.** Each output element depends on exactly one input element,
> so positions are preserved"

P and ¬P, verbatim. Same for `fold`: `iteration.md:270-272` ("canonical element order …
source-order only if you folded over `Indexed<T>`") vs `collections.md:241-243`
("Combines elements in field order. Deterministic; **field order is real for arrays**").

Its "What exists today" section, by contrast, I verified against the code and it is
**exact**: `array_filter_exclude` is at `lower.rs:4720`, `array_map_pending` at
`lower.rs:4721`, as claimed (`iteration.md:116-123`). Credit where due — that section
was written by someone who read the code.

The changelog still endorses the dead doctrine, unstruck:

> `spec-changelog.md:336-339` — "**Ordering doctrine** (in `design/iteration.md`):
> positional order dies for derived aggregates (canonical value order; concurrency
> wins) […] `Indexed<T>` opt-in."
> `spec-changelog.md:419` — "**Indexed<T> RATIFIED**: plain std alias `(Int, T)` +
> enumerate. Nothing special."

See §C4 — the changelog uses `~~strikethrough~~` elsewhere and did not use it here.

### B3. `maps-all-the-way-down.md` — teaches a type deleted in its own round
Status line says "PROPOSAL (round 9)". Round 9's addenda deleted its mechanism:

> `spec-changelog.md:766-769` — "**`Keyed<K,V>` is deleted as a type.** The key is a
> parameter of the STREAM, not a wrapper on the element — otherwise `map(compile)` hands
> the closure a row to rebuild, which is `enumerate` with extra steps."

> `maps-all-the-way-down.md:72-80` — "Write a row as `Keyed<K, V>`: `struct Keyed<K, V>
> { key: K, value: V }` […] That one declaration is doing more work than it looks like"

and it is load-bearing there (`:93-94`, `:103-104`, `:137-138`). `SURFACE.md:285-286`
lists `Keyed<K,V>` under "NOT banked — do not use."

Worse, it introduces `ExecTree` as *evidence for* the model:

> `maps-all-the-way-down.md:30-38` — "A **Tree** — the thing `exec` produces — is a map
> from path to content. **Not approximately.** This is `ExecTree`, in the code that runs
> today: `pub struct ExecTree { pub entries: BTreeMap<…>, pub blobs: BTreeMap<…> }`"

> `spec-changelog.md:787-793` — "**`ExecTree` is wrong today** […] two `BTreeMap`s over
> one key space […] A path can be in both; `bytes()` prefers `entries`, so the blob
> becomes a ghost."

I read `exec-protocol/src/lib.rs:283-300`: confirmed. `insert_bytes` routes by
`String::from_utf8(contents)`, so "is this file valid UTF-8" decides which map it lands
in, and `bytes()` (`:298-302`) reads `entries` first. The page quotes the two-map struct
and calls it "not approximately" a `Map<Path, Blob>`. It is approximately one at best.

Also `maps-all-the-way-down.md:161` — `fn compile(src: Tree, c: Path) -> Tree` — two
positional parameters, against `SURFACE.md:22`.

### B4. `where-a-build-runs.md` — right answer, dead evidence
The `Target::host()` ruling is correct and landed. The document's proof is gone:

> `where-a-build-runs.md:5-7` — "The `Target::host()` finding in §2 is a **live bug** in
> `vix/corpus-next/`."
> `where-a-build-runs.md:23` — "`Target::host()` is a plane smear, and **it is in the
> corpus eight times**"

`rg 'Target::host' vix/corpus-next/*.vix` returns **zero**. All three cited lines now
point at unrelated code:

| cited | claimed | actually, at pin |
|---|---|---|
| `crate.vix:921` | `target: Target::host(),` | `    pkg: Int,` |
| `crate.vix:588` | `let rustc = Rustc::acquire(unit.target);` | `                None => fixed,` |
| `rodin.vix:516` | `let rustc = Rustc::acquire(Target::host());` | a `cfg_expr_eval` call |

(`Rustc::acquire` is now `rodin.vix:495` `Rustc::acquire(target)` and `crate.vix:634`
`Rustc::acquire(unit.toolchain)` — the memo's §2 complaint about parameterizing
acquisition on the *target* was fixed too.) The changelog says nine, not eight:
`spec-changelog.md:830` — "(8 sites in crate.vix, 1 in rodin.vix)".

And its reasoning carries a premise the spec has since declared void — see §F4.

### B5. `innovation-points.md` — cited for entries it does not contain
Two documents cite the ledger as authority for the yield-order finding:

> `SURFACE.md:216-217` — "what comes out is *availability* order. **Top entry in the
> innovation-points ledger.**"
> `spec-changelog.md:683-685` — "**Highest-ranked entry in the innovation-points
> ledger**: a construct whose entire mental model in every other language is 'resumes in
> order.'"

I read all 66 lines of `innovation-points.md`. **There is no yield-order entry.** Nor is
there a string-interpolation entry, though the round-10 addendum writes one — labelled
"**LEDGER (innovation points)**" — into the *changelog* instead
(`spec-changelog.md:910-914`). The ledger's own discipline (`:50-57`) requires an entry
per trap; the two most important traps of the last two rounds are filed elsewhere and
back-referenced to a page that never received them.

Its live entries are stale: `:32-35` defends "**Multiset fold order** — RULED […]
multisets have `fold_ascending`, no bare `fold`" (both deleted), and `:36-38` is titled
"**Arguments are wires**" (struck vocabulary, §A8). `:56-57` still nominates the
fold-order trap as "the strongest candidate for spending" an innovation point.

### B6–B10, briefly
- **`test-surface-forms.md`** — superseded and *correct*: it recommended Form D and Form
  D won. Its spellings are all dead (`yields Check` `:57`, `assert_eq` `:59`, the `Test`
  receiver). Its `:112` open question ("`yields Check` vs `-> Stream<Check>`") is closed.
  It is the origin of "arguments are wires" (`:81-83`), which `demand.md:69` still
  carries. **Keep, stamp SUPERSEDED-BY `testing.md`.**
- **`exec-observers.md`** — substance alive and reinforced (`spec-changelog.md:756-761`
  re-proves it). But `:55-56` grounds observers in "**journaled effect inputs** (external
  streams were already ruled: journaled, each read a pinned observation)", and round 10
  ruled that a read whose identity is unknown until performed is the **observation**
  primitive, which "has no surface" (`primitive.md:141`). The design rests on a primitive
  that does not exist. `:69` still says `exec!`.
- **`reconcile-decisions.md`** — Decision 1 explicitly superseded
  (`lesson-two-decisions.md:7-9`) and resolved by round 5's Definition-A ruling. Its
  `vixen.*` question (`:125-182`) was answered `vixd.*` (`spec-changelog.md:324`).
- **`lesson-two-decisions.md`** — Part 1 is a full treatment of warm facts;
  `spec-changelog.md:387-390` records that "the bespoke warm-facts subsystem is dead […]
  Its inverted index survives only as the nomination design's worked precedent." Unmarked.
- **`heritage-comparison.md`** — **alive.** Its parens-as-blocks and `|>` are deliberate
  quotations of the zoo language (`:176-183`), not vix. (My first grep false-positived on
  this; I checked.) One bullet needs a look: `:18-20` cites V12's "collect yields the
  canonical total order" as a *convergence*, and collect now yields **key** order.

---

## C. Dangling references

**Mechanically, rule ids are clean.** I extracted all 171 `r[…]` definitions under
`spec/` and every `r[…]`/backticked citation across `content/` + `content-design/` and
diffed. Every cited id resolves. The dangling problem here is entirely *semantic* —
struck and OPEN rules cited as authority — plus a handful of non-rule citations.

- **C1.** `identity.md:93` — `carried-hasher` **[SETTLED]** asserts the map encoding;
  `identity.md:185` — "Nothing may cite this rule as settled until it is." The citation
  was removed, the assertion kept. **(§A4, §F1.)**
- **C2.** `placement.md:98` cites `machine.receipt.fetch-observation-pin`, which is
  "retained struck so stale references fail loudly" (`receipt.md:100`). **(§A5.)**
- **C3.** `SURFACE.md:216` and `spec-changelog.md:684` cite the innovation-points ledger
  for an entry it does not contain. **(§B5.)**
- **C4.** The amendment record does not amend itself. Round 7's superseded ruling is
  properly struck — `spec-changelog.md:482-483`, "~~**`<=>` is user-overridable…**~~
  **STRUCK in round 9**" — establishing the convention. Round 5's
  `spec-changelog.md:419` "**Indexed<T> RATIFIED**" and `:336-339` (the ordering
  doctrine) are deleted by round 9 and carry no mark. A reader searching for "RATIFIED"
  finds a ratification of a deleted type.
- **C5.** `store.md:47` cites `` `machine.persistence...` `` — a literal ellipsis where a
  rule id belongs. (The intended rule is `machine.persistence.ephemeral-stays-ephemeral`,
  per `spec-changelog.md:144-147`.)
- **C6.** `value.md:100` cites "`machine.identity` closure identity" — a namespace, not a
  rule. Function/closure identity has no rule anywhere, and `value.md:99-101` makes it a
  base case of `<=>`. **(Also §E.)**
- **C7.** `SURFACE.md:104` and `spec-changelog.md:557` cite `taxon/src/identity.rs:242`.
  No such path: taxon is at `phon/rust/taxon/`. The claim is *true* — the type name is
  written into the canonical encoding at `phon/rust/taxon/src/identity.rs:244`
  (`write_str(out, name)`), under `// r[impl schema-identity.canonical-encoding]` at
  `:237`. Path and line are both wrong; the substance is right. This is the one code
  citation in SURFACE, and it is the one a porter would try to open.
- **C8.** `primitive.md:38` still carries the text that `primitive.md:143-145` declares
  stale, by quotation: "Corollary: `machine.primitive.memo-policy`'s parenthetical
  '(memoizable by observation pin: fetch)' is stale." It was never edited. A rule that
  announces another rule's staleness, in the same file, and leaves it.

---

## D. Doc-vs-code drift

### D1. `Target::host()` is dead in every document and live in the compiler
The docs killed it in the *corpus* (prose, unexecutable) and left it in *`vix/src`*
(executable). This is exactly backwards.

> `SURFACE.md:250` — "**`Target::host()` is DEAD.**"
> `effects.md` (Coming-from section) — "`Target::host()`, `uname`, `process.platform` and
> `cfg!(target_os)` evaluated in the recipe are all the same bug."
> `placement.md:47-48` — `r[machine.placement.no-in-program-steering]` **[SETTLED, round 10]**

Meanwhile, `vix/src/machine/lower.rs:4567-4578`:

```rust
["Target", "host"] => {
    …
    if !call.args.args.is_empty() {
        return Err("Target::host takes no arguments".into());
    }
    Ok(Some(self.target_host()))
}
```

and a test that depends on it, `lower.rs:10413-10419`:

```rust
fn target_host_and_cross_target_are_distinct_capabilities() {
    …
    pub fn host_cc() -> Cc { Cc::acquire(Target::host()) }
```

`rg 'Target::host' vix/corpus-next/*.vix` → zero. `rg 'Target::host' vix/src` → two
sites, one of them a passing test asserting the dead semantics.

### D2. `.vix-cas` — the claim is false, and the same round contradicts it
Round 8 accepted a cost on the strength of a file on disk:

> `spec-changelog.md:592-594` — "Ride the pending **stage-6 identity freeze**
> (NEXT.md:94) rather than minting an epoch — and note **`.vix-cas` now exists on disk
> (real_process.rs:322), so post-freeze breaks are not free.**"

Both halves are wrong, and I read the code.

1. **It is not on disk.** `vix/src/real_process.rs:317-323`: `root` is
   `tempfile::Builder::new().prefix("vix-real-process-").tempdir()`, and `.vix-cas` is
   created *inside it*. There is no `into_path`, no `keep`, no `persist` anywhere in the
   file — the `TempDir` drops at the end of `run()` and takes the CAS with it.
2. **It contains no schema identity.** `real_process.rs:688-698` keys entries on
   `blake3("vix-real-process-cas" ‖ len ‖ bytes)` of a *staged file's bytes*. No
   `SchemaId` participates. An identity-epoch break could not invalidate it even if it
   were persistent.

And forty lines later the same round says the opposite:

> `spec-changelog.md:632-633` — "**SchemaId rehash cost: zero.** No deployed users of the
> current identity bytes (Amos). The byte-discriminant kind tag rides stage-6 freely."

The addendum is right. The original flag is a `NEXT.md`-style stale relay — the exact
failure round 8 apologises for at `spec-changelog.md:635-642`, committed twice in one
round. (That apology, by contrast, I verified and it is **correct**:
`vix/src/module.rs:1071` uses blake3, domain-separated `b"vix-legacy-schema-marker"`,
length-prefixed; `rg DefaultHasher vix/src` → nothing; and `NEXT.md:99-100` was in fact
corrected in place.)

### D3. `target-probe` is a registered primitive that reads the host
> `primitive.md:58-61` — `r[machine.primitive.effect-set-v1]` **[DESIGN]**
> "The initial registered set is the census class-B eleven: exec, fetch, doc-parse,
> crate-archive, ELF-doc, AST-doc, OCI-doc, **target-probe**, and the sealed triple"

Against `r[machine.placement.no-in-program-steering]` **[SETTLED, round 10]**
(`placement.md:47`): "`Target::host()`, `uname`, `cfg!(target_os)` evaluated inside a
recipe are the same bug: they read the executor into the artifact." A `target-probe`
primitive is that read, promoted to the effect set. The string `target-probe` occurs
**nowhere else** in `vix/docs`, `vix/src`, or `exec-protocol` — it has no definition, no
request type, no rule. Either it means "probe a *toolchain's* supported targets" (in
which case it is misnamed) or round 10 deleted it and `effect-set-v1` was not swept.

### D4. `0.0 == -0.0` is `false`, and no page says so
`value.md:87` gives the Float base case of `<=>`:

> "**Float**: IEEE `totalOrder`, NaN canonicalized to one bit pattern, so `<=>` is
> reflexive and equality-consistent. (Precedent: `TotalF64`, `vix/src/machine/value.rs:16`.)"

The citation is essentially right (`TotalF64` is declared at `value.rs:21`, doc comment
from `:15`). But NaN is not the only place `totalOrder` diverges from `==`. From
`vix/src/machine/value.rs:39-57`:

```rust
impl PartialEq for TotalF64 { fn eq(&self, other:&Self)->bool { self.0.to_bits()==other.0.to_bits() } }
impl Ord        for TotalF64 { fn cmp(&self, other:&Self)->Ordering { self.0.total_cmp(&other.0) } }
```

and the test at `value.rs:71-80` asserts `-0.0` sorts strictly before `0.0`. So in vix,
`0.0 == -0.0` is **false** and `-0.0 < 0.0` — an equality every reader of `values.md:120`
("`let same = a == b; // structural, any type`") will assume holds. Signed zero is the
one IEEE surprise a numerics user will actually hit. Document it or canonicalize it.

### D5. `vixen.*` vs `vixd.*`
Round 5 ratified the daemon namespace (`spec-changelog.md:324`: "**Namespace for
daemon/capability packages RATIFIED: `vixd.*`**"). `spec/_index.md:16` uses `vixd.*`.
Two spec pages did not get the memo: `primitive.md:9` ("a `vixen.*` concern") and
`capability.md:8` ("are `vixen.*` spec territory").

---

## E. Load-bearing claims with no rule

- **E1. The location-indexed memo.** `three-planes.md:63`, `placement.md:80`,
  `effects.md`, `spec-changelog.md:858-860` all assert it. No rule states it, and the
  charter rule states its negation. **(§A3.)** This is the biggest one: `placement.md`'s
  headline promise ("changing a file nobody read invalidates nothing") is unbacked.
- **E2. `rekey`.** The entire error-attribution story for `collect()` rests on it —
  `SURFACE.md:207-208`, `collections.md:168` ("a duplicate key is always attributable to a
  `rekey` you wrote"), `spec-changelog.md:776-778`. `rekey` appears in **no** stream API
  listing (`SURFACE.md:194-202`, `collections.md:269-303`) and nowhere in the corpus. The
  one operation that can produce the failure is undefined.
- **E3. `take_min`.** `collections.md:314` names it as an existing by-value form
  ("`split_last`, `take_min`"). It was defined on `Multiset<T>` (`combinators.md:78`) and
  died with it. It appears in no operations table. `split_last` is real (used at
  `building-a-solver.md:95`, ratchet rung `031-split-last.vix`) but is likewise absent
  from `collections.md`'s array-operations list.
- **E4. Where the exec tree's fold lives.** Round 9 ruled the progressive exec tree is a
  **fold** (last-write-wins per path), not a `collect()`, because the VFS event stream has
  duplicate keys (`spec-changelog.md:756-761`). There is no `fold` on `Stream` — the
  question is still open (`spec-changelog.md:732`: "does `fold` on a stream exist […] or
  must you `collect()` first?"). `Tree` is "produced by `exec`" (`SURFACE.md:166`) by a
  mechanism the surface does not contain.
- **E5. Function and closure identity.** `value.md:99-101` makes it a base case of `<=>`
  ("the canonical AST of everything it transitively references") and `values.md:133`
  promises it to users. Cited to a namespace, not a rule (§C6). Nothing defines it.
- **E6. Zero-argument application.** `SURFACE.md:18` — "Parentheses are grouping. They
  are never call syntax." The book is full of `getx()`, `expensive()`, `collect()`,
  `.values()`. Under the rule these are applications to the unit value `()`, which is
  coherent — but nothing says so, and `calling.md` (the chapter that exists to settle
  this) covers `f(x)` and `f(a,b)` and never mentions `f()`.

---

## F. Rules that survived a premise change

The mission says this error was made twice today. I found four live instances, and a
fifth committed *by the fix for the first*.

### F1. `carried-hasher` — the fix reproduced the disease
`1dcccff81` is titled "Fix carried-hasher resting on an OPEN rule." It removed the
citation (`identity.md:99-103`) and kept the conclusion (`identity.md:93`, "maps use
sort-first-then-stream", still `[SETTLED]`). The note even says the quiet part —
"only the rule's *status* is unsettled" — which is precisely the objection. A settled
rule may not rest on an unratified one *whether or not it names it*. **(§A4, §C1.)**

### F2. `pending-identity`'s reason is a struck definition
> `identity.md:136-139` — "This is NOT the realized value's `ContentHash` — **under
> flat-memory hashing** the pending bytes (closure/args/promise state) and the result
> bytes are different, so a pending value and its eventual realized value do NOT share a
> content hash."

Flat-memory hashing is struck (`identity.md:32`). The conclusion is almost certainly
still right — under the framed walk, `Pending<T>` and `Realized<T>` are distinct schemas
(`identity.md:141`) and the frame carries the schema — but the *reason on the page* is
void, and it is the only reason given. Re-derive it from `framed-encoding`.

### F3. `value-identity-pair`'s reason is void under `framed-encoding`
> `identity.md:41-47` — "Semantic value identity is the pair `(SchemaRef, ContentHash)`,
> not `ContentHash` alone. **A bytes-only hash collides values with identical bytes and
> different schemas** (`Bool(false)` and `Int(0)`, newtypes over one word, `None`
> singletons, layout-equal records with different field meaning)."

That collision is a property of flat-memory hashing. Under the ruling definition,
`ContentHash` *is* the framed walk, and the frame opens with
`start(domain, schema, arity)` (`identity.md:51-52`) — the schema is already in the
bytes. `Bool(false)` and `Int(0)` cannot collide. The conclusion (a typed pair) may well
be worth keeping for other reasons — API clarity, cheap schema checks without a store
hit — but **the stated reason no longer holds**, and as written the pair is redundant.
Worth deciding on purpose rather than inheriting.

### F4. `where-a-build-runs.md` cites a reason the spec declares VOID
> `where-a-build-runs.md:39-41` — "It is the same disease as
> `machine.identity.canonical-memory` (ABI into identity) and **`map-order-independence`
> (user code into identity)**, and it is the most expensive instance"

> `identity.md:172-178` — "ROUND-9 CORRECTION: this rule was first struck on the grounds
> that sort-first-then-hash keys identity on a user-overridable `<=>`. **That ground is
> VOID** — `<=>` is structural and not overridable"

"User code into identity" *was* the reason. `<=>` is no longer user code. The analogy in
`where-a-build-runs.md` is now the analogy to a retracted argument.

### F5. `hash-at-construction` keeps the mechanism and the retired name
> `spec-changelog.md:392-402` (round 5) — "**Entry-carried identity RATIFIED,
> 'hash-as-field' name retired** […] the content hash is a property of the STORE ENTRY
> (beside bytes/refcount — **never a slot in any value's layout**) […] Rewrite
> vocabulary: entry-carried identity + carried midstate, **not 'hash-as-field'**."

> `identity.md:107-113` — "Value identity is computed once […] and **carried as a field**
> (the term 'write-once identity slot' is session vocabulary […]). […] (**The
> hash-as-field distinction**: an interned value's `ContentHash` is write-once…)"

The rule states the mechanism the ruling forbade ("carried as a field" vs "never a slot
in any value's layout") and uses the retired name twice. `identity.md:31` gets it right
("carried on the store entry (entry-carried identity)"), so the file contradicts itself
about where the hash lives. Downstream, `value.md:113-114` inherits it: "Identities are
carried **in a slot** (`machine.identity.hash-at-construction`)", and so does
`memo.md:26-27`. `spec/machine/_index.md:15` still lists "the hash-as-field proposal" as
provenance.

---

## G. Two things the spec already flags, and deserves credit for

Not findings — the spec caught these itself, and I confirm both are still open.

1. `identity.md:208-212` — the **molten/codata asymmetry**: streams may cross an island
   edge, molten values may not, and "Molten and codata are structurally the same problem
   […] the asymmetry must be principled. Currently it is not written down." Correct, and
   still not written down.
2. The **journal collision** flagged at `spec-changelog.md:728-731` (`_index.md:40` names
   "fetch journals" as a banned counter-example; `scheduler.md:122` journals capability
   observations approvingly) **is resolved** — `receipt.md:64-73` distinguishes them
   explicitly ("distinct from the banned 'fetch journal cache' (a naming collision — that
   was a private result cache, this is a receipt authority)"). The changelog still lists
   it as unresolved under "UNDER DISCUSSION."

---

## V. Fixed under me, mid-session (do not re-report)

The tree advanced four commits while I read. These were live when I found them and are
resolved at the pin. Recording them so they are not rediscovered:

- **Calling convention, unswept through the book.** Eleven multi-positional `fn`
  signatures across six files, including `values.md:48` (`fn put_domain(state, pkg,
  domain)`) and three in `building-a-solver.md`. Fixed by `23fd9c38f`, which also added
  `calling.md` — whose commit message independently reports "violated by five of eight
  chapters." `maps-all-the-way-down.md:161`, `iteration.md` ×4, `effects.md:109` remain
  (all in stale or design docs; `effects.md:109` is the live one).
- **`Map.insert` spelled three ways.** `collections.md` `.insert(k, v)`, `values.md:49`
  `.insert(pkg, domain)`, `building-a-solver.md:65` `.insert dep.pkg where { value: … }`.
  Swept onto the last form; `collections.md:257` is now `.insert(k) where { value: V }`.
- **`Stream<Check>` had one type parameter; `Stream<K,V>` has two.** Resolved by
  `2e81266db` declaring `Stream<T>` sugar for `Stream<Int,T>` — which opened **§A7**.
- **`cc!{ }` vs backticks.** SURFACE.md §3 said backticks were string templates; round
  10's addendum (`spec-changelog.md:899-926`) reversed it — backticks are commands, `"…"`
  interpolates, `'…'` is literal. SURFACE was updated in the same window. I nearly
  reported SURFACE as stale here; it is current. **Still owed**, and the changelog says
  so: 7 corpus sites (`crate.vix` ×6, `rodin.vix:496`) used `rustc! {`/`build_script! {`
  — swept by `c41ad53ef` — and **12 ratchet files still use `exec! { }`**.

---

## What I did not read

Spec pages not read as prose: `abi.md`, `conventions.md`, `error.md`, `execution.md`,
`lifecycle.md`, `observability.md`. Of `store.md` I read `dedup` and `immutable-bytes`;
of `persistence.md`, `trait-boundary` and `lookup-order`. `capability.md` I read in full.
All 17 pages were covered by the mechanical rule-id extraction and by the `location`
sweep behind §A3, so the negative claims there hold; the positive readings above are
limited to the pages named.

Not opened: `vix/docs-internal/**` (all 12 pages), `corpus-next/ANALYSIS-*.md` (three
files, ~1600 lines), `CAMPAIGN.md`, `GAPS-crate.md`, `GAPS-cargo-manifest.md`
(`GAPS-rodin.md` read), the `rodin/` solver spec that `building-a-solver.md` points at.

Corpus: I grepped `*.vix` for constructs but read no port end-to-end. The four ports are
clean of `Multiset`/`enumerate`/`Indexed`/`sorted_by`/`Target::host`/`Keyed` (one
`Multiset` remains, in a comment at `rodin.vix:16` saying it is gone). I did not verify
that the ports *preserve the meaning* of their originals — that is the GAPS files' job
and I did not audit them against the v1 sources.

Ratchet: `vix/tests/ratchet/*.vix` — counted `exec! {` occurrences (12 files), read none.

Code: `taxon` identity/sink, `exec-protocol` `ExecTree`, `vix/src/machine/lower.rs`
(two regions), `vix/src/machine/value.rs`, `vix/src/real_process.rs`, `vix/src/module.rs`
(one function), `NEXT.md`. I did not run the test suite, and I did not check whether
`lower.rs`'s `Target::host` path is reachable from any live `.vix` program.

## Ranked shortlist, if only five get fixed

1. **§A1** — `tier-not-in-hash` sentence two. One deletion. It is cited by `store.dedup`
   from the wrong side.
2. **§A3 / §E1** — decide what indexes the memo, and write the rule. A `[SETTLED]`
   charter rule and three chapters disagree; `placement.md`'s central promise is unbacked.
3. **§A2** — `zero-padding`'s "flat-byte hashing is valid unconditionally" and
   `_index.md:178`'s `blake3(memory)`. Two edits, and the struck definition stops being law.
4. **§A4 / §F1** — `carried-hasher` is `[SETTLED]` on an unratified map encoding. Either
   ratify `map-order-independence`'s round-10 replacement or downgrade `carried-hasher`.
5. **§B1 / §B2** — stamp `combinators.md` and `iteration.md` §4. They are the only pages
   asserting canonical-value-order as doctrine, `iteration.md` §4 is attributed to Amos,
   and `spec-changelog.md:419` still says "**Indexed<T> RATIFIED**."
