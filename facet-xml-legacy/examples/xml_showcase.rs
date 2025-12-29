//! XML showcase demonstrating serialization, deserialization, and
//! the explicit-annotation error added in #1244.
//!
//! Run with: `cargo run -p facet-xml --example xml_showcase`

use facet::Facet;
use facet_showcase::{Language, ShowcaseRunner};
use facet_xml_legacy as xml;

#[derive(Facet, Debug)]
#[facet(xml::ns_all = "https://example.com/contacts")]
struct ContactBook {
    #[facet(xml::attribute)]
    owner: String,
    #[facet(xml::elements)]
    contacts: Vec<Contact>,
}

#[derive(Facet, Debug)]
struct Contact {
    #[facet(xml::attribute)]
    id: u32,
    #[facet(xml::element)]
    name: String,
    #[facet(xml::element)]
    email: Option<String>,
}

#[derive(Facet, Debug)]
struct AlertFeed {
    #[facet(xml::attribute)]
    severity: String,
    #[facet(xml::element)]
    title: String,
    #[facet(xml::elements)]
    messages: Vec<AlertMessage>,
}

#[derive(Facet, Debug)]
struct AlertMessage {
    #[facet(xml::attribute)]
    code: String,
    #[facet(xml::text)]
    body: String,
}

#[derive(Facet, Debug)]
struct MissingXmlAnnotations {
    // Intentionally missing `#[facet(xml::...)]`
    title: String,
    details: String,
}

fn main() {
    let mut runner = ShowcaseRunner::new("XML")
        .slug("xml")
        .language(Language::Xml);

    runner.header();
    runner.intro(
        "`facet-xml` maps Facet types to XML via explicit field annotations. \
         This showcase highlights common serialization patterns and the new \
         diagnostic you get when a field forgets to declare its XML role.",
    );

    runner.section("Serialization");
    scenario_contacts(&mut runner);
    scenario_alert_feed(&mut runner);

    runner.section("Diagnostics");
    scenario_missing_annotations(&mut runner);

    runner.footer();
}

fn scenario_contacts(runner: &mut ShowcaseRunner) {
    let book = ContactBook {
        owner: "Operations".to_string(),
        contacts: vec![
            Contact {
                id: 1,
                name: "Alice".into(),
                email: Some("alice@example.com".into()),
            },
            Contact {
                id: 2,
                name: "Bob".into(),
                email: None,
            },
        ],
    };

    let xml_output = xml::to_string_pretty(&book).expect("XML serialization succeeds");

    runner
        .scenario("Attributes, elements, and Vec fields")
        .description(
            "Attributes live on the root `<ContactBook>` tag while \
             `#[facet(xml::elements)]` turns a Vec into repeated `<contacts>` children.",
        )
        .target_type::<ContactBook>()
        .input_value(&book)
        .serialized_output(Language::Xml, &xml_output)
        .finish();
}

fn scenario_alert_feed(runner: &mut ShowcaseRunner) {
    let xml_input = r#"
<AlertFeed severity="warning">
  <title>System Notices</title>
  <messages code="OPS-201">Deploying new release at 02:00 UTC</messages>
  <messages code="DB-503">Database failover test scheduled</messages>
</AlertFeed>
"#
    .trim();

    let parsed: Result<AlertFeed, _> = xml::from_str(xml_input);

    runner
        .scenario("xml::text for content")
        .description(
            "`#[facet(xml::text)]` captures character data inside an element, \
             while attributes remain on the tag. This scenario deserializes \
             the feed and pretty-prints the resulting Facet value.",
        )
        .input(Language::Xml, xml_input)
        .target_type::<AlertFeed>()
        .result(&parsed)
        .finish();
}

fn scenario_missing_annotations(runner: &mut ShowcaseRunner) {
    let post = MissingXmlAnnotations {
        title: "Weekly Report".into(),
        details: "Compile-time errors per crate".into(),
    };

    let err = xml::to_string(&post).expect_err("fields without xml annotations now error");

    runner
        .scenario("Missing XML annotations")
        .description(
            "Every field must opt into XML via `#[facet(xml::attribute/element/...)]` \
             (or `#[facet(child)]`). Leaving a field unannotated now produces a \
             descriptive error before serialization begins.",
        )
        .target_type::<MissingXmlAnnotations>()
        .input_value(&post)
        .error(&err)
        .finish();
}
