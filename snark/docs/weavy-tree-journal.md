# The Weavy tree journal: a cactus stack, and the memory design space

This note captures *why* the Weavy runtime stores tree events the way it does, and
— more importantly — what the real design space is, so nobody mistakes the current
implementation's incidental costs for fundamental ones.

## The problem it solves

GLR parsing forks a branch at every parse conflict. The older branch-local
event model gave each branch its own `Vec<TreeEvent>` holding the **entire event
history so far**, and cloned that vector on every fork. Because the history
grows with input position, this is roughly `O(forks × position)` of deep
copying.

A `stax flame` of a recovering gingembre parse (`blog-index.html`, 816 bytes, ~50 s)
put ~85% of the time in `Vec::clone` + `RuntimeBranch::clone` + `subtree_tree_events`.
The parse table build was ~4%. The blowup was *copying the growing event stream*, not
parsing.

## The structure: a cactus stack (aka spaghetti stack / parent-pointer tree)

In `snark/src/lower/weavy.rs`:

```rust
struct RuntimeWeavyBranch { tree_journal: RuntimeWeavyTreeJournalHead, /* … */ }

#[derive(Clone, Copy)]
struct RuntimeWeavyTreeJournalHead(Option<usize>);          // a branch's tip: just an index

struct RuntimeWeavyTreeJournalEntry { parent: RuntimeWeavyTreeJournalHead, event: TreeEvent }

struct RuntimeWeavyTreeJournal { entries: Vec<RuntimeWeavyTreeJournalEntry> }  // one shared pool
```

There is **one** journal for the whole parse. Each entry parent-points to the
previous one, so the entries form a tree. A branch is just a `Copy` index (its tip).

- **Fork a branch** = copy the `Option<usize>` tip. `O(1)`. Both branches share the
  entire trunk below the split; nothing is copied.
- **Append** = push one entry whose `parent` is the current tip, advance the tip.
- **Read a branch's events** (`collect` / `event_refs`, e.g. in `accepted_tree_events()`)
  = walk parent-pointers tip→root, reverse. Done on demand, not maintained.

This is the same trick GLR already uses for the *parse stack* (the Graph-Structured
Stack); the journal applies it to the event *output*, which is where the older
branch-local model paid the clone cost.

## The memory tradeoff (and what is NOT fundamental)

The pool is **append-only** today: a discarded branch just drops its tip, leaving its
entries orphaned in `entries` until the whole `RuntimeWeavyTreeJournal` is dropped at
end of parse. So peak memory ≈ total events ever appended (live + dead).

For a single parse this is fine: it's transient, bounded by the work, and GLR prunes
most speculative branches within a few tokens, so dead-branch garbage is usually a
small constant over the accepted lineage. What you keep *past* the parse is the
accepted lineage (`accepted_tree_events()` walks only the accepted tip), not the pool.

**None of the costs are laws.** We own the allocator and the layout. The real space:

- **Pre-allocation** — already have it. The pool is a `Vec`; pushes are amortized
  `O(1)` into pre-grown capacity, no per-node malloc. (So the "`im::Vector` would cost
  `log(n)` + an allocation per push" framing is about an off-the-shelf default, not
  about us.)
- **Branch/node reuse (free list)** — append-only is the *current* simple impl, not a
  property of the approach. A free list on the pool lets a discarded branch return its
  exclusive tail slots, reclaiming dead nodes mid-parse in the same arena, with none of
  `im`'s machinery. "We still hold the entire vector" is a free-list away from false.
- **Ropes** — if we want persistence *and* reclamation *and* locality at once: a
  chunked, balanced, shared tree. Fork shares subtrees, discard drops a subtree, chunks
  stay cache-friendly, per-element cost amortizes across the chunk.
- **Compaction** — at accept or at edit boundaries, materialize the live lineage into a
  tight form and drop the rest.

Pick your sharing, your reclamation, your locality, and build the structure with all
three. The cactus-vs-`im` binary is false.

## Discipline

The time fire is out (cheap fork). The current pool is pre-allocated and amortized
`O(1)`. The only open memory axis is **mid-parse / cross-edit reclamation**, and it has
cheap answers ready (free list, compact-at-accept). Reach for the rope only when a heap
profile asks for it — `stax` and a heap profile, same method that found the time blowup.

## Answer (verified): the session compacts — the pool is never retained

The append-only pool is **per-parse transient**. `RuntimeWeavyTreeJournal` is a *local*
in the parse driver (`parse_prepared_weavy_*`), used to accumulate events cheaply
during GLR forking. At accept, the accepted branch's lineage is materialized into a
`Vec<TreeEvent>` (the cactus walk, `collect`), and the pool is dropped when the parse
function returns. It never enters the report.

`WeavyParseSession` retains only `last_report: Option<WeavyParseReport>`, and the
report holds:

- `tree_events: Vec<TreeEvent>` — the materialized **accepted lineage** (no dead branches),
- `tree_store: RuntimeWeavyTreeStore` — handle-based accepted nodes/child lists used to
  avoid reduce-time subtree copies, and
- `reusable_nodes: Vec<RuntimeWeavyReusableNode>` — populated only when reuse collection is
  requested, each carrying the compact payload needed for incremental replay.

So nothing speculative survives a parse. **Materialization-at-accept *is* the compaction**;
the "if it's the full pool, fix is compact-at-accept" branch above does not apply, because
it already compacts. The only memory held across an edit is the accepted tree, bounded by
input size — not the speculative garbage. There is no slow session leak.

### Reuse payloads are opt-in

Reusable nodes still store the compact subtree/event payload needed to replay a subtree
across an edit, but the parser only builds that payload when the caller asks for
incremental reuse. From-scratch report parses leave reuse collection disabled, so large
flat repeats and deep nesting build by tree-store handle and materialize the final
S-expression once at accept.
