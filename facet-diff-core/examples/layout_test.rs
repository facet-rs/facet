//! Test layout rendering with build_layout.

use facet::Facet;
use facet_diff::FacetDiff;
use facet_diff_core::{BuildOptions, RenderOptions, build_layout, render_to_string};
use facet_reflect::Peek;

fn main() {
    let opts = RenderOptions::default();

    println!("=== Struct diff (one field changed) ===\n");
    {
        #[derive(Facet, Debug)]
        struct Point {
            x: i32,
            y: i32,
        }

        let from = Point { x: 10, y: 20 };
        let to = Point { x: 30, y: 20 }; // only x changed

        let diff = from.diff(&to);
        let layout = build_layout(
            &diff,
            Peek::new(&from),
            Peek::new(&to),
            &BuildOptions::default(),
        );
        println!("{}", render_to_string(&layout, &opts));
    }

    println!("=== Struct diff (all fields changed, test alignment) ===\n");
    {
        #[derive(Facet, Debug)]
        struct Point {
            x: i32,
            y: i32,
        }

        let from = Point { x: 10, y: 20 };
        let to = Point { x: 3235832, y: 2 }; // both changed, different widths

        let diff = from.diff(&to);
        let layout = build_layout(
            &diff,
            Peek::new(&from),
            Peek::new(&to),
            &BuildOptions::default(),
        );
        println!("{}", render_to_string(&layout, &opts));
    }

    println!("=== Nested struct diff ===\n");
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
            name: "origin",               // unchanged
            point: Inner { x: 10, y: 0 }, // x changed
        };

        let diff = from.diff(&to);
        let layout = build_layout(
            &diff,
            Peek::new(&from),
            Peek::new(&to),
            &BuildOptions::default(),
        );
        println!("{}", render_to_string(&layout, &opts));
    }

    println!("=== Struct with many unchanged fields ===\n");
    {
        #[derive(Facet, Debug)]
        struct Config {
            a: i32,
            b: i32,
            c: i32,
            d: i32,
            e: i32,
            f: i32,
            g: i32,
            changed: i32,
        }

        let from = Config {
            a: 1,
            b: 2,
            c: 3,
            d: 4,
            e: 5,
            f: 6,
            g: 7,
            changed: 100,
        };
        let to = Config {
            a: 1,
            b: 2,
            c: 3,
            d: 4,
            e: 5,
            f: 6,
            g: 7,
            changed: 200,
        };

        let diff = from.diff(&to);
        let layout = build_layout(
            &diff,
            Peek::new(&from),
            Peek::new(&to),
            &BuildOptions::default(),
        );
        println!("{}", render_to_string(&layout, &opts));
    }

    println!("=== Sequence diff ===\n");
    {
        let from = vec![1, 2, 3, 4, 5];
        let to = vec![1, 2, 99, 4, 5]; // 3 -> 99

        let diff = from.diff(&to);
        let layout = build_layout(
            &diff,
            Peek::new(&from),
            Peek::new(&to),
            &BuildOptions::default(),
        );
        println!("{}", render_to_string(&layout, &opts));
    }

    println!("=== Sequence diff (with collapsing) ===\n");
    {
        let from: Vec<i32> = (0..20).collect();
        let mut to = from.clone();
        to[10] = 999; // change one element in the middle

        let diff = from.diff(&to);
        let layout = build_layout(
            &diff,
            Peek::new(&from),
            Peek::new(&to),
            &BuildOptions::default(),
        );
        println!("{}", render_to_string(&layout, &opts));
    }
}
