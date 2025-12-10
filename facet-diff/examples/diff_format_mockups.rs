//! Diff Format Mockups
//!
//! This example shows MOCKUPS of what diff output SHOULD look like when diffing
//! SVG/XML documents parsed via facet-xml.
//!
//! The core use case:
//! 1. Parse old.svg → Svg struct (via facet-xml)
//! 2. Parse new.svg → Svg struct
//! 3. old.diff(&new) → get structural diff
//! 4. Render that diff beautifully WITHOUT rendering the entire tree
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
// MOCKUP: SVG Diff - Compact Path Format
// ============================================================================

fn mockup_svg_compact() {
    print_subsection("Single attribute change in nested element");

    // Scenario: rect's fill changed from "red" to "blue"
    println!(
        "  {}: {} {} {}",
        "children[0].Rect.fill".color(colors::CYAN),
        "\"red\"".color(colors::RED),
        "→".color(colors::GRAY),
        "\"blue\"".color(colors::GREEN)
    );

    print_subsection("Multiple scattered changes");

    // Several changes across the SVG
    println!(
        "  {}: {} {} {}",
        "view_box".color(colors::CYAN),
        "\"0 0 100 100\"".color(colors::RED),
        "→".color(colors::GRAY),
        "\"0 0 200 200\"".color(colors::GREEN)
    );
    println!(
        "  {}: {} {} {}",
        "children[0].Rect.x".color(colors::CYAN),
        "\"10\"".color(colors::RED),
        "→".color(colors::GRAY),
        "\"20\"".color(colors::GREEN)
    );
    println!(
        "  {}: {} {} {}",
        "children[1].Circle.r".color(colors::CYAN),
        "\"25\"".color(colors::RED),
        "→".color(colors::GRAY),
        "\"30\"".color(colors::GREEN)
    );
    println!(
        "  {}: {} {} {}",
        "children[2].Group.children[0].Path.stroke".color(colors::CYAN),
        "\"black\"".color(colors::RED),
        "→".color(colors::GRAY),
        "\"#333\"".color(colors::GREEN)
    );

    print_subsection("Element added/removed in sequence");

    println!(
        "  {}: {}",
        "children[3]".color(colors::CYAN),
        "deleted Rect { x: \"50\", y: \"50\", ... }".color(colors::RED)
    );
    println!(
        "  {}: {}",
        "children[3]".color(colors::CYAN),
        "inserted Circle { cx: \"75\", cy: \"75\", r: \"20\" }".color(colors::GREEN)
    );
}

// ============================================================================
// MOCKUP: SVG Diff - Tree Format (like current facet-diff Display)
// ============================================================================

fn mockup_svg_tree() {
    print_subsection("Tree format showing structure with collapsed unchanged");

    // This shows the hierarchical structure but collapses unchanged parts
    println!("{}", "Svg {".color(colors::GRAY));
    println!(
        "  {} {}",
        ".. 3 unchanged attributes".color(colors::GRAY).italic(),
        "(width, height, xmlns)".color(colors::GRAY).dimmed()
    );
    println!(
        "  {}{} {} {} {}",
        "view_box".color(colors::CYAN),
        ":".color(colors::GRAY),
        "\"0 0 100 100\"".color(colors::RED),
        "→".color(colors::GRAY),
        "\"0 0 200 200\"".color(colors::GREEN)
    );
    println!(
        "  {}{} {} {{",
        "children".color(colors::CYAN),
        ":".color(colors::GRAY),
        "[".color(colors::GRAY)
    );
    println!(
        "    {}{} {{",
        "[0]".color(colors::BLUE),
        " Rect".color(colors::WHITE)
    );
    println!(
        "      {}{} {} {} {}",
        "fill".color(colors::CYAN),
        ":".color(colors::GRAY),
        "\"red\"".color(colors::RED),
        "→".color(colors::GRAY),
        "\"blue\"".color(colors::GREEN)
    );
    println!(
        "      {} {}",
        ".. 4 unchanged".color(colors::GRAY).italic(),
        "(x, y, width, height)".color(colors::GRAY).dimmed()
    );
    println!("    {}", "}".color(colors::GRAY));
    println!(
        "    {} {}",
        ".. 2 unchanged elements".color(colors::GRAY).italic(),
        "([1] Circle, [2] Path)".color(colors::GRAY).dimmed()
    );
    println!("  {} {}", "]".color(colors::GRAY), "}".color(colors::GRAY));
    println!("{}", "}".color(colors::GRAY));
}

// ============================================================================
// MOCKUP: SVG Diff - XML-style output (renders back as XML)
// ============================================================================

fn mockup_svg_xml() {
    print_subsection("XML-style diff output (could be valid XML)");

    println!("{}", "<!-- SVG Diff: 3 changes -->".color(colors::GRAY));
    println!("{}", "<svg".color(colors::WHITE));
    println!(
        "  {} {}{}{}",
        "-".color(colors::RED),
        "viewBox=\"".color(colors::RED),
        "0 0 100 100".color(colors::RED),
        "\"".color(colors::RED)
    );
    println!(
        "  {} {}{}{}",
        "+".color(colors::GREEN),
        "viewBox=\"".color(colors::GREEN),
        "0 0 200 200".color(colors::GREEN),
        "\"".color(colors::GREEN)
    );
    println!(
        "    {}",
        "xmlns=\"http://www.w3.org/2000/svg\">"
            .color(colors::GRAY)
            .dimmed()
    );
    println!();
    println!(
        "  {}",
        "<!-- ... 1 unchanged element (circle) ... -->".color(colors::GRAY)
    );
    println!();
    println!("  {}", "<rect".color(colors::WHITE));
    println!(
        "    {} {}{}{}",
        "-".color(colors::RED),
        "fill=\"".color(colors::RED),
        "red".color(colors::RED),
        "\"".color(colors::RED)
    );
    println!(
        "    {} {}{}{}",
        "+".color(colors::GREEN),
        "fill=\"".color(colors::GREEN),
        "blue".color(colors::GREEN),
        "\"".color(colors::GREEN)
    );
    println!(
        "      {} {}",
        "x=\"10\" y=\"10\" width=\"50\" height=\"50\""
            .color(colors::GRAY)
            .dimmed(),
        "<!-- unchanged -->".color(colors::GRAY)
    );
    println!("  {}", "/>".color(colors::WHITE));
    println!();
    println!(
        "  {}",
        "<!-- ... 1 unchanged element (path) ... -->".color(colors::GRAY)
    );
    println!();
    println!("{}", "</svg>".color(colors::WHITE));
}

// ============================================================================
// MOCKUP: SVG Diff - Side-by-side
// ============================================================================

fn mockup_svg_side_by_side() {
    print_subsection("Side-by-side comparison (terminal)");

    println!(
        "{}{}{}",
        "┌─ OLD ".color(colors::RED),
        "─".repeat(28).color(colors::GRAY),
        "┬─ NEW ─────────────────────────┐".color(colors::GREEN)
    );

    // Unchanged line
    println!(
        "{} {:<33}{} {:<33}{}",
        "│".color(colors::GRAY),
        "<svg".color(colors::GRAY).dimmed(),
        "│".color(colors::GRAY),
        "<svg".color(colors::GRAY).dimmed(),
        "│".color(colors::GRAY)
    );

    // Changed line - viewBox
    println!(
        "{} {:<33}{} {:<33}{}",
        "│".color(colors::RED),
        "  viewBox=\"0 0 100 100\"".color(colors::RED),
        "│".color(colors::YELLOW),
        "  viewBox=\"0 0 200 200\"".color(colors::GREEN),
        "│".color(colors::GREEN)
    );

    // Unchanged
    println!(
        "{} {:<33}{} {:<33}{}",
        "│".color(colors::GRAY),
        "  xmlns=\"...\">".color(colors::GRAY).dimmed(),
        "│".color(colors::GRAY),
        "  xmlns=\"...\">".color(colors::GRAY).dimmed(),
        "│".color(colors::GRAY)
    );

    // Unchanged
    println!(
        "{} {:<33}{} {:<33}{}",
        "│".color(colors::GRAY),
        "  <rect".color(colors::GRAY).dimmed(),
        "│".color(colors::GRAY),
        "  <rect".color(colors::GRAY).dimmed(),
        "│".color(colors::GRAY)
    );

    // Changed line - fill
    println!(
        "{} {:<33}{} {:<33}{}",
        "│".color(colors::RED),
        "    fill=\"red\"".color(colors::RED),
        "│".color(colors::YELLOW),
        "    fill=\"blue\"".color(colors::GREEN),
        "│".color(colors::GREEN)
    );

    // Unchanged
    println!(
        "{} {:<33}{} {:<33}{}",
        "│".color(colors::GRAY),
        "    x=\"10\" y=\"10\" .../>".color(colors::GRAY).dimmed(),
        "│".color(colors::GRAY),
        "    x=\"10\" y=\"10\" .../>".color(colors::GRAY).dimmed(),
        "│".color(colors::GRAY)
    );

    // Collapsed
    println!(
        "{} {:<33}{} {:<33}{}",
        "│".color(colors::GRAY),
        "  <!-- 2 more elements -->".color(colors::GRAY).dimmed(),
        "│".color(colors::GRAY),
        "  <!-- 2 more elements -->".color(colors::GRAY).dimmed(),
        "│".color(colors::GRAY)
    );

    // Unchanged
    println!(
        "{} {:<33}{} {:<33}{}",
        "│".color(colors::GRAY),
        "</svg>".color(colors::GRAY).dimmed(),
        "│".color(colors::GRAY),
        "</svg>".color(colors::GRAY).dimmed(),
        "│".color(colors::GRAY)
    );

    println!(
        "└{}┴{}┘",
        "─".repeat(34).color(colors::GRAY),
        "─".repeat(34).color(colors::GRAY)
    );
}

// ============================================================================
// MOCKUP: Deep nesting (the hard case)
// ============================================================================

fn mockup_deep_nesting() {
    print_subsection("Deep nesting - breadcrumb + focused view");

    // Show the path to the change
    println!(
        "{} {}",
        "Path:".color(colors::BLUE),
        "svg › children[2] › Group › children[0] › Group › children[3] › Path".color(colors::CYAN)
    );
    println!();

    // Show just the changed element with minimal context
    println!("{}", "Path {".color(colors::WHITE));
    println!(
        "  {}: {} {} {}",
        "d".color(colors::CYAN),
        "\"M10 10 L90 90\"".color(colors::RED),
        "→".color(colors::GRAY),
        "\"M10 10 Q50 50 90 90\"".color(colors::GREEN)
    );
    println!(
        "  {}: {} {} {}",
        "stroke".color(colors::CYAN),
        "\"black\"".color(colors::RED),
        "→".color(colors::GRAY),
        "\"#333\"".color(colors::GREEN)
    );
    println!(
        "  {} {}",
        ".. 2 unchanged".color(colors::GRAY).italic(),
        "(stroke_width, fill)".color(colors::GRAY).dimmed()
    );
    println!("{}", "}".color(colors::WHITE));

    println!();
    println!("{}", "Collapsed ancestors:".color(colors::GRAY));
    println!(
        "  {} {{ {} }}",
        "Svg".color(colors::GRAY).dimmed(),
        "...".color(colors::GRAY).dimmed()
    );
    println!(
        "    {} {{ {} }}",
        "└─ Group[2]".color(colors::GRAY).dimmed(),
        "id: \"layer1\", ...".color(colors::GRAY).dimmed()
    );
    println!(
        "      {} {{ {} }}",
        "└─ Group[0]".color(colors::GRAY).dimmed(),
        "id: \"shapes\", ...".color(colors::GRAY).dimmed()
    );
    println!(
        "        {} {{ {} }}",
        "└─ Path[3]".color(colors::YELLOW),
        "← CHANGED".color(colors::YELLOW)
    );
}

// ============================================================================
// MOCKUP: Sequence changes (elements added/removed/moved)
// ============================================================================

fn mockup_sequence_changes() {
    print_subsection("Sequence changes - elements added/removed");

    println!("{}", "children: [".color(colors::WHITE));
    println!(
        "    {} {}",
        "[0] Rect { ... }".color(colors::GRAY).dimmed(),
        "unchanged".color(colors::GRAY).italic()
    );
    println!(
        "    {} {}",
        "[1] Circle { ... }".color(colors::GRAY).dimmed(),
        "unchanged".color(colors::GRAY).italic()
    );
    println!(
        "  {} {} {}",
        "-".color(colors::RED),
        "[2] Path { d: \"M...\" }".color(colors::RED),
        "DELETED".color(colors::RED)
    );
    println!(
        "  {} {} {}",
        "+".color(colors::GREEN),
        "[2] Group { id: \"new-group\", children: [...] }".color(colors::GREEN),
        "INSERTED".color(colors::GREEN)
    );
    println!(
        "    {} {}",
        "[3] Text { ... }".color(colors::GRAY).dimmed(),
        "unchanged".color(colors::GRAY).italic()
    );
    println!("{}", "]".color(colors::WHITE));

    print_subsection("Move detection (tree diff enriches this)");

    // Tree diff can detect that an element moved rather than was deleted+inserted
    println!(
        "  {} {} {}",
        "Circle { cx: \"50\", cy: \"50\", ... }".color(colors::BLUE),
        "moved".color(colors::YELLOW),
        "[1] → [3]".color(colors::YELLOW)
    );
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
        "║                     SVG DIFF FORMAT MOCKUPS                          ║"
            .color(colors::YELLOW)
    );
    println!(
        "{}",
        "║                                                                      ║"
            .color(colors::YELLOW)
    );
    println!(
        "{}",
        "║  These show what diff output SHOULD look like when comparing two    ║"
            .color(colors::GRAY)
    );
    println!(
        "{}",
        "║  SVG documents parsed via facet-xml. The key challenge:             ║"
            .color(colors::GRAY)
    );
    println!(
        "{}",
        "║                                                                      ║"
            .color(colors::YELLOW)
    );
    println!(
        "{}",
        "║  • Don't render the entire tree (could be huge)                     ║"
            .color(colors::GRAY)
    );
    println!(
        "{}",
        "║  • Collapse/skip unchanged subtrees                                 ║"
            .color(colors::GRAY)
    );
    println!(
        "{}",
        "║  • Highlight what actually changed                                  ║"
            .color(colors::GRAY)
    );
    println!(
        "{}",
        "║  • Show enough context to understand WHERE the change is            ║"
            .color(colors::GRAY)
    );
    println!(
        "{}",
        "╚══════════════════════════════════════════════════════════════════════╝"
            .color(colors::YELLOW)
    );

    print_section("Compact Path Format (current facet-diff style)");
    mockup_svg_compact();

    print_section("Tree Format (hierarchical with collapsed unchanged)");
    mockup_svg_tree();

    print_section("XML-Style Output (renders back as markup)");
    mockup_svg_xml();

    print_section("Side-by-Side (terminal vimdiff style)");
    mockup_svg_side_by_side();

    print_section("Deep Nesting (breadcrumb + focused view)");
    mockup_deep_nesting();

    print_section("Sequence Changes (add/remove/move elements)");
    mockup_sequence_changes();

    println!();
    println!("{}", "═".repeat(70).color(colors::YELLOW));
    println!("  {}", "Design Notes".color(colors::YELLOW).bold());
    println!("{}", "═".repeat(70).color(colors::YELLOW));
    println!();
    println!(
        "{}",
        "The key insight: we need to marry facet-diff's Diff tree with".color(colors::WHITE)
    );
    println!(
        "{}",
        "format-specific serializers (facet-xml, facet-json, etc).".color(colors::WHITE)
    );
    println!();
    println!("{}", "Options:".color(colors::BLUE));
    println!(
        "  {} Walk the Diff tree, emit format-specific syntax",
        "1.".color(colors::CYAN)
    );
    println!(
        "  {} Wrap Peek to return \"...\" for unchanged subtrees",
        "2.".color(colors::CYAN)
    );
    println!(
        "  {} Two-pass: collect changed paths, then serialize with filter",
        "3.".color(colors::CYAN)
    );
    println!();
    println!(
        "{}",
        "For SVG specifically, we could even render an ANNOTATED SVG:".color(colors::WHITE)
    );
    println!(
        "  {} Changed elements highlighted with colored outlines",
        "•".color(colors::GREEN)
    );
    println!(
        "  {} Deleted elements shown with red strikethrough overlay",
        "•".color(colors::RED)
    );
    println!(
        "  {} Inserted elements shown with green glow",
        "•".color(colors::GREEN)
    );
    println!();
}
