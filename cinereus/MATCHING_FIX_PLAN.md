# Cinereus Matching Algorithm Fix Plan

## Problem Statement

When diffing `<div>` → `<div id="x">`, cinereus produces **51 operations** instead of **1** (Insert attr).

Root cause: The matching algorithm matches nodes **across tree levels** when they share the same `NodeKind`. For example, `Html.attrs` (at root) matches with `Div.attrs` (nested) because both are empty attrs structs with identical hashes.

## Current State

- **Branch (facet)**: `shadow-tree-path-tracking`
- **Branch (dodeca)**: `main`
- **Test command**: `RUST_LOG=debug cargo nextest run --package html-diff-tests --features tracing add_attribute`
- **Result**: 23/27 tests pass, 4 fail (all attribute-related)
- **Issue**: https://github.com/facet-rs/facet/issues/1835

## Research Summary

### Paper 1: "Beyond GumTree" (Matsumoto et al.)
- Confirms GumTree's matching is imprecise: "27% of diffs not optimal"
- Their fix: Use line-diff to constrain matching (nodes in non-edited lines only match with nodes in non-edited lines)
- Key insight: **Positional constraints prevent cross-level matching**

### Paper 2: "Improving Pattern Tracking" (Palix, Falleri, Lawall - includes GumTree author)
- **Authoritative GumTree description**:
  1. **Top-down**: Find isomorphic subtrees by hash → "anchor mappings"
  2. **Bottom-up**: Match "containers" if descendants have many anchor mappings → "container mappings"
  3. **Recovery**: Expensive TED algorithm for additional mappings
- Key insight: Top-down should naturally maintain ancestry because you traverse from roots down
- **The `b_by_hash` global lookup breaks this** - it allows jumping to arbitrary tree locations

### Paper 3: "Scalable Structural Code Diffs" (van Seventer, 2025)
- Compares **GumTree Greedy** (original) vs **GumTree Simple** (faster)
- Simple uses **99% fewer CPU cycles** with similar quality
- **GumTree Simple's Recovery Phase** (3 sub-phases):
  1. **Exact Isomorphism**: Match children with identical structure AND labels, use LCS
  2. **Structural Isomorphism**: Ignore leaf labels, match by structure (catches renames)
  3. **Type Matching**: If node type appears **only once** among children of both parents → match them

## The Bugs in cinereus

### Bug 1: Global hash lookup in top_down_phase (matching.rs:254-260)

```rust
// BUG: Looks up ANY node in tree B with matching hash
if let Some(b_candidates) = b_by_hash.get(&a_child_data.hash) {
    for &b_candidate in b_candidates {
        candidates.push((a_child, b_candidate));  // Can jump anywhere!
    }
}
```

**Fix**: Remove global lookup. Only consider children of the current B node as candidates.

### Bug 2: No ancestry constraint in bottom_up_phase (matching.rs:403-434)

Candidates are selected purely by `NodeKind`:
```rust
let candidates = b_by_kind.get(&a_data.kind).cloned().unwrap_or_default();
```

Two attrs at different tree levels match because they're both `Struct("DivAttrs")`.

**Fix**: Add ancestry check - if A's parent is matched to P_b, then B must be a descendant of P_b.

### Bug 3: Missing ALIGN phase in chawathe.rs

The comment claims 5 phases but ALIGN is not implemented:
```rust
//! 2. ALIGN: Reorder children to match destination order  // <-- MISSING
```

Real Chawathe uses LCS to minimize moves. Current code emits MOVE for ANY position change.

**Fix**: Implement LCS-based child alignment.

## Implementation Plan

### Phase 1: Fix top_down_phase (HIGH PRIORITY)

File: `cinereus/src/matching.rs`

**Change**: Remove the `b_by_hash` global lookup. When exploring children of `(a_id, b_id)`, only consider:
- Children of `b_id` with matching kind
- NOT arbitrary nodes from anywhere in tree B

```rust
// BEFORE (buggy):
for a_child in tree_a.children(a_id) {
    // Global lookup - can match anywhere!
    if let Some(b_candidates) = b_by_hash.get(&a_child_data.hash) {
        for &b_candidate in b_candidates {
            candidates.push((a_child, b_candidate));
        }
    }
    // Also children - this part is fine
    for b_child in tree_b.children(b_id) { ... }
}

// AFTER (fixed):
for a_child in tree_a.children(a_id) {
    let a_child_data = tree_a.get(a_child);
    // ONLY consider children of b_id
    for b_child in tree_b.children(b_id) {
        if !matching.contains_b(b_child) {
            let b_child_data = tree_b.get(b_child);
            // Match by hash (exact) or kind (structural)
            if a_child_data.hash == b_child_data.hash
               || a_child_data.kind == b_child_data.kind {
                candidates.push((a_child, b_child));
            }
        }
    }
}
```

### Phase 2: Add ancestry constraint to bottom_up_phase

File: `cinereus/src/matching.rs`

Already partially implemented in the `ancestry_compatible()` function. The issue is that bottom-up processes in post-order (children before parents), so parent matches don't exist yet when checking children.

**Better approach**: Use the "unique type among children" heuristic from GumTree Simple:
- If a node type appears only once among unmatched children of both matched parents, match them
- This naturally prevents cross-level matching

### Phase 3: Implement ALIGN phase with LCS

File: `cinereus/src/chawathe.rs`

**Current behavior**: Emits MOVE for any position change, even if just shifted by an insertion.

**Fix**:
1. After matching children, compute LCS of matched children positions
2. Only emit MOVE for children not in the LCS
3. This minimizes spurious moves when siblings shift due to insertions

```rust
// Pseudocode for ALIGN
fn align_children(parent_a: NodeId, parent_b: NodeId, matching: &Matching) {
    let children_a: Vec<NodeId> = tree_a.children(parent_a).collect();
    let children_b: Vec<NodeId> = tree_b.children(parent_b).collect();

    // Get matched pairs in order
    let matched_in_a: Vec<(usize, NodeId)> = children_a.iter()
        .enumerate()
        .filter_map(|(i, &a)| matching.get_b(a).map(|b| (i, b)))
        .collect();

    let matched_in_b: Vec<(usize, NodeId)> = children_b.iter()
        .enumerate()
        .filter_map(|(i, &b)| matching.get_a(b).map(|_| (i, b)))
        .collect();

    // Compute LCS - children in LCS don't need MOVE
    let lcs = longest_common_subsequence(&matched_in_a, &matched_in_b);

    // Only MOVE children not in LCS
    for (pos_a, node_b) in matched_in_a {
        if !lcs.contains(&(pos_a, node_b)) {
            emit_move(node_b, parent_b, correct_position);
        }
    }
}
```

### Phase 4: Consider GumTree Simple's recovery approach (OPTIONAL)

If quality is still insufficient after phases 1-3, implement Simple's 3-phase recovery:

1. **Exact Isomorphism**: Already done by top-down
2. **Structural Isomorphism**: Match by structure ignoring leaf labels (for detecting updates)
3. **Type Matching**: Unique type among children → automatic match

## Files to Modify

1. **`cinereus/src/matching.rs`**
   - `top_down_phase()`: Remove `b_by_hash` global lookup
   - `bottom_up_phase()`: Use ancestry constraint or unique-type heuristic

2. **`cinereus/src/chawathe.rs`**
   - Add ALIGN phase with LCS before MOVE phase

3. **`cinereus/src/lcs.rs`** (new file)
   - Implement longest common subsequence algorithm

## Success Criteria

1. `<div>` → `<div id="x">` produces **1 op** (Insert attr), not 51
2. All 27 html-diff-tests pass
3. html-diff-tests translation layer is simple (no complex deduplication needed)

## What NOT to Do

- Don't add complexity to html-diff-tests translation layer
- Don't require `Debug` bounds on cinereus generics
- Don't try to filter/deduplicate bad ops in facet-diff - fix the source

## Testing

```bash
# Run single failing test with debug output
cd /Users/amos/bearcove/dodeca
RUST_LOG=debug cargo nextest run --package html-diff-tests --features tracing add_attribute

# Run all html-diff-tests
cargo nextest run --package html-diff-tests

# Run cinereus unit tests
cd /Users/amos/bearcove/facet
cargo nextest run --package cinereus
```

## References

- GumTree paper: Falleri et al., "Fine-grained and accurate source code differencing", ASE 2014
- Chawathe paper: "Change detection in hierarchically structured information", SIGMOD 1996
- GumTree Simple: Falleri & Martinez, "Fine-grained, accurate and scalable source differencing", ICSE 2024
- Issue: https://github.com/facet-rs/facet/issues/1835
