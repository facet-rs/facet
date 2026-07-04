# Gingembre unification recon (no build, no commitment)

This is the recon round called for by the `secrets-as-sealed-values.md` Q1
ruling (`/Users/amos/vixenware/vixen/docs/design/secrets-as-sealed-values.md:40-47`):
before committing to lowering gingembre (Dodeca's template engine) onto the
vix/snark/weavy machine, understand what gingembre and picante actually do
today, and stress-test the thesis:

> "A template IS a grammar (snark parses it) and rendering IS evaluation with
> holes as demand edges — so a template is a vix function of its holes."

All file paths below are absolute; line numbers are as of the checkouts read
during this recon (2026-07-04). This doc makes no code changes to gingembre,
picante, dodeca, or vix.

## 1. Gingembre and picante today

**Gingembre** is a Jinja-like template engine used only by Dodeca
(`/Users/amos/oss/facet-cc/gingembre/README.md:6-9`, `/Users/amos/oss/facet-cc/gingembre/src/lib.rs:4-24`).
Its grammar is a hand-written recursive-descent + Pratt parser producing a
`cstree`-based lossless CST, defined in the separate `gingembre-syntax` crate
(`SyntaxKind` at `/Users/amos/oss/facet-cc/gingembre-syntax/src/lib.rs:30-240`,
lexer/parser/ast at `gingembre-syntax/src/{lexer,parser,ast}.rs`). The formal
spec is `/Users/amos/oss/dodeca/docs/spec/gingembre.md` (771 lines, tracey-covered).

Template feature surface (full inventory with citations gathered during this
recon — summarized here, see the recon transcript for the per-feature table):
interpolation `{{ }}`, statements `{% %}`, comments `{# #}` with whitespace
trim variants (`{{- -}}`, `{%- -%}`); `if/elif/else`, `for ... in ... [else]`
with `break`/`continue` and `loop.*` metadata, `set` (block-form `{% set
%}...{% endset %}` was explicitly deferred as of the cstree rewrite,
`/Users/amos/oss/dodeca/notes/gingembre-cstree-parser.md:108`); `extends` +
`block`/`endblock` inheritance (`/Users/amos/oss/facet-cc/gingembre/src/render.rs:269-291`);
`include` (`render.rs:616`); a macro system with `import`/`macro`/`self::`
namespacing where macros call other macros recursively
(`render.rs:808-845`); 26 built-in filters and a test (`is`) system
(`/Users/amos/oss/facet-cc/gingembre/src/eval.rs:21-159`); field/index/slice
access, optional (`?`) lenient access, ternary expressions.

**Shortcodes are not a gingembre grammar construct.** Markdown rendering (a
separate cell) emits `<dodeca-shortcode data-name=... data-args=...>body</dodeca-shortcode>`
HTML placeholders (`/Users/amos/oss/dodeca/crates/dodeca/src/shortcode.rs:1-6`),
and Dodeca resolves them with a **string-level, innermost-first HTML-tag
replace loop** (`shortcode.rs:26-69`), where "nesting" is a property of the
markdown-emitted HTML, not of gingembre call graphs. Each shortcode
occurrence gets its own independent `gingembre` `Engine::render` call
(`shortcode.rs:90-158`). One shortcode (`include`) is a Rust built-in, not a
template (`shortcode.rs:99-105`).

**Picante** is Dodeca's own general async incremental-query runtime,
"inspired by Salsa" (`/Users/amos/oss/facet-cc/picante/README.md:1-13`). It is
**not a gingembre dependency** — gingembre's `Cargo.toml` doesn't list it
(`/Users/amos/oss/facet-cc/gingembre/Cargo.toml:15-22`); gingembre only
exposes a `DataResolver` trait and a `LazyValue` wrapper so a picante-aware
host *can* track fine-grained access (`/Users/amos/oss/facet-cc/gingembre/src/lazy.rs:1-100`,
doc comments only mention picante, no import). The actual picante wiring is
entirely in Dodeca's `crates/dodeca/src/queries.rs`: `render_page` is one
big `#[picante::tracked]` query (`queries.rs:1354-1396`), similarly
`load_all_templates` (`queries.rs:30-31`) and 30+ other tracked queries.
**Memoization granularity today is whole-page** (one `render_page` query per
page), not per-template or per-hole. Within a single render, gingembre
reparses every template from source text on every `Engine::load` call — there
is no AST cache anywhere in the crate (`/Users/amos/oss/facet-cc/gingembre/src/render.rs:116-314`,
confirmed no cache field on `Engine`, no `HashMap<String, Template>`).

**Snark is not in production for gingembre at all.** `gingembre-snark-spike`
is explicitly a throwaway ("S P I K E — T H R O W A W A Y. Do not build on
this.", `/Users/amos/oss/facet-cc/gingembre-snark-spike/src/main.rs:1-14`) that
answers "can snark be gingembre's front end?" for a narrow expression subset
only (binary/number/variable/literal —
`/Users/amos/oss/dodeca/notes/gingembre-snark-ast.md:35-36`), achieving a
byte-identical oracle against native gingembre render but running **15-25x
slower** than the hand-written parser due to snark's GLR bookkeeping
(`/Users/amos/oss/dodeca/notes/gingembre-snark-ast.md:38-45`,
`/Users/amos/oss/dodeca/notes/gingembre-snark-lean-parse.md:1-20`). A "lean
deterministic" fast-path fix is proposed but not implemented, blocked on new
snark public API surface (`gingembre-snark-lean-parse.md:82-99`). Dodeca's
`Cargo.toml` has no `snark` dependency at all. The one production seam
prepared for an alternate frontend is `Template::from_ast`
(`/Users/amos/oss/facet-cc/gingembre/src/render.rs:155-167`), explicitly
documented as "the seam an alternative front end (e.g. a snark-based parser)
needs" — unused today.

## 2. What the vix machine actually provides (the target substrate)

Read directly in this repo (`vix/src/machine/lower.rs`, `vix/src/lib.rs`,
`vix/src/machine/PARITY.md`, `vix/docs/cargo-manifest-build.md`) rather than
gingembre-side docs, since this is the substrate the thesis proposes to land
on.

- **Grammar-is-the-source-of-truth is already how vix itself works.** `vix/src/lib.rs:1-19`:
  the vix AST is generated at build time from `playgrounds/snark/src/bundled/vix/grammar.js`
  via snark ("Nobody hand-writes the AST"). This is the exact pattern the
  thesis proposes for gingembre — vix is an existence proof, not a fantasy.
- **A user-function call is a memo boundary through an INVOKE protocol**
  (`vix/src/machine/lower.rs:2245-2248`, `call()` at `2249-2338`): args are
  computed into slots, copied into an invoke region, then `HostCall + Await`.
  Memo identity is the function's closure hash plus argument values
  (`lower.rs:12` module doc; `PARITY.md` O02/O04/O08 pin closure-hash
  stability and exact "blast radius" reruns on edits). This is the "demand
  edge" mechanism the thesis wants holes to ride.
- **Laziness is real and load-bearing**: unused bindings, untaken match
  arms, and undemanded function calls never spawn, never trace
  (`PARITY.md` G06/G07/G09; `lower.rs` tests `untaken_arms_never_spawn`,
  `undemanded_functions_never_trace` at `4578-4587`). Warm reloads reuse the
  memo/value store and recompute exactly the transitive closure of what
  changed (`PARITY.md` O03/O04: editing one leaf misses exactly `{leaf, left,
  right, main}`; editing something undemanded costs **zero** misses). This is
  the actual mechanism behind "only affected pages re-render."
- **Structured documents already exist as a machine type**: `toml()`/`json()`
  lower to `doc_parse_call` (`lower.rs:2265-2266`, `4058-4095`) producing a
  `Doc` schema value; `PARITY.md` E08/E09 confirm TOML/JSON parse into
  "structural `Doc` values" where `.get()` stays structural (doesn't
  materialize to strings early) and nested projection is a first-class op.
  This is the closest existing analog to what a gingembre "hole" (a `Doc`
  field access into page/site data) would look like as a vix value.
- **What's conspicuously absent from the current grammar**: no loop/iteration
  construct. `playgrounds/snark/src/bundled/vix/grammar.js` has `match_expr`
  (lines 156, 182, 250) and function calls, but no `for`, `while`, or
  `map`-over-collection expression form. The only iteration-shaped thing in
  the parity ledger is `[2,1].collect(0)` / `[2,1].collect()` on a **scalar
  `Array<Int>`**, which sorts, not maps (`PARITY.md` G01). There is no
  vix-level "apply this closure to each element and collect results" op
  today. **This is a real gap, not a detail** — see §3.

## 3. Evaluating the thesis against reality

### Where "template = function of its holes" fits cleanly

- **Plain interpolation** (`{{ page.title }}`, `{{ site.data.foo }}`) is
  structurally identical to a vix `Doc.get()` projection chain
  (`lower.rs` E08/E09 pattern). A hole that reads one page field is exactly
  one demand edge into a `Doc` value, and gingembre's own `DataResolver`/
  `LazyValue` abstraction (`gingembre/src/lazy.rs:1-100`) was *already*
  designed for exactly this kind of per-path tracked access — it's currently
  unimplemented in gingembre itself, waiting for a picante- (or vix-) shaped
  host. This is the strongest part of the thesis: it's not a stretch, it's
  gingembre's own documented intent finally getting a substrate.
- **Filters and tests** (`eval.rs` 26 filters, `is` tests) are pure functions
  of their input value(s) — they lower straight onto vix function calls
  (possibly builtins, like `toml`/`json`/`elf` are today) with no extra
  machinery needed.
- **`if`/`elif`/`else`** is a `match`/branch on a boolean-ish scrutinee —
  vix already has `match_expr` and untaken-arm laziness is exactly the
  property gingembre wants (an `if` branch that's never taken should never
  demand the data it would have read). Clean fit.
- **Per-hole secret taint** (the actual point of this recon, per the ruling)
  falls out of the memo/demand model for free *if* holes are demand edges:
  a `Doc` value derived from a sealed source stays taggable at exactly the
  granularity the machine already tracks provenance at (function closure +
  args). This is the one place where "it's not just plausible, it's the
  same mechanism already built for something else" — see §4 Q1 in the
  secrets doc.

### Where it does NOT fit cleanly — the hard parts

1. **Loops are not "just" holes.** `{% for post in posts %}...{% endfor %}`
   is not one demand edge, it's N demand edges fanned out from one
   collection value, each producing a fragment, concatenated back together
   in order. Vix has no map-collect primitive today (§2) — `[x].collect()`
   sorts a scalar array, it doesn't map a closure over elements. Building
   "for" on vix means adding a genuine new machine capability: something
   like `Array<T>.map(closure) -> Array<U>` plus a "join array of strings in
   order" reduction, wired so that (a) each element's rendering is its own
   memo boundary (so editing one post's title only reruns that post's
   fragment, not the whole loop) and (b) order is preserved deterministically
   for output byte-stability. This is more than a lowering exercise; it's a
   language/machine feature that doesn't exist and needs its own design
   (per-element memo keys probably need to be `(loop_body_fn_hash, element's
   own identity)`, not just index, or edits to unrelated list items would
   still shift every subsequent index's memo key — classic "insert at head
   of list invalidates everything downstream" problem familiar from
   incremental-computation literature, not solved by "just use vix's
   existing call memoization").
2. **Whitespace control is not a value-level property — it's textual,
   cross-node, and stateful.** `compute_trim`
   (`/Users/amos/oss/facet-cc/gingembre/src/cst_lower.rs:178-192`) inspects
   sibling text nodes at CST-lowering time and mutates adjacent string
   literals (`cst_lower.rs:282-291`, `.trim_start()`/`.trim_end()`). A vix
   function-of-holes model produces a value (a string, presumably) per
   node; there's no natural place for "trim my neighbor's trailing
   whitespace" in a demand graph where nodes only see their own inputs.
   This has to be resolved entirely at the CST→grammar-node-sequence lowering
   stage, before anything becomes a vix expression — i.e. whitespace control
   is a **parse-time textual transform**, not part of the evaluation
   semantics, and the unification has to keep treating it that way rather
   than trying to make it "some hole's problem."
3. **Includes/extends are calls to *named*, *externally-resolved* templates**,
   not calls to statically-known vix functions. `Engine::load` resolves a
   template name to source text via a `TemplateLoader` at render time
   (`render.rs:255-260`); `extends` walks a parent chain recursively
   (`render.rs:269-291`). For vix, "function name" is presently a
   statically-resolved identifier bound at lowering time (`fn_refs` lookup,
   `lower.rs:2270`) — turning "include this named file" into a vix call
   means either (a) doing template-name → vix-function-reference resolution
   as a *pre-pass* over the whole template set before lowering any one
   template (so `include` becomes an ordinary static call), which requires
   knowing the template graph in advance (Dodeca already builds
   `load_all_templates`, so this is available, just needs to happen before
   lowering, not after), or (b) adding genuinely dynamic/indirect call
   support to vix (calling a function value chosen at runtime), which is a
   bigger machine feature. (a) looks tractable given Dodeca's existing
   template-loading pass; (b) would be a much larger ask.
4. **Macros calling macros via `self::`** (`render.rs:833-845`) are ordinary
   recursive function calls once template-name resolution (point 3) is
   solved — this part is a clean fit, *contingent* on point 3 being solved
   first.
5. **Shortcodes are not gingembre constructs at all — they're a Dodeca-level,
   string-scanning HTML post-process over already-rendered output**
   (`shortcode.rs:26-69`). "Shortcodes calling shortcodes" today is not
   recursion in any evaluator sense; it's regex-adjacent nesting in rendered
   HTML text, only working because markdown emits properly nested tags. If
   gingembre becomes a vix-lowered grammar, shortcode resolution either (a)
   stays exactly as it is today — an opaque post-process step outside the
   template-as-function model, which means shortcode-injected content is
   NOT visible to the per-hole taint system described in §4 of the secrets
   doc (a real limitation, not a detail) — or (b) gets pulled *into* the
   grammar as a first-class node type (parsed inside markdown, not
   string-scanned afterward), which is a bigger scope change to how
   Dodeca's markdown/shortcode pipeline works, well beyond "lower gingembre
   onto vix." Recommend treating this as explicitly out of scope for a first
   unification pass, and flagging (a)'s taint gap to Amos rather than
   quietly accepting it.
6. **Lenient/optional access (`expr?`) and `default` filters silently
   swallow errors** (`gingembre-cstree-parser.md:33,65`) — meaning some
   holes are allowed to observe "was this present" without observing the
   value. That's fine for demand-tracking (an "is present" check is still a
   demand edge on the container, just not on the leaf) but needs explicit
   modeling: a taint system has to decide whether "checked presence of a
   secret-tainted map, got `null`" counts as tainted output. This is a real,
   not hypothetical, design question (see Q4 below) — Jinja-style languages
   lean on this pattern constantly (`default(...)`, `?`, `is defined`).

### Bottom line on the thesis

The thesis is right about the *interpolation core* — that part really is
"parse the grammar, evaluate holes as demand edges, get memoization and
taint for free," and vix's existing `Doc`/call/memo machinery is a genuine,
not superficial, match for it. It is **not** right, as stated, about the
*whole template engine* — control flow (loops especially), whitespace
control, and the include/extends/shortcode resolution layers are real
engineering problems that need either new machine capabilities (map-collect
with per-element memo keys) or a pre-pass architecture (static template-name
resolution before lowering) or an explicit scope cut (shortcodes staying
outside the model). None of these are fatal to the thesis, but "gingembre is
a vix function of its holes" undersells the amount of *new* design work
needed for anything beyond `{{ interpolation }}`.

## 4. Unification shape (sketch — not a plan)

Rough mapping, marked by confidence:

| Gingembre construct | vix lowering | Confidence |
|---|---|---|
| `{{ expr }}` interpolation | `Doc.get()` projection chain, direct value read | Clean — matches existing E08/E09 pattern |
| filters / tests | ordinary function calls (builtin or user) | Clean |
| `if/elif/else` | `match_expr` / boolean branch, untaken-arm laziness for free | Clean |
| `for ... in ...` | **new**: `Array<T>.map(closure) -> Array<U>` + ordered string-join reduction, with per-element memo keys keyed by element identity (not index) | Needs new machine capability + its own design (memo-key stability under list edits) |
| `set` (inline) | `let` binding | Clean |
| `set` (block form) | not even implemented in gingembre today — moot until it exists | N/A |
| `include "path"` | static call to a statically-resolved template-function, resolved in a pre-pass over the known template set (Dodeca already computes this set via `load_all_templates`) | Tractable, needs a pre-pass, not a new vix feature |
| `extends` / `block` | same static-resolution pre-pass, then ordinary nested calls with "the child's block wins" modeled as... an argument? a closure passed down? (open question, see Q3) | Design question |
| `macro` / `import` / `self::` | ordinary (possibly recursive) function calls, contingent on include/extends resolution | Clean once (above) is solved |
| whitespace trim (`{{- -}}`) | resolved entirely at CST→node-sequence lowering time, never reaches vix expressions | Clean, but must stay out of the evaluation semantics |
| shortcodes | **out of scope** — stays a Dodeca-level HTML post-process outside the vix-lowered template, OR pulled into the grammar as a first-class node (bigger scope) | Explicit scope cut recommended |
| `expr?` / lenient access | presence-check as a demand edge on the container without leaf materialization | Needs explicit taint-propagation rule (see Q4) |

**What replaces picante**: nothing needs to replace picante's role for
*non-template* Dodeca queries (asset processing, site-tree building, etc.)
— those stay picante-tracked exactly as today. What changes is: template
render stops being one opaque `#[picante::tracked] render_page` call
(`queries.rs:1354`) wrapping an uncached from-scratch gingembre parse+eval,
and becomes N vix `demand()` calls (per page, or finer — per template
function, if pages are decomposed into blocks) against a machine that
already gives per-function memoization and closure-hash-based blast-radius
recompute. Whether picante's page-level query still wraps the outermost vix
`demand()` call (picante for build-graph orchestration, vix for the
template evaluation itself) or whether vix's own memoization fully subsumes
picante's role for the render step specifically, is Q5 below.

**Per-hole taint sketch**: if `Doc` values (from `toml()`/`json()`/a future
"page-data" doc source) carry a recipient/seal tag the way the secrets doc
proposes, and every hole read is a `Doc.get()` projection (a demand edge,
per §3), then taint composition is exactly the existing "value derived from
X, X is tracked" story vix already tells for memo keys — a tainted leaf's
value participates in whichever `{{ hole }}` print node consumed it, and
that print node's output (a string fragment) becomes tainted by
construction, joining the rest of the rendered output structurally (per-leaf
where the machine understands the derivation — doc/tree/map/template ops,
matching the ruling's Q1(b) language verbatim). This is real, contingent on
(a) `Doc` gaining a taint tag at all (not yet true — this recon found no
sealed-value support in vix's `Doc` type; that's the separate sealed-values
work) and (b) loops/includes being solved per §3, since real-world templates
mixing secrets with loops (e.g. "for each recipient, print an API key") need
per-element taint, which inherits whatever memo-key design point 1 above
lands on.

## 5. Payoff ledger

1. **Dodeca warm incremental rebuilds** (edit a shortcode → only affected
   pages re-render by memo): **PARTIAL.** The mechanism is real for the parts
   of the thesis that land cleanly (interpolation, if/filters, macros once
   include-resolution is solved) — vix's warm-reload blast-radius behavior
   (`PARITY.md` O03/O04) is exactly "only the transitive dependents of what
   changed rerun," proven, not aspirational. But shortcodes specifically —
   the example named in the ruling — are explicitly the part of gingembre
   that is *not* a grammar construct today (§3.5); unless shortcodes are
   pulled into the grammar (a bigger scope change), "edit a shortcode" stays
   a whole-page-granularity Dodeca-level HTML post-process, not a fine
   vix-memoized call. The payoff is real for template-internal edits (change
   a filter, a macro body, an included partial) but the ruling's own headline
   example needs the shortcode-into-grammar move to actually get finer than
   page granularity.
2. **Config-generation as the same primitive**: **REAL, with a caveat.**
   `toml()`/`json()` already exist as vix builtins parsing into structural
   `Doc` values (`lower.rs:2265-2266`, `PARITY.md` E08/E09) — this is
   *already* the pattern config-gen would use, independent of gingembre. If
   gingembre templates lower to vix functions over `Doc` holes, then "render
   a config file from a template" and "render an HTML page from a template"
   genuinely become the same code path (a vix function call over `Doc`
   inputs producing a string). The caveat: config files rarely need loops
   over large collections or macro/include graphs the way HTML pages do, so
   this payoff is realest for the simplest slice of gingembre (interpolation
   + filters + if), which is also the part with the least remaining design
   risk — this is the safest place to start, not a bonus that comes free
   with the harder HTML-templating work.
3. **Per-hole secret taint = real**: **REAL for the interpolation/filter/if
   core, PARTIAL overall.** As argued in §4, the mechanism genuinely exists
   once holes are demand edges over tagged `Doc` values — this isn't
   hand-waving, it reuses the same provenance machinery vix already has for
   memo keys. But taint precision for loops and includes inherits whichever
   memo-key design gets chosen for §3 point 1 (map-collect) and point 3
   (static template resolution) — until those are designed, "per-hole taint"
   is proven only for templates with no loops and no includes, i.e. a
   fairly small slice of real Dodeca templates. Shortcode-injected content
   is a **taint blind spot** under the (a) scope cut in §3.5: a shortcode
   that emits a secret-derived string into rendered HTML is invisible to the
   grammar-level taint tracker, since shortcode resolution happens as an
   opaque string post-process outside the vix-lowered template evaluation.
   This should be surfaced to Amos explicitly, not silently accepted — see
   Q2 below.
4. **Picante dropped**: **DOESN'T FOLLOW, at least not entirely.** Picante is
   currently used for far more than gingembre rendering — 30+ tracked
   queries in `queries.rs` cover site-tree building, data-file parsing, font
   decompression, LSP flows (`dodeca/Cargo.toml:275` comment references
   "libs/lsp-types" flowing through picante-tracked queries too). Lowering
   *gingembre rendering specifically* onto vix's own memo model would let
   the render step stop needing picante, but "picante dropped" as stated in
   the ruling would require re-homing every other picante-tracked query in
   Dodeca onto something else too (either vix itself, generalized beyond
   template rendering into a general Dodeca build-graph substrate, or kept
   as picante indefinitely for non-template work). That's a much larger
   migration than "unify gingembre," and this recon found no evidence it's
   been scoped. Recommend treating "picante dropped" as a *possible
   consequence of a much later, separate migration* (Dodeca's whole build
   graph moving onto vix), not a direct payoff of the gingembre unification
   itself.

## Design questions for Amos

- **Q1 (loops/memo-key stability).** Map-collect over a collection needs
  per-element memo keys. Index-based keys break under insertion/deletion
  (everything downstream reindexes and misses). What's the intended
  element identity for lists in Dodeca templates — is there already a
  natural key (e.g. post slug, file path) that templates iterate by, or
  does this need a general "stable list diffing" primitive in vix itself
  (a real, non-trivial machine feature, not a lowering detail)?
- **Q2 (shortcode taint blind spot).** Given shortcodes resolve as an opaque
  HTML string post-process outside any vix-lowered template evaluation
  (§3.5), a shortcode that surfaces secret-derived content would currently
  evade the grammar-level taint tracker entirely. Is pulling shortcodes into
  the gingembre grammar (as first-class nodes, parsed inside markdown
  rather than string-scanned after) in scope for this unification, or should
  shortcodes be banned from touching sealed values by policy instead (a
  cheaper, narrower fix)?
- **Q3 (extends/block override modeling).** `{% extends %}` + `{% block %}`
  is "child overrides parent's named region." What's the intended vix
  shape for "a child template supplies a named override that a parent
  template's body calls into"? Options seem to be: parent template becomes
  a higher-order function taking block-implementations as closure
  arguments; or block resolution stays a pre-pass (like include resolution)
  that produces one flattened function per (child, parent-chain) pair
  ahead of lowering. Which direction fits vix's current closure/call model
  better?
- **Q4 (lenient access `expr?` / `default` and taint).** Should "checked
  presence of a value inside a sealed container, got null/absent" count as
  touching the seal (and thus require capability) or not? This is a real
  fork in the taint semantics with observable UX consequences (can a
  template safely do `{{ secrets.api_key is defined }}` without a reveal
  capability?).
- **Q5 (does vix subsume picante for rendering, or sit under it?).** Should
  Dodeca's page-level picante query become a thin wrapper around one vix
  `demand()` call (picante still orchestrates the overall build graph, vix
  owns template evaluation specifically), or is the intent for vix's own
  memoization to fully replace picante's role for rendering, with picante
  surviving only for the non-template queries listed in §5.4? This
  determines how much of `queries.rs`'s current tracked-query structure
  needs touching versus staying put.
- **Q6 (scope of "first slice").** Given §5.2's finding that config-gen is
  the *lowest-risk* payoff (parses `Doc`, no loops, no shortcodes, no
  extends chains typically), is the intended first concrete slice "port
  gingembre's interpolation+filter+if subset onto vix for config-gen
  first," proving the substrate before tackling HTML templating's harder
  parts (loops, includes, shortcodes)? That ordering seems to fall out of
  this recon regardless of the eventual full scope.
