//! Test layout rendering with build_layout.

use facet::Facet;
use facet_diff_core::{BuildOptions, Diff, RenderOptions, build_layout, render_to_string};
use facet_reflect::Peek;

fn main() {
    let opts = RenderOptions::default();

    println!("=== Diff::Equal ===\n");
    {
        let value = 42i32;
        let diff = Diff::Equal {
            value: Some(Peek::new(&value)),
        };

        let layout = build_layout(&diff, &BuildOptions::default());
        println!("{}", render_to_string(&layout, &opts));
    }

    println!("=== Diff::Replace (scalars) ===\n");
    {
        let from = 10i32;
        let to = 20i32;
        let diff = Diff::Replace {
            from: Peek::new(&from),
            to: Peek::new(&to),
        };

        let layout = build_layout(&diff, &BuildOptions::default());
        println!("{}", render_to_string(&layout, &opts));
    }

    println!("=== Diff::Replace (strings) ===\n");
    {
        let from = "hello";
        let to = "world";
        let diff = Diff::Replace {
            from: Peek::new(&from),
            to: Peek::new(&to),
        };

        let layout = build_layout(&diff, &BuildOptions::default());
        println!("{}", render_to_string(&layout, &opts));
    }

    println!("=== Diff::Replace (structs) ===\n");
    {
        #[derive(Facet)]
        struct Point {
            x: i32,
            y: i32,
        }

        let from = Point { x: 10, y: 20 };
        let to = Point { x: 30, y: 40 };
        let diff = Diff::Replace {
            from: Peek::new(&from),
            to: Peek::new(&to),
        };

        let layout = build_layout(&diff, &BuildOptions::default());
        println!("{}", render_to_string(&layout, &opts));
    }

    println!("=== Diff::Sequence (with changes) ===\n");
    {
        use facet_diff_core::{Interspersed, ReplaceGroup, Updates, UpdatesGroup};

        let items_before = vec![1i32, 2, 3, 4, 5];
        let items_after = vec![1i32, 2, 99, 4, 5];

        let removal = Peek::new(&items_before[2]); // 3
        let addition = Peek::new(&items_after[2]); // 99

        let replace_group = ReplaceGroup {
            removals: vec![removal],
            additions: vec![addition],
        };

        let update_group = UpdatesGroup(Interspersed {
            first: Some(replace_group),
            values: vec![],
            last: None,
        });

        let unchanged_before: Vec<_> = items_before[0..2].iter().map(|x| Peek::new(x)).collect();
        let unchanged_after: Vec<_> = items_before[3..5].iter().map(|x| Peek::new(x)).collect();

        let updates = Updates(Interspersed {
            first: None,
            values: vec![(unchanged_before, update_group)],
            last: Some(unchanged_after),
        });

        let diff = Diff::Sequence {
            from: <Vec<i32> as facet_core::Facet>::SHAPE,
            to: <Vec<i32> as facet_core::Facet>::SHAPE,
            updates,
        };

        let layout = build_layout(&diff, &BuildOptions::default());
        println!("{}", render_to_string(&layout, &opts));
    }

    println!("=== Diff::Sequence (with collapsing) ===\n");
    {
        use facet_diff_core::{Interspersed, ReplaceGroup, Updates, UpdatesGroup};

        let items: Vec<i32> = (0..10).collect();

        let removal = Peek::new(&100i32);
        let addition = Peek::new(&999i32);

        let replace_group = ReplaceGroup {
            removals: vec![removal],
            additions: vec![addition],
        };

        let update_group = UpdatesGroup(Interspersed {
            first: Some(replace_group),
            values: vec![],
            last: None,
        });

        // 5 unchanged items (should collapse with threshold=3)
        let unchanged: Vec<_> = items[0..5].iter().map(|x| Peek::new(x)).collect();

        let updates = Updates(Interspersed {
            first: None,
            values: vec![(unchanged, update_group)],
            last: None,
        });

        let diff = Diff::Sequence {
            from: <Vec<i32> as facet_core::Facet>::SHAPE,
            to: <Vec<i32> as facet_core::Facet>::SHAPE,
            updates,
        };

        let layout = build_layout(&diff, &BuildOptions::default());
        println!("{}", render_to_string(&layout, &opts));
    }
}
