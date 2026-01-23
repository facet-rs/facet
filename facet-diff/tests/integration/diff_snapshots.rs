//! Snapshot tests for facet-diff output formatting.
//!
//! These tests use insta to capture the diff output format and ensure
//! consistent rendering across changes.

#![allow(dead_code)]

use boxen::{BorderStyle, Color, builder};
use facet::Facet;
use facet_diff::FacetDiff;
use facet_pretty::{PrettyPrinter, tokyo_night};
use facet_reflect::Peek;
use facet_testhelpers::test;
use facet_value::value;
use insta::assert_snapshot;

// Convert Tokyo Night Rgb to boxen Color
const fn to_boxen(c: owo_colors::Rgb) -> Color {
    Color::Rgb(c.0, c.1, c.2)
}

/// Strip ANSI escape codes from a string
fn strip_ansi(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip escape sequence
            if chars.peek() == Some(&'[') {
                chars.next(); // consume '['
                // Skip until we hit a letter (end of sequence)
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Format a diff comparison showing before, after, and the resulting diff
fn format_diff_comparison<'a, A, B>(before: &A, after: &B) -> String
where
    A: facet::Facet<'a>,
    B: facet::Facet<'a>,
{
    let printer = PrettyPrinter::default()
        .with_colors(facet_pretty::ColorMode::Always)
        .with_minimal_option_names(true);

    let before_str = printer.format_peek(Peek::new(before));
    let after_str = printer.format_peek(Peek::new(after));
    let diff = before.diff(after);
    // Keep colors in diff output
    let diff_str = format!("{diff}");

    // For width calculation, strip ANSI codes from all strings
    let before_str_plain = strip_ansi(&before_str);
    let after_str_plain = strip_ansi(&after_str);
    let diff_str_plain = strip_ansi(&diff_str);

    // Calculate minimum width to fit titles and content
    let min_width = 12;
    let content_width = before_str_plain
        .lines()
        .chain(after_str_plain.lines())
        .chain(diff_str_plain.lines())
        .map(|l| l.len())
        .max()
        .unwrap_or(0)
        .max(min_width);

    let before_box = builder()
        .border_style(BorderStyle::Round)
        .border_color(to_boxen(tokyo_night::BORDER))
        .title("Before")
        .width(content_width + 2)
        .render(&before_str)
        .unwrap_or_else(|_| before_str.clone());

    let after_box = builder()
        .border_style(BorderStyle::Round)
        .border_color(to_boxen(tokyo_night::BORDER))
        .title("After")
        .width(content_width + 2)
        .render(&after_str)
        .unwrap_or_else(|_| after_str.clone());

    let diff_box = builder()
        .border_style(BorderStyle::Double)
        .border_color(to_boxen(tokyo_night::YELLOW))
        .title("Diff")
        .width(content_width + 2)
        .render(diff_str.trim_end())
        .unwrap_or_else(|_| diff_str.clone());

    // Strip ANSI codes from final output to ensure consistent snapshots across platforms
    // (boxen's border coloring behaves differently on macOS vs Linux)
    strip_ansi(&format!("{before_box}\n{after_box}\n{diff_box}"))
}

// ============================================================================
// Scalar diff tests
// ============================================================================

#[test]
fn diff_integers_equal() {
    let a = 42i32;
    let b = 42i32;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_integers_different() {
    let a = 42i32;
    let b = 100i32;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_integers_different_types() {
    let a = 42i32;
    let b = 42i64;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_strings_equal() {
    let a = "hello";
    let b = "hello";
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_strings_different() {
    let a = "hello";
    let b = "world";
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_bools_equal() {
    let a = true;
    let b = true;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_bools_different() {
    let a = true;
    let b = false;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_floats_equal() {
    let a = 2.14f64;
    let b = 2.14f64;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_floats_different() {
    let a = 2.14f64;
    let b = 2.71f64;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

// ============================================================================
// Struct diff tests
// ============================================================================

#[derive(Facet)]
struct User<'a> {
    name: &'a str,
    age: u8,
}

#[derive(Facet)]
struct OwnedUser {
    name: String,
    age: u8,
}

#[derive(Facet)]
struct ExtendedUser<'a> {
    name: &'a str,
    age: u8,
    email: &'a str,
}

#[derive(Facet)]
struct Address {
    street: String,
    city: String,
}

#[derive(Facet)]
struct UserWithAddress<'a> {
    name: &'a str,
    age: u8,
    address: Address,
}

#[test]
fn diff_structs_equal() {
    let a = User {
        name: "Alice",
        age: 30,
    };
    let b = User {
        name: "Alice",
        age: 30,
    };
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_structs_one_field_different() {
    let a = User {
        name: "Alice",
        age: 30,
    };
    let b = User {
        name: "Bob",
        age: 30,
    };
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_structs_all_fields_different() {
    let a = User {
        name: "Alice",
        age: 30,
    };
    let b = User {
        name: "Bob",
        age: 25,
    };
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_structs_different_types_same_shape() {
    let a = User {
        name: "Alice",
        age: 30,
    };
    let b = OwnedUser {
        name: "Alice".to_string(),
        age: 30,
    };
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_structs_field_added() {
    let a = User {
        name: "Alice",
        age: 30,
    };
    let b = ExtendedUser {
        name: "Alice",
        age: 30,
        email: "alice@example.com",
    };
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_structs_field_removed() {
    let a = ExtendedUser {
        name: "Alice",
        age: 30,
        email: "alice@example.com",
    };
    let b = User {
        name: "Alice",
        age: 30,
    };
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_structs_nested() {
    let a = UserWithAddress {
        name: "Alice",
        age: 30,
        address: Address {
            street: "123 Main St".to_string(),
            city: "Wonderland".to_string(),
        },
    };
    let b = UserWithAddress {
        name: "Alice",
        age: 30,
        address: Address {
            street: "456 Oak Ave".to_string(),
            city: "Wonderland".to_string(),
        },
    };
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_structs_nested_multiple_changes() {
    let a = UserWithAddress {
        name: "Alice",
        age: 30,
        address: Address {
            street: "123 Main St".to_string(),
            city: "Wonderland".to_string(),
        },
    };
    let b = UserWithAddress {
        name: "Bob",
        age: 25,
        address: Address {
            street: "456 Oak Ave".to_string(),
            city: "Elsewhere".to_string(),
        },
    };
    assert_snapshot!(format_diff_comparison(&a, &b));
}

// ============================================================================
// Enum diff tests
// ============================================================================

#[derive(Facet)]
#[repr(C)]
enum Status {
    Active,
    Inactive,
    Pending(u32),
}

#[derive(Facet)]
#[repr(C)]
enum Message {
    Text(String),
    Number(i32),
    Pair(String, i32),
}

#[derive(Facet)]
#[repr(C)]
enum DetailedStatus {
    Active { since: u32 },
    Inactive { reason: String },
}

#[test]
fn diff_enums_same_unit_variant() {
    let a = Status::Active;
    let b = Status::Active;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_enums_different_unit_variants() {
    let a = Status::Active;
    let b = Status::Inactive;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_enums_same_tuple_variant_equal() {
    let a = Status::Pending(5);
    let b = Status::Pending(5);
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_enums_same_tuple_variant_different() {
    let a = Status::Pending(5);
    let b = Status::Pending(10);
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_enums_unit_to_tuple_variant() {
    let a = Status::Active;
    let b = Status::Pending(5);
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_enums_tuple_variants_same_content() {
    let a = Message::Text("hello".to_string());
    let b = Message::Text("hello".to_string());
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_enums_tuple_variants_different_content() {
    let a = Message::Text("hello".to_string());
    let b = Message::Text("world".to_string());
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_enums_different_tuple_variants() {
    let a = Message::Text("hello".to_string());
    let b = Message::Number(42);
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_enums_struct_variant_same() {
    let a = DetailedStatus::Active { since: 2020 };
    let b = DetailedStatus::Active { since: 2020 };
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_enums_struct_variant_different_field() {
    let a = DetailedStatus::Active { since: 2020 };
    let b = DetailedStatus::Active { since: 2024 };
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_enums_different_struct_variants() {
    let a = DetailedStatus::Active { since: 2020 };
    let b = DetailedStatus::Inactive {
        reason: "maintenance".to_string(),
    };
    assert_snapshot!(format_diff_comparison(&a, &b));
}

// ============================================================================
// Sequence diff tests
// ============================================================================

#[test]
fn diff_vecs_equal() {
    let a = vec![1, 2, 3];
    let b = vec![1, 2, 3];
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_vecs_one_element_different() {
    let a = vec![1, 2, 3];
    let b = vec![1, 2, 4];
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_vecs_element_added() {
    let a = vec![1, 2, 3];
    let b = vec![1, 2, 3, 4];
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_vecs_element_removed() {
    let a = vec![1, 2, 3, 4];
    let b = vec![1, 2, 3];
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_vecs_element_inserted_middle() {
    let a = vec![1, 2, 4];
    let b = vec![1, 2, 3, 4];
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_vecs_completely_different() {
    let a = vec![1, 2, 3];
    let b = vec![4, 5, 6];
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_vecs_reordered() {
    let a = vec![1, 2, 3];
    let b = vec![3, 1, 2];
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_arrays_equal() {
    let a = [1, 2, 3];
    let b = [1, 2, 3];
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_arrays_different() {
    let a = [1, 2, 3];
    let b = [1, 5, 3];
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_slices() {
    let a = [1, 2, 3];
    let b = [1, 2, 4];
    let a_slice: &[i32] = &a;
    let b_slice: &[i32] = &b;
    assert_snapshot!(format_diff_comparison(&a_slice, &b_slice));
}

#[test]
fn diff_vec_vs_array() {
    let a = vec![1, 2, 3];
    let b = [1, 2, 3];
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_vec_of_structs() {
    let a = vec![
        User {
            name: "Alice",
            age: 30,
        },
        User {
            name: "Bob",
            age: 25,
        },
    ];
    let b = vec![
        User {
            name: "Alice",
            age: 31,
        },
        User {
            name: "Bob",
            age: 25,
        },
    ];
    assert_snapshot!(format_diff_comparison(&a, &b));
}

// ============================================================================
// Tuple diff tests
// ============================================================================

#[test]
fn diff_tuples_equal() {
    let a = (1, 2, 3);
    let b = (1, 2, 3);
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_tuples_one_element_different() {
    let a = (1, 2, 3);
    let b = (1, 5, 3);
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_tuples_different_sizes() {
    let a = (1, 2, 3);
    let b = (1, 2, 3, 4);
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_tuples_nested() {
    let a = ((1, 2), (3, 4));
    let b = ((1, 2), (3, 5));
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_tuples_nested_different_sizes() {
    let a = ((1, 2), (3, 4));
    let b = ((1, 2, 3), (4, 5));
    assert_snapshot!(format_diff_comparison(&a, &b));
}

// ============================================================================
// Option diff tests
// ============================================================================

#[test]
fn diff_options_both_some_equal() {
    let a = Some(42);
    let b = Some(42);
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_options_both_some_different() {
    let a = Some(42);
    let b = Some(100);
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_options_some_vs_none() {
    let a = Some(42);
    let b: Option<i32> = None;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_options_none_vs_some() {
    let a: Option<i32> = None;
    let b = Some(42);
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_options_both_none() {
    let a: Option<i32> = None;
    let b: Option<i32> = None;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_options_nested_structs() {
    let a = Some(User {
        name: "Alice",
        age: 30,
    });
    let b = Some(User {
        name: "Alice",
        age: 31,
    });
    assert_snapshot!(format_diff_comparison(&a, &b));
}

// ============================================================================
// Value vs Value tests (dynamic values)
// ============================================================================

#[test]
fn diff_value_null_equal() {
    let a = value!(null);
    let b = value!(null);
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_value_bool_equal() {
    let a = value!(true);
    let b = value!(true);
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_value_bool_different() {
    let a = value!(true);
    let b = value!(false);
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_value_number_equal() {
    let a = value!(42);
    let b = value!(42);
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_value_number_different() {
    let a = value!(42);
    let b = value!(100);
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_value_string_equal() {
    let a = value!("hello");
    let b = value!("hello");
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_value_string_different() {
    let a = value!("hello");
    let b = value!("world");
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_value_array_equal() {
    let a = value!([1, 2, 3]);
    let b = value!([1, 2, 3]);
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_value_array_different() {
    let a = value!([1, 2, 3]);
    let b = value!([1, 2, 4]);
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_value_array_element_added() {
    let a = value!([1, 2, 3]);
    let b = value!([1, 2, 3, 4]);
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_value_object_equal() {
    let a = value!({"name": "Alice", "age": 30});
    let b = value!({"name": "Alice", "age": 30});
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_value_object_field_different() {
    let a = value!({"name": "Alice", "age": 30});
    let b = value!({"name": "Bob", "age": 30});
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_value_object_field_added() {
    let a = value!({"name": "Alice"});
    let b = value!({"name": "Alice", "age": 30});
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_value_object_field_removed() {
    let a = value!({"name": "Alice", "age": 30});
    let b = value!({"name": "Alice"});
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_value_nested_object() {
    let a = value!({
        "user": {
            "name": "Alice",
            "address": {
                "city": "Wonderland"
            }
        }
    });
    let b = value!({
        "user": {
            "name": "Alice",
            "address": {
                "city": "Elsewhere"
            }
        }
    });
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_value_different_types() {
    let a = value!(42);
    let b = value!("42");
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_value_null_vs_value() {
    let a = value!(null);
    let b = value!(42);
    assert_snapshot!(format_diff_comparison(&a, &b));
}

// ============================================================================
// Value vs T and T vs Value tests (cross-type comparisons)
// ============================================================================

#[test]
fn diff_value_number_vs_i32() {
    let a = value!(42);
    let b: i32 = 42;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_i32_vs_value_number() {
    let a: i32 = 42;
    let b = value!(42);
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_value_string_vs_string() {
    let a = value!("hello");
    let b = String::from("hello");
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_string_vs_value_string() {
    let a = String::from("hello");
    let b = value!("hello");
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_value_array_vs_vec() {
    let a = value!([1, 2, 3]);
    let b: Vec<i64> = vec![1, 2, 3];
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_vec_vs_value_array() {
    let a: Vec<i64> = vec![1, 2, 3];
    let b = value!([1, 2, 3]);
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_value_array_vs_vec_different() {
    let a = value!([1, 2, 3]);
    let b: Vec<i64> = vec![1, 2, 4];
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_value_bool_vs_bool() {
    let a = value!(true);
    let b = true;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_bool_vs_value_bool() {
    let a = true;
    let b = value!(true);
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_value_bool_vs_bool_different() {
    let a = value!(true);
    let b = false;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

// ============================================================================
// Edge cases and complex scenarios
// ============================================================================

#[test]
fn diff_empty_vecs() {
    let a: Vec<i32> = vec![];
    let b: Vec<i32> = vec![];
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_empty_to_nonempty_vec() {
    let a: Vec<i32> = vec![];
    let b = vec![1, 2, 3];
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_nonempty_to_empty_vec() {
    let a = vec![1, 2, 3];
    let b: Vec<i32> = vec![];
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_value_empty_array() {
    let a = value!([]);
    let b = value!([]);
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_value_empty_object() {
    let a = value!({});
    let b = value!({});
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_deeply_nested_structs() {
    #[derive(Facet)]
    struct Level3 {
        value: i32,
    }

    #[derive(Facet)]
    struct Level2 {
        inner: Level3,
    }

    #[derive(Facet)]
    struct Level1 {
        inner: Level2,
    }

    let a = Level1 {
        inner: Level2 {
            inner: Level3 { value: 1 },
        },
    };
    let b = Level1 {
        inner: Level2 {
            inner: Level3 { value: 2 },
        },
    };
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_vec_with_many_changes() {
    let a = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    let b = vec![1, 20, 3, 40, 5, 60, 7, 80, 9, 100];
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_struct_with_vec_field() {
    #[derive(Facet)]
    struct Container {
        items: Vec<i32>,
    }

    let a = Container {
        items: vec![1, 2, 3],
    };
    let b = Container {
        items: vec![1, 2, 4],
    };
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_struct_with_option_field() {
    #[derive(Facet)]
    struct MaybeUser<'a> {
        name: &'a str,
        nickname: Option<&'a str>,
    }

    let a = MaybeUser {
        name: "Alice",
        nickname: Some("Ali"),
    };
    let b = MaybeUser {
        name: "Alice",
        nickname: None,
    };
    assert_snapshot!(format_diff_comparison(&a, &b));
}

// ============================================================================
// Comprehensive shape matrix tests
// T/T, Dynamic/T, T/Dynamic, Dynamic/Dynamic for various types
// ============================================================================

// --- Structs/Objects ---

#[test]
fn diff_value_object_vs_value_object_nested_change() {
    // Dynamic/Dynamic with nested field change
    let a = value!({"user": {"name": "Alice", "age": 30}});
    let b = value!({"user": {"name": "Alice", "age": 31}});
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_value_object_vs_value_object_field_type_change() {
    // Dynamic/Dynamic with type change in field
    let a = value!({"count": 42});
    let b = value!({"count": "forty-two"});
    assert_snapshot!(format_diff_comparison(&a, &b));
}

// --- Sequences with nested diffs ---

#[test]
fn diff_value_array_nested_objects() {
    // Dynamic/Dynamic - array of objects with changes
    let a = value!([{"id": 1, "name": "Alice"}, {"id": 2, "name": "Bob"}]);
    let b = value!([{"id": 1, "name": "Alicia"}, {"id": 2, "name": "Bob"}]);
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_vec_of_values() {
    // T/Dynamic - Vec containing Value items
    let a: Vec<facet_value::Value> = vec![value!(1), value!(2), value!(3)];
    let b: Vec<facet_value::Value> = vec![value!(1), value!(20), value!(3)];
    assert_snapshot!(format_diff_comparison(&a, &b));
}

// --- Mixed scalar comparisons ---

#[test]
fn diff_value_number_vs_i32_different() {
    // Dynamic/T with different values
    let a = value!(42);
    let b: i32 = 100;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_i32_vs_value_number_different() {
    // T/Dynamic with different values
    let a: i32 = 42;
    let b = value!(100);
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_value_string_vs_str_different() {
    // Dynamic/T with different strings
    let a = value!("hello");
    let b = "world";
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_str_vs_value_string_different() {
    // T/Dynamic with different strings
    let a = "hello";
    let b = value!("world");
    assert_snapshot!(format_diff_comparison(&a, &b));
}

// --- Type mismatches ---

#[test]
fn diff_value_number_vs_string() {
    // Dynamic/T type mismatch
    let a = value!(42);
    let b = "42";
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_string_vs_value_number() {
    // T/Dynamic type mismatch
    let a = "42";
    let b = value!(42);
    assert_snapshot!(format_diff_comparison(&a, &b));
}

// --- Sequences cross-type ---

#[test]
fn diff_value_array_vs_vec_element_change() {
    // Dynamic/T with element changes
    let a = value!([1, 2, 3, 4, 5]);
    let b: Vec<i64> = vec![1, 2, 30, 4, 5];
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_vec_vs_value_array_element_added() {
    // T/Dynamic with additions
    let a: Vec<i64> = vec![1, 2, 3];
    let b = value!([1, 2, 3, 4, 5]);
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_vec_vs_value_array_element_removed() {
    // T/Dynamic with removals
    let a: Vec<i64> = vec![1, 2, 3, 4, 5];
    let b = value!([1, 2, 3]);
    assert_snapshot!(format_diff_comparison(&a, &b));
}

// ============================================================================
// Complex/stress tests - big structs, deep nesting, large collections
// ============================================================================

#[derive(Facet, Debug, PartialEq)]
struct BigConfig {
    name: String,
    version: u32,
    enabled: bool,
    max_connections: u64,
    timeout_ms: u32,
    retry_count: u8,
    debug_mode: bool,
    log_level: String,
    cache_size: usize,
    description: String,
}

#[test]
fn diff_big_struct_multiple_changes() {
    let a = BigConfig {
        name: "my-service".to_string(),
        version: 1,
        enabled: true,
        max_connections: 100,
        timeout_ms: 5000,
        retry_count: 3,
        debug_mode: false,
        log_level: "info".to_string(),
        cache_size: 1024,
        description: "A cool service".to_string(),
    };
    let b = BigConfig {
        name: "my-service".to_string(),
        version: 2, // changed
        enabled: true,
        max_connections: 200, // changed
        timeout_ms: 10000,    // changed
        retry_count: 3,
        debug_mode: true,               // changed
        log_level: "debug".to_string(), // changed
        cache_size: 1024,
        description: "An even cooler service".to_string(), // changed
    };
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[derive(Facet, Debug, PartialEq)]
struct Level4 {
    value: i32,
    name: String,
}

#[derive(Facet, Debug, PartialEq)]
struct Level3 {
    inner: Level4,
    count: u32,
}

#[derive(Facet, Debug, PartialEq)]
struct Level2 {
    inner: Level3,
    enabled: bool,
}

#[derive(Facet, Debug, PartialEq)]
struct Level1 {
    inner: Level2,
    label: String,
}

#[test]
fn diff_four_level_nesting() {
    let a = Level1 {
        inner: Level2 {
            inner: Level3 {
                inner: Level4 {
                    value: 42,
                    name: "original".to_string(),
                },
                count: 10,
            },
            enabled: true,
        },
        label: "root".to_string(),
    };
    let b = Level1 {
        inner: Level2 {
            inner: Level3 {
                inner: Level4 {
                    value: 100,                   // changed deep inside
                    name: "modified".to_string(), // changed deep inside
                },
                count: 10,
            },
            enabled: false, // changed at level 2
        },
        label: "root".to_string(),
    };
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_large_vec_scattered_changes() {
    let a: Vec<i32> = (0..20).collect();
    let mut b: Vec<i32> = (0..20).collect();
    b[3] = 300; // change at index 3
    b[7] = 700; // change at index 7
    b[15] = 1500; // change at index 15
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_large_vec_with_insertions_and_deletions() {
    let a: Vec<i32> = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    let b: Vec<i32> = vec![1, 2, 100, 101, 5, 6, 8, 9, 10, 11, 12];
    // removed: 3, 4, 7
    // added: 100, 101, 11, 12
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[derive(Facet, Debug, PartialEq)]
struct Employee {
    id: u32,
    name: String,
    department: String,
    salary: u64,
}

#[derive(Facet, Debug, PartialEq)]
struct Team {
    name: String,
    lead: Employee,
    members: Vec<Employee>,
}

#[test]
fn diff_struct_with_vec_of_structs_complex() {
    let a = Team {
        name: "Engineering".to_string(),
        lead: Employee {
            id: 1,
            name: "Alice".to_string(),
            department: "Engineering".to_string(),
            salary: 150000,
        },
        members: vec![
            Employee {
                id: 2,
                name: "Bob".to_string(),
                department: "Engineering".to_string(),
                salary: 120000,
            },
            Employee {
                id: 3,
                name: "Charlie".to_string(),
                department: "Engineering".to_string(),
                salary: 110000,
            },
            Employee {
                id: 4,
                name: "Diana".to_string(),
                department: "Engineering".to_string(),
                salary: 130000,
            },
        ],
    };
    let b = Team {
        name: "Engineering".to_string(),
        lead: Employee {
            id: 1,
            name: "Alice".to_string(),
            department: "Engineering".to_string(),
            salary: 160000, // got a raise
        },
        members: vec![
            Employee {
                id: 2,
                name: "Bob".to_string(),
                department: "Engineering".to_string(),
                salary: 125000, // got a raise
            },
            // Charlie left
            Employee {
                id: 4,
                name: "Diana".to_string(),
                department: "Engineering".to_string(),
                salary: 135000, // got a raise
            },
            Employee {
                id: 5,
                name: "Eve".to_string(), // new hire
                department: "Engineering".to_string(),
                salary: 115000,
            },
        ],
    };
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[derive(Facet, Debug, PartialEq)]
struct NestedOptions {
    outer: Option<Option<Option<i32>>>,
}

#[test]
fn diff_triple_nested_option() {
    let a = NestedOptions {
        outer: Some(Some(Some(42))),
    };
    let b = NestedOptions {
        outer: Some(Some(Some(100))),
    };
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_value_deeply_nested_object() {
    let a = value!({
        "level1": {
            "level2": {
                "level3": {
                    "level4": {
                        "value": 42,
                        "name": "deep"
                    }
                }
            }
        }
    });
    let b = value!({
        "level1": {
            "level2": {
                "level3": {
                    "level4": {
                        "value": 100,
                        "name": "deep"
                    }
                }
            }
        }
    });
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_value_array_of_objects_complex() {
    let a = value!([
        {"id": 1, "name": "Alice", "active": true},
        {"id": 2, "name": "Bob", "active": true},
        {"id": 3, "name": "Charlie", "active": false},
    ]);
    let b = value!([
        {"id": 1, "name": "Alicia", "active": true},
        {"id": 2, "name": "Bob", "active": false},
        {"id": 4, "name": "Diana", "active": true},
    ]);
    assert_snapshot!(format_diff_comparison(&a, &b));
}

// ============================================================================
// Value vs Native Type comparisons
// ============================================================================

#[test]
fn diff_value_array_vs_native_array_equal() {
    let a = value!([1, 2, 3]);
    let b: [i64; 3] = [1, 2, 3];
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_native_array_vs_value_array_equal() {
    let a: [i64; 3] = [1, 2, 3];
    let b = value!([1, 2, 3]);
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_value_array_vs_native_array_different() {
    let a = value!([1, 2, 3]);
    let b: [i64; 3] = [1, 2, 100];
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_value_array_vs_native_array_different_length() {
    let a = value!([1, 2, 3, 4, 5]);
    let b: [i64; 3] = [1, 2, 3];
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_value_object_vs_struct_equal() {
    let a = value!({"name": "Alice", "age": 30});
    let b = OwnedUser {
        name: "Alice".to_string(),
        age: 30,
    };
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_struct_vs_value_object_equal() {
    let a = OwnedUser {
        name: "Alice".to_string(),
        age: 30,
    };
    let b = value!({"name": "Alice", "age": 30});
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_value_object_vs_struct_different() {
    let a = value!({"name": "Alice", "age": 30});
    let b = OwnedUser {
        name: "Bob".to_string(),
        age: 25,
    };
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_value_nested_vs_nested_struct() {
    let a = value!({
        "name": "Alice",
        "age": 30,
        "address": {
            "street": "123 Main St",
            "city": "Wonderland"
        }
    });
    let b = UserWithAddress {
        name: "Alice",
        age: 30,
        address: Address {
            street: "123 Main St".to_string(),
            city: "Wonderland".to_string(),
        },
    };
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_value_nested_vs_nested_struct_different() {
    let a = value!({
        "name": "Alice",
        "age": 30,
        "address": {
            "street": "123 Main St",
            "city": "Wonderland"
        }
    });
    let b = UserWithAddress {
        name: "Alice",
        age: 31,
        address: Address {
            street: "456 Oak Ave".to_string(),
            city: "Wonderland".to_string(),
        },
    };
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_value_array_of_objects_vs_vec_of_structs() {
    let a = value!([
        {"name": "Alice", "age": 30},
        {"name": "Bob", "age": 25}
    ]);
    let b: Vec<OwnedUser> = vec![
        OwnedUser {
            name: "Alice".to_string(),
            age: 30,
        },
        OwnedUser {
            name: "Bob".to_string(),
            age: 25,
        },
    ];
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_value_array_of_objects_vs_vec_of_structs_different() {
    let a = value!([
        {"name": "Alice", "age": 30},
        {"name": "Bob", "age": 25}
    ]);
    let b: Vec<OwnedUser> = vec![
        OwnedUser {
            name: "Alice".to_string(),
            age: 31,
        },
        OwnedUser {
            name: "Charlie".to_string(),
            age: 35,
        },
    ];
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_value_mixed_array_vs_tuple() {
    let a = value!([1, "hello", true]);
    let b: (i64, &str, bool) = (1, "hello", true);
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_value_mixed_array_vs_tuple_different() {
    let a = value!([1, "hello", true]);
    let b: (i64, &str, bool) = (2, "world", false);
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_value_number_vs_float() {
    let a = value!(2.14);
    let b: f64 = 2.14;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_value_number_vs_float_different() {
    let a = value!(2.14);
    let b: f64 = 2.71;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_value_null_vs_none() {
    let a = value!(null);
    let b: Option<i32> = None;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_value_vs_some() {
    let a = value!(42);
    let b: Option<i64> = Some(42);
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_value_vs_some_different() {
    let a = value!(42);
    let b: Option<i64> = Some(100);
    assert_snapshot!(format_diff_comparison(&a, &b));
}

// ============================================================================
// COMPREHENSIVE SCALAR TYPE TESTS
// ============================================================================
// Tests for all primitive/scalar types to ensure consistent diff behavior

// --- Signed integers ---

#[test]
fn diff_scalar_i8_equal() {
    let a: i8 = 42;
    let b: i8 = 42;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_i8_different() {
    let a: i8 = 42;
    let b: i8 = -42;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_i8_min_max() {
    let a: i8 = i8::MIN;
    let b: i8 = i8::MAX;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_i16_equal() {
    let a: i16 = 1000;
    let b: i16 = 1000;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_i16_different() {
    let a: i16 = 1000;
    let b: i16 = -1000;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_i16_min_max() {
    let a: i16 = i16::MIN;
    let b: i16 = i16::MAX;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_i32_equal() {
    let a: i32 = 100000;
    let b: i32 = 100000;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_i32_different() {
    let a: i32 = 100000;
    let b: i32 = -100000;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_i32_min_max() {
    let a: i32 = i32::MIN;
    let b: i32 = i32::MAX;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_i64_equal() {
    let a: i64 = 10000000000;
    let b: i64 = 10000000000;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_i64_different() {
    let a: i64 = 10000000000;
    let b: i64 = -10000000000;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_i64_min_max() {
    let a: i64 = i64::MIN;
    let b: i64 = i64::MAX;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_i128_equal() {
    let a: i128 = 100000000000000000000;
    let b: i128 = 100000000000000000000;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_i128_different() {
    let a: i128 = 100000000000000000000;
    let b: i128 = -100000000000000000000;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_i128_min_max() {
    let a: i128 = i128::MIN;
    let b: i128 = i128::MAX;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_isize_equal() {
    let a: isize = 1000;
    let b: isize = 1000;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_isize_different() {
    let a: isize = 1000;
    let b: isize = -1000;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

// --- Unsigned integers ---

#[test]
fn diff_scalar_u8_equal() {
    let a: u8 = 200;
    let b: u8 = 200;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_u8_different() {
    let a: u8 = 0;
    let b: u8 = 255;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_u8_min_max() {
    let a: u8 = u8::MIN;
    let b: u8 = u8::MAX;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_u16_equal() {
    let a: u16 = 50000;
    let b: u16 = 50000;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_u16_different() {
    let a: u16 = 0;
    let b: u16 = 65535;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_u16_min_max() {
    let a: u16 = u16::MIN;
    let b: u16 = u16::MAX;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_u32_equal() {
    let a: u32 = 3000000000;
    let b: u32 = 3000000000;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_u32_different() {
    let a: u32 = 0;
    let b: u32 = 4000000000;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_u32_min_max() {
    let a: u32 = u32::MIN;
    let b: u32 = u32::MAX;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_u64_equal() {
    let a: u64 = 10000000000000000000;
    let b: u64 = 10000000000000000000;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_u64_different() {
    let a: u64 = 0;
    let b: u64 = 18000000000000000000;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_u64_min_max() {
    let a: u64 = u64::MIN;
    let b: u64 = u64::MAX;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_u128_equal() {
    let a: u128 = 200000000000000000000000000000000000000;
    let b: u128 = 200000000000000000000000000000000000000;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_u128_different() {
    let a: u128 = 0;
    let b: u128 = 340000000000000000000000000000000000000;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_u128_min_max() {
    let a: u128 = u128::MIN;
    let b: u128 = u128::MAX;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_usize_equal() {
    let a: usize = 1000;
    let b: usize = 1000;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_usize_different() {
    let a: usize = 0;
    let b: usize = 1000000;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

// --- Floating point ---

#[test]
fn diff_scalar_f32_equal() {
    let a: f32 = std::f32::consts::PI;
    let b: f32 = std::f32::consts::PI;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_f32_different() {
    let a: f32 = std::f32::consts::PI;
    let b: f32 = std::f32::consts::E;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_f32_zero_vs_negative_zero() {
    let a: f32 = 0.0;
    let b: f32 = -0.0;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_f32_infinity() {
    let a: f32 = f32::INFINITY;
    let b: f32 = f32::NEG_INFINITY;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_f32_nan() {
    let a: f32 = f32::NAN;
    let b: f32 = f32::NAN;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_f64_equal() {
    let a: f64 = std::f64::consts::PI;
    let b: f64 = std::f64::consts::PI;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_f64_different() {
    let a: f64 = std::f64::consts::PI;
    let b: f64 = std::f64::consts::E;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_f64_zero_vs_negative_zero() {
    let a: f64 = 0.0;
    let b: f64 = -0.0;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_f64_infinity() {
    let a: f64 = f64::INFINITY;
    let b: f64 = f64::NEG_INFINITY;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_f64_nan() {
    let a: f64 = f64::NAN;
    let b: f64 = f64::NAN;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_f64_very_small() {
    let a: f64 = 1e-300;
    let b: f64 = 1e-308;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_f64_very_large() {
    let a: f64 = 1e300;
    let b: f64 = 1e308;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

// --- Boolean ---

#[test]
fn diff_scalar_bool_equal_true() {
    let a: bool = true;
    let b: bool = true;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_bool_equal_false() {
    let a: bool = false;
    let b: bool = false;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_bool_true_to_false() {
    let a: bool = true;
    let b: bool = false;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_bool_false_to_true() {
    let a: bool = false;
    let b: bool = true;
    assert_snapshot!(format_diff_comparison(&a, &b));
}

// --- Character ---

#[test]
fn diff_scalar_char_equal() {
    let a: char = 'A';
    let b: char = 'A';
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_char_different() {
    let a: char = 'A';
    let b: char = 'Z';
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_char_unicode() {
    let a: char = '🦀';
    let b: char = '🐍';
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_char_ascii_to_unicode() {
    let a: char = 'R';
    let b: char = '日';
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_char_newline() {
    let a: char = '\n';
    let b: char = '\t';
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_char_null() {
    let a: char = '\0';
    let b: char = 'X';
    assert_snapshot!(format_diff_comparison(&a, &b));
}

// --- Unit type ---

#[test]
fn diff_scalar_unit_equal() {
    let a: () = ();
    let b: () = ();
    assert_snapshot!(format_diff_comparison(&a, &b));
}

// --- Strings ---

#[test]
fn diff_scalar_str_equal() {
    let a: &str = "hello";
    let b: &str = "hello";
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_str_different() {
    let a: &str = "hello";
    let b: &str = "world";
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_str_empty_vs_nonempty() {
    let a: &str = "";
    let b: &str = "hello";
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_str_unicode() {
    let a: &str = "Hello 世界";
    let b: &str = "Hello 🌍";
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_str_multiline() {
    let a: &str = "line1\nline2";
    let b: &str = "line1\nline3";
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_str_whitespace() {
    let a: &str = "hello world";
    let b: &str = "hello  world";
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_string_equal() {
    let a: String = "hello".to_string();
    let b: String = "hello".to_string();
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_string_different() {
    let a: String = "hello".to_string();
    let b: String = "world".to_string();
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_str_vs_string_equal() {
    let a: &str = "hello";
    let b: String = "hello".to_string();
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_str_vs_string_different() {
    let a: &str = "hello";
    let b: String = "world".to_string();
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_cow_str_borrowed_equal() {
    use std::borrow::Cow;
    let a: Cow<str> = Cow::Borrowed("hello");
    let b: Cow<str> = Cow::Borrowed("hello");
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_cow_str_owned_equal() {
    use std::borrow::Cow;
    let a: Cow<str> = Cow::Owned("hello".to_string());
    let b: Cow<str> = Cow::Owned("hello".to_string());
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_cow_str_borrowed_vs_owned() {
    use std::borrow::Cow;
    let a: Cow<str> = Cow::Borrowed("hello");
    let b: Cow<str> = Cow::Owned("hello".to_string());
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_cow_str_different() {
    use std::borrow::Cow;
    let a: Cow<str> = Cow::Borrowed("hello");
    let b: Cow<str> = Cow::Borrowed("world");
    assert_snapshot!(format_diff_comparison(&a, &b));
}

// --- Byte slices ---

#[test]
fn diff_scalar_byte_slice_equal() {
    let a: &[u8] = b"hello";
    let b: &[u8] = b"hello";
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_byte_slice_different() {
    let a: &[u8] = b"hello";
    let b: &[u8] = b"world";
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_byte_slice_empty_vs_nonempty() {
    let a: &[u8] = b"";
    let b: &[u8] = b"data";
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_byte_slice_binary() {
    let a: &[u8] = &[0x00, 0xFF, 0x42];
    let b: &[u8] = &[0x00, 0xFE, 0x42];
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_vec_u8_equal() {
    let a: Vec<u8> = vec![1, 2, 3, 4, 5];
    let b: Vec<u8> = vec![1, 2, 3, 4, 5];
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_vec_u8_different() {
    let a: Vec<u8> = vec![1, 2, 3, 4, 5];
    let b: Vec<u8> = vec![1, 2, 99, 4, 5];
    assert_snapshot!(format_diff_comparison(&a, &b));
}

#[test]
fn diff_scalar_byte_slice_vs_vec_u8() {
    let a: &[u8] = &[1, 2, 3];
    let b: Vec<u8> = vec![1, 2, 3];
    assert_snapshot!(format_diff_comparison(&a, &b));
}
