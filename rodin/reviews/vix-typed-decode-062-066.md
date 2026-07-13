# Adversarial architecture review — typed-decode rungs 062–066

- **Base (immutable):** `102bd4376a8e335bd35e8fcf491493a6ab0c34b4`
- **Branch:** `audit/vix-typed-decode-062-066`
- **Tree state at review:** clean, `HEAD == base` (verified before any action).
- **Scope:** read-only. No implementation, fixture, generated file, or spec was
  edited. The only repository change is this artifact.

## Central determination

The branch's own claim — *literal JSON/TOML is parsed at compile time through
`facet-format`'s `FormatParser` directly against the expected Vix `Type`, then
emitted as ordinary typed-construction VIR; no runtime `HostCall` or generic
`Doc` exists* — is **accurate as a description of what the code does**. What is
in dispute is what that code *is*.

It is a **faithful as-if constant-fold of the decode operation, restricted to
compile-time-constant documents**. For a literal source string the fold is a
legitimate as-if rewrite: the computation is pure and deterministic, and the
emitted VIR uses the exact op vocabulary a hand-authored literal produces
(`Op::Record`, `Op::String`, `Op::Variant`), so the constructed value interns to
the same content-addressed handle. Rungs 062–065 are behaviorally correct **for
that subset** and run through the production `run_source` path.

It is **not** the runtime primitive the doctrine describes, and the surface has
**accidentally made `json_decode`/`toml_decode` a literal-only compile macro**:

- `r[machine.primitive.typed-deserialization]` (`vix/docs/content/spec/machine/primitive.md:172`)
  is marked **[DESIGN]**, not SETTLED, and specifies a **runtime host call** —
  "one host call per document, typed store values out … Generic Doc access
  remains for dynamic/exploratory use only." The implementation performs **zero**
  host calls; the test at `vix/tests/ratchet_runner.rs:5398` *asserts* their
  absence. The doctrine's "one host call per document" is satisfied invertedly
  (by having none), and the primitive's stated purpose — *dynamic* documents —
  is unreachable.
- A runtime `String`/`Blob` (the real doc-parse use case: a fetched registry
  index, a manifest read from disk) can never be decoded. `lower_decode` requires
  an `ast::Expr::Str` and otherwise rejects with `Diagnostic::unsupported`
  (`vix/src/compiler.rs:4006`). There is no runtime decode op anywhere in
  `vir.rs`, `exec.rs`, or `lowering.rs` (grep: none).
- Decode failure is a **compile-time diagnostic** (`UnsupportedExpression`,
  `vix/src/compiler.rs:4018`), not a runtime `Outcome`/`Failure` value. Rung 066
  — the failure/`Result` rung — is correctly still red, but it stops at the
  *grammar* (`ParseRejected`, `vix/tests/ratchet_runner.rs:5478`); the
  decode-failure path is never even reached, and the architecture provides no
  seam that would reach it.

So the answer to the central question: **not an accidental corruption of a
runtime primitive — a legitimate as-if fold of a subset — but presented with an
overclaim, and architected in a way that provides no path to the dynamic and
fallible cases the primitive exists for.** The green rungs are sound; the risk is
faithfulness of framing and the absence of the dynamic/failure seam.

## Disposition

**ACCEPT 062–065 for integration**, as a *literal-document constant-fold lane*,
conditioned on:

1. **Correcting the framing (F1).** The module doc, the `lower_decode` comment,
   and the ratchet certificate must present this as an as-if constant fold of
   *literal* decodes, not as "the scheduler-edge realization of
   `r[machine.primitive.typed-deserialization]`." The `r[...]` cross-reference
   should read "constant-folded subset of," not "realization of."
2. **Recording that the fold must become a constant-fold *of* a real runtime
   primitive when 066+ lands**, not a replacement for it (F2/F3).

Keep 066 red — it correctly is. Behavior of 062–065 is correct for its subset and
exercises production `run_source`. Nothing here warrants reverting committed work;
the corrections are additive (framing + a future runtime seam).

## Findings, ranked by severity

### F1 — High — Overclaim: a zero-host-call compile fold presented as the runtime doc-parse primitive
`vix/src/decode.rs:1`–`15`, `vix/src/compiler.rs:3981`, `vix/tests/ratchet_runner.rs:5378`–`5387`
vs `vix/docs/content/spec/machine/primitive.md:172`–`178`, `vix/tests/ratchet/FOUNDATION.md:55`.

The doctrine primitive is a runtime host call producing typed store values from a
(dynamic) document. The implementation drives the parser inside
`Compiler::compile` and emits pure construction VIR with no host call. The code
and certificate claim this *is* that primitive. It is instead an as-if fold of the
constant-input case. FOUNDATION §gate ("To score past 066 | typed decode through
the doc-parse primitive — one host call per document") is being demonstrated by an
architecture that structurally cannot host that host call.

*Corrective seam:* reword the three sites above to name the mechanism (compile-time
constant fold of literal documents) and downgrade the `r[...]` reference to
"constant-folded subset of." No behavior change.

### F2 — High — Nonliteral input is an accidental rejection, not a typed seam
`vix/src/compiler.rs:4006`–`4011` (`"a decode source that is not a string literal"`),
`4012`–`4017` (`"a decode whose target type is not known from context"`).

The surface accepted is exactly `let x: T = json_decode("<literal>")`. A runtime
`String`/`Blob` — the primitive's whole reason to exist — yields
`Diagnostic::unsupported`. This is a clean rejection (not a panic; hypothesis of a
panic is **withdrawn**), but it is *accidental*: there is no typed runtime seam,
so the design cannot extend to dynamic documents without new machinery.

*Corrective seam:* introduce a runtime decode VIR op — a real `HostCall`/`observe`
node typed `Result<T, DecodeError>` that accepts a runtime `String` — and make
`lower_decode` emit it for nonliteral args, *constant-folding* it only when the
argument is a literal (and the target is infallible-by-construction). The literal
fold then is provably the constant-folded form of the same primitive, which is
what the as-if doctrine licenses.

### F3 — High — Decode failure is a compile diagnostic, not a runtime `Failure`, and `DecodeError` is stringly
`vix/src/compiler.rs:4018`–`4030`, `vix/src/decode.rs:41`–`52`
vs `vix/docs/content/errors.md:20`,`42`,`107`–`116` and `vix/tests/ratchet/FOUNDATION.md:53`.

`errors.md` defines `Outcome<T> = Ok(T) | Failed(Failure)` with payload/subject/
site, poisoning demanders, cached, span+chain reconstructed at read. A malformed
document at a decode site instead becomes an `UnsupportedExpression` *compilation*
failure. `DecodeError { message: String }` is a **stringly** error, which
FOUNDATION.md:53 explicitly bans ("typed errors, no stringly `Result<_, String>`").
Rung 066's own fixture reads `e.message.contains("name")`
(`vix/tests/ratchet/066-decode-failure.vix`), i.e. the red rung is authored
against the same stringly surface — the target of 066 needs redesign before it can
go green, not merely a grammar extension.

*Corrective seam:* model `DecodeError` as a typed Vix `Failure` payload
(subject = field path, site = call span), surfaced as `Outcome<T>` /
`Result<T, DecodeError>` at runtime. A failing *literal* decode should fold to
`Failed(Failure)` on the fallible surface, or remain a *typed* diagnostic that
carries the document byte span — never `UnsupportedExpression`.

### F4 — Medium — Decode errors drop the document source span
`vix/src/decode.rs:313`–`331` (`consume`/`peek_kind` read only `.kind`),
`vix/src/decode.rs:359`–`361` (field path as string prefix),
`vix/src/compiler.rs:4019` (`call.span` for the whole call)
vs `facet-format/src/event.rs:456`–`460` (`ParseEvent { pub span: facet_reflect::Span }`)
and `vix/docs/content/errors.md:20`,`99` (the current byte span is part of the failure).

`facet-format` carries a byte span on every event; the vix decoder discards it and
attributes every failure to the entire `json_decode(...)` call. Field *path* is
preserved (recursively: `field "outer": field "inner": …`), but the offending byte
range in the literal is lost. Bounded for a literal fold, but the attribution
machinery the runtime primitive (F2/F3) will need does not exist.

*Corrective seam:* add `span: Option<Span>` to `DecodeError`, thread
`ParseEvent.span` through `consume`/`peek_kind`, and offset it into the literal's
source span at the call site.

### F5 — Medium — Enum variants selected by payload *shape*, not tag; ambiguous shapes silently pick the first
`vix/src/decode.rs:146`–`214` (`decode_enum`, `string_form_variant`, `table_form_variant`).

Variant selection is: scalar string → *first* single-`String` tuple variant;
object → *first* record-payload variant. An enum with two `String`-tuple variants,
or two record variants, silently binds the first with no diagnostic. Scalar forms
are String-only: an integer scalar against an enum with an `Int` tuple variant
fails as "no short (single-string) form" (`decode.rs:150`–`163`). Canonical for the
Cargo `DepSpec` shape (rung 065), unsound as a general enum decoder; no externally-
or adjacently-tagged form exists.

*Corrective seam:* select by an explicit representation/tag contract on the enum;
reject ambiguous shape-only matches with a typed diagnostic; either add scalar
forms beyond `String` or document the restriction as intentional.

### F6 — Low — The "identity-equivalent to authored construction" claim is unverified by an oracle
`vix/src/decode.rs:8`–`10`, `vix/tests/ratchet_runner.rs:5384`.

The claim is asserted structurally (no `HostCall` in lowered frames) and by
`expect_eq` on values, but no test compares the decode-emitted island's canonical
recipe / store handle against the hand-authored literal of the same value. The
claim is **structurally sound** — same ops (`Op::Record`/`Op::String`/`Op::Variant`
with `OPTION_SOME_VARIANT`/`OPTION_NONE_VARIANT`, matching `lower_named_constructor`
at `compiler.rs:5782`, `lower_some` at `3974`, `lower_none` at `3926`), and identity
is content-based, so per-node `call.span` differences are not identity-visible
(FOUNDATION §4). I therefore do not dispute it — but the strongest, cheapest test
in this shop's identity model is absent. Note also: because the decode is frozen at
compile time, the chaos/replay lanes validate the *constructed value*, not the
decode; a decode semantic divergence would be caught only by the `expect_eq`
content checks, and only for literal inputs.

*Corrective seam:* add an oracle asserting the decode island's canonical recipe /
store handle equals the authored literal's for at least one rung (e.g. 062).

## Hypotheses the current source refutes (explicitly withdrawn)

- **"The fold improperly changes demand/read-set semantics."** Withdrawn for the
  shipped (literal) case. Decode of a compile-time-constant string is pure and
  deterministic; folding it is a legitimate as-if with an identical observable
  value, and `pure_host_calls == 0` / `receipt_count == 0`
  (`vix/tests/ratchet_runner.rs:5416`–`5417`) are correct, not a smell. A demand-
  semantics divergence appears only for the *unimplemented* dynamic case (F2).
- **"`facet-format` is an unwanted host authority."** Withdrawn. It is a pure
  compile-time parser dependency emitting a schema-agnostic event stream; it is
  the correct shared edge. The type-direction lives in the vix decoder, which
  chooses what to expect from `ty` (`decode.rs:97`–`140`), not from the events —
  so there is no hidden untyped *authority* deciding structure. Caveat: for TOML
  the underlying parser buffers the whole document into a `Vec<Event>` first
  (`facet-toml/src/parser.rs:161`–`186`), so the "one pass / nothing materialized"
  wording is loose at the parser layer — though the vix decoder is single-pass and
  materializes no generic Vix `Doc`.
- **"Nonliteral input panics / crashes."** Withdrawn — it is a clean
  `Diagnostic::unsupported` (`compiler.rs:4006`). Reframed as F2 (accidental
  rejection, not a crash).
- **"Field order is document-order (non-canonical)."** Withdrawn. The decoder emits
  *declaration* order via indexed slots regardless of document order
  (`decode.rs:236`–`289`); unknown, duplicate, and missing-non-`Option` fields are
  all strict typed failures; missing-`Option` resolves to `None`; trailing content
  is rejected (`expect_end`, `decode.rs:333`–`345`). Field handling is canonical.

## What the surface accepts, and how each anomaly behaves (evidence)

| Case | Behavior | Anchor |
|---|---|---|
| Nonliteral source (`String` var) | `Diagnostic::unsupported` (compile) | `compiler.rs:4006` |
| No target type from context | `Diagnostic::unsupported` (compile) | `compiler.rs:4012` |
| Malformed / type-mismatched document | `Diagnostic::unsupported` (compile), stringly message | `compiler.rs:4018`, `decode.rs:42` |
| JSON float / `1e2` into `Int` | rejected "expected Int, found a float" | `decode.rs:119`,`375` |
| Integer > i64 via u64 | range-checked convert, else error | `decode.rs:116`–`118` |
| Absent `Option` field | `None` | `decode.rs:280` |
| Absent non-`Option` field | "missing field" failure | `decode.rs:281`–`287` |
| Unknown field | "unknown field" failure (strict) | `decode.rs:260`–`264` |
| Duplicate field | "duplicate field" failure | `decode.rs:251`–`254` |
| Field order | declaration order (slots) | `decode.rs:236`,`276`–`289` |
| Enum scalar form | first `String`-tuple variant | `decode.rs:190`–`202` |
| Enum object form | first record-payload variant | `decode.rs:205`–`214` |
| Trailing content | rejected | `decode.rs:333`–`345` |
| Runtime failure (`try_json_decode<T>`) | red at grammar (`ParseRejected`) | `ratchet_runner.rs:5478` |

## Resolution audit — implementation `00f3ca354`

The corrective implementation satisfies this review's integration conditions.
The disposition is now **ACCEPT 062–065** as the literal-document constant-fold
lane; rung 066 remains red at its exact grammar/Outcome boundary.

- **F1 closed.** The decoder module, compiler seam, and production certificate
  consistently call this the constant-folded subset of the designed runtime
  primitive. Zero `HostCall` is an as-if optimization certificate, not a claim
  that runtime typed deserialization exists.
- **F2 closed as a typed boundary.** A nonliteral document or unknown target now
  produces `DiagnosticCode::RuntimeDecodeUnavailable` with structured format and
  target fields. It is neither host-evaluated nor collapsed into a generic
  unsupported-expression diagnostic. The runtime primitive itself remains
  deliberately unbuilt.
- **F3 closed for the accepted fold lane.** Constant-document failures carry a
  closed `DecodeErrorKind`, structured field path, and structured span payload;
  rendered prose is convenience only. Runtime `Outcome`/`Failure` semantics stay
  on rung 066's red side and were not invented here.
- **F4 partially closed, with the residual explicit.** Parser document-byte spans
  and field paths are retained. Translating a decoded-document span back through
  Vix string-literal escapes into a source span is still unavailable; the code
  names the coordinate system and leaves the mapping absent rather than
  fabricating it. This was not a condition of the ACCEPT disposition.
- **F5 closed.** Multiple string-form or table-form variants produce typed
  ambiguity failures; declaration order is never used as a hidden first-match
  authority.
- **F6 closed.** A discriminating certificate compares canonical VIR,
  `RecipeId`, `DemandKey`, framed Store identities, and completed check
  identities against an authored construction, with a different-value negative
  control.

Independent re-audit selection:

```text
cargo nextest run -p vix -E 'test(decode::) | (binary(ratchet_runner) & (test(/decode/) | test(=typed_decode_066_red_boundary)))'
```

Run `a265d07b-57e6-47cf-a614-714db00cb56c`: 15/15 passed.

Authoritative integration verification:

- merged decode and cross-lane selection, default: run
  `fc12ca5c…`, 16/16 passed;
- the same selection under `WEAVY_JIT=0`: run
  `0ee50c9a…`, 16/16 passed;
- full Vix and Weavy suite: run
  `4e7cd165…`, 709/709 passed;
- the native-target CI interpreter selection after isolating the unchanged
  source-budget certificate from sibling-runner contention: run
  `ba042201-32f0-4055-a7b3-217f42949339`, 116/116 passed.

## Verification performed

- `git rev-parse HEAD` == base; `git status --porcelain` empty before acting.
- Read `vix/src/decode.rs` in full; `lower_decode`/`lower_decoded_value`
  (`vix/src/compiler.rs:3981`–`4143`); the decode certificate and all five rung
  fixtures; `run_source`/`prepare_source_with_cache` (`vix/src/ratchet.rs:529`+).
- Cross-read the normative sources: `primitive.md:172`, `errors.md`,
  `FOUNDATION.md:43`–`72`, `testing.md`.
- Confirmed no runtime decode op exists (grep of `vir.rs`/`exec.rs`/`lowering.rs`).
- Confirmed `facet-format` `ParseEvent` carries a span that the decoder drops, and
  that `TomlParser::new` pre-buffers the document.
- Confirmed authored-construction ops equal the decode ops
  (`Op::Record`/`Op::Variant`/`OPTION_*_VARIANT`).
