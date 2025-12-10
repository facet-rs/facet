//! Test layout rendering with build_layout showing all three flavors.

use facet::Facet;
use facet_diff::FacetDiff;
use facet_diff_core::{
    BuildOptions, JsonFlavor, RenderOptions, RustFlavor, XmlFlavor, build_layout, render_to_string,
};
use facet_reflect::Peek;
use facet_value::{VObject, Value};

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

    // Field additions using facet_value::Value
    {
        let mut from_obj = VObject::new();
        from_obj.insert("name", "server");
        from_obj.insert("port", 8080);
        let from: Value = from_obj.into();

        let mut to_obj = VObject::new();
        to_obj.insert("name", "server");
        to_obj.insert("port", 8080);
        to_obj.insert("host", "localhost");
        to_obj.insert("timeout", 30);
        to_obj.insert("debug", true);
        let to: Value = to_obj.into();

        print_all_flavors("Field additions (multiple)", &from, &to);
    }

    // Field removals using facet_value::Value
    {
        let mut from_obj = VObject::new();
        from_obj.insert("name", "server");
        from_obj.insert("port", 8080);
        from_obj.insert("host", "localhost");
        from_obj.insert("timeout", 30);
        from_obj.insert("debug", true);
        let from: Value = from_obj.into();

        let mut to_obj = VObject::new();
        to_obj.insert("name", "server");
        to_obj.insert("port", 8080);
        let to: Value = to_obj.into();

        print_all_flavors("Field removals (multiple)", &from, &to);
    }

    // Mixed additions, removals, and changes
    {
        let mut from_obj = VObject::new();
        from_obj.insert("name", "old-server");
        from_obj.insert("port", 80);
        from_obj.insert("legacy", true);
        let from: Value = from_obj.into();

        let mut to_obj = VObject::new();
        to_obj.insert("name", "new-server");
        to_obj.insert("port", 443);
        to_obj.insert("secure", true);
        to_obj.insert("tls_version", "1.3");
        let to: Value = to_obj.into();

        print_all_flavors("Mixed changes (add, remove, modify)", &from, &to);
    }

    // Deeper nesting - SVG-like structure
    {
        #[derive(Facet, Debug)]
        struct Svg {
            width: u32,
            height: u32,
            viewbox: ViewBox,
            elements: Vec<Element>,
        }

        #[derive(Facet, Debug)]
        struct ViewBox {
            min_x: i32,
            min_y: i32,
            width: u32,
            height: u32,
        }

        #[derive(Facet, Debug)]
        struct Element {
            kind: &'static str,
            transform: Transform,
            style: Style,
        }

        #[derive(Facet, Debug)]
        struct Transform {
            translate_x: f32,
            translate_y: f32,
            scale: f32,
            rotate: f32,
        }

        #[derive(Facet, Debug)]
        struct Style {
            fill: &'static str,
            stroke: &'static str,
            stroke_width: f32,
            opacity: f32,
        }

        let from = Svg {
            width: 800,
            height: 600,
            viewbox: ViewBox {
                min_x: 0,
                min_y: 0,
                width: 800,
                height: 600,
            },
            elements: vec![
                Element {
                    kind: "rect",
                    transform: Transform {
                        translate_x: 100.0,
                        translate_y: 100.0,
                        scale: 1.0,
                        rotate: 0.0,
                    },
                    style: Style {
                        fill: "blue",
                        stroke: "black",
                        stroke_width: 2.0,
                        opacity: 1.0,
                    },
                },
                Element {
                    kind: "circle",
                    transform: Transform {
                        translate_x: 400.0,
                        translate_y: 300.0,
                        scale: 1.0,
                        rotate: 0.0,
                    },
                    style: Style {
                        fill: "red",
                        stroke: "none",
                        stroke_width: 0.0,
                        opacity: 0.8,
                    },
                },
            ],
        };

        let to = Svg {
            width: 1024, // changed
            height: 768, // changed
            viewbox: ViewBox {
                min_x: 0,
                min_y: 0,
                width: 1024, // changed
                height: 768, // changed
            },
            elements: vec![
                Element {
                    kind: "rect",
                    transform: Transform {
                        translate_x: 150.0, // changed
                        translate_y: 120.0, // changed
                        scale: 1.5,         // changed
                        rotate: 45.0,       // changed
                    },
                    style: Style {
                        fill: "green",     // changed
                        stroke: "white",   // changed
                        stroke_width: 3.0, // changed
                        opacity: 0.9,      // changed
                    },
                },
                Element {
                    kind: "circle",
                    transform: Transform {
                        translate_x: 500.0, // changed
                        translate_y: 400.0, // changed
                        scale: 2.0,         // changed
                        rotate: 0.0,
                    },
                    style: Style {
                        fill: "yellow",    // changed
                        stroke: "orange",  // changed
                        stroke_width: 1.0, // changed
                        opacity: 1.0,      // changed
                    },
                },
            ],
        };

        print_all_flavors("Deep nesting (SVG-like)", &from, &to);
    }
}
