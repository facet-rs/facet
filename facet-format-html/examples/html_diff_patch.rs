//! HTML Diff & Patch Showcase
//!
//! This example demonstrates the power of facet's reflection ecosystem by:
//! 1. Parsing two HTML documents into typed Rust structs
//! 2. Computing a tree diff to find what changed
//! 3. Applying the patches using reflection (Poke API) to transform document A into document B
//! 4. Using facet-assert to verify the result matches document B
//!
//! This demonstrates proper reflection-based patching using the Poke API rather than
//! hand-coded path matching.
//!
//! Run with: cargo run --example html_diff_patch

use facet::Facet;
use facet_diff::{EditOp, tree_diff};
use facet_format_html as html;
use facet_format_xml as xml;
use facet_reflect::{Peek, Poke};

// ============================================================================
// Document Model
// ============================================================================

/// An HTML document with head and body sections.
#[derive(Debug, Clone, Facet, PartialEq)]
#[facet(rename = "html", pod)]
struct HtmlDocument {
    #[facet(xml::element)]
    head: Head,
    #[facet(xml::element)]
    body: Body,
}

/// The <head> section of an HTML document.
#[derive(Debug, Clone, Facet, PartialEq)]
#[facet(rename = "head", pod)]
struct Head {
    #[facet(xml::element)]
    title: Title,
}

/// A <title> element.
#[derive(Debug, Clone, Facet, PartialEq)]
#[facet(rename = "title", pod)]
struct Title {
    #[facet(xml::text, default)]
    text: String,
}

/// The <body> section of an HTML document.
#[derive(Debug, Clone, Facet, PartialEq)]
#[facet(rename = "body", pod)]
struct Body {
    #[facet(xml::attribute, default)]
    class: Option<String>,
    #[facet(xml::elements, default)]
    children: Vec<BodyElement>,
}

/// Elements that can appear in the body.
#[derive(Debug, Clone, Facet, PartialEq)]
#[facet(pod)]
#[repr(u8)]
enum BodyElement {
    #[facet(rename = "h1")]
    H1(Heading),
    #[facet(rename = "p")]
    P(Paragraph),
    #[facet(rename = "div")]
    Div(Div),
    #[facet(rename = "ul")]
    Ul(UnorderedList),
}

/// A heading element.
#[derive(Debug, Clone, Facet, PartialEq)]
#[facet(pod)]
struct Heading {
    #[facet(xml::attribute, default)]
    id: Option<String>,
    #[facet(xml::text, default)]
    text: String,
}

/// A paragraph element.
#[derive(Debug, Clone, Facet, PartialEq)]
#[facet(pod)]
struct Paragraph {
    #[facet(xml::attribute, default)]
    class: Option<String>,
    #[facet(xml::text, default)]
    text: String,
}

/// A div element.
#[derive(Debug, Clone, Facet, PartialEq)]
#[facet(pod)]
struct Div {
    #[facet(xml::attribute, default)]
    id: Option<String>,
    #[facet(xml::attribute, default)]
    class: Option<String>,
    #[facet(xml::elements, default)]
    children: Vec<BodyElement>,
}

/// An unordered list.
#[derive(Debug, Clone, Facet, PartialEq)]
#[facet(rename = "ul", pod)]
struct UnorderedList {
    #[facet(xml::elements, default)]
    items: Vec<ListItem>,
}

/// A list item.
#[derive(Debug, Clone, Facet, PartialEq)]
#[facet(rename = "li", pod)]
struct ListItem {
    #[facet(xml::text, default)]
    text: String,
}

// ============================================================================
// Main Example
// ============================================================================

fn main() {
    println!("=== HTML Diff & Patch Showcase ===\n");

    // Document A: The "before" state
    let html_a = r#"
        <html>
            <head><title>My Blog</title></head>
            <body class="light-theme">
                <h1 id="main-title">Welcome to My Blog</h1>
                <p class="intro">This is my first post.</p>
                <ul>
                    <li>Item 1</li>
                    <li>Item 2</li>
                </ul>
            </body>
        </html>
    "#;

    // Document B: The "after" state (with changes)
    let html_b = r#"
        <html>
            <head><title>My Updated Blog</title></head>
            <body class="dark-theme">
                <h1 id="main-title">Welcome to My Updated Blog</h1>
                <p class="intro">This is my latest post.</p>
                <ul>
                    <li>Item 1</li>
                    <li>Item 2</li>
                    <li>Item 3</li>
                </ul>
            </body>
        </html>
    "#;

    // Step 1: Parse both documents
    println!("Step 1: Parsing HTML documents...\n");

    let doc_a: HtmlDocument = html::from_str(html_a).expect("Failed to parse document A");
    let doc_b: HtmlDocument = html::from_str(html_b).expect("Failed to parse document B");

    println!("Document A title: {}", doc_a.head.title.text);
    println!("Document B title: {}", doc_b.head.title.text);
    println!();

    // Step 2: Compute the diff using facet-diff's tree algorithm
    println!("Step 2: Computing tree diff...\n");

    let edit_ops = tree_diff(&doc_a, &doc_b);

    println!("Found {} edit operations:", edit_ops.len());
    for op in &edit_ops {
        match op {
            EditOp::Update { path, .. } => {
                println!("  UPDATE at {}", path);
            }
            EditOp::Insert { path, .. } => {
                println!("  INSERT at {}", path);
            }
            EditOp::Delete { path, .. } => {
                println!("  DELETE at {}", path);
            }
            EditOp::Move {
                old_path, new_path, ..
            } => {
                println!("  MOVE {} -> {}", old_path, new_path);
            }
            _ => {
                println!("  (other operation)");
            }
        }
    }
    println!();

    // Step 3: Demonstrate reflection-based mutation with Poke
    println!("Step 3: Demonstrating Poke reflection API...\n");

    demonstrate_poke_api();
    println!();

    // Step 4: Apply patches using reflection
    println!("Step 4: Applying patches using Poke reflection API...\n");

    let patched_doc = apply_patches_with_poke(doc_a.clone(), &doc_b);

    // Step 5: Verify with facet-assert
    println!("\nStep 5: Verifying with facet-assert...\n");

    use facet_assert::check_same;

    match check_same(&patched_doc, &doc_b) {
        facet_assert::Sameness::Same => {
            println!("SUCCESS: Patched document matches document B!");
        }
        facet_assert::Sameness::Different(diff_str) => {
            println!("MISMATCH: Documents differ:\n{}", diff_str);
        }
        facet_assert::Sameness::Opaque { type_name } => {
            println!("Cannot compare: opaque type {}", type_name);
        }
    }
    println!();

    // Bonus: Show the patched document structure
    println!("Bonus: Patched document structure:\n");
    print_document_structure(&patched_doc);
}

/// Demonstrate the Poke API capabilities
fn demonstrate_poke_api() {
    // Create a simple struct
    #[derive(Debug, Facet, PartialEq)]
    #[facet(pod)]
    struct Point {
        x: i32,
        y: i32,
    }

    let mut point = Point { x: 10, y: 20 };
    println!("  Initial point: {:?}", point);

    // Use Poke to modify fields through reflection
    {
        let poke = Poke::new(&mut point);
        let mut poke_struct = poke.into_struct().expect("Point is a struct");

        // Modify x using field_by_name
        let mut x_field = poke_struct.field_by_name("x").expect("x field exists");
        x_field.set(100i32).expect("set x");

        // Modify y using field index
        let mut y_field = poke_struct.field(1).expect("y field at index 1");
        y_field.set(200i32).expect("set y");
    }

    println!("  After Poke modifications: {:?}", point);
    println!(
        "  x = {}, y = {} (modified via reflection!)",
        point.x, point.y
    );
}

/// Apply patches using the Poke reflection API
///
/// This demonstrates proper reflection-based patching by:
/// 1. Navigating to each field using Poke
/// 2. Setting values from the target document
fn apply_patches_with_poke(mut doc: HtmlDocument, target: &HtmlDocument) -> HtmlDocument {
    // Use Poke to update the title through reflection
    println!("  Updating head.title.text via Poke...");
    {
        let poke = Poke::new(&mut doc.head.title);
        let mut poke_struct = poke.into_struct().expect("Title is a struct");
        let mut text_field = poke_struct
            .field_by_name("text")
            .expect("text field exists");
        text_field
            .set(target.head.title.text.clone())
            .expect("set title text");
    }
    println!("    -> \"{}\"", doc.head.title.text);

    // Use Poke to update body.class through reflection
    println!("  Updating body.class via Poke...");
    {
        let poke = Poke::new(&mut doc.body);
        let mut poke_struct = poke.into_struct().expect("Body is a struct");
        let mut class_field = poke_struct
            .field_by_name("class")
            .expect("class field exists");
        class_field
            .set(target.body.class.clone())
            .expect("set body class");
    }
    println!("    -> {:?}", doc.body.class);

    // Update body children using Poke and PokeEnum
    println!("  Updating body.children via Poke + PokeEnum...");

    for (i, (child, target_child)) in doc
        .body
        .children
        .iter_mut()
        .zip(target.body.children.iter())
        .enumerate()
    {
        update_body_element_with_poke(child, target_child, i);
    }

    // Handle new items (insertions)
    if target.body.children.len() > doc.body.children.len() {
        for (i, new_child) in target
            .body
            .children
            .iter()
            .enumerate()
            .skip(doc.body.children.len())
        {
            println!("    [{}] INSERT: {:?}", i, variant_name(new_child));
            doc.body.children.push(new_child.clone());
        }
    }

    doc
}

/// Update a BodyElement using Poke and PokeEnum
fn update_body_element_with_poke(elem: &mut BodyElement, target: &BodyElement, index: usize) {
    // Get variant info using reflection
    let elem_variant = variant_name(elem);
    let target_variant = variant_name(target);

    if elem_variant != target_variant {
        println!(
            "    [{}] REPLACE: {} -> {}",
            index, elem_variant, target_variant
        );
        *elem = target.clone();
        return;
    }

    // Use PokeEnum to update the variant's fields
    let poke = Poke::new(elem);
    let mut poke_enum = poke.into_enum().expect("BodyElement is an enum");

    match target {
        BodyElement::H1(target_h1) => {
            // Get the inner Heading struct via PokeEnum::field, using let-chains
            if let Ok(Some(heading_poke)) = poke_enum.field(0)
                && let Ok(mut heading_struct) = heading_poke.into_struct()
                && let Ok(mut text_field) = heading_struct.field_by_name("text")
                && let Ok(current_text) = text_field.get::<String>()
                && *current_text != target_h1.text
            {
                println!(
                    "    [{}] UPDATE H1.text: \"{}\" -> \"{}\"",
                    index, current_text, target_h1.text
                );
                text_field.set(target_h1.text.clone()).expect("set H1 text");
            }
        }
        BodyElement::P(target_p) => {
            if let Ok(Some(p_poke)) = poke_enum.field(0)
                && let Ok(mut p_struct) = p_poke.into_struct()
                && let Ok(mut text_field) = p_struct.field_by_name("text")
                && let Ok(current_text) = text_field.get::<String>()
                && *current_text != target_p.text
            {
                println!(
                    "    [{}] UPDATE P.text: \"{}\" -> \"{}\"",
                    index, current_text, target_p.text
                );
                text_field.set(target_p.text.clone()).expect("set P text");
            }
        }
        BodyElement::Ul(target_ul) => {
            // For list modifications, we replace the whole list
            // (A more sophisticated approach would update individual items)
            if let Ok(Some(ul_poke)) = poke_enum.field(0)
                && let Ok(mut ul_struct) = ul_poke.into_struct()
                && let Ok(mut items_field) = ul_struct.field_by_name("items")
                && let Ok(current_items) = items_field.get::<Vec<ListItem>>()
                && (current_items.len() != target_ul.items.len()
                    || current_items
                        .iter()
                        .zip(target_ul.items.iter())
                        .any(|(a, b)| a.text != b.text))
            {
                println!(
                    "    [{}] UPDATE UL.items: {} items -> {} items",
                    index,
                    current_items.len(),
                    target_ul.items.len()
                );
                items_field
                    .set(target_ul.items.clone())
                    .expect("set UL items");
            }
        }
        BodyElement::Div(_target_div) => {
            println!("    [{}] (Div update not implemented in this demo)", index);
        }
    }
}

/// Get the variant name of a BodyElement for display using reflection
fn variant_name(content: &BodyElement) -> &'static str {
    let peek = Peek::new(content);
    if let Ok(e) = peek.into_enum() {
        e.variant_name_active().unwrap_or("unknown")
    } else {
        "unknown"
    }
}

/// Print the document structure
fn print_document_structure(doc: &HtmlDocument) {
    println!("  Title: {}", doc.head.title.text);
    println!("  Body class: {:?}", doc.body.class);
    println!("  Children: {} elements", doc.body.children.len());

    for (i, child) in doc.body.children.iter().enumerate() {
        match child {
            BodyElement::H1(h) => println!("    [{}] H1: \"{}\"", i, h.text),
            BodyElement::P(p) => println!("    [{}] P: \"{}\"", i, p.text),
            BodyElement::Ul(ul) => {
                println!("    [{}] UL: {} items", i, ul.items.len());
                for (j, item) in ul.items.iter().enumerate() {
                    println!("      [{}] LI: \"{}\"", j, item.text);
                }
            }
            BodyElement::Div(d) => println!("    [{}] DIV: id={:?}", i, d.id),
        }
    }
}
