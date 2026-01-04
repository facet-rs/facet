//! facet-html Showcase
//!
//! This example demonstrates facet-html's capabilities for HTML parsing and serialization,
//! including custom type definitions and integration with facet-html-dom.
//!
//! Run with: cargo run -p facet-html --example html_showcase

use facet::Facet;
use facet_html as html;
use facet_showcase::{Language, ShowcaseRunner};

// =============================================================================
// Custom Type Definitions
// =============================================================================

/// A simple page structure with head and body.
#[derive(Facet, Debug)]
#[facet(rename = "html")]
struct SimplePage {
    #[facet(html::element, default)]
    head: Option<SimpleHead>,
    #[facet(html::element, default)]
    body: Option<SimpleBody>,
}

#[derive(Facet, Debug)]
#[facet(rename = "head")]
struct SimpleHead {
    #[facet(html::element, default)]
    title: Option<SimpleTitle>,
}

#[derive(Facet, Debug)]
#[facet(rename = "title")]
struct SimpleTitle {
    #[facet(html::text, default)]
    text: String,
}

#[derive(Facet, Debug)]
#[facet(rename = "body")]
struct SimpleBody {
    #[facet(html::attribute, default)]
    class: Option<String>,
    #[facet(html::elements, default)]
    children: Vec<BodyElement>,
}

/// Elements that can appear in the body.
#[allow(dead_code)]
#[derive(Facet, Debug)]
#[repr(u8)]
enum BodyElement {
    #[facet(rename = "h1")]
    H1(Heading),
    #[facet(rename = "p")]
    P(Paragraph),
    #[facet(rename = "div")]
    Div(DivElement),
}

#[derive(Facet, Debug)]
struct Heading {
    #[facet(html::attribute, default)]
    id: Option<String>,
    #[facet(html::text, default)]
    text: String,
}

#[derive(Facet, Debug)]
struct Paragraph {
    #[facet(html::attribute, default)]
    class: Option<String>,
    #[facet(html::text, default)]
    text: String,
}

#[derive(Facet, Debug)]
struct DivElement {
    #[facet(html::attribute, default)]
    id: Option<String>,
    #[facet(html::attribute, default)]
    class: Option<String>,
    #[facet(html::text, default)]
    content: String,
}

/// A form element with various input types.
#[derive(Facet, Debug)]
#[facet(rename = "form")]
struct ContactForm {
    #[facet(html::attribute, default)]
    action: Option<String>,
    #[facet(html::attribute, default)]
    method: Option<String>,
    #[facet(html::elements, default)]
    inputs: Vec<FormInput>,
}

#[derive(Facet, Debug)]
#[facet(rename = "input")]
struct FormInput {
    #[facet(html::attribute, default, rename = "type")]
    input_type: Option<String>,
    #[facet(html::attribute, default)]
    name: Option<String>,
    #[facet(html::attribute, default)]
    placeholder: Option<String>,
    #[facet(html::attribute, default)]
    required: Option<String>,
}

/// A div that captures extra attributes (data-*, aria-*, etc.)
#[derive(Facet, Debug, Default)]
#[facet(rename = "div")]
struct DivWithExtras {
    #[facet(html::attribute, default)]
    id: Option<String>,
    #[facet(html::attribute, default)]
    class: Option<String>,
    /// Captures data-*, aria-*, and other unknown attributes
    #[facet(flatten, default)]
    extra: std::collections::BTreeMap<String, String>,
    #[facet(html::text, default)]
    content: String,
}

fn main() {
    let mut runner = ShowcaseRunner::new("HTML").language(Language::Html);

    runner.header();
    runner.intro("[`facet-html`](https://docs.rs/facet-html) parses and serializes HTML documents using Facet. Define your document structure with `#[facet(html::element)]` for child elements, `#[facet(html::attribute)]` for tag attributes, and `#[facet(html::text)]` for text content.");

    // =========================================================================
    // PART 1: Basic Parsing
    // =========================================================================
    runner.section("Parsing HTML");

    showcase_simple_document(&mut runner);
    showcase_nested_elements(&mut runner);
    showcase_form_elements(&mut runner);

    // =========================================================================
    // PART 2: Serialization
    // =========================================================================
    runner.section("Serialization");

    showcase_serialize_minified(&mut runner);
    showcase_serialize_pretty(&mut runner);

    // =========================================================================
    // PART 3: Advanced Features
    // =========================================================================
    runner.section("Advanced Features");

    showcase_extra_attributes(&mut runner);

    runner.footer();
}

// =============================================================================
// Parsing Scenarios
// =============================================================================

fn showcase_simple_document(runner: &mut ShowcaseRunner) {
    let input = r#"<html>
    <head><title>My Page</title></head>
    <body class="main">
        <h1 id="header">Welcome</h1>
        <p>Hello, world!</p>
    </body>
</html>"#;

    let result: SimplePage = html::from_str(input).expect("valid HTML");

    runner
        .scenario("Simple Document")
        .description("Parse a basic HTML document with head, body, and nested elements.")
        .target_type::<SimplePage>()
        .input(Language::Html, input)
        .success(&result)
        .finish();
}

fn showcase_nested_elements(runner: &mut ShowcaseRunner) {
    let input = r#"<html>
    <body>
        <div id="container" class="wrapper">
            <h1>Title</h1>
            <p class="intro">Introduction paragraph.</p>
            <div class="content">Main content here.</div>
        </div>
    </body>
</html>"#;

    let result: SimplePage = html::from_str(input).expect("valid HTML");

    runner
        .scenario("Nested Elements")
        .description("Parse nested HTML elements into an enum-based content model.")
        .target_type::<SimplePage>()
        .input(Language::Html, input)
        .success(&result)
        .finish();
}

fn showcase_form_elements(runner: &mut ShowcaseRunner) {
    let input = r#"<form action="/submit" method="post">
    <input type="text" name="username" placeholder="Username" required />
    <input type="email" name="email" placeholder="Email" />
    <input type="submit" name="submit" />
</form>"#;

    let result: ContactForm = html::from_str(input).expect("valid HTML");

    runner
        .scenario("Form Elements")
        .description("Parse HTML form elements with their attributes.")
        .target_type::<ContactForm>()
        .input(Language::Html, input)
        .success(&result)
        .finish();
}

// =============================================================================
// Serialization Scenarios
// =============================================================================

fn showcase_serialize_minified(runner: &mut ShowcaseRunner) {
    let div = DivElement {
        id: Some("main".to_string()),
        class: Some("container".to_string()),
        content: "Hello!".to_string(),
    };

    let output = html::to_string(&div).unwrap();

    runner
        .scenario("Minified Output")
        .description("Serialize to compact HTML without extra whitespace.")
        .target_type::<DivElement>()
        .serialized_output(Language::Html, &output)
        .finish();
}

fn showcase_serialize_pretty(runner: &mut ShowcaseRunner) {
    let form = ContactForm {
        action: Some("/api/contact".to_string()),
        method: Some("post".to_string()),
        inputs: vec![
            FormInput {
                input_type: Some("text".to_string()),
                name: Some("name".to_string()),
                placeholder: Some("Your name".to_string()),
                required: Some("required".to_string()),
            },
            FormInput {
                input_type: Some("email".to_string()),
                name: Some("email".to_string()),
                placeholder: Some("your@email.com".to_string()),
                required: None,
            },
        ],
    };

    let output = html::to_string_pretty(&form).unwrap();

    runner
        .scenario("Pretty-Printed Output")
        .description("Serialize with indentation for readability.")
        .target_type::<ContactForm>()
        .serialized_output(Language::Html, &output)
        .finish();
}

// =============================================================================
// Advanced Features
// =============================================================================

fn showcase_extra_attributes(runner: &mut ShowcaseRunner) {
    let input = r#"<div id="widget" class="card" data-user-id="123" data-theme="dark" aria-label="User Card">Content</div>"#;

    let result: DivWithExtras = html::from_str(input).expect("valid HTML");

    runner
        .scenario("Extra Attributes (data-*, aria-*)")
        .description("Unknown attributes like `data-*` and `aria-*` are captured in the `extra` field via `#[facet(flatten)]`.")
        .target_type::<DivWithExtras>()
        .input(Language::Html, input)
        .success(&result)
        .finish();
}
