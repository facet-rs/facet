# Implementability Critic

Mission seat: the person who has to build vix, not the person judging whether
the language is pleasant.

This report only cites files opened in this session. I read the ratified surface,
the requested book pages, the machine pages I cite, the round 7-9 changelog
section plus addenda, the ported corpus by targeted pass, and targeted
implementation anchors in `vix/src`, `weavy`, `phon/rust/taxon`, and
`exec-protocol`. I did not exhaustively read every line of the 54k-line current
implementation; the language surface is not implemented there, so implementation
citations are grounding, not claims that the v2 language already exists.

## Findings

### 1. BLOCKED: `Check` phase partition has no type or lowering contract

The book says a test is `fn ... -> Stream<Check>` and "running a test means
demanding its checks" (`vix/docs/content/testing.md:29-31`). It then says
`expect_eq(a: T, b: T) -> Check` and related constructors all return plain
`Check` (`vix/docs/content/testing.md:45-52`). Trace checks are also ordinary
calls: `never_demanded(expensive())` can hold `expensive()` without demanding it
because "passing an expression describes a value" (`vix/docs/content/testing.md:85-88`).

But the harness algorithm requires a distinction that the surface does not
expose: it "drains the stream (which constructs every `Check` and demands
nothing), demands the value checks, and only then demands the trace checks"
(`vix/docs/content/testing.md:129-131`). The surface says "the type does" the
ordering (`vix/docs/content/testing.md:131`), but the signatures do not show what
type-level fact lets the harness tell an `expect_eq` from `never_demanded`.

Counterexample:

```vix
#[test]
fn ambiguous_check() -> Stream<Check> {
    yield expect_eq(expensive(), 1);
    yield never_demanded(expensive());
}
```

Exact unanswered question: what is the type of the argument to `never_demanded`,
and what does lowering emit so constructing the `Check` captures the demand
identity of `expensive()` without demanding it? The same page also says
`demanded_times(f, n)` takes "a function value" (`vix/docs/content/testing.md:90-99`),
but the function-value identity story is only asserted elsewhere, not specified:
functions are comparable by "identity of their definition" in prose
(`vix/docs/content/values.md:131-133`), while the current old `Value` orders
functions by hash and closures/partials by canonical hash (`vix/src/value.rs:349-352`).

PROPOSAL: make `Check` a closed enum with a phase tag in its schema:
`Check::Value(ValueCheck)` and `Check::Trace(TraceCheck)`. Trace-check
constructors take a compiler-only `DemandExpr<T>`/`DemandPattern<T>` argument,
not a demanded `T`. Lowering for `never_demanded(expensive())` must emit a
demand descriptor `(closure identity, argument identities, source span)` and
must not emit an invocation edge. Lowering for `expect_eq(expensive(), 1)` emits
a `ValueCheck` whose own demand phase forces both sides.

### 2. BLOCKED: `where` records do not define default elaboration or stable structural schema identity

The surface says signature `where` declares the named-argument record type,
call-site `where { }` constructs it, and inline `where { ... }` is a structural
anonymous record while `where ExecOpts` is nominal
(`vix/corpus-next/SURFACE.md:54-69`). Defaults are allowed in the signature, and
a parameter with a default must be passed by name
(`vix/corpus-next/SURFACE.md:58-72`). Punning is also allowed
(`vix/corpus-next/SURFACE.md:75-77`), and record spread is part of the surface
(`vix/corpus-next/SURFACE.md:88`).

The compiler has to decide whether this call constructs a full record, a sparse
record, or a record plus a defaulting thunk:

```vix
fn exec(cmd: Command) where {
    mounts: [Mount] = [],
    writable: [Path] = [],
} -> ExecOutcome

exec cc!{ -c input.c -o input.o } where { mounts }
```

Exact unanswered questions:

- Is the type of `where { mounts }` the full structural record with `writable`
  present after default elaboration, or a smaller record that is coerced at the
  callee?
- Does a default expression enter the structural `SchemaId`, the function
  closure identity, both, or neither?
- Are two inline `where { mounts: [Mount] = [] }` signatures in different
  modules the same structural type if the default expressions differ?
- Is `..spread` applied before or after default insertion, and how are duplicate
  fields rejected?

This is not just surface sugar. The surface promises identical anonymous structs
in different modules are the same type (`vix/corpus-next/SURFACE.md:104-105`).
The changelog ties nominal identity to taxon's current name-hashing
(`vix/docs/content-design/spec-changelog.md:556-566`), but current taxon writes
`"struct"` and then the struct name into the canonical walk
(`phon/rust/taxon/src/identity.rs:243-244`). It does not show the anonymous
struct encoding needed for inline `where` records.

PROPOSAL: elaborate every call-site `where` to the full declared option record
before type checking the call. Missing defaulted fields are inserted as
field-level expressions owned by the callee's declaration; defaults affect the
function's closure identity, not the record schema. Structural `SchemaId` is
field names plus field schemas in declaration order, with no default
expressions. `..spread` is expanded first, then explicit fields override only if
the duplicate rule says they may; otherwise duplicates are a compile error.

### 3. BLOCKED: universal structural order is asserted for values whose order is not specified

The surface says `<=>` is structural, field-declaration order, total, and not
overridable (`vix/corpus-next/SURFACE.md:129-131`). The collections chapter says
every vix value supports `<=>`, and values order by fields in declaration order
(`vix/docs/content/std/collections.md:53-65`). It also says `by_key(f)` is total
because ties are broken by structural order of `x`
(`vix/corpus-next/SURFACE.md:145-148`,
`vix/docs/content/std/collections.md:94-97`).

The proof is valid only if `T` has a total, equality-consistent structural order.
The spec has not defined that order for all values it claims are values:

- `Map<K,V>`: rows are kept in key order (`vix/corpus-next/SURFACE.md:155-156`),
  but the still-open machine rule says `machine.identity.map-order-independence`
  is not ratified (`vix/docs/content/spec/machine/identity.md:157-178`).
- `Set<T>`: the changelog explicitly leaves set streaming semantics open
  (`vix/docs/content-design/spec-changelog.md:780-783`), and `Set<T>` is an alias
  for `Map<T,()>` in the surface (`vix/corpus-next/SURFACE.md:161`).
- `Stream<K,V>`: it has recipe identity and no value identity until resolved and
  may not be a record field (`vix/corpus-next/SURFACE.md:184-188`), so it cannot
  participate in the same universal value order unless the type system excludes
  it from "value".
- functions and closures: the book says they compare by definition identity
  (`vix/docs/content/values.md:131-133`), but capture identity and alpha/rename
  behavior are not specified at the language level. The old implementation orders
  functions by hash and closures/partials by canonical hash
  (`vix/src/value.rs:349-352`).
- floats: current machine code has a good precedent, canonicalizing NaN and using
  `total_cmp` (`vix/src/machine/value.rs:16-25`,
  `vix/src/machine/value.rs:53-61`), but the surface law does not say that.
- blobs and trees: old `Value` orders blobs lexicographically and forced trees by
  split text/blob maps (`vix/src/value.rs:311`,
  `vix/src/value.rs:353-359`), while the v2 surface wants `Tree = Map<Path, Blob>`
  (`vix/corpus-next/SURFACE.md:162`).
- recursive types: the store page says values form a DAG, no cycles by
  construction (`vix/docs/content/spec/machine/store.md:28-35`), but the language
  type story should say recursive value cycles are impossible, not leave it as a
  store implementation note.

Cost question: `<=>` on a 4GB `Tree` cannot mean "walk the whole value" on every
comparison if maps and sets are first-class sort keys. The machine spec bans
per-lookup identity recomputation (`vix/docs/content/spec/machine/memo.md:24-29`)
and says a tree is a Merkle map where changing one file rehashes one path
(`vix/docs/content/spec/machine/identity.md:180-187`). The order spec needs the
same carried/indexed answer, or `by_key(|x| x.big_tree)` is total but
unusable.

PROPOSAL: add a "StructuralOrder" section. Define primitives explicitly:
integers numeric, strings/path codepoint or byte order, blobs byte lexicographic,
floats IEEE totalOrder with NaN canonicalization, arrays lexicographic by index,
maps lexicographic over canonical `(key,value)` rows, sets as maps to unit,
enums by variant declaration order then payload, functions by stable definition
identity plus capture structural order, closures by definition identity plus
capture record. State that `Stream` is codata, not a structural field type and
not orderable until collected. State that recursive runtime values are DAGs.

### 4. BLOCKED: `Stream<K,V>` lacks the type-system and protocol decisions the compiler needs

The stream surface is precise about operations:
`map`/`filter` preserve keys, `flat_map` returns `Stream<(K,J),U>`, and
`collect()` returns `Map<K,V>` (`vix/corpus-next/SURFACE.md:190-198`). The
collections page repeats this and adds that streams are not ordered and have no
content hash until resolved (`vix/docs/content/std/collections.md:108-112`,
`vix/docs/content/std/collections.md:251-267`).

The compiler still lacks these decisions:

- Method lookup: the language says no Rust-style trait system, but `Stream<K,V>`
  needs a generic method family. Is this builtin syntax, namespace methods,
  monomorphized stdlib, or a trait-like hidden mechanism?
- `Set<T>.map(f) -> Map<T,U>` conflicts with `Set<T> = Map<T,()>`. The surface
  states the special result (`vix/corpus-next/SURFACE.md:198`), while the
  changelog says this exact point is OPEN: `Set<T> = Map<T,()>` says
  `Stream<T,()>`, but set mapping wants key and value both to be `T`
  (`vix/docs/content-design/spec-changelog.md:780-783`).
- `flat_map` says tuple key in the type but "keys compose into a path" in prose
  (`vix/corpus-next/SURFACE.md:194`,
  `vix/docs/content/std/collections.md:261-263`). Does nested flat-map produce
  `(K,(J,L))`, `((K,J),L)`, or a normalized path key? That affects equality,
  rendering, and SchemaId.
- Back-pressure and cancellation are semantics once streams cross boundaries.
  The placement spec explicitly leaves codata across `place` open because it
  becomes a distributed protocol (`vix/docs/content/spec/machine/placement.md:115-118`).
- Drain points are not enumerated. `collect()`, `.values()`, `count`, `any`,
  `all`, `contains`, tests, and `exec` observers all have different early/total
  behavior. The changelog notes monotone reads versus drains, but fold on stream
  remains open (`vix/docs/content-design/spec-changelog.md:701-736`).

Concrete corpus pressure: the ported test corpus uses `fn ... -> Stream<Check>`
and yielded checks (`vix/corpus-next/cargo_manifest.vix:2117-2123`), and ratchet
tests already use `flat_map`, `collect`, and `.values()` together
(`vix/tests/ratchet/037-filter-map-flat-map.vix:7-11`). An implementer will not
be able to fake this as arrays for long.

PROPOSAL: make `Stream<K,V>` a primitive codata type with compiler-known
methods, not trait sugar. Normalize stream keys as `PathKey<[component]>` in the
type identity, with `(K,J)` as display sugar only if that is still desired.
Define a closed drain table: total drains (`collect`, `count`), monotone early
drains (`any`, `all`, `contains` with proof), forbidden nondeterministic drains
(`head`, `take first-ready`), and protocol drains (test harness, progressive
exec tree observers). Set mapping should be a dedicated `Set<T>.map` rule or the
alias must become `Set<T>` as a real type.

### 5. BLOCKED: `place` eligibility is not a static analysis yet

The rule is strong: a value may cross `place` only if its identity is known
without evaluating it (`vix/corpus-next/SURFACE.md:230-237`,
`vix/docs/content/spec/machine/placement.md:26-37`). The effects page says this
makes placement analyzable before dispatch (`vix/docs/content/effects.md:135-137`).

The examples settle literals, pinned fetches, capabilities, and demand-root
inputs (`vix/docs/content/effects.md:126-133`). They do not settle the cases an
implementer immediately hits:

```vix
let x = f(y);
place (use x)

let v = m.get(k).unwrap();
place (use v)

let out = place (compile (fetch url where { sha256 }) source_tree)
```

Exact unanswered questions:

- Is the analysis syntactic, type-based, or graph-based?
- Can a derived demand cross as a recipe identity, or only a realized value
  identity?
- For a map lookup, can the subgraph `(m, k, get)` cross if `m` and `k` have
  known identities, even though `v` does not?
- What is the false-negative behavior? Does the compiler reject safe but
  unproven crossings, or silently widen the placed subgraph?
- What bytes actually travel: AST, lowered weavy island, `DemandKey`, value
  bundle, tree mount grant, capability grant?

The placement page leaves "Does the AST travel, or the lowered island?" open
(`vix/docs/content/spec/machine/placement.md:105-110`), and also leaves codata
across `place` open (`vix/docs/content/spec/machine/placement.md:115-118`).
Those are implementation-shaping, not polish.

PROPOSAL: define an `A Priori Identity` judgment. Literals, capabilities,
pinned fetches, root inputs, tree grants, and demand recipes whose free inputs
all satisfy the judgment may cross as recipes. Realized derived values may cross
only after being demanded before the boundary. If the judgment cannot prove a
crossing, the compiler must either reject with "draw place wider" or widen the
subgraph in a specified way; it must not invent a runtime discovery path. The
serialized unit should be chosen before implementation: AST+environment, lowered
island+schema table, or demand recipe bundle.

### 6. BLOCKED: the identity plane still contains load-bearing OPEN rules

Three identity-plane items are not ready for implementation as law:

1. `machine.identity.map-order-independence` is OPEN and says no one may cite it
   as settled (`vix/docs/content/spec/machine/identity.md:157-178`). But
   `machine.identity.carried-hasher` scopes maps by citing that rule:
   maps use sort-first-then-stream because insertion order is not semantic order
   (`vix/docs/content/spec/machine/identity.md:87-96`). The current driver has a
   concrete sort/dedup/hash path (`vix/src/machine/driver.rs:11293-11308`,
   `vix/src/machine/driver.rs:11370-11388`,
   `vix/src/machine/driver.rs:11589-11610`), but the spec status still says the
   map rule is unratified.
2. `machine.identity.merkle-tree` is [DESIGN] and says Tree is a Merkle map,
   maintained incrementally by the daemon (`vix/docs/content/spec/machine/identity.md:180-187`).
   It does not specify the node format, path normalization, empty-directory
   representation, symlink/device policy, executable bits, or whether text/blob
   decoding is outside the identity. Current `ExecTree` is definitely not that:
   it stores two `BTreeMap`s over one key space, `entries` and `blobs`
   (`exec-protocol/src/lib.rs:284-286`), and chooses the map by UTF-8 validity
   in `insert_bytes` (`exec-protocol/src/lib.rs:290-306`).
3. `machine.identity.streams-cross-island-edges` is settled for codata but
   admits the molten/codata asymmetry is not written down
   (`vix/docs/content/spec/machine/identity.md:189-205`). Both cross identity
   boundaries without stable public value identity. If streams may cross and
   molten may not, the builder needs the principle before lowering islands.

PROPOSAL: ratify the map replacement in `identity.md`: map identity is framed
`SchemaId + unique key rows in structural key order`, not "set of pairs".
Merkle tree needs its own byte-level node spec before persistence. For streams
versus molten, write the distinction as replayable codata recipe versus
private mutable construction state: a stream edge may cross only if it carries a
replayable producer identity and drain protocol; molten state never crosses
because it has no replayable public identity and must be merged into one island
or frozen first.

### 7. WRONG-THING RISK: anonymous structural kinds are not specified against taxon's nominal encoding

The surface says nominal `Point{x,y}` differs from `Vec2{x,y}` because `SchemaId`
hashes the type name, while anonymous `struct { x: 1 }` is structural and shape
alone (`vix/corpus-next/SURFACE.md:97-105`). Spread uses this boundary: dropping
a name is legal, earning a name requires explicit reconstruction
(`vix/corpus-next/SURFACE.md:115-125`).

Current taxon grounds the nominal side: `Kind::Struct` writes `"struct"`, then
the name, then type params and fields (`phon/rust/taxon/src/identity.rs:243-249`).
The sink writes strings as length-prefixed byte strings
(`phon/rust/taxon/src/sink.rs:36-38`). The changelog says kind tags are currently
length-prefixed strings drawn from the same alphabet as type names and proposes
closed byte discriminants (`vix/docs/content-design/spec-changelog.md:607-614`).

Exact unanswered question: what does the anonymous struct's kind tag encode, and
how does it avoid colliding with nominal structs while still making identical
anonymous structs in different modules share a `SchemaId`?

The current string framing prevents simple byte-concatenation ambiguity, but it
does not by itself define the v2 anonymous kind. A builder could easily encode
anonymous as `Kind::Struct { name: "" }`, which risks making "empty name" a
semantic namespace hole rather than a distinct kind.

PROPOSAL: add a distinct `Kind::AnonymousStruct`/`Kind::StructuralRecord` to
taxon identity, encoded with a closed byte discriminant, not a string tag and
not an empty name. The identity walk for anonymous structs is field names,
field schemas, and order only. The nominal `Kind::Struct` walk includes the
qualified type name.

### 8. ANNOYANCE THAT WILL BECOME BUGS: the expression grammar is mostly decidable, but `where` and block atoms need rejections written down

The core grammar is implementable if the parser follows the precedence table:
field/method > juxtaposition > unary minus > binary ops > `where { }`
(`vix/corpus-next/SURFACE.md:34-43`). Cases:

- `f -x`: parses as binary subtraction `f - x`, not application to `-x`,
  because a juxtaposed argument must be an atom and `-x` is not one
  (`vix/corpus-next/SURFACE.md:40-43`). This is decidable, but diagnostics need
  to say "write `f (-x)`".
- `f.g x`: parses as `(f.g) x` because field access is tighter than
  juxtaposition (`vix/corpus-next/SURFACE.md:34-38`).
- `f x.y`: parses as `f (x.y)`, not `(f x).y`, for the same reason.
- `%[` and `%{` are lexically unambiguous because there is no `%` operator
  (`vix/corpus-next/SURFACE.md:45-47`), and the round-9 changelog says `%` is a
  prefix collection sigil only (`vix/docs/content-design/spec-changelog.md:737-743`).
- `struct { x: 1 }` versus a block is unambiguous because anonymous structural
  literals are keyword-prefixed, while braces are blocks
  (`vix/corpus-next/SURFACE.md:82-86`,
  `vix/docs/content-design/spec-changelog.md:615-629`).

The missing written rejections:

- `a where { b } where { c }`: is this parsed left-associatively then rejected
  as "where applied twice", or is `where` non-associative in the grammar?
- Can a block be a juxtaposed atom? The surface says "bracketed group" is an
  atom (`vix/corpus-next/SURFACE.md:40-42`) and braces are blocks
  (`vix/corpus-next/SURFACE.md:82-86`). If `f { let x = 1; x }` is legal, say so.
  If not, the atom list should say parenthesized/bracket literals, not blocks.
- Suffix literals are closed and language-defined
  (`vix/corpus-next/SURFACE.md:91-92`), but the scanner needs the rejection rule
  for `5sec`, `1GiB`, `1 GB`, and identifier adjacency like `x5s`.
- Backtick templates are in the lexical surface
  (`vix/corpus-next/SURFACE.md:88-90`), while effects examples also use
  capability command templates such as `rustc\`...\``
  (`vix/docs/content/effects.md:62-64`). The lexer needs to know whether these
  are one token family parameterized by the callee or two modes.

PROPOSAL: make `where` non-associative and reject a second `where` with a fix-it
to merge records or bind an option record. Explicitly list "parenthesized
expression, array literal, map/set literal, path/string/literal, identifier,
struct literal" as atoms; decide whether block expressions are atoms.

## What I could not break

- At-most-one positional plus juxtaposition is parseable. `f x y` is
  left-associative and type-errors unless `f x` returns callable; this is
  explicitly stated (`vix/corpus-next/SURFACE.md:31-32`).
- `%{}` and `%[]` are a good lexer decision. Removing `%` as modulo eliminates
  the `f %[a]` ambiguity that round 9 called out
  (`vix/docs/content-design/spec-changelog.md:737-743`).
- `by_key(f)` really is total if structural order is total for `T` and for
  `f(x)`. The tie-break-by-`x` argument is sound; the gap is the universal
  structural-order base cases, not `by_key` itself.
- Map insertion-order independence has a plausible implementation path. The
  current driver already canonicalizes map rows, sorts/dedups, compares by
  structural key order with a key-hash tie-break, and hashes ordered rows
  (`vix/src/machine/driver.rs:11293-11308`,
  `vix/src/machine/driver.rs:11392-11407`,
  `vix/src/machine/driver.rs:11589-11610`). The spec status, not the mechanism,
  is the problem.
- Float total order has a concrete implementation precedent: `TotalF64`
  canonicalizes NaN and uses `total_cmp`, with hashing over canonical bits
  (`vix/src/machine/value.rs:16-25`,
  `vix/src/machine/value.rs:53-61`).
- Location-plane nomination looks implementable because the three-planes chapter
  correctly makes it "nomination, never validation": false negatives are
  performance misses, not stale reuse (`vix/docs/content/three-planes.md:71-88`).
  The chapter also explicitly leaves path compression and trajectory-class
  edges open as cost-model questions (`vix/docs/content/three-planes.md:248-256`);
  those are not semantic blockers unless a caller depends on them for `place`.

## Proof boundary

I did not run tests; this is a design attack report.

I did not exhaustively read all of `vix/src/machine/driver.rs` or
`vix/src/machine/lower.rs`; I read targeted implementation regions around value
ordering, map canonicalization, fetch/exec/lowering anchors, and test/corpus
surfaces. I did not treat `vix/corpus-next/ANALYSIS-implementability.md` as
evidence for any finding, because the mission required grounding in source read
this session.
