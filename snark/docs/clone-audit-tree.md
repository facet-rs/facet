# Clone/allocation audit: tree materialization + grammar prep

Scope: `snark/src/corpus.rs`, the tree-output side of `snark/src/lower/weavy.rs`
(`subtree_tree_events`, `RuntimeWeavyReusableNode`, `tree()`/report building),
and grammar prep (`snark/src/grammar.rs`, `snark/src/validated.rs`,
`ParserGrammar::normalize_from_validated`/`prepare_productions_for_items`,
`ItemMap::insert`).

Static audit only (grep + read + reason about call frequency), per the profile
context: a 181KB JSON parse spent ~95% of time in `malloc`/`free`/`memmove`,
with `drop_in_place<[snark::corpus::SexpChild]>` and
`drop_in_place<RuntimeWeavyReusableNode>` showing up hot. Both symptoms trace
straight to the findings below — this is a from-scratch parse (not an edit
session), so anything gated on incremental reuse still firing unconditionally
is especially suspicious.

Background fact that shapes everything here: `corpus::SexpNode` is a fully
owned, recursive tree (`children: Vec<SexpChild>`, `SexpValue::Node(SexpNode)`
inline, no indirection). It was designed for parsing small fixed corpus
fixture files and comparing them for equality — cheap to build once from text,
cheap to compare. `snark/src/lower/weavy.rs` reuses this exact type as the
*runtime* parser's output tree, rebuilt fresh on every reduce. That reuse is
the root cause of findings 1–3.

## Current Status

- **Resolved:** reusable-node collection is opt-in. From-scratch/report parses
  no longer build reusable-node payloads or walk subtree events for every
  reduction.
- **Resolved:** Weavy parse trees are stored by handle in
  `RuntimeWeavyTreeStore` child lists. Reductions no longer deep-clone child
  subtrees into owned `SexpNode` children on every fold.
- **Resolved:** flat-width and nested-depth JSON ladders are linear after the
  reuse opt-in and handle-based tree-store fixes.
- **Still open:** the remaining items are constant-factor or linear work:
  intern node/alias names, collapse double event storage if it proves hot, and
  avoid the final one-off tree clone if report shape changes.

The detailed findings below are retained as provenance. Resolved critical
entries describe the pre-fix state, not current behavior.

## 1. [RESOLVED, was O(n²)] `subtree_tree_events` walked the whole journal on every reduce, unconditionally

`snark/src/lower/weavy.rs:2823-2828`, called from `run_runtime_weavy_action`'s
`Reduce` arm, for **every** reduction that produces a `RuntimeWeavyFragment::Node`
with `start_byte < end_byte` — i.e. essentially every named node in the tree:

```rust
tree_events: self.tree_journal.subtree_tree_events(
    self.tree_journal_head,
    *start_byte,
    *end_byte,
    *node,
),
```

`RuntimeWeavyTreeJournal::subtree_tree_events` (weavy.rs:1937-1950) calls
`self.event_refs(head)` (weavy.rs:1952-1962), which walks the cactus journal
from the branch's *current tip all the way back to the root* — i.e. every
tree event emitted by this branch since the start of the parse — pushing each
into a freshly allocated `Vec<&TreeEvent>`. That's `O(events-so-far)`.

`runtime_weavy_subtree_tree_events_from_iter` (weavy.rs:4076-4108) then:
1. `.collect::<Vec<_>>()`s that iterator again,
2. builds a `BTreeSet<TreeNodeId>` by scanning it once,
3. scans it again, filtering + `.clone()`-ing matching events into the result.

So every single reduce pays a cost proportional to **all events emitted so
far in the document**, not to the size of the subtree being reduced. Summed
over every reduce in a parse, this is `O(reduces × total_events)` —
quadratic in input size. For 181KB of JSON (tens of thousands of reduces),
this is very likely the dominant cost in the profile: continuous allocation
and drop of `Vec<&TreeEvent>` and `Vec<TreeEvent>` at every level of the tree.

This is also called from `try_reuse_runtime_weavy_node`'s sibling
(`RuntimeWeavyReusableNode.tree_events`, see finding 3) purely to populate
**incremental-edit reuse bookkeeping** — a feature this from-scratch parse
never uses.

**Fix**: don't compute `subtree_tree_events` eagerly per reduce. Either:
- Store `(start_index, end_index)` into the journal's flat `entries: Vec<_>`
  for each node instead of a filtered event copy — journal entries are
  appended in order, so a node's own subtree is a *contiguous range* in the
  entries vector from when the node's construction began to when it closed
  (track that start index on the stack entry, no walk needed). Materialize
  the actual `Vec<TreeEvent>` slice lazily, only when
  `RuntimeWeavyReuseIndex::from_report` actually needs it (i.e. only when a
  caller performs an edit).
- Or, at minimum, make `reusable_nodes` population itself lazy/opt-in (see
  finding 3) so this cost disappears entirely for parses that never edit.

## 2. [RESOLVED, was O(n·depth)] `into_children` deep-cloned the whole child subtree on every fold

`snark/src/lower/weavy.rs:4040-4048`:

```rust
fn into_children(self, tree_store: &RuntimeWeavyTreeStore) -> Vec<SexpChild> {
    match self {
        Self::Hidden { children, .. } => children,
        Self::Node { node, .. } => vec![SexpChild {
            field: None,
            value: SexpValue::Node(tree_store.node(node).clone()),
        }],
    }
}
```

Called from `runtime_reduce_fragment` (weavy.rs:3524) for every popped stack
entry of every reduce: `let mut step_children = fragment.into_children(self.tree_store);`.
`tree_store.node(node)` returns `&SexpNode` — the *already fully materialized*
recursive tree for that child, built the same way when the child itself was
reduced. `.clone()` here does a full recursive deep copy (every descendant
`String` kind, every nested `Vec<SexpChild>`) into the parent's `children`.

That cloned tree then gets embedded in a new `SexpNode` that is itself pushed
into `tree_store` (weavy.rs:3593) — so the *next* level up will clone this
already-doubled copy again. A leaf `n` levels deep from the root gets
recursively deep-copied roughly once per ancestor level: total copying is
`O(n · average_depth)`, not `O(n)`. For nested JSON (arrays of objects of
arrays…) this compounds badly and directly explains
`drop_in_place<[snark::corpus::SexpChild]>` showing up hot — those vectors
are the *duplicate* copies, not the originals.

**Resolution**: Weavy now stores parse trees by handle during reduction and
materializes the owned `corpus::SexpNode` projection once at accept. Flat-repeat
and nesting-depth ladder checks both moved back to linear scaling after that
change.

## 3. [RESOLVED] `reusable_nodes` (incremental-reuse bookkeeping) populated unconditionally, on every reduce, even for parses that never edit

`snark/src/lower/weavy.rs:2812-2829` (fresh reduce) and mirrored at
weavy.rs:2186-2198 (reuse-path) push a `RuntimeWeavyReusableNode` for **every**
node-producing reduce:

```rust
self.reusable_nodes.push(RuntimeWeavyReusableNode {
    tree: self.tree_store.node(*node).clone(),   // full deep clone, see #2
    ...
    tree_events: self.tree_journal.subtree_tree_events(...), // O(n) walk, see #1
});
```

This is the direct source of `drop_in_place<RuntimeWeavyReusableNode>` in the
profile. `reusable_nodes` exists purely to let a *later* `session.edit(...)`
call find nodes it can reuse (`RuntimeWeavyReuseIndex::from_report`,
weavy.rs:1302). A one-shot parse of a 181KB JSON file that's never edited
still pays a full subtree clone + full journal walk for every node in the
document, for a feature it never uses.

**Resolution**: reuse collection is opt-in. From-scratch report parses skip the
subtree replay payload entirely; incremental sessions request it when they need
to build a reuse index from the accepted report.

## 4. [RESOLVED for runtime tree store] Node-kind `String` allocated fresh from an already-owned `String` on every reduce

`snark/src/parser.rs:2260-2263`: `PublicNodeKind.name: String` is built once
at grammar-prep time and lives for the parser's lifetime. But every runtime
reduce that produces a public node re-allocates a fresh copy of that same
string:

- `snark/src/lower/weavy.rs:3592`: `let kind = self.parser.public_node_kinds()[...].name().to_owned();`
- `snark/src/lower/weavy.rs:3889`: same pattern in `extra_node_for_lookahead`
- `snark/src/lower/weavy.rs:4290`: same pattern in the reuse-remap path

Before the handle-based tree store, each of these fed directly into a
`SexpNode { kind, .. }` that was then deep-cloned repeatedly per findings 2–3.
After the handle-based tree-store fix, the remaining issue was the per-node
runtime allocation needed to duplicate a string the grammar table already owns
for the parser lifetime.

**Resolution**: `WeavyParserProgram` now owns `Arc<str>` public-node names, and
the transient Weavy tree store carries those handles through reduction. The
public `corpus::SexpNode` projection still owns `String` kinds when materialized
at accept.

## 5. [RESOLVED for runtime tree store] Alias name `.to_owned()`/`.clone()` per aliased production step

`snark/src/lower/weavy.rs:3530-3552`, inside `runtime_reduce_fragment`, for
every step that carries a grammar alias (`step.alias()`):

```rust
let alias_name = self.parser.aliases()[alias.get() as usize]
    .value()
    .to_owned();
...
kind: alias_name.clone(),   // if step_children was empty
...
node.kind.clone_from(&alias_name);  // for every existing child, if non-empty
let alias_node = self.tree_store.push(SexpNode {
    kind: alias_name,   // moved here
    ...
});
```

Same shape as finding 4: `AliasDecl.value()` is a table string owned for the
parser's lifetime. `WeavyParserProgram` now owns `Arc<str>` alias names, and
alias tree-store entries clone those handles rather than allocating fresh
strings.

## 6. [RESOLVED] Double-storing tree events (journal + flat `Vec`)

Several sites push the same `TreeEvent` into both the cactus journal *and* a
flat `Vec<TreeEvent>` (`self.tree_events` / `output.tree_events`), requiring a
`.clone()` for one of the two copies — e.g. weavy.rs:2158-2163 (reuse path,
also cloning `replayed_events` twice), weavy.rs:2714-2717 (Shift), 2787-2788
(Reduce, cloning the whole `reduction.tree_events` Vec into the journal too).
Since `TreeEvent` is entirely `Copy`-able scalar fields (ids, byte/point
ranges, bools — see `snark/src/parser.rs:5954+`), each individual clone is
cheap; the cost here is the double `Vec` allocation/storage, not deep-copy
cost. This matches the "known minor redundancy" already flagged in
`snark/docs/weavy-tree-journal.md`.

**Resolution**: the hot parser path now keeps only the branch journal. Accepted
reports materialize their `Vec<TreeEvent>` with `RuntimeWeavyTreeJournal::collect`
at accept.

## 7. [RESOLVED] One extra full clone of the final tree at parse-accept time

`snark/src/lower/weavy.rs:1703-1707`:

```rust
let Some((first_version, first_node, _, first_tree_events, first_reusable_nodes)) =
    best_accepted.first().map(|accepted| (**accepted).clone())
```

The current accept path owns the accepted branch vector and moves the first
winning tree/events/reuse payload into `WeavyParseReport` with `remove(0)`.

## 8. [RESOLVED] `WeavyParseReport` retained `tree_store` alongside the already-flattened `tree`

`WeavyParseReport` no longer retains `RuntimeWeavyTreeStore`. The accepted
resolved CST view derives node kinds from `TreeEvent` payloads and
`ParserGrammar` production/public-node metadata, so the parse tree store remains
transient and drops when the parse function returns.

## 9. [LOW, one-time, grammar prep] Not on the hot per-parse path

Checked `snark/src/grammar.rs`, `snark/src/validated.rs`,
`ParserGrammar::normalize_from_validated`/`seed`/`prepare_productions_for_items`,
and `ItemMap::insert` (`snark/src/parser.rs:3244-3280`, `BTreeMap`-keyed item
sets). All clones found here (`RuleName::new(name.clone())` ×3 in
grammar.rs:108/120/127, `LanguageName::new(self.name.clone())` at
grammar.rs:226, `PrecedenceGroupEntry::Name(name.clone())` at parser.rs:241,
`.name.to_owned()` at parser.rs:189) run once per grammar rule/production at
grammar-load time, not once per parse or per token. Given the recent
"steady-state parse-throughput bench (prepare once, parse N times)" commit
already isolates prep from the per-parse hot loop, these are not implicated
in the 181KB-JSON-parse profile. `ItemMap::insert` (flagged in the task brief
as hot in a *sibling* profile) is LR item-set construction — also one-time
per grammar. Not worth touching unless a profile of grammar *loading itself*
(not parsing) asks for it.

---

## Summary, ranked by impact

| # | Finding | Asymptotic cost | Confidence |
|---|---|---|---|
| # | Finding | Status |
|---|---|---|
| 1 | `subtree_tree_events`/`event_refs` walked the whole journal per reduce | Resolved by opt-in reuse collection |
| 2 | `into_children` deep-cloned child subtrees per fold | Resolved by handle-based tree storage |
| 3 | `reusable_nodes` populated unconditionally | Resolved by opt-in reuse collection |
| 4 | Node-kind `String::to_owned()` per reduce | Resolved in transient tree store; public S-expression still owns strings |
| 5 | Alias-name `String` clone per aliased step | Resolved in transient tree store |
| 6 | Double journal+flat `Vec<TreeEvent>` storage | Resolved by collecting accepted events from the branch journal |
| 7 | One extra full-tree clone at accept | Resolved by moving the accepted payload |
| 8 | `tree_store` retained alongside `tree` | Resolved by deriving resolved CST kinds from events |
| 9 | Grammar-prep clones | Not hot per-parse |

**Remaining order of attack**: measure before touching the remaining linear
items. The algorithmic width/depth scaling issues from #1–#3 are no longer
current work items.
