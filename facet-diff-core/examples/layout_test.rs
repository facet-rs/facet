//! Test layout rendering with build_layout showing all three flavors.

use facet::Facet;
use facet_diff::FacetDiff;
// Import facet_xml as `xml` to bring the xml attribute grammar into scope
use facet_diff_core::{
    BuildOptions, JsonFlavor, RenderOptions, RustFlavor, XmlFlavor, build_layout, render_to_string,
};
use facet_reflect::Peek;
use facet_value::{VObject, Value};
use facet_xml_legacy as xml;

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

    // Deeper nesting - SVG-like structure with proper element names and XML annotations
    {
        #[derive(Facet, Debug)]
        #[facet(rename = "svg")]
        struct Svg {
            #[facet(xml::attribute)]
            width: u32,
            #[facet(xml::attribute)]
            height: u32,
            #[facet(xml::element, rename = "viewBox")]
            viewbox: ViewBox,
            #[facet(xml::elements)]
            groups: Vec<Group>,
        }

        #[derive(Facet, Debug)]
        #[facet(rename = "viewBox")]
        struct ViewBox {
            #[facet(xml::attribute, rename = "minX")]
            min_x: i32,
            #[facet(xml::attribute, rename = "minY")]
            min_y: i32,
            #[facet(xml::attribute)]
            width: u32,
            #[facet(xml::attribute)]
            height: u32,
        }

        #[derive(Facet, Debug)]
        #[facet(rename = "g")]
        struct Group {
            #[facet(xml::attribute)]
            id: &'static str,
            #[facet(xml::element)]
            transform: Transform,
            #[facet(xml::element)]
            style: Style,
        }

        #[derive(Facet, Debug)]
        #[facet(rename = "transform", rename_all = "kebab-case")]
        struct Transform {
            #[facet(xml::attribute)]
            translate_x: f32,
            #[facet(xml::attribute)]
            translate_y: f32,
            #[facet(xml::attribute)]
            scale: f32,
            #[facet(xml::attribute)]
            rotate: f32,
        }

        #[derive(Facet, Debug)]
        #[facet(rename = "style", rename_all = "kebab-case")]
        struct Style {
            #[facet(xml::attribute)]
            fill: &'static str,
            #[facet(xml::attribute)]
            stroke: &'static str,
            #[facet(xml::attribute)]
            stroke_width: f32,
            #[facet(xml::attribute)]
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
            groups: vec![
                Group {
                    id: "rect-group",
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
                Group {
                    id: "circle-group",
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
            groups: vec![
                Group {
                    id: "rect-group",
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
                Group {
                    id: "circle-group",
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

    // Many attributes scenario
    {
        #[derive(Facet, Debug)]
        struct ManyAttrs {
            name: &'static str,
            id: u32,
            enabled: bool,
            visible: bool,
            priority: i32,
            weight: f32,
            category: &'static str,
            tags: &'static str,
            version: u32,
            revision: u32,
            status: &'static str,
            owner: &'static str,
        }

        let from = ManyAttrs {
            name: "widget",
            id: 1001,
            enabled: true,
            visible: true,
            priority: 5,
            weight: 1.0,
            category: "ui",
            tags: "interactive,clickable",
            version: 1,
            revision: 0,
            status: "active",
            owner: "alice",
        };

        let to = ManyAttrs {
            name: "super-widget", // changed
            id: 1001,
            enabled: false, // changed
            visible: true,
            priority: 10, // changed
            weight: 2.5,  // changed
            category: "ui",
            tags: "interactive,draggable,resizable", // changed
            version: 2,                              // changed
            revision: 3,                             // changed
            status: "updated",                       // changed
            owner: "bob",                            // changed
        };

        print_all_flavors("Many attributes (12 fields)", &from, &to);
    }

    // Mixed scalar and struct attributes (XML will be interesting here)
    {
        #[derive(Facet, Debug)]
        struct Metadata {
            created_by: &'static str,
            version: u32,
        }

        #[derive(Facet, Debug)]
        struct Bounds {
            min: i32,
            max: i32,
        }

        #[derive(Facet, Debug)]
        struct Config {
            name: &'static str,
            enabled: bool,
            metadata: Metadata,
            bounds: Bounds,
            tags: &'static str,
        }

        let from = Config {
            name: "widget",
            enabled: true,
            metadata: Metadata {
                created_by: "alice",
                version: 1,
            },
            bounds: Bounds { min: 0, max: 100 },
            tags: "ui,interactive",
        };

        let to = Config {
            name: "super-widget",
            enabled: false,
            metadata: Metadata {
                created_by: "bob",
                version: 2,
            },
            bounds: Bounds { min: -10, max: 200 },
            tags: "ui,draggable",
        };

        print_all_flavors("Mixed scalar and struct fields", &from, &to);
    }

    // Enum variant change (struct variants with fields)
    {
        #[derive(Facet, Debug)]
        struct Circle {
            radius: f32,
            center_x: f32,
            center_y: f32,
        }

        #[derive(Facet, Debug)]
        struct Rectangle {
            width: f32,
            height: f32,
            x: f32,
            y: f32,
        }

        #[derive(Facet, Debug)]
        #[repr(u8)]
        enum Shape {
            #[allow(dead_code)]
            Circle(Circle),
            #[allow(dead_code)]
            Rectangle(Rectangle),
        }

        #[derive(Facet, Debug)]
        struct Drawing {
            name: &'static str,
            shape: Shape,
        }

        let from = Drawing {
            name: "my-shape",
            shape: Shape::Circle(Circle {
                radius: 50.0,
                center_x: 100.0,
                center_y: 100.0,
            }),
        };

        let to = Drawing {
            name: "my-shape",
            shape: Shape::Rectangle(Rectangle {
                width: 80.0,
                height: 60.0,
                x: 50.0,
                y: 70.0,
            }),
        };

        print_all_flavors("Enum variant change", &from, &to);
    }
}
