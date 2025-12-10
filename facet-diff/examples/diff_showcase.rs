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
}
