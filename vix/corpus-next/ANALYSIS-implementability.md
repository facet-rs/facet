# Implementer's attack on the current vix surface

Seat: implementer's attack. Assumption: I am about to implement the checker,
lowering, and runtime from the requested book pages, machine spec pages,
`SURFACE.md`, round-5/round-6 rulings, and the four corpus-next ports.
This is semantics-only: no style feedback, no readability nits, and no
proposals that merely make the ports prettier.

Classification key:

- **UNDERSPECIFIED**: I found no rule that answers the question.
- **AMBIGUOUS**: current text supports at least two incompatible readings.
- **WISHY-WASHY**: an answer is gestured at, but not tight enough for an implementation.
- **HARD**: the answer exists, but implementing it is a major correctness or architecture item.

## P0 blockers

### 1. Which identity definition is authoritative now?

Classification: **AMBIGUOUS**

Port construct: every stored value in all four ports depends on this, especially
maps/multisets whose order and memo identity are content-derived.

Conflicting text:

- `vix/docs/content/spec/machine/identity.md:26`: `ContentHash` is defined as
  `blake3(memory)`.
- `vix/docs/content-design/spec-changelog.md:365`: round 5 says content hash is
  the schema-specialized framed walked encoding and that canonical-memory
  identity "STRIKES".

Question for Amos: Is the implementation target now exclusively the framed
walked encoding with entry-carried identity, and should the old
`machine.identity.canonical-memory` rule be treated as superseded?

Why it blocks: the runtime cannot build store identity, memo keys, map
canonicalization, or persistence until this is one rule.

### 1a. Is the island partition a day-one correctness boundary, not an optimization?

Classification: **HARD**

Port construct: the ports contain no island annotations; all examples rely on
ordinary calls, folds, recursion, and closures, e.g.
`vix/corpus-next/rodin.vix:953` and `vix/corpus-next/crate.vix:587`.

Settled text:

- `vix/docs/content/_index.md:114`: the compiler's central job is the
  partition from grains into islands and edges.
- `vix/docs/content/_index.md:150`: the partition filters which values are
  observed; it is never the key-maker.
- `vix/docs/content-design/spec-changelog.md:316`: a task is a maximal inline
  path through the island graph, not every edge.

Question for Amos: Is the first implementation required to preserve
partition-independent value identity, receipts, and memo keys across movable
island boundaries from day one?

Why it is hard: the lowerer must compute identities for possible edges even
when it fuses them away, emit diagnostics for fast-path falloff, and keep
runtime task/safepoint behavior equivalent under future split/merge.

### 2. What exactly is inferred, and where is annotation required?

Classification: **UNDERSPECIFIED**

Port constructs:

- `vix/corpus-next/rodin.vix:199`: `[].values()`
- `vix/corpus-next/index.vix:155`: `let package_ids: Map<String, Int> = %{};`
- `vix/corpus-next/cargo_manifest.vix:315`: `package_ids: %{},`
- `vix/corpus-next/cargo_manifest.vix:2023`: `let missing = try_json_decode<SparseIndexRow>(...)`
- `vix/corpus-next/cargo_manifest.vix:1907`: `let row: SparseIndexRow = json_decode(row_json);`

Question for Amos: Is vix bidirectionally typed, so expected types flow into
empty arrays, `%{}`, `None`, decode calls, record fields, and closure
parameters, or must each ambiguous site carry an explicit annotation?

Why it blocks: the checker cannot type the ports without a target-type rule for
empty literals and polymorphic calls.

### 3. Are generic functions monomorphized schema-polymorphism, dictionaries, or something else?

Classification: **UNDERSPECIFIED**

Port constructs:

- `vix/corpus-next/cargo_manifest.vix:2023`: `try_json_decode<SparseIndexRow>(...)`
- `vix/corpus-next/crate.vix:198`: `toml_decode<CargoToml>(...)`
- `vix/docs/content/testing.md:42`: `expect_eq(a: T, b: T) -> Check`
- `vix/docs/content/values.md:122`: `things.sorted()` with no bounds

Question for Amos: Are all generic functions and types compiled by
monomorphization over `SchemaRef` parameters, and are equality, ordering,
hashing, serialization, and decoding always available for every `T` without
dictionary arguments?

Why it blocks: lowering needs to know whether `Map<K, V>`, `Option<T>`,
`Stream<T>`, `expect_eq<T>`, and `json_decode<T>` produce specialized code,
carry runtime type descriptors, or call dictionary-like operations.

### 4. What is the namespace and method lookup order?

Classification: **UNDERSPECIFIED**

Port constructs:

- `vix/corpus-next/rodin.vix:40`: `namespace Version { fn <=>(self, other) -> Ordering { ... } }`
- `vix/corpus-next/crate.vix:216`: `namespace CargoEdition { fn literal(self) -> String { ... } }`
- `vix/corpus-next/crate.vix:209`: `...edition.unwrap().literal()`
- `vix/corpus-next/SURFACE.md:22`: `namespace Type { ... }` plus
  import-scoped `extend Type { }`

Question for Amos: When resolving `x.foo()` or `a < b`, what is the precedence
between inherent namespace methods, import-scoped `extend` methods, std/builtin
methods, and ambient structural operators?

Why it blocks: the checker needs a deterministic lookup/coherence rule before
methods, operators, and extension imports can be legal.

### 5. How do custom `<=>` methods interact with ambient structural ordering?

Classification: **AMBIGUOUS**

Port constructs:

- `vix/corpus-next/rodin.vix:40`: custom `Version::<=>`
- `vix/corpus-next/rodin.vix:1280`: custom `Rank::<=>`
- `vix/corpus-next/crate.vix:1082`: `}).sorted()` over `RustExtern` records
- `vix/docs/content/values.md:114`: every value supports structural `<=>`

Question for Amos: Does a namespace-defined `fn <=>(self, other)` override the
ambient structural order for that type, and if yes, what prevents two visible
extensions from defining competing orderings?

Why it blocks: sorted multisets, map canonicalization, and user-visible
ordering all need one comparator.

### 6. What is the real `Map` contract?

Classification: **UNDERSPECIFIED**

Port constructs:

- `vix/corpus-next/cargo_manifest.vix:508`: `row.features.keys()`
- `vix/corpus-next/cargo_manifest.vix:517`: `row.features.get(feature).unwrap()`
- `vix/corpus-next/rodin.vix:258`: `state.domains.insert(pkg, domain)`
- `vix/corpus-next/SURFACE.md:12`: map literals `%{ k => v }`, empty `%{}`

Question for Amos: Are `Map` keys ordered by canonical value order, do
`keys()` return `[K]`, `Multiset<K>`, or another collection, and does
`insert` replace by equality of `K` under the ambient/custom comparator?

Why it blocks: the ports allocate ids by folding over map keys; different key
order or duplicate semantics changes solver behavior.

### 7. Is `.push()` legal by-value surface or rejected mutation-shaped sugar?

Classification: **AMBIGUOUS**

Port constructs:

- `vix/corpus-next/index.vix:96`: `out.push(parse_sparse_row(text))`
- `vix/corpus-next/index.vix:203`: `state.packages.push(state.next_pkg)`
- `vix/corpus-next/rodin.vix:337`: `selected.hypotheses.push(...)`
- `vix/docs/content/std/collections.md:230`: `push` is listed under what does
  not exist as mutation-shaped API.

Question for Amos: Should `.push(x)` be accepted as a by-value append returning
a fresh array, or should every port site be rewritten to `[...xs, x]` /
`[..xs, x]`?

Why it blocks: the checker cannot both reject mutation-shaped APIs and accept
the current `index.vix`/`rodin.vix` ports.

### 8. What is the collection strictness/demand rule for folds and combinators?

Classification: **UNDERSPECIFIED**

Port constructs:

- `vix/corpus-next/rodin.vix:916`: `state.learned.fold_ascending(...)` carries
  a `Step::Conflict` accumulator.
- `vix/corpus-next/rodin.vix:988`: `try_candidates` recurses via
  `split_last()` to demand one branch at a time.
- `vix/corpus-next/cargo_manifest.vix:1025`: `take_strings` uses
  `enumerate().values().filter(...).sorted()`.

Question for Amos: Are `fold`, `fold_ascending`, `map`, `filter`,
`filter_map`, `any`, and `all` semantically strict over the whole collection,
or can the result's demanded shape avoid demanding later elements and closure
applications when the accumulator/result no longer depends on them?

Why it blocks: the solver's branch exploration and conflict propagation rely
on demand selectivity, while the collections page only specifies result values.

### 9. What is the closure capture and closure identity rule?

Classification: **UNDERSPECIFIED**

Port constructs:

- `vix/corpus-next/cargo_manifest.vix:852`: closure captures `workspace`,
  `workspace_root`, and `state`.
- `vix/corpus-next/rodin.vix:566`: fold closure captures `index` and `target`.
- `vix/corpus-next/crate.vix:1072`: filter-map closure captures target/source
  build context.

Question for Amos: Does a closure capture the value graph node, the demanded
value, or a frozen by-value copy of each free variable, and is a closure's memo
identity its definition site plus captured argument identities?

Why it blocks: anonymous closures are everywhere in the ports and they also
feed memo keys, location paths, lowering artifacts, and rematerialization.

### 10. What recursion is legal, and what is a demand boundary?

Classification: **UNDERSPECIFIED**

Port constructs:

- `vix/corpus-next/rodin.vix:953`: `propagate(index, next, target)`
- `vix/corpus-next/rodin.vix:988`: recursive candidate search
- `vix/corpus-next/cargo_manifest.vix:244`: recursive `crate_name`
- `vix/docs/content-design/spec-changelog.md:316`: a task is a maximal inline
  path through the island graph, not every edge.

Question for Amos: Which recursive calls are legal source programs, which ones
lower as same-island loops, which ones become memoized invocations, and what
static cycle/fixpoint cases must the checker reject?

Why it blocks: the checker must know what recursion forms are allowed, and the
lowerer must know when recursion is a loop versus an invocation with a
`DemandKey`.

### 11. What are generator blocks allowed to contain?

Classification: **UNDERSPECIFIED**

Port constructs:

- `vix/corpus-next/cargo_manifest.vix:1920`: `fn ... -> Stream<Check>`
- `vix/corpus-next/cargo_manifest.vix:1925`: `yield assert_eq(...)`
- `vix/corpus-next/cargo_manifest.vix:2025`: `yield match missing { ... };`
- `vix/docs/content-design/spec-changelog.md:459`: `yield` appears in the body.

Question for Amos: Is `yield` only a top-level generator-block line, or may it
appear under `if`, `match`, nested blocks, loops/folds, or closures, and does a
generator block have no final expression by construction?

Why it blocks: lowering a generator to codata cells needs a grammar and control
translation, especially for nested selection.

### 12. What is the real test surface?

Classification: **AMBIGUOUS**

Port constructs:

- `vix/corpus-next/cargo_manifest.vix:1919`: `#[test]`
- `vix/corpus-next/cargo_manifest.vix:1920`: `fn ... -> Stream<Check>`
- `vix/corpus-next/cargo_manifest.vix:1925`: `yield assert_eq(...)`

Conflicting text:

- `vix/docs/content/testing.md:27`: `test NAME { ... }`
- `vix/docs/content/testing.md:58`: `expecting { ... }`
- `vix/corpus-next/SURFACE.md:26`: `#[test] fn name() -> Stream<Check>`
- `vix/docs/content-design/spec-changelog.md:441`: the old expecting
  mini-grammar is dead.

Question for Amos: Is the only accepted test declaration now
`#[test] fn name() -> Stream<Check>` with yielded checks and ordinary trace
check calls?

Why it blocks: a checker cannot implement both old block tests and the
round-6 generator surface without choosing precedence.

### 13. What is the attribute grammar and ownership model?

Classification: **UNDERSPECIFIED**

Port constructs:

- `vix/corpus-next/cargo_manifest.vix:57`: `#[decode(rename = "rust-version")]`
- `vix/corpus-next/cargo_manifest.vix:89`: `#[decode(rename = "default-features")]`
- `vix/corpus-next/cargo_manifest.vix:1919`: `#[test]`
- `vix/corpus-next/SURFACE.md:31`: attributes exist; shape per book.

Question for Amos: What is the grammar, namespace, allowed target set, and
checker behavior for attributes such as `#[test]` and
`#[decode(rename = "...")]`?

Why it blocks: decode and tests are the first two real attribute consumers, but
the syntax is only acknowledged, not specified.

### 14. What exactly does typed decode do?

Classification: **UNDERSPECIFIED**

Port constructs:

- `vix/corpus-next/crate.vix:20`: `enum CargoEdition { Literal(String), Workspace { workspace: Bool } }`
- `vix/corpus-next/crate.vix:198`: `toml_decode<CargoToml>(manifest / p"Cargo.toml")`
- `vix/corpus-next/cargo_manifest.vix:78`: `enum CargoDependencySpec { Req(String), Detailed(CargoDependencyTable) }`
- `vix/corpus-next/cargo_manifest.vix:187`: `toml_decode((manifest / p"Cargo.toml").text())`
- `vix/corpus-next/cargo_manifest.vix:1888`: `json_decode(text)`

Question for Amos: For `json_decode`/`toml_decode`, what are the exact rules
for type argument syntax, expected-type inference, source argument types,
missing fields, null, extra fields, hyphenated keys, string-or-table enums,
variant selection, and literal-required fields such as `workspace = true`?

Why it blocks: the ports' largest claimed simplification depends on decode
being deterministic and typed.

### 15. Is `Result` part of the language, and what is the decode error type?

Classification: **UNDERSPECIFIED**

Port construct:

- `vix/corpus-next/cargo_manifest.vix:2023`: `try_json_decode<SparseIndexRow>(...)`
- `vix/corpus-next/cargo_manifest.vix:2025`: `yield match missing { Ok(_) => ..., Err(e) => ... }`
- `vix/corpus-next/cargo_manifest.vix:2027`: `e.message.contains(...)`

Question for Amos: Does vix have a standard `Result<T, E>` with `Ok`/`Err`,
and what schema and stability guarantees does the decode error value expose?

Why it blocks: checker and runtime error paths differ depending on whether
decode failures are ordinary values, `MachineError`s, or test-only helpers.

### 16. What are pattern matching and exhaustiveness rules?

Classification: **UNDERSPECIFIED**

Port constructs:

- `vix/corpus-next/cargo_manifest.vix:642`: `Some(path) if path == member`
- `vix/corpus-next/crate.vix:476`: `_ => false`
- `vix/corpus-next/rodin.vix:371`: `Step::Conflict(conflict)`
- `vix/corpus-next/rodin.vix:956`: `Step::Pass { state: next, changed: changed }`

Question for Amos: Are matches required to be exhaustive, how do guards affect
exhaustiveness, are unreachable arms errors, and what destructuring forms are
legal in let bindings, match arms, and closure parameters?

Why it blocks: the checker cannot type match expressions or guarantee that
pattern-match demand only forces the needed tag/payload.

### 17. What are numeric semantics?

Classification: **UNDERSPECIFIED**

Port constructs:

- `vix/corpus-next/rodin.vix:227`: `999999`
- `vix/corpus-next/rodin.vix:1248`: `... + seed - seed`
- `vix/corpus-next/cargo_manifest.vix:1228`: `workspace_members(workspace).len() * 2`
- `vix/docs/content/spec/machine/value.md:10`: machine boundary word is
  `Word(i64)`.

Question for Amos: Is source `Int` exactly signed 64-bit, and what are the
specified results for overflow, division/remainder, unary minus of the minimum
value, numeric literal defaulting, and parse failures?

Why it blocks: arithmetic appears in solver ids, version compatibility, target
limits, and profile rendering.

### 18. What are string semantics?

Classification: **UNDERSPECIFIED**

Port constructs:

- `vix/corpus-next/rodin.vix:436`: `line.after("=").after("\"").before("\"")`
- `vix/corpus-next/crate.vix:1333`: `line.strip_prefix("cargo::")`
- `vix/corpus-next/rodin.vix:1268`: `v.before(".").parse_int()`
- `vix/corpus-next/cargo_manifest.vix:1933`: path strings in assertions

Question for Amos: Are strings UTF-8, what is their total order, and are
`contains`, `before`, `after`, `strip_prefix`, `parse_int`, concatenation, and
interpolation defined over bytes, Unicode scalar values, or graphemes,
including missing-delimiter behavior?

Why it blocks: string order is value order, and these methods are used for
Cargo parsing, cfg parsing, path rendering, and diagnostics.

### 19. What is the real capability/effect expression surface?

Classification: **UNDERSPECIFIED**

Port constructs:

- `vix/corpus-next/rodin.vix:452`: `let rustc = Rustc::acquire(Target::host());`
- `vix/corpus-next/rodin.vix:453`: `rustc! { ... }`
- `vix/corpus-next/crate.vix:598`: `rustc! { ... }`
- `vix/corpus-next/crate.vix:1211`: `build_script! { ... }`
- `vix/corpus-next/index.vix:82`: `fetch(url: ...)`

Question for Amos: Are `Rustc::acquire`, `rustc!`, `build_script!`, `fetch`,
and eventual `exec!` part of the source language, std wrappers around
registered primitives, capability-specific macros, or corpus-only escape
hatches?

Why it blocks: lowering and receipts for real builds depend on whether these
forms are typed values or privileged syntax.

### 19a. Are witness-typed reads required for every projection in the first runtime?

Classification: **HARD**

Port constructs:

- `vix/corpus-next/cargo_manifest.vix:187`: reading `Cargo.toml` text.
- `vix/corpus-next/crate.vix:1427`: reading `build.stdout`.
- `vix/corpus-next/rodin.vix:418`: dynamic `Doc` reads.
- `vix/corpus-next/cargo_manifest.vix:719`: target-table map lookup.

Settled text:

- `vix/docs/content/spec/machine/receipt.md:11`: store-backed bytes are
  obtainable only through an accessor that records the read.
- `vix/docs/content/spec/machine/receipt.md:28`: read granularity includes
  whole, field, tag, map-key, tree-path, doc-key, and artifact projections.

Question for Amos: Is the first runtime forbidden from adding any non-recording
read path, even for dynamic `Doc`, map lookup, string text extraction, and
tree/path helpers used by the ports?

Why it is hard: every value inspection API must carry witness types and source
span/demand context before the implementation can honestly claim exact
receipts.

### 20. How do exec/build-script observers fit the surface?

Classification: **WISHY-WASHY**

Port constructs:

- `vix/corpus-next/crate.vix:1211`: build-script execution writes stdout.
- `vix/corpus-next/crate.vix:1427`: `run.text(p"build.stdout")`
- `vix/docs/content-design/exec-observers.md:69`: observer surface could be
  part of `exec!` or capability defaults.

Question for Amos: Are stdout/stderr observers declared per call, provided by
capabilities by default, or separate ordinary vix closures inside the exec
request?

Why it is hard: this decides readiness, pipelining, diagnostics, read-set
granularity, and whether `build_script!` is special.

### 21. What does `fixture_tree` mean in the real surface?

Classification: **UNDERSPECIFIED**

Port constructs:

- `vix/corpus-next/cargo_manifest.vix:1921`: `fixture_tree("cargo_manifest_real")`
- `vix/corpus-next/cargo_manifest.vix:1938`: `fixture_tree("cargo_manifest_glob_workspace")`

Question for Amos: Is `fixture_tree(name)` a test-only primitive, a std
function backed by the harness, or a shorthand for an imported tree value, and
what identity/read-set does it carry?

Why it blocks: the checker can fake the signature, but the runtime/test
harness needs fixture identity and demand semantics.

## P1 semantic divergence risks

### 22. What is the `/` operator over `Tree`, `Path`, and `String`?

Classification: **UNDERSPECIFIED**

Port constructs:

- `vix/corpus-next/cargo_manifest.vix:285`: `(workspace_root / member / "Cargo.toml").to_string()`
- `vix/corpus-next/cargo_manifest.vix:355`: `workspace / workspace_root / member`
- `vix/corpus-next/crate.vix:1572`: `p"libemit_answer_macro.so".with_ext(...)`

Question for Amos: What overloads of `/` are legal, how are absolute paths,
empty paths, `..`, duplicate separators, and string segments normalized, and
is `Path.to_string()` stable across platforms?

Why it matters: path identity is part of effects, tree projection, receipts,
and test outputs.

### 23. What is `Tree.collect()` and tree union semantics?

Classification: **UNDERSPECIFIED**

Port constructs:

- `vix/corpus-next/crate.vix:798`: `[alpha, formatting, core].collect()`
- `vix/corpus-next/crate.vix:867`: `resolved_dependency_artifacts(...).collect()`
- `vix/corpus-next/crate.vix:1067`: `Some(artifacts.collect())`

Question for Amos: Does collecting `[Tree]` into `Tree` union roots in array
order, canonical order, or by path, and what happens on path collisions?

Why it matters: dependency trees are exec inputs; union semantics change
command identity and receipts.

### 24. How are arrays indexed, especially by dynamic `Int`?

Classification: **UNDERSPECIFIED**

Port construct:

- `vix/corpus-next/cargo_manifest.vix:1219`: `state.packages[0]`
- `vix/docs/content/std/collections.md:86`: `array[i]` is field access.

Question for Amos: Is `array[i]` limited to compile-time integer field names,
or may `i` be a dynamic `Int`, and what is the typed error for out-of-bounds
or negative indexes?

Why it matters: arrays are specified as structs, but source syntax looks like
dynamic indexing.

### 25. How do array spread and mixed spread elements type-check?

Classification: **UNDERSPECIFIED**

Port constructs:

- `vix/corpus-next/rodin.vix:326`: `[..state.features.sorted(), feature].values()`
- `vix/corpus-next/cargo_manifest.vix:301`: `[..out, ..workspace_members_from_entry(...)]`
- `vix/corpus-next/rodin.vix:1148`: `[pkg, ..reversed]`

Question for Amos: Does array spread require exact `[T]`, how does it infer
`T` from empty spreads and element literals, and are spreads allowed before,
after, and between normal elements?

Why it matters: spread is the replacement for mutation-shaped append in many
ports.

### 26. What exactly are multiset equality, multiplicity, and conversion rules?

Classification: **WISHY-WASHY**

Port constructs:

- `vix/corpus-next/rodin.vix:199`: `[].values()`
- `vix/corpus-next/rodin.vix:819`: `[..deactivate_contained(...).sorted(), fact].values()`
- `vix/corpus-next/cargo_manifest.vix:1027`: `.values().filter(...).sorted()`

Question for Amos: Is `Multiset<T>` a true bag with multiplicities in `len`,
`contains`, equality, and ordering, and is `.values()` from arrays the only
implicit bridge into it?

Why it matters: learned facts, enabled features, and position-erasing filters
depend on exact duplicate behavior.

### 27. How is total value order defined structurally?

Classification: **UNDERSPECIFIED**

Port constructs:

- `vix/corpus-next/crate.vix:1082`: `.sorted()` over `RustExtern`
- `vix/corpus-next/rodin.vix:819`: `.sorted()` over `LearnedNoGood`
- `vix/corpus-next/rodin.vix:858`: `region.packages.find_min(...)`

Question for Amos: For records, enums, tuples, arrays, maps, multisets,
functions, closures, paths, trees, and capabilities, what is the field/variant
ordering used by ambient `<=>`?

Why it matters: canonical multiset order and map order must not depend on hash
bytes, and the round-5 ruling only says "content order".

### 28. What is record spread precisely?

Classification: **UNDERSPECIFIED**

Port constructs:

- `vix/corpus-next/cargo_manifest.vix:1662`: `CargoUnitProfile { incremental: "false", ..workspace_profile() }`
- `vix/corpus-next/rodin.vix:259`: `State { domains: ..., ..state }`
- `vix/docs/content/values.md:39`: `Point { x: p.x + 1, ..p }`

Question for Amos: Do explicit fields always override the spread base, must
the spread base have the exact same nominal type, and are duplicate explicit
fields or missing fields checker errors?

Why it matters: by-value update syntax is core to the solver and manifest
ports.

### 29. What is the module/import/visibility system?

Classification: **UNDERSPECIFIED**

Port constructs:

- `vix/corpus-next/index.vix:4`: `use vix::{Doc, Map, Tree, VersionSet};`
- `vix/corpus-next/index.vix:5`: `use rodin::{Index, Problem, solve_selected_versions_text};`
- `vix/corpus-next/crate.vix:2`: `use caps::Rustc;`
- exported structs/functions across ports.

Question for Amos: What are module paths, package namespaces, public/private
visibility, import cycles, and name collision rules?

Why it matters: the checker cannot resolve the four-port corpus as separate
modules without a module system.

### 30. Is named-argument syntax general?

Classification: **UNDERSPECIFIED**

Port construct:

- `vix/corpus-next/index.vix:82`: `fetch(url: `${base_url}/${sparse_index_path(name)}`)`

Question for Amos: Are named arguments part of ordinary function calls, only
primitive calls, or only this inherited `fetch` shape?

Why it matters: the parser/checker and function identity must know whether
argument names are semantic.

### 31. What is the standard library boundary versus builtin lowering?

Classification: **WISHY-WASHY**

Port constructs:

- `vix/corpus-next/rodin.vix:350`: `candidates_for` uses `fold`.
- `vix/corpus-next/cargo_manifest.vix:1027`: `values/filter/sorted/map`.
- `vix/docs/content-design/combinators.md:107`: builtin-vs-std dividing line
  is explicitly undecided.

Question for Amos: For the first implementation, which collection, map,
string, path, decode, version, target, and capability operations are compiler
builtins and which are ordinary vix std functions?

Why it matters: method lookup, lowering, memo identity, and where diagnostics
point all change depending on this line.

### 32. What is the source-level error construction story?

Classification: **UNDERSPECIFIED**

Port constructs:

- `vix/corpus-next/crate.vix:222`: `let missing: Map<String, String> = %{};`
- `vix/corpus-next/crate.vix:223`: `missing.get("workspace.package.edition").unwrap()`
- `vix/corpus-next/crate.vix:1329`: `missing.get(line).unwrap()`

Question for Amos: Should source code intentionally construct failures via
`Option.unwrap()`, or is there a typed error/result expression surface for
domain failures such as malformed manifests and build-script directives?

Why it matters: using missing-map unwraps turns domain errors into machine
errors unless a normal error-value path exists.

### 33. What is `Option.unwrap()` in source language terms?

Classification: **WISHY-WASHY**

Port constructs:

- `vix/corpus-next/rodin.vix:244`: `region.versions.get(pkg).unwrap()`
- `vix/corpus-next/crate.vix:209`: chained `.unwrap()`
- `vix/docs/content/spec/machine/error.md:34`: unwrap of `None` produces
  `MachineError` with span and demand chain.

Question for Amos: Is `unwrap` a normal std method accepted everywhere, a
checker-recognized partial primitive, or a temporary escape hatch until typed
domain errors exist?

Why it matters: the runtime error model is specified, but the language-level
legality and intended use are not.

### 34. What does `Doc` still mean?

Classification: **AMBIGUOUS**

Port constructs:

- `vix/corpus-next/index.vix:109`: `let doc = json(line);`
- `vix/corpus-next/index.vix:112`: `name: doc.name`
- `vix/corpus-next/rodin.vix:418`: `fn doc_string(doc: Doc, key: String) -> String`
- `vix/docs/content/spec/machine/primitive.md:130`: typed parsing should target
  structs directly; generic `Doc` remains dynamic/exploratory only.

Question for Amos: Is dynamic `Doc` access part of the real language surface
for production code, or should these remaining sites be treated as corpus
escape hatches to replace with typed decode?

Why it matters: dynamic field projection has different checker, runtime, and
receipt behavior from typed struct access.

### 35. What is `Target` source syntax and completeness?

Classification: **UNDERSPECIFIED**

Port constructs:

- `vix/corpus-next/crate.vix:921`: `Target::host()`
- `vix/corpus-next/crate.vix:1564`: `match host.os`
- `vix/corpus-next/crate.vix:1623`: `Target { os: Os::Macos }`
- `vix/docs/content/spec/machine/primitive.md:136`: `Target` is a first-class
  value with schema and literal syntax.

Question for Amos: What fields does `Target` have, are partial record literals
with only `os` legal, and what does `Target::host()` capture in identity and
receipts?

Why it matters: cross-compilation, capability acquisition, and exec identity
all depend on target semantics.

### 36. What are `Arg` and interpolation values?

Classification: **UNDERSPECIFIED**

Port constructs:

- `vix/corpus-next/crate.vix:566`: `Arg::Interpolation { tree: dep.tree, subpath: dep.subpath }`
- `vix/corpus-next/crate.vix:572`: `Arg::Str("dependency=")`
- `vix/corpus-next/crate.vix:598`: `{profile_args}` spliced into `rustc!`

Question for Amos: Is `Arg` a normal enum in std, a capability grammar output,
or macro-internal syntax, and how are arrays of `Arg` spliced into capability
invocations?

Why it matters: this determines whether command grammars are values or
compiler syntax.

### 37. Are static functions and enum constructors methods in the same namespace?

Classification: **UNDERSPECIFIED**

Port constructs:

- `vix/corpus-next/rodin.vix:273`: `VersionSet::from_req("*")`
- `vix/corpus-next/crate.vix:921`: `Target::host()`
- `vix/corpus-next/rodin.vix:292`: `Step::Conflict(...)`

Question for Amos: Does `Type::name` resolve constructors, static functions,
namespace functions, and associated constants through one namespace, and what
happens on collisions?

Why it matters: type/value namespace separation affects parsing and lookup.

### 38. What is the grammar and type of `p""` literals?

Classification: **UNDERSPECIFIED**

Port constructs:

- `vix/corpus-next/crate.vix:572`: `subpath: p""`
- `vix/corpus-next/cargo_manifest.vix:1933`: `p"/workspace"`
- `vix/corpus-next/SURFACE.md:21`: `p""` literals, `/` joins, String only as
  joined segment.

Question for Amos: Are `p""` literals relative or absolute by syntax, are
empty paths legal identity-bearing values, and do invalid path bytes exist?

Why it matters: path values are not just strings; they participate in tree
projection and exec mounts.

### 39. How do fixture/oracle tests compare rich values?

Classification: **UNDERSPECIFIED**

Port constructs:

- `vix/corpus-next/cargo_manifest.vix:1926`: `assert_eq(..., CargoPackage { ... })`
- `vix/corpus-next/cargo_manifest.vix:2002`: `assert_eq(problem.root_req, VersionSet::from_req("*"))`
- `vix/docs/content/testing.md:49`: failures render both sides structurally.

Question for Amos: What stable render/diff format do `Check` failures use for
records, maps, multisets, paths, trees, capabilities, and decode errors?

Why it matters: this is observable test semantics, not just UI.

## P2 lower-severity but still semantic

### 40. Are `Set<T>` and `Multiset<T>` both intended?

Classification: **AMBIGUOUS**

Port constructs:

- `vix/corpus-next/SURFACE.md:12`: "Sets per book."
- `vix/docs/content/std/collections.md:10`: vix has two collection kinds:
  array and multiset.
- `vix/corpus-next/GAPS-rodin.md:22`: requests `Set<T>`.

Question for Amos: Is `Set<T>` part of the ratified surface, or should unique
unordered collections be represented as `Multiset<T>` plus explicit uniqueness
logic for now?

Why it matters: the ports repeatedly simulate set insertion and uniqueness.

### 41. Are boolean operators lazy selectors or eager primitive operations?

Classification: **UNDERSPECIFIED**

Port constructs:

- `vix/corpus-next/rodin.vix:291`: `if !selected_allowed(...)`
- `vix/corpus-next/cargo_manifest.vix:362`: `if found.name != "" || found.vers != ""`
- `vix/corpus-next/rodin.vix:792`: `ok && match ...`

Question for Amos: Do `&&` and `||` semantically depend only on the selected
right-hand side when the left side decides the result, like `if`, or are they
strict boolean operations whose laziness is only an optimization?

Why it matters: exact read-sets and demand selectivity differ.

### 42. How are comments like `r[impl ...]` inside ports treated?

Classification: **UNDERSPECIFIED**

Port constructs:

- `vix/corpus-next/rodin.vix:714`: `// r[impl solver.learning.point]`
- `vix/corpus-next/rodin.vix:772`: `// r[impl solver.learning.widen.sound]`

Question for Amos: Are traceability annotations in comments checker-visible,
doc-tool-visible only, or ignored by the language parser?

Why it matters: if traceability is a first-class conformance tool, generated
spans and parser trivia need a rule.

### 43. What is source compatibility for obsolete book text?

Classification: **WISHY-WASHY**

Port constructs:

- `vix/corpus-next/SURFACE.md:3`: ports use exactly the book plus round-6
  rulings.
- `vix/docs/content/testing.md:27`: old `test NAME {}` shape remains in the
  book.
- `vix/docs/content/spec/machine/identity.md:26`: old canonical-memory identity
  remains in the machine spec.

Question for Amos: Should implementers treat round rulings as overriding stale
book/spec pages until the pages are edited, or should any stale page be a
blocker that must be reconciled before implementation?

Why it matters: without a precedence rule, the same document set gives
contradictory implementation instructions.
