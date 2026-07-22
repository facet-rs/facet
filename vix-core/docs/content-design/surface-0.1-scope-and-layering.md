+++
title = "vixen 0.1 surface: scope, the three-layer model, and retirements"
+++

Status: decision memo from conversation (2026-07-22). Supersedes nothing;
records rulings that previously lived only in people's heads. **TL;DR: 0.1 is
"an existing Cargo.lock → a built binary on x86 Linux". Everything lock-*time*
(solving, observing a live index) is out of scope. Surface functions belong to
exactly one of three layers — EFFECT / AXIOM / VIX — and the pressure is always
to move work *down* into pure vix, never *up* into new primitives.**

## The 0.1 north star

No-config compile of a real Rust workspace (facet itself, Cargo, any Rust lib) →
built binary, on **x86 Linux**, **starting from an already-existing Cargo.lock**.

Do not make decisions that would harm eventual expansion to **Darwin / arm** (and
maybe Windows) as build *hosts*, or to other language ecosystems as build
*targets*.

The load-bearing consequence: **you are handed a solved lock, so 0.1 never
resolves dependencies.** The entire lock-time half of the system — dependency
solving, reading a live registry index — is out of scope. See the in/out lists
below.

## The three-layer model

Every surface name lives in exactly one layer. The boundaries are principled, not
matters of taste.

| layer | home | admits only | litmus |
|---|---|---|---|
| **EFFECT** | Rust primitive (`RawPrimitive`) | operations that cross an authority boundary | does it need a receipt / capability / witness? |
| **AXIOM** | compiler intrinsic → Weavy op | pure work that is *inexpressible* in vix, or kept intrinsic for scheduler fusion | can vix express it given the other axioms? if no → axiom |
| **VIX** | pure-vix stdlib under the `std` module | everything derivable from the two layers above | everything else |

Rules that fall out of this:

- **"Everything a primitive" is backwards.** A primitive is an *effect*, not
  "code that happens to live in Rust". Making `contains`/`any` a primitive turns a
  visible demand sub-graph into an opaque host-call, defeats short-circuiting and
  parallelism, and violates `machine.execution.no-pure-hostcalls`. Keep the
  primitive set tiny and effect-only. See [registered primitives](/vix/registered-primitives).
- **The intrinsic set should shrink over time.** Many current method intrinsics
  (`any`, `all`, collection `contains`, `find_min`, `filter_map`) are pure and
  foldable — they are intrinsics only because std couldn't express them *before
  generics/monomorphization landed*. They are owed a migration to the VIX layer.
  See [combinators](/design/combinators) for the provisional column.
- **`decode` is the one deliberate exception.** It is a pure transform of
  (bytes, format) — by the classification rule it should be VIX, not an EFFECT —
  but it is a primitive because the parsers (facet-json/toml) live in Rust and vix
  cannot tokenize bytes yet. Label it a pragmatic effect, not a real authority
  crossing.

**The VIX namespace now exists as `std`** (PR #2517, 2026-07-19): the stdlib
items are assembled into a `mod std { … }` (`STD_MODULE_SOURCE`) and reachable as
`std::fetch`, `std::Format::Json`, `std::json_decode`, etc., with the unqualified
spellings retained as compatibility aliases. That closes the "the VIX layer has no
namespace" gap this memo originally flagged — just spelled `std::`, not `vix::`.
The dispatch machinery is still split (free functions route through
`vix::binding::BindingRegistry`, whose `BindingTarget` is exactly Primitive |
Intrinsic | VixFunction; *methods* route through the hardcoded
`PreludeMethodRegistry` in `compiler.rs`, whose only target is an intrinsic op — a
vix function can still be method-called via the UFCS fallback in
`lower_method_call`). Unifying the two rails is future work.

## In / parked / deleted for 0.1

**In the critical path** (build and polish these):

- `fetch` — pinned; crate-archive hashes come from the lock. *Gap:* surface
  `fetch` is currently `fetch(url)` with no hash argument; 0.1 needs the lock's
  checksum threaded through (the backend `verify_checksum` already exists). Do it
  as the self-describing-hash change (below).
- `exec` — run rustc. The linchpin. Note it is **not** a registered primitive; it
  lives on its own dedicated-op rail (tests: ratchet 067–074).
- `decode` / `toml_decode` — parse Cargo.lock + Cargo.toml. `Format` enum stays.
- tree surface — `.text()` (a tree-read primitive), `glob`, tree projection.
- `untar` / `extract` — unpack `.crate` archives.
- the pure glue — string/map/collection intrinsics to resolve features and build
  rustc command lines.

**Parked** (lock-time / resolve machinery — correct, just not now):

- rodin solver + `Version` / `VersionSet` (`vix-core/std/version.vix`) — a lock
  has exact versions; no interval/requirement solving is reachable.

**Deleted:**

- `is_blank` — trivial (`text == ""`), zero corpus call sites. It was pure
  filler serving only as a test fixture (the sole non-generic pure std
  function); the injection-mechanism tests that used it are re-rooted onto a
  throwaway `demo_blank` prelude so nothing real ships it.
- the **observe bundle** — `observe`, `refresh`, `Mode`, `Registry.coordinate`,
  and all the observe-only claim-history machinery (see the entanglement map
  below). This is how you'd read a live index to *produce* a lock; you are given
  one. Removed outright rather than parked — by the time we need it back the API
  will have shifted, and the shared value-persistence layer stayed intact for
  `fetch`.

## Retirement entanglement map

Retirements are not independent; do them in the right bundles or you break the
tree:

- **`is_blank`** — free-standing *shipping*, but upstream (#2517) had made it the
  probe for the new `std::` mechanism tests. Removed by re-rooting all of those
  (in `stdlib.rs` and `vix-core/tests/modules.rs`) onto a throwaway `demo_blank`
  fixture, then deleting the `.vix` and dropping it from `STD_MODULE_SOURCE` +
  `PRELUDE_SOURCES`.
- **`observe` bundle = `observe` + `refresh` + `coordinate` + `Mode`** — removed
  as one deliberate surgery (~1,300 lines). The clean separation held: the
  observe-only claim-history seam (`ClaimHistory`/`ObserveCoordinate`/
  `ObservedClaim`, `observe_origin`, `claim_head`/`append_claim`) came out while
  the shared value-persistence + origin-adapter layer that `fetch` relies on
  stayed intact. Guardrails that were *kept* (they only look observe-shaped):
  `runtime/observe.rs` (the observability event log, unrelated),
  `EffectCtx::observe(JournalObservation)` (journal plumbing), the memo-journal /
  `PersistentMemoClaim` machinery (used by all memoized effects), and
  `PrimitiveMemoPolicy::Observed` (also used by tree-read). Left as documented
  dead code for a follow-up: the now-unused `Selector`/`ArgRole::Selector` path
  and `PrimitiveMachineError::RefreshConflict` (removing the latter would shift
  `#[repr(u8)]` wire tags).
- **rodin / version** — not wired into any Rust build (only the unwired
  `corpus-next/` `use`s them), so safe to leave dormant until decided.

## API-shape smells (fix when reshaping the surface)

Distinct from *where* a function lives — whether its *signature* is right.

- **`String.split_once(d) -> (String, String)` is fallible** (fails the demand on
  a missing delimiter), forcing callers to pre-guard with `contains`. `version.vix`
  has 16 such guarded sites. **Fix: return `Option<(String, String)>`.**
- **Missing plural `split(d) -> [String]`** — `version.vix` hand-rolls
  `head`/`tail`/`nth_field` recursion for want of it.
- **`parse_int() -> Int` is fallible with no fallible twin** — callers guard with
  `is_numeric()`. **Fix: add `try_parse_int -> Result`/`Option`** (mirrors
  `decode`/`try_decode`).
- **`fetch` hashes put the algorithm in the key.** `sha256: "…"` should be
  `check: "sha256:…"` — self-describing value, field named by *role*. Keep two
  fields, not one `hash:`: `blake3`/`id` is the value's identity (required),
  `sha256`/`check` is transfer provenance (optional). Contrast: `Format`/`Mode`
  are *closed, runtime-implemented* sets → enums are correct; do not stringly-type
  those. The discriminator: closed-and-implemented → enum; open-and-external →
  self-describing tagged string.
- **Consistency:** vix has three fallibility idioms (fail-the-demand + `?`, a
  `Result` twin, `Option`) applied unevenly. Adopt the rule: **a routinely-missing
  parse/lookup returns `Option`/`Result`; `?`/unwrap is for propagation.**

## Design insight worth not re-deriving: observe *is* a lockfile writer

`observe` is "fetch whose pin is minted-and-logged by the act of looking." The
per-coordinate append-only **claim history is a lockfile**; `observe` *writes* an
entry, `fetch` *reads* a pin, and `refresh` is `cargo update` (CAS-advance the
claim head, `RefreshConflict` on a concurrent move). This is why:

- fetch and observe are genuinely two effects, not one primitive with an optional
  hash: fetch is `Pinned` (no claim head), observe is `Observed` (appends one) —
  by the binding contract's own "do they share claim/memo state?" test, they don't.
- you must **not** collapse them into `fetch(url, hash?)`: a missing hash would
  silently flip the *trust model* (trustlessly-verifiable vs. trust-the-observer)
  and `place`-crossing legality. The right unification is one *authoring surface*
  over two witnessed behaviors (write `fetch url`; a recorded pin → pinned fetch,
  none → observe + offer to record it), never one primitive.

Whether `refresh` even belongs in the *surface language* is open: reading-and-
pinning (observe) is a value; racing-to-advance-a-mutable-head (refresh) is
administration and may belong in the runtime/CLI over the claim store.

## Pre-1.0 constraint: stable IDs

Function/module IDs are assigned by *position* — prelude items occupy the first
slots in the frozen order declared in `vixen-primitives/src/stdlib.rs`, so editing
the prelude renumbers everything after it, which is why order was frozen. #2517
took a first step here (`adding_std_does_not_shift_user_function_ids` proves the
`std` module can be appended without shifting *user* function ids). Before 1.0 we
need a general scheme that keeps IDs stable across surface edits so caching stays
reproducible. **For 0.1 the remaining renumbering churn is ignored on purpose.**
