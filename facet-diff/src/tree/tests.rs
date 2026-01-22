use super::*;
use facet::Facet;
use facet_testhelpers::test;

#[derive(Debug, Clone, PartialEq, Facet)]
struct Person {
    name: String,
    age: u32,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct Container {
    items: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
struct Nested {
    name: String,
    children: Vec<Container>,
}

#[test]
fn test_identical_trees() {
    let a = Person {
        name: "Alice".into(),
        age: 30,
    };
    let b = a.clone();

    let ops = tree_diff(&a, &b);
    assert!(ops.is_empty(), "Identical trees should have no edits");
}

#[test]
fn test_simple_update() {
    let a = Person {
        name: "Alice".into(),
        age: 30,
    };
    let b = Person {
        name: "Alice".into(),
        age: 31,
    };

    let ops = tree_diff(&a, &b);
    assert!(!ops.is_empty(), "Changed values should have edits");
}

#[test]
fn test_tree_building() {
    let person = Person {
        name: "Alice".into(),
        age: 30,
    };

    let peek = Peek::new(&person);
    let tree = build_tree(peek);

    // Should have root + 2 fields (at minimum)
    let node_count = tree.arena.count();
    assert!(
        node_count >= 3,
        "Tree should have root and field nodes, got {}",
        node_count
    );
}

/// Test that tree building produces correct paths for list elements
#[test]
fn test_tree_paths_for_list() {
    let container = Container {
        items: vec!["a".into(), "b".into(), "c".into()],
    };

    let peek = Peek::new(&container);
    let tree = build_tree(peek);

    // Collect all paths from the tree
    let mut paths: Vec<Path> = Vec::new();
    for node in tree.arena.iter() {
        if let Some(label) = &node.get().label {
            paths.push(label.path.clone());
        }
    }

    debug!(?paths, "all paths in tree");

    // Should have:
    // - [] (root)
    // - [Field("items")] (the vec field)
    // - [Field("items"), Index(0)] (first element)
    // - [Field("items"), Index(1)] (second element)
    // - [Field("items"), Index(2)] (third element)
    assert!(
        paths.iter().any(|p| p.0.is_empty()),
        "Should have root path"
    );
    assert!(
        paths
            .iter()
            .any(|p| p.0 == vec![PathSegment::Field("items".into())]),
        "Should have items field path"
    );
    assert!(
        paths
            .iter()
            .any(|p| p.0 == vec![PathSegment::Field("items".into()), PathSegment::Index(0)]),
        "Should have items[0] path"
    );
    assert!(
        paths
            .iter()
            .any(|p| p.0 == vec![PathSegment::Field("items".into()), PathSegment::Index(2)]),
        "Should have items[2] path"
    );
}

/// Test that compute_adjusted_path correctly updates Index segments
#[test]
fn test_compute_adjusted_path_basic() {
    let container = Container {
        items: vec!["a".into(), "b".into(), "c".into()],
    };

    let peek = Peek::new(&container);
    let tree = build_tree(peek);

    // Find the node for items[1]
    let target_path = Path(vec![
        PathSegment::Field("items".into()),
        PathSegment::Index(1),
    ]);

    let mut target_node = None;
    for node_id in tree.root.descendants(&tree.arena) {
        if let Some(label) = &tree.arena.get(node_id).unwrap().get().label
            && label.path == target_path
        {
            target_node = Some(node_id);
            break;
        }
    }
    let target_node = target_node.expect("Should find items[1] node");

    // Without any modifications, the adjusted path should equal the original
    let adjusted = compute_adjusted_path(&tree.arena, tree.root, target_node, &target_path);
    assert_eq!(
        adjusted, target_path,
        "Unmodified tree should have unchanged paths"
    );
}

/// Test that after deleting an element, subsequent element paths shift
#[test]
fn test_path_adjustment_after_delete() {
    let container = Container {
        items: vec!["a".into(), "b".into(), "c".into()],
    };

    let peek = Peek::new(&container);
    let tree = build_tree(peek);

    // Clone the arena to simulate shadow tree
    let mut shadow_arena = tree.arena.clone();

    // Find the nodes
    let _items_path = Path(vec![PathSegment::Field("items".into())]);
    let item0_path = Path(vec![
        PathSegment::Field("items".into()),
        PathSegment::Index(0),
    ]);
    let item1_path = Path(vec![
        PathSegment::Field("items".into()),
        PathSegment::Index(1),
    ]);
    let item2_path = Path(vec![
        PathSegment::Field("items".into()),
        PathSegment::Index(2),
    ]);

    let mut item0_node = None;
    let mut item1_node = None;
    let mut item2_node = None;

    for node_id in tree.root.descendants(&tree.arena) {
        if let Some(label) = &tree.arena.get(node_id).unwrap().get().label {
            if label.path == item0_path {
                item0_node = Some(node_id);
            } else if label.path == item1_path {
                item1_node = Some(node_id);
            } else if label.path == item2_path {
                item2_node = Some(node_id);
            }
        }
    }

    let item0_node = item0_node.expect("Should find items[0]");
    let item1_node = item1_node.expect("Should find items[1]");
    let item2_node = item2_node.expect("Should find items[2]");

    // Delete item0 from shadow tree
    item0_node.remove(&mut shadow_arena);

    // Now item1 (originally at index 1) should be at index 0
    let adjusted1 = compute_adjusted_path(&shadow_arena, tree.root, item1_node, &item1_path);
    let expected1 = Path(vec![
        PathSegment::Field("items".into()),
        PathSegment::Index(0),
    ]);
    assert_eq!(
        adjusted1, expected1,
        "After deleting item[0], item[1] should become item[0]"
    );

    // And item2 (originally at index 2) should be at index 1
    let adjusted2 = compute_adjusted_path(&shadow_arena, tree.root, item2_node, &item2_path);
    let expected2 = Path(vec![
        PathSegment::Field("items".into()),
        PathSegment::Index(1),
    ]);
    assert_eq!(
        adjusted2, expected2,
        "After deleting item[0], item[2] should become item[1]"
    );
}

/// Test list element deletion produces some diff operations
#[test]
fn test_diff_list_delete() {
    let a = Container {
        items: vec!["a".into(), "b".into(), "c".into()],
    };
    let b = Container {
        items: vec!["a".into(), "c".into()], // removed "b"
    };

    let ops = tree_diff(&a, &b);
    debug!(?ops, "diff ops for list delete");

    // Should have some operations (the exact ops depend on cinereus's matching algorithm)
    // The algorithm may emit Delete, or Move+Delete, etc.
    assert!(!ops.is_empty(), "Should have some operations for deletion");

    // Should have at least one Delete or Move operation
    let has_structural_change = ops
        .iter()
        .any(|op| matches!(op, EditOp::Delete { .. } | EditOp::Move { .. }));
    assert!(
        has_structural_change,
        "Should have Delete or Move for structural change, got: {:?}",
        ops
    );
}

/// Test list element insertion produces operations that handle the change
#[test]
fn test_diff_list_insert() {
    let a = Container {
        items: vec!["a".into(), "c".into()],
    };
    let b = Container {
        items: vec!["a".into(), "b".into(), "c".into()], // inserted "b" at index 1
    };

    let ops = tree_diff(&a, &b);
    debug!(?ops, "diff ops for list insert");

    // The algorithm should produce an Insert somewhere in items
    // (The exact strategy may vary - e.g., update items[1] to "b" and insert "c" at items[2],
    // or insert "b" at items[1] directly)
    let has_insert_in_items = ops.iter().any(|op| {
        if let EditOp::Insert { path, .. } = op {
            path.0.first() == Some(&PathSegment::Field("items".into()))
        } else {
            false
        }
    });
    assert!(
        has_insert_in_items,
        "Should have Insert in items, got: {:?}",
        ops
    );
}

/// Test that nested structures produce diff operations
#[test]
fn test_nested_list_paths() {
    let a = Nested {
        name: "root".into(),
        children: vec![
            Container {
                items: vec!["a".into()],
            },
            Container {
                items: vec!["b".into()],
            },
        ],
    };
    let b = Nested {
        name: "root".into(),
        children: vec![
            Container {
                items: vec!["a".into()],
            },
            Container {
                items: vec!["modified".into()],
            }, // changed
        ],
    };

    let ops = tree_diff(&a, &b);
    debug!(?ops, "diff ops for nested change");

    // Should have some operations for the change
    assert!(!ops.is_empty(), "Should have operations for nested change");

    // At minimum, there should be something touching children
    let has_children_op = ops.iter().any(|op| {
        let path = match op {
            EditOp::Insert { path, .. } => Some(path),
            EditOp::Delete { node, .. } => match node {
                NodeRef::Path(p) => Some(p),
                NodeRef::Slot(..) => None,
            },
            EditOp::Move { to, .. } => match to {
                NodeRef::Path(p) => Some(p),
                NodeRef::Slot(..) => None,
            },
            EditOp::UpdateAttributes { path, .. } => Some(path),
            #[allow(unreachable_patterns)]
            _ => None,
        };
        path.is_some_and(|p| p.0.first() == Some(&PathSegment::Field("children".into())))
    });
    assert!(
        has_children_op,
        "Should have operation touching children, got: {:?}",
        ops
    );
}

// =========================================================================
// Tests for collect_properties (HTML attribute collection)
// =========================================================================

/// Simple struct with direct attribute fields
#[derive(Debug, Clone, PartialEq, Facet)]
struct SimpleElement {
    #[facet(attribute)]
    id: Option<String>,
    #[facet(attribute)]
    class: Option<String>,
    // Non-attribute field
    content: String,
}

/// Attrs struct that gets flattened (like GlobalAttrs in facet-html-dom)
#[derive(Debug, Clone, PartialEq, Default, Facet)]
struct Attrs {
    #[facet(attribute)]
    id: Option<String>,
    #[facet(attribute)]
    class: Option<String>,
    #[facet(attribute)]
    style: Option<String>,
}

/// Element with flattened attrs (mimics facet-html-dom structure)
#[derive(Debug, Clone, PartialEq, Facet)]
struct ElementWithFlattenedAttrs {
    #[facet(flatten)]
    attrs: Attrs,
    // Non-attribute field
    children: Vec<String>,
}

#[test]
fn test_collect_properties_direct_attrs() {
    let elem = SimpleElement {
        id: Some("my-id".into()),
        class: Some("my-class".into()),
        content: "hello".into(),
    };

    let peek = Peek::new(&elem);
    let builder = TreeBuilder::new();
    let props = builder.collect_properties(peek);

    // Should collect both attribute fields
    assert_eq!(
        props.get("id"),
        Some(&Some("my-id".to_string())),
        "Should collect id attribute"
    );
    assert_eq!(
        props.get("class"),
        Some(&Some("my-class".to_string())),
        "Should collect class attribute"
    );
    // Should NOT collect non-attribute field
    assert!(
        !props.contains("content"),
        "Should not collect non-attribute field"
    );
}

#[test]
fn test_collect_properties_none_values() {
    let elem = SimpleElement {
        id: None,
        class: Some("visible".into()),
        content: "hello".into(),
    };

    let peek = Peek::new(&elem);
    let builder = TreeBuilder::new();
    let props = builder.collect_properties(peek);

    // Should collect None as None
    assert_eq!(
        props.get("id"),
        Some(&None),
        "Should collect None attribute"
    );
    assert_eq!(
        props.get("class"),
        Some(&Some("visible".to_string())),
        "Should collect Some attribute"
    );
}

#[test]
fn test_collect_properties_flattened_attrs() {
    let elem = ElementWithFlattenedAttrs {
        attrs: Attrs {
            id: Some("my-id".into()),
            class: Some("my-class".into()),
            style: None,
        },
        children: vec!["child".into()],
    };

    let peek = Peek::new(&elem);
    let builder = TreeBuilder::new();
    let props = builder.collect_properties(peek);

    // Should collect attributes from flattened struct
    assert_eq!(
        props.get("id"),
        Some(&Some("my-id".to_string())),
        "Should collect id from flattened attrs"
    );
    assert_eq!(
        props.get("class"),
        Some(&Some("my-class".to_string())),
        "Should collect class from flattened attrs"
    );
    assert_eq!(
        props.get("style"),
        Some(&None),
        "Should collect style=None from flattened attrs"
    );
}

#[test]
fn test_diff_emits_update_attribute_for_flattened() {
    let a = ElementWithFlattenedAttrs {
        attrs: Attrs {
            id: Some("old-id".into()),
            class: None,
            style: None,
        },
        children: vec![],
    };
    let b = ElementWithFlattenedAttrs {
        attrs: Attrs {
            id: Some("new-id".into()),
            class: Some("added-class".into()),
            style: None,
        },
        children: vec![],
    };

    let ops = tree_diff(&a, &b);

    eprintln!("All ops: {:#?}", ops);

    // Should emit UpdateAttributes op with changes for both attributes
    let update_attrs_ops: Vec<_> = ops
        .iter()
        .filter(|op| matches!(op, EditOp::UpdateAttributes { .. }))
        .collect();

    assert_eq!(
        update_attrs_ops.len(),
        1,
        "Should have 1 UpdateAttributes op, got {:?}",
        update_attrs_ops
    );

    if let Some(EditOp::UpdateAttributes { changes, .. }) = update_attrs_ops.first() {
        let keys: Vec<_> = changes.iter().map(|c| c.key.clone()).collect();
        eprintln!("UpdateAttributes keys: {:?}", keys);

        assert!(
            keys.contains(&PropKey::Attr("id".into())),
            "Should have change for id, got: {:?}",
            keys
        );
        assert!(
            keys.contains(&PropKey::Attr("class".into())),
            "Should have change for class, got: {:?}",
            keys
        );
    }
}
