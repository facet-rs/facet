//! Diff Format Mockups
//!
//! This example shows MOCKUPS of what diff output SHOULD look like when diffing
//! structured documents (SVG/XML, JSON, Rust-like).
//!
//! Key design decisions:
//! - NO INDICES in output (they shift when things move, confusing)
//! - Moves use `←`/`→` arrows (blue): element at old position (←), element at new position (→)
//! - Deletes use `-` (red): element truly removed
//! - Inserts use `+` (green): element truly new
//! - Value-only coloring: keys stay white, only changed VALUES are colored red/green
//! - Alignment: multiple changed attrs on same -/+ line have columns aligned
//! - Wrapping: too many attrs split into multiple -/+ pairs
//! - Collapse unchanged runs: `<!-- 5 unchanged -->`
//!
//! Run with: cargo run -p facet-diff --example diff_format_mockups

use owo_colors::OwoColorize;

// Tokyo Night theme colors (same as facet-diff uses)
mod colors {
    use owo_colors::Rgb;
    pub const RED: Rgb = Rgb(247, 118, 142); // deletions
    pub const GREEN: Rgb = Rgb(158, 206, 106); // insertions
    pub const BLUE: Rgb = Rgb(122, 162, 247); // paths/labels
    pub const GRAY: Rgb = Rgb(86, 95, 137); // muted/unchanged
    pub const CYAN: Rgb = Rgb(115, 218, 202); // field names
    pub const YELLOW: Rgb = Rgb(224, 175, 104); // section headers
    pub const WHITE: Rgb = Rgb(192, 202, 245); // normal text
}

fn print_section(title: &str) {
    println!();
    println!("{}", "═".repeat(70).color(colors::YELLOW));
    println!("  {}", title.color(colors::YELLOW).bold());
    println!("{}", "═".repeat(70).color(colors::YELLOW));
    println!();
}

fn print_subsection(title: &str) {
    println!();
    println!(
        "{} {} {}",
        "---".color(colors::GRAY),
        title.color(colors::BLUE),
        "---".color(colors::GRAY)
    );
    println!();
}

// ============================================================================
// XML Format
// ============================================================================

fn mockup_xml_simple() {
    print_subsection("Single attribute change (value-only colored)");

    // The changed attr gets its own -/+ lines
    // Key in white, only value colored
    println!("{}", "<rect".color(colors::WHITE));
    print!("  {} ", "-".color(colors::RED));
    print!("{}", "fill=".color(colors::WHITE));
    println!("{}", "\"red\"".color(colors::RED));
    print!("  {} ", "+".color(colors::GREEN));
    print!("{}", "fill=".color(colors::WHITE));
    println!("{}", "\"blue\"".color(colors::GREEN));
    println!(
        "    {}",
        "x=\"10\" y=\"10\" width=\"50\" height=\"50\""
            .color(colors::GRAY)
            .dimmed()
    );
    println!("{}", "/>".color(colors::WHITE));
}

fn mockup_xml_multiple_attrs() {
    print_subsection("Multiple attribute changes (aligned, values-only colored)");

    // Changed attrs aligned - keys in white, only values colored
    println!("{}", "<rect".color(colors::WHITE));
    // The minus line: fill="red"  x="10"
    //                      ^^^       ^^  <- only these red
    print!("  {} ", "-".color(colors::RED));
    print!("{}", "fill=".color(colors::WHITE));
    print!("{}", "\"red\"".color(colors::RED));
    print!("   "); // padding to align x=
    print!("{}", "x=".color(colors::WHITE));
    println!("{}", "\"10\"".color(colors::RED));
    // The plus line: fill="blue" x="20"
    print!("  {} ", "+".color(colors::GREEN));
    print!("{}", "fill=".color(colors::WHITE));
    print!("{}", "\"blue\"".color(colors::GREEN));
    print!("  "); // padding to align x=
    print!("{}", "x=".color(colors::WHITE));
    println!("{}", "\"20\"".color(colors::GREEN));
    println!(
        "    {}",
        "y=\"10\" width=\"50\" height=\"50\""
            .color(colors::GRAY)
            .dimmed()
    );
    println!("{}", "/>".color(colors::WHITE));
}

fn mockup_xml_many_attrs_wrapped() {
    print_subsection("Many attribute changes (wrapped into groups)");

    // When too many attrs change to fit on one line, split into multiple -/+ pairs
    // Each pair is independently aligned
    println!("{}", "<rect".color(colors::WHITE));

    // First group: fill and stroke (fit together)
    print!("  {} ", "-".color(colors::RED));
    print!("{}", "fill=".color(colors::WHITE));
    print!("{}", "\"red\"".color(colors::RED));
    print!("    "); // align
    print!("{}", "stroke=".color(colors::WHITE));
    println!("{}", "\"black\"".color(colors::RED));

    print!("  {} ", "+".color(colors::GREEN));
    print!("{}", "fill=".color(colors::WHITE));
    print!("{}", "\"blue\"".color(colors::GREEN));
    print!("   "); // align
    print!("{}", "stroke=".color(colors::WHITE));
    println!("{}", "\"white\"".color(colors::GREEN));

    // Second group: x and width
    print!("  {} ", "-".color(colors::RED));
    print!("{}", "x=".color(colors::WHITE));
    print!("{}", "\"10\"".color(colors::RED));
    print!("   "); // align
    print!("{}", "width=".color(colors::WHITE));
    println!("{}", "\"100\"".color(colors::RED));

    print!("  {} ", "+".color(colors::GREEN));
    print!("{}", "x=".color(colors::WHITE));
    print!("{}", "\"200\"".color(colors::GREEN));
    print!("  "); // align (200 is wider than 10)
    print!("{}", "width=".color(colors::WHITE));
    println!("{}", "\"50\"".color(colors::GREEN));

    // Third group: stroke-width and opacity
    print!("  {} ", "-".color(colors::RED));
    print!("{}", "stroke-width=".color(colors::WHITE));
    print!("{}", "\"1\"".color(colors::RED));
    print!("  "); // align
    print!("{}", "opacity=".color(colors::WHITE));
    println!("{}", "\"0.5\"".color(colors::RED));

    print!("  {} ", "+".color(colors::GREEN));
    print!("{}", "stroke-width=".color(colors::WHITE));
    print!("{}", "\"3\"".color(colors::GREEN));
    print!("  "); // align
    print!("{}", "opacity=".color(colors::WHITE));
    println!("{}", "\"1.0\"".color(colors::GREEN));

    println!(
        "    {}",
        "y=\"10\" height=\"50\"".color(colors::GRAY).dimmed()
    );
    println!("{}", "/>".color(colors::WHITE));
}

fn mockup_xml_child_added_simple() {
    print_subsection("Child element added (simple)");

    println!("{}", "<svg>".color(colors::WHITE));
    println!(
        "    {}",
        "<rect fill=\"red\"/>".color(colors::GRAY).dimmed()
    );
    println!(
        "  {} {}",
        "+".color(colors::GREEN),
        "<circle cx=\"50\" cy=\"50\" r=\"25\"/>".color(colors::GREEN)
    );
    println!("{}", "</svg>".color(colors::WHITE));
}

fn mockup_xml_child_added_large() {
    print_subsection("Child element added (large, with children)");

    // When a big element is inserted, show all of it in green
    // but with proper structure/indentation
    println!("{}", "<svg>".color(colors::WHITE));
    println!(
        "    {}",
        "<rect fill=\"red\"/>".color(colors::GRAY).dimmed()
    );
    println!(
        "  {} {}",
        "+".color(colors::GREEN),
        "<g id=\"new-layer\">".color(colors::GREEN)
    );
    println!(
        "  {}   {}",
        "+".color(colors::GREEN),
        "<rect x=\"0\" y=\"0\" width=\"100\" height=\"100\"/>".color(colors::GREEN)
    );
    println!(
        "  {}   {}",
        "+".color(colors::GREEN),
        "<circle cx=\"50\" cy=\"50\" r=\"25\"/>".color(colors::GREEN)
    );
    println!(
        "  {}   {}",
        "+".color(colors::GREEN),
        "<text x=\"10\" y=\"20\">Hello</text>".color(colors::GREEN)
    );
    println!(
        "  {}   {}",
        "+".color(colors::GREEN),
        "<path d=\"M10 10 L90 90 L10 90 Z\"/>".color(colors::GREEN)
    );
    println!(
        "  {} {}",
        "+".color(colors::GREEN),
        "</g>".color(colors::GREEN)
    );
    println!("{}", "</svg>".color(colors::WHITE));
}

fn mockup_xml_child_removed() {
    print_subsection("Child element removed");

    println!("{}", "<svg>".color(colors::WHITE));
    println!(
        "    {}",
        "<rect fill=\"red\"/>".color(colors::GRAY).dimmed()
    );
    println!(
        "  {} {}",
        "-".color(colors::RED),
        "<circle cx=\"50\" cy=\"50\" r=\"25\"/>".color(colors::RED)
    );
    println!("{}", "</svg>".color(colors::WHITE));
}

fn mockup_xml_complex() {
    print_subsection("Complex: move + delete + modify + insert");

    // Scenario:
    // - circle#a moved from position 2 to later
    // - path#b deleted
    // - rect#7 fill changed
    // - ellipse#new inserted
    //
    // Legend:
    // - = deleted (truly gone)
    // + = inserted (truly new)
    // ← = moved from here (old position)
    // → = moved to here (new position)

    println!("{}", "<svg>".color(colors::WHITE));
    println!("    {}", "<!-- 2 unchanged -->".color(colors::GRAY));

    // circle#a moved - show with ← at old position
    println!(
        "  {} {}",
        "←".color(colors::BLUE),
        "<circle id=\"a\"/>".color(colors::BLUE)
    );

    println!("    {}", "<rect id=\"3\"/>".color(colors::GRAY).dimmed());

    // path#b deleted (truly gone)
    println!(
        "  {} {}",
        "-".color(colors::RED),
        "<path id=\"b\"/>".color(colors::RED)
    );

    println!("    {}", "<!-- 2 unchanged -->".color(colors::GRAY));

    // rect#7 modified (not moved)
    println!("    {}", "<rect id=\"7\"".color(colors::WHITE));
    print!("      {} ", "-".color(colors::RED));
    print!("{}", "fill=".color(colors::WHITE));
    println!("{}", "\"red\"".color(colors::RED));
    print!("      {} ", "+".color(colors::GREEN));
    print!("{}", "fill=".color(colors::WHITE));
    println!("{}", "\"blue\"".color(colors::GREEN));
    println!("    {}", "/>".color(colors::WHITE));

    // ellipse#new inserted (truly new)
    println!(
        "  {} {}",
        "+".color(colors::GREEN),
        "<ellipse id=\"new\"/>".color(colors::GREEN)
    );

    // circle#a at new position - show with →
    println!(
        "  {} {}",
        "→".color(colors::BLUE),
        "<circle id=\"a\"/>".color(colors::BLUE)
    );

    println!("    {}", "<!-- 8 unchanged -->".color(colors::GRAY));
    println!("{}", "</svg>".color(colors::WHITE));
}

fn mockup_xml_moved_and_modified() {
    print_subsection("Element moved AND modified");

    // circle moved and its radius changed
    // Use ← for old position, → for new position
    // Changed value (r) highlighted within
    println!("{}", "<svg>".color(colors::WHITE));
    println!("    {}", "<!-- 2 unchanged -->".color(colors::GRAY));

    // Old position - moved from here
    print!("  {} ", "←".color(colors::BLUE));
    print!(
        "{}",
        "<circle id=\"a\" cx=\"50\" cy=\"50\" ".color(colors::BLUE)
    );
    print!("{}", "r=".color(colors::BLUE));
    print!("{}", "\"25\"".color(colors::RED)); // old value
    println!("{}", "/>".color(colors::BLUE));

    println!("    {}", "<!-- 3 unchanged -->".color(colors::GRAY));

    // New position - moved to here
    print!("  {} ", "→".color(colors::BLUE));
    print!(
        "{}",
        "<circle id=\"a\" cx=\"50\" cy=\"50\" ".color(colors::BLUE)
    );
    print!("{}", "r=".color(colors::BLUE));
    print!("{}", "\"30\"".color(colors::GREEN)); // new value
    println!("{}", "/>".color(colors::BLUE));

    println!("    {}", "<!-- 5 unchanged -->".color(colors::GRAY));
    println!("{}", "</svg>".color(colors::WHITE));
}

fn mockup_xml_deep_nesting() {
    print_subsection("Deep nesting (change buried in tree)");

    println!("{}", "<svg>".color(colors::WHITE));
    println!("    {}", "<g id=\"layer1\">".color(colors::GRAY).dimmed());
    println!("        {}", "<!-- 50 unchanged -->".color(colors::GRAY));
    println!("        {}", "<g id=\"shapes\">".color(colors::WHITE));
    println!("            {}", "<rect".color(colors::WHITE));
    // Value-only coloring
    print!("              {} ", "-".color(colors::RED));
    print!("{}", "fill=".color(colors::WHITE));
    println!("{}", "\"red\"".color(colors::RED));
    print!("              {} ", "+".color(colors::GREEN));
    print!("{}", "fill=".color(colors::WHITE));
    println!("{}", "\"blue\"".color(colors::GREEN));
    println!("            {}", "/>".color(colors::WHITE));
    println!("        {}", "</g>".color(colors::WHITE));
    println!("        {}", "<!-- 50 unchanged -->".color(colors::GRAY));
    println!("    {}", "</g>".color(colors::GRAY).dimmed());
    println!("{}", "</svg>".color(colors::WHITE));
}

// ============================================================================
// JSON Format
// ============================================================================

fn mockup_json_simple() {
    print_subsection("Single field change (value-only colored)");

    // Key in white, only value colored
    println!("{}", "{".color(colors::WHITE));
    print!("  {} ", "-".color(colors::RED));
    print!("{}", "\"fill\": ".color(colors::WHITE));
    println!("{}", "\"red\"".color(colors::RED));
    print!("  {} ", "+".color(colors::GREEN));
    print!("{}", "\"fill\": ".color(colors::WHITE));
    println!("{}", "\"blue\"".color(colors::GREEN));
    println!(
        "    {}",
        "\"x\": 10, \"y\": 10, \"width\": 50, \"height\": 50"
            .color(colors::GRAY)
            .dimmed()
    );
    println!("{}", "}".color(colors::WHITE));
}

fn mockup_json_multiple_fields() {
    print_subsection("Multiple field changes (aligned, values-only colored)");

    println!("{}", "{".color(colors::WHITE));
    // Aligned: "fill": and "x": line up
    print!("  {} ", "-".color(colors::RED));
    print!("{}", "\"fill\": ".color(colors::WHITE));
    print!("{}", "\"red\"".color(colors::RED));
    print!(",   ");
    print!("{}", "\"x\": ".color(colors::WHITE));
    println!("{}", "10".color(colors::RED));
    print!("  {} ", "+".color(colors::GREEN));
    print!("{}", "\"fill\": ".color(colors::WHITE));
    print!("{}", "\"blue\"".color(colors::GREEN));
    print!(",  ");
    print!("{}", "\"x\": ".color(colors::WHITE));
    println!("{}", "20".color(colors::GREEN));
    println!(
        "    {}",
        "\"y\": 10, \"width\": 50, \"height\": 50"
            .color(colors::GRAY)
            .dimmed()
    );
    println!("{}", "}".color(colors::WHITE));
}

fn mockup_json_array() {
    print_subsection("Array changes");

    println!("{}", "{".color(colors::WHITE));
    println!("    {}", "\"items\": [".color(colors::WHITE));
    println!("        {}", "1, 2,".color(colors::GRAY).dimmed());
    println!(
        "      {} {}",
        "-".color(colors::RED),
        "3,".color(colors::RED)
    );
    println!(
        "      {} {}",
        "-".color(colors::RED),
        "4,".color(colors::RED)
    );
    println!(
        "      {} {}",
        "+".color(colors::GREEN),
        "100,".color(colors::GREEN)
    );
    println!(
        "      {} {}",
        "+".color(colors::GREEN),
        "101,".color(colors::GREEN)
    );
    println!("        {}", "5, 6,".color(colors::GRAY).dimmed());
    println!(
        "      {} {}",
        "-".color(colors::RED),
        "7,".color(colors::RED)
    );
    println!("        {}", "8, 9, 10,".color(colors::GRAY).dimmed());
    println!(
        "      {} {}",
        "+".color(colors::GREEN),
        "11,".color(colors::GREEN)
    );
    println!(
        "      {} {}",
        "+".color(colors::GREEN),
        "12".color(colors::GREEN)
    );
    println!("    {}", "]".color(colors::WHITE));
    println!("{}", "}".color(colors::WHITE));
}

fn mockup_json_nested() {
    print_subsection("Nested object change (value-only colored)");

    println!("{}", "{".color(colors::WHITE));
    println!("    {}", "\"user\": {".color(colors::WHITE));
    println!(
        "        {}",
        "\"name\": \"Alice\",".color(colors::GRAY).dimmed()
    );
    println!("        {}", "\"settings\": {".color(colors::WHITE));
    // Value-only coloring
    print!("          {} ", "-".color(colors::RED));
    print!("{}", "\"theme\": ".color(colors::WHITE));
    println!("{}", "\"dark\"".color(colors::RED));
    print!("          {} ", "+".color(colors::GREEN));
    print!("{}", "\"theme\": ".color(colors::WHITE));
    println!("{}", "\"light\"".color(colors::GREEN));
    println!(
        "            {}",
        "\"notifications\": true".color(colors::GRAY).dimmed()
    );
    println!("        {}", "}".color(colors::WHITE));
    println!("    {}", "}".color(colors::WHITE));
    println!("{}", "}".color(colors::WHITE));
}

fn mockup_json_complex() {
    print_subsection("Complex: multiple changes at different depths");

    println!("{}", "{".color(colors::WHITE));
    // Value-only coloring
    print!("  {} ", "-".color(colors::RED));
    print!("{}", "\"version\": ".color(colors::WHITE));
    println!("{}", "\"1.0\"".color(colors::RED));
    print!("  {} ", "+".color(colors::GREEN));
    print!("{}", "\"version\": ".color(colors::WHITE));
    println!("{}", "\"2.0\"".color(colors::GREEN));
    println!("    {}", "\"data\": {".color(colors::WHITE));
    println!(
        "        {}",
        "/* 10 unchanged fields */".color(colors::GRAY)
    );
    println!("        {}", "\"items\": [".color(colors::WHITE));
    println!("            {}", "/* 5 unchanged */".color(colors::GRAY));
    // For nested object in array, show full object but highlight changed value
    print!("          {} ", "-".color(colors::RED));
    print!("{}", "{ \"id\": 6, \"value\": ".color(colors::WHITE));
    print!("{}", "\"old\"".color(colors::RED));
    println!("{}", " }".color(colors::WHITE));
    print!("          {} ", "+".color(colors::GREEN));
    print!("{}", "{ \"id\": 6, \"value\": ".color(colors::WHITE));
    print!("{}", "\"new\"".color(colors::GREEN));
    println!("{}", " }".color(colors::WHITE));
    println!("            {}", "/* 3 unchanged */".color(colors::GRAY));
    println!("        {}", "]".color(colors::WHITE));
    println!("    {}", "}".color(colors::WHITE));
    println!("{}", "}".color(colors::WHITE));
}

// ============================================================================
// Rust-like Format (facet-pretty style)
// ============================================================================

fn mockup_rust_simple() {
    print_subsection("Single field change (value-only colored)");

    println!("{}", "Rect {".color(colors::WHITE));
    print!("  {} ", "-".color(colors::RED));
    print!("{}", "fill: ".color(colors::WHITE));
    println!("{}", "\"red\"".color(colors::RED));
    print!("  {} ", "+".color(colors::GREEN));
    print!("{}", "fill: ".color(colors::WHITE));
    println!("{}", "\"blue\"".color(colors::GREEN));
    println!(
        "    {}",
        "x: 10, y: 10, width: 50, height: 50,"
            .color(colors::GRAY)
            .dimmed()
    );
    println!("{}", "}".color(colors::WHITE));
}

fn mockup_rust_enum_variant() {
    print_subsection("Enum variant change");

    println!("{}", "Status::".color(colors::WHITE));
    println!(
        "  {} {}",
        "-".color(colors::RED),
        "Active".color(colors::RED)
    );
    println!(
        "  {} {}",
        "+".color(colors::GREEN),
        "Inactive { reason: \"maintenance\" }".color(colors::GREEN)
    );
}

fn mockup_rust_vec() {
    print_subsection("Vec changes");

    println!("{}", "items: [".color(colors::WHITE));
    println!(
        "    {}",
        "Rect { /* unchanged */ },".color(colors::GRAY).dimmed()
    );
    println!(
        "    {}",
        "Circle { /* unchanged */ },".color(colors::GRAY).dimmed()
    );
    println!(
        "  {} {}",
        "-".color(colors::RED),
        "Path { d: \"M...\" },".color(colors::RED)
    );
    println!(
        "  {} {}",
        "+".color(colors::GREEN),
        "Group { id: \"new\", children: [...] },".color(colors::GREEN)
    );
    println!(
        "    {}",
        "Text { /* unchanged */ },".color(colors::GRAY).dimmed()
    );
    println!("{}", "]".color(colors::WHITE));
}

fn mockup_rust_nested() {
    print_subsection("Nested struct change (value-only colored)");

    println!("{}", "Svg {".color(colors::WHITE));
    print!("  {} ", "-".color(colors::RED));
    print!("{}", "view_box: ".color(colors::WHITE));
    println!("{}", "\"0 0 100 100\"".color(colors::RED));
    print!("  {} ", "+".color(colors::GREEN));
    print!("{}", "view_box: ".color(colors::WHITE));
    println!("{}", "\"0 0 200 200\"".color(colors::GREEN));
    println!(
        "    {}",
        "/* width, height, xmlns unchanged */".color(colors::GRAY)
    );
    println!("    {}", "children: [".color(colors::WHITE));
    println!("        {}", "Rect {".color(colors::WHITE));
    print!("          {} ", "-".color(colors::RED));
    print!("{}", "fill: ".color(colors::WHITE));
    println!("{}", "\"red\"".color(colors::RED));
    print!("          {} ", "+".color(colors::GREEN));
    print!("{}", "fill: ".color(colors::WHITE));
    println!("{}", "\"blue\"".color(colors::GREEN));
    println!(
        "            {}",
        "/* x, y, width, height unchanged */".color(colors::GRAY)
    );
    println!("        {}", "},".color(colors::WHITE));
    println!(
        "        {}",
        "/* 2 unchanged elements */".color(colors::GRAY)
    );
    println!("    {}", "],".color(colors::WHITE));
    println!("{}", "}".color(colors::WHITE));
}

fn mockup_rust_complex() {
    print_subsection("Complex: move + delete + modify + insert");

    // Legend:
    // - = deleted
    // + = inserted
    // ← = moved from here (old position)
    // → = moved to here (new position)

    println!("{}", "Svg {".color(colors::WHITE));
    println!("    {}", "/* 2 unchanged fields */".color(colors::GRAY));
    println!("    {}", "children: [".color(colors::WHITE));
    println!("        {}", "/* 2 unchanged */".color(colors::GRAY));

    // Circle id="a" moved AND modified - use ← to show "moved from here"
    // The arrow indicates this element exists elsewhere (at its new position)
    print!("      {} ", "←".color(colors::BLUE));
    print!(
        "{}",
        "Circle { id: \"a\", cx: 50, cy: 50, ".color(colors::BLUE)
    );
    print!("{}", "r: ".color(colors::BLUE));
    print!("{}", "25".color(colors::RED)); // old value highlighted
    println!("{}", " },".color(colors::BLUE));

    println!(
        "        {}",
        "Rect { id: \"3\", /* ... */ },"
            .color(colors::GRAY)
            .dimmed()
    );

    // Path deleted (truly gone, not moved)
    println!(
        "      {} {}",
        "-".color(colors::RED),
        "Path { id: \"b\", /* ... */ },".color(colors::RED)
    );

    println!("        {}", "/* 2 unchanged */".color(colors::GRAY));

    // Rect id="7" modified (not moved)
    println!("        {}", "Rect {".color(colors::WHITE));
    println!("            {}", "id: \"7\",".color(colors::GRAY).dimmed());
    print!("          {} ", "-".color(colors::RED));
    print!("{}", "fill: ".color(colors::WHITE));
    println!("{}", "\"red\"".color(colors::RED));
    print!("          {} ", "+".color(colors::GREEN));
    print!("{}", "fill: ".color(colors::WHITE));
    println!("{}", "\"blue\"".color(colors::GREEN));
    println!("        {}", "},".color(colors::WHITE));

    // Ellipse inserted (truly new)
    println!(
        "      {} {}",
        "+".color(colors::GREEN),
        "Ellipse { id: \"new\", /* ... */ },".color(colors::GREEN)
    );

    // Circle id="a" at new position - use → to show "moved to here"
    print!("      {} ", "→".color(colors::BLUE));
    print!(
        "{}",
        "Circle { id: \"a\", cx: 50, cy: 50, ".color(colors::BLUE)
    );
    print!("{}", "r: ".color(colors::BLUE));
    print!("{}", "30".color(colors::GREEN)); // new value highlighted
    println!("{}", " },".color(colors::BLUE));

    println!("        {}", "/* 8 unchanged */".color(colors::GRAY));
    println!("    {}", "],".color(colors::WHITE));
    println!("{}", "}".color(colors::WHITE));
}

// ============================================================================
// Main
// ============================================================================

fn main() {
    println!();
    println!(
        "{}",
        "╔══════════════════════════════════════════════════════════════════════╗"
            .color(colors::YELLOW)
    );
    println!(
        "{}",
        "║                     DIFF FORMAT MOCKUPS                              ║"
            .color(colors::YELLOW)
    );
    println!(
        "{}",
        "╠══════════════════════════════════════════════════════════════════════╣"
            .color(colors::YELLOW)
    );
    println!(
        "{}",
        "║  Key design decisions:                                               ║"
            .color(colors::GRAY)
    );
    println!(
        "{}",
        "║  • NO INDICES - they shift, they're confusing                        ║"
            .color(colors::GRAY)
    );
    println!(
        "{}",
        "║  • Moves: ← (old position) → (new position) in blue                  ║"
            .color(colors::GRAY)
    );
    println!(
        "{}",
        "║  • Deletes: - (red), Inserts: + (green)                              ║"
            .color(colors::GRAY)
    );
    println!(
        "{}",
        "║  • Value-only coloring: keys white, only VALUES colored              ║"
            .color(colors::GRAY)
    );
    println!(
        "{}",
        "║  • Alignment: changed attrs aligned in columns                       ║"
            .color(colors::GRAY)
    );
    println!(
        "{}",
        "║  • Unchanged runs collapsed: /* N unchanged */                       ║"
            .color(colors::GRAY)
    );
    println!(
        "{}",
        "╚══════════════════════════════════════════════════════════════════════╝"
            .color(colors::YELLOW)
    );

    // XML
    print_section("XML Format");
    mockup_xml_simple();
    mockup_xml_multiple_attrs();
    mockup_xml_many_attrs_wrapped();
    mockup_xml_child_added_simple();
    mockup_xml_child_added_large();
    mockup_xml_child_removed();
    mockup_xml_complex();
    mockup_xml_moved_and_modified();
    mockup_xml_deep_nesting();

    // JSON
    print_section("JSON Format");
    mockup_json_simple();
    mockup_json_multiple_fields();
    mockup_json_array();
    mockup_json_nested();
    mockup_json_complex();

    // Rust-like
    print_section("Rust-like Format (facet-pretty style)");
    mockup_rust_simple();
    mockup_rust_enum_variant();
    mockup_rust_vec();
    mockup_rust_nested();
    mockup_rust_complex();

    // Design notes
    println!();
    println!("{}", "═".repeat(70).color(colors::YELLOW));
    println!("  {}", "Implementation Notes".color(colors::YELLOW).bold());
    println!("{}", "═".repeat(70).color(colors::YELLOW));
    println!();
    println!(
        "{}",
        "The serializer needs a DiffContext to query at each node:".color(colors::WHITE)
    );
    println!();
    println!(
        "  {} {}",
        "changed_children(path)".color(colors::CYAN),
        "→ attrs that changed (need -/+ lines)".color(colors::GRAY)
    );
    println!(
        "  {} {}",
        "unchanged_children(path)".color(colors::CYAN),
        "→ attrs that stayed same (inline, dimmed)".color(colors::GRAY)
    );
    println!(
        "  {} {}",
        "deleted_children(path)".color(colors::CYAN),
        "→ elements removed (- prefix)".color(colors::GRAY)
    );
    println!(
        "  {} {}",
        "inserted_children(path)".color(colors::CYAN),
        "→ elements added (+ prefix)".color(colors::GRAY)
    );
    println!(
        "  {} {}",
        "moved_children(path)".color(colors::CYAN),
        "→ elements relocated (← old, → new)".color(colors::GRAY)
    );
    println!(
        "  {} {}",
        "should_collapse(path)".color(colors::CYAN),
        "→ can we skip this subtree?".color(colors::GRAY)
    );
    println!();
    println!("{}", "Layout decisions:".color(colors::WHITE));
    println!();
    println!(
        "  {} {}",
        "•".color(colors::GREEN),
        "Changed attrs → -/+ lines, keys white, VALUES colored".color(colors::GRAY)
    );
    println!(
        "  {} {}",
        "•".color(colors::GREEN),
        "Multiple changed attrs → aligned columns, wrap if needed".color(colors::GRAY)
    );
    println!(
        "  {} {}",
        "•".color(colors::GREEN),
        "Unchanged attrs → inline, dimmed".color(colors::GRAY)
    );
    println!(
        "  {} {}",
        "•".color(colors::GREEN),
        "Many unchanged siblings → collapse: /* N unchanged */".color(colors::GRAY)
    );
    println!(
        "  {} {}",
        "•".color(colors::GREEN),
        "Deleted element → - prefix (red), full element".color(colors::GRAY)
    );
    println!(
        "  {} {}",
        "•".color(colors::GREEN),
        "Inserted element → + prefix (green), full element".color(colors::GRAY)
    );
    println!(
        "  {} {}",
        "•".color(colors::GREEN),
        "Moved element → ← at old pos, → at new pos (blue)".color(colors::GRAY)
    );
    println!();
}
