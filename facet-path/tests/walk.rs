//! Tests for the shape-only visitor API (`walk_shape`).

use facet::Facet;
use facet_core::Shape;
use facet_path::{Path, ShapeVisitor, VisitDecision, WalkStatus, walk_shape};
use std::collections::HashMap;

/// A visitor that records every `enter` and `leave` call as strings
/// for snapshot testing.
struct RecordingVisitor {
    events: Vec<String>,
}

impl RecordingVisitor {
    fn new() -> Self {
        Self { events: Vec::new() }
    }
}

impl ShapeVisitor for RecordingVisitor {
    fn enter(&mut self, path: &Path, shape: &'static Shape) -> VisitDecision {
        self.events.push(format!(
            "enter {} ({})",
            if path.is_empty() {
                "<root>".to_string()
            } else {
                path.format()
            },
            shape.type_identifier,
        ));
        VisitDecision::Recurse
    }

    fn leave(&mut self, path: &Path, shape: &'static Shape) {
        self.events.push(format!(
            "leave {} ({})",
            if path.is_empty() {
                "<root>".to_string()
            } else {
                path.format()
            },
            shape.type_identifier,
        ));
    }
}

// ---------------------------------------------------------------------------
// Struct traversal
// ---------------------------------------------------------------------------

#[test]
fn test_walk_simple_struct() {
    facet_testhelpers::setup();

    #[derive(Facet)]
    #[allow(dead_code)]
    struct Point {
        x: f64,
        y: f64,
    }

    let mut v = RecordingVisitor::new();
    let status = walk_shape(Point::SHAPE, &mut v);
    assert_eq!(status, WalkStatus::Completed);
    insta::assert_snapshot!(v.events.join("\n"));
}

#[test]
fn test_walk_nested_struct() {
    facet_testhelpers::setup();

    #[derive(Facet)]
    #[allow(dead_code)]
    struct Inner {
        value: i32,
    }

    #[derive(Facet)]
    #[allow(dead_code)]
    struct Outer {
        label: String,
        inner: Inner,
    }

    let mut v = RecordingVisitor::new();
    let status = walk_shape(Outer::SHAPE, &mut v);
    assert_eq!(status, WalkStatus::Completed);
    insta::assert_snapshot!(v.events.join("\n"));
}

#[test]
fn test_walk_unit_struct() {
    facet_testhelpers::setup();

    #[derive(Facet)]
    #[allow(dead_code)]
    struct Unit;

    let mut v = RecordingVisitor::new();
    let status = walk_shape(Unit::SHAPE, &mut v);
    assert_eq!(status, WalkStatus::Completed);
    insta::assert_snapshot!(v.events.join("\n"));
}

#[test]
fn test_walk_tuple_struct() {
    facet_testhelpers::setup();

    #[derive(Facet)]
    #[allow(dead_code)]
    struct Pair(u32, String);

    let mut v = RecordingVisitor::new();
    let status = walk_shape(Pair::SHAPE, &mut v);
    assert_eq!(status, WalkStatus::Completed);
    insta::assert_snapshot!(v.events.join("\n"));
}

// ---------------------------------------------------------------------------
// Enum traversal
// ---------------------------------------------------------------------------

#[test]
fn test_walk_enum_all_variant_kinds() {
    facet_testhelpers::setup();

    #[derive(Facet)]
    #[repr(C)]
    #[allow(dead_code)]
    enum Message {
        Quit,
        Text(String),
        Move { x: i32, y: i32 },
    }

    let mut v = RecordingVisitor::new();
    let status = walk_shape(Message::SHAPE, &mut v);
    assert_eq!(status, WalkStatus::Completed);
    insta::assert_snapshot!(v.events.join("\n"));
}

#[test]
fn test_walk_enum_nested() {
    facet_testhelpers::setup();

    #[derive(Facet)]
    #[allow(dead_code)]
    struct Payload {
        data: Vec<u8>,
    }

    #[derive(Facet)]
    #[repr(C)]
    #[allow(dead_code)]
    enum Packet {
        Empty,
        Single(Payload),
        Multi { first: Payload, second: Payload },
    }

    let mut v = RecordingVisitor::new();
    let status = walk_shape(Packet::SHAPE, &mut v);
    assert_eq!(status, WalkStatus::Completed);
    insta::assert_snapshot!(v.events.join("\n"));
}

// ---------------------------------------------------------------------------
// Container types
// ---------------------------------------------------------------------------

#[test]
fn test_walk_vec() {
    facet_testhelpers::setup();

    #[derive(Facet)]
    #[allow(dead_code)]
    struct Item {
        id: u32,
    }

    #[derive(Facet)]
    #[allow(dead_code)]
    struct Container {
        items: Vec<Item>,
    }

    let mut v = RecordingVisitor::new();
    let status = walk_shape(Container::SHAPE, &mut v);
    assert_eq!(status, WalkStatus::Completed);
    insta::assert_snapshot!(v.events.join("\n"));
}

#[test]
fn test_walk_hashmap() {
    facet_testhelpers::setup();

    #[derive(Facet)]
    #[allow(dead_code)]
    struct Config {
        settings: HashMap<String, u32>,
    }

    let mut v = RecordingVisitor::new();
    let status = walk_shape(Config::SHAPE, &mut v);
    assert_eq!(status, WalkStatus::Completed);
    insta::assert_snapshot!(v.events.join("\n"));
}

#[test]
fn test_walk_option() {
    facet_testhelpers::setup();

    #[derive(Facet)]
    #[allow(dead_code)]
    struct MaybeNamed {
        name: Option<String>,
    }

    let mut v = RecordingVisitor::new();
    let status = walk_shape(MaybeNamed::SHAPE, &mut v);
    assert_eq!(status, WalkStatus::Completed);
    insta::assert_snapshot!(v.events.join("\n"));
}

// ---------------------------------------------------------------------------
// SkipChildren
// ---------------------------------------------------------------------------

/// A visitor that skips children of any shape whose type_identifier matches.
struct SkipVisitor {
    skip_type: &'static str,
    events: Vec<String>,
}

impl ShapeVisitor for SkipVisitor {
    fn enter(&mut self, path: &Path, shape: &'static Shape) -> VisitDecision {
        let label = if path.is_empty() {
            "<root>".to_string()
        } else {
            path.format()
        };
        self.events
            .push(format!("enter {} ({})", label, shape.type_identifier));
        if shape.type_identifier == self.skip_type {
            VisitDecision::SkipChildren
        } else {
            VisitDecision::Recurse
        }
    }

    fn leave(&mut self, path: &Path, shape: &'static Shape) {
        let label = if path.is_empty() {
            "<root>".to_string()
        } else {
            path.format()
        };
        self.events
            .push(format!("leave {} ({})", label, shape.type_identifier));
    }
}

#[test]
fn test_skip_children() {
    facet_testhelpers::setup();

    #[derive(Facet)]
    #[allow(dead_code)]
    struct Inner {
        deep: u32,
    }

    #[derive(Facet)]
    #[allow(dead_code)]
    struct Middle {
        inner: Inner,
    }

    #[derive(Facet)]
    #[allow(dead_code)]
    struct Top {
        a: u32,
        mid: Middle,
        b: u32,
    }

    // Skip Middle — should see enter/leave for Middle but not Inner or deep
    let mut v = SkipVisitor {
        skip_type: "Middle",
        events: Vec::new(),
    };
    let status = walk_shape(Top::SHAPE, &mut v);
    assert_eq!(status, WalkStatus::Completed);
    insta::assert_snapshot!(v.events.join("\n"));
}

// ---------------------------------------------------------------------------
// Stop
// ---------------------------------------------------------------------------

/// A visitor that stops when it encounters a specific type.
struct StopVisitor {
    stop_at: &'static str,
    events: Vec<String>,
}

impl ShapeVisitor for StopVisitor {
    fn enter(&mut self, path: &Path, shape: &'static Shape) -> VisitDecision {
        let label = if path.is_empty() {
            "<root>".to_string()
        } else {
            path.format()
        };
        self.events
            .push(format!("enter {} ({})", label, shape.type_identifier));
        if shape.type_identifier == self.stop_at {
            VisitDecision::Stop
        } else {
            VisitDecision::Recurse
        }
    }

    fn leave(&mut self, path: &Path, shape: &'static Shape) {
        let label = if path.is_empty() {
            "<root>".to_string()
        } else {
            path.format()
        };
        self.events
            .push(format!("leave {} ({})", label, shape.type_identifier));
    }
}

#[test]
fn test_stop() {
    facet_testhelpers::setup();

    #[derive(Facet)]
    #[allow(dead_code)]
    struct Inner {
        deep: u32,
    }

    #[derive(Facet)]
    #[allow(dead_code)]
    struct Top {
        a: u32,
        inner: Inner,
        b: u32,
    }

    // Stop at Inner — should not see deep or b, and no leave calls after stop
    let mut v = StopVisitor {
        stop_at: "Inner",
        events: Vec::new(),
    };
    let status = walk_shape(Top::SHAPE, &mut v);
    assert_eq!(status, WalkStatus::Stopped);
    insta::assert_snapshot!(v.events.join("\n"));
}

// ---------------------------------------------------------------------------
// Recursive types (cycle detection)
// ---------------------------------------------------------------------------

#[test]
fn test_walk_recursive_type() {
    facet_testhelpers::setup();

    #[derive(Facet)]
    #[allow(dead_code)]
    struct TreeNode {
        value: i32,
        #[facet(recursive_type)]
        children: Vec<TreeNode>,
    }

    let mut v = RecordingVisitor::new();
    let status = walk_shape(TreeNode::SHAPE, &mut v);
    assert_eq!(status, WalkStatus::Completed);
    insta::assert_snapshot!(v.events.join("\n"));
}

// ---------------------------------------------------------------------------
// Deterministic ordering
// ---------------------------------------------------------------------------

#[test]
fn test_deterministic_ordering() {
    facet_testhelpers::setup();

    #[derive(Facet)]
    #[allow(dead_code)]
    struct Ordered {
        alpha: u8,
        beta: u16,
        gamma: u32,
        delta: u64,
    }

    // Run twice and verify identical output
    let mut v1 = RecordingVisitor::new();
    walk_shape(Ordered::SHAPE, &mut v1);
    let mut v2 = RecordingVisitor::new();
    walk_shape(Ordered::SHAPE, &mut v2);
    assert_eq!(v1.events, v2.events);
    insta::assert_snapshot!(v1.events.join("\n"));
}

// ---------------------------------------------------------------------------
// Scalar leaf
// ---------------------------------------------------------------------------

#[test]
fn test_walk_scalar() {
    facet_testhelpers::setup();

    let mut v = RecordingVisitor::new();
    let status = walk_shape(<u32 as Facet>::SHAPE, &mut v);
    assert_eq!(status, WalkStatus::Completed);
    insta::assert_snapshot!(v.events.join("\n"));
}
