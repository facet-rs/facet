//! HTML Diff & Patch Showcase
//!
//! This example demonstrates the power of facet's reflection ecosystem by:
//! 1. Parsing two HTML documents into typed Rust structs
//! 2. Computing a tree diff to find what changed
//! 3. Displaying the changes as patch instructions
//! 4. Applying the patches to transform document A into document B
//! 5. Using facet-assert to verify the result matches document B
//!
//! Run with: cargo run --example html_diff_patch

use facet::Facet;
use facet_format_html as html;
use facet_format_xml as xml;

// ============================================================================
// Document Model - A simple HTML page structure
// ============================================================================

/// An HTML document with head and body sections.
#[derive(Debug, Clone, Facet, PartialEq)]
#[facet(rename = "html")]
struct HtmlDocument {
    #[facet(xml::element)]
    head: Head,
    #[facet(xml::element)]
    body: Body,
}

/// The <head> section of an HTML document.
#[derive(Debug, Clone, Facet, PartialEq)]
#[facet(rename = "head")]
struct Head {
    #[facet(xml::element)]
    title: Title,
}

/// A <title> element.
#[derive(Debug, Clone, Facet, PartialEq)]
#[facet(rename = "title")]
struct Title {
    #[facet(xml::text, default)]
    text: String,
}

/// The <body> section of an HTML document.
#[derive(Debug, Clone, Facet, PartialEq)]
#[facet(rename = "body")]
struct Body {
    #[facet(xml::attribute, default)]
    class: Option<String>,
    #[facet(xml::elements, default)]
    children: Vec<BodyElement>,
}

/// Elements that can appear in the body.
#[derive(Debug, Clone, Facet, PartialEq)]
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
struct Heading {
    #[facet(xml::attribute, default)]
    id: Option<String>,
    #[facet(xml::text, default)]
    text: String,
}

/// A paragraph element.
#[derive(Debug, Clone, Facet, PartialEq)]
struct Paragraph {
    #[facet(xml::attribute, default)]
    class: Option<String>,
    #[facet(xml::text, default)]
    text: String,
}

/// A div element.
#[derive(Debug, Clone, Facet, PartialEq)]
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
#[facet(rename = "ul")]
struct UnorderedList {
    #[facet(xml::elements, default)]
    items: Vec<ListItem>,
}

/// A list item.
#[derive(Debug, Clone, Facet, PartialEq)]
#[facet(rename = "li")]
struct ListItem {
    #[facet(xml::text, default)]
    text: String,
}

// ============================================================================
// Patch Operations
// ============================================================================

/// A patch operation that can transform a document.
#[derive(Debug, Clone)]
enum PatchOp {
    /// Update a value at the given path
    Update {
        path: String,
        from: String,
        to: String,
    },
    /// Insert a value at the given path
    Insert { path: String, value: String },
    /// Delete a value at the given path
    Delete { path: String, value: String },
}

impl std::fmt::Display for PatchOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PatchOp::Update { path, from, to } => {
                write!(f, "UPDATE {}: {} -> {}", path, from, to)
            }
            PatchOp::Insert { path, value } => {
                write!(f, "INSERT {}: {}", path, value)
            }
            PatchOp::Delete { path, value } => {
                write!(f, "DELETE {}: {}", path, value)
            }
        }
    }
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

    use facet_diff::{EditOp, tree_diff};

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

    // Step 3: Compute semantic diff for detailed changes
    println!("Step 3: Computing semantic diff...\n");

    use facet_diff::{FacetDiff, collect_leaf_changes};

    let diff = doc_a.diff(&doc_b);
    let leaf_changes = collect_leaf_changes(&diff);

    println!("Leaf-level changes:");
    for change in &leaf_changes {
        println!("  {}", change);
    }
    println!();

    // Step 4: Generate patch instructions
    println!("Step 4: Generating patch instructions...\n");

    let patches = generate_patches(&doc_a, &doc_b);
    for patch in &patches {
        println!("  {}", patch);
    }
    println!();

    // Step 5: Apply patches by reconstructing
    println!("Step 5: Applying patches...\n");

    let patched_doc = apply_patches(doc_a.clone(), &patches);

    println!("Patched document title: {}", patched_doc.head.title.text);
    println!("Patched body class: {:?}", patched_doc.body.class);
    println!();

    // Step 6: Verify with facet-assert
    println!("Step 6: Verifying with facet-assert...\n");

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
    println!("  Title: {}", patched_doc.head.title.text);
    println!("  Body class: {:?}", patched_doc.body.class);
    println!("  Children: {} elements", patched_doc.body.children.len());
    for (i, child) in patched_doc.body.children.iter().enumerate() {
        match child {
            BodyElement::H1(h) => println!("    [{}] H1: {}", i, h.text),
            BodyElement::P(p) => println!("    [{}] P: {}", i, p.text),
            BodyElement::Ul(ul) => {
                println!("    [{}] UL: {} items", i, ul.items.len());
                for (j, item) in ul.items.iter().enumerate() {
                    println!("      [{}] LI: {}", j, item.text);
                }
            }
            BodyElement::Div(d) => println!("    [{}] DIV: id={:?}", i, d.id),
        }
    }
}

/// Generate patch operations by comparing two documents.
fn generate_patches(from: &HtmlDocument, to: &HtmlDocument) -> Vec<PatchOp> {
    let mut patches = Vec::new();

    // Compare title
    if from.head.title.text != to.head.title.text {
        patches.push(PatchOp::Update {
            path: "head.title.text".to_string(),
            from: from.head.title.text.clone(),
            to: to.head.title.text.clone(),
        });
    }

    // Compare body class
    if from.body.class != to.body.class {
        patches.push(PatchOp::Update {
            path: "body.class".to_string(),
            from: format!("{:?}", from.body.class),
            to: format!("{:?}", to.body.class),
        });
    }

    // Compare body children
    let from_children = &from.body.children;
    let to_children = &to.body.children;

    // Simple comparison - check each position
    let max_len = from_children.len().max(to_children.len());
    for i in 0..max_len {
        match (from_children.get(i), to_children.get(i)) {
            (Some(from_elem), Some(to_elem)) => {
                compare_body_elements(
                    &mut patches,
                    &format!("body.children[{}]", i),
                    from_elem,
                    to_elem,
                );
            }
            (None, Some(to_elem)) => {
                patches.push(PatchOp::Insert {
                    path: format!("body.children[{}]", i),
                    value: format!("{:?}", to_elem),
                });
            }
            (Some(from_elem), None) => {
                patches.push(PatchOp::Delete {
                    path: format!("body.children[{}]", i),
                    value: format!("{:?}", from_elem),
                });
            }
            (None, None) => unreachable!(),
        }
    }

    patches
}

/// Compare two body elements and generate patches.
fn compare_body_elements(
    patches: &mut Vec<PatchOp>,
    path: &str,
    from: &BodyElement,
    to: &BodyElement,
) {
    match (from, to) {
        (BodyElement::H1(h1_from), BodyElement::H1(h1_to)) => {
            if h1_from.text != h1_to.text {
                patches.push(PatchOp::Update {
                    path: format!("{}.text", path),
                    from: h1_from.text.clone(),
                    to: h1_to.text.clone(),
                });
            }
        }
        (BodyElement::P(p_from), BodyElement::P(p_to)) => {
            if p_from.text != p_to.text {
                patches.push(PatchOp::Update {
                    path: format!("{}.text", path),
                    from: p_from.text.clone(),
                    to: p_to.text.clone(),
                });
            }
        }
        (BodyElement::Ul(ul_from), BodyElement::Ul(ul_to)) => {
            let max_len = ul_from.items.len().max(ul_to.items.len());
            for i in 0..max_len {
                match (ul_from.items.get(i), ul_to.items.get(i)) {
                    (Some(li_from), Some(li_to)) if li_from.text != li_to.text => {
                        patches.push(PatchOp::Update {
                            path: format!("{}.items[{}].text", path, i),
                            from: li_from.text.clone(),
                            to: li_to.text.clone(),
                        });
                    }
                    (None, Some(li_to)) => {
                        patches.push(PatchOp::Insert {
                            path: format!("{}.items[{}]", path, i),
                            value: li_to.text.clone(),
                        });
                    }
                    (Some(li_from), None) => {
                        patches.push(PatchOp::Delete {
                            path: format!("{}.items[{}]", path, i),
                            value: li_from.text.clone(),
                        });
                    }
                    _ => {}
                }
            }
        }
        _ => {
            // Type changed - generate replace
            patches.push(PatchOp::Update {
                path: path.to_string(),
                from: format!("{:?}", from),
                to: format!("{:?}", to),
            });
        }
    }
}

/// Apply patches to transform a document.
///
/// This creates a new document by cloning and modifying based on the patches.
fn apply_patches(mut doc: HtmlDocument, patches: &[PatchOp]) -> HtmlDocument {
    for patch in patches {
        match patch {
            PatchOp::Update { path, to, .. } => {
                apply_update(&mut doc, path, to);
            }
            PatchOp::Insert { path, value } => {
                apply_insert(&mut doc, path, value);
            }
            PatchOp::Delete { path, .. } => {
                apply_delete(&mut doc, path);
            }
        }
    }
    doc
}

fn apply_update(doc: &mut HtmlDocument, path: &str, to: &str) {
    match path {
        "head.title.text" => {
            doc.head.title.text = to.to_string();
        }
        "body.class" => {
            // Parse the Option<String> format
            if to == "None" {
                doc.body.class = None;
            } else if let Some(inner) = to
                .strip_prefix("Some(\"")
                .and_then(|s| s.strip_suffix("\")"))
            {
                doc.body.class = Some(inner.to_string());
            }
        }
        _ if path.starts_with("body.children[") => {
            // Parse index and field
            if let Some(rest) = path.strip_prefix("body.children[")
                && let Some((idx_str, field)) = rest.split_once(']')
                && let Ok(idx) = idx_str.parse::<usize>()
                && let Some(elem) = doc.body.children.get_mut(idx)
            {
                apply_element_update(elem, field, to);
            }
        }
        _ => {
            eprintln!("Unknown path for update: {}", path);
        }
    }
}

fn apply_element_update(elem: &mut BodyElement, field: &str, to: &str) {
    match elem {
        BodyElement::H1(h1) if field == ".text" => {
            h1.text = to.to_string();
        }
        BodyElement::P(p) if field == ".text" => {
            p.text = to.to_string();
        }
        BodyElement::Ul(ul) if field.starts_with(".items[") => {
            if let Some(rest) = field.strip_prefix(".items[")
                && let Some((idx_str, inner_field)) = rest.split_once(']')
                && let Ok(idx) = idx_str.parse::<usize>()
                && inner_field == ".text"
                && let Some(item) = ul.items.get_mut(idx)
            {
                item.text = to.to_string();
            }
        }
        _ => {
            eprintln!("Unknown element/field combination");
        }
    }
}

fn apply_insert(doc: &mut HtmlDocument, path: &str, value: &str) {
    // Handle list item insertions
    if path.starts_with("body.children[")
        && let Some(rest) = path.strip_prefix("body.children[")
        && let Some((idx_str, field)) = rest.split_once(']')
        && let Ok(idx) = idx_str.parse::<usize>()
        && field.starts_with(".items[")
        && let Some(BodyElement::Ul(ul)) = doc.body.children.get_mut(idx)
    {
        ul.items.push(ListItem {
            text: value.to_string(),
        });
    }
}

fn apply_delete(doc: &mut HtmlDocument, path: &str) {
    // Handle list item deletions
    if path.starts_with("body.children[")
        && let Some(rest) = path.strip_prefix("body.children[")
        && let Some((idx_str, field)) = rest.split_once(']')
        && let Ok(idx) = idx_str.parse::<usize>()
        && field.starts_with(".items[")
        && let Some(rest2) = field.strip_prefix(".items[")
        && let Some((item_idx_str, _)) = rest2.split_once(']')
        && let Ok(item_idx) = item_idx_str.parse::<usize>()
        && let Some(BodyElement::Ul(ul)) = doc.body.children.get_mut(idx)
        && item_idx < ul.items.len()
    {
        ul.items.remove(item_idx);
    }
}
