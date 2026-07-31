+++
title = "The origin rail"
weight = 35
+++

How the last residents of [issue #2597](https://github.com/facet-rs/facet/issues/2597)
— `Op::FixtureTree`, `Op::FixtureRegistry`, and `FixtureStore` — leave
`vix-core`'s scheduler. The issue's gate: their exit rides the fetch/origin
design, designed on its own merits, because `FixtureStore` is an
`OriginAdapter` — origin-shaped, not exec-shaped.

That sentence turned out to be one-fifth true, and the missing four fifths are
the design.

## What the fixture store actually is

An inventory of `FixtureStore`'s roles and how each is reached:

1. **An origin** (`fetch_url` behind `OriginAdapter::read`) — already on the
   rail. `OriginAdapter` is the spec's own named archetype for a backend
   service (`machine.primitive.effect-backend-service` cites "the
   `OriginAdapter` shape"), the fetch primitive is fully generic over it, and
   `HttpBlobOriginAdapter` is its production sibling. Nothing to design here.
2. **A tree byte source** (`tree_file_bytes`) — reached by
   `StagedEffectAuthority::read`'s `TreePath` branch, off the rail.
3. **A directory enumerator** (`tree_dir_entries`) — reached through
   `CodataDrainCtx::fixture_directory`, a *core trait method with a domain
   backend's name in it*, returning core-exported `FixtureEntryKind`. One
   implementor, one caller: a two-party protocol pretending to be a rail.
4. **A manifest source** (`registry_manifest`) — reached through
   `ReadProjection::RegistryManifest`, a core projection variant whose only
   meaning is "the offline harness manifest".
5. **The re-verification oracle.** `Runtime::reverify_read_witness` calls the
   concrete store — `registry_manifest`, `tree_file_bytes`, `fetch_url`,
   `tree_dir_entries` — *directly from the scheduler* to decide whether a memo
   entry's receipt still holds under the rerun audit. Not a primitive, not an
   `EffectCtx`, not an adapter: domain code in machine code, this arc's exact
   analogue of the exec arc's `ProgressiveSh` sniff.

Meanwhile the two ops themselves are nearly nothing: both scheduler arms are
pure constant construction (a `fixture-tree\0<name>` or `fixture-registry`
leaf), no I/O, no store access. The store is reached later, by primitives,
through the staged authority. What keeps the vixen domain resident is not the
ops — it is roles 2–5 having no seam.

So the consequence, in the exec arc's words: **nothing here needs new
machinery — the origin seam needs two more verbs, re-verification needs to
ride the seam instead of bypassing it, and routing needs to become declared
data.** Then the ops delete the way `Op::Untar` and `Op::Exec` did.

## The rail, in four moves

### 1. Origins route by declaration, and the silent fallback dies

Today: one `OriginAdapter` slot, each adapter sniffing its coordinate scheme
(`fixture://`, `http(s)://`) and rejecting the rest; both duplicate the same
hardcoded capability-schema admissibility check; and when no adapter is
installed the scheduler silently falls back to the fixture store — a program
that meant to hit the network gets an offline lookup and a confusing lie. The
machine-manifest arc's lesson applies verbatim: conjuring means nothing can
fail.

The move: `PrimitiveServices` carries a *set* of origin adapters, each entry
declaring what it serves — its coordinate schemes and its admissible
capability schema — as data, the way capability packages declare their command
grammars. Selection is a lookup over declarations; an unroutable coordinate or
an unconfigured origin is a loud typed refusal naming what was asked and what
is installed. The fixture adapter becomes an ordinary entry the *harness*
installs, with a declared root — never a compile-time `CARGO_MANIFEST_DIR`
default resident in every `Runtime`. `machine.primitive.origin-routing` pins
this.

**AMENDED (review): the routing key for tree projections.** Coordinate reads
route by scheme, but a tree projection carries no coordinate — its witness
holds an opaque source `ValueId` and a `TreePath`. So an adapter's declaration
has a third field: its **tree-handle namespace**, a byte prefix of the handle
values it owns (`fixture-tree\0` becomes the fixture adapter's declared
namespace instead of a sentinel spelled in four places). Routing a tree read
or re-verifying a tree witness resolves the source value's resident bytes from
the store and matches the declared namespaces; a handle no namespace claims is
the loud refusal; **overlapping declarations — two adapters claiming
prefix-related namespaces, or a scheme claimed twice — are rejected at
install time**, so "exactly as the original read did" is a property of the
declaration set, not a hope. Content-identified trees (carrier, canonical,
ustar residents) are recognized before namespace matching and never route to
any adapter.

### 2. The seam gains the tree verbs, structured

`OriginAdapter::read(capability, coordinate) -> Vec<u8>` is a transport, and
roles 2–3 cannot ride it: they need *entry kind*, *directory listing*, and
*distinguishable misses* (`Missing` / `NotAFile` / `NotADir` all collapse to
`Unavailable` today, and the fetch fallthrough loop cannot tell "not found
here, try the next origin" from "found and corrupt, stop"). The move: the
origin seam speaks two verbs —

- **coordinate read**: bytes by coordinate, as today, plus a structured
  failure taxonomy (miss / refusal / corruption are different answers);
- **tree projection**: entry kind, file bytes, and directory listing for a
  lazily-backed tree — `CodataDrainCtx::fixture_directory` (the fixture-named
  core method) retires.

**AMENDED (review): the listing vocabulary is `TreeEntryKind`, not
`TreeEntry`.** A `TreeEntry` carries full file content, recursive subtrees,
or symlink targets — returning it from an enumeration would force eager
materialization and defeat the lazy projection this verb exists for. The seam
speaks a neutral `TreeEntryKind { File, Dir, Symlink }` (which is exactly what
`FixtureEntryKind` already is, minus the backend's name): listings are
`(name, TreeEntryKind)` rows, file bytes come from the file-bytes verb, and
nothing is materialized that was not asked for.

`machine.primitive.origin-verbs` pins this. The `tree-read`/`tree-glob`
primitives then serve *any* lazily-backed tree through the seam — fixture
trees are one backend, and nothing in their contracts names it.

### 3. Re-verification rides the seam, and misses are witnessed

Role 5 is the deep one. A memo entry's receipt is re-verified today by the
scheduler hand-reading a concrete struct; the rail's answer is that **the seam
that produced a witness is the seam that re-verifies it** — the adapter set
resolves the witness's projection exactly as it resolved the original read,
and the scheduler compares observations without knowing any backend.
`machine.primitive.witness-reverification` pins this, together with two
receipt gaps the survey found:

- **Misses are witnessed — and today NONE are.** ~~`ReadObservation::Missing`
  exists and `TreePath` reads use it~~ **AMENDED (review): that claim was
  false.** `Missing` has re-verification support but *no production site*: a
  missing tree path becomes `PrimitiveMachineError::Unavailable` with no
  witness, exactly as a failed origin candidate does. Stage 2 therefore
  *introduces* miss witnessing for both verbs: a failed origin candidate
  records one `Missing` witness per tried coordinate (a multi-origin
  fallthrough is in the receipt, not forgotten), and a tree read that finds
  nothing records a `Missing` witness for its path — including directory
  absence. A wrong-kind outcome (`NotAFile`/`NotADir`) is not a miss: the
  entry exists with a different shape, so it is witnessed as the *kind
  observation* that contradicts the request, which is what lets the rerun
  audit distinguish "file appeared" from "file became a directory".
- **The upstream digest enters the receipt.**
  `machine.primitive.fetch-integrity-vs-identity` says both digests are
  recorded; today only the blake3 is. **AMENDED (review): the representation
  and its journal consequences, pinned.** The digest rides the witness as an
  additive field (`ReadWitness.provenance: Option<self-describing digest>`),
  not a new observation variant. Its re-verification meaning is *none*:
  provenance is a record of what the transfer was checked against, never a
  claim the audit re-checks — the vix identity is the claim. `ReadWitness` is
  persisted inside the runtime journal, which has **no format version**;
  stage 2 adds one, and old journals are **intentionally invalidated** (a
  journal is a cache: rejection costs one recompute, never correctness — the
  arc's standing precedent).

### 4. Fixture trees keep their coordinate identity — declared, not accidental

The survey's deepest finding: a fixture tree's value identity is
`Tree`-framed `fixture-tree\0<name>` — **the identity of its coordinate, not
its content**. Change the fixture root, or apply the rerun overlay, and the
content changes under an unchanged identity. For any ordinary value that would
be a hole; for fixtures it is *the point*: the store exists to simulate "the
world changed under the same name" — it is how the receipts band tests
invalidation, and the rerun-audit oracle plus witnessed reads are exactly the
machinery that catches the change. A fixture tree is the test-harness model of
an **unpinned observation**, and its identity model says so.

The rail therefore does not "fix" this — it *declares* it: the fixture adapter
is the origin-rail backend whose trees are coordinate-identified and
re-verified against live content, installed only by harnesses; and the
`fixture-tree\0` sentinel becomes that adapter's declared coordinate encoding,
spelled in exactly one place instead of today's four (`scheduler.rs`,
`lowering.rs`, `ratchet.rs`, `fixture.rs`). Production trees remain
content-identified (`machine.identity.tree-model`), and nothing
content-identified ever routes to the fixture adapter.

## What deletes from core

When fixtures ride the rail: `Op::FixtureTree` and `Op::FixtureRegistry` (ABI
ordinals 90 and 94 retire-and-reserve, the `Op::Exec` precedent), their
compiler arm (`lower_effect_intrinsic` — the whole function; both surface
spellings move to the injected-surface rail like `untar`'s), their scheduler
arms, `WireArg::FixtureTree` (retire-and-reserve — it is identity-bearing),
the `Intrinsic` enum **entirely** (these are its last two members, so
`BindingTarget::Intrinsic` and the intrinsic resolvers go too),
`CodataDrainCtx::fixture_directory` and `FixtureEntryKind`,
`ReadProjection::RegistryManifest` (retire-and-reserve; the manifest becomes
an ordinary coordinate read), the scheduler's `fixture_store` field, its four
constructor sites, the services hoist, the rerun-overlay setter, the silent
origin fallback, and every direct fixture call in `reverify_read_witness`.
`FixtureStore` itself moves to the harness side (`vixen-runtime`), taking its
virtual-file overlay with it as declared harness data. What *stays* in core:
the Tree representation layer currently sharing `fixture.rs` (`parse_ustar`,
`tree_from_resident`, `canonical_resident_tree` — consumed by untar, glob,
exec capture; the file splits), the `tree-read`/`tree-glob` contracts (exec's
progressive machinery names them), and `binding::TREE` (of its ten core uses,
only the two fixture ones leave).

## Honest couplings, stated before they bite

- **Dead vocabulary goes first.** `ReadProjection::Document` has no producer;
  `FixtureStore::url_projection` has no caller; `registered-primitives.md`
  names a `ClaimHistory` service that does not exist in code. Cleared as
  mechanical pre-work, separately reviewable.
- **`ReadProjection` is identity-bearing** (`projection_fingerprint` feeds
  demand fingerprints) and closed: variants retire-and-reserve, never
  renumber.
- **`fetch` predates `ArgRoleDecl::Capability`**: its registry capability
  rides *inside* a value argument, so its identity lands in the closure half
  of the preimage rather than the arguments half. Functionally sound (the
  backend never enters the key either way), and aligning it would re-key every
  fetch memo for zero behavioral gain — explicitly deferred, recorded here so
  the asymmetry with exec is a decision, not an oversight.
- **`ExternKind::Registry` stays core for now.** `Tree` became an
  embedder-declared host type; `Registry` following it is the symmetric end
  state, but data-driving `ExternKind` is a declared non-goal and this arc
  does not need it — after the move `Registry` keeps exactly one core naming
  site (the extern kind itself), noted as follow-up.
- **`fixture_tree` stays literal-only, and the constraint becomes declared.**
  **AMENDED (review):** today the compiler rejects a computed fixture name (a
  hard diagnostic on non-`Str` arguments); a naive move to the injected
  surface would silently begin accepting arbitrary `String` expressions —
  a language behavior change nobody asked for, and one that would make a
  harness's fixture requirement set non-static (against the spirit of
  `vixen.machine.requirements-are-static`). Dynamic fixture coordinates are
  *not* intended: the injected-surface mechanism gains a declared
  literal-argument constraint, checked at lowering with the same diagnostic
  quality as today's, and `fixture_tree` declares it. (The constraint is
  general vocabulary — any injected surface with coordinate-like arguments
  may want it.)
- **A stale verdict falls.** `data-driven-primitives.md` records that
  promoting `fixture_tree`/`fixture_registry` to primitives was "considered
  and rejected: they don't cross an authority boundary". `untar` has since
  been promoted on exactly the grounds that rejection used, and the fixture
  store demonstrably *is* an authority (the rerun oracle re-reads the
  filesystem). The verdict is superseded by this document.

**AMENDED (stage 2 implementation), three precisions:**

- The provenance field's "minimal self-describing digest repr in core" was a
  stale premise — `UpstreamDigest`/`DigestAlgorithm` have lived in core's
  identity module since the pins arc; the witness cites the existing type.
  The constraint that falls out is right: provenance can only record
  algorithms the machine can verify, and an unverifiable digest is provenance
  of nothing.
- Routing the registry manifest as "an ordinary coordinate read" needs core
  to spell the coordinate somewhere while `ReadProjection::RegistryManifest`
  still exists: one transitional `pub(crate)` constant, documented as
  retiring with the projection in stage 3.
- Tree witness paths remain `<fixture-name>/<path>` in stage 2 — re-spelling
  them handle-relative would churn recorded paths and trace vocabulary for no
  seam gain. Stage 3, which owns the sentinel-in-one-place goal, decides
  whether projection paths become handle-relative. Likewise stage 3 should
  expect downstream tests that relied on the implicit default store to need
  explicit harness installs (`harness_services()` is public for exactly
  that).

## Acceptance

1. **Identities survive.** Every existing fixture-using test (the ratchet
   rungs, `persistence_journal`, `solver_value_lane`, `fetch_origin`) passes
   with unchanged value identities — the coordinate encoding moves, its bytes
   do not. If any identity must change, it is called out à la the exec re-key,
   not discovered.
2. **The oracle works through the seam.** The rerun-audit and
   persistence-journal suites pass with `reverify_read_witness` containing
   zero direct fixture calls.
3. **Misses are witnessed.** A multi-origin fetch that falls through records
   one `Missing`-observed witness per tried origin, pinned by test; the
   upstream digest appears in the receipt beside the blake3.
4. **No conjuring.** With no origin adapters installed, an origin read is a
   loud typed refusal naming the coordinate and the installed set — pinned
   against the silent-fixture-fallback failure mode.
5. **Core is clean.** No `fixture` spelling in `vix-core/src` outside the
   split-off Tree layer's history; `Intrinsic` gone; grep-zero for
   `fixture-tree\0` outside the harness adapter.

## Staging

1. **Mechanical pre-work** — split the Tree layer out of `fixture.rs`; delete
   dead vocabulary (`Document`, `url_projection`); replace the silent origin
   fallback with the loud refusal. **AMENDED (review), the transition stated:**
   removing the fallback alone breaks every fixture fetch, because the normal
   ratchet path installs no origin adapter — so the same commit makes the
   *harness* install the existing `FixtureStore` as its origin adapter
   explicitly. The conjuring moves from the machine to the harness in stage 1;
   stage 3 then moves the implementation. No design commitment; green on its
   own.
2. **The seam** — the two-verb origin surface with declared routing and the
   structured failure taxonomy; re-verification through it; witnessed misses
   and the digest receipt. Fixtures still on their ops, served by the seam.
3. **The move** — the ops, the intrinsics, and the store leave core; the
   fixture adapter lands in `vixen-runtime` with the overlay as harness data;
   surface spellings ride the injected rail.
4. **Follow-ups** — `Registry` as host type; `fetch`'s capability-role
   alignment; whatever stage 2 surfaces.
