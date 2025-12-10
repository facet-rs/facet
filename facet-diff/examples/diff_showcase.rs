//! facet-diff Showcase
//!
//! Demonstrates all diffing capabilities in one place.
//!
//! Run with: cargo run -p facet-diff --example showcase

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
// Helpers
// ============================================================================

fn section(title: &str) {
    println!("\n{}", "─".repeat(60));
    println!("{title}");
    println!("{}\n", "─".repeat(60));
}

// ============================================================================
// Main
// ============================================================================

fn main() {
    println!("facet-diff Showcase");
    println!("===================\n");

    // -------------------------------------------------------------------------
    section("1. Struct field changes");
    // -------------------------------------------------------------------------
    {
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

    // -------------------------------------------------------------------------
    section("2. Nested structures");
    // -------------------------------------------------------------------------
    {
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

    // -------------------------------------------------------------------------
    section("3. Sequences (lists/arrays)");
    // -------------------------------------------------------------------------
    {
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

    // -------------------------------------------------------------------------
    section("4. Enums");
    // -------------------------------------------------------------------------
    {
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

    // -------------------------------------------------------------------------
    section("5. Options");
    // -------------------------------------------------------------------------
    {
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

    // -------------------------------------------------------------------------
    section("6. Many changes (truncated)");
    // -------------------------------------------------------------------------
    {
        let old: Vec<i32> = (0..30).collect();
        let mut new = old.clone();
        for i in (0..30).step_by(2) {
            new[i] *= 100;
        }
        println!("{}", old.diff(&new).format_default());
    }

    // -------------------------------------------------------------------------
    section("7. No changes");
    // -------------------------------------------------------------------------
    {
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

    // -------------------------------------------------------------------------
    section("8. Scalar types");
    // -------------------------------------------------------------------------
    {
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
        let old: f64 = 3.141592653589793;
        let new: f64 = 2.718281828459045;
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

    // -------------------------------------------------------------------------
    section("9. Confusable strings (visually similar but different)");
    // -------------------------------------------------------------------------
    {
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

    // -------------------------------------------------------------------------
    section("10. Byte slices");
    // -------------------------------------------------------------------------
    {
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
}
