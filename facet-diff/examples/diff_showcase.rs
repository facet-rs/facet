//! facet-diff Showcase
//!
//! Demonstrates all diffing capabilities in one place.
//!
//! Run with: cargo run -p facet-diff --example diff_showcase
//! Filter:   cargo run -p facet-diff --example diff_showcase -- confusable scalar

#![allow(dead_code)]

use facet::Facet;
use facet_diff::{FacetDiff, format_diff_default};

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
// Showcase infrastructure
// ============================================================================

struct Showcase {
    name: &'static str,
    run: fn(),
}

impl Showcase {
    fn matches(&self, filter: &str) -> bool {
        let name_lower = self.name.to_lowercase();
        let filter_lower = filter.to_lowercase();
        name_lower.contains(&filter_lower)
    }
}

fn print_section(title: &str) {
    println!("\n{}", "─".repeat(60));
    println!("{title}");
    println!("{}\n", "─".repeat(60));
}

// ============================================================================
// Showcases
// ============================================================================

fn showcase_struct_fields() {
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

    println!("Compact format:");
    println!("{}\n", format_diff_default(&diff));

    println!("Tree format:");
    println!("{diff}");
}

fn showcase_nested_structures() {
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
                heading: "Introduction".into(), // changed
                content: "Hello world".into(),
            },
            Section {
                heading: "Body".into(),
                content: "Updated content".into(), // changed
            },
        ],
    };

    let diff = old.diff(&new);
    println!("{}", format_diff_default(&diff));
}

fn showcase_sequences() {
    println!("a) Single element change:");
    let old: Vec<i32> = vec![1, 2, 3, 4, 5];
    let new: Vec<i32> = vec![1, 2, 99, 4, 5];
    println!("{}\n", format_diff_default(&old.diff(&new)));

    println!("b) Insertions and deletions:");
    let old: Vec<i32> = vec![1, 2, 3];
    let new: Vec<i32> = vec![1, 4, 5, 3];
    println!("{}\n", old.diff(&new));

    println!("c) Reordering:");
    let old: Vec<&str> = vec!["a", "b", "c"];
    let new: Vec<&str> = vec!["c", "a", "b"];
    println!("{}", old.diff(&new));
}

fn showcase_enums() {
    println!("a) Same variant, different data:");
    let old = Status::Inactive {
        reason: "vacation".into(),
    };
    let new = Status::Inactive {
        reason: "sick leave".into(),
    };
    println!("{}\n", format_diff_default(&old.diff(&new)));

    println!("b) Different variants:");
    let old = Status::Active;
    let new = Status::Pending { since: 42 };
    println!("{}", format_diff_default(&old.diff(&new)));
}

fn showcase_options() {
    println!("a) Some to Some (inner change):");
    let old: Option<User> = Some(User {
        name: "Bob".into(),
        email: "bob@example.com".into(),
        age: 25,
        settings: Settings {
            theme: "auto".into(),
            notifications: false,
        },
    });
    let new: Option<User> = Some(User {
        name: "Bob".into(),
        email: "bob@company.com".into(),
        age: 25,
        settings: Settings {
            theme: "auto".into(),
            notifications: true,
        },
    });
    println!("{}\n", format_diff_default(&old.diff(&new)));

    println!("b) None to Some:");
    let old: Option<i32> = None;
    let new: Option<i32> = Some(42);
    println!("{}", format_diff_default(&old.diff(&new)));
}

fn showcase_many_changes() {
    let old: Vec<i32> = (0..30).collect();
    let mut new = old.clone();
    for i in (0..30).step_by(2) {
        new[i] *= 100;
    }
    println!("{}", format_diff_default(&old.diff(&new)));
}

fn showcase_no_changes() {
    let val = User {
        name: "Alice".into(),
        email: "alice@example.com".into(),
        age: 30,
        settings: Settings {
            theme: "dark".into(),
            notifications: true,
        },
    };
    println!("{}", format_diff_default(&val.diff(&val.clone())));
}

fn showcase_scalar_types() {
    println!("a) Integers:");
    let old: i32 = 42;
    let new: i32 = -42;
    println!("  i32: {}", format_diff_default(&old.diff(&new)));

    let old: i128 = i128::MIN;
    let new: i128 = i128::MAX;
    println!("  i128 min→max: {}", format_diff_default(&old.diff(&new)));

    let old: u64 = 0;
    let new: u64 = u64::MAX;
    println!("  u64 0→max: {}", format_diff_default(&old.diff(&new)));

    println!("\nb) Floats:");
    let old: f64 = std::f64::consts::PI;
    let new: f64 = std::f64::consts::E;
    println!("  f64: {}", format_diff_default(&old.diff(&new)));

    let old: f64 = f64::INFINITY;
    let new: f64 = f64::NEG_INFINITY;
    println!("  f64 inf→-inf: {}", format_diff_default(&old.diff(&new)));

    let old: f64 = f64::NAN;
    let new: f64 = f64::NAN;
    println!("  f64 NaN→NaN: {}", format_diff_default(&old.diff(&new)));

    println!("\nc) Booleans:");
    let old: bool = true;
    let new: bool = false;
    println!("  bool: {}", format_diff_default(&old.diff(&new)));

    println!("\nd) Characters:");
    let old: char = 'A';
    let new: char = 'Z';
    println!("  char: {}", format_diff_default(&old.diff(&new)));

    let old: char = '🦀';
    let new: char = '🐍';
    println!("  emoji: {}", format_diff_default(&old.diff(&new)));

    println!("\ne) Strings:");
    let old: &str = "hello";
    let new: &str = "world";
    println!("  &str: {}", format_diff_default(&old.diff(&new)));

    let old: String = "Hello 世界".into();
    let new: String = "Hello 🌍".into();
    println!("  String unicode: {}", format_diff_default(&old.diff(&new)));
}

fn showcase_confusables() {
    // The tree format (Display) shows confusable detection with Unicode codepoints
    // Detection uses the Unicode TR39 confusables database via the `confusables` crate

    // Latin 'a' vs Cyrillic 'а' (U+0430) - DETECTED as confusable
    println!("a) Latin 'a' vs Cyrillic 'а' (detected):");
    let old: &str = "abc";
    let new: &str = "аbc"; // first char is Cyrillic
    println!("{}", old.diff(&new));

    // Latin 'o' vs Greek 'ο' (U+03BF) - DETECTED as confusable
    println!("\nb) Latin 'o' vs Greek 'ο' (detected):");
    let old: &str = "foo";
    let new: &str = "fοo"; // middle char is Greek omicron
    println!("{}", old.diff(&new));

    // Latin 'e' vs Cyrillic 'е' (U+0435) - DETECTED as confusable
    println!("\nc) Latin 'e' vs Cyrillic 'е' (detected):");
    let old: &str = "hello";
    let new: &str = "hеllo"; // 'e' is Cyrillic
    println!("{}", old.diff(&new));

    // The following are NOT in the Unicode confusables database:

    // Zero-width characters - NOT detected (invisible char not in TR39)
    println!("\nd) With zero-width joiner (not in TR39):");
    let old: &str = "test";
    let new: &str = "te\u{200D}st"; // zero-width joiner in middle
    println!("{}", old.diff(&new));

    // Different quotes - NOT detected (curly quotes not in TR39)
    println!("\ne) Different quote styles (not in TR39):");
    let old: &str = "\"quoted\"";
    let new: &str = "\u{201C}quoted\u{201D}"; // curly quotes
    println!("{}", old.diff(&new));

    // Greek Iota vs Latin I - NOT detected (not in TR39)
    println!("\nf) Greek Iota vs Latin I (not in TR39):");
    let old: &str = "userId";
    let new: &str = "userΙd"; // Greek capital Iota (U+0399) vs Latin I (U+0049)
    println!("{}", old.diff(&new));
}

fn showcase_byte_slices() {
    println!("a) ASCII bytes:");
    let old: &[u8] = b"hello";
    let new: &[u8] = b"world";
    println!("  {}", old.diff(&new));

    println!("\nb) Binary data:");
    let old: &[u8] = &[0x00, 0xFF, 0x42, 0x13];
    let new: &[u8] = &[0x00, 0xFE, 0x42, 0x37];
    println!("  {}", old.diff(&new));

    println!("\nc) Vec<u8>:");
    let old: Vec<u8> = vec![1, 2, 3, 4, 5];
    let new: Vec<u8> = vec![1, 2, 99, 4, 5];
    println!("  {}", format_diff_default(&old.diff(&new)));
}

fn showcase_deep_tree() {
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

    println!("a) Change at deepest level (level 6):");
    let old = make_deep(42, "original", 10, true, "config", 1, "root");
    let new = make_deep(999, "modified", 10, true, "config", 1, "root");
    println!("{}", format_diff_default(&old.diff(&new)));

    println!("\nb) Changes at multiple levels (2, 4, 6):");
    let old = make_deep(42, "tag", 10, true, "config", 1, "root");
    let new = make_deep(100, "tag", 10, false, "config", 5, "root");
    println!("{}", format_diff_default(&old.diff(&new)));

    println!("\nc) Changes at every level:");
    let old = make_deep(1, "a", 10, true, "old", 1, "label-old");
    let new = make_deep(2, "b", 20, false, "new", 2, "label-new");
    println!("{}", format_diff_default(&old.diff(&new)));

    println!("\nd) Tree format for deep change:");
    let old = make_deep(42, "deep", 10, true, "config", 1, "root");
    let new = make_deep(999, "deep", 10, true, "config", 1, "root");
    println!("{}", old.diff(&new));
}

fn showcase_wide_tree() {
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

    println!("a) Single field change (among 20 fields):");
    let old = make_wide(
        "a", "b", "c", "d", "e", 1, 2, 3, 4, 5, true, true, true, true, true, 100, 200, 300, 400,
        500,
    );
    let new = make_wide(
        "a", "b", "c", "d", "e", 1, 2, 3, 4, 5, true, true, true, true, true, 100, 200, 999, 400,
        500,
    );
    println!("{}", format_diff_default(&old.diff(&new)));

    println!("\nb) Scattered changes (fields 2, 8, 14, 19):");
    let old = make_wide(
        "a", "b", "c", "d", "e", 1, 2, 3, 4, 5, true, true, true, true, true, 100, 200, 300, 400,
        500,
    );
    let new = make_wide(
        "a", "CHANGED", "c", "d", "e", 1, 2, 999, 4, 5, true, true, true, false, true, 100, 200,
        300, 888, 500,
    );
    println!("{}", format_diff_default(&old.diff(&new)));

    println!("\nc) Many changes (exceeds truncation limit):");
    let old = make_wide(
        "a", "b", "c", "d", "e", 1, 2, 3, 4, 5, true, true, true, true, true, 100, 200, 300, 400,
        500,
    );
    let new = make_wide(
        "A", "B", "C", "D", "E", 10, 20, 30, 40, 50, false, false, false, false, false, 1000, 2000,
        3000, 4000, 5000,
    );
    println!("{}", format_diff_default(&old.diff(&new)));

    println!("\nd) Tree format with few changes:");
    let old = make_wide(
        "a", "b", "c", "d", "e", 1, 2, 3, 4, 5, true, true, true, true, true, 100, 200, 300, 400,
        500,
    );
    let new = make_wide(
        "a", "b", "c", "d", "e", 1, 2, 3, 4, 5, true, false, true, true, true, 100, 200, 300, 400,
        500,
    );
    println!("{}", old.diff(&new));
}

// ============================================================================
// Registry
// ============================================================================

fn all_showcases() -> Vec<Showcase> {
    vec![
        Showcase {
            name: "1. Struct field changes",
            run: showcase_struct_fields,
        },
        Showcase {
            name: "2. Nested structures",
            run: showcase_nested_structures,
        },
        Showcase {
            name: "3. Sequences (lists/arrays)",
            run: showcase_sequences,
        },
        Showcase {
            name: "4. Enums",
            run: showcase_enums,
        },
        Showcase {
            name: "5. Options",
            run: showcase_options,
        },
        Showcase {
            name: "6. Many changes (truncated)",
            run: showcase_many_changes,
        },
        Showcase {
            name: "7. No changes",
            run: showcase_no_changes,
        },
        Showcase {
            name: "8. Scalar types",
            run: showcase_scalar_types,
        },
        Showcase {
            name: "9. Confusable strings",
            run: showcase_confusables,
        },
        Showcase {
            name: "10. Byte slices",
            run: showcase_byte_slices,
        },
        Showcase {
            name: "11. Deep tree (6 levels)",
            run: showcase_deep_tree,
        },
        Showcase {
            name: "12. Wide tree (20 fields)",
            run: showcase_wide_tree,
        },
    ]
}

// ============================================================================
// Main
// ============================================================================

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let showcases = all_showcases();

    // Filter showcases if args provided
    let filtered: Vec<&Showcase> = if args.is_empty() {
        showcases.iter().collect()
    } else {
        showcases
            .iter()
            .filter(|s| args.iter().any(|filter| s.matches(filter)))
            .collect()
    };

    if filtered.is_empty() {
        eprintln!("No showcases matched the filter(s): {:?}", args);
        eprintln!("\nAvailable showcases:");
        for s in &showcases {
            eprintln!("  - {}", s.name);
        }
        std::process::exit(1);
    }

    println!("facet-diff Showcase");
    println!("===================");

    if !args.is_empty() {
        println!("(filtered by: {:?})", args);
    }

    for showcase in filtered {
        print_section(showcase.name);
        (showcase.run)();
    }
}
