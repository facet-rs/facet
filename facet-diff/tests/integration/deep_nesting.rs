//! Tests for deeply nested structures that could cause performance issues

use facet::Facet;
use facet_diff::FacetDiff;

/// Create a deeply nested structure
fn create_nested_tree(depth: usize, width: usize) -> Tree {
    if depth == 0 {
        Tree {
            value: 0,
            children: vec![],
        }
    } else {
        Tree {
            value: depth,
            children: (0..width)
                .map(|_| create_nested_tree(depth - 1, width))
                .collect(),
        }
    }
}

#[derive(Facet, Clone)]
struct Tree {
    value: usize,
    children: Vec<Tree>,
}

#[test]
fn test_shallow_wide_tree() {
    // Shallow but wide - should be fast
    let a = create_nested_tree(2, 5);
    let b = create_nested_tree(2, 5);

    let _diff = a.diff(&b);
    // Should complete quickly
}

#[test]
fn test_moderate_depth() {
    // Moderate depth - should still be reasonable
    let a = create_nested_tree(4, 2);
    let b = create_nested_tree(4, 2);

    let _diff = a.diff(&b);
    // Should complete in reasonable time
}

#[test]
#[ignore] // Already slow even at depth 5
fn test_depth_5() {
    let a = create_nested_tree(5, 3);
    let b = create_nested_tree(5, 3);

    let _diff = a.diff(&b);
}

#[test]
#[ignore] // This will hang without depth limit
fn test_deep_nesting() {
    // Deep nesting - this is what facet-shapelike might hit
    let a = create_nested_tree(10, 5);
    let b = create_nested_tree(10, 5);

    let _diff = a.diff(&b);
    // This currently hangs
}

#[test]
#[ignore] // This will hang without depth limit
fn test_pathological_diff() {
    // Completely different deep trees - worst case for Myers' algorithm
    let a = create_nested_tree(8, 4);
    let mut b = create_nested_tree(8, 4);

    // Make them completely different at every level
    fn mutate_tree(tree: &mut Tree) {
        tree.value = tree.value.wrapping_add(1000);
        for child in &mut tree.children {
            mutate_tree(child);
        }
    }
    mutate_tree(&mut b);

    let _diff = a.diff(&b);
    // This is the worst case - every element differs
}

#[test]
#[cfg_attr(miri, ignore)]
fn test_single_difference_in_deep_tree() {
    // Deep tree with only one value different
    let a = create_nested_tree(8, 3);
    let mut b = a.clone();

    // Change just one value deep in the tree
    if let Some(level1) = b.children.get_mut(0)
        && let Some(level2) = level1.children.get_mut(0)
        && let Some(level3) = level2.children.get_mut(0)
    {
        level3.value = 999;
    }

    let _diff = a.diff(&b);
    // Should find the single difference efficiently
}

#[test]
fn test_vec_of_different_lengths() {
    // Testing sequence diffing with very different lengths
    let a = Tree {
        value: 1,
        children: (0..100)
            .map(|i| Tree {
                value: i,
                children: vec![],
            })
            .collect(),
    };
    let b = Tree {
        value: 1,
        children: (0..200)
            .map(|i| Tree {
                value: i,
                children: vec![],
            })
            .collect(),
    };

    let _diff = a.diff(&b);
    // Sequence diff should handle this reasonably
}
