//! XML Diff Scenarios
//!
//! This test file defines concrete scenarios for XML diff rendering.
//! Each scenario shows:
//! 1. The old and new Rust values
//! 2. What the diff looks like
//! 3. What facet-xml currently renders (just the new value)
//! 4. What we WANT the diff-aware XML to look like (as a comment)

use facet::Facet;
use facet_diff::FacetDiff;
use facet_testhelpers::test;

// =============================================================================
// SVG-like types for testing
// =============================================================================

#[derive(Facet, Debug, Clone, PartialEq)]
struct Rect {
    fill: String,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

#[derive(Facet, Debug, Clone, PartialEq)]
struct Circle {
    id: String,
    cx: i32,
    cy: i32,
    r: i32,
}

#[derive(Facet, Debug, Clone, PartialEq)]
struct Path {
    id: String,
    d: String,
}

#[derive(Facet, Debug, Clone, PartialEq)]
struct Group {
    id: String,
    children: Vec<Element>,
}

#[derive(Facet, Debug, Clone, PartialEq)]
#[repr(u8)]
enum Element {
    Rect(Rect),
    Circle(Circle),
    Path(Path),
    Group(Group),
}

#[derive(Facet, Debug, Clone, PartialEq)]
struct Svg {
    view_box: String,
    children: Vec<Element>,
}

// =============================================================================
// Scenario 1: Single attribute change
// =============================================================================

#[test]
fn scenario_01_single_attr_change() {
    let old = Rect {
        fill: "red".into(),
        x: 10,
        y: 10,
        width: 50,
        height: 50,
    };
    let new = Rect {
        fill: "blue".into(),
        x: 10,
        y: 10,
        width: 50,
        height: 50,
    };

    let diff = old.diff(&new);
    println!("=== Scenario 1: Single attribute change ===");
    println!("Diff: {diff}");
    println!();

    // What we want:
    // <rect
    // - fill="red"
    // + fill="blue"
    //   x="10" y="10" width="50" height="50"
    // />
}

// =============================================================================
// Scenario 2: Multiple attribute changes
// =============================================================================

#[test]
fn scenario_02_multiple_attr_changes() {
    let old = Rect {
        fill: "red".into(),
        x: 10,
        y: 10,
        width: 50,
        height: 50,
    };
    let new = Rect {
        fill: "blue".into(),
        x: 20,
        y: 10,
        width: 50,
        height: 50,
    };

    let diff = old.diff(&new);
    println!("=== Scenario 2: Multiple attribute changes ===");
    println!("Diff: {diff}");
    println!();

    // What we want (aligned):
    // <rect
    // - fill="red"   x="10"
    // + fill="blue"  x="20"
    //   y="10" width="50" height="50"
    // />
}

// =============================================================================
// Scenario 3: Child element added (simple)
// =============================================================================

#[test]
fn scenario_03_child_added_simple() {
    let old = Svg {
        view_box: "0 0 100 100".into(),
        children: vec![Element::Rect(Rect {
            fill: "red".into(),
            x: 0,
            y: 0,
            width: 50,
            height: 50,
        })],
    };
    let new = Svg {
        view_box: "0 0 100 100".into(),
        children: vec![
            Element::Rect(Rect {
                fill: "red".into(),
                x: 0,
                y: 0,
                width: 50,
                height: 50,
            }),
            Element::Circle(Circle {
                id: "c1".into(),
                cx: 50,
                cy: 50,
                r: 25,
            }),
        ],
    };

    let diff = old.diff(&new);
    println!("=== Scenario 3: Child element added (simple) ===");
    println!("Diff: {diff}");
    println!();

    // What we want:
    // <svg view_box="0 0 100 100">
    //   <rect fill="red" x="0" y="0" width="50" height="50"/>
    // + <circle id="c1" cx="50" cy="50" r="25"/>
    // </svg>
}

// =============================================================================
// Scenario 4: Child element removed
// =============================================================================

#[test]
fn scenario_04_child_removed() {
    let old = Svg {
        view_box: "0 0 100 100".into(),
        children: vec![
            Element::Rect(Rect {
                fill: "red".into(),
                x: 0,
                y: 0,
                width: 50,
                height: 50,
            }),
            Element::Circle(Circle {
                id: "c1".into(),
                cx: 50,
                cy: 50,
                r: 25,
            }),
        ],
    };
    let new = Svg {
        view_box: "0 0 100 100".into(),
        children: vec![Element::Rect(Rect {
            fill: "red".into(),
            x: 0,
            y: 0,
            width: 50,
            height: 50,
        })],
    };

    let diff = old.diff(&new);
    println!("=== Scenario 4: Child element removed ===");
    println!("Diff: {diff}");
    println!();

    // What we want:
    // <svg view_box="0 0 100 100">
    //   <rect fill="red" x="0" y="0" width="50" height="50"/>
    // - <circle id="c1" cx="50" cy="50" r="25"/>
    // </svg>
}

// =============================================================================
// Scenario 5: Children reordered (swap)
// =============================================================================

#[test]
fn scenario_05_children_reordered() {
    let old = Svg {
        view_box: "0 0 100 100".into(),
        children: vec![
            Element::Rect(Rect {
                fill: "red".into(),
                x: 0,
                y: 0,
                width: 50,
                height: 50,
            }),
            Element::Circle(Circle {
                id: "c1".into(),
                cx: 50,
                cy: 50,
                r: 25,
            }),
        ],
    };
    let new = Svg {
        view_box: "0 0 100 100".into(),
        children: vec![
            Element::Circle(Circle {
                id: "c1".into(),
                cx: 50,
                cy: 50,
                r: 25,
            }),
            Element::Rect(Rect {
                fill: "red".into(),
                x: 0,
                y: 0,
                width: 50,
                height: 50,
            }),
        ],
    };

    let diff = old.diff(&new);
    println!("=== Scenario 5: Children reordered ===");
    println!("Diff: {diff}");
    println!();

    // What we want (moves shown with arrows):
    // <svg view_box="0 0 100 100">
    // → <circle id="c1" cx="50" cy="50" r="25"/>
    // → <rect fill="red" x="0" y="0" width="50" height="50"/>
    // </svg>
    //
    // Or maybe just show final order since both moved?
}

// =============================================================================
// Scenario 6: Element moved AND modified
// =============================================================================

#[test]
fn scenario_06_moved_and_modified() {
    let old = Svg {
        view_box: "0 0 100 100".into(),
        children: vec![
            Element::Rect(Rect {
                fill: "red".into(),
                x: 0,
                y: 0,
                width: 50,
                height: 50,
            }),
            Element::Circle(Circle {
                id: "c1".into(),
                cx: 50,
                cy: 50,
                r: 25,
            }),
        ],
    };
    let new = Svg {
        view_box: "0 0 100 100".into(),
        children: vec![
            Element::Circle(Circle {
                id: "c1".into(),
                cx: 50,
                cy: 50,
                r: 30, // changed!
            }),
            Element::Rect(Rect {
                fill: "red".into(),
                x: 0,
                y: 0,
                width: 50,
                height: 50,
            }),
        ],
    };

    let diff = old.diff(&new);
    println!("=== Scenario 6: Element moved AND modified ===");
    println!("Diff: {diff}");
    println!();

    // What we want:
    // <svg view_box="0 0 100 100">
    // ← <circle id="c1" cx="50" cy="50" r="25"/>  (old position, old value)
    // ...
    // → <circle id="c1" cx="50" cy="50" r="30"/>  (new position, new value)
    // </svg>
    //
    // Or with inline change:
    // → <circle id="c1" cx="50" cy="50"
    // -   r="25"
    // +   r="30"
    // />
}

// =============================================================================
// Scenario 7: Large element inserted
// =============================================================================

#[test]
fn scenario_07_large_element_inserted() {
    let old = Svg {
        view_box: "0 0 100 100".into(),
        children: vec![Element::Rect(Rect {
            fill: "red".into(),
            x: 0,
            y: 0,
            width: 50,
            height: 50,
        })],
    };
    let new = Svg {
        view_box: "0 0 100 100".into(),
        children: vec![
            Element::Rect(Rect {
                fill: "red".into(),
                x: 0,
                y: 0,
                width: 50,
                height: 50,
            }),
            Element::Group(Group {
                id: "layer1".into(),
                children: vec![
                    Element::Circle(Circle {
                        id: "c1".into(),
                        cx: 10,
                        cy: 10,
                        r: 5,
                    }),
                    Element::Circle(Circle {
                        id: "c2".into(),
                        cx: 20,
                        cy: 20,
                        r: 5,
                    }),
                    Element::Path(Path {
                        id: "p1".into(),
                        d: "M0 0 L100 100".into(),
                    }),
                ],
            }),
        ],
    };

    let diff = old.diff(&new);
    println!("=== Scenario 7: Large element inserted ===");
    println!("Diff: {diff}");
    println!();

    // What we want (all lines of inserted element get + prefix):
    // <svg view_box="0 0 100 100">
    //   <rect fill="red" x="0" y="0" width="50" height="50"/>
    // + <group id="layer1">
    // +   <circle id="c1" cx="10" cy="10" r="5"/>
    // +   <circle id="c2" cx="20" cy="20" r="5"/>
    // +   <path id="p1" d="M0 0 L100 100"/>
    // + </group>
    // </svg>
}

// =============================================================================
// Scenario 8: Deep nested change
// =============================================================================

#[test]
fn scenario_08_deep_nested_change() {
    let old = Svg {
        view_box: "0 0 100 100".into(),
        children: vec![Element::Group(Group {
            id: "layer1".into(),
            children: vec![Element::Group(Group {
                id: "shapes".into(),
                children: vec![Element::Rect(Rect {
                    fill: "red".into(),
                    x: 0,
                    y: 0,
                    width: 50,
                    height: 50,
                })],
            })],
        })],
    };
    let new = Svg {
        view_box: "0 0 100 100".into(),
        children: vec![Element::Group(Group {
            id: "layer1".into(),
            children: vec![Element::Group(Group {
                id: "shapes".into(),
                children: vec![Element::Rect(Rect {
                    fill: "blue".into(), // changed!
                    x: 0,
                    y: 0,
                    width: 50,
                    height: 50,
                })],
            })],
        })],
    };

    let diff = old.diff(&new);
    println!("=== Scenario 8: Deep nested change ===");
    println!("Diff: {diff}");
    println!();

    // What we want:
    // <svg view_box="0 0 100 100">
    //   <group id="layer1">
    //     <group id="shapes">
    //       <rect
    //       - fill="red"
    //       + fill="blue"
    //         x="0" y="0" width="50" height="50"
    //       />
    //     </group>
    //   </group>
    // </svg>
}
