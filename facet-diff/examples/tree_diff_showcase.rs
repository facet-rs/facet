//! Tree Diff Showcase
//!
//! Demonstrates how the current diff algorithm handles tree-like structures.
//! This showcases the limitations we want to improve with a Merkle-tree based approach.
//!
//! Run with: cargo run -p facet-diff --example tree_diff_showcase

use facet::Facet;
use facet_diff::FacetDiff;
use owo_colors::OwoColorize;

// ============================================================================
// SVG-like tree structure for demonstration
// ============================================================================

#[derive(Facet, Debug, Clone, PartialEq)]
struct Svg {
    width: String,
    height: String,
    children: Vec<SvgElement>,
}

#[derive(Facet, Debug, Clone, PartialEq)]
#[repr(C)]
enum SvgElement {
    Rect(SvgRect),
    Circle(SvgCircle),
    Group(SvgGroup),
}

#[derive(Facet, Debug, Clone, PartialEq)]
struct SvgRect {
    x: String,
    y: String,
    width: String,
    height: String,
    fill: String,
}

#[derive(Facet, Debug, Clone, PartialEq)]
struct SvgCircle {
    cx: String,
    cy: String,
    r: String,
    fill: String,
}

#[derive(Facet, Debug, Clone, PartialEq)]
struct SvgGroup {
    id: String,
    children: Vec<SvgElement>,
}

// ============================================================================
// Test scenarios
// ============================================================================

fn check_hash<T: Facet<'static>>(name: &str) {
    let shape = T::SHAPE;
    let has_hash = shape.is_hash();
    let status = if has_hash { "YES" } else { "NO" };
    let colored = if has_hash {
        status.green().to_string()
    } else {
        status.red().to_string()
    };
    println!("  {:20} Hash: {}", name, colored);
}

fn main() {
    println!("{}", "═".repeat(80).dimmed());
    println!(
        "{} {}",
        "TREE DIFF SHOWCASE".bold().cyan(),
        "- Demonstrating Current Limitations".dimmed()
    );
    println!("{}", "═".repeat(80).dimmed());
    println!();

    // First, show which types have Hash support
    println!("{}", "HASH SUPPORT CHECK".bold().yellow());
    println!(
        "{}",
        "Checking which SVG types have vtable.hash filled in:".dimmed()
    );
    println!();
    check_hash::<String>("String");
    check_hash::<i32>("i32");
    check_hash::<bool>("bool");
    println!("  {}", "---".dimmed());
    check_hash::<Svg>("Svg");
    check_hash::<SvgElement>("SvgElement");
    check_hash::<SvgRect>("SvgRect");
    check_hash::<SvgCircle>("SvgCircle");
    check_hash::<SvgGroup>("SvgGroup");
    check_hash::<Vec<SvgElement>>("Vec<SvgElement>");
    println!();
    println!(
        "{}",
        "Conclusion: Custom structs/enums don't have Hash - we need structural hashing!".yellow()
    );
    println!();

    println!(
        "{}",
        "This showcase demonstrates how facet-diff currently handles tree mutations.".dimmed()
    );
    println!(
        "{}",
        "The goal is to identify areas for improvement with Merkle-tree based diffing.".dimmed()
    );
    println!();

    // Scenario 1: Change a deep attribute
    scenario_deep_attribute_change();

    // Scenario 2: Swap two children
    scenario_swap_children();

    // Scenario 3: Delete a child
    scenario_delete_child();

    // Scenario 4: Add a child
    scenario_add_child();

    // Scenario 5: Move a child (delete + add elsewhere)
    scenario_move_child();

    // Scenario 6: Nested group modifications
    scenario_nested_modification();

    println!("{}", "═".repeat(80).dimmed());
    println!("{}", "END OF SHOWCASE".bold().cyan());
    println!("{}", "═".repeat(80).dimmed());
}

fn print_scenario(name: &str, description: &str, before: &Svg, after: &Svg) {
    println!("{}", "─".repeat(80).dimmed());
    println!("{} {}", "SCENARIO:".bold().yellow(), name.bold().white());
    println!("{}", description.dimmed());
    println!("{}", "─".repeat(80).dimmed());
    println!();

    let diff = before.diff(after);
    println!("{}", "Diff output:".bold());
    println!("{diff}");
    println!();
}

fn scenario_deep_attribute_change() {
    let before = Svg {
        width: "100".into(),
        height: "100".into(),
        children: vec![SvgElement::Group(SvgGroup {
            id: "layer1".into(),
            children: vec![
                SvgElement::Rect(SvgRect {
                    x: "10".into(),
                    y: "10".into(),
                    width: "50".into(),
                    height: "50".into(),
                    fill: "red".into(), // <-- This changes
                }),
                SvgElement::Circle(SvgCircle {
                    cx: "75".into(),
                    cy: "75".into(),
                    r: "20".into(),
                    fill: "blue".into(),
                }),
            ],
        })],
    };

    let mut after = before.clone();
    // Change fill from "red" to "green" deep in the tree
    if let SvgElement::Group(ref mut g) = after.children[0]
        && let SvgElement::Rect(ref mut r) = g.children[0]
    {
        r.fill = "green".into();
    }

    print_scenario(
        "Deep Attribute Change",
        "Change a single attribute (fill: red → green) deep in a nested group.",
        &before,
        &after,
    );
}

fn scenario_swap_children() {
    let before = Svg {
        width: "100".into(),
        height: "100".into(),
        children: vec![
            SvgElement::Rect(SvgRect {
                x: "10".into(),
                y: "10".into(),
                width: "30".into(),
                height: "30".into(),
                fill: "red".into(),
            }),
            SvgElement::Circle(SvgCircle {
                cx: "70".into(),
                cy: "70".into(),
                r: "25".into(),
                fill: "blue".into(),
            }),
        ],
    };

    // Swap the order of children
    let after = Svg {
        width: "100".into(),
        height: "100".into(),
        children: vec![
            SvgElement::Circle(SvgCircle {
                cx: "70".into(),
                cy: "70".into(),
                r: "25".into(),
                fill: "blue".into(),
            }),
            SvgElement::Rect(SvgRect {
                x: "10".into(),
                y: "10".into(),
                width: "30".into(),
                height: "30".into(),
                fill: "red".into(),
            }),
        ],
    };

    print_scenario(
        "Swap Two Children",
        "Swap the order of rect and circle elements. Ideally shows as a reorder, not delete+insert.",
        &before,
        &after,
    );
}

fn scenario_delete_child() {
    let before = Svg {
        width: "100".into(),
        height: "100".into(),
        children: vec![
            SvgElement::Rect(SvgRect {
                x: "10".into(),
                y: "10".into(),
                width: "30".into(),
                height: "30".into(),
                fill: "red".into(),
            }),
            SvgElement::Circle(SvgCircle {
                cx: "50".into(),
                cy: "50".into(),
                r: "15".into(),
                fill: "green".into(),
            }),
            SvgElement::Rect(SvgRect {
                x: "70".into(),
                y: "70".into(),
                width: "20".into(),
                height: "20".into(),
                fill: "blue".into(),
            }),
        ],
    };

    // Delete the middle child (circle)
    let after = Svg {
        width: "100".into(),
        height: "100".into(),
        children: vec![
            SvgElement::Rect(SvgRect {
                x: "10".into(),
                y: "10".into(),
                width: "30".into(),
                height: "30".into(),
                fill: "red".into(),
            }),
            SvgElement::Rect(SvgRect {
                x: "70".into(),
                y: "70".into(),
                width: "20".into(),
                height: "20".into(),
                fill: "blue".into(),
            }),
        ],
    };

    print_scenario(
        "Delete a Child",
        "Remove the middle element (circle) from a list of three elements.",
        &before,
        &after,
    );
}

fn scenario_add_child() {
    let before = Svg {
        width: "100".into(),
        height: "100".into(),
        children: vec![
            SvgElement::Rect(SvgRect {
                x: "10".into(),
                y: "10".into(),
                width: "30".into(),
                height: "30".into(),
                fill: "red".into(),
            }),
            SvgElement::Rect(SvgRect {
                x: "70".into(),
                y: "70".into(),
                width: "20".into(),
                height: "20".into(),
                fill: "blue".into(),
            }),
        ],
    };

    // Add a circle in the middle
    let after = Svg {
        width: "100".into(),
        height: "100".into(),
        children: vec![
            SvgElement::Rect(SvgRect {
                x: "10".into(),
                y: "10".into(),
                width: "30".into(),
                height: "30".into(),
                fill: "red".into(),
            }),
            SvgElement::Circle(SvgCircle {
                cx: "50".into(),
                cy: "50".into(),
                r: "15".into(),
                fill: "green".into(),
            }),
            SvgElement::Rect(SvgRect {
                x: "70".into(),
                y: "70".into(),
                width: "20".into(),
                height: "20".into(),
                fill: "blue".into(),
            }),
        ],
    };

    print_scenario(
        "Add a Child",
        "Insert a new circle element between two existing rect elements.",
        &before,
        &after,
    );
}

fn scenario_move_child() {
    let before = Svg {
        width: "200".into(),
        height: "100".into(),
        children: vec![
            SvgElement::Group(SvgGroup {
                id: "left".into(),
                children: vec![SvgElement::Circle(SvgCircle {
                    cx: "25".into(),
                    cy: "50".into(),
                    r: "20".into(),
                    fill: "red".into(),
                })],
            }),
            SvgElement::Group(SvgGroup {
                id: "right".into(),
                children: vec![SvgElement::Rect(SvgRect {
                    x: "130".into(),
                    y: "30".into(),
                    width: "40".into(),
                    height: "40".into(),
                    fill: "blue".into(),
                })],
            }),
        ],
    };

    // Move the circle from "left" group to "right" group
    let after = Svg {
        width: "200".into(),
        height: "100".into(),
        children: vec![
            SvgElement::Group(SvgGroup {
                id: "left".into(),
                children: vec![], // Circle removed
            }),
            SvgElement::Group(SvgGroup {
                id: "right".into(),
                children: vec![
                    SvgElement::Rect(SvgRect {
                        x: "130".into(),
                        y: "30".into(),
                        width: "40".into(),
                        height: "40".into(),
                        fill: "blue".into(),
                    }),
                    SvgElement::Circle(SvgCircle {
                        // Circle added here
                        cx: "25".into(),
                        cy: "50".into(),
                        r: "20".into(),
                        fill: "red".into(),
                    }),
                ],
            }),
        ],
    };

    print_scenario(
        "Move a Child Between Groups",
        "Move the circle from the 'left' group to the 'right' group. Ideally detected as a move.",
        &before,
        &after,
    );
}

fn scenario_nested_modification() {
    let before = Svg {
        width: "200".into(),
        height: "200".into(),
        children: vec![SvgElement::Group(SvgGroup {
            id: "outer".into(),
            children: vec![
                SvgElement::Group(SvgGroup {
                    id: "inner1".into(),
                    children: vec![SvgElement::Rect(SvgRect {
                        x: "10".into(),
                        y: "10".into(),
                        width: "40".into(),
                        height: "40".into(),
                        fill: "red".into(),
                    })],
                }),
                SvgElement::Group(SvgGroup {
                    id: "inner2".into(),
                    children: vec![SvgElement::Circle(SvgCircle {
                        cx: "150".into(),
                        cy: "150".into(),
                        r: "30".into(),
                        fill: "blue".into(),
                    })],
                }),
            ],
        })],
    };

    let mut after = before.clone();
    // Change the circle's fill in the deeply nested structure
    if let SvgElement::Group(ref mut outer) = after.children[0]
        && let SvgElement::Group(ref mut inner2) = outer.children[1]
        && let SvgElement::Circle(ref mut c) = inner2.children[0]
    {
        c.fill = "yellow".into();
        c.r = "40".into(); // Also change radius
    }

    print_scenario(
        "Nested Group Modification",
        "Modify circle attributes (fill, r) three levels deep in nested groups.",
        &before,
        &after,
    );
}
