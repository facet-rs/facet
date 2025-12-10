//! Test layout rendering.

use facet_diff_core::ElementChange;
use facet_diff_core::layout::{
    Attr, FormatArena, FormattedValue, Layout, LayoutNode, RenderOptions, group_changed_attrs,
    render_to_string,
};
use indextree::Arena;

fn main() {
    println!("=== Simple changed attribute ===\n");

    let mut strings = FormatArena::new();
    let tree = Arena::new();

    let (red_span, red_width) = strings.push_str("red");
    let (blue_span, blue_width) = strings.push_str("blue");

    let fill_attr = Attr::changed(
        "fill",
        4,
        FormattedValue::new(red_span, red_width),
        FormattedValue::new(blue_span, blue_width),
    );

    let attrs = vec![fill_attr];
    let changed_groups = group_changed_attrs(&attrs, 80, 0);

    let root = LayoutNode::Element {
        tag: "rect",
        attrs,
        changed_groups,
        change: ElementChange::None,
    };

    let layout = Layout::new(strings, tree, root);

    println!("Plain output:");
    let opts = RenderOptions::plain();
    let output = render_to_string(&layout, &opts);
    println!("{}", output);

    println!("Colored output:");
    let opts = RenderOptions::default();
    let output = render_to_string(&layout, &opts);
    println!("{}", output);

    println!("\n=== Multiple changed attributes ===\n");

    let mut strings = FormatArena::new();
    let tree = Arena::new();

    let (red_span, red_width) = strings.push_str("red");
    let (blue_span, blue_width) = strings.push_str("blue");
    let (ten_span, ten_width) = strings.push_str("10");
    let (twenty_span, twenty_width) = strings.push_str("20");

    let fill_attr = Attr::changed(
        "fill",
        4,
        FormattedValue::new(red_span, red_width),
        FormattedValue::new(blue_span, blue_width),
    );
    let x_attr = Attr::changed(
        "x",
        1,
        FormattedValue::new(ten_span, ten_width),
        FormattedValue::new(twenty_span, twenty_width),
    );

    let (five_span, five_width) = strings.push_str("5");
    let y_attr = Attr::unchanged("y", 1, FormattedValue::new(five_span, five_width));

    let attrs = vec![fill_attr, x_attr, y_attr];
    let changed_groups = group_changed_attrs(&attrs, 80, 0);

    let root = LayoutNode::Element {
        tag: "rect",
        attrs,
        changed_groups,
        change: ElementChange::None,
    };

    let layout = Layout::new(strings, tree, root);

    println!("Plain output:");
    let opts = RenderOptions::plain();
    let output = render_to_string(&layout, &opts);
    println!("{}", output);

    println!("Colored output:");
    let opts = RenderOptions::default();
    let output = render_to_string(&layout, &opts);
    println!("{}", output);
}
