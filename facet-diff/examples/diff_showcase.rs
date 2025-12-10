//! facet-diff Showcase
//!
//! Demonstrates all diffing capabilities in one place.
//!
//! Run with: cargo run -p facet-diff --example diff_showcase
//! Filter:   cargo run -p facet-diff --example diff_showcase -- confusable scalar

#![allow(dead_code)]

use facet::Facet;
use facet_diff::FacetDiff;

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
    println!("{}\n", diff.format_default());

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
    println!("{}", diff.format_default());
}

fn showcase_sequences() {
    println!("a) Single element change:");
    let old: Vec<i32> = vec![1, 2, 3, 4, 5];
    let new: Vec<i32> = vec![1, 2, 99, 4, 5];
    println!("{}\n", old.diff(&new).format_default());

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
    println!("{}\n", old.diff(&new).format_default());

    println!("b) Different variants:");
    let old = Status::Active;
    let new = Status::Pending { since: 42 };
    println!("{}", old.diff(&new).format_default());
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
    println!("{}\n", old.diff(&new).format_default());

    println!("b) None to Some:");
    let old: Option<i32> = None;
    let new: Option<i32> = Some(42);
    println!("{}", old.diff(&new).format_default());
}

fn showcase_many_changes() {
    let old: Vec<i32> = (0..30).collect();
    let mut new = old.clone();
    for i in (0..30).step_by(2) {
        new[i] *= 100;
    }
    println!("{}", old.diff(&new).format_default());
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
    println!("{}", val.diff(&val.clone()).format_default());
}

fn showcase_scalar_types() {
    println!("a) Integers:");
    let old: i32 = 42;
    let new: i32 = -42;
    println!("  i32: {}", old.diff(&new).format_default());

    let old: i128 = i128::MIN;
    let new: i128 = i128::MAX;
    println!("  i128 min→max: {}", old.diff(&new).format_default());

    let old: u64 = 0;
    let new: u64 = u64::MAX;
    println!("  u64 0→max: {}", old.diff(&new).format_default());

    println!("\nb) Floats:");
    let old: f64 = std::f64::consts::PI;
    let new: f64 = std::f64::consts::E;
    println!("  f64: {}", old.diff(&new).format_default());

    let old: f64 = f64::INFINITY;
    let new: f64 = f64::NEG_INFINITY;
    println!("  f64 inf→-inf: {}", old.diff(&new).format_default());

    let old: f64 = f64::NAN;
    let new: f64 = f64::NAN;
    println!("  f64 NaN→NaN: {}", old.diff(&new).format_default());

    println!("\nc) Booleans:");
    let old: bool = true;
    let new: bool = false;
    println!("  bool: {}", old.diff(&new).format_default());

    println!("\nd) Characters:");
    let old: char = 'A';
    let new: char = 'Z';
    println!("  char: {}", old.diff(&new).format_default());

    let old: char = '🦀';
    let new: char = '🐍';
    println!("  emoji: {}", old.diff(&new).format_default());

    println!("\ne) Strings:");
    let old: &str = "hello";
    let new: &str = "world";
    println!("  &str: {}", old.diff(&new).format_default());

    let old: String = "Hello 世界".into();
    let new: String = "Hello 🌍".into();
    println!("  String unicode: {}", old.diff(&new).format_default());
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
    println!("  {}", old.diff(&new).format_default());
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
