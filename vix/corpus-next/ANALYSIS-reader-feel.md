+++
title = "Reader-feel analysis: corpus-next against the vix book"
+++

*Seat: reader feel, readability, teachability only. No semantics
adjudication — where a finding touches correctness I've flagged it as a
readability trap, not a verdict on whether the port is right.*

**Persona**: a competent Rust programmer who has read exactly seven pages
of the vix book (`_index`, `values`, `demand`, `std/collections`,
`testing`, `building-a-solver`, `three-planes`) and now opens
`vix/corpus-next/{rodin,index,cargo_manifest,crate}.vix` cold, diffing
mentally against the old corpus (`rodin/rodin.vix`, `rodin/index.vix`,
`playgrounds/snark/.../samples/{cargo_manifest,crate}.vix`) where useful.

---

## 1. Where the code reads well

**`try_candidates` / `try_candidate`, `rodin.vix:988-1001`.** This is the
book's own "Backtracking is free" section, verbatim, in the real corpus:

```
fn try_candidates(index: Index, state: State, target: String, pkg: Int, candidates: [Version]) -> SearchResult {
    match candidates.split_last() {
        None => SearchResult::Failed(state),
        Some((version, rest)) => try_candidate(index, state, target, pkg, version, rest),
    }
}

fn try_candidate(index: Index, state: State, target: String, pkg: Int, version: Version, rest: [Version]) -> SearchResult {
    let branch = hypothesize_version(state, pkg, version);
    match search(index, branch, target) {
        SearchResult::Solved(solution) => SearchResult::Solved(solution),
        SearchResult::Failed(failed) => try_candidates(index, state_with_learned(state, failed), target, pkg, rest),
    }
}
```

Reading this *after* `building-a-solver.md` is genuinely satisfying — you
already know the shape (`state` is right there, untouched, to try the next
candidate), and the code delivers exactly that promise with `split_last`
doing the "pop without mutating" work the book advertised. This is the
strongest evidence in the corpus that the book's central claim (by-value
state makes backtracking free) survives contact with real code.

**`region_contains_region`, `rodin.vix:790-797`.**

```
fn region_contains_region(outer: Region, inner: Region) -> Bool {
    outer.packages.fold_ascending(true, |ok, pkg| {
        ok && match inner.versions.get(pkg) {
            Some(inner_versions) => inner_versions.subset(region_versions_for(outer, pkg)),
            None => false,
        }
    })
}
```

Eight lines that read like the definition on the whiteboard: "outer
contains inner iff every package outer constrains, inner's constraint on
it is a subset." `fold_ascending` over a `Multiset<Int>` combined with a
plain boolean accumulator is exactly the multiset chapter's promised
"commutative-associative `f`... behaves exactly as you'd expect" — no
surprise tax paid here.

**`workspace_members`, `cargo_manifest.vix:300-302`.**

```
fn workspace_members(workspace: Tree) -> [String] {
    workspace_toml(workspace).members.fold([], |out, member| [..out, ..workspace_members_from_entry(workspace, member)])
}
```

One line, replacing what the old corpus did with a recursive
entry-walker plus a separate glob-walker (both gone). Array spread
(`[..out, ..xs]`) inside `fold` is the single cleanest demonstration in
the whole corpus of "array operations compose"; the porter's own GAPS
file proposes promoting this exact line into the collections chapter as
an example, and having read the chapter first, I'd agree — it's the
line I'd point a newcomer to.

**Honorable mention — the tests.** `cargo_manifest.vix:1918-1929`
(`workspace_members_and_package_identity_come_from_real_manifest_copies`)
loads three real fixture trees and checks package identity end to end.
It is exactly the kind of test Amos wants: integration-shaped, exercising
the real decode + inheritance path, no mocks. It's also the best-written
prose in the file — each `yield assert_eq(...)` reads like a sentence.

## 2. Where the code reads badly

**The single worst regression versus the original: explanatory comments
were deleted, and almost only in `rodin.vix`.** Counting removed `//`
lines in each diff: `rodin.vix` lost **79**, `index.vix` lost **6**,
`cargo_manifest.vix` and `crate.vix` lost **0** and **1**. This isn't
noise — it's concentrated exactly where the original had the richest
narration. The file header went from:

```
// rodin.vix — the cargo-shaped version resolver, in vix.
//
// A faithful port of the Rust reference `rodin-core` (in vixenware/vixen:
// crates/rodin-core, crates/rodin-facts). It is not line-by-line: ...
// ...
// The algorithm is CSP-style domain narrowing over per-package `VersionSet`
// intervals with region-based conflict learning and a fixpoint clause-
// application loop — NOT literal 2-watched CDCL.
```

to (`rodin.vix:1-2`):

```
// rodin.vix - the cargo-shaped version resolver, ported to the ratified
// language-as-designed surface in vix/corpus-next/SURFACE.md.
```

A first-time reader of `corpus-next/rodin.vix` gets *no* algorithmic
orientation at all — not "CSP-style domain narrowing," not "region-based
conflict learning," not the pointer to the Rust reference. Every section
banner (`Package identity`, `Clauses`, `Search state`, `Conflict
learning`, `Problem + index`) is gone too, so the file reads as one long
undifferentiated function list where the original read as a annotated
tour. The `r[impl ...]` trace tags suffered the same haircut — compare
`rodin.vix:772`:

```
// r[impl solver.learning.widen.sound]
```

against the original:

```
// r[impl solver.learning.widen.sound] — declared-structure evidence only;
// read-set widening awaits read-set exposure (rodin/docs/90, Phase-2 gap).
```

The qualifying half-sentence — the part that tells you *why* this rule is
only partially satisfied — is exactly what got cut. Same story at
`region.vix:789` (`r[impl solver.learning.region]`, stripped of "—
containment; region-level subtract not yet needed/implemented").  And the
`Rank`/`<=>` operator-overload smoke test lost the one comment explaining
its point (that `>`, `<`, `<=`, `>=` all derive automatically from a
single `<=>` impl) — the code moved into a `namespace Rank { }` block, but
the sentence that would tell a Rust reader *why* `hi > lo` even
typechecks on a struct with only a spaceship operator defined is gone
(`rodin.vix:1279-1289`, contrast with the deleted `// Operator-overload
smoke test: ...` in the original). None of this shows up in the porter's
own GAPS files, which log surface gaps, not narration loss — this is a
pure readability regression the line-count wins are quietly built on.

**`directive_field`, `crate.vix:1370-1396`.** An 8-level-deep nested
`match key == "..." { true => ..., false => match ... }` pyramid,
27 lines, dispatching on a string key:

```
fn directive_field(key: String, value: String, directives: BuildScriptDirectives) -> BuildScriptDirectives {
    match key == "rustc-cfg" {
        true => BuildScriptDirectives { rustc_cfg: [..directives.rustc_cfg, value], ..directives },
        false => match key == "rustc-check-cfg" {
            true => BuildScriptDirectives { rustc_check_cfg: [..directives.rustc_check_cfg, value], ..directives },
            false => match key == "rustc-env" {
            ...
```

This is the exact shape ("boolean-match pyramids") the port's own GAPS
notes claim were "replaced... with if/else" elsewhere in the corpus —
and mostly they were. This one wasn't, and its neighbor
`cargo_directive_payload` (`crate.vix:1332-1343`) and
`metadata_directive` (`crate.vix:1360-1368`) are cut from the same cloth.
A reader who has just finished three files' worth of clean `if/else`
chains hits this and reasonably wonders whether they missed a rule about
when `match` is required. (It isn't; it's leftover, not law.)

**`compile_rust_unit`'s four `rustc!` arms, `crate.vix:596-649`.** Four
copy-pasted macro blocks differing only in the `--emit=` line:

```
match unit.emit_metadata {
    Some(metadata) => match unit.emit_link {
        Some(link) => rustc! { ... --emit=metadata={metadata},link={link} ... },
        None => rustc! { ... --emit=metadata={metadata} ... },
    },
    None => match unit.emit_link {
        Some(link) => rustc! { ... --emit=link={link} ... },
        None => rustc! { ... /* no --emit at all */ ... },
    },
}
```

Fifty-odd lines to express "one optional flag with two independently
optional payload fragments." This is the single clearest "begs for a
name" spot in the whole corpus — the name it begs for is a typed argv
fragment for `--emit=`, which the porter's own gap note already asks for.

**The empty-map-as-panic idiom, used twice.** `crate.vix:220-224`
(`CargoEdition::literal`) and `crate.vix:1327-1330`
(`malformed_build_script_directive`) both do this:

```
let missing: Map<String, String> = %{};
missing.get("workspace.package.edition").unwrap()
```

There is no typed error/panic surface, so "this case is impossible, blow
up" is spelled as "build an empty map and demand a key from it." It reads
initially as a bug (why would you look up a key you know isn't there?)
before you realize it's a deliberate crash device. Two independent
occurrences of the identical trick suggest it's already a de facto
idiom, not a one-off improvisation — worth naming if it's going to keep
happening.

**The same ~20-field struct-of-parallel-`Map`s declared three times.**
`Index` (`rodin.vix:133-153`), `SparseBridgeState`
(`index.vix:28-53`), and `WorkspaceIndexState`
(`cargo_manifest.vix:130-164`) are, field for field, the same clause/
guard/consequent/gate table (`clause_ids`, `guard_clause_ids`,
`guard_tags`, `guard_kinds`, `guard_pkgs`, `guard_version_values`,
`guard_features`, `consequent_tags`, `consequent_pkgs`,
`consequent_version_sets`, `consequent_features`, `gate_kinds`,
`gate_targets`, ...) redeclared with minor additions each time. A reader
moving file to file recognizes the shape on sight the third time — this
is exactly the "patterns repeated so often they beg for a name" case the
task asked about, and it spans *files*, which is a stronger signal than
repetition within one function.

**The `Step`-fold short-circuit shape, repeated four times with no
name.** `force_singletons` (`rodin.vix:369-376`), `apply_clauses`
(`rodin.vix:565-572`), `apply_learned_no_goods`
(`rodin.vix:915-928`), and `enable_named_features`
(`rodin.vix:1034-1047`) all write out the same nine-to-fourteen line
skeleton: fold over a collection, accumulator is `Step` (conflict or
pass-with-changed-flag), match to propagate `Conflict` immediately and
otherwise thread the next state and OR the changed flags. It's correct
every time and it's boilerplate every time. The porter's own GAPS file
names the missing piece (`try_fold_ascending`/`try_fold`) at
`rodin.vix:915` but doesn't note that it would collapse *four* call
sites, not one — this is the biggest single lever for shrinking the file
further, bigger than any of the individual line-count wins already
logged.

## 3. Where the book failed you

**`Map<K, V>` is used everywhere and taught nowhere.** `std/collections.md`
covers exactly two collection kinds — arrays and multisets — and never
mentions `Map` once. Every struct in all four port files leans on `Map`
for its core state (`Map<Int, Domain>`, `Map<PkgId, VersionSet>` in the
book's own solver example, `Map<Int, Bool>`...). A reader who has done
only the assigned reading has *zero* grounding for `.get(k) -> Option<V>`,
`.insert(k, v) -> Map<K,V>`, or the `%{}` / `%{ k => v }` literal syntax
that appears in nearly every function in every file
(`rodin.vix:216-217`, `index.vix:155-170`,
`cargo_manifest.vix:315-341`, and dozens more). This is the single
largest gap between the assigned book chapters and the corpus's actual
vocabulary — bigger than any syntax-level surprise below, because it's
not a wrinkle, it's the load-bearing collection type the solver's own
`State` struct is built from (per `building-a-solver.md` itself: `Map<PkgId,
VersionSet>`!) and the book simply never circles back to teach it.

**Typed TOML/JSON decode is the backbone of two files and appears in no
chapter.** `toml_decode<CargoManifestToml>(...)`,
`json_decode<SparseIndexRow>(...)`, `try_json_decode<T>(...)`,
`#[decode(rename = "rust-version")]`, string-or-table enum decoding
(`CargoDependencySpec::Req(String) | Detailed(CargoDependencyTable)`) —
all of `cargo_manifest.vix` and half of `crate.vix` is built on this
machinery, and none of the assigned chapters introduce decode
annotations, the generic-call syntax, or the string-or-enum convention.
A reader hits `struct CargoPackageToml { ... #[decode(rename =
"rust-version")] rust_version: Option<PackageScalar>, ... }`
(`cargo_manifest.vix:57-58`) completely cold.

**Generic call syntax without `::`.** `toml_decode<CargoToml>(tree)`
(`crate.vix:198`) and `try_json_decode<SparseIndexRow>(text)`
(`cargo_manifest.vix:2023`) both write the type parameter directly after
the function name with no `::` turbofish. Nothing in the assigned
chapters shows a generic function call at all, so there's no way to have
predicted this — and a Rust reader's first instinct on seeing `f<T>(x)`
is to parse `<` as less-than, exactly the ambiguity Rust's turbofish
exists to dodge. It resolves fine here (call position + the following
`(` presumably disambiguate), but it's a construct the book gave no
preparation for.

**Generators (`Stream<T>`, `yield`) are the test vehicle for the entire
`cargo_manifest.vix` file, and `testing.md` doesn't mention either.**
The book's testing chapter teaches `test NAME { ... }` blocks with
`expect_eq`/`expect`/`expecting` trace clauses. The actual corpus uses
`#[test] fn name() -> Stream<Check> { yield assert_eq(...); ... }`
throughout (first at `cargo_manifest.vix:1919-1929`) — different
declaration form (`#[test] fn` vs `test NAME`), different assertion name
(`assert_eq` vs `expect_eq`), and a completely different underlying
mechanism (a generator yielding a stream of checks vs an implicit
combination of expression-lines). A reader who studied the testing
chapter and then opens the actual tests would reasonably conclude they're
looking at a different, unannounced test system, because they are.
Compounding this: `crate.vix` has **zero** `#[test]` functions and still
exposes 20 `pub fn crate_*` demand-probe functions in the old style — so
the "tests are generators now" story doesn't even hold across the two
files uniformly.

**Match guards** (`Some(path) if path == member => ...`,
`cargo_manifest.vix:643`) are pure Rust-alike and behave exactly as a
Rust reader expects, so this isn't a trap — but it's still a construct
none of the seven assigned chapters shows even once, alongside
`if`/`else` expressions themselves (used everywhere after this port,
introduced nowhere in the assigned reading — `demand.md`'s only control-flow
example is `match`).

## 4. The uncanny inventory: new traps not yet in the ledger

`innovation-points.md`'s current ledger covers `insert`/`push` return
values, `pop` absence, multiset fold order, wire-not-eager arguments,
ambient `==`/`<=>`, struct-update field-order-affects-sort, and
no-statements. None of the entries cover what I actually tripped on
reading the corpus:

**Trap: the `fold(sentinel, ...)` "first/last match" idiom looks like one
combinator and is actually two, silently.** `find_sparse_row`
(`index.vix:359-370`) and `first_sparse_dep`
(`index.vix:372-376`) sit sixteen lines apart, use visually identical
shapes (`collection.fold(sentinel_or_empty, |acc, x| if acc-looks-empty
{x} else {acc})`), and differ in an invisible way: `find_sparse_row`
pre-reverses its input (`rows.fold([], |reversed, row| [row,
..reversed])`) before the search fold, to preserve last-match-wins
semantics from the original's pop-from-the-end recursion;
`first_sparse_dep` does *not* pre-reverse, and folds in field order,
which only equals "the true first element" if the array's actual first
element never happens to look like the empty sentinel. A reader (or a
future porter) skimming both functions has no way to tell, from the shape
alone, that one specifically undoes the fold's natural order and the
other doesn't — the reversal is the entire content of the semantic
choice and it's a one-line, easy-to-miss preamble. This is exactly the
"looks like Rust, quietly doesn't behave like it" pattern the ledger is
tracking, except the trap here is vix-fold-idiom-vs-vix-fold-idiom, not
vix-vs-Rust. Whether the current code is *correct* is not this seat's
call, but the *pattern itself* — same-looking fold, opposite-order
semantics, no visual marker distinguishing them — is new territory worth
a ledger entry and probably a named combinator (`find_first`/`find_last`)
so the two cases stop looking identical at the call site.

**Trap: deleting the section-banner comments turned a previously
"annotated tour" file into a same-looking wall of functions with buried
markers.** This isn't a semantic trap but a navigation one: in the
original, `// --- Package identity ---` / `// --- Clauses ---` style
banners let you jump to the right forty lines by scanning. In
`corpus-next/rodin.vix` those are gone (see §2), so the *only* remaining
navigational aid is the sparse `r[impl solver.learning.*]` tags, which
cover four functions out of ~140. A reader orienting themselves in this
file for the first time has meaningfully less to grab onto than a reader
of the original, despite the file being 400 lines shorter — the line
count went down for two different reasons (real boilerplate removal,
which is a win, and comment removal, which isn't) and the port's own
"Wins" ledger only credits the first reason.

## 5. Per-file verdict

**`rodin.vix`** — would maintain happily. It is the most direct
enactment of the book's own solver chapter, and once you've spotted the
four repeated idioms (empty-multiset-via-`[].values()`, the `Step`-fold
short-circuit, the `fold(sentinel, ...)` first/last search, state
threading through folds) the whole file reads as combinations of five or
six moves. The one thing that would most improve *this specific file*:
a `try_fold`/`try_fold_ascending` combinator that stops on `Step::Conflict`
— it would delete the four hand-written short-circuit folds
(`force_singletons`, `apply_clauses`, `apply_learned_no_goods`,
`enable_named_features`) in one stroke and is a strictly bigger win than
anything already in the port's own "Wins" list. A close second: restoring
even a one-line "what this file is" banner at the top, since the current
two-line header only describes the *porting effort*, not the *domain*.

**`index.vix`** — smallest and most mechanical; nothing dramatic to
maintain, but it duplicates the `Index`-shaped table wholesale as
`SparseBridgeState` instead of factoring a shared "clause table" type out
of `rodin.vix` and `index.vix` together. One language change: `Set<T>`
(already asked for in the port's own GAPS) would clean up the
`register_package`/uniqueness dance, but the bigger structural win for
*this file* specifically would be a shared clause-table type so the
three near-identical 20-field structs (§2) collapse to one.

**`cargo_manifest.vix`** — the best-reading file of the four, and the
one where "reads well" material concentrates (typed decode, the
workspace-glob fold, the fixture-tree tests). The remaining rough edges
are all still-stringly Cargo domain concepts (dependency kind as
`"normal"/"build"/"dev"`, `cfg` still a raw `Doc`) that the book gives no
vocabulary for modeling as enums — not the port's fault, a genuine
language/library gap. One change that would most improve this file: a
typed `CfgExpr` enum, so `cfg_expr_eval`-style `Doc`-tag matching
(inherited wholesale from `rodin.vix`, since this file re-implements the
same `Doc`-walking `cfg(...)` evaluator a second time) becomes an
ordinary enum match like everything else in the file already is.

**`crate.vix`** — the roughest of the four and the one I'd be least
happy inheriting: it has the corpus's worst leftover match pyramid
(`directive_field`), its most repetitive single function
(`compile_rust_unit`'s four `rustc!` arms), the only *repeated*
occurrence of the empty-map-crash idiom, and it is the one file with no
native tests at all — the testing story `cargo_manifest.vix` demonstrates
doesn't extend here, so old-style `pub fn crate_*` demand probes are
still the only way anything in this file gets exercised. One language
change that would most improve it: a typed argv/emit fragment for
path-valued `rustc!`/`build_script!` arguments (per the port's own gap
note at `crate.vix:596`), which alone would delete the file's single
worst offender.
