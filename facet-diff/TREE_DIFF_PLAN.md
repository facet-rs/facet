# GumTree-style Tree Diff for facet-diff

## Problem Statement

Current facet-diff limitations (demonstrated in `examples/tree_diff_showcase.rs`):

1. **Swap two children**: Shows as delete+insert, not a move/reorder
2. **Delete middle child**: Confuses indices, shows wrong transformations
3. **Add middle child**: Works but could be smarter about matching
4. **Move between groups**: Shows as delete+insert, not detected as a move

## Proposed Solution: GumTree Algorithm

Based on the [GumTree paper](https://hal.science/hal-01054552/document) (ICSE 2014, updated 2024).

### Phase 1: Bottom-Up Hashing (Merkle Tree)

Compute a hash for every node in both trees:

```
hash(node) = hash(type_label || own_attributes || hash(child_0) || hash(child_1) || ...)
```

**Key insight**: Identical subtrees get identical hashes → O(1) to detect "unchanged".

**Implementation options**:

| Option | Pros | Cons |
|--------|------|------|
| A. Use vtable `hash` | Already implemented, respects type's Hash impl | Only works for types with Hash, opaque to structure |
| B. Structural hash via Peek | Works for all Facet types, sees structure | Need to implement, may differ from type's Hash |
| C. Hybrid | Best of both | More complex |

**Recommendation**: Option B (structural hash) because:
- We need to hash *structure*, not just value
- A `Rect { x: "10", y: "20" }` should hash the same whether it comes from type A or B
- We need recursive hashing of children, vtable hash doesn't do that

**Parallelization**: Hash nodes level-by-level from leaves up. Within each level, all nodes can be hashed in parallel (rayon).

### Phase 2: Top-Down Matching

Match nodes with identical hashes (identical subtrees):

```rust
fn top_down_match(tree_a: &Tree, tree_b: &Tree) -> Matches {
    let mut matches = Matches::new();
    let mut queue = PriorityQueue::new(); // by height, descending

    queue.push((tree_a.root, tree_b.root));

    while let Some((a, b)) = queue.pop() {
        if hash(a) == hash(b) && !matches.contains(a) && !matches.contains(b) {
            matches.add(a, b);
            // Also match all descendants (they must be equal)
            for (child_a, child_b) in zip(a.children, b.children) {
                matches.add_recursive(child_a, child_b);
            }
        } else {
            // Hashes differ, try matching children
            for child_a in a.children {
                for child_b in b.children {
                    if compatible_types(child_a, child_b) {
                        queue.push((child_a, child_b));
                    }
                }
            }
        }
    }

    matches
}
```

### Phase 3: Bottom-Up Matching

For unmatched nodes, find best matches using similarity:

```rust
fn bottom_up_match(tree_a: &Tree, tree_b: &Tree, matches: &mut Matches) {
    // Post-order traversal of tree_a
    for node_a in tree_a.post_order() {
        if matches.contains(node_a) {
            continue;
        }

        // Find candidates in tree_b with same type
        let candidates: Vec<_> = tree_b.nodes()
            .filter(|b| same_type(node_a, b) && !matches.contains(b))
            .collect();

        // Score by dice coefficient of matched descendants
        let best = candidates.iter()
            .map(|b| (b, dice_coefficient(node_a, b, matches)))
            .filter(|(_, score)| *score > THRESHOLD)
            .max_by_key(|(_, score)| *score);

        if let Some((node_b, _)) = best {
            matches.add(node_a, node_b);
        }
    }
}

fn dice_coefficient(a: &Node, b: &Node, matches: &Matches) -> f64 {
    let desc_a: HashSet<_> = a.descendants().collect();
    let desc_b: HashSet<_> = b.descendants().collect();
    let common = desc_a.iter()
        .filter(|d| matches.get(*d).map(|m| desc_b.contains(m)).unwrap_or(false))
        .count();
    2.0 * common as f64 / (desc_a.len() + desc_b.len()) as f64
}
```

### Phase 4: Edit Script Generation (Chawathe)

Convert matches to edit operations:

```rust
enum EditOp {
    Update { node: NodeId, field: String, old: Value, new: Value },
    Insert { node: NodeId, parent: NodeId, position: usize },
    Delete { node: NodeId },
    Move { node: NodeId, new_parent: NodeId, new_position: usize },
}
```

The Chawathe algorithm:
1. For each matched pair where labels differ → `Update`
2. For each unmatched node in tree_b → `Insert`
3. For each unmatched node in tree_a → `Delete`
4. For each matched pair where parent/position differs → `Move`

## Integration with facet-diff

### New Types

```rust
/// A node in the tree representation
struct TreeNode<'mem, 'facet> {
    peek: Peek<'mem, 'facet>,
    hash: u64,
    height: usize,
    children: Vec<TreeNodeId>,
    parent: Option<TreeNodeId>,
}

/// Structural hash that recursively hashes children
fn structural_hash(peek: Peek) -> u64 {
    let mut hasher = DefaultHasher::new();

    // Hash the type/shape
    peek.shape().hash(&mut hasher);

    // Hash based on kind
    match peek.innermost_peek() {
        // Scalars: use their value
        InnerPeek::Scalar(s) => s.hash(&mut hasher),

        // Structs: hash field names + recursive child hashes
        InnerPeek::Struct(s) => {
            for (name, child) in s.fields() {
                name.hash(&mut hasher);
                structural_hash(child).hash(&mut hasher);
            }
        }

        // Sequences: hash child hashes in order
        InnerPeek::List(l) => {
            for child in l.iter() {
                structural_hash(child).hash(&mut hasher);
            }
        }

        // Enums: hash variant name + variant value hash
        InnerPeek::Enum(e) => {
            e.variant_name().hash(&mut hasher);
            structural_hash(e.value()).hash(&mut hasher);
        }
    }

    hasher.finish()
}
```

### API Changes

```rust
// New diff output types
pub enum TreeDiff<'mem, 'facet> {
    Equal { value: Peek<'mem, 'facet> },
    Update { path: Path, old: Peek<'mem, 'facet>, new: Peek<'mem, 'facet> },
    Insert { path: Path, value: Peek<'mem, 'facet> },
    Delete { path: Path, value: Peek<'mem, 'facet> },
    Move { old_path: Path, new_path: Path, value: Peek<'mem, 'facet> },
}

// Compute tree diff
pub fn tree_diff<'a, 'f, A: Facet<'f>, B: Facet<'f>>(
    a: &'a A,
    b: &'a B
) -> Vec<TreeDiff<'a, 'f>>;
```

## Implementation Order

1. **Structural hashing** - `TreeNode` and `structural_hash()`
2. **Tree building** - Convert `Peek` to tree representation
3. **Top-down matching** - Hash-based identical subtree matching
4. **Bottom-up matching** - Similarity-based matching for remaining nodes
5. **Edit script** - Generate `TreeDiff` operations
6. **Display** - Pretty-print the diff with move detection

## Questions to Resolve

1. **Hashing**: Use existing vtable `hash` or structural hash?
   - Structural hash is more appropriate for diffing (sees structure)
   - But vtable hash respects custom Hash impls

2. **Thresholds**: What similarity threshold for bottom-up matching?
   - GumTree uses 0.5 by default

3. **Move detection scope**: Detect moves only within same parent, or globally?
   - Global is more powerful but more expensive

4. **Parallel hashing**: Use rayon for parallel bottom-up hashing?
   - Yes, this is embarrassingly parallel
