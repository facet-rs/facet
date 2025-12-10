//! Diff Format Mockups
//!
//! This example shows MOCKUPS of what diff output SHOULD look like when diffing
//! structured documents (SVG/XML, JSON, Rust-like).
//!
//! Key design decisions:
//! - NO INDICES in output (they shift when things move, confusing)
//! - Moves are implicit: same element appears as `-` then `+`
//! - Collapse unchanged runs: `<!-- 5 unchanged -->`
//! - Changed fields get their own lines with `-`/`+`
//! - Unchanged fields shown inline or collapsed
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
    print_subsection("Single attribute change");

    // The changed attr gets its own -/+ lines
    // Unchanged attrs stay together
    println!("{}", "<rect".color(colors::WHITE));
    println!(
        "  {} {}",
        "-".color(colors::RED),
        "fill=\"red\"".color(colors::RED)
    );
    println!(
        "  {} {}",
        "+".color(colors::GREEN),
        "fill=\"blue\"".color(colors::GREEN)
    );
    println!(
        "    {}",
        "x=\"10\" y=\"10\" width=\"50\" height=\"50\""
            .color(colors::GRAY)
            .dimmed()
    );
    println!("{}", "/>".color(colors::WHITE));
}

fn mockup_xml_multiple_attrs() {
    print_subsection("Multiple attribute changes (grouped)");

    // Changed attrs grouped on single -/+ lines
    println!("{}", "<rect".color(colors::WHITE));
    println!(
        "  {} {}",
        "-".color(colors::RED),
        "fill=\"red\" x=\"10\"".color(colors::RED)
    );
    println!(
        "  {} {}",
        "+".color(colors::GREEN),
        "fill=\"blue\" x=\"20\"".color(colors::GREEN)
    );
    println!(
        "    {}",
        "y=\"10\" width=\"50\" height=\"50\""
            .color(colors::GRAY)
            .dimmed()
    );
    println!("{}", "/>".color(colors::WHITE));
}

fn mockup_xml_child_added() {
    print_subsection("Child element added");

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

    println!("{}", "<svg>".color(colors::WHITE));
    println!("    {}", "<!-- 2 unchanged -->".color(colors::GRAY));
    println!(
        "  {} {}",
        "-".color(colors::RED),
        "<circle id=\"a\"/>".color(colors::RED)
    );
    println!("    {}", "<rect id=\"3\"/>".color(colors::GRAY).dimmed());
    println!(
        "  {} {}",
        "-".color(colors::RED),
        "<path id=\"b\"/>".color(colors::RED)
    );
    println!("    {}", "<!-- 2 unchanged -->".color(colors::GRAY));
    println!("    {}", "<rect id=\"7\"".color(colors::WHITE));
    println!(
        "      {} {}",
        "-".color(colors::RED),
        "fill=\"red\"".color(colors::RED)
    );
    println!(
        "      {} {}",
        "+".color(colors::GREEN),
        "fill=\"blue\"".color(colors::GREEN)
    );
    println!("    {}", "/>".color(colors::WHITE));
    println!(
        "  {} {}",
        "+".color(colors::GREEN),
        "<ellipse id=\"new\"/>".color(colors::GREEN)
    );
    println!(
        "  {} {}",
        "+".color(colors::GREEN),
        "<circle id=\"a\"/>".color(colors::GREEN)
    );
    println!("    {}", "<!-- 8 unchanged -->".color(colors::GRAY));
    println!("{}", "</svg>".color(colors::WHITE));
}

fn mockup_xml_moved_and_modified() {
    print_subsection("Element moved AND modified");

    // circle moved and its radius changed
    println!("{}", "<svg>".color(colors::WHITE));
    println!("    {}", "<!-- 2 unchanged -->".color(colors::GRAY));
    println!(
        "  {} {}",
        "-".color(colors::RED),
        "<circle id=\"a\" cx=\"50\" cy=\"50\"".color(colors::RED)
    );
    println!(
        "      {} {}",
        "-".color(colors::RED),
        "r=\"25\"".color(colors::RED)
    );
    println!("    {}", "/>".color(colors::RED));
    println!("    {}", "<!-- 3 unchanged -->".color(colors::GRAY));
    println!(
        "  {} {}",
        "+".color(colors::GREEN),
        "<circle id=\"a\" cx=\"50\" cy=\"50\"".color(colors::GREEN)
    );
    println!(
        "      {} {}",
        "+".color(colors::GREEN),
        "r=\"30\"".color(colors::GREEN)
    );
    println!("    {}", "/>".color(colors::GREEN));
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
    println!(
        "              {} {}",
        "-".color(colors::RED),
        "fill=\"red\"".color(colors::RED)
    );
    println!(
        "              {} {}",
        "+".color(colors::GREEN),
        "fill=\"blue\"".color(colors::GREEN)
    );
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
    print_subsection("Single field change");

    println!("{}", "{".color(colors::WHITE));
    println!(
        "  {} {}",
        "-".color(colors::RED),
        "\"fill\": \"red\",".color(colors::RED)
    );
    println!(
        "  {} {}",
        "+".color(colors::GREEN),
        "\"fill\": \"blue\",".color(colors::GREEN)
    );
    println!(
        "    {}",
        "\"x\": 10, \"y\": 10, \"width\": 50, \"height\": 50"
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
    print_subsection("Nested object change");

    println!("{}", "{".color(colors::WHITE));
    println!("    {}", "\"user\": {".color(colors::WHITE));
    println!(
        "        {}",
        "\"name\": \"Alice\",".color(colors::GRAY).dimmed()
    );
    println!("        {}", "\"settings\": {".color(colors::WHITE));
    println!(
        "          {} {}",
        "-".color(colors::RED),
        "\"theme\": \"dark\",".color(colors::RED)
    );
    println!(
        "          {} {}",
        "+".color(colors::GREEN),
        "\"theme\": \"light\",".color(colors::GREEN)
    );
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
    println!(
        "  {} {}",
        "-".color(colors::RED),
        "\"version\": \"1.0\",".color(colors::RED)
    );
    println!(
        "  {} {}",
        "+".color(colors::GREEN),
        "\"version\": \"2.0\",".color(colors::GREEN)
    );
    println!("    {}", "\"data\": {".color(colors::WHITE));
    println!(
        "        {}",
        "/* 10 unchanged fields */".color(colors::GRAY)
    );
    println!("        {}", "\"items\": [".color(colors::WHITE));
    println!("            {}", "/* 5 unchanged */".color(colors::GRAY));
    println!(
        "          {} {}",
        "-".color(colors::RED),
        "{ \"id\": 6, \"value\": \"old\" },".color(colors::RED)
    );
    println!(
        "          {} {}",
        "+".color(colors::GREEN),
        "{ \"id\": 6, \"value\": \"new\" },".color(colors::GREEN)
    );
    println!("            {}", "/* 3 unchanged */".color(colors::GRAY));
    println!("        {}", "]".color(colors::WHITE));
    println!("    {}", "}".color(colors::WHITE));
    println!("{}", "}".color(colors::WHITE));
}

// ============================================================================
// Rust-like Format (facet-pretty style)
// ============================================================================

fn mockup_rust_simple() {
    print_subsection("Single field change");

    println!("{}", "Rect {".color(colors::WHITE));
    println!(
        "  {} {}",
        "-".color(colors::RED),
        "fill: \"red\",".color(colors::RED)
    );
    println!(
        "  {} {}",
        "+".color(colors::GREEN),
        "fill: \"blue\",".color(colors::GREEN)
    );
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
    print_subsection("Nested struct change");

    println!("{}", "Svg {".color(colors::WHITE));
    println!(
        "  {} {}",
        "-".color(colors::RED),
        "view_box: \"0 0 100 100\",".color(colors::RED)
    );
    println!(
        "  {} {}",
        "+".color(colors::GREEN),
        "view_box: \"0 0 200 200\",".color(colors::GREEN)
    );
    println!(
        "    {}",
        "/* width, height, xmlns unchanged */".color(colors::GRAY)
    );
    println!("    {}", "children: [".color(colors::WHITE));
    println!("        {}", "Rect {".color(colors::WHITE));
    println!(
        "          {} {}",
        "-".color(colors::RED),
        "fill: \"red\",".color(colors::RED)
    );
    println!(
        "          {} {}",
        "+".color(colors::GREEN),
        "fill: \"blue\",".color(colors::GREEN)
    );
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
    print_subsection("Complex: move + modify");

    println!("{}", "Svg {".color(colors::WHITE));
    println!("    {}", "/* 2 unchanged fields */".color(colors::GRAY));
    println!("    {}", "children: [".color(colors::WHITE));
    println!("        {}", "/* 2 unchanged */".color(colors::GRAY));
    println!(
        "      {} {}",
        "-".color(colors::RED),
        "Circle { id: \"a\", cx: 50, cy: 50, r: 25 },".color(colors::RED)
    );
    println!(
        "        {}",
        "Rect { id: \"3\", /* ... */ },"
            .color(colors::GRAY)
            .dimmed()
    );
    println!(
        "      {} {}",
        "-".color(colors::RED),
        "Path { id: \"b\", /* ... */ },".color(colors::RED)
    );
    println!("        {}", "/* 2 unchanged */".color(colors::GRAY));
    println!("        {}", "Rect {".color(colors::WHITE));
    println!("            {}", "id: \"7\",".color(colors::GRAY).dimmed());
    println!(
        "          {} {}",
        "-".color(colors::RED),
        "fill: \"red\",".color(colors::RED)
    );
    println!(
        "          {} {}",
        "+".color(colors::GREEN),
        "fill: \"blue\",".color(colors::GREEN)
    );
    println!("        {}", "},".color(colors::WHITE));
    println!(
        "      {} {}",
        "+".color(colors::GREEN),
        "Ellipse { id: \"new\", /* ... */ },".color(colors::GREEN)
    );
    println!(
        "      {} {}",
        "+".color(colors::GREEN),
        "Circle { id: \"a\", cx: 50, cy: 50, r: 30 },".color(colors::GREEN)
    );
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
        "║  • Moves are implicit: element appears as - then + elsewhere         ║"
            .color(colors::GRAY)
    );
    println!(
        "{}",
        "║  • Unchanged runs collapsed: /* N unchanged */                       ║"
            .color(colors::GRAY)
    );
    println!(
        "{}",
        "║  • Changed fields get own lines with -/+                             ║"
            .color(colors::GRAY)
    );
    println!(
        "{}",
        "║  • Unchanged fields shown inline (dimmed) or collapsed               ║"
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
    mockup_xml_child_added();
    mockup_xml_child_removed();
    mockup_xml_complex();
    mockup_xml_moved_and_modified();
    mockup_xml_deep_nesting();

    // JSON
    print_section("JSON Format");
    mockup_json_simple();
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
        "is_changed(path)".color(colors::CYAN),
        "→ does this leaf need -/+ treatment?".color(colors::GRAY)
    );
    println!(
        "  {} {}",
        "old_value(path)".color(colors::CYAN),
        "→ what to show on the - line".color(colors::GRAY)
    );
    println!(
        "  {} {}",
        "should_collapse(path)".color(colors::CYAN),
        "→ can we skip this subtree?".color(colors::GRAY)
    );
    println!(
        "  {} {}",
        "is_deleted(path)".color(colors::CYAN),
        "→ exists in old, not in new".color(colors::GRAY)
    );
    println!(
        "  {} {}",
        "is_inserted(path)".color(colors::CYAN),
        "→ exists in new, not in old".color(colors::GRAY)
    );
    println!();
    println!(
        "{}",
        "Layout decisions depend on diff status:".color(colors::WHITE)
    );
    println!();
    println!(
        "  {} {}",
        "•".color(colors::GREEN),
        "Changed attrs → own lines with -/+".color(colors::GRAY)
    );
    println!(
        "  {} {}",
        "•".color(colors::GREEN),
        "Unchanged attrs → inline, dimmed".color(colors::GRAY)
    );
    println!(
        "  {} {}",
        "•".color(colors::GREEN),
        "Many unchanged siblings → collapse".color(colors::GRAY)
    );
    println!(
        "  {} {}",
        "•".color(colors::GREEN),
        "Deleted element → - prefix, full element".color(colors::GRAY)
    );
    println!(
        "  {} {}",
        "•".color(colors::GREEN),
        "Inserted element → + prefix, full element".color(colors::GRAY)
    );
    println!();
}
