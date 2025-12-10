//! Test layout rendering with build_layout showing all three flavors.

use facet::Facet;
use facet_diff::FacetDiff;
use facet_diff_core::{
    BuildOptions, JsonFlavor, RenderOptions, RustFlavor, XmlFlavor, build_layout, render_to_string,
};
use facet_reflect::Peek;

fn print_all_flavors<'a, T: facet::Facet<'a>>(label: &str, from: &T, to: &T) {
    let opts = RenderOptions::default();
    let build_opts = BuildOptions::default();

    println!("=== {} ===\n", label);

    let diff = from.diff(to);

    // Rust flavor
    println!("--- Rust ---");
    let layout = build_layout(
        &diff,
        Peek::new(from),
        Peek::new(to),
        &build_opts,
        &RustFlavor,
    );
    println!("{}", render_to_string(&layout, &opts, &RustFlavor));

    // JSON flavor
    println!("--- JSON ---");
    let layout = build_layout(
        &diff,
        Peek::new(from),
        Peek::new(to),
        &build_opts,
        &JsonFlavor,
    );
    println!("{}", render_to_string(&layout, &opts, &JsonFlavor));

    // XML flavor
    println!("--- XML ---");
    let layout = build_layout(
        &diff,
        Peek::new(from),
        Peek::new(to),
        &build_opts,
        &XmlFlavor,
    );
    println!("{}", render_to_string(&layout, &opts, &XmlFlavor));

    println!();
}

fn main() {
    // Simple struct diff
    {
        #[derive(Facet, Debug)]
        struct Point {
            x: i32,
            y: i32,
        }

        let from = Point { x: 10, y: 20 };
        let to = Point { x: 30, y: 20 }; // only x changed

        print_all_flavors("Struct diff (one field changed)", &from, &to);
    }

    // All fields changed
    {
        #[derive(Facet, Debug)]
        struct Point {
            x: i32,
            y: i32,
        }

        let from = Point { x: 10, y: 20 };
        let to = Point { x: 3235832, y: 2 };

        print_all_flavors("Struct diff (all fields changed)", &from, &to);
    }

    // Nested struct
    {
        #[derive(Facet, Debug)]
        struct Outer {
            name: &'static str,
            point: Inner,
        }

        #[derive(Facet, Debug)]
        struct Inner {
            x: i32,
            y: i32,
        }

        let from = Outer {
            name: "origin",
            point: Inner { x: 0, y: 0 },
        };
        let to = Outer {
            name: "origin",
            point: Inner { x: 10, y: 0 },
        };

        print_all_flavors("Nested struct diff", &from, &to);
    }

    // Sequence diff
    {
        let from = vec![1, 2, 3, 4, 5];
        let to = vec![1, 2, 99, 4, 5];

        print_all_flavors("Sequence diff", &from, &to);
    }

    // Sequence with collapsing
    {
        let from: Vec<i32> = (0..20).collect();
        let mut to = from.clone();
        to[10] = 999;

        print_all_flavors("Sequence diff (with collapsing)", &from, &to);
    }

    // Longer sequence - shows XML width problem
    // XML tags make items much wider than Rust/JSON
    {
        let from: Vec<i32> = (100..150).collect(); // 50 items, 3-digit numbers
        let mut to = from.clone();
        to[25] = 9999; // change item at index 25

        print_all_flavors("Long sequence (XML width test)", &from, &to);
    }
}
