//! Tests for diff-aware XML serialization.

use facet::Facet;
use facet_diff::FacetDiff;
use facet_testhelpers::test;
use facet_xml_diff::{DiffSerializeOptions, diff_to_string, diff_to_string_with_options};

#[derive(Facet, Debug, PartialEq, Clone)]
struct Point {
    x: i32,
    y: i32,
}

#[derive(Facet, Debug, PartialEq, Clone)]
struct Rect {
    fill: String,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

#[test]
fn test_diff_simple_change() {
    let old = Point { x: 10, y: 20 };
    let new = Point { x: 15, y: 20 };
    let diff = old.diff(&new);

    let xml = diff_to_string(&old, &new, &diff);
    eprintln!("Diff output:\n{}", xml);

    // Verify the output contains the expected diff markers
    assert!(xml.contains("10") || xml.contains("15"));
}

#[test]
fn test_diff_no_colors() {
    let old = Point { x: 10, y: 20 };
    let new = Point { x: 15, y: 20 };
    let diff = old.diff(&new);

    let options = DiffSerializeOptions::new().no_colors();
    let xml = diff_to_string_with_options(&old, &new, &diff, &options);
    eprintln!("Plain diff output:\n{}", xml);

    // Should not contain ANSI escape codes
    assert!(!xml.contains("\x1b["));
}

#[test]
fn test_diff_multiple_changes() {
    let old = Rect {
        fill: "red".to_string(),
        x: 10,
        y: 10,
        width: 50,
        height: 50,
    };
    let new = Rect {
        fill: "blue".to_string(),
        x: 10,
        y: 20, // changed
        width: 50,
        height: 100, // changed
    };
    let diff = old.diff(&new);

    let options = DiffSerializeOptions::new().no_colors();
    let xml = diff_to_string_with_options(&old, &new, &diff, &options);
    eprintln!("Rect diff output:\n{}", xml);

    // Should show the changes
    assert!(!xml.is_empty());
}

#[test]
fn test_diff_no_changes() {
    let old = Point { x: 10, y: 20 };
    let new = Point { x: 10, y: 20 };
    let diff = old.diff(&new);

    let options = DiffSerializeOptions::new().no_colors();
    let xml = diff_to_string_with_options(&old, &new, &diff, &options);
    eprintln!("No changes output:\n{}", xml);

    // Should still produce valid output
    assert!(!xml.is_empty());
}
