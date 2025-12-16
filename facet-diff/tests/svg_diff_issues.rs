//! Tests for SVG-like XML diff rendering issues.
//!
//! These tests reproduce issues found when diffing SVG structures:
//! 1. Wrapper elements like `<children>`, `<PathData>`, `<commands>` appearing in output
//! 2. Empty placeholder (`∅`) behavior for missing attribute values

use facet::Facet;
use facet_diff::{DiffOptions, DiffReport, diff_new_peek_with_options};
use facet_reflect::Peek;
use facet_testhelpers::test;
use facet_xml as xml;

// =============================================================================
// Types that reproduce the wrapper element issue
// =============================================================================

/// A path command similar to SVG's path commands
#[derive(Facet, Debug, Clone, PartialEq)]
#[repr(u8)]
pub enum PathCommand {
    MoveTo { x: f64, y: f64 },
    LineTo { x: f64, y: f64 },
    Arc { rx: f64, ry: f64, x: f64, y: f64 },
    ClosePath,
}

/// Structured path data - a sequence of commands
#[derive(Facet, Debug, Clone, PartialEq, Default)]
pub struct PathData {
    /// This field does NOT have xml::elements, so it will render
    /// as a wrapper `<commands>` element in XML diff output
    pub commands: Vec<PathCommand>,
}

/// A path element with the PathData
#[derive(Facet, Debug, Clone, Default)]
#[facet(xml::ns_all = "http://www.w3.org/2000/svg", rename = "path")]
pub struct Path {
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub fill: Option<String>,
    /// The `d` attribute in SVG - this is a complex nested type
    pub d: Option<PathData>,
}

/// SVG node enum (newtype variants)
#[derive(Facet, Debug, Clone)]
#[facet(xml::ns_all = "http://www.w3.org/2000/svg")]
#[repr(u8)]
pub enum SvgNode {
    #[facet(rename = "path")]
    Path(Path),
    #[facet(rename = "circle")]
    Circle(Circle),
}

/// A circle element
#[derive(Facet, Debug, Clone, Default)]
#[facet(xml::ns_all = "http://www.w3.org/2000/svg", rename = "circle")]
pub struct Circle {
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub cx: Option<f64>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub cy: Option<f64>,
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub r: Option<f64>,
}

/// SVG root element
#[derive(Facet, Debug, Clone, Default)]
#[facet(xml::ns_all = "http://www.w3.org/2000/svg", rename = "svg")]
pub struct Svg {
    #[facet(xml::attribute, default, skip_serializing_if = Option::is_none)]
    pub view_box: Option<String>,
    /// This has xml::elements, so children should be rendered properly
    #[facet(xml::elements)]
    pub children: Vec<SvgNode>,
}

/// Helper to compute diff and render as plain XML
fn diff_to_xml<'f, T: facet_core::Facet<'f>>(from: &T, to: &T, float_tolerance: f64) -> String {
    let left = Peek::new(from);
    let right = Peek::new(to);

    let options = DiffOptions::new()
        .with_float_tolerance(float_tolerance)
        .with_similarity_threshold(0.5);
    let diff = diff_new_peek_with_options(left, right, &options);

    let report = DiffReport::new(diff, left, right).with_float_tolerance(float_tolerance);
    report.render_plain_xml()
}

// =============================================================================
// Issue 1: Wrapper elements in diff output
// =============================================================================

/// Test that PathData.commands doesn't render with a <commands> wrapper
#[test]
fn test_path_data_commands_no_wrapper() {
    let from = PathData {
        commands: vec![
            PathCommand::MoveTo { x: 10.0, y: 20.0 },
            PathCommand::LineTo { x: 30.0, y: 40.0 },
            PathCommand::ClosePath,
        ],
    };

    let to = PathData {
        commands: vec![
            PathCommand::MoveTo { x: 10.0, y: 20.0 },
            PathCommand::LineTo { x: 50.0, y: 60.0 }, // changed
            PathCommand::ClosePath,
        ],
    };

    let xml_output = diff_to_xml(&from, &to, 0.002);
    println!("=== PathData diff (XML) ===\n{}", xml_output);

    // The output should NOT contain <commands> wrapper
    // It should show the path commands directly
    assert!(
        !xml_output.contains("<commands>"),
        "Output should NOT have <commands> wrapper:\n{}",
        xml_output
    );
}

/// Test that Svg.children with xml::elements doesn't render with a <children> wrapper
#[test]
fn test_svg_children_no_wrapper() {
    let from = Svg {
        view_box: Some("0 0 100 100".to_string()),
        children: vec![SvgNode::Circle(Circle {
            cx: Some(50.0),
            cy: Some(50.0),
            r: Some(25.0),
        })],
    };

    let to = Svg {
        view_box: Some("0 0 100 100".to_string()),
        children: vec![SvgNode::Circle(Circle {
            cx: Some(50.0),
            cy: Some(50.0),
            r: Some(30.0), // changed
        })],
    };

    let xml_output = diff_to_xml(&from, &to, 0.002);
    println!("=== Svg.children diff (XML) ===\n{}", xml_output);

    // The output should NOT contain <children> wrapper since we have xml::elements
    assert!(
        !xml_output.contains("<children>"),
        "Output should NOT have <children> wrapper:\n{}",
        xml_output
    );
}

/// Test that PathData doesn't render as a <PathData> element when nested
#[test]
fn test_nested_path_data_no_wrapper() {
    let from = Path {
        fill: Some("red".to_string()),
        d: Some(PathData {
            commands: vec![
                PathCommand::MoveTo { x: 10.0, y: 20.0 },
                PathCommand::LineTo { x: 30.0, y: 40.0 },
            ],
        }),
    };

    let to = Path {
        fill: Some("red".to_string()),
        d: Some(PathData {
            commands: vec![
                PathCommand::MoveTo { x: 10.0, y: 20.0 },
                PathCommand::LineTo { x: 50.0, y: 60.0 }, // changed
            ],
        }),
    };

    let xml_output = diff_to_xml(&from, &to, 0.002);
    println!("=== Nested PathData diff (XML) ===\n{}", xml_output);

    // The output should NOT contain <PathData> as a wrapper element
    assert!(
        !xml_output.contains("<PathData>"),
        "Output should NOT have <PathData> wrapper:\n{}",
        xml_output
    );
}

// =============================================================================
// Issue 2: Empty placeholder (∅) behavior
// =============================================================================

/// Test when comparing LineTo commands where one has values and one doesn't
/// This reproduces the strange:
///   ← <LineTo ∅           ∅           />
///   → <LineTo x="106.589" y="348.771" />
#[test]
fn test_lineto_empty_placeholder() {
    // Scenario: One list has a LineTo, the other doesn't (or has different commands)
    let from = PathData {
        commands: vec![
            PathCommand::MoveTo { x: 100.0, y: 200.0 },
            // No LineTo here - this will show as "deleted"
            PathCommand::ClosePath,
        ],
    };

    let to = PathData {
        commands: vec![
            PathCommand::MoveTo { x: 100.0, y: 200.0 },
            PathCommand::LineTo { x: 100.0, y: 200.0 }, // Added
            PathCommand::ClosePath,
        ],
    };

    let xml_output = diff_to_xml(&from, &to, 0.002);
    println!("=== LineTo insertion diff (XML) ===\n{}", xml_output);

    // This should show LineTo as inserted with + prefix, not as a modification
    // with ∅ placeholders
    assert!(
        !xml_output.contains("∅"),
        "Output should NOT contain empty placeholder ∅:\n{}",
        xml_output
    );
}

/// Test when comparing two LineTo commands where values differ
/// This should show a proper inline diff, not ∅ placeholders
#[test]
fn test_lineto_value_change_no_empty_placeholder() {
    let from = PathData {
        commands: vec![
            PathCommand::MoveTo { x: 100.0, y: 200.0 },
            PathCommand::LineTo { x: 30.0, y: 40.0 },
            PathCommand::ClosePath,
        ],
    };

    let to = PathData {
        commands: vec![
            PathCommand::MoveTo { x: 100.0, y: 200.0 },
            PathCommand::LineTo { x: 50.0, y: 60.0 }, // Changed values
            PathCommand::ClosePath,
        ],
    };

    let xml_output = diff_to_xml(&from, &to, 0.002);
    println!("=== LineTo value change diff (XML) ===\n{}", xml_output);

    // Should show the old and new values, not ∅
    assert!(
        !xml_output.contains("∅"),
        "Output should NOT contain empty placeholder ∅:\n{}",
        xml_output
    );

    // Should contain both old and new x/y values
    assert!(xml_output.contains("30"), "Should contain old x value");
    assert!(xml_output.contains("40"), "Should contain old y value");
    assert!(xml_output.contains("50"), "Should contain new x value");
    assert!(xml_output.contains("60"), "Should contain new y value");
}

/// Test when comparing variant change (MoveTo to LineTo) - this should properly
/// show as deleted/inserted, not as a modification with empty placeholders
#[test]
fn test_variant_change_no_empty_placeholder() {
    let from = PathData {
        commands: vec![PathCommand::MoveTo { x: 100.0, y: 200.0 }],
    };

    let to = PathData {
        commands: vec![PathCommand::LineTo { x: 100.0, y: 200.0 }],
    };

    let xml_output = diff_to_xml(&from, &to, 0.002);
    println!("=== Variant change diff (XML) ===\n{}", xml_output);

    // Should show MoveTo as deleted and LineTo as inserted
    // Not as a modification with ∅ placeholders
    assert!(
        !xml_output.contains("∅"),
        "Output should NOT contain empty placeholder ∅:\n{}",
        xml_output
    );
}

// =============================================================================
// Combined tests - full SVG structure
// =============================================================================

/// Test a full SVG structure diff
#[test]
fn test_full_svg_diff_no_wrappers() {
    let from = Svg {
        view_box: Some("0 0 200 200".to_string()),
        children: vec![
            SvgNode::Path(Path {
                fill: Some("red".to_string()),
                d: Some(PathData {
                    commands: vec![
                        PathCommand::MoveTo { x: 10.0, y: 20.0 },
                        PathCommand::LineTo { x: 30.0, y: 40.0 },
                        PathCommand::Arc {
                            rx: 5.0,
                            ry: 5.0,
                            x: 50.0,
                            y: 60.0,
                        },
                        PathCommand::ClosePath,
                    ],
                }),
            }),
            SvgNode::Circle(Circle {
                cx: Some(100.0),
                cy: Some(100.0),
                r: Some(50.0),
            }),
        ],
    };

    let to = Svg {
        view_box: Some("0 0 200 200".to_string()),
        children: vec![
            SvgNode::Path(Path {
                fill: Some("blue".to_string()), // changed
                d: Some(PathData {
                    commands: vec![
                        PathCommand::MoveTo { x: 10.0, y: 20.0 },
                        PathCommand::LineTo { x: 35.0, y: 45.0 }, // changed
                        PathCommand::Arc {
                            rx: 5.0,
                            ry: 5.0,
                            x: 50.0,
                            y: 60.0,
                        },
                        PathCommand::ClosePath,
                    ],
                }),
            }),
            SvgNode::Circle(Circle {
                cx: Some(100.0),
                cy: Some(100.0),
                r: Some(60.0), // changed
            }),
        ],
    };

    let xml_output = diff_to_xml(&from, &to, 0.002);
    println!("=== Full SVG diff (XML) ===\n{}", xml_output);

    // Should NOT have wrapper elements for sequences
    assert!(
        !xml_output.contains("<children>"),
        "Output should NOT have <children> wrapper:\n{}",
        xml_output
    );
    assert!(
        !xml_output.contains("<commands>"),
        "Output should NOT have <commands> wrapper:\n{}",
        xml_output
    );
    // NOTE: <PathData> still appears because it's a type name, not a sequence wrapper.
    // Removing type wrappers would require explicit #[facet(xml::transparent)] support.

    // Should NOT have empty placeholders
    assert!(
        !xml_output.contains("∅"),
        "Output should NOT contain empty placeholder ∅:\n{}",
        xml_output
    );
}
