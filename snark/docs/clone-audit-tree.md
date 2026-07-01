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

## 1. [CRITICAL, confirmed, O(n²)] `subtree_tree_events` walks the whole journal on every reduce, unconditionally

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

## 2. [CRITICAL, confirmed, O(n·depth)] `into_children` deep-clones the whole child subtree on every fold

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

**Fix**: this needs an actual architecture change, not a one-line patch:
`tree_store` should hold nodes by `TreeNodeId` indirection all the way through
construction (`SexpChild`/`SexpValue::Node` referencing a `TreeNodeId` rather
than embedding an owned `SexpNode`), and the recursive, fully-owned
`corpus::SexpNode` should only be materialized **once**, in a single
bottom-up flatten pass at the very end (`finish_runtime_root` /
`RuntimeWeavyReport` construction), where each `TreeNodeId` is visited and
moved into its owned form exactly once (memoize by id, or just move since
each id is the child of exactly one parent in the final accepted tree).
A cheaper, smaller-blast-radius interim fix: wrap `tree_store`'s entries in
`Rc<SexpNode>` internally so `into_children`'s clone becomes a refcount bump;
still requires one real deep-clone at the very end when the report's public
`tree: SexpNode` field is populated, but removes the O(depth) multiplier
during construction. (Caveat: GLR branch forking may let two live branches
reference the same `TreeNodeId` through the shared graph-structured stack, in
which case `Rc` is actually *required* for correctness of a take/move-based
scheme, not just an optimization — verify with the GLR-machinery agent before
attempting a move-based rewrite.)

## 3. [HIGH, confirmed] `reusable_nodes` (incremental-reuse bookkeeping) populated unconditionally, on every reduce, even for parses that never edit

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

**Fix**: make this opt-in. Either gate it behind a flag on the parse call
("build a reuse index" vs "just parse"), or — better — make it lazy: keep
only `(TreeNodeId, entry_state, scanner_snapshot, byte_range)` per node during
the parse (cheap, `Copy`), and defer materializing `tree`/`tree_events` for a
`RuntimeWeavyReusableNode` until `RuntimeWeavyReuseIndex::from_report` is
actually called (at which point `report.tree_store` and `report.tree_events`
are already available to reconstruct them on demand, once, only for nodes
that survive the edit-position filter — which is usually a small fraction of
the tree).

## 4. [MEDIUM, confirmed] Node-kind `String` allocated fresh from an already-owned `String` on every reduce

`snark/src/parser.rs:2260-2263`: `PublicNodeKind.name: String` is built once
at grammar-prep time and lives for the parser's lifetime. But every runtime
reduce that produces a public node re-allocates a fresh copy of that same
string:

- `snark/src/lower/weavy.rs:3592`: `let kind = self.parser.public_node_kinds()[...].name().to_owned();`
- `snark/src/lower/weavy.rs:3889`: same pattern in `extra_node_for_lookahead`
- `snark/src/lower/weavy.rs:4290`: same pattern in the reuse-remap path

Each of these feeds directly into a `SexpNode { kind, .. }` that then gets
deep-cloned repeatedly per findings 2–3. This is one heap alloc+free per named
node in the tree, purely to duplicate a string the grammar table already owns
for the whole parser lifetime.

**Fix**: change `PublicNodeKind.name` (and the corresponding field on the
public-facing side, if `SexpNode.kind` changes) to an `Rc<str>` interned once
at grammar-prep time. `.name()` returns `Rc<str>`/`&Rc<str>`; runtime code
does `Rc::clone` (refcount bump) instead of `.to_owned()` (alloc + memcpy).
This does touch `corpus::SexpNode::kind: String` if you want the public type
to also stop paying for it — lower priority than 1–3 since it's `O(n)` not
superlinear, but it's a large constant-factor multiplier on top of findings
2–3, and it's a small, self-contained, low-risk change.

## 5. [MEDIUM, confirmed] Alias name `.to_owned()`/`.clone()` per aliased production step

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
parser's lifetime; runtime code re-allocates a copy per occurrence, plus an
extra `.clone()` when the aliased step's children vector isn't empty (looped
over every child). Less hot than finding 4 (aliases are grammar-specific and
typically apply to a minority of productions — e.g. JSON likely uses few or
none), but same fix (`Rc<str>` for `AliasDecl.value()`).

## 6. [LOW, confirmed but cheap] Double-storing tree events (journal + flat `Vec`)

Several sites push the same `TreeEvent` into both the cactus journal *and* a
flat `Vec<TreeEvent>` (`self.tree_events` / `output.tree_events`), requiring a
`.clone()` for one of the two copies — e.g. weavy.rs:2158-2163 (reuse path,
also cloning `replayed_events` twice), weavy.rs:2714-2717 (Shift), 2787-2788
(Reduce, cloning the whole `reduction.tree_events` Vec into the journal too).
Since `TreeEvent` is entirely `Copy`-able scalar fields (ids, byte/point
ranges, bools — see `snark/src/parser.rs:5954+`), each individual clone is
cheap; the cost here is the double `Vec` allocation/storage, not deep-copy
cost. This matches the "known minor redundancy" already flagged in
`snark/docs/weavy-tree-journal.md`. Worth collapsing (keep only the journal,
derive the flat `Vec` via `collect()` once at the end, the same way
`accepted_tree_events()` already does) but it's additive/linear, not a
multiplier — fix only after 1–3 land, since 1–3 will change these call sites
anyway.

## 7. [LOW] One extra full clone of the final tree at parse-accept time

`snark/src/lower/weavy.rs:1703-1707`:

```rust
let Some((first_version, first_node, _, first_tree_events, first_reusable_nodes)) =
    best_accepted.first().map(|accepted| (**accepted).clone())
```

Clones the accepted branch's entire `SexpNode` tree, `Vec<TreeEvent>`, and
`Vec<RuntimeWeavyReusableNode>` (which themselves each own a tree + tree_events
per finding 3) once, to pull an owned value out of a shared reference. `O(n)`
once per parse — not a multiplier, low priority — but it doubles peak memory
at the exact moment the tree is largest. Once finding 3 makes
`reusable_nodes` lazy, this clone gets much cheaper for free.

## 8. [LOW] `RuntimeWeavyReport` retains `tree_store` alongside the already-flattened `tree`

`snark/src/lower/weavy.rs:1751-1762`: `RuntimeWeavyReport` keeps both `tree:
SexpNode` (the final flattened, fully-recursive tree) and `tree_store:
RuntimeWeavyTreeStore` (`Vec<SexpNode>`, one entry per node ever pushed during
the parse, each already containing full copies of its descendants per
finding 2). Once `tree` is materialized, `tree_store`'s entries are
duplicate weight — same content, retained for the report's whole lifetime,
not just transiently during parsing. `accepted_resolved_tree` (weavy.rs:1799-1809)
is the only consumer of `tree_store` on the accepted path, and it only reads
`.kind` per node (a `String` clone, finding-4-shaped) — it doesn't need the
full recursive subtrees stored in `tree_store`, only flat per-id kind lookup.
Once finding 2 is fixed (tree_store holds thin `TreeNodeId`-indexed nodes,
not embedded duplicates), this stops being a problem on its own; flagging
here mainly so the fix for 2 accounts for this consumer.

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
| 1 | `subtree_tree_events`/`event_refs` walks whole journal per reduce | O(n²) | Confirmed |
| 2 | `into_children` deep-clones child subtree per fold | O(n·depth) | Confirmed |
| 3 | `reusable_nodes` populated unconditionally (wraps 1+2) | drives 1+2 | Confirmed |
| 4 | Node-kind `String::to_owned()` per reduce | O(n) | Confirmed |
| 5 | Alias-name `String` clone per aliased step | O(aliased steps) | Confirmed |
| 6 | Double journal+flat `Vec<TreeEvent>` storage | O(n), cheap items | Confirmed |
| 7 | One extra full-tree clone at accept | O(n), once | Confirmed |
| 8 | `tree_store` retained as dead weight alongside `tree` | O(n·depth) retained | Confirmed, follows from #2 |
| 9 | Grammar-prep clones | O(grammar size), once | Confirmed, not hot per-parse |

**Recommended order of attack**: fix 3 first (make reuse bookkeeping opt-in/lazy) —
it's the smallest, most surgical change and it eliminates 1's O(n²) walk and
most of 2's clones for any caller that isn't doing incremental edits, which
almost certainly includes the JSON throughput bench. Then tackle 2 properly
(indirection through construction, single flatten pass) since it's the one
whose cost survives even after 3 is fixed (fold-in cloning happens regardless
of whether reuse bookkeeping exists). 4/5 (interning) are cheap, independent,
and can land any time. 6/7/8 are cleanup that mostly resolves itself once 2
is restructured.
