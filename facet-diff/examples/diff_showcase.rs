//! facet-diff Showcase
//!
//! Demonstrates all diffing capabilities using the facet-showcase infrastructure.
//!
//! Run with: cargo run -p facet-diff --example diff_showcase

#![allow(dead_code)]

use facet::Facet;
use facet_diff::{FacetDiff, format_diff_default};
use facet_showcase::ShowcaseRunner;

// ============================================================================
// Example types
// ============================================================================

#[derive(Debug, Clone, Facet)]
struct User {
    name: String,
    email: String,
    age: u32,
    settings: Settings,
}

#[derive(Debug, Clone, Facet)]
struct Settings {
    theme: String,
    notifications: bool,
}

#[derive(Debug, Clone, Facet)]
struct Document {
    title: String,
    sections: Vec<Section>,
}

#[derive(Debug, Clone, Facet)]
struct Section {
    heading: String,
    content: String,
}

#[derive(Debug, Clone, Facet)]
#[repr(C)]
enum Status {
    Active,
    Inactive { reason: String },
    Pending { since: u32 },
}

// Deep tree types (6 levels of nesting)
#[derive(Debug, Clone, Facet)]
struct Level6 {
    value: i32,
    tag: String,
}

#[derive(Debug, Clone, Facet)]
struct Level5 {
    inner: Level6,
    count: u32,
}

#[derive(Debug, Clone, Facet)]
struct Level4Deep {
    inner: Level5,
    enabled: bool,
}

#[derive(Debug, Clone, Facet)]
struct Level3Deep {
    inner: Level4Deep,
    name: String,
}

#[derive(Debug, Clone, Facet)]
struct Level2Deep {
    inner: Level3Deep,
    priority: u8,
}

#[derive(Debug, Clone, Facet)]
struct Level1Deep {
    inner: Level2Deep,
    label: String,
}

// Wide tree type (many siblings)
#[derive(Debug, Clone, Facet)]
struct WideConfig {
    field_01: String,
    field_02: String,
    field_03: String,
    field_04: String,
    field_05: String,
    field_06: i32,
    field_07: i32,
    field_08: i32,
    field_09: i32,
    field_10: i32,
    field_11: bool,
    field_12: bool,
    field_13: bool,
    field_14: bool,
    field_15: bool,
    field_16: u64,
    field_17: u64,
    field_18: u64,
    field_19: u64,
    field_20: u64,
}

// ============================================================================
// Main
// ============================================================================

fn main() {
    let mut runner = ShowcaseRunner::new("Diff");

    runner.header();
    runner.intro("facet-diff provides comprehensive diffing capabilities for any type that implements `Facet`. It includes compact and tree formats with syntax highlighting and confusable character detection.");

    showcase_struct_fields(&mut runner);
    showcase_nested_structures(&mut runner);
    showcase_sequences(&mut runner);
    showcase_enums(&mut runner);
    showcase_options(&mut runner);
    showcase_many_changes(&mut runner);
    showcase_no_changes(&mut runner);
    showcase_scalar_types(&mut runner);
    showcase_confusables(&mut runner);
    showcase_byte_slices(&mut runner);
    showcase_deep_tree(&mut runner);
    showcase_wide_tree(&mut runner);

    runner.footer();
}

// ============================================================================
// Showcase functions
// ============================================================================

fn showcase_struct_fields(runner: &mut ShowcaseRunner) {
    let old = User {
        name: "Alice".into(),
        email: "alice@example.com".into(),
        age: 30,
        settings: Settings {
            theme: "dark".into(),
            notifications: true,
        },
    };

    let new = User {
        name: "Alice".into(),
        email: "alice@newdomain.com".into(),
        age: 31,
        settings: Settings {
            theme: "light".into(),
            notifications: true,
        },
    };

    let diff = old.diff(&new);
    let mut output = format_diff_default(&diff).to_string();
    output.push_str("\n\n");
    output.push_str(&diff.to_string());

    runner
        .scenario("Struct field changes")
        .description("Changes to multiple fields in a struct including nested settings.")
        .ansi_output(&output)
        .finish();
}

fn showcase_nested_structures(runner: &mut ShowcaseRunner) {
    let old = Document {
        title: "My Doc".into(),
        sections: vec![
            Section {
                heading: "Intro".into(),
                content: "Hello world".into(),
            },
            Section {
                heading: "Body".into(),
                content: "Some content here".into(),
            },
        ],
    };

    let new = Document {
        title: "My Doc".into(),
        sections: vec![
            Section {
                heading: "Introduction".into(),
                content: "Hello world".into(),
            },
            Section {
                heading: "Body".into(),
                content: "Updated content".into(),
            },
        ],
    };

    let diff = old.diff(&new);
    let output = format_diff_default(&diff).to_string();

    runner
        .scenario("Nested structures")
        .description("Changes to fields deep within nested structures.")
        .ansi_output(&output)
        .finish();
}

fn showcase_sequences(runner: &mut ShowcaseRunner) {
    let old_a: Vec<i32> = vec![1, 2, 3, 4, 5];
    let new_a: Vec<i32> = vec![1, 2, 99, 4, 5];
    let output_a = format!(
        "a) Single element change:\n{}\n\n",
        format_diff_default(&old_a.diff(&new_a))
    );

    let old_b: Vec<i32> = vec![1, 2, 3];
    let new_b: Vec<i32> = vec![1, 4, 5, 3];
    let output_b = format!("b) Insertions and deletions:\n{}\n\n", old_b.diff(&new_b));

    let old_c: Vec<&str> = vec!["a", "b", "c"];
    let new_c: Vec<&str> = vec!["c", "a", "b"];
    let output_c = format!("c) Reordering:\n{}", old_c.diff(&new_c));

    let output = format!("{output_a}{output_b}{output_c}");

    runner
        .scenario("Sequences (lists/arrays)")
        .description("Various operations on sequences including single element changes, insertions, deletions, and reordering.")
        .ansi_output(&output)
        .finish();
}

fn showcase_enums(runner: &mut ShowcaseRunner) {
    let old_a = Status::Inactive {
        reason: "vacation".into(),
    };
    let new_a = Status::Inactive {
        reason: "sick leave".into(),
    };
    let output_a = format!(
        "a) Same variant, different data:\n{}\n\n",
        format_diff_default(&old_a.diff(&new_a))
    );

    let old_b = Status::Active;
    let new_b = Status::Pending { since: 42 };
    let output_b = format!(
        "b) Different variants:\n{}",
        format_diff_default(&old_b.diff(&new_b))
    );

    let output = format!("{output_a}{output_b}");

    runner
        .scenario("Enums")
        .description(
            "Enum diffing including same variant with different data and different variants.",
        )
        .ansi_output(&output)
        .finish();
}

fn showcase_options(runner: &mut ShowcaseRunner) {
    let old_a: Option<User> = Some(User {
        name: "Bob".into(),
        email: "bob@example.com".into(),
        age: 25,
        settings: Settings {
            theme: "auto".into(),
            notifications: false,
        },
    });
    let new_a: Option<User> = Some(User {
        name: "Bob".into(),
        email: "bob@company.com".into(),
        age: 25,
        settings: Settings {
            theme: "auto".into(),
            notifications: true,
        },
    });
    let output_a = format!(
        "a) Some to Some (inner change):\n{}\n\n",
        format_diff_default(&old_a.diff(&new_a))
    );

    let old_b: Option<i32> = None;
    let new_b: Option<i32> = Some(42);
    let output_b = format!(
        "b) None to Some:\n{}",
        format_diff_default(&old_b.diff(&new_b))
    );

    let output = format!("{output_a}{output_b}");

    runner
        .scenario("Options")
        .description("Option types including inner value changes and None to Some transitions.")
        .ansi_output(&output)
        .finish();
}

fn showcase_many_changes(runner: &mut ShowcaseRunner) {
    let old: Vec<i32> = (0..30).collect();
    let mut new = old.clone();
    for i in (0..30).step_by(2) {
        new[i] *= 100;
    }

    let output = format_diff_default(&old.diff(&new)).to_string();

    runner
        .scenario("Many changes (truncated)")
        .description("Large number of changes that get truncated to show summary.")
        .ansi_output(&output)
        .finish();
}

fn showcase_no_changes(runner: &mut ShowcaseRunner) {
    let val = User {
        name: "Alice".into(),
        email: "alice@example.com".into(),
        age: 30,
        settings: Settings {
            theme: "dark".into(),
            notifications: true,
        },
    };

    let output = format_diff_default(&val.diff(&val.clone())).to_string();

    runner
        .scenario("No changes")
        .description("Comparing a value with itself shows no differences.")
        .ansi_output(&output)
        .finish();
}

fn showcase_scalar_types(runner: &mut ShowcaseRunner) {
    let mut output = String::new();

    output.push_str("a) Integers:\n");
    let old: i32 = 42;
    let new: i32 = -42;
    output.push_str(&format!(
        "  i32: {}\n",
        format_diff_default(&old.diff(&new))
    ));

    let old: i128 = i128::MIN;
    let new: i128 = i128::MAX;
    output.push_str(&format!(
        "  i128 min→max: {}\n",
        format_diff_default(&old.diff(&new))
    ));

    let old: u64 = 0;
    let new: u64 = u64::MAX;
    output.push_str(&format!(
        "  u64 0→max: {}\n\n",
        format_diff_default(&old.diff(&new))
    ));

    output.push_str("b) Floats:\n");
    let old: f64 = std::f64::consts::PI;
    let new: f64 = std::f64::consts::E;
    output.push_str(&format!(
        "  f64: {}\n",
        format_diff_default(&old.diff(&new))
    ));

    let old: f64 = f64::INFINITY;
    let new: f64 = f64::NEG_INFINITY;
    output.push_str(&format!(
        "  f64 inf→-inf: {}\n",
        format_diff_default(&old.diff(&new))
    ));

    let old: f64 = f64::NAN;
    let new: f64 = f64::NAN;
    output.push_str(&format!(
        "  f64 NaN→NaN: {}\n\n",
        format_diff_default(&old.diff(&new))
    ));

    output.push_str("c) Booleans:\n");
    let old: bool = true;
    let new: bool = false;
    output.push_str(&format!(
        "  bool: {}\n\n",
        format_diff_default(&old.diff(&new))
    ));

    output.push_str("d) Characters:\n");
    let old: char = 'A';
    let new: char = 'Z';
    output.push_str(&format!(
        "  char: {}\n",
        format_diff_default(&old.diff(&new))
    ));

    let old: char = '🦀';
    let new: char = '🐍';
    output.push_str(&format!(
        "  emoji: {}\n\n",
        format_diff_default(&old.diff(&new))
    ));

    output.push_str("e) Strings:\n");
    let old: &str = "hello";
    let new: &str = "world";
    output.push_str(&format!(
        "  &str: {}\n",
        format_diff_default(&old.diff(&new))
    ));

    let old: String = "Hello 世界".into();
    let new: String = "Hello 🌍".into();
    output.push_str(&format!(
        "  String unicode: {}",
        format_diff_default(&old.diff(&new))
    ));

    runner
        .scenario("Scalar types")
        .description("Diffing primitive types including integers, floats, booleans, characters, and strings.")
        .ansi_output(&output)
        .finish();
}

fn showcase_confusables(runner: &mut ShowcaseRunner) {
    let mut output = String::new();

    output.push_str("a) Latin 'a' vs Cyrillic 'а' (detected):\n");
    let old: &str = "abc";
    let new: &str = "аbc";
    output.push_str(&format!("{}\n\n", old.diff(&new)));

    output.push_str("b) Latin 'o' vs Greek 'ο' (detected):\n");
    let old: &str = "foo";
    let new: &str = "fοo";
    output.push_str(&format!("{}\n\n", old.diff(&new)));

    output.push_str("c) Latin 'e' vs Cyrillic 'е' (detected):\n");
    let old: &str = "hello";
    let new: &str = "hеllo";
    output.push_str(&format!("{}\n\n", old.diff(&new)));

    output.push_str("d) With zero-width joiner (not in TR39):\n");
    let old: &str = "test";
    let new: &str = "te\u{200D}st";
    output.push_str(&format!("{}\n\n", old.diff(&new)));

    output.push_str("e) Different quote styles (not in TR39):\n");
    let old: &str = "\"quoted\"";
    let new: &str = "\u{201C}quoted\u{201D}";
    output.push_str(&format!("{}\n\n", old.diff(&new)));

    output.push_str("f) Greek Iota vs Latin I (not in TR39):\n");
    let old: &str = "userId";
    let new: &str = "userΙd";
    output.push_str(&format!("{}", old.diff(&new)));

    runner
        .scenario("Confusable strings")
        .description("Detection of Unicode confusable characters using the Unicode TR39 confusables database. These include homoglyphs that look similar but are from different scripts.")
        .ansi_output(&output)
        .finish();
}

fn showcase_byte_slices(runner: &mut ShowcaseRunner) {
    let mut output = String::new();

    output.push_str("a) ASCII bytes:\n");
    let old: &[u8] = b"hello";
    let new: &[u8] = b"world";
    output.push_str(&format!("  {}\n\n", old.diff(&new)));

    output.push_str("b) Binary data:\n");
    let old: &[u8] = &[0x00, 0xFF, 0x42, 0x13];
    let new: &[u8] = &[0x00, 0xFE, 0x42, 0x37];
    output.push_str(&format!("  {}\n\n", old.diff(&new)));

    output.push_str("c) Vec<u8>:\n");
    let old: Vec<u8> = vec![1, 2, 3, 4, 5];
    let new: Vec<u8> = vec![1, 2, 99, 4, 5];
    output.push_str(&format!("  {}", format_diff_default(&old.diff(&new))));

    runner
        .scenario("Byte slices")
        .description("Diffing byte arrays including ASCII and binary data.")
        .ansi_output(&output)
        .finish();
}

fn showcase_deep_tree(runner: &mut ShowcaseRunner) {
    fn make_deep(
        value: i32,
        tag: &str,
        count: u32,
        enabled: bool,
        name: &str,
        priority: u8,
        label: &str,
    ) -> Level1Deep {
        Level1Deep {
            label: label.into(),
            inner: Level2Deep {
                priority,
                inner: Level3Deep {
                    name: name.into(),
                    inner: Level4Deep {
                        enabled,
                        inner: Level5 {
                            count,
                            inner: Level6 {
                                value,
                                tag: tag.into(),
                            },
                        },
                    },
                },
            },
        }
    }

    let mut output = String::new();

    output.push_str("a) Change at deepest level (level 6):\n");
    let old = make_deep(42, "original", 10, true, "config", 1, "root");
    let new = make_deep(999, "modified", 10, true, "config", 1, "root");
    output.push_str(&format!("{}\n\n", format_diff_default(&old.diff(&new))));

    output.push_str("b) Changes at multiple levels (2, 4, 6):\n");
    let old = make_deep(42, "tag", 10, true, "config", 1, "root");
    let new = make_deep(100, "tag", 10, false, "config", 5, "root");
    output.push_str(&format!("{}\n\n", format_diff_default(&old.diff(&new))));

    output.push_str("c) Changes at every level:\n");
    let old = make_deep(1, "a", 10, true, "old", 1, "label-old");
    let new = make_deep(2, "b", 20, false, "new", 2, "label-new");
    output.push_str(&format!("{}\n\n", format_diff_default(&old.diff(&new))));

    output.push_str("d) Tree format for deep change:\n");
    let old = make_deep(42, "deep", 10, true, "config", 1, "root");
    let new = make_deep(999, "deep", 10, true, "config", 1, "root");
    output.push_str(&format!("{}", old.diff(&new)));

    runner
        .scenario("Deep tree (6 levels)")
        .description(
            "Deeply nested structures demonstrating change detection at multiple nesting levels.",
        )
        .ansi_output(&output)
        .finish();
}

fn showcase_wide_tree(runner: &mut ShowcaseRunner) {
    #[allow(clippy::too_many_arguments)]
    fn make_wide(
        s1: &str,
        s2: &str,
        s3: &str,
        s4: &str,
        s5: &str,
        i1: i32,
        i2: i32,
        i3: i32,
        i4: i32,
        i5: i32,
        b1: bool,
        b2: bool,
        b3: bool,
        b4: bool,
        b5: bool,
        u1: u64,
        u2: u64,
        u3: u64,
        u4: u64,
        u5: u64,
    ) -> WideConfig {
        WideConfig {
            field_01: s1.into(),
            field_02: s2.into(),
            field_03: s3.into(),
            field_04: s4.into(),
            field_05: s5.into(),
            field_06: i1,
            field_07: i2,
            field_08: i3,
            field_09: i4,
            field_10: i5,
            field_11: b1,
            field_12: b2,
            field_13: b3,
            field_14: b4,
            field_15: b5,
            field_16: u1,
            field_17: u2,
            field_18: u3,
            field_19: u4,
            field_20: u5,
        }
    }

    let mut output = String::new();

    output.push_str("a) Single field change (among 20 fields):\n");
    let old = make_wide(
        "a", "b", "c", "d", "e", 1, 2, 3, 4, 5, true, true, true, true, true, 100, 200, 300, 400,
        500,
    );
    let new = make_wide(
        "a", "b", "c", "d", "e", 1, 2, 3, 4, 5, true, true, true, true, true, 100, 200, 999, 400,
        500,
    );
    output.push_str(&format!("{}\n\n", format_diff_default(&old.diff(&new))));

    output.push_str("b) Scattered changes (fields 2, 8, 14, 19):\n");
    let old = make_wide(
        "a", "b", "c", "d", "e", 1, 2, 3, 4, 5, true, true, true, true, true, 100, 200, 300, 400,
        500,
    );
    let new = make_wide(
        "a", "CHANGED", "c", "d", "e", 1, 2, 999, 4, 5, true, true, true, false, true, 100, 200,
        300, 888, 500,
    );
    output.push_str(&format!("{}\n\n", format_diff_default(&old.diff(&new))));

    output.push_str("c) Many changes (exceeds truncation limit):\n");
    let old = make_wide(
        "a", "b", "c", "d", "e", 1, 2, 3, 4, 5, true, true, true, true, true, 100, 200, 300, 400,
        500,
    );
    let new = make_wide(
        "A", "B", "C", "D", "E", 10, 20, 30, 40, 50, false, false, false, false, false, 1000, 2000,
        3000, 4000, 5000,
    );
    output.push_str(&format!("{}\n\n", format_diff_default(&old.diff(&new))));

    output.push_str("d) Tree format with few changes:\n");
    let old = make_wide(
        "a", "b", "c", "d", "e", 1, 2, 3, 4, 5, true, true, true, true, true, 100, 200, 300, 400,
        500,
    );
    let new = make_wide(
        "a", "b", "c", "d", "e", 1, 2, 3, 4, 5, true, false, true, true, true, 100, 200, 300, 400,
        500,
    );
    output.push_str(&format!("{}", old.diff(&new)));

    runner
        .scenario("Wide tree (20 fields)")
        .description("Structure with many fields demonstrating diff truncation and summarization.")
        .ansi_output(&output)
        .finish();
}
